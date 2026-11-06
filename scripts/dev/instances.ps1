# Lists running Zenith processes with their window title and start time.
Get-Process zenith -ErrorAction SilentlyContinue | ForEach-Object {
  [pscustomobject]@{ Id = $_.Id; Title = $_.MainWindowTitle; Started = $_.StartTime.ToString('T'); Path = $_.Path }
} | Format-Table -AutoSize | Out-String -Width 200
