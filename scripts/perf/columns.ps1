# Memory in the Miller columns 30 levels deep (spec 10 §1, §12, §13.1 10e): the columns at the
# start, after → 30 times (target: the start + the listings of the columns shown + 0.5 MB) and
# after ← 30 times. Windows only. Starts its own Gezik with its own config folder (`[view] mode
# = "columns"`) on a chain of 30 nested folders (each with the next folder and 20 files).
# Keys go only to the Gezik window: before every key the script checks that Gezik is in the
# foreground and stops if not ("Gezik lost the focus"). Needs an unlocked desktop.
# Usage: scripts/perf/columns.ps1 [-Wait 4] [-Step 300]
param([string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe", [int]$Wait = 4, [int]$Step = 300)

. "$PSScriptRoot\_window.ps1"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -Name Focus -Namespace GezikColumns -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();'

$Depth = 30
$root = Join-Path $env:TEMP "gezik-perf-columns"
$config = Join-Path $env:TEMP "gezik-perf-columns-config"
Remove-Item -Recurse -Force $root, $config -ErrorAction SilentlyContinue
$dir = $root
for ($level = 0; $level -le $Depth; $level++) {
    New-Item -ItemType Directory -Force $dir | Out-Null
    for ($i = 0; $i -lt 20; $i++) { Set-Content -Path (Join-Path $dir "file$i.txt") -Value "$i" }
    # Folders come first: → always enters the next level. Short names keep the deepest path
    # under 260 characters (Windows PowerShell 5.1).
    $dir = Join-Path $dir ("L{0:D2}" -f ($level + 1))
}
New-Item -ItemType Directory -Force $config | Out-Null
# Without a BOM: Windows PowerShell 5.1's "utf8" writes one.
[IO.File]::WriteAllText((Join-Path $config "settings.toml"), "[view]`nmode = `"columns`"`n", (New-Object Text.UTF8Encoding $false))

function Sample($id) {
    (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$id").WorkingSetPrivate / 1MB
}

$oldConfig = $env:GEZIK_CONFIG_DIR
$env:GEZIK_CONFIG_DIR = $config
$p = $null
try {
    $p = Start-Process $Exe -ArgumentList "`"$root`"" -PassThru
    $hwnd = Find-GezikWindow $p.Id 10000
    if ($hwnd -eq [IntPtr]::Zero) { throw "no Gezik window within 10 s" }
    Start-Sleep -Seconds $Wait
    [void][GezikColumns.Focus]::SetForegroundWindow($hwnd)
    Start-Sleep -Milliseconds 300
    function Press($keys) {
        # Never type into another window (someone may have clicked elsewhere).
        if ([GezikColumns.Focus]::GetForegroundWindow() -ne $hwnd) { throw "Gezik lost the focus" }
        [System.Windows.Forms.SendKeys]::SendWait($keys)
        Start-Sleep -Milliseconds $Step
    }
    # The first item (the next level's folder) selected: its column opens.
    Press "{HOME}"
    Start-Sleep -Seconds $Wait
    $start = Sample $p.Id
    for ($i = 0; $i -lt $Depth; $i++) { Press "{RIGHT}" }
    Start-Sleep -Seconds $Wait
    $deep = Sample $p.Id
    for ($i = 0; $i -lt $Depth; $i++) { Press "{LEFT}" }
    Start-Sleep -Seconds $Wait
    $back = Sample $p.Id
    "start      {0,6:N1} MB" -f $start
    "30 deep    {0,6:N1} MB  ({1:+0.0;-0.0} MB; target: the columns shown's listings + 0.5)" -f $deep, ($deep - $start)
    "back       {0,6:N1} MB  ({1:+0.0;-0.0} MB vs the start)" -f $back, ($back - $start)
} finally {
    if ($p) {
        Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
        Wait-Process -Id $p.Id -Timeout 10 -ErrorAction SilentlyContinue
    }
    Remove-Item -Recurse -Force $root, $config -ErrorAction SilentlyContinue
    # The calling shell keeps its own config folder (or none).
    if ($null -eq $oldConfig) { Remove-Item Env:GEZIK_CONFIG_DIR -ErrorAction SilentlyContinue } else { $env:GEZIK_CONFIG_DIR = $oldConfig }
}
