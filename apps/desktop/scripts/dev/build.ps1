# Builds the frontend and the CEF sandboxed debug (or release) bundle (zenith.dll + bootstrap.exe
# renamed to zenith.exe + colocated CEF runtime DLLs + pruned en-US.pak locale).
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev/build.ps1 [-Release]
param(
  [switch]$Release,
  [switch]$Offline,
  [string]$CefDir = $(if ($env:CEF_PATH) { $env:CEF_PATH } else { "$env:LOCALAPPDATA\zenith-cef\156.0.31" })
)
$ErrorActionPreference = 'Continue'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$profileName = if ($Release) { 'release' } else { 'debug' }
$dir = Join-Path $root "src-tauri\target\$profileName"

if (-not (Test-Path (Join-Path $CefDir 'libcef.dll'))) {
  throw "CEF runtime not found at $CefDir. Run: export-cef-dir --force `"$CefDir`""
}

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

$candidates = @()
$candidates += Get-ChildItem -Path $dir -Filter 'zenith.exe' -ErrorAction SilentlyContinue
$candidates += Get-ChildItem -Path $dir -Filter 'zenith.dll' -ErrorAction SilentlyContinue
$candidates += Get-ChildItem -Path (Join-Path $dir 'deps') -Filter 'zenith*.dll' -ErrorAction SilentlyContinue
foreach ($f in $candidates) {
  try {
    [IO.File]::Open($f.FullName, 'Open', 'ReadWrite', 'None').Dispose()
  } catch {
    $new = "$($f.Name).inuse-$(Get-Date -Format yyyyMMddHHmmss)"
    Rename-Item -Path $f.FullName -NewName $new
    Write-Output "renamed locked $($f.Name) -> $new"
  }
}
Get-ChildItem -Path $dir, (Join-Path $dir 'deps') -Filter '*.inuse-*' -ErrorAction SilentlyContinue |
  ForEach-Object { Remove-Item $_.FullName -ErrorAction SilentlyContinue }

Push-Location $root
try {
  & pnpm build 2>&1 | Select-String -Pattern 'built in|error' | ForEach-Object { $_.Line }
  if ($LASTEXITCODE -ne 0) { throw 'frontend build failed' }

  Push-Location (Join-Path $root 'src-tauri')
  $cargoArgs = @('build', '--lib', '--message-format', 'short')
  if ($Offline -or (-not $env:CI)) { $cargoArgs += '--offline' }
  if ($Release) { $cargoArgs += '--release' }
  & cargo @cargoArgs 2>&1 | Select-String -Pattern 'error|warning|Finished' | ForEach-Object { $_.Line }
  $code = $LASTEXITCODE
  Pop-Location
  if ($code -ne 0) { throw "cargo build failed ($code)" }

  # Stage CEF Windows sandbox host (`bootstrap.exe` -> `zenith.exe`) + colocated runtime DLLs.
  $bootstrapSrc = Join-Path $CefDir 'bootstrap.exe'
  $exeDst = Join-Path $dir 'zenith.exe'
  if (-not (Test-Path $exeDst) -or (Get-Item $exeDst).Length -ne (Get-Item $bootstrapSrc).Length) {
    Copy-Item -Path $bootstrapSrc -Destination $exeDst -Force
  }

  $manifest = @'
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
      <maxversiontested Id="10.0.18362.0"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="Win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
'@
  Set-Content -Path (Join-Path $dir 'zenith.exe.manifest') -Value $manifest -Encoding UTF8

  $runtimeFiles = @(
    'libcef.dll', 'chrome_elf.dll', 'd3dcompiler_47.dll', 'dxcompiler.dll',
    'icudtl.dat', 'resources.pak', 'chrome_100_percent.pak', 'chrome_200_percent.pak',
    'v8_context_snapshot.bin', 'vk_swiftshader.dll', 'vk_swiftshader_icd.json', 'vulkan-1.dll'
  )
  foreach ($rf in $runtimeFiles) {
    $src = Join-Path $CefDir $rf
    $dst = Join-Path $dir $rf
    if ((Test-Path $src) -and (-not (Test-Path $dst) -or (Get-Item $dst).Length -ne (Get-Item $src).Length)) {
      Copy-Item -Path $src -Destination $dst -Force
    }
  }
  $locDst = Join-Path $dir 'locales'
  New-Item -ItemType Directory -Force -Path $locDst | Out-Null
  $enPakSrc = Join-Path $CefDir 'locales\en-US.pak'
  $enPakDst = Join-Path $locDst 'en-US.pak'
  if ((Test-Path $enPakSrc) -and (-not (Test-Path $enPakDst))) {
    Copy-Item -Path $enPakSrc -Destination $enPakDst -Force
  }

  # Stage built Svelte UI assets under `ui/` for packaged `zenith://app/` serving.
  $distSrc = Join-Path $root 'dist'
  $uiDst = Join-Path $dir 'ui'
  if (Test-Path $uiDst) { Remove-Item -Recurse -Force $uiDst }
  Copy-Item -Path $distSrc -Destination $uiDst -Recurse -Force

  if ($Release) {
    $pkg = Get-Content (Join-Path $root 'package.json') -Raw | ConvertFrom-Json
    $manifestFiles = [ordered]@{}
    $toHash = @('zenith.exe', 'zenith.dll', 'zenith.exe.manifest', 'locales/en-US.pak', 'ui/index.html') + $runtimeFiles
    foreach ($rel in $toHash) {
      $full = Join-Path $dir ($rel -replace '/', '\')
      if (Test-Path $full) {
        $manifestFiles[$rel] = (Get-FileHash -Path $full -Algorithm SHA256).Hash.ToLowerInvariant()
      }
    }
    $rtManifest = [ordered]@{
      version = $pkg.version
      cefVersion = '156.0.31+ge603d97+chromium-156.0.8078.100'
      files = $manifestFiles
    }
    $rtManifest | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $dir 'runtime-manifest.json') -Encoding UTF8
  }
  Write-Output "staged CEF sandbox bundle at $exeDst"
} finally {
  Pop-Location
}
