$ErrorActionPreference = "Stop"

$Installer = Join-Path $PSScriptRoot "install.ps1"
$Root = Split-Path $PSScriptRoot -Parent
$Target = & $Installer -PrintTarget
if ($Target -ne "x86_64-pc-windows-msvc") {
  throw "installer target was '$Target'"
}

$tokens = $null
$errors = $null
[System.Management.Automation.Language.Parser]::ParseFile($Installer, [ref]$tokens, [ref]$errors) | Out-Null
if ($errors.Count -ne 0) {
  throw ($errors | Out-String)
}

$InvalidVersionFailed = $false
try {
  & $Installer -Version "not-a-version"
} catch {
  $InvalidVersionFailed = $_.Exception.Message -match "RAINY_INSTALL_INVALID_VERSION"
}
if (-not $InvalidVersionFailed) { throw "invalid installer version was accepted" }

$Binary = Join-Path $Root "target/debug/rainy.exe"
if (-not (Test-Path $Binary)) {
  throw "build rainy.exe before running the PowerShell installer test"
}

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("rainy-installer-test-" + [System.Guid]::NewGuid())
$ServerRoot = Join-Path $TempDir "server"
$InstallDir = Join-Path $TempDir "install"
$PortFile = Join-Path $TempDir "port"
$Server = $null

function Write-TestRelease {
  param([string]$Version)
  $ReleaseDir = Join-Path $ServerRoot $Version
  New-Item -ItemType Directory -Force -Path $ReleaseDir | Out-Null
  $Archive = Join-Path $ReleaseDir "rainy-x86_64-pc-windows-msvc.zip"
  Compress-Archive -Force -Path $Binary -DestinationPath $Archive
  $Hash = (Get-FileHash -Algorithm SHA256 $Archive).Hash.ToLowerInvariant()
  "$Hash  rainy-x86_64-pc-windows-msvc.zip" | Out-File -NoNewline -Encoding ascii "$Archive.sha256"
  return $ReleaseDir
}

function Assert-Version {
  param([string]$Expected)
  $Output = & (Join-Path $InstallDir "rainy.exe") --version
  if ($LASTEXITCODE -ne 0 -or $Output -notmatch (" " + [Regex]::Escape($Expected) + "$")) {
    throw "installed binary did not report $Expected"
  }
}

function Assert-InstallerFails {
  param([string]$Version, [string]$BaseUrl)
  $Failed = $false
  try {
    & $Installer -Version $Version -InstallDir $InstallDir -BaseUrl $BaseUrl
  } catch {
    $Failed = $true
  }
  if (-not $Failed) {
    throw "installer unexpectedly accepted invalid release $Version"
  }
}

New-Item -ItemType Directory -Force -Path $ServerRoot | Out-Null
try {
  $Release011 = Write-TestRelease -Version "v0.1.1"
  $Python = (Get-Command python -ErrorAction SilentlyContinue)
  if (-not $Python) { $Python = Get-Command python3 -ErrorAction Stop }
  $Server = Start-Process -FilePath $Python.Source -ArgumentList @(
    (Join-Path $Root "scripts/test-installer-server.py"),
    $ServerRoot,
    $PortFile
  ) -PassThru -WindowStyle Hidden
  for ($Attempt = 0; $Attempt -lt 100 -and -not (Test-Path $PortFile); $Attempt++) {
    Start-Sleep -Milliseconds 50
  }
  if (-not (Test-Path $PortFile)) { throw "installer test server did not start" }
  $ServerBase = "http://127.0.0.1:$(Get-Content $PortFile)"

  & $Installer -Version "v0.1.1" -InstallDir $InstallDir -BaseUrl "$ServerBase/v0.1.1"
  Assert-Version -Expected "0.1.1"

  Write-TestRelease -Version "v9.9.9" | Out-Null
  Assert-InstallerFails -Version "v9.9.9" -BaseUrl "$ServerBase/v9.9.9"
  Assert-Version -Expected "0.1.1"

  $Archive = Join-Path $Release011 "rainy-x86_64-pc-windows-msvc.zip"
  ("0" * 64 + "  rainy-x86_64-pc-windows-msvc.zip") | Out-File -NoNewline -Encoding ascii "$Archive.sha256"
  Assert-InstallerFails -Version "v0.1.1" -BaseUrl "$ServerBase/v0.1.1"
  Assert-Version -Expected "0.1.1"

  Remove-Item "$Archive.sha256"
  Assert-InstallerFails -Version "v0.1.1" -BaseUrl "$ServerBase/v0.1.1"
  Assert-Version -Expected "0.1.1"

  Remove-Item $Archive
  Assert-InstallerFails -Version "v0.1.1" -BaseUrl "$ServerBase/v0.1.1"
  Assert-Version -Expected "0.1.1"
} finally {
  if ($Server -and -not $Server.HasExited) { Stop-Process -Id $Server.Id -Force }
  Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}

Write-Output "PowerShell installer tests passed"
