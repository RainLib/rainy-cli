param(
  [string]$Repo = $(if ($env:RAINY_REPO) { $env:RAINY_REPO } else { "RainLib/rainy-cli" }),
  [string]$Version = $(if ($env:RAINY_VERSION) { $env:RAINY_VERSION } else { "latest" }),
  [string]$InstallDir = $(if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { Join-Path $HOME ".rainy\bin" }),
  [string]$BaseUrl = $(if ($env:RAINY_INSTALLER_BASE_URL) { $env:RAINY_INSTALLER_BASE_URL } else { "" }),
  [switch]$AddToPath,
  [switch]$PrintTarget
)

$ErrorActionPreference = "Stop"

if ($Repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
  throw "RAINY_INSTALL_INVALID_REPOSITORY: expected owner/repo, got $Repo"
}

function Resolve-RainyVersion {
  param([string]$Repo, [string]$Version)
  if ($Version -eq "latest") {
    $release = Invoke-RestMethod -TimeoutSec 15 -Headers @{ "User-Agent" = "rainy-installer" } -Uri "https://api.github.com/repos/$Repo/releases/latest"
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
$ResolvedVersion = Resolve-RainyVersion -Repo $Repo -Version $Version
if ($ResolvedVersion -notmatch '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
  throw "RAINY_INSTALL_INVALID_VERSION: expected vX.Y.Z, got $ResolvedVersion"
}
if (-not $BaseUrl) {
  $BaseUrl = "https://github.com/$Repo/releases/download/$ResolvedVersion"
}
if ($BaseUrl -notmatch '^https://' -and $BaseUrl -notmatch '^http://(127\.0\.0\.1|localhost)(:\d+)?(/|$)') {
  throw "rainy installer: release URL must use HTTPS or loopback HTTP: $BaseUrl"
}
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("rainy-install-" + [System.Guid]::NewGuid())

New-Item -ItemType Directory -Path $TempDir | Out-Null
try {
  $Archive = Join-Path $TempDir $Asset
  $Checksum = "$Archive.sha256"
  Write-Host "Installing rainy $ResolvedVersion for $Target"
  Invoke-WebRequest -UseBasicParsing -TimeoutSec 600 "$BaseUrl/$Asset" -OutFile $Archive

  Invoke-WebRequest -UseBasicParsing -TimeoutSec 30 "$BaseUrl/$Asset.sha256" -OutFile $Checksum
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

  if ($AddToPath) {
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not (($UserPath -split ";") -contains $InstallDir)) {
      [Environment]::SetEnvironmentVariable("Path", "$InstallDir;$UserPath", "User")
      Write-Host "Added $InstallDir to the user PATH. Restart your shell to pick it up."
    }
  }

  Write-Host "rainy installed to $(Join-Path $InstallDir 'rainy.exe')"
  if (-not (($env:Path -split ";") -contains $InstallDir)) {
    Write-Host "Add this directory to PATH if needed: $InstallDir"
  }
} finally {
  Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
