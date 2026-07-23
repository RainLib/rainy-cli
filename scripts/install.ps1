param(
  [string]$Repo = $(if ($env:RAINY_REPO) { $env:RAINY_REPO } else { "RainLib/rainy-cli" }),
  [string]$Version = $(if ($env:RAINY_VERSION) { $env:RAINY_VERSION } else { "latest" }),
  [string]$InstallDir = $(if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { Join-Path $HOME ".rainy\bin" }),
  [string]$BaseUrl = $(if ($env:RAINY_INSTALLER_BASE_URL) { $env:RAINY_INSTALLER_BASE_URL } else { "" }),
  [string]$ReleaseBaseUrl = $(if ($env:RAINY_RELEASE_BASE_URL) { $env:RAINY_RELEASE_BASE_URL } else { "" }),
  [string]$LatestVersionUrl = $(if ($env:RAINY_LATEST_VERSION_URL) { $env:RAINY_LATEST_VERSION_URL } else { "" }),
  [switch]$AddToPath,
  [switch]$NoModifyPath,
  [switch]$PrintTarget
)

$ErrorActionPreference = "Stop"

if ($AddToPath -and ($NoModifyPath -or $env:RAINY_NO_MODIFY_PATH -eq "1")) {
  throw "rainy installer: -AddToPath and -NoModifyPath cannot be used together"
}
$SkipPathUpdate = $NoModifyPath -or $env:RAINY_NO_MODIFY_PATH -eq "1"

function Test-RainyPathContains {
  param([AllowNull()][string]$PathValue, [string]$Directory)
  if (-not $PathValue) { return $false }
  $Expected = $Directory.TrimEnd([char[]]"\/")
  return [bool]($PathValue -split ";" | Where-Object {
    $_ -and $_.Trim().TrimEnd([char[]]"\/") -ieq $Expected
  } | Select-Object -First 1)
}

function Send-RainyEnvironmentChanged {
  if (-not ("Rainy.NativeMethods" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace Rainy {
  public static class NativeMethods {
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr SendMessageTimeout(
      IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam,
      uint flags, uint timeout, out UIntPtr result);
  }
}
"@
  }
  $Result = [UIntPtr]::Zero
  [Rainy.NativeMethods]::SendMessageTimeout(
    [IntPtr]0xffff, 0x001A, [UIntPtr]::Zero, "Environment", 0x0002, 5000, [ref]$Result
  ) | Out-Null
}

function Add-RainyToPath {
  param([string]$Directory)
  $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if (-not (Test-RainyPathContains -PathValue $UserPath -Directory $Directory)) {
    $UpdatedUserPath = if ($UserPath) { "$Directory;$UserPath" } else { $Directory }
    [Environment]::SetEnvironmentVariable("Path", $UpdatedUserPath, "User")
    try {
      Send-RainyEnvironmentChanged
    } catch {
      Write-Warning "User PATH was updated, but the environment-change broadcast failed: $_"
    }
    Write-Host "Added $Directory to the user PATH."
  } else {
    Write-Host "$Directory is already present in the user PATH."
  }

  if (-not (Test-RainyPathContains -PathValue $env:Path -Directory $Directory)) {
    $env:Path = "$Directory;$env:Path"
  }
}

function Save-RainyReleaseSource {
  param([string]$Url)
  if (-not $Url) { return }
  $RainyHome = if ($env:RAINY_HOME) { $env:RAINY_HOME } else { Join-Path $HOME ".rainy" }
  New-Item -ItemType Directory -Force -Path $RainyHome | Out-Null
  $SourceFile = Join-Path $RainyHome "release-source"
  $Temporary = "$SourceFile.tmp.$PID"
  $Url.TrimEnd('/') | Out-File -NoNewline -Encoding ascii $Temporary
  Move-Item -Force $Temporary $SourceFile
  Write-Host "Saved Rainy release mirror to $SourceFile."
}

function Invoke-RainyDownload {
  param([string]$Uri, [string]$OutFile, [int]$TimeoutSec)
  for ($Attempt = 1; $Attempt -le 4; $Attempt++) {
    try {
      Invoke-WebRequest -UseBasicParsing -TimeoutSec $TimeoutSec $Uri -OutFile $OutFile
      return
    } catch {
      if ($Attempt -eq 4) { throw }
      Start-Sleep -Seconds (2 * $Attempt)
    }
  }
}

function Assert-RainyDownloadUrl {
  param([string]$Uri)
  if ($Uri -notmatch '^https://' -and $Uri -notmatch '^http://(127\.0\.0\.1|localhost)(:\d+)?(/|$)') {
    throw "rainy installer: download URL must use HTTPS or loopback HTTP: $Uri"
  }
}

function Get-RainyText {
  param([string]$Uri)
  for ($Attempt = 1; $Attempt -le 4; $Attempt++) {
    try {
      return (Invoke-WebRequest -UseBasicParsing -TimeoutSec 90 -Headers @{ "User-Agent" = "rainy-installer" } -Uri $Uri).Content
    } catch {
      if ($Attempt -eq 4) { throw }
      Start-Sleep -Seconds (2 * $Attempt)
    }
  }
}

function Get-RainyLatestRelease {
  param([string]$Repo)
  for ($Attempt = 1; $Attempt -le 4; $Attempt++) {
    try {
      return Invoke-RestMethod -TimeoutSec 90 -Headers @{ "User-Agent" = "rainy-installer" } -Uri "https://api.github.com/repos/$Repo/releases/latest"
    } catch {
      if ($Attempt -eq 4) { throw }
      Start-Sleep -Seconds (2 * $Attempt)
    }
  }
}

if ($Repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
  throw "RAINY_INSTALL_INVALID_REPOSITORY: expected owner/repo, got $Repo"
}

function Resolve-RainyVersion {
  param(
    [string]$Repo,
    [string]$Version,
    [string]$ReleaseBaseUrl,
    [string]$LatestVersionUrl
  )
  if ($Version -eq "latest") {
    $VersionUrl = $LatestVersionUrl
    if (-not $VersionUrl -and $ReleaseBaseUrl) {
      $VersionUrl = "$($ReleaseBaseUrl.TrimEnd('/'))/latest.txt"
    }
    if ($VersionUrl) {
      Assert-RainyDownloadUrl -Uri $VersionUrl
      return (Get-RainyText -Uri $VersionUrl).Trim()
    }
    $release = Get-RainyLatestRelease -Repo $Repo
    return $release.tag_name
  }
  if ($Version.StartsWith("v")) {
    return $Version
  }
  return "v$Version"
}

if (-not [Environment]::Is64BitOperatingSystem) {
  throw "rainy installer: only 64-bit Windows is supported"
}

$Target = "x86_64-pc-windows-msvc"
if ($PrintTarget) {
  Write-Output $Target
  exit 0
}
$Asset = "rainy-$Target.zip"
$ResolvedVersion = Resolve-RainyVersion -Repo $Repo -Version $Version `
  -ReleaseBaseUrl $ReleaseBaseUrl -LatestVersionUrl $LatestVersionUrl
if ($ResolvedVersion -notmatch '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
  throw "RAINY_INSTALL_INVALID_VERSION: expected vX.Y.Z, got $ResolvedVersion"
}
if (-not $BaseUrl) {
  if ($ReleaseBaseUrl) {
    $BaseUrl = "$($ReleaseBaseUrl.TrimEnd('/'))/$ResolvedVersion"
  } else {
    $BaseUrl = "https://github.com/$Repo/releases/download/$ResolvedVersion"
  }
}
Assert-RainyDownloadUrl -Uri $BaseUrl
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("rainy-install-" + [System.Guid]::NewGuid())

New-Item -ItemType Directory -Path $TempDir | Out-Null
try {
  $Archive = Join-Path $TempDir $Asset
  $Checksum = "$Archive.sha256"
  Write-Host "Installing rainy $ResolvedVersion for $Target"
  Invoke-RainyDownload -Uri "$BaseUrl/$Asset" -OutFile $Archive -TimeoutSec 900

  Invoke-RainyDownload -Uri "$BaseUrl/$Asset.sha256" -OutFile $Checksum -TimeoutSec 90
  $Expected = (Get-Content $Checksum | Select-Object -First 1).Split(" ", [System.StringSplitOptions]::RemoveEmptyEntries)[0].ToLowerInvariant()
  if ($Expected -notmatch '^[a-f0-9]{64}$') {
    throw "rainy installer: invalid checksum format"
  }
  $Actual = (Get-FileHash -Algorithm SHA256 $Archive).Hash.ToLowerInvariant()
  if ($Expected -ne $Actual) {
    throw "checksum mismatch: expected $Expected, actual $Actual"
  }

  $ExtractDir = Join-Path $TempDir "extract"
  Expand-Archive -Path $Archive -DestinationPath $ExtractDir
  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
  $Destination = Join-Path $InstallDir "rainy.exe"
  $NewBinary = Join-Path $InstallDir (".rainy.new." + [System.Guid]::NewGuid() + ".exe")
  $Backup = Join-Path $InstallDir (".rainy.backup." + [System.Guid]::NewGuid() + ".exe")
  Copy-Item (Join-Path $ExtractDir "rainy.exe") $NewBinary
  $ReportedVersion = & $NewBinary --version
  if ($LASTEXITCODE -ne 0 -or $ReportedVersion -notmatch (" " + [Regex]::Escape($ResolvedVersion.TrimStart("v")) + "$")) {
    throw "rainy installer: downloaded binary version does not match $ResolvedVersion"
  }
  if (Test-Path $Destination) { Move-Item $Destination $Backup }
  try {
    Move-Item $NewBinary $Destination
    $InstalledVersion = & $Destination --version
    if ($LASTEXITCODE -ne 0 -or $InstalledVersion -notmatch (" " + [Regex]::Escape($ResolvedVersion.TrimStart("v")) + "$")) {
      throw "rainy installer: installation verification failed"
    }
    Remove-Item $Backup -Force -ErrorAction SilentlyContinue
  } catch {
    Remove-Item $Destination -Force -ErrorAction SilentlyContinue
    if (Test-Path $Backup) { Move-Item $Backup $Destination }
    throw "$_; previous binary restored"
  }

  Save-RainyReleaseSource -Url $ReleaseBaseUrl
  if (-not $SkipPathUpdate) {
    Add-RainyToPath -Directory $InstallDir
  } else {
    Write-Host "PATH was not modified because path modification was disabled."
  }

  Write-Host "rainy installed to $(Join-Path $InstallDir 'rainy.exe')"
  if (-not (Test-RainyPathContains -PathValue $env:Path -Directory $InstallDir)) {
    Write-Host "Add this directory to PATH if needed: $InstallDir"
  }
} finally {
  Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
