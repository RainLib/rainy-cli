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

$Binary = if ($env:RAINY_TEST_BINARY) { $env:RAINY_TEST_BINARY } else { Join-Path $Root "target/debug/rainy.exe" }
if (-not (Test-Path $Binary)) {
  throw "build rainy.exe before running the PowerShell installer test"
}
$Binary = (Resolve-Path $Binary).Path
$VersionOutput = & $Binary --version
if ($LASTEXITCODE -ne 0 -or $VersionOutput -notmatch '^rainy (?<Version>[0-9]+\.[0-9]+\.[0-9]+)$') {
  throw "PowerShell installer test binary did not report a valid version"
}
$CurrentVersion = $Matches.Version
$CurrentTag = "v$CurrentVersion"
$OriginalUserPath = [Environment]::GetEnvironmentVariable("Path", "User")
$OriginalProcessPath = $env:Path
$OriginalRainyHome = $env:RAINY_HOME

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("rainy-installer-test-" + [System.Guid]::NewGuid())
$ServerRoot = Join-Path $TempDir "server"
$InstallDir = Join-Path $TempDir "install"
$PortFile = Join-Path $TempDir "port"
$ServerOutput = Join-Path $TempDir "server.stdout.log"
$ServerError = Join-Path $TempDir "server.stderr.log"
$RainyHome = Join-Path $TempDir "rainy-home"
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

function Get-PathEntryCount {
  param([AllowNull()][string]$PathValue, [string]$Directory)
  if (-not $PathValue) { return 0 }
  $Expected = $Directory.TrimEnd([char[]]"\/")
  return @($PathValue -split ";" | Where-Object {
    $_ -and $_.Trim().TrimEnd([char[]]"\/") -ieq $Expected
  }).Count
}

New-Item -ItemType Directory -Force -Path $ServerRoot | Out-Null
try {
  $env:RAINY_HOME = $RainyHome
  $ReleaseCurrent = Write-TestRelease -Version $CurrentTag
  $CurrentTag | Out-File -NoNewline -Encoding ascii (Join-Path $ServerRoot "latest.txt")
  $Python = (Get-Command python -ErrorAction SilentlyContinue)
  if (-not $Python) { $Python = Get-Command python3 -ErrorAction Stop }
  $ServerArguments = @(
    "-u",
    ('"{0}"' -f (Join-Path $Root "scripts/test-installer-server.py")),
    ('"{0}"' -f $ServerRoot),
    ('"{0}"' -f $PortFile),
    "2"
  )
  $Server = Start-Process -FilePath $Python.Source -ArgumentList $ServerArguments `
    -RedirectStandardOutput $ServerOutput -RedirectStandardError $ServerError `
    -PassThru -WindowStyle Hidden
  $ServerPort = $null
  for ($Attempt = 0; $Attempt -lt 300; $Attempt++) {
    if (Test-Path $PortFile) {
      $PortText = Get-Content $PortFile -Raw
      if ($PortText) {
        $CandidatePort = $PortText.Trim()
        if ($CandidatePort -match '^[0-9]+$') {
          $ServerPort = $CandidatePort
          break
        }
      }
    }
    if ($Server.HasExited) { break }
    Start-Sleep -Milliseconds 100
  }
  if (-not $ServerPort) {
    $Diagnostics = @()
    if ($Server.HasExited) { $Diagnostics += "exit=$($Server.ExitCode)" }
    if (Test-Path $ServerError) {
      $ErrorText = Get-Content $ServerError -Raw
      if ($ErrorText) { $Diagnostics += $ErrorText.Trim() }
    }
    if (Test-Path $ServerOutput) {
      $OutputText = Get-Content $ServerOutput -Raw
      if ($OutputText) { $Diagnostics += $OutputText.Trim() }
    }
    $Detail = ($Diagnostics | Where-Object { $_ }) -join "; "
    if (-not $Detail) { $Detail = "no child-process diagnostics" }
    throw "installer test server did not start within 30 seconds: $Detail"
  }
  $ServerBase = "http://127.0.0.1:$ServerPort"

  & $Installer -InstallDir $InstallDir -ReleaseBaseUrl $ServerBase
  Assert-Version -Expected $CurrentVersion
  if ((Get-Content (Join-Path $RainyHome "release-source") -Raw) -ne $ServerBase) {
    throw "installer did not persist the release mirror"
  }
  if ((Get-PathEntryCount -PathValue $env:Path -Directory $InstallDir) -ne 1) {
    throw "installer did not add the install directory to the current process PATH"
  }
  $UpdatedUserPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if ((Get-PathEntryCount -PathValue $UpdatedUserPath -Directory $InstallDir) -ne 1) {
    throw "installer did not add the install directory to the user PATH"
  }

  & $Installer -Version $CurrentTag -InstallDir $InstallDir -BaseUrl "$ServerBase/$CurrentTag"
  $UpdatedUserPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if ((Get-PathEntryCount -PathValue $UpdatedUserPath -Directory $InstallDir) -ne 1) {
    throw "installer added duplicate user PATH entries"
  }

  Write-TestRelease -Version "v9.9.9" | Out-Null
  Assert-InstallerFails -Version "v9.9.9" -BaseUrl "$ServerBase/v9.9.9"
  Assert-Version -Expected $CurrentVersion

  $Archive = Join-Path $ReleaseCurrent "rainy-x86_64-pc-windows-msvc.zip"
  ("0" * 64 + "  rainy-x86_64-pc-windows-msvc.zip") | Out-File -NoNewline -Encoding ascii "$Archive.sha256"
  Assert-InstallerFails -Version $CurrentTag -BaseUrl "$ServerBase/$CurrentTag"
  Assert-Version -Expected $CurrentVersion

  Remove-Item "$Archive.sha256"
  Assert-InstallerFails -Version $CurrentTag -BaseUrl "$ServerBase/$CurrentTag"
  Assert-Version -Expected $CurrentVersion

  Remove-Item $Archive
  Assert-InstallerFails -Version $CurrentTag -BaseUrl "$ServerBase/$CurrentTag"
  Assert-Version -Expected $CurrentVersion
} finally {
  if ($Server -and -not $Server.HasExited) { Stop-Process -Id $Server.Id -Force }
  [Environment]::SetEnvironmentVariable("Path", $OriginalUserPath, "User")
  $env:Path = $OriginalProcessPath
  if ($null -eq $OriginalRainyHome) {
    Remove-Item Env:RAINY_HOME -ErrorAction SilentlyContinue
  } else {
    $env:RAINY_HOME = $OriginalRainyHome
  }
  Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}

Write-Output "PowerShell installer tests passed"
