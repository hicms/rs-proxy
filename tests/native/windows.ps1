param([Parameter(Mandatory=$true)][string]$Action, [int]$ProcessId, [string]$Value)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class NativeWindows {
  public delegate bool Callback(IntPtr hwnd, IntPtr value);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Callback callback, IntPtr value);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int size);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, StringBuilder text, int size);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wp, IntPtr lp);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wp, string text);
  public static IntPtr[] ForProcess(int processId) {
    var result = new List<IntPtr>();
    EnumWindows((hwnd, unused) => { uint pid; GetWindowThreadProcessId(hwnd, out pid); if (pid == processId) result.Add(hwnd); return true; }, IntPtr.Zero);
    return result.ToArray();
  }
  public static string Title(IntPtr hwnd) { var value = new StringBuilder(512); GetWindowText(hwnd,value,512); return value.ToString(); }
  public static string Class(IntPtr hwnd) { var value = new StringBuilder(512); GetClassName(hwnd,value,512); return value.ToString(); }
}
'@
$windows = @([NativeWindows]::ForProcess($ProcessId))
if ($Action -eq 'inspect') {
  @($windows | ForEach-Object { @{ Handle=$_.ToInt64(); Title=[NativeWindows]::Title($_); Class=[NativeWindows]::Class($_); Visible=[NativeWindows]::IsWindowVisible($_) } }) | ConvertTo-Json -Compress
  exit
}
if ($Action -eq 'close') {
  $main = @($windows | Where-Object { [NativeWindows]::Title($_) -eq $Value })
  if ($main.Count -ne 1) { throw 'Expected one application window' }
  [NativeWindows]::PostMessage($main[0], 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
  exit
}
if ($Action -eq 'file' -or $Action -eq 'cancel' -or $Action -eq 'controls') {
  for ($attempt = 0; $attempt -lt 50; $attempt++) {
    $dialog = @([NativeWindows]::ForProcess($ProcessId) | Where-Object { [NativeWindows]::Class($_) -eq '#32770' -and [NativeWindows]::IsWindowVisible($_) })
    if ($dialog.Count -eq 1) { break }
    Start-Sleep -Milliseconds 100
  }
  if ($dialog.Count -ne 1) { throw "Expected one file dialog, found $($dialog.Count)" }
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($dialog[0])
  $scope = [System.Windows.Automation.TreeScope]::Descendants
  if ($Action -eq 'controls') {
    $root.FindAll($scope, [System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object { @{ Name=$_.Current.Name; Id=$_.Current.AutomationId; Type=$_.Current.ControlType.ProgrammaticName; Class=$_.Current.ClassName; Handle=$_.Current.NativeWindowHandle; Patterns=@($_.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName }) } } | ConvertTo-Json -Depth 4
    exit
  }
  if ($Action -eq 'file') {
    $hostCondition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'FileNameControlHost')
    $editHost = $root.FindFirst($scope, $hostCondition)
    if ($editHost) {
      $editCondition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, '1001')
      $edit = $editHost.FindFirst($scope, $editCondition)
    } else {
      $editCondition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, '1148')
      $edit = @($root.FindAll($scope, $editCondition) | Where-Object { $_.Current.ClassName -eq 'Edit' }) | Select-Object -First 1
    }
    if (-not $edit -or -not $edit.Current.NativeWindowHandle) { throw 'File name edit handle not found' }
    [NativeWindows]::SendMessage([IntPtr]$edit.Current.NativeWindowHandle, 0x000C, [IntPtr]::Zero, $Value) | Out-Null
  }
  $buttonId = if ($Action -eq 'cancel') { '2' } else { '1' }
  $buttonCondition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $buttonId)
  $button = @($root.FindAll($scope, $buttonCondition) | Where-Object { $_.Current.NativeWindowHandle -and [NativeWindows]::Class([IntPtr]$_.Current.NativeWindowHandle) -eq 'Button' }) | Select-Object -First 1
  if (-not $button) { throw 'Dialog action button not found' }
  if (-not $button.Current.NativeWindowHandle) { throw 'Dialog button handle not found' }
  [NativeWindows]::PostMessage([IntPtr]$button.Current.NativeWindowHandle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
  exit
}
throw "Unknown action $Action"
