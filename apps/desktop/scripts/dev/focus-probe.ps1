# Reproduces "shortcuts dead after switching windows" on the Zenith TEST instance (run.ps1).
# Brings the test window to front, switches to a throwaway window of this script, comes back
# (real Alt+Tab, or a plain activation with -Activate), then reports which HWND has keyboard
# focus and optionally sends chords. Keys are only sent while the test window is foreground.
#   powershell -File scripts/dev/focus-probe.ps1 [-Activate] [-Chord "ctrl+l"] [-NoAway]
#   -Stay leaves the test window in front (for a follow-up CDP check that needs it focused).
#   -FocusTab first puts the keyboard focus into the active page (needs the CDP port of run.ps1).
#   -Minimize leaves by minimizing the test window (like Win+D) and comes back by restoring it.
param([string[]]$Chord = @(), [switch]$Activate, [switch]$NoAway, [switch]$Stay, [switch]$FocusTab, [switch]$Minimize)
$pidFile = Join-Path $env:TEMP 'zenith-dev\test-instance.pid'
$zpid = [int](Get-Content $pidFile)
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class ZFocus {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern void SwitchToThisWindow(IntPtr h, bool altTab);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool attach);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int l, t, r, b; }
  [StructLayout(LayoutKind.Sequential)] public struct GUITHREADINFO {
    public int cbSize; public int flags; public IntPtr hwndActive; public IntPtr hwndFocus;
    public IntPtr hwndCapture; public IntPtr hwndMenuOwner; public IntPtr hwndMoveSize;
    public IntPtr hwndCaret; public RECT rcCaret;
  }
  [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint tid, ref GUITHREADINFO info);
  public static IntPtr MainOf(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint owner; GetWindowThreadProcessId(h, out owner);
      if (owner == pid && IsWindowVisible(h)) {
        var c = new StringBuilder(256); GetClassName(h, c, 256);
        if (c.ToString().StartsWith("Tauri")) { found = h; return false; }
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static string Describe(IntPtr h) {
    if (h == IntPtr.Zero) return "<none>";
    var c = new StringBuilder(256); GetClassName(h, c, 256);
    uint p; GetWindowThreadProcessId(h, out p);
    return h.ToString("X") + " [" + c + "] pid=" + p;
  }
  public static void Front(IntPtr h) {
    uint dummy;
    uint fg = GetWindowThreadProcessId(GetForegroundWindow(), out dummy);
    uint me = GetCurrentThreadId();
    AttachThreadInput(me, fg, true);
    SetForegroundWindow(h);
    AttachThreadInput(me, fg, false);
    // Foreground lock (another app is active): switch the way Alt+Tab does.
    if (GetForegroundWindow() != h) { SwitchToThisWindow(h, true); System.Threading.Thread.Sleep(300); }
  }
}
"@
$h = [ZFocus]::MainOf([uint32]$zpid)
if ($h -eq [IntPtr]::Zero) { Write-Output 'no test window'; exit 3 }
$dummy = 0
$tid = [ZFocus]::GetWindowThreadProcessId($h, [ref]$dummy)

function Report($label) {
  $info = New-Object ZFocus+GUITHREADINFO
  $info.cbSize = [Runtime.InteropServices.Marshal]::SizeOf($info)
  [ZFocus]::GetGUIThreadInfo($tid, [ref]$info) | Out-Null
  $fg = [ZFocus]::GetForegroundWindow()
  Write-Output "$label  foreground=$([ZFocus]::Describe($fg)) focus=$([ZFocus]::Describe($info.hwndFocus))"
}
function Send($c) {
  $vk = @{ ctrl = 0x11; shift = 0x10; alt = 0x12; escape = 0x1B; tab = 0x09 }
  $codes = foreach ($k in $c.ToLower().Split('+')) {
    if ($vk.ContainsKey($k)) { $vk[$k] } else { [int][char]$k.ToUpper() }
  }
  foreach ($code in $codes) { [ZFocus]::keybd_event([byte]$code, 0, 0, [UIntPtr]::Zero) }
  [array]::Reverse($codes)
  foreach ($code in $codes) { [ZFocus]::keybd_event([byte]$code, 0, 2, [UIntPtr]::Zero) }
}

# Handed back at the end so the user's own window is in front again.
$original = [ZFocus]::GetForegroundWindow()
function Finish($code) {
  if ($original -ne [IntPtr]::Zero -and $original -ne $h) { [ZFocus]::Front($original) }
  exit $code
}
[ZFocus]::Front($h); Start-Sleep -Milliseconds 400
if ($FocusTab) {
  $cdp = Join-Path $PSScriptRoot 'cdp.mjs'
  $expr = "(async()=>{const el=document.querySelector('[data-tab-id][aria-selected=true],[data-tab-id].active')||document.querySelector('[data-tab-id]');const id=el.dataset.tabId;await window.__TAURI_INTERNALS__.invoke('focus_view',{target:id});return id})()"
  Write-Output "focused tab $(node $cdp eval $expr)"
  # node may have surfaced a console window: make the test window the latest one again.
  [ZFocus]::Front($h); Start-Sleep -Milliseconds 400
}
Report 'start'
if (-not $NoAway) {
  $form = New-Object System.Windows.Forms.Form
  $form.Text = 'zenith focus probe'; $form.Width = 300; $form.Height = 120
  $form.Show(); [ZFocus]::Front($form.Handle); [System.Windows.Forms.Application]::DoEvents()
  Start-Sleep -Milliseconds 400; [System.Windows.Forms.Application]::DoEvents()
  if ([ZFocus]::GetForegroundWindow() -ne $form.Handle) { Write-Output 'probe window not foreground'; $form.Close(); Finish 2 }
  if ($Minimize) {
    [ZFocus]::ShowWindow($h, 6) | Out-Null   # SW_MINIMIZE
    for ($i = 0; $i -lt 5; $i++) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 80 }
  }
  Report 'away '
  if ($Minimize) {
    # Restore the way the taskbar button does: activate + restore.
    [ZFocus]::ShowWindow($h, 9) | Out-Null   # SW_RESTORE
    [ZFocus]::Front($h)
  } elseif ($Activate) {
    [ZFocus]::Front($h)
  } else {
    # Real Alt+Tab from the probe window: lands on the previous window, the test instance.
    [ZFocus]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    [ZFocus]::keybd_event(0x09, 0, 0, [UIntPtr]::Zero)
    [ZFocus]::keybd_event(0x09, 0, 2, [UIntPtr]::Zero)
    [ZFocus]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
  }
  for ($i = 0; $i -lt 10; $i++) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 80 }
  Report 'back '
  $form.Close()
}
foreach ($c in $Chord) {
  if ([ZFocus]::GetForegroundWindow() -ne $h) { Write-Output "skip $c (test window not foreground)"; Finish 2 }
  Send $c; Start-Sleep -Milliseconds 400
  Write-Output "sent $c"
}
Report 'end  '
if (-not $Stay) { Finish 0 }
