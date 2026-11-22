# Reproducible Windows x64 custom CEF + Chromium build driver (Section 15.1).
# Applies Zenith's auditable patch set (proprietary codecs, Widevine hook,
# OnAudioStateChanged, and native extension tabs/windows/badge delegate),
# builds `libcef.dll` + `bootstrap.exe`, prunes locales, and verifies the 400 MB budget.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File cef-build/build-cef.ps1 [-CheckOnly]
param(
  [string]$WorkDir = 'C:\cef-build',
  [string]$OutDir = "$env:LOCALAPPDATA\zenith-cef\156.0.31-custom",
  [switch]$CheckOnly
)
$ErrorActionPreference = 'Stop'

$root = Resolve-Path $PSScriptRoot
$manifest = Get-Content (Join-Path $root 'manifest.json') -Raw | ConvertFrom-Json
$argsGn = Get-Content (Join-Path $root 'args.gn') -Raw
$patches = Get-ChildItem (Join-Path $root 'patches') -Filter '*.patch' | Sort-Object Name

Write-Output "Zenith Custom CEF Build Pipeline"
Write-Output "  CEF:      $($manifest.cef.version)"
Write-Output "  Chromium: $($manifest.chromium.version)"
Write-Output "  Patches:  $($patches.Count)"

# Verify every patch listed in manifest.json exists on disk.
foreach ($p in $manifest.patches) {
  $fp = Join-Path $root $p.file
  if (-not (Test-Path $fp)) { throw "Missing patch declared in manifest.json: $fp" }
}

$drive = (Split-Path -Qualifier (Resolve-Path -LiteralPath (Split-Path -Parent $WorkDir) -ErrorAction SilentlyContinue | ForEach-Object Path) 2>$null)
if (-not $drive) { $drive = 'C:' }
$freeGb = [math]::Round((Get-PSDrive ($drive.TrimEnd(':'))).Free / 1GB, 1)
$hasDepot = [bool](Get-Command gclient -ErrorAction SilentlyContinue)
$hasNinja = [bool](Get-Command ninja -ErrorAction SilentlyContinue)

Write-Output "  Disk:     $freeGb GB free on $drive (minimum $($manifest.toolchain.minFreeDiskGb) GB)"
Write-Output "  depot_tools (gclient): $hasDepot"
Write-Output "  ninja:    $hasNinja"

if ($CheckOnly) {
  Write-Output "CheckOnly complete: manifest, args.gn, and $($patches.Count) patches verified."
  exit 0
}

if ($freeGb -lt $manifest.toolchain.minFreeDiskGb) {
  throw "Insufficient free disk space on $drive ($freeGb GB < $($manifest.toolchain.minFreeDiskGb) GB)."
}
if (-not $hasDepot) {
  throw "depot_tools (gclient) not found on PATH. Install Chromium depot_tools before running a full build."
}

New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
$automate = Join-Path $WorkDir 'automate-git.py'
if (-not (Test-Path $automate)) {
  Invoke-WebRequest -Uri 'https://bitbucket.org/chromiumembedded/cef/raw/master/tools/automate/automate-git.py' -OutFile $automate
}

# Export GN args for automate-git.py (single-line space-separated assignments).
$gnLines = ($argsGn -split "`r?`n") | Where-Object { $_ -and -not $_.TrimStart().StartsWith('#') }
$env:GN_DEFINES = ($gnLines -join ' ')
$env:CEF_USE_GN = '1'
$env:GYP_MSVS_VERSION = '2022'

Write-Output "1/4 Syncing Chromium $($manifest.chromium.version) & CEF branch $($manifest.cef.branch)..."
python $automate `
  --download-dir="$WorkDir" `
  --branch="$($manifest.cef.branch)" `
  --checkout="$($manifest.cef.commit)" `
  --x64-build `
  --no-build `
  --no-distrib

$cefSrc = Join-Path $WorkDir 'chromium\src\cef'
$chrSrc = Join-Path $WorkDir 'chromium\src'

Write-Output "2/4 Verifying and applying Zenith patch set..."
foreach ($p in $manifest.patches) {
  $repoDir = if ($p.repo -eq 'cef') { $cefSrc } else { $chrSrc }
  $patchFile = Join-Path $root $p.file
  Push-Location $repoDir
  try {
    & git apply --check $patchFile
    if ($LASTEXITCODE -ne 0) { throw "Patch check failed for $($p.file) in $repoDir" }
    & git apply $patchFile
    Write-Output "  applied $($p.file)"
  } finally {
    Pop-Location
  }
}

Write-Output "3/4 Building Release x64 CEF + minimal binary distribution..."
python $automate `
  --download-dir="$WorkDir" `
  --branch="$($manifest.cef.branch)" `
  --x64-build `
  --no-update `
  --force-build `
  --minimal-distrib `
  --client-distrib `
  --no-debug-build

Write-Output "4/4 Staging pruned runtime & generating SHA-256 runtime manifest..."
$distDir = Get-ChildItem (Join-Path $cefSrc 'binary_distrib') -Directory -Filter '*windows64_minimal' | Select-Object -First 1
if (-not $distDir) { throw "CEF binary_distrib output not found" }

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Copy-Item -Path (Join-Path $distDir.FullName 'Release\*') -Destination $OutDir -Recurse -Force
Copy-Item -Path (Join-Path $distDir.FullName 'Resources\*') -Destination $OutDir -Recurse -Force

# Prune locales to supported UI languages (en-US.pak) to stay within the 400 MB budget.
$keepLocales = $manifest.budgets.prunedLocales
Get-ChildItem (Join-Path $OutDir 'locales') -Filter '*.pak' -ErrorAction SilentlyContinue |
  Where-Object { $keepLocales -notcontains $_.Name } |
  Remove-Item -Force

$files = Get-ChildItem $OutDir -Recurse -File
$totalMb = [math]::Round((($files | Measure-Object Length -Sum).Sum) / 1MB, 2)
$hashes = $files | ForEach-Object {
  [ordered]@{
    path = $_.FullName.Substring($OutDir.Length).TrimStart('\')
    bytes = $_.Length
    sha256 = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  }
}
$runtimeManifest = [ordered]@{
  cefVersion = $manifest.cef.version
  chromiumVersion = $manifest.chromium.version
  unpackedMb = $totalMb
  files = $hashes
}
$runtimeManifest | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutDir 'runtime-manifest.json') -Encoding UTF8

Write-Output "Custom CEF staged at $OutDir ($totalMb MB unpacked; budget <= $($manifest.budgets.unpackedRuntimeMaxMb) MB)"
if ($totalMb -gt $manifest.budgets.unpackedRuntimeMaxMb) {
  throw "Unpacked CEF runtime ($totalMb MB) exceeds budget ($($manifest.budgets.unpackedRuntimeMaxMb) MB)"
}
