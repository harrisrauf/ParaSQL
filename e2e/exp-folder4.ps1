param([Parameter(Mandatory = $true)][string]$FilePath)
# Experiment 4: enumerate Edit children of the folder dialog, target the real one.
$ErrorActionPreference = 'Continue'
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class EDlg4 {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageW")] public static extern IntPtr SendMessageText(IntPtr h, uint msg, IntPtr w, string l);
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
  public static List<IntPtr> Children(IntPtr parent) {
    var found = new List<IntPtr>();
    EnumChildWindows(parent, (h, l) => { found.Add(h); return true; }, IntPtr.Zero);
    return found;
  }
}
"@
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Windows.Forms

$hwnd = $null
$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline -and -not $hwnd) {
  $l = [EDlg4]::Find(); if ($l.Count -gt 0) { $hwnd = $l[0] }
  if (-not $hwnd) { Start-Sleep -Milliseconds 200 }
}
if (-not $hwnd) { Write-Output 'no dialog'; exit 1 }
Start-Sleep -Milliseconds 800
[EDlg4]::ShowWindow($hwnd, 9) | Out-Null
[EDlg4]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 300

$target = [IntPtr]::Zero
foreach ($h in [EDlg4]::Children($hwnd)) {
  $cls = New-Object System.Text.StringBuilder 256
  [void][EDlg4]::GetClassName($h, $cls, 256)
  if ($cls.ToString() -ne 'Edit') { continue }
  $txt = New-Object System.Text.StringBuilder 1024
  [void][EDlg4]::GetWindowText($h, $txt, 1024)
  $uia = ''
  try {
    $el = [System.Windows.Automation.AutomationElement]::FromHandle($h)
    $uia = 'uiaid=' + $el.Current.AutomationId + ' name=' + $el.Current.Name
  } catch {}
  Write-Output ("edit hwnd=$h text='" + $txt.ToString() + "' $uia")
  if ($uia -match "uiaid=1152 ") { $target = $h }
}
if ($target -eq [IntPtr]::Zero) { Write-Output 'no 1152 edit found (using none)'; exit 3 }
[void][EDlg4]::SendMessageText($target, 0x000C, [IntPtr]::Zero, $FilePath)
Start-Sleep -Milliseconds 400
$sb = New-Object System.Text.StringBuilder 1024
[void][EDlg4]::GetWindowText($target, $sb, 1024)
Write-Output ("after set: '" + $sb.ToString() + "'")

$attempts = 0
while ([EDlg4]::IsWindow($hwnd) -and $attempts -lt 10) {
  if ($attempts % 2 -eq 0) { [System.Windows.Forms.SendKeys]::SendWait('{ENTER}') }
  else { [System.Windows.Forms.SendKeys]::SendWait('%s') }
  Start-Sleep -Milliseconds 1100
  $attempts++
}
if (-not [EDlg4]::IsWindow($hwnd)) { Write-Output 'CLOSED'; exit 0 }
Write-Output 'STILL OPEN'
exit 2
