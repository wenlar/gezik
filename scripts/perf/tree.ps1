# Opens a branch of many folders in the sidebar tree and reports the longest the UI thread was
# busy meanwhile, memory with and without the branch, and the sidebar's scroll CPU (spec 10
# section 5.4). Windows only; run while nobody uses the machine (it moves the mouse).
# Usage: scripts/perf/tree.ps1 [-Dirs 50000] [-Exe target\release\gezik.exe]
param(
    [string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe",
    [int]$Dirs = 50000,
    [string]$Dir = "$env:TEMP\gezik-tree-$Dirs"
)

if (-not (Test-Path $Dir) -or @(Get-ChildItem $Dir -Directory).Count -ne $Dirs) {
    New-Item -ItemType Directory -Force $Dir | Out-Null
    for ($i = 0; $i -lt $Dirs; $i++) { [IO.Directory]::CreateDirectory("$Dir\dir_$i") | Out-Null }
}

Add-Type @"
using System; using System.Runtime.InteropServices;
public class GezikTree {
  [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, IntPtr w, IntPtr l, uint f, uint t, out IntPtr r);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int x, int y, int d, IntPtr e);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  public struct R { public int L, T, Rt, B; }
}
"@
[GezikTree]::SetProcessDPIAware() | Out-Null
. "$PSScriptRoot\_window.ps1"
function Mem($id) { (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$id").WorkingSetPrivate / 1MB }

# The longest the window took to answer an empty message during $ms: its UI thread's longest
# busy stretch (stands in for the longest frame).
function Stall($h, $ms) {
    $max = 0.0
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt $ms) {
        $t = [Diagnostics.Stopwatch]::StartNew()
        $r = [IntPtr]::Zero
        [GezikTree]::SendMessageTimeout($h, 0, [IntPtr]::Zero, [IntPtr]::Zero, 2, 5000, [ref]$r) | Out-Null
        $max = [Math]::Max($max, $t.Elapsed.TotalMilliseconds)
        Start-Sleep -Milliseconds 5
    }
    $max
}

# One run: Gezik with the test folder pinned, at %TEMP%; then a second gezik.exe sends
# "$Dir\dir_0" to it (a new tab, no typing). With tree-follow the tree opens $Dir (all its
# folders) to show dir_0.
function Run($follow) {
    $config = "$env:TEMP\gezik-tree-config-$follow"
    New-Item -ItemType Directory -Force $config | Out-Null
    $pinned = $Dir.Replace('\', '\\')
    [IO.File]::WriteAllText("$config\settings.toml", "pinned = [`"$pinned`"]`n[sidebar]`ntree-follow = $follow`n")
    $env:GEZIK_CONFIG_DIR = $config
    $p = Start-Process $Exe -ArgumentList "`"$env:TEMP`"" -PassThru
    try {
        Start-Sleep -Seconds 4
        $h = Find-GezikWindow $p.Id 10000
        if ($h -eq [IntPtr]::Zero) { throw "Gezik window not found" }
        $before = Mem $p.Id
        Start-Process $Exe -ArgumentList "`"$Dir\dir_0`"" | Out-Null
        $stall = Stall $h 6000
        $after = Mem $p.Id
        # The sidebar's scroll: 60 wheel steps over it.
        [GezikTree]::SetForegroundWindow($h) | Out-Null
        $r = New-Object GezikTree+R; [GezikTree]::GetWindowRect($h, [ref]$r) | Out-Null
        [GezikTree]::SetCursorPos($r.L + 80, [int](($r.T + $r.B) / 2)) | Out-Null
        Start-Sleep -Milliseconds 300
        $p.Refresh(); $cpu0 = $p.TotalProcessorTime.TotalMilliseconds
        for ($i = 0; $i -lt 60; $i++) { [GezikTree]::mouse_event(0x0800, 0, 0, -360, [IntPtr]::Zero); Start-Sleep -Milliseconds 16 }
        Start-Sleep -Milliseconds 500
        $p.Refresh()
        "[tree-follow {0}] {1:N0} folders | longest busy {2:N1} ms | memory before {3:N2} MB, after {4:N2} MB | sidebar scroll CPU {5:N0} ms" -f $follow, $Dirs, $stall, $before, $after, ($p.TotalProcessorTime.TotalMilliseconds - $cpu0)
    } finally {
        Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
        Remove-Item Env:GEZIK_CONFIG_DIR -ErrorAction SilentlyContinue
    }
}

Run "false"
Run "true"
