# Sends real keyboard input to Zenith, but only when Zenith is the foreground window, so keys
# never leak into another application. Exit code 2 = Zenith could not be brought to front.
# Usage: powershell -File scripts/dev/keys.ps1 -Chord "ctrl+t" [-Chord "escape"]...
param([string[]]$Chord)
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class ZKeys {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool attach);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
}
"@
$p = Get-Process zenith -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Output 'zenith-not-running'; exit 3 }
$h = $p.MainWindowHandle

function Test-Front { [ZKeys]::GetForegroundWindow() -eq $h }

if (-not (Test-Front)) {
  if ([ZKeys]::IsIconic($h)) { [ZKeys]::ShowWindow($h, 9) | Out-Null }
  $fg = [ZKeys]::GetForegroundWindow()
  $dummy = 0
  $fgThread = [ZKeys]::GetWindowThreadProcessId($fg, [ref]$dummy)
  $me = [ZKeys]::GetCurrentThreadId()
  [ZKeys]::AttachThreadInput($me, $fgThread, $true) | Out-Null
  [ZKeys]::SetForegroundWindow($h) | Out-Null
  [ZKeys]::AttachThreadInput($me, $fgThread, $false) | Out-Null
  Start-Sleep -Milliseconds 250
}
if (-not (Test-Front)) { Write-Output 'not-foreground'; exit 2 }

$vk = @{
  ctrl = 0x11; shift = 0x10; alt = 0x12; escape = 0x1B; enter = 0x0D; tab = 0x09; left = 0x25; right = 0x27;
  up = 0x26; down = 0x28; f5 = 0x74; f6 = 0x75; f11 = 0x7A; f12 = 0x7B; pageup = 0x21; pagedown = 0x22
}
foreach ($c in $Chord) {
  if (-not (Test-Front)) { Write-Output 'lost-foreground'; exit 2 }
  $parts = $c.ToLower().Split('+')
  $codes = foreach ($k in $parts) {
    if ($vk.ContainsKey($k)) { $vk[$k] } elseif ($k.Length -eq 1) { [int][char]$k.ToUpper() } else { throw "unknown key $k" }
  }
  foreach ($code in $codes) { [ZKeys]::keybd_event([byte]$code, 0, 0, [UIntPtr]::Zero) }
  [array]::Reverse($codes)
  foreach ($code in $codes) { [ZKeys]::keybd_event([byte]$code, 0, 2, [UIntPtr]::Zero) }
  Start-Sleep -Milliseconds 350
  Write-Output "sent $c"
}
