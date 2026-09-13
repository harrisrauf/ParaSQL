param([Parameter(Mandatory = $true)][string]$FilePath)
# Experiment 6: paste path into the focused Folder: edit, then click Select Folder
# via UIA patterns, falling back to a real mouse click at the button's center.
$ErrorActionPreference = 'Continue'
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class EDlg6 {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
  public static List<IntPtr> Find() {
    var found = new List<IntPtr>();
    EnumWindows((h, l) => {
      if (!IsWindowVisible(h)) return true;
      var cls = new StringBuilder(256); GetClassName(h, cls, 256);
      if (cls.ToString() == "#32770") {
        uint pid; GetWindowThreadProcessId(h, out pid);
        try { var n = System.Diagnostics.Process.GetProcessById((int)pid).ProcessName.ToLower();
          if (n.Contains("parquet") || n.Contains("msedgewebview2")) found.Add(h); } catch {}
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static void Click(int x, int y) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(120);
    mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero);
    System.Threading.Thread.Sleep(60);
    mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero);
  }
}
"@
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Windows.Forms

$hwnd = $null
$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline -and -not $hwnd) {
  $l = [EDlg6]::Find(); if ($l.Count -gt 0) { $hwnd = $l[0] }
  if (-not $hwnd) { Start-Sleep -Milliseconds 200 }
}
if (-not $hwnd) { Write-Output 'no dialog'; exit 1 }
Start-Sleep -Milliseconds 800
[EDlg6]::ShowWindow($hwnd, 9) | Out-Null
[EDlg6]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 300

$root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
$stack = New-Object System.Collections.Stack
$stack.Push($root)
$editNode = $null; $btnNode = $null
$count = 0
while ($stack.Count -gt 0 -and $count -lt 800) {
  $node = $stack.Pop(); $count++
  try {
    $id = $node.Current.AutomationId
    if ($id -eq '1152' -and -not $editNode) { $editNode = $node }
    if ($id -eq '1' -and $node.Current.ClassName -eq 'Button' -and -not $btnNode) { $btnNode = $node }
  } catch {}
  try { $c = $walker.GetFirstChild($node); while ($c) { $stack.Push($c); $c = $walker.GetNextSibling($c) } } catch {}
}
if (-not $editNode) { Write-Output 'no 1152 node'; exit 3 }
try { $editNode.SetFocus() } catch {}
Start-Sleep -Milliseconds 300
try { Set-Clipboard -Value $FilePath } catch {}
[System.Windows.Forms.SendKeys]::SendWait('^a')
Start-Sleep -Milliseconds 120
[System.Windows.Forms.SendKeys]::SendWait('^v')
Start-Sleep -Milliseconds 400
try {
  $vp = $null
  if ($editNode.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$vp)) {
    Write-Output ("1152 value: '" + $vp.Current.Value + "'")
  }
} catch {}

if (-not $btnNode) { Write-Output 'no Select Folder button node'; exit 4 }
$rect = $btnNode.Current.BoundingRectangle
Write-Output ('button rect: ' + $rect.ToString())
$invoked = $false
try {
  $p = $null
  if ($btnNode.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) { $p.Invoke(); $invoked = 'invoke' }
} catch {}
if (-not $invoked) {
  try {
    $lp = $null
    if ($btnNode.TryGetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern, [ref]$lp)) { $lp.DoDefaultAction(); $invoked = 'legacy' }
  } catch {}
}
if (-not $invoked) {
  $cx = [int]($rect.X + $rect.Width / 2); $cy = [int]($rect.Y + $rect.Height / 2)
  Write-Output ("mouse click at $cx,$cy")
  [EDlg6]::Click($cx, $cy)
  $invoked = 'mouse'
}
Write-Output ("select via: " + $invoked)
Start-Sleep -Milliseconds 1800
if (-not [EDlg6]::IsWindow($hwnd)) { Write-Output 'CLOSED'; exit 0 }

$attempts = 0
while ([EDlg6]::IsWindow($hwnd) -and $attempts -lt 6) {
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
  Start-Sleep -Milliseconds 1100
  $attempts++
}
if (-not [EDlg6]::IsWindow($hwnd)) { Write-Output 'CLOSED in hammer'; exit 0 }
Write-Output 'STILL OPEN'
exit 2
