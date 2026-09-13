param([Parameter(Mandatory = $true)][string]$FilePath)
# Experiment: figure out the working sequence for the Select Folder picker.
$ErrorActionPreference = 'Continue'
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class EDlg {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
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

function SetEdit1152([IntPtr]$hwnd, [string]$path) {
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
  $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
  $stack = New-Object System.Collections.Stack
  $stack.Push($root); $count = 0
  while ($stack.Count -gt 0 -and $count -lt 800) {
    $node = $stack.Pop(); $count++
    try {
      $isEdit = ($node.Current.ControlType -eq [System.Windows.Automation.ControlType]::Edit) -or ($node.Current.ClassName -eq 'Edit')
      if ($isEdit -and $node.Current.AutomationId -eq '1152') {
        $vp = $null
        if ($node.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$vp)) { $vp.SetValue($path); return $true }
      }
    } catch {}
    try { $c = $walker.GetFirstChild($node); while ($c) { $stack.Push($c); $c = $walker.GetNextSibling($c) } } catch {}
  }
  return $false
}

$hwnd = $null
$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline -and -not $hwnd) {
  $l = [EDlg]::Find(); if ($l.Count -gt 0) { $hwnd = $l[0] }
  if (-not $hwnd) { Start-Sleep -Milliseconds 200 }
}
if (-not $hwnd) { Write-Output 'no dialog'; exit 1 }

Write-Output ('set1152: ' + (SetEdit1152 $hwnd $FilePath))
Start-Sleep -Milliseconds 500
Write-Output 'sending %s'
[System.Windows.Forms.SendKeys]::SendWait('%s')
Start-Sleep -Milliseconds 1500
if (-not [EDlg]::IsWindow($hwnd)) { Write-Output 'CLOSED after %s'; exit 0 }
Write-Output 'still open; sending ENTER then %s'
[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
Start-Sleep -Milliseconds 1500
[System.Windows.Forms.SendKeys]::SendWait('%s')
Start-Sleep -Milliseconds 1500
if (-not [EDlg]::IsWindow($hwnd)) { Write-Output 'CLOSED after Enter+%s'; exit 0 }
Write-Output 'STILL OPEN'
exit 2
