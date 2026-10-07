# Memory with 1 tab vs 20 tabs. Windows only.
# Tabs are opened by posting Ctrl+T key messages straight to the Gezik window, so no
# focus is needed. If Gezik ignores them (it may read modifier state from the keyboard
# rather than from the messages), pass -Method SendKeys: that uses SendKeys and
# steals focus; run it when nothing else is in the foreground.
# Usage: scripts/perf/tabs.ps1 [-Tabs 20] [-Method PostMessage|SendKeys]  [-Session N]
param(
    [string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe",
    [int]$Tabs = 20,
    [ValidateSet("PostMessage", "SendKeys")] [string]$Method = "PostMessage",
    [int]$Session = 0
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
function Mem($id) { (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$id").WorkingSetPrivate / 1MB }

$WM_KEYDOWN = 0x0100; $WM_KEYUP = 0x0101; $VK_CONTROL = [IntPtr]0x11; $VK_T = [IntPtr]0x54
function NewTab($h) {
    if ($Method -eq "SendKeys") { [System.Windows.Forms.SendKeys]::SendWait("^t"); return }
    [GezikTabs]::PostMessage($h, $WM_KEYDOWN, $VK_CONTROL, [IntPtr]0x001D0001) | Out-Null
    [GezikTabs]::PostMessage($h, $WM_KEYDOWN, $VK_T, [IntPtr]0x00140001) | Out-Null
    [GezikTabs]::PostMessage($h, $WM_KEYUP, $VK_T, [IntPtr]([int64]0xC0140001L)) | Out-Null
    [GezikTabs]::PostMessage($h, $WM_KEYUP, $VK_CONTROL, [IntPtr]([int64]0xC01D0001L)) | Out-Null
}

if ($Session -gt 0) {
    $cfg = Join-Path $env:TEMP "gezik-session-perf"
    Remove-Item -Recurse -Force $cfg -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $cfg | Out-Null
    $lines = @("[session]", "active = 0", "")
    for ($i = 0; $i -lt $Session; $i++) {
        $dir = Join-Path $env:TEMP "gezik-session-perf-tabs\t$i"
        New-Item -ItemType Directory -Force $dir | Out-Null
        $lines += "[[session.tabs]]", ("path = '" + $dir + "'"), ""
    }
    Set-Content -Encoding utf8 (Join-Path $cfg "state.toml") $lines
    $env:GEZIK_CONFIG_DIR = $cfg
    $times = @()
    for ($r = 0; $r -lt 5; $r++) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $q = Start-Process $Exe -PassThru
        try {
            if ((Find-GezikWindow $q.Id 10000) -eq [IntPtr]::Zero) { throw "no Gezik window within 10 s" }
            $times += $sw.ElapsedMilliseconds
            Start-Sleep -Seconds 3
            $mem = Mem $q.Id
        } finally { Stop-Process -Id $q.Id -ErrorAction SilentlyContinue }
        Start-Sleep -Milliseconds 500
    }
    "{0} saved tabs: open {1:N0} ms | Task Manager memory {2:N1} MB" -f $Session, ($times | Measure-Object -Average).Average, $mem
    return
}

$p = Start-Process $Exe -PassThru
try {
$h = Find-GezikWindow $p.Id 10000
if ($h -eq [IntPtr]::Zero) { throw "Gezik window not found" }
Start-Sleep -Seconds 3
$one = Mem $p.Id
if ($Method -eq "SendKeys") { [GezikTabs]::SetForegroundWindow($h) | Out-Null }
for ($i = 1; $i -lt $Tabs; $i++) { NewTab $h; Start-Sleep -Milliseconds 150 }
Start-Sleep -Seconds 3
$many = Mem $p.Id
"1 tab {0:N1} MB | {1} tabs {2:N1} MB | +{3:N1} MB" -f $one, $Tabs, $many, ($many - $one)
if ($Method -eq "PostMessage") {
    Write-Warning "The number of open tabs is NOT verified. Growth near 0 MB with -Method PostMessage may mean the tabs never opened (Gezik may ignore posted key messages); rerun with -Method SendKeys."
}
} finally {
    Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
}
