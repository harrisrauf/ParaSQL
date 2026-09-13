param([int]$MaxNodes = 500)
# Dump the UIA control tree of the first visible file dialog (diagnostics only).
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public class DlgFind {
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lp);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public delegate bool EnumWindowsProc(IntPtr h, IntPtr lp);
  public static System.Collections.Generic.List<IntPtr> Found = new System.Collections.Generic.List<IntPtr>();
  public static bool Callback(IntPtr h, IntPtr lp) {
    if (!IsWindowVisible(h)) return true;
    var sb = new StringBuilder(64); GetClassName(h, sb, 64);
    if (sb.ToString() != "#32770") return true;
    uint pid; GetWindowThreadProcessId(h, out pid);
    try { var p = System.Diagnostics.Process.GetProcessById((int)pid).ProcessName.ToLowerInvariant();
      if (p.Contains("parquet") || p.Contains("msedgewebview2")) Found.Add(h);
    } catch {}
    return true;
  }
  public static System.Collections.Generic.List<IntPtr> Find() {
    Found = new System.Collections.Generic.List<IntPtr>();
    EnumWindows(Callback, IntPtr.Zero);
    return Found;
  }
}
'@
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

$hwnds = [DlgFind]::Find()
if ($hwnds.Count -eq 0) { Write-Output 'no dialog found'; exit 1 }
$hwnd = $hwnds[0]
Write-Output ("dialog hwnd=" + $hwnd)
$root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
$stack = New-Object System.Collections.Stack
$stack.Push(@($root, 0))
$n = 0
while ($stack.Count -gt 0 -and $n -lt $MaxNodes) {
  $item = $stack.Pop(); $el = $item[0]; $depth = $item[1]
  $n++
  $ct = '?'; $id = ''; $name = ''; $cls = ''; $en = $false; $val = ''
  try { $ct = $el.Current.ControlType.ProgrammaticName -replace 'ControlType\.', '' } catch {}
  try { $id = $el.Current.AutomationId } catch {}
  try { $name = $el.Current.Name } catch {}
  try { $cls = $el.Current.ClassName } catch {}
  try { $en = $el.Current.IsEnabled } catch {}
  try {
    $vp = $null
    if ($el.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$vp)) { $val = $vp.Current.Value }
  } catch {}
  Write-Output ("{0}{1} id='{2}' cls='{3}' en={4} name='{5}' val='{6}'" -f ('  ' * $depth), $ct, $id, $cls, $en, $name, ($val -replace "`r?`n", ' '))
  try {
    $c = $walker.GetFirstChild($el)
    while ($c) { $stack.Push(@($c, [Math]::Min($depth + 1, 12))); $c = $walker.GetNextSibling($c) }
  } catch {}
}
Write-Output ("nodes=" + $n)
exit 0
