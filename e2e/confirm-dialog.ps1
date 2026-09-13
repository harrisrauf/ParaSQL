# Find a native #32770 confirm/message dialog owned by the app and accept or dismiss it.
# The tauri-plugin-dialog init script rewrites window.confirm() into an IPC call that
# renders a native OS dialog, so Playwright cannot see it; this helper drives it.
# Usage: powershell -File confirm-dialog.ps1 -Action accept|dismiss [-TimeoutSec 20]
# Prints "CONFIRM_FOUND title=... text=..." then CONFIRM_ACCEPTED|CONFIRM_DISMISSED, exit 0.
# Prints CONFIRM_NONE and exits 3 when no dialog appears before the timeout.
param(
  [ValidateSet('accept', 'dismiss')][string]$Action = 'accept',
  [int]$TimeoutSec = 20
)
$ErrorActionPreference = 'Continue'
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class CfdDlg {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
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
  public static string ClassOf(IntPtr h) {
    var sb = new StringBuilder(256);
    GetClassName(h, sb, 256);
    return sb.ToString();
  }
  public static string TextOf(IntPtr h) {
    var sb = new StringBuilder(1024);
    GetWindowText(h, sb, 1024);
    return sb.ToString();
  }
  public static List<IntPtr> Find() {
    var found = new List<IntPtr>();
    EnumWindows((h, l) => {
      if (!IsWindowVisible(h)) return true;
      if (ClassOf(h) != "#32770") return true;
      uint pid; GetWindowThreadProcessId(h, out pid);
      try {
        var n = System.Diagnostics.Process.GetProcessById((int)pid).ProcessName.ToLower();
        if (n.Contains("parquet") || n.Contains("msedgewebview2")) found.Add(h);
      } catch {}
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static List<IntPtr> Children(IntPtr parent) {
    var list = new List<IntPtr>();
    EnumChildWindows(parent, (h, l) => { list.Add(h); return true; }, IntPtr.Zero);
    return list;
  }
  public static void ClickButton(IntPtr h) {
    SendMessage(h, 0x00F5, IntPtr.Zero, IntPtr.Zero); // BM_CLICK
  }
}
"@
Add-Type -AssemblyName System.Windows.Forms, UIAutomationClient, UIAutomationTypes

function Focus-Dlg($h) {
  try {
    $targetPid = 0
    $target = [CfdDlg]::GetWindowThreadProcessId($h, [ref]$targetPid)
    [CfdDlg]::AttachThreadInput([CfdDlg]::GetCurrentThreadId(), $target, $true) | Out-Null
    [CfdDlg]::ShowWindow($h, 9) | Out-Null
    [CfdDlg]::SetForegroundWindow($h) | Out-Null
    [CfdDlg]::AttachThreadInput([CfdDlg]::GetCurrentThreadId(), $target, $false) | Out-Null
  } catch {}
  Start-Sleep -Milliseconds 350
}

# Collect dialog title + visible text/button labels via UIA (best effort; for evidence).
function Get-DialogText($h) {
  $parts = New-Object System.Collections.Generic.List[string]
  $t = [CfdDlg]::TextOf($h)
  if ($t) { [void]$parts.Add($t) }
  try {
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($h)
    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    $stack = New-Object System.Collections.Stack
    $stack.Push($root)
    $n = 0
    while ($stack.Count -gt 0 -and $n -lt 800) {
      $node = $stack.Pop(); $n++
      try {
        $nm = $node.Current.Name
        $ct = $node.Current.ControlType.ProgrammaticName
        if ($nm -and ($ct -eq 'ControlType.Text' -or $ct -eq 'ControlType.Button')) {
          if (-not $parts.Contains($nm)) { [void]$parts.Add($nm) }
        }
      } catch {}
      try { $c = $walker.GetFirstChild($node); while ($c) { $stack.Push($c); $c = $walker.GetNextSibling($c) } } catch {}
    }
  } catch {}
  ($parts | Where-Object { $_ }) -join ' | '
}

function Find-ButtonNode($h, $ids, $patterns) {
  try {
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($h)
    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    $stack = New-Object System.Collections.Stack
    $stack.Push($root)
    $n = 0
    while ($stack.Count -gt 0 -and $n -lt 800) {
      $node = $stack.Pop(); $n++
      try {
        $id = $node.Current.AutomationId
        $nm = $node.Current.Name
        $isBtn = $node.Current.ControlType.ProgrammaticName -eq 'ControlType.Button'
        if ($isBtn) {
          if ($ids -contains $id) { return $node }
          if ($nm) {
            foreach ($p in $patterns) { if ($nm -match $p) { return $node } }
          }
        }
      } catch {}
      try { $c = $walker.GetFirstChild($node); while ($c) { $stack.Push($c); $c = $walker.GetNextSibling($c) } } catch {}
    }
  } catch {}
  return $null
}

function Wait-Close([IntPtr]$h, [int]$ms) {
  $deadline = (Get-Date).AddMilliseconds($ms)
  while ((Get-Date) -lt $deadline) {
    if (-not [CfdDlg]::IsWindow($h)) { return $true }
    Start-Sleep -Milliseconds 100
  }
  return -not [CfdDlg]::IsWindow($h)
}

$deadline = (Get-Date).AddSeconds($TimeoutSec)
$h = [IntPtr]::Zero
while ((Get-Date) -lt $deadline) {
  $list = [CfdDlg]::Find()
  if ($list.Count -gt 0) { $h = $list[0]; break }
  Start-Sleep -Milliseconds 150
}
if ($h -eq [IntPtr]::Zero) {
  Write-Output 'CONFIRM_NONE'
  exit 3
}

Start-Sleep -Milliseconds 350
$title = ([CfdDlg]::TextOf($h) -replace '\s+', ' ').Trim()
$text = Get-DialogText $h
Write-Output "CONFIRM_FOUND title=""$title"" text=""$text"""

if ($Action -eq 'accept') {
  $ids = @('6', '1')
  $names = @('^&?Yes$', '^&?OK$', '^&?确定$', '^&?是$')
  $key = '{ENTER}'
} else {
  $ids = @('7', '2')
  $names = @('^&?No$', '^&?Cancel$', '^&?取消$', '^&?否$')
  $key = '{ESC}'
}

Focus-Dlg $h

$node = $null
for ($i = 0; $i -lt 4 -and -not $node; $i++) {
  $node = Find-ButtonNode $h $ids $names
  if (-not $node) { Start-Sleep -Milliseconds 250 }
}
if ($node) {
  try {
    $node.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
  } catch {
    try {
      $r = $node.Current.BoundingRectangle
      if ($r.Width -gt 0) {
        [CfdDlg]::Click([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2))
      }
    } catch {}
  }
}

$closed = Wait-Close $h 2500
if (-not $closed) {
  # Fallback: click the matching native Button child directly.
  $plain = if ($Action -eq 'accept') { @('yes', 'ok', '确定', '是') } else { @('no', 'cancel', '取消', '否') }
  foreach ($c in [CfdDlg]::Children($h)) {
    if ([CfdDlg]::ClassOf($c) -ne 'Button') { continue }
    $t = ([CfdDlg]::TextOf($c) -replace '&', '').Trim().ToLower()
    if ($plain -contains $t) { [CfdDlg]::ClickButton($c) | Out-Null; break }
  }
  $closed = Wait-Close $h 2000
}
if (-not $closed) {
  Focus-Dlg $h
  [System.Windows.Forms.SendKeys]::SendWait($key)
  $closed = Wait-Close $h 1500
}

if ($closed) {
  if ($Action -eq 'accept') { Write-Output 'CONFIRM_ACCEPTED' } else { Write-Output 'CONFIRM_DISMISSED' }
  exit 0
}
Write-Output 'CONFIRM_STUCK'
exit 4
