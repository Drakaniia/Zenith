# Launches an isolated Zenith test instance: its own profile folder (ZENITH_PROFILE_DIR), its
# own remote-debugging port, no single-instance redirect. It never touches the normal profile
# or any other running Zenith window; only the instance this script started is stopped.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev/run.ps1 [-Fresh] [-SkipSetup] [-Release] [-Url <url>]
#
# A fresh profile opens the first-run setup assistant. -SkipSetup marks setup as done (for the
# e2e tests, which drive the browser itself).
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev/run.ps1 -Stop
param(
  [switch]$Release,
  [int]$Port = 9444,
  [string]$Url = '',
  [switch]$Fresh,
  [switch]$SkipSetup,
  [switch]$Stop,
  [string]$ProfileDir = (Join-Path $env:TEMP 'zenith-test-profile')
)
$ErrorActionPreference = 'Continue'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$profileName = if ($Release) { 'release' } else { 'debug' }
$exe = Join-Path $root "src-tauri\target\$profileName\zenith.exe"
$logDir = Join-Path $env:TEMP 'zenith-dev'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$pidFile = Join-Path $logDir 'test-instance.pid'

function Stop-TestInstance {
  if (Test-Path $pidFile) {
    $old = [int](Get-Content $pidFile -ErrorAction SilentlyContinue)
    Get-Process -Id $old -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -eq 'zenith' } | Stop-Process -Force
    Remove-Item $pidFile -ErrorAction SilentlyContinue
  }
  # The WebView2 processes of the test profile can outlive zenith.exe for a moment.
  for ($i = 0; $i -lt 25; $i++) {
    $left = Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" -ErrorAction SilentlyContinue |
      Where-Object { $_.CommandLine -and $_.CommandLine.Contains($ProfileDir) }
    if (-not $left) { return }
    if ($i -ge 10) { $left | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue } }
    Start-Sleep -Milliseconds 200
  }
}

Stop-TestInstance
if ($Stop) { Write-Output 'test instance stopped'; return }
if (-not (Test-Path $exe)) { throw "missing $exe (build first)" }

if ($Fresh) {
  # WebView2 releases its files a moment after its processes exit; retry for a few seconds.
  for ($i = 0; $i -lt 20 -and (Test-Path $ProfileDir); $i++) {
    Remove-Item -Recurse -Force $ProfileDir -ErrorAction SilentlyContinue
    if (Test-Path $ProfileDir) { Start-Sleep -Milliseconds 500 }
  }
  if (Test-Path $ProfileDir) { throw "could not clear $ProfileDir" }
}
New-Item -ItemType Directory -Force -Path $ProfileDir | Out-Null
if ($SkipSetup) {
  $roaming = Join-Path $ProfileDir 'roaming'
  $settingsFile = Join-Path $roaming 'settings.json'
  if (-not (Test-Path $settingsFile)) {
    New-Item -ItemType Directory -Force -Path $roaming | Out-Null
    [IO.File]::WriteAllText($settingsFile, '{"onboarded":true}')
  }
}

$env:ZENITH_PROFILE_DIR = $ProfileDir
$env:ZENITH_REMOTE_DEBUG_PORT = "$Port"
$env:ZENITH_PERF = '1'
$startArgs = @{
  FilePath = $exe
  PassThru = $true
  RedirectStandardOutput = (Join-Path $logDir 'stdout.log')
  RedirectStandardError = (Join-Path $logDir 'stderr.log')
}
if ($Url) { $startArgs.ArgumentList = $Url }
$p = Start-Process @startArgs
$p.Id | Out-File -Encoding ascii $pidFile
Write-Output "started test instance pid=$($p.Id) port=$Port profile=$ProfileDir logs=$logDir"
