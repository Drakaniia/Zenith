# Resizes the Zenith test instance's normal window (restored first if maximized), for layout checks.
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev/resize.ps1 -Width 560 -Height 600
param([int]$Width = 1280, [int]$Height = 820, [int]$ProcessId = 0)
if (-not $ProcessId) {
  $pidFile = Join-Path $env:TEMP 'zenith-dev\test-instance.pid'
  $ProcessId = [int](Get-Content $pidFile)
}
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class ZSize {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool repaint);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  public static IntPtr Find(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint owner; GetWindowThreadProcessId(h, out owner);
      if (owner == pid && IsWindowVisible(h)) {
        var t = new StringBuilder(256); GetWindowText(h, t, 256);
        if (t.ToString().StartsWith("Zenith") && !t.ToString().Contains("Incognito")) { found = h; return false; }
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@
$h = [ZSize]::Find([uint32]$ProcessId)
if ($h -eq [IntPtr]::Zero) { throw "no Zenith window for pid $ProcessId" }
[void][ZSize]::ShowWindow($h, 9) # SW_RESTORE
[void][ZSize]::MoveWindow($h, 80, 80, $Width, $Height, $true)
Write-Output "resized $($h.ToString('X')) to ${Width}x${Height}"
