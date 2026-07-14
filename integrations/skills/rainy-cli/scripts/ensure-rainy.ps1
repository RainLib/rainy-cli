param(
  [string]$Repo = $(if ($env:RAINY_REPO) { $env:RAINY_REPO } else { "RainLib/rainy-cli" }),
  [string]$InstallDir = $(if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { Join-Path $HOME ".rainy\bin" }),
  [string]$ReleaseUrl = $(if ($env:RAINY_SKILL_RELEASE_URL) { $env:RAINY_SKILL_RELEASE_URL } else { "" }),
  [switch]$ForceInstall
)

$ErrorActionPreference = "Stop"
if (-not $ReleaseUrl) {
  $ReleaseUrl = "https://github.com/$Repo/releases/latest/download"
}

function Resolve-RainyCommand {
  param([string]$Candidate)
  if (-not $Candidate) { return $null }
  if (Test-Path -LiteralPath $Candidate -PathType Leaf) {
    $Resolved = (Resolve-Path -LiteralPath $Candidate).Path
  } else {
    $Command = Get-Command $Candidate -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $Command) { return $null }
    $Resolved = $Command.Source
  }
  $Version = & $Resolved --version
  if ($LASTEXITCODE -ne 0 -or $Version -notmatch '^rainy [0-9]+\.[0-9]+\.[0-9]+$') {
    return $null
  }
  Write-Host $Version
  return $Resolved
}

function Invoke-RainyDownload {
  param([string]$Uri, [string]$OutFile)
  for ($Attempt = 1; $Attempt -le 4; $Attempt++) {
    try {
      Invoke-WebRequest -UseBasicParsing -TimeoutSec 90 $Uri -OutFile $OutFile
      return
    } catch {
      if ($Attempt -eq 4) { throw }
      Start-Sleep -Seconds (2 * $Attempt)
    }
  }
}

if (-not $ForceInstall -and $env:RAINY_SKILL_FORCE_INSTALL -ne "1") {
  $Resolved = Resolve-RainyCommand -Candidate $env:RAINY_BIN
  if (-not $Resolved) { $Resolved = Resolve-RainyCommand -Candidate "rainy" }
  if (-not $Resolved) { $Resolved = Resolve-RainyCommand -Candidate (Join-Path $InstallDir "rainy.exe") }
  if ($Resolved) {
    Write-Output $Resolved
    exit 0
  }
}

if ($Repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
  throw "rainy skill bootstrap: invalid repository; expected owner/repo, got $Repo"
}
if ($ReleaseUrl -notmatch '^https://' -and $ReleaseUrl -notmatch '^http://(127\.0\.0\.1|localhost)(:\d+)?(/|$)') {
  throw "rainy skill bootstrap: release URL must use HTTPS or loopback HTTP: $ReleaseUrl"
}

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("rainy-skill-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $TempDir | Out-Null
try {
  $Installer = Join-Path $TempDir "install.ps1"
  $Checksums = Join-Path $TempDir "installers.sha256"
  Write-Host "rainy command not found; installing the verified latest release"
  Invoke-RainyDownload -Uri "$ReleaseUrl/install.ps1" -OutFile $Installer
  Invoke-RainyDownload -Uri "$ReleaseUrl/installers.sha256" -OutFile $Checksums

  $Line = Get-Content $Checksums | Where-Object { $_ -match '\s+install\.ps1$' } | Select-Object -First 1
  if (-not $Line) { throw "rainy skill bootstrap: installers.sha256 has no install.ps1 digest" }
  $Expected = ($Line -split '\s+', 2)[0].ToLowerInvariant()
  if ($Expected -notmatch '^[a-f0-9]{64}$') {
    throw "rainy skill bootstrap: installers.sha256 has an invalid install.ps1 digest"
  }
  $Actual = (Get-FileHash -Algorithm SHA256 $Installer).Hash.ToLowerInvariant()
  if ($Actual -ne $Expected) { throw "rainy skill bootstrap: install.ps1 checksum verification failed" }

  & $Installer -Repo $Repo -Version "latest" -InstallDir $InstallDir
  $Resolved = Resolve-RainyCommand -Candidate (Join-Path $InstallDir "rainy.exe")
  if (-not $Resolved) {
    throw "rainy skill bootstrap: Rainy CLI was installed but its executable could not be verified"
  }
  Write-Output $Resolved
} finally {
  Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
