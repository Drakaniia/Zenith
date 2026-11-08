# Capture the Zenith window. Default mode uses PrintWindow(PW_RENDERFULLCONTENT), which renders the
# window (including DirectComposition/WebView2 content) even when other windows cover it, without
# stealing focus. -Screen copies the on-screen pixels instead (needs the window on top).
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev/shot.ps1 [-Out path.png] [-Screen]
param(
  [string]$Out = (Join-Path $env:TEMP 'zenith-dev\shot.png'),
  [switch]$Screen,
  [int]$DelayMs = 250
)
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class ZWin {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
Add-Type -AssemblyName System.Drawing
[ZWin]::SetProcessDPIAware() | Out-Null
$p = Get-Process zenith -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { throw 'zenith window not found' }
$h = $p.MainWindowHandle
if ([ZWin]::IsIconic($h)) { [ZWin]::ShowWindow($h, 4) | Out-Null; Start-Sleep -Milliseconds 300 }
Start-Sleep -Milliseconds $DelayMs
New-Item -ItemType Directory -Force -Path (Split-Path $Out) | Out-Null

if ($Screen) {
  [ZWin]::SetForegroundWindow($h) | Out-Null
  Start-Sleep -Milliseconds 300
  $r = New-Object ZWin+RECT
  [ZWin]::DwmGetWindowAttribute($h, 9, [ref]$r, 16) | Out-Null
  $w = $r.R - $r.L; $hh = $r.B - $r.T
  $bmp = New-Object System.Drawing.Bitmap $w, $hh
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
} else {
  $r = New-Object ZWin+RECT
  [ZWin]::GetWindowRect($h, [ref]$r) | Out-Null
  $w = $r.R - $r.L; $hh = $r.B - $r.T
  $bmp = New-Object System.Drawing.Bitmap $w, $hh
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $hdc = $g.GetHdc()
  [ZWin]::PrintWindow($h, $hdc, 2) | Out-Null
  $g.ReleaseHdc($hdc)
}
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output "saved $Out ${w}x${hh}"
