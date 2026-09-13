param([Parameter(Mandatory = $true)][string]$FilePath)
# Experiment 3: set the folder dialog's Folder: edit via WM_SETTEXT, navigate, confirm.
$ErrorActionPreference = 'Continue'
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class EDlg3 {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageW")] public static extern IntPtr SendMessageText(IntPtr h, uint msg, IntPtr w, string l);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool f);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
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
Add-Type -AssemblyName System.Windows.Forms

$hwnd = $null
$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline -and -not $hwnd) {
  $l = [EDlg3]::Find(); if ($l.Count -gt 0) { $hwnd = $l[0] }
  if (-not $hwnd) { Start-Sleep -Milliseconds 200 }
}
if (-not $hwnd) { Write-Output 'no dialog'; exit 1 }

try {
  $pid = 0; $tt = [EDlg3]::GetWindowThreadProcessId($hwnd, [ref]$pid); $ct = [EDlg3]::GetCurrentThreadId()
  [EDlg3]::AttachThreadInput($ct, $tt, $true) | Out-Null
  [EDlg3]::ShowWindow($hwnd, 9) | Out-Null
  [EDlg3]::SetForegroundWindow($hwnd) | Out-Null
  [EDlg3]::AttachThreadInput($ct, $tt, $false) | Out-Null
} catch {}
Start-Sleep -Milliseconds 400

$edit = [EDlg3]::FindWindowEx($hwnd, [IntPtr]::Zero, 'Edit', $null)
Write-Output ("folder edit hwnd=" + $edit)
if ($edit -eq [IntPtr]::Zero) { Write-Output 'NO EDIT'; exit 3 }
# WM_SETTEXT = 0x000C
[void][EDlg3]::SendMessageText($edit, 0x000C, [IntPtr]::Zero, $FilePath)
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
Start-Sleep -Milliseconds 2000
if (-not [EDlg3]::IsWindow($hwnd)) { Write-Output 'CLOSED after settext+ENTER'; exit 0 }

[System.Windows.Forms.SendKeys]::SendWait('%s')
Start-Sleep -Milliseconds 1500
if (-not [EDlg3]::IsWindow($hwnd)) { Write-Output 'CLOSED after %s'; exit 0 }

[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
Start-Sleep -Milliseconds 1200
[System.Windows.Forms.SendKeys]::SendWait('%s')
Start-Sleep -Milliseconds 1500
if (-not [EDlg3]::IsWindow($hwnd)) { Write-Output 'CLOSED after ENTER+%s'; exit 0 }

Write-Output 'STILL OPEN'
exit 2
