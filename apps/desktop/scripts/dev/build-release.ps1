# Starts `pnpm tauri build` detached and logs to %TEMP%\zenith-dev\build.log; poll the log.
# A marker line "BUILD-EXIT <code>" is appended when the build finishes.
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$logDir = Join-Path $env:TEMP 'zenith-dev'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$log = Join-Path $logDir 'build.log'
$cmd = "cd /d `"$root`" && pnpm tauri build > `"$log`" 2>&1 & echo BUILD-EXIT %ERRORLEVEL% >> `"$log`""
$p = Start-Process -FilePath cmd.exe -ArgumentList '/c', $cmd -WindowStyle Hidden -PassThru
Write-Output "build started pid=$($p.Id) log=$log"
