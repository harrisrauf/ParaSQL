# Fills and submits a native Windows file dialog (class #32770) owned by the app.
# Keyboard-based: focuses the dialog, Alt+N (File name), types the path, Enter.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File e2e/auto-dialog.ps1 -FilePath "C:\path\to\file" [-TimeoutSec 30]
param(
  [Parameter(Mandatory = $true)][string]$FilePath,
  [int]$TimeoutSec = 30
)
$ErrorActionPreference = 'Stop'

Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class DlgWin {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageW")] public static extern IntPtr SendMessageText(IntPtr h, uint msg, IntPtr w, string l);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
  public static void Click(int x, int y) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(120);
    mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero);
    System.Threading.Thread.Sleep(60);
    mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero);
  }
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
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
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Windows.Forms, Microsoft.VisualBasic

function Wait-Dialog([int]$seconds) {
  $deadline = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $deadline) {
    $list = [DlgWin]::FindDialogs()
    if ($list.Count -gt 0) { return $list[0] }
    Start-Sleep -Milliseconds 200
  }
  return [IntPtr]::Zero
}

function Focus-Dialog($hwnd) {
  if ([DlgWin]::GetForegroundWindow() -eq $hwnd) { return }
  try {
    $pid = 0
    $targetThread = [DlgWin]::GetWindowThreadProcessId($hwnd, [ref]$pid)
    $curThread = [DlgWin]::GetCurrentThreadId()
    [DlgWin]::AttachThreadInput($curThread, $targetThread, $true) | Out-Null
    [DlgWin]::ShowWindow($hwnd, 9) | Out-Null
    [DlgWin]::SetForegroundWindow($hwnd) | Out-Null
    [DlgWin]::AttachThreadInput($curThread, $targetThread, $false) | Out-Null
  } catch {
    try { [Microsoft.VisualBasic.Interaction]::AppActivate([System.Diagnostics.Process]::GetCurrentProcess().Id) } catch {}
  }
  Start-Sleep -Milliseconds 350
}

function Get-DialogTitle([IntPtr]$hwnd) {
  try {
    $sb = New-Object System.Text.StringBuilder 256
    [void][DlgWin]::GetWindowText($hwnd, $sb, 256)
    return $sb.ToString()
  } catch { return '' }
}

function Invoke-UiaButton($hwnd, $ids, $names) {
  try {
    $el = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
    if (-not $el) { return $false }
    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    $stack = New-Object System.Collections.Stack
    $stack.Push($el)
    $seen = 0
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($stack.Count -gt 0 -and $seen -lt 800 -and $sw.Elapsed.TotalSeconds -lt 4) {
      $node = $stack.Pop()
      $seen++
      $c = $node.Current
      if ($c.AutomationId -and ($ids -contains $c.AutomationId) -and $c.IsEnabled) {
        $p = $null
        if ($node.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) {
          $p.Invoke(); return $true
        }
      }
      if ($names -and $c.Name) {
        foreach ($n in $names) {
          if ($c.Name -match $n -and $c.IsEnabled) {
            $p = $null
            if ($node.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) {
              $p.Invoke(); return $true
            }
          }
        }
      }
      $child = $walker.GetFirstChild($node)
      while ($child) {
        $stack.Push($child)
        $child = $walker.GetNextSibling($child)
      }
    }
  } catch {}
  return $false
}

# Set the file-name edit via UIA ValuePattern (atomic, avoids SendKeys/autocomplete races).
function Set-FileNameValue([IntPtr]$hwnd, [string]$path) {
  try {
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
    if (-not $root) { return $false }
    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    $stack = New-Object System.Collections.Stack
    $stack.Push($root)
    $edits = New-Object System.Collections.ArrayList
    $count = 0
    while ($stack.Count -gt 0 -and $count -lt 800) {
      $node = $stack.Pop()
      $count++
      try {
        $isEdit = ($node.Current.ControlType -eq [System.Windows.Automation.ControlType]::Edit) -or ($node.Current.ClassName -eq 'Edit')
        if ($isEdit) {
          [void]$edits.Add($node)
        }
      } catch {}
      try {
        $child = $walker.GetFirstChild($node)
        while ($child) { $stack.Push($child); $child = $walker.GetNextSibling($child) }
      } catch {}
    }
    $chosen = $null
    # Win32 file dialog: id '1001' is the File name combo edit.
    foreach ($e in $edits) { if ($e.Current.AutomationId -eq '1001') { $chosen = $e; break } }
    # Folder picker: the "Folder:" edit is id '1152' class 'Edit' with no name.
    if (-not $chosen) {
      foreach ($e in $edits) { if ($e.Current.AutomationId -eq '1152') { $chosen = $e; break } }
    }
    if (-not $chosen) {
      foreach ($e in $edits) {
        try { if ($e.Current.ClassName -eq 'Edit' -and -not $e.Current.Name) { $chosen = $e; break } } catch {}
      }
    }
    if (-not $chosen) {
      foreach ($e in $edits) { if ($e.Current.Name -match 'File name') { $chosen = $e; break } }
    }
    if (-not $chosen) {
      foreach ($e in $edits) { if ($e.Current.Value -match '\.[A-Za-z0-9]{1,6}$') { $chosen = $e; break } }
    }
    if (-not $chosen -and $edits.Count -gt 0) { $chosen = $edits[$edits.Count - 1] }
    if (-not $chosen) { return $false }
    Write-Output ("uia edit: id='" + $chosen.Current.AutomationId + "' name='" + $chosen.Current.Name + "'")
    Start-Sleep -Milliseconds 50    $vp = $null
    if ($chosen.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$vp)) {
      $vp.SetValue($path)
      return $true
    }
  } catch {}
  return $false
}

$hwnd = Wait-Dialog $TimeoutSec
if ($hwnd -eq [IntPtr]::Zero) { Write-Error "dialog not found within $TimeoutSec s"; exit 1 }
Write-Output 'dialog found'
$title = Get-DialogTitle $hwnd
Write-Output ("dialog title: " + $title)
# Let the dialog finish initializing (address bar/folder load) before acting.
Start-Sleep -Milliseconds 800
$isFolder = $title -match 'Select Folder|Choose Folder|Browse For Folder'

Focus-Dialog $hwnd
if ($isFolder) {
  # Folder picker: the "Folder:" edit (UIA id 1152) is focused by default; paste
  # the path into it, then click "Select Folder" (id 1) at its real screen
  # position — the Pane-style button exposes no InvokePattern, so a mouse click
  # is the reliable path (running app is foreground at this point).
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
  $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
  $stack = New-Object System.Collections.Stack
  $stack.Push($root)
  $editNode = $null; $btnNode = $null; $count = 0
  while ($stack.Count -gt 0 -and $count -lt 800) {
    $node = $stack.Pop(); $count++
    try {
      $id = $node.Current.AutomationId
      if ($id -eq '1152' -and -not $editNode) { $editNode = $node }
      if ($id -eq '1' -and $node.Current.ClassName -eq 'Button' -and -not $btnNode) { $btnNode = $node }
    } catch {}
    try { $c = $walker.GetFirstChild($node); while ($c) { $stack.Push($c); $c = $walker.GetNextSibling($c) } } catch {}
  }
  try { Set-Clipboard -Value $FilePath } catch {}
  if ($editNode) {
    try { $editNode.SetFocus() } catch {}
    Start-Sleep -Milliseconds 250
    [System.Windows.Forms.SendKeys]::SendWait('^a')
    Start-Sleep -Milliseconds 120
    [System.Windows.Forms.SendKeys]::SendWait('^v')
    Start-Sleep -Milliseconds 350
    Write-Output 'folder path pasted into Folder: edit'
  } else {
    $edit = [DlgWin]::FindWindowEx($hwnd, [IntPtr]::Zero, 'Edit', $null)
    if ($edit -ne [IntPtr]::Zero) {
      [void][DlgWin]::SendMessageText($edit, 0x000C, [IntPtr]::Zero, $FilePath)
      Write-Output 'folder path set via WM_SETTEXT'
      Start-Sleep -Milliseconds 300
    }
  }
  if ($btnNode) {
    $rect = $btnNode.Current.BoundingRectangle
    $cx = [int]($rect.X + $rect.Width / 2); $cy = [int]($rect.Y + $rect.Height / 2)
    Write-Output ("clicking Select Folder at $cx,$cy")
    [DlgWin]::Click($cx, $cy)
    Start-Sleep -Milliseconds 1500
  }
  $attempts = 0
  while ([DlgWin]::IsWindow($hwnd) -and $attempts -lt 6) {
    [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
    Start-Sleep -Milliseconds 1100
    $attempts++
  }
} else {
  # Paste the full path with the clipboard (atomic; SendKeys typing races with the
  # file-name combo's autocomplete and can drop characters).
  try { Set-Clipboard -Value $FilePath } catch {}
  [System.Windows.Forms.SendKeys]::SendWait('%n')
  Start-Sleep -Milliseconds 300
  [System.Windows.Forms.SendKeys]::SendWait('^a')
  Start-Sleep -Milliseconds 100
  [System.Windows.Forms.SendKeys]::SendWait('^v')
  Start-Sleep -Milliseconds 400
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
}

# Wait for the dialog to close (up to 6s); if it stays, press Enter again
# (folder pickers navigate into the typed path on the first Enter, the second confirms).
$deadline = (Get-Date).AddSeconds(6)
$ok = $false
while ((Get-Date) -lt $deadline) {
  if (-not [DlgWin]::IsWindow($hwnd)) { $ok = $true; break }
  Start-Sleep -Milliseconds 200
}
if (-not $ok) {
  Write-Output 'dialog still open; trying second ENTER + UIA file name'
  Focus-Dialog $hwnd
  if (Set-FileNameValue $hwnd $FilePath) { Write-Output 'file name set via UIA fallback' }
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
  Start-Sleep -Milliseconds 800
}
if (-not [DlgWin]::IsWindow($hwnd)) { $ok = $true }
if (-not $ok) {
  Write-Output 'dialog still open; trying UIA buttons'
  if (Invoke-UiaButton $hwnd @('1', '6') @('^&?Yes', '^&?Open', '^&?Save', '^&?Select', '^&?Choose', '^&?OK')) { Start-Sleep -Milliseconds 800 }
  $deadline = (Get-Date).AddSeconds(5)
  while ((Get-Date) -lt $deadline) {
    if (-not [DlgWin]::IsWindow($hwnd)) { $ok = $true; break }
    Start-Sleep -Milliseconds 200
  }
}

# Overwrite confirmation dialog may have appeared instead (ignore unrelated
# dialogs: only handle one whose title looks like a confirmation, or when the
# main dialog has already closed).
$confirm = [DlgWin]::FindDialogs() | Where-Object { $_ -ne $hwnd -and [DlgWin]::IsWindow($_) } | Select-Object -First 1
if ($confirm) {
  $ctitle = Get-DialogTitle $confirm
  if ($ctitle -match 'Confirm|Overwrite|Replace' -or -not [DlgWin]::IsWindow($hwnd)) {
    Write-Output ('handling confirm dialog: ' + $ctitle)
    Focus-Dialog $confirm
    Invoke-UiaButton $confirm @('6') @('^&?Yes') | Out-Null
    Start-Sleep -Milliseconds 400
    [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
  } else {
    Write-Output ('ignoring unrelated dialog: ' + $ctitle)
  }
}

if ($ok) { Write-Output 'DIALOG_OK'; exit 0 }
Write-Output 'DIALOG_TIMEOUT'
exit 2
