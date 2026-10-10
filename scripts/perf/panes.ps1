# Memory and handles with one pane, two panes and after closing the second (spec 10 §4.2,
# §13.1 10b). Windows only. Starts its own Gezik with its own config folder (two small
# folders as the panes' tabs), and opens and closes the second pane with F3.
# A folder watcher cannot be counted from outside: the process's HandleCount stands in for it
# (two panes should hold about as many handles as one: one watcher, the active pane's).
# Keys go only to the Gezik window: before every key the script checks that Gezik is in the
# foreground and stops if not ("Gezik lost the focus"). Needs an unlocked desktop.
# Usage: scripts/perf/panes.ps1 [-Wait 4]
param([string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe", [int]$Wait = 4)

. "$PSScriptRoot\_window.ps1"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -Name Focus -Namespace GezikPanes -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();'

$root = Join-Path $env:TEMP "gezik-perf-panes"
$left = Join-Path $root "left"; $right = Join-Path $root "right"
foreach ($dir in $left, $right) {
    New-Item -ItemType Directory -Force $dir | Out-Null
    for ($i = 0; $i -lt 20; $i++) { Set-Content -Path (Join-Path $dir "file$i.txt") -Value "$i" }
}

$config = Join-Path $env:TEMP "gezik-perf-panes-config"
Remove-Item -Recurse -Force $config -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $config | Out-Null
# The left pane on one folder, the right pane's tabs (kept while it is closed) on the other;
# the F3 hint is marked shown so it writes nothing.
$lines = @(
    "[session]", "active = 0", "right-active = 0", "",
    "[[session.tabs]]", ("path = '" + $left + "'"), "",
    "[[session.right-tabs]]", ("path = '" + $right + "'"), "",
    "[hints]", "f3-moved = true"
)
# Without a BOM: Windows PowerShell 5.1's "utf8" writes one.
[IO.File]::WriteAllLines((Join-Path $config "state.toml"), [string[]]$lines, (New-Object Text.UTF8Encoding $false))

function Sample($id) {
    $mb = (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$id").WorkingSetPrivate / 1MB
    [pscustomobject]@{ Mb = $mb; Handles = (Get-Process -Id $id).HandleCount }
}

$oldConfig = $env:GEZIK_CONFIG_DIR
$env:GEZIK_CONFIG_DIR = $config
$p = $null
try {
    $p = Start-Process $Exe -PassThru
    $hwnd = Find-GezikWindow $p.Id 10000
    if ($hwnd -eq [IntPtr]::Zero) { throw "no Gezik window within 10 s" }
    Start-Sleep -Seconds $Wait
    $one = Sample $p.Id
    [void][GezikPanes.Focus]::SetForegroundWindow($hwnd)
    Start-Sleep -Milliseconds 300
    function Press($keys) {
        # Never type into another window (someone may have clicked elsewhere).
        if ([GezikPanes.Focus]::GetForegroundWindow() -ne $hwnd) { throw "Gezik lost the focus" }
        [System.Windows.Forms.SendKeys]::SendWait($keys)
    }
    Press "{F3}"
    Start-Sleep -Seconds $Wait
    $two = Sample $p.Id
    Press "{F3}"
    Start-Sleep -Seconds $Wait
    $closed = Sample $p.Id
    "one pane      {0,6:N1} MB  {1,5} handles" -f $one.Mb, $one.Handles
    "two panes     {0,6:N1} MB  {1,5} handles  ({2:+0.0;-0.0} MB, {3:+0;-0} handles)" -f $two.Mb, $two.Handles, ($two.Mb - $one.Mb), ($two.Handles - $one.Handles)
    "after closing {0,6:N1} MB  {1,5} handles  ({2:+0.0;-0.0} MB vs one pane, target within 0.1; {3:+0;-0} handles)" -f $closed.Mb, $closed.Handles, ($closed.Mb - $one.Mb), ($closed.Handles - $one.Handles)
} finally {
    if ($p) {
        Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
        Wait-Process -Id $p.Id -Timeout 10 -ErrorAction SilentlyContinue
    }
    Remove-Item -Recurse -Force $root, $config -ErrorAction SilentlyContinue
    # The calling shell keeps its own config folder (or none).
    if ($null -eq $oldConfig) { Remove-Item Env:GEZIK_CONFIG_DIR -ErrorAction SilentlyContinue } else { $env:GEZIK_CONFIG_DIR = $oldConfig }
}
