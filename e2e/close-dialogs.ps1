# Close any native #32770 dialogs owned by the app (cleanup after failed dialog tests).
# Strategy per dialog: ESC -> mouse-click Cancel -> Alt+F4, re-checking after each.
param([int]$SettleMs = 250)
$ErrorActionPreference = 'Continue'
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class EscDlg {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool f);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
  public static void Click(int x, int y) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(120);
    mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero);
    System.Threading.Thread.Sleep(60);
    mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero);
  }
  public static List<IntPtr> Find() {
    var found = new List<IntPtr>();
    EnumWindows((h, l) => {
      if (!IsWindowVisible(h)) return true;
      var cls = new StringBuilder(256);
      GetClassName(h, cls, 256);
      if (cls.ToString() == "#32770") {
        uint pid; GetWindowThreadProcessId(h, out pid);
        try {
          var n = System.Diagnostics.Process.GetProcessById((int)pid).ProcessName.ToLower();
          if (n.Contains("parquet") || n.Contains("msedgewebview2")) found.Add(h);
        } catch {}
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@
Add-Type -AssemblyName System.Windows.Forms, UIAutomationClient, UIAutomationTypes

function Focus-Dlg($h) {
  try {
    $targetPid = 0
    $target = [EscDlg]::GetWindowThreadProcessId($h, [ref]$targetPid)
    [EscDlg]::AttachThreadInput([EscDlg]::GetCurrentThreadId(), $target, $true) | Out-Null
    [EscDlg]::ShowWindow($h, 9) | Out-Null
    [EscDlg]::SetForegroundWindow($h) | Out-Null
    [EscDlg]::AttachThreadInput([EscDlg]::GetCurrentThreadId(), $target, $false) | Out-Null
  } catch {}
  Start-Sleep -Milliseconds $SettleMs
}

function Find-CancelButton($h) {
  try {
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($h)
    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    $stack = New-Object System.Collections.Stack
    $stack.Push($root)
    $n = 0
    while ($stack.Count -gt 0 -and $n -lt 500) {
      $node = $stack.Pop(); $n++
      try {
        if ($node.Current.AutomationId -eq '2' -and $node.Current.ClassName -eq 'Button') { return $node }
      } catch {}
      try { $c = $walker.GetFirstChild($node); while ($c) { $stack.Push($c); $c = $walker.GetNextSibling($c) } } catch {}
    }
  } catch {}
  return $null
}

$closed = 0
for ($round = 0; $round -lt 5; $round++) {
  $list = [EscDlg]::Find()
  if ($list.Count -eq 0) { break }
  $progress = $false
  foreach ($h in $list) {
    if (-not [EscDlg]::IsWindow($h)) { continue }
    Focus-Dlg $h
    [System.Windows.Forms.SendKeys]::SendWait('{ESC}')
    Start-Sleep -Milliseconds 300
    if ([EscDlg]::IsWindow($h)) {
      $btn = Find-CancelButton $h
      if ($btn) {
        $r = $btn.Current.BoundingRectangle
        $cx = [int]($r.X + $r.Width / 2); $cy = [int]($r.Y + $r.Height / 2)
        [EscDlg]::Click($cx, $cy)
        Start-Sleep -Milliseconds 350
      }
    }
    if ([EscDlg]::IsWindow($h)) {
      Focus-Dlg $h
      [System.Windows.Forms.SendKeys]::SendWait('%{F4}')
      Start-Sleep -Milliseconds 350
    }
    if (-not [EscDlg]::IsWindow($h)) { $closed++; $progress = $true }
  }
  if (-not $progress) { break }
}
Write-Output "escaped $closed dialog(s)"
