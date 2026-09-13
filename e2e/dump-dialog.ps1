# Waits for the app's file dialog, dumps its UIA tree, then closes it.
param(
  [int]$TimeoutSec = 40,
  [string]$OutFile = "$env:TEMP\opencode\dialog-dump.txt"
)
$ErrorActionPreference = 'Stop'

Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class DlgWin2 {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr w, IntPtr l);
  public static List<IntPtr> FindDialogs() {
    var found = new List<IntPtr>();
    EnumWindows((h, l) => {
      if (!IsWindowVisible(h)) return true;
      var cls = new StringBuilder(256);
      GetClassName(h, cls, 256);
      if (cls.ToString() == "#32770") {
        uint pid; GetWindowThreadProcessId(h, out pid);
        try {
          var p = System.Diagnostics.Process.GetProcessById((int)pid);
          var n = p.ProcessName.ToLower();
          if (n.Contains("parquet") || n.Contains("msedgewebview2")) found.Add(h);
        } catch {}
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

$deadline = (Get-Date).AddSeconds($TimeoutSec)
$hwnd = [IntPtr]::Zero
while ((Get-Date) -lt $deadline -and $hwnd -eq [IntPtr]::Zero) {
  $list = [DlgWin2]::FindDialogs()
  if ($list.Count -gt 0) { $hwnd = $list[0] }
  else { Start-Sleep -Milliseconds 200 }
}
if ($hwnd -eq [IntPtr]::Zero) { Write-Error 'dialog not found'; exit 1 }

$root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
$lines = New-Object System.Collections.Generic.List[string]
$walker = [System.Windows.Automation.TreeWalker]::RawViewWalker

function Walk($el, $depth, $count) {
  if (!$el -or $depth -gt 7 -or $count.Value -gt 500) { return }
  $count.Value++
  $c = $el.Current
  $indent = ' ' * ($depth * 2)
  $ct = $c.ControlType.ProgrammaticName -replace 'ControlType\.', ''
  $lines.Add("$indent[$ct] id='$($c.AutomationId)' name='$($c.Name)' enabled=$($c.IsEnabled)")
  $child = $walker.GetFirstChild($el)
  while ($child) {
    Walk $child ($depth + 1) $count
    $child = $walker.GetNextSibling($child)
  }
}

Walk $root 0 ([ref]0)
$lines | Set-Content -LiteralPath $OutFile -Encoding UTF8
Write-Output ("dumped " + $lines.Count + " nodes to " + $OutFile)

[DlgWin2]::PostMessage($hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Write-Output 'closed dialog via WM_CLOSE'
exit 0
