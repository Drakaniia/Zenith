# Builds and verifies the Phase 1 CEF spike (zero WebView2, unit tests, single-instance lock, live CEF self-test).
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File spikes/cef-phase1/run-spike.ps1 [-UiDir <path>]
param(
  [string]$CefDir = "$env:LOCALAPPDATA\zenith-cef\156.0.31",
  [string]$UiDir = ""
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

Push-Location $spikeDir
try {
  Write-Output "== 1. Verify zero wry/webview2 dependencies =="
  $tree = & cargo tree
  if ($tree -match "wry|webview2") {
    throw "Found disallowed wry/webview2 dependency in Phase 1 spike!"
  }
  Write-Output "cargo tree clean (no wry/webview2)"

  Write-Output "== 2. Run Rust unit tests =="
  & cargo test
  if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }

  Write-Output "== 3. Build Phase 1 spike binary =="
  & cargo build
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

  $exe = Join-Path $spikeDir "target\debug\zenith-cef-phase1.exe"
  $profile = Join-Path $env:TEMP ("zenith-cef-phase1-run-" + [Guid]::NewGuid().ToString("N").Substring(0, 8))

  Write-Output "== 4. Verify per-profile single-instance lock =="
  $holder = Start-Process -FilePath $exe -ArgumentList "--profile-dir", $profile, "--hold-lock-ms", "4000" -PassThru -NoNewWindow
  Start-Sleep -Milliseconds 600
  $second = Start-Process -FilePath $exe -ArgumentList "--profile-dir", $profile, "--hold-lock-ms", "100" -PassThru -Wait -NoNewWindow
  $secondCode = $second.ExitCode
  $holder.WaitForExit()
  if ($secondCode -ne 10) {
    throw "Expected single-instance exit code 10, got $secondCode"
  }
  Write-Output "single-instance lock verified (second launch exited 10)"

  Write-Output "== 5. Run live CEF self-test (trusted bridge, untrusted isolation, fail-closed nav, crash recovery) =="
  $runArgs = @("--profile-dir", $profile, "--cef-dir", $CefDir)
  if ($UiDir -ne "") {
    $runArgs += @("--ui-dir", $UiDir)
  } else {
    $runArgs += "--self-test"
  }
  & $exe @runArgs
  if ($LASTEXITCODE -ne 0) { throw "live CEF spike exited with $LASTEXITCODE" }
} finally {
  Pop-Location
}
