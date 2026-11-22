# Builds and runs the Phase 2 CEF rendering + MV3 extension go/no-go prototype.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File spikes/cef-phase2/run-spike.ps1 [-Style chrome|alloy]
param(
  [string]$CefDir = "$env:LOCALAPPDATA\zenith-cef\156.0.31",
  [ValidateSet("chrome", "alloy", "default")]
  [string]$Style = "chrome"
)

$ErrorActionPreference = "Stop"
$spikeDir = $PSScriptRoot
$extraPaths = @("$env:USERPROFILE\.cargo\bin", $CefDir)
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
  $vsCmake = @(
    "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin",
    "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin",
    "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin",
    "C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"
  ) | Where-Object { Test-Path (Join-Path $_ "cmake.exe") } | Select-Object -First 1
  if ($vsCmake) { $extraPaths += $vsCmake }
}
$env:PATH = (($extraPaths -join ";") + ";" + $env:PATH)
$env:CEF_PATH = $CefDir

if (-not (Test-Path (Join-Path $CefDir "libcef.dll"))) {
  throw "CEF runtime not found at $CefDir. Run: export-cef-dir --force `"$CefDir`""
}

$regPath = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.zenith.phase2.echo"
$tempRunDir = Join-Path $env:TEMP ("zenith-cef-phase2-run-" + [Guid]::NewGuid().ToString("N").Substring(0, 8))

Push-Location $spikeDir
try {
  Write-Output "== 1. Verify zero wry/webview2 dependencies =="
  $tree = & cargo tree
  if ($tree -match "wry|webview2") {
    throw "Found disallowed wry/webview2 dependency in Phase 2 spike!"
  }
  Write-Output "cargo tree clean (no wry/webview2)"

  Write-Output "== 2. Run Rust unit tests (native messaging stdio framing) =="
  & cargo test
  if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }

  Write-Output "== 3. Build Phase 2 spike binary =="
  & cargo build
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

  $exe = Join-Path $spikeDir "target\debug\zenith-cef-phase2.exe"
  $extDir = Join-Path $spikeDir "fixtures\zenith-phase2-ext"
  New-Item -ItemType Directory -Path $tempRunDir -Force | Out-Null

  # Register temporary Chrome Native Messaging host manifest pointing to our `--native-host` mode.
  $hostBat = Join-Path $tempRunDir "native-host.cmd"
  Set-Content -Path $hostBat -Value "@echo off`r`n`"$exe`" --native-host %*" -Encoding Ascii
  $manifestPath = Join-Path $tempRunDir "com.zenith.phase2.echo.json"
  $manifestObj = @{
    name = "com.zenith.phase2.echo"
    description = "Zenith Phase 2 Native Messaging Echo Host"
    path = $hostBat
    type = "stdio"
    allowed_origins = @("chrome-extension://fkjmjfpacnjjgnppeenhiolleglehgik/")
  } | ConvertTo-Json
  Set-Content -Path $manifestPath -Value $manifestObj -Encoding UTF8
  New-Item -Path $regPath -Force | Out-Null
  Set-ItemProperty -Path $regPath -Name "(Default)" -Value $manifestPath

  Write-Output "== 4. Run live Phase 2 prototype (style=$Style) =="
  $profileDir = Join-Path $tempRunDir "profile"
  $stdoutFile = Join-Path $tempRunDir "stdout.txt"
  $stderrFile = Join-Path $tempRunDir "stderr.txt"
  $proc = Start-Process -FilePath $exe `
    -ArgumentList "--profile-dir", $profileDir, "--cef-dir", $CefDir, "--ext-dir", $extDir, "--style", $Style `
    -PassThru -NoNewWindow -RedirectStandardOutput $stdoutFile -RedirectStandardError $stderrFile
  $null = $proc.Handle
  if (-not $proc.WaitForExit(30000)) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    if (Test-Path $stderrFile) { Get-Content $stderrFile }
    if (Test-Path $stdoutFile) { Get-Content $stdoutFile }
    throw "Phase 2 prototype timed out after 30s"
  }
  $proc.WaitForExit()
  if (Test-Path $stderrFile) { Get-Content $stderrFile }
  if (Test-Path $stdoutFile) { Get-Content $stdoutFile }
  if ($proc.ExitCode -ne 0) { throw "Phase 2 prototype exited with $($proc.ExitCode)" }
} finally {
  if (Test-Path $regPath) {
    Remove-Item -Path $regPath -Force -ErrorAction SilentlyContinue
  }
  Pop-Location
}
