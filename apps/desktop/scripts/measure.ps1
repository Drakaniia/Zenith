# Memory and startup measurement for Zenith (plan section 7).
#
# Launches the release build on a throwaway profile (ZENITH_PROFILE_DIR, so it never touches
# your own profile or windows), opens N tabs from the command line, waits for them to settle,
# then sums private bytes and working set over zenith.exe and every process it spawned (the
# WebView2 browser, GPU, utility and renderer processes), with a per-process-type breakdown.
#
#   powershell -ExecutionPolicy Bypass -File scripts/measure.ps1                # all scenarios
#   powershell -ExecutionPolicy Bypass -File scripts/measure.ps1 -Scenario 1tab
#
# Results are printed and appended to measurements/memory.csv.
param(
  [ValidateSet('all', 'idle', '1tab', '5tabs', '10tabs', 'sleep', 'edge')] [string]$Scenario = 'all',
  [int]$SettleSeconds = 12,
  [string]$Exe = (Join-Path $PSScriptRoot '..\src-tauri\target\release\zenith.exe'),
  [string]$ProfileDir = (Join-Path $env:TEMP 'zenith-measure-profile')
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$out = Join-Path $root 'measurements'
New-Item -ItemType Directory -Force -Path $out | Out-Null
$csv = Join-Path $out 'memory.csv'
if (-not (Test-Path $csv)) {
  'date,version,scenario,tabs,processes,private_mb,working_set_mb,startup_ms,browser_mb,gpu_mb,utility_mb,renderers,renderer_mb,zenith_mb' |
    Out-File -Encoding ascii $csv
}

function Get-Family([int]$rootPid) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name, CommandLine
  $family = @{ $rootPid = $true }
  do {
    $added = $false
    foreach ($p in $all) {
      if ($family.ContainsKey([int]$p.ParentProcessId) -and -not $family.ContainsKey([int]$p.ProcessId)) {
        $family[[int]$p.ProcessId] = $true
        $added = $true
      }
    }
  } while ($added)
  foreach ($p in $all) {
    if (-not $family.ContainsKey([int]$p.ProcessId)) { continue }
    $proc = Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue
    if (-not $proc) { continue }
    $type = if ($p.Name -eq 'zenith.exe') { 'zenith' }
      elseif ($p.CommandLine -match '--type=([a-z-]+)') { $Matches[1] }
      elseif ($p.Name -eq 'msedgewebview2.exe') { 'browser' }
      else { $p.Name }
    [pscustomobject]@{ Id = $p.ProcessId; Type = $type; Private = $proc.PrivateMemorySize64; WorkingSet = $proc.WorkingSet64 }
  }
}

function Stop-Run([int]$rootPid) {
  Get-Process -Id $rootPid -ErrorAction SilentlyContinue | Stop-Process -Force
  for ($i = 0; $i -lt 30; $i++) {
    $left = Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" |
      Where-Object { $_.CommandLine -and $_.CommandLine.Contains($ProfileDir) }
    if (-not $left) { return }
    if ($i -gt 10) { $left | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue } }
    Start-Sleep -Milliseconds 300
  }
}

function Measure-Run([string]$name, [string[]]$urls, [string]$settingsJson) {
  if (Test-Path $ProfileDir) { Remove-Item -Recurse -Force $ProfileDir }
  New-Item -ItemType Directory -Force -Path (Join-Path $ProfileDir 'roaming') | Out-Null
  # No filter-list download during the measurement; the built-in list stays active.
  $settings = if ($settingsJson) { $settingsJson } else { '{}' }
  $obj = $settings | ConvertFrom-Json
  $obj | Add-Member -NotePropertyName filterListsUpdated -NotePropertyValue ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()) -Force
  [IO.File]::WriteAllText((Join-Path $ProfileDir 'roaming\settings.json'), ($obj | ConvertTo-Json -Depth 5))
  $env:ZENITH_PROFILE_DIR = $ProfileDir
  $env:ZENITH_PERF = '1'
  Remove-Item Env:ZENITH_REMOTE_DEBUG_PORT -ErrorAction SilentlyContinue
  $p = if ($urls.Count) { Start-Process -FilePath $Exe -ArgumentList $urls -PassThru } else { Start-Process -FilePath $Exe -PassThru }
  $perf = Join-Path $ProfileDir 'local\perf.json'
  $startup = ''
  for ($i = 0; $i -lt 150 -and -not (Test-Path $perf); $i++) { Start-Sleep -Milliseconds 100 }
  if (Test-Path $perf) { $startup = (Get-Content $perf | ConvertFrom-Json).startupMs }
  Start-Sleep -Seconds $SettleSeconds
  $procs = @(Get-Family $p.Id)
  $mb = { param($list) [math]::Round((($list | Measure-Object -Property Private -Sum).Sum) / 1MB, 1) }
  $private = & $mb $procs
  $ws = [math]::Round((($procs | Measure-Object -Property WorkingSet -Sum).Sum) / 1MB, 1)
  $renderers = @($procs | Where-Object Type -eq 'renderer')
  $row = [ordered]@{
    browser = & $mb @($procs | Where-Object Type -eq 'browser')
    gpu = & $mb @($procs | Where-Object Type -eq 'gpu-process')
    utility = & $mb @($procs | Where-Object Type -eq 'utility')
    renderer = & $mb $renderers
    zenith = & $mb @($procs | Where-Object Type -eq 'zenith')
  }
  $version = (Get-Item $Exe).VersionInfo.ProductVersion
  ('{0},{1},{2},{3},{4},{5},{6},{7},{8},{9},{10},{11},{12},{13}' -f (Get-Date -Format s), $version, $name, $urls.Count, $procs.Count,
    $private, $ws, $startup, $row.browser, $row.gpu, $row.utility, $renderers.Count, $row.renderer, $row.zenith) | Out-File -Append -Encoding ascii $csv
  Write-Output ("{0,-6} tabs={1} procs={2,-2} private={3,6} MB  ws={4,6} MB  startup={5,4} ms | zenith {6} | browser {7} | gpu {8} | utility {9} | {10} renderer(s) {11} MB" -f `
    $name, $urls.Count, $procs.Count, $private, $ws, $startup, $row.zenith, $row.browser, $row.gpu, $row.utility, $renderers.Count, $row.renderer)
  Stop-Run $p.Id
}

# Edge baseline: one tab in a throwaway Edge profile (its own browser process tree).
function Measure-Edge {
  $edge = Join-Path ${env:ProgramFiles(x86)} 'Microsoft\Edge\Application\msedge.exe'
  if (-not (Test-Path $edge)) { Write-Output 'edge: not installed'; return }
  $dir = Join-Path $env:TEMP 'zenith-measure-edge'
  if (Test-Path $dir) { Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue }
  $p = Start-Process -FilePath $edge -PassThru -ArgumentList @("--user-data-dir=`"$dir`"", '--no-first-run', '--no-default-browser-check', 'https://example.com/')
  Start-Sleep -Seconds $SettleSeconds
  $all = Get-CimInstance Win32_Process -Filter "Name = 'msedge.exe'" | Where-Object { $_.CommandLine -and $_.CommandLine.Contains($dir) }
  $procs = foreach ($c in $all) {
    $proc = Get-Process -Id $c.ProcessId -ErrorAction SilentlyContinue
    if ($proc) { [pscustomobject]@{ Private = $proc.PrivateMemorySize64; WorkingSet = $proc.WorkingSet64; Type = if ($c.CommandLine -match '--type=([a-z-]+)') { $Matches[1] } else { 'browser' } } }
  }
  $procs = @($procs)
  $private = [math]::Round((($procs | Measure-Object -Property Private -Sum).Sum) / 1MB, 1)
  $ws = [math]::Round((($procs | Measure-Object -Property WorkingSet -Sum).Sum) / 1MB, 1)
  $gpu = [math]::Round((($procs | Where-Object Type -eq 'gpu-process' | Measure-Object -Property Private -Sum).Sum) / 1MB, 1)
  $renderers = @($procs | Where-Object Type -eq 'renderer').Count
  ('{0},edge,edge-1tab,1,{1},{2},{3},,,{4},,{5},,' -f (Get-Date -Format s), $procs.Count, $private, $ws, $gpu, $renderers) | Out-File -Append -Encoding ascii $csv
  Write-Output ("edge   tabs=1 procs={0,-2} private={1,6} MB  ws={2,6} MB | gpu {3} | {4} renderer(s)" -f $procs.Count, $private, $ws, $gpu, $renderers)
  $all | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

$pages = @('https://example.com/', 'https://example.org/', 'https://example.net/', 'https://www.iana.org/help/example-domains', 'https://www.rfc-editor.org/')
$pages10 = $pages + @('https://info.cern.ch/', 'https://www.rfc-editor.org/rfc/rfc2606.html', 'https://www.iana.org/domains', 'https://www.w3.org/', 'https://html.spec.whatwg.org/dev/')
if ($Scenario -in 'all', 'idle') { Measure-Run 'idle' @() $null }
if ($Scenario -in 'all', '1tab') { Measure-Run '1tab' @($pages[0]) $null }
if ($Scenario -in 'all', '5tabs') { Measure-Run '5tabs' $pages $null }
if ($Scenario -in 'all', '10tabs') { Measure-Run '10tabs' $pages10 $null }
if ($Scenario -in 'all', 'edge') { Measure-Edge }
if ($Scenario -in 'all', 'sleep') {
  # Same five tabs, background tabs put to sleep after one minute.
  $SettleSeconds = 100
  Measure-Run 'sleep' $pages '{"sleepMinutes":1,"suspendHidden":true}'
}
