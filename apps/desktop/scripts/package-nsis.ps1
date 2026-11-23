# Packages the release CEF bundle into a signed per-user NSIS installer and updater feed JSON.
#   powershell -NoProfile -ExecutionPolicy Bypass -File apps/desktop/scripts/package-nsis.ps1 [-Preview] [-SkipBuild]
param(
  [switch]$Preview,
  [switch]$SkipBuild,
  [string]$CefDir = $(if ($env:CEF_PATH) { $env:CEF_PATH } else { "$env:LOCALAPPDATA\zenith-cef\156.0.31" }),
  [string]$DownloadBaseUrl = ''
)
$ErrorActionPreference = 'Stop'
$desktopRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$pkg = Get-Content (Join-Path $desktopRoot 'package.json') -Raw | ConvertFrom-Json
$version = $pkg.version
$stageDir = Join-Path $desktopRoot 'src-tauri\target\release'
$outDir = Join-Path $stageDir 'bundle\nsis'
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

if (-not $SkipBuild) {
  & (Join-Path $PSScriptRoot 'dev\build.ps1') -Release -CefDir $CefDir
  if ($LASTEXITCODE -ne 0) { throw "release build failed ($LASTEXITCODE)" }
}

$manifestPath = Join-Path $stageDir 'runtime-manifest.json'
if (-not (Test-Path $manifestPath)) {
  throw "runtime-manifest.json missing at $manifestPath"
}

# Verify unpacked runtime budget (<= 400 MB per spec Section 12.1).
$manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
$unpackedBytes = 0L
foreach ($prop in $manifest.files.PSObject.Properties) {
  $p = Join-Path $stageDir ($prop.Name -replace '/', '\')
  if (Test-Path $p) { $unpackedBytes += (Get-Item $p).Length }
}
$uiBytes = (Get-ChildItem -Recurse -File (Join-Path $stageDir 'ui') | Measure-Object -Property Length -Sum).Sum
$totalUnpackedMb = [math]::Round(($unpackedBytes + $uiBytes) / 1MB, 1)
Write-Output "Unpacked app + CEF runtime: ${totalUnpackedMb} MB (budget <= 400 MB)"
if ($totalUnpackedMb -gt 400) {
  throw "Unpacked bundle (${totalUnpackedMb} MB) exceeds 400 MB spec budget"
}

$makensis = Get-Command makensis -ErrorAction SilentlyContinue
if (-not $makensis) {
  $candidates = @('C:\Program Files (x86)\NSIS\makensis.exe', 'C:\Program Files\NSIS\makensis.exe')
  $found = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
  if (-not $found) { throw 'makensis.exe not found. Install NSIS (choco install nsis).' }
  $makensisPath = $found
} else {
  $makensisPath = $makensis.Source
}

$installerName = "Zenith_${version}_x64-setup.exe"
$installerPath = Join-Path $outDir $installerName
$nsiPath = Join-Path $desktopRoot 'src-tauri\windows\installer.nsi'
$iconPath = Join-Path $desktopRoot 'src-tauri\icons\icon.ico'

& $makensisPath `
  "/DVERSION=$version" `
  "/DSTAGE_DIR=$stageDir" `
  "/DOUT_FILE=$installerPath" `
  "/DICON_FILE=$iconPath" `
  $nsiPath
if ($LASTEXITCODE -ne 0) { throw "makensis failed ($LASTEXITCODE)" }

$installerMb = [math]::Round((Get-Item $installerPath).Length / 1MB, 1)
Write-Output "NSIS installer: $installerPath (${installerMb} MB, budget <= 150 MB)"
if ($installerMb -gt 150) {
  throw "Installer (${installerMb} MB) exceeds 150 MB spec budget"
}

Copy-Item -Path $manifestPath -Destination (Join-Path $outDir 'runtime-manifest.json') -Force

# Build the lightweight web stub installer (ZenithSetup.exe or ZenithPreviewSetup.exe).
$stubManifest = Join-Path $desktopRoot 'stub-installer\Cargo.toml'
$stubArgs = @('build', '--manifest-path', $stubManifest, '--release')
if ($Preview) { $stubArgs += @('--features', 'preview') }
& cargo @stubArgs
if ($LASTEXITCODE -ne 0) { throw "stub installer build failed ($LASTEXITCODE)" }
$stubName = if ($Preview) { 'ZenithPreviewSetup.exe' } else { 'ZenithSetup.exe' }
$stubPath = Join-Path $outDir $stubName
Copy-Item -Path (Join-Path $desktopRoot 'stub-installer\target\release\zenith-setup.exe') -Destination $stubPath -Force
$stubKb = [math]::Round((Get-Item $stubPath).Length / 1KB, 1)
Write-Output "Web stub installer: $stubPath (${stubKb} KB)"

# Optional minisign signature + update feed generation (compatible with TAURI_SIGNING_PRIVATE_KEY).
if ($env:TAURI_SIGNING_PRIVATE_KEY) {
  $sigPath = "$installerPath.sig"
  & cargo run --manifest-path (Join-Path $desktopRoot 'src-tauri\Cargo.toml') --release --features signer --bin zenith-sign -- $installerPath $sigPath
  if ($LASTEXITCODE -ne 0) { throw "minisign signing failed ($LASTEXITCODE)" }

  $sigB64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($sigPath))
  $tag = "v$version"
  $baseUrl = if ($DownloadBaseUrl) { $DownloadBaseUrl.TrimEnd('/') } else { "https://github.com/Drakaniia/Zenith/releases/download/$tag" }
  $feedName = if ($Preview) { 'preview.json' } else { 'latest.json' }
  $feed = [ordered]@{
    version = $version
    notes = "Zenith $tag (CEF 156.0.31 / Chromium 156.0.8078.100)"
    pub_date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
      'windows-x86_64' = [ordered]@{
        signature = $sigB64
        url = "$baseUrl/$installerName"
      }
    }
  }
  $feedPath = Join-Path $outDir $feedName
  [System.IO.File]::WriteAllText($feedPath, ($feed | ConvertTo-Json -Depth 5), [System.Text.UTF8Encoding]::new($false))
  Write-Output "Signed installer and wrote $feedPath"
}
