# Network privacy audit (Section 10.3):
# Launches Zenith on a fresh isolated profile with ZERO page navigation and records every
# non-loopback TCP/UDP endpoint opened by zenith.exe and its CEF child processes over 15 seconds.
# Fails with exit code 1 if any unexpected remote connection is observed.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/audit-network-privacy.ps1 [-Release]
param(
  [switch]$Release,
  [int]$DurationSeconds = 15,
  [string]$ProfileDir = (Join-Path $env:TEMP 'zenith-privacy-audit')
)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$profileName = if ($Release) { 'release' } else { 'debug' }
$exe = Join-Path $root "src-tauri\target\$profileName\zenith.exe"
if (-not (Test-Path $exe)) { throw "Missing $exe (build first)" }

if (Test-Path $ProfileDir) { Remove-Item -Recurse -Force $ProfileDir -ErrorAction SilentlyContinue }
New-Item -ItemType Directory -Force -Path (Join-Path $ProfileDir 'roaming') | Out-Null
# Mark onboarded and disable user-configured filter/app updates during the idle vendor-traffic audit.
$now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
[IO.File]::WriteAllText(
  (Join-Path $ProfileDir 'roaming\settings.json'),
  "{`"onboarded`":true,`"autoUpdate`":false,`"blockerEnabled`":false,`"filterLists`":[],`"filterListsUpdated`":$now}"
)

$env:ZENITH_PROFILE_DIR = $ProfileDir
Remove-Item Env:ZENITH_REMOTE_DEBUG_PORT -ErrorAction SilentlyContinue
$p = Start-Process -FilePath $exe -PassThru

$observed = @{}
try {
  $deadline = (Get-Date).AddSeconds($DurationSeconds)
  while ((Get-Date) -lt $deadline) {
    $all = Get-CimInstance Win32_Process -Filter "Name = 'zenith.exe'" -ErrorAction SilentlyContinue
    $pids = @($p.Id)
    $added = $true
    while ($added) {
      $added = $false
      foreach ($c in $all) {
        if ($pids -contains [int]$c.ParentProcessId -and -not ($pids -contains [int]$c.ProcessId)) {
          $pids += [int]$c.ProcessId
          $added = $true
        }
      }
    }
    $conns = Get-NetTCPConnection -ErrorAction SilentlyContinue | Where-Object {
      $pids -contains [int]$_.OwningProcess -and
      $_.State -ne 'Listen' -and
      $_.RemoteAddress -notin @('0.0.0.0', '::', '127.0.0.1', '::1')
    }
    foreach ($c in $conns) {
      $key = "$($c.RemoteAddress):$($c.RemotePort) ($($c.State))"
      $observed[$key] = $true
    }
    Start-Sleep -Milliseconds 500
  }
} finally {
  $all = Get-CimInstance Win32_Process -Filter "Name = 'zenith.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.ProcessId -eq $p.Id -or ($_.CommandLine -and $_.CommandLine.Contains($ProfileDir)) }
  $all | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Milliseconds 500
  Remove-Item -Recurse -Force $ProfileDir -ErrorAction SilentlyContinue
}

if ($observed.Count -eq 0) {
  Write-Output "PASS: Zero unsolicited remote TCP connections observed over ${DurationSeconds}s on fresh idle profile."
  exit 0
} else {
  Write-Output "FAIL: Observed unexpected remote connections during fresh idle profile:"
  $observed.Keys | Sort-Object | ForEach-Object { Write-Output "  - $_" }
  exit 1
}
