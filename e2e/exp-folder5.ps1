param([Parameter(Mandatory = $true)][string]$FilePath)
# Experiment 5: focus the Folder: edit via UIA, paste, Enter.
$ErrorActionPreference = 'Continue'
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class EDlg5 {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
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
}
"@
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Windows.Forms

$hwnd = $null
$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline -and -not $hwnd) {
  $l = [EDlg5]::Find(); if ($l.Count -gt 0) { $hwnd = $l[0] }
  if (-not $hwnd) { Start-Sleep -Milliseconds 200 }
}
if (-not $hwnd) { Write-Output 'no dialog'; exit 1 }
Start-Sleep -Milliseconds 800
[EDlg5]::ShowWindow($hwnd, 9) | Out-Null
[EDlg5]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 300

function Show-Focus($tag) {
  try {
    $f = [System.Windows.Automation.AutomationElement]::FocusedElement
    Write-Output ("$tag focus: type=" + $f.Current.ControlType.ProgrammaticName + " id=" + $f.Current.AutomationId + " cls=" + $f.Current.ClassName + " name=" + $f.Current.Name)
  } catch { Write-Output "$tag focus: <err>" }
}
Show-Focus 'before'

$root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
$stack = New-Object System.Collections.Stack
$stack.Push($root)
$target = $null
$count = 0
while ($stack.Count -gt 0 -and $count -lt 800) {
  $node = $stack.Pop(); $count++
  try { if ($node.Current.AutomationId -eq '1152') { $target = $node; break } } catch {}
  try { $c = $walker.GetFirstChild($node); while ($c) { $stack.Push($c); $c = $walker.GetNextSibling($c) } } catch {}
}
if (-not $target) { Write-Output 'no 1152 node'; exit 3 }
Write-Output '1152 found; SetFocus'
try { $target.SetFocus() } catch { Write-Output ('SetFocus err: ' + $_.Exception.Message) }
Start-Sleep -Milliseconds 500
Show-Focus 'after SetFocus'
try { Set-Clipboard -Value $FilePath } catch {}
[System.Windows.Forms.SendKeys]::SendWait('^a')
Start-Sleep -Milliseconds 150
[System.Windows.Forms.SendKeys]::SendWait('^v')
Start-Sleep -Milliseconds 500

try {
  $vp = $null
  if ($target.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$vp)) {
    Write-Output ("1152 value now: '" + $vp.Current.Value + "'")
  } else { Write-Output '1152 no ValuePattern' }
} catch {}

[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
Start-Sleep -Milliseconds 1800
if (-not [EDlg5]::IsWindow($hwnd)) { Write-Output 'CLOSED after paste+ENTER'; exit 0 }

$attempts = 0
while ([EDlg5]::IsWindow($hwnd) -and $attempts -lt 10) {
  if ($attempts % 2 -eq 0) { [System.Windows.Forms.SendKeys]::SendWait('{ENTER}') }
  else { [System.Windows.Forms.SendKeys]::SendWait('%s') }
  Start-Sleep -Milliseconds 1100
  $attempts++
}
if (-not [EDlg5]::IsWindow($hwnd)) { Write-Output 'CLOSED in hammer'; exit 0 }
Write-Output 'STILL OPEN'
exit 2
