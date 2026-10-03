# Memory with 1 tab vs 20 tabs. Windows only.
# Tabs are opened by posting Ctrl+T key messages straight to the Gezik window, so no
# focus is needed. If Gezik ignores them (it may read modifier state from the keyboard
# rather than from the messages), pass -Method SendKeys: that uses SendKeys and
# steals focus; run it when nothing else is in the foreground.
# Usage: scripts/perf/tabs.ps1 [-Tabs 20] [-Method PostMessage|SendKeys]
param(
    [string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe",
    [int]$Tabs = 20,
    [ValidateSet("PostMessage", "SendKeys")] [string]$Method = "PostMessage"
)

Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices;
public class GezikTabs {
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}
"@
. "$PSScriptRoot\_window.ps1"
Get-Process gezik -ErrorAction SilentlyContinue | Stop-Process
function Mem($id) { (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$id").WorkingSetPrivate / 1MB }

$WM_KEYDOWN = 0x0100; $WM_KEYUP = 0x0101; $VK_CONTROL = [IntPtr]0x11; $VK_T = [IntPtr]0x54
function NewTab($h) {
    if ($Method -eq "SendKeys") { [System.Windows.Forms.SendKeys]::SendWait("^t"); return }
    [GezikTabs]::PostMessage($h, $WM_KEYDOWN, $VK_CONTROL, [IntPtr]0x001D0001) | Out-Null
    [GezikTabs]::PostMessage($h, $WM_KEYDOWN, $VK_T, [IntPtr]0x00140001) | Out-Null
    [GezikTabs]::PostMessage($h, $WM_KEYUP, $VK_T, [IntPtr]([int64]0xC0140001 -band 0xFFFFFFFF)) | Out-Null
    [GezikTabs]::PostMessage($h, $WM_KEYUP, $VK_CONTROL, [IntPtr]([int64]0xC01D0001 -band 0xFFFFFFFF)) | Out-Null
}

$p = Start-Process $Exe -PassThru
$h = Find-GezikWindow $p.Id 10000
if ($h -eq [IntPtr]::Zero) { Stop-Process -Id $p.Id; throw "Gezik window not found" }
Start-Sleep -Seconds 3
$one = Mem $p.Id
if ($Method -eq "SendKeys") { [GezikTabs]::SetForegroundWindow($h) | Out-Null }
for ($i = 1; $i -lt $Tabs; $i++) { NewTab $h; Start-Sleep -Milliseconds 150 }
Start-Sleep -Seconds 3
$many = Mem $p.Id
"1 tab {0:N1} MB | {1} tabs {2:N1} MB | +{3:N1} MB" -f $one, $Tabs, $many, ($many - $one)
Stop-Process -Id $p.Id
