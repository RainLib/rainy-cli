$ErrorActionPreference = "Stop"

$Root = Split-Path $PSScriptRoot -Parent
$Bootstrap = Join-Path $Root "integrations/skills/rainy-cli/scripts/ensure-rainy.ps1"
$CometSkill = Join-Path $Root "integrations/skills/rainy-comet/SKILL.md"
$Binary = if ($env:RAINY_TEST_BINARY) { $env:RAINY_TEST_BINARY } else { Join-Path $Root "target/debug/rainy.exe" }
if (-not (Test-Path $Binary)) {
  throw "build rainy.exe before running the PowerShell skill test"
}
$Binary = (Resolve-Path $Binary).Path
$ExpectedVersion = & $Binary --version
if ($LASTEXITCODE -ne 0 -or $ExpectedVersion -notmatch '^rainy [0-9]+\.[0-9]+\.[0-9]+$') {
  throw "PowerShell skill test binary did not report a valid version"
}
if (-not (Test-Path $CometSkill)) {
  throw "Rainy Comet Skill is missing"
}
if ((Get-Content -Raw $CometSkill) -match "TODO") {
  throw "Rainy Comet Skill contains unfinished TODO markers"
}

$tokens = $null
$errors = $null
[System.Management.Automation.Language.Parser]::ParseFile($Bootstrap, [ref]$tokens, [ref]$errors) | Out-Null
if ($errors.Count -ne 0) { throw ($errors | Out-String) }

$PreviousRainyBin = $env:RAINY_BIN
$PreviousTestBinary = $env:RAINY_SKILL_TEST_BINARY
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("rainy-skill-test-" + [System.Guid]::NewGuid())
$ServerRoot = Join-Path $TempDir "server"
$ReleaseDir = Join-Path $ServerRoot "release"
$InstallDir = Join-Path $TempDir "install"
$PortFile = Join-Path $TempDir "port"
$Server = $null

New-Item -ItemType Directory -Force -Path $ReleaseDir | Out-Null
try {
  $env:RAINY_BIN = $Binary
  $Resolved = & $Bootstrap
  if ((Resolve-Path $Resolved).Path -ne (Resolve-Path $Binary).Path) {
    throw "PowerShell bootstrap did not reuse RAINY_BIN"
  }

  $FakeInstaller = @'
param(
  [string]$Repo,
  [string]$Version,
  [string]$InstallDir
)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -LiteralPath $env:RAINY_SKILL_TEST_BINARY -Destination (Join-Path $InstallDir "rainy.exe")
'@
  $Installer = Join-Path $ReleaseDir "install.ps1"
  $FakeInstaller | Out-File -Encoding utf8 $Installer
  $Hash = (Get-FileHash -Algorithm SHA256 $Installer).Hash.ToLowerInvariant()
  "$Hash  install.ps1" | Out-File -NoNewline -Encoding ascii (Join-Path $ReleaseDir "installers.sha256")

  $Python = Get-Command python -ErrorAction SilentlyContinue
  if (-not $Python) { $Python = Get-Command python3 -ErrorAction Stop }
  $Server = Start-Process -FilePath $Python.Source -ArgumentList @(
    (Join-Path $Root "scripts/test-installer-server.py"),
    $ServerRoot,
    $PortFile,
    "2"
  ) -PassThru -WindowStyle Hidden
  for ($Attempt = 0; $Attempt -lt 100 -and -not (Test-Path $PortFile); $Attempt++) {
    Start-Sleep -Milliseconds 50
  }
  if (-not (Test-Path $PortFile)) { throw "skill test server did not start" }

  $env:RAINY_BIN = $null
  $env:RAINY_SKILL_TEST_BINARY = $Binary
  $ReleaseUrl = "http://127.0.0.1:$(Get-Content $PortFile)/release"
  $Resolved = & $Bootstrap -InstallDir $InstallDir -ReleaseUrl $ReleaseUrl -ForceInstall
  if ((Resolve-Path $Resolved).Path -ne (Resolve-Path (Join-Path $InstallDir "rainy.exe")).Path) {
    throw "PowerShell bootstrap did not return the installed binary"
  }
  $Version = & $Resolved --version
  if ($LASTEXITCODE -ne 0 -or $Version -ne $ExpectedVersion) {
    throw "PowerShell bootstrap installed an unusable binary"
  }

  (("0" * 64) + "  install.ps1") | Out-File -NoNewline -Encoding ascii (Join-Path $ReleaseDir "installers.sha256")
  $ChecksumFailed = $false
  try {
    & $Bootstrap -InstallDir (Join-Path $TempDir "rejected") -ReleaseUrl $ReleaseUrl -ForceInstall
  } catch {
    $ChecksumFailed = $_.Exception.Message -match "checksum verification failed"
  }
  if (-not $ChecksumFailed) { throw "PowerShell bootstrap accepted an invalid installer checksum" }
} finally {
  $env:RAINY_BIN = $PreviousRainyBin
  $env:RAINY_SKILL_TEST_BINARY = $PreviousTestBinary
  if ($Server -and -not $Server.HasExited) { Stop-Process -Id $Server.Id -Force }
  Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}

Write-Output "PowerShell skill tests passed"
