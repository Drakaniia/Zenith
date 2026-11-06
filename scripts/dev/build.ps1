# Builds the frontend and the debug (or release) binary. A running zenith.exe locks its file,
# which makes the linker fail; Windows still allows renaming a running executable, so locked
# copies are renamed out of the way first and the running window keeps working.
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev/build.ps1 [-Release]
param([switch]$Release)
$ErrorActionPreference = 'Continue'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$profileName = if ($Release) { 'release' } else { 'debug' }
$dir = Join-Path $root "src-tauri\target\$profileName"
$candidates = @()
$candidates += Get-ChildItem -Path $dir -Filter 'zenith.exe' -ErrorAction SilentlyContinue
# Cargo links the binary as deps\zenith.exe (sometimes with a hash suffix), then hard-links it.
$candidates += Get-ChildItem -Path (Join-Path $dir 'deps') -Filter 'zenith*.exe' -ErrorAction SilentlyContinue
foreach ($f in $candidates) {
  try {
    [IO.File]::Open($f.FullName, 'Open', 'ReadWrite', 'None').Dispose()
  } catch {
    $new = "$($f.Name).inuse-$(Get-Date -Format yyyyMMddHHmmss)"
    Rename-Item -Path $f.FullName -NewName $new
    Write-Output "renamed locked $($f.Name) -> $new"
  }
}
# Delete renamed copies that are no longer running.
Get-ChildItem -Path $dir, (Join-Path $dir 'deps') -Filter '*.inuse-*' -ErrorAction SilentlyContinue |
  ForEach-Object { Remove-Item $_.FullName -ErrorAction SilentlyContinue }

Push-Location $root
try {
  & pnpm build 2>&1 | Select-String -Pattern 'built in|error' | ForEach-Object { $_.Line }
  if ($LASTEXITCODE -ne 0) { throw 'frontend build failed' }
  Push-Location (Join-Path $root 'src-tauri')
  $cargoArgs = @('build', '--features', 'tauri/custom-protocol', '--message-format', 'short')
  if ($Release) { $cargoArgs += '--release' }
  & cargo @cargoArgs 2>&1 | Select-String -Pattern 'error|warning|Finished' | ForEach-Object { $_.Line }
  $code = $LASTEXITCODE
  Pop-Location
  if ($code -ne 0) { throw "cargo build failed ($code)" }
} finally {
  Pop-Location
}
