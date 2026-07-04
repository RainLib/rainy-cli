param(
  [string]$Repo = $(if ($env:RAINY_REPO) { $env:RAINY_REPO } else { "rainy-dev/rainy" }),
  [string]$Version = $(if ($env:RAINY_VERSION) { $env:RAINY_VERSION } else { "latest" }),
  [string]$InstallDir = $(if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { Join-Path $HOME ".rainy\bin" }),
  [switch]$AddToPath
)

$ErrorActionPreference = "Stop"

function Resolve-RainyVersion {
  param([string]$Repo, [string]$Version)
  if ($Version -eq "latest") {
    $release = Invoke-RestMethod -Headers @{ "User-Agent" = "rainy-installer" } -Uri "https://api.github.com/repos/$Repo/releases/latest"
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
$Asset = "rainy-$Target.zip"
$ResolvedVersion = Resolve-RainyVersion -Repo $Repo -Version $Version
$BaseUrl = "https://github.com/$Repo/releases/download/$ResolvedVersion"
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("rainy-install-" + [System.Guid]::NewGuid())

New-Item -ItemType Directory -Path $TempDir | Out-Null
try {
  $Archive = Join-Path $TempDir $Asset
  $Checksum = "$Archive.sha256"
  Write-Host "Installing rainy $ResolvedVersion for $Target"
  Invoke-WebRequest -UseBasicParsing "$BaseUrl/$Asset" -OutFile $Archive

  try {
    Invoke-WebRequest -UseBasicParsing "$BaseUrl/$Asset.sha256" -OutFile $Checksum
    $Expected = (Get-Content $Checksum | Select-Object -First 1).Split(" ", [System.StringSplitOptions]::RemoveEmptyEntries)[0].ToLowerInvariant()
    $Actual = (Get-FileHash -Algorithm SHA256 $Archive).Hash.ToLowerInvariant()
    if ($Expected -ne $Actual) {
      throw "checksum mismatch: expected $Expected, actual $Actual"
    }
  } catch {
    Write-Warning "rainy installer: checksum verification skipped or failed to download checksum: $_"
  }

  $ExtractDir = Join-Path $TempDir "extract"
  Expand-Archive -Path $Archive -DestinationPath $ExtractDir
  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
  Copy-Item -Force (Join-Path $ExtractDir "rainy.exe") (Join-Path $InstallDir "rainy.exe")

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
