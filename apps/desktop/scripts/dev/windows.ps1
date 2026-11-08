# Lists the visible top-level windows owned by the Zenith test instance (or a given PID).
param([int]$ProcessId = 0)
if (-not $ProcessId) {
  $pidFile = Join-Path $env:TEMP 'zenith-dev\test-instance.pid'
  $ProcessId = [int](Get-Content $pidFile)
}
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class ZTop {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  public static List<string> List(uint pid) {
    var result = new List<string>();
    EnumWindows((h, l) => {
      uint owner; GetWindowThreadProcessId(h, out owner);
      if (owner == pid && IsWindowVisible(h)) {
        var t = new StringBuilder(256); GetWindowText(h, t, 256);
        var c = new StringBuilder(256); GetClassName(h, c, 256);
        result.Add(h.ToString("X") + " [" + c + "] " + t);
      }
      return true;
    }, IntPtr.Zero);
    return result;
  }
}
"@
$wins = [ZTop]::List([uint32]$ProcessId)
Write-Output "pid $ProcessId has $($wins.Count) visible top-level window(s):"
$wins | ForEach-Object { Write-Output "  $_" }
