# Lists processes whose command line mentions the test profile (or a given path).
param([string]$Needle = (Join-Path $env:TEMP 'zenith-test-profile'))
Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine.Contains($Needle) } |
  ForEach-Object { '{0,6} {1,-22} {2}' -f $_.ProcessId, $_.Name, $_.CommandLine.Substring(0, [Math]::Min(140, $_.CommandLine.Length)) }
Get-Process zenith -ErrorAction SilentlyContinue | ForEach-Object { 'zenith pid {0} "{1}" started {2}' -f $_.Id, $_.MainWindowTitle, $_.StartTime }
