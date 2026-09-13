param([Parameter(Mandatory = $true)][string]$FilePath)
# Experiment 2: navigate the Select Folder picker via the address bar (Alt+D).
$ErrorActionPreference = 'Continue'
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class EDlg2 {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
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
Add-Type -AssemblyName System.Windows.Forms, Microsoft.VisualBasic

$hwnd = $null
$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline -and -not $hwnd) {
  $l = [EDlg2]::Find(); if ($l.Count -gt 0) { $hwnd = $l[0] }
  if (-not $hwnd) { Start-Sleep -Milliseconds 200 }
}
if (-not $hwnd) { Write-Output 'no dialog'; exit 1 }

# focus
try {
  $pid = 0; $tt = [EDlg2]::GetWindowThreadProcessId($hwnd, [ref]$pid); $ct = [EDlg2]::GetCurrentThreadId()
  [EDlg2]::AttachThreadInput($ct, $tt, $true) | Out-Null
  [EDlg2]::ShowWindow($hwnd, 9) | Out-Null
  [EDlg2]::SetForegroundWindow($hwnd) | Out-Null
  [EDlg2]::AttachThreadInput($ct, $tt, $false) | Out-Null
} catch {}
Start-Sleep -Milliseconds 400

try { Set-Clipboard -Value $FilePath } catch {}
Write-Output 'address bar (Alt+D) + paste'
[System.Windows.Forms.SendKeys]::SendWait('%d')
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait('^a')
Start-Sleep -Milliseconds 150
[System.Windows.Forms.SendKeys]::SendWait('^v')
Start-Sleep -Milliseconds 500
[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
Start-Sleep -Milliseconds 2000
if (-not [EDlg2]::IsWindow($hwnd)) { Write-Output 'CLOSED after address+ENTER'; exit 0 }

Write-Output 'confirm with Alt+S'
[System.Windows.Forms.SendKeys]::SendWait('%s')
Start-Sleep -Milliseconds 1500
if (-not [EDlg2]::IsWindow($hwnd)) { Write-Output 'CLOSED after %s'; exit 0 }

Write-Output 'second ENTER then Alt+S'
[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
Start-Sleep -Milliseconds 1200
[System.Windows.Forms.SendKeys]::SendWait('%s')
Start-Sleep -Milliseconds 1500
if (-not [EDlg2]::IsWindow($hwnd)) { Write-Output 'CLOSED after ENTER+%s'; exit 0 }

Write-Output 'STILL OPEN'
exit 2
