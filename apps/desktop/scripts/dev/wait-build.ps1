# Waits for a build started by build-release.ps1 to finish, then prints its summary lines.
param([int]$TimeoutSeconds = 560)
$log = Join-Path $env:TEMP 'zenith-dev\build.log'
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
while ((Get-Date) -lt $deadline) {
  if ((Test-Path $log) -and (Select-String -Path $log -Pattern 'BUILD-EXIT' -Quiet)) { break }
  Start-Sleep -Seconds 5
}
Select-String -Path $log -Pattern 'Finished|BUILD-EXIT|error' | ForEach-Object { $_.Line }
