# Times the filter at 100,000 entries. Without -Gui: the pure part (examples/filter_bench.rs:
# compile, match, carry the selection over), release build. With -Gui also the whole path in
# release Gezik: opens the 100,000-file folder (made on first use, as stress.ps1 does), brings
# it to the front, Ctrl+F, then types file_1234 letter by letter with SendKeys, reading the
# process's CPU time before each keystroke and 300 ms after it; prints the worst and median
# CPU ms per keystroke (less the idle CPU of 300 ms with the bar open), then the same after
# Ctrl+A on all 100,000, and Esc (closing the bar) each time. Needs an unlocked desktop.
# Windows only. Budget: 15 ms for the pure part, 30 ms per keystroke for the whole.
# Usage: scripts/perf/filter.ps1 [-Gui] [-Files 100000]
param(
    [switch]$Gui,
    [string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe",
    [int]$Files = 100000,
    [string]$Dir = "$env:TEMP\gezik-stress-$Files"
)

$root = Resolve-Path "$PSScriptRoot\..\.."
& cargo run -q -p gezik-core --release --example filter_bench --manifest-path "$root\Cargo.toml" -- $Files
if (-not $Gui) { return }

if (-not (Test-Path $Dir) -or (Get-ChildItem $Dir).Count -ne $Files) {
    New-Item -ItemType Directory -Force $Dir | Out-Null
    for ($i = 0; $i -lt $Files; $i++) { [IO.File]::WriteAllBytes("$Dir\file_$i.txt", [byte[]]::new(0)) }
}
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices;
public class GezikFilter {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[GezikFilter]::SetProcessDPIAware() | Out-Null

function Cpu($p) { $p.Refresh(); $p.TotalProcessorTime.TotalMilliseconds }
# The CPU ms of one SendKeys string, from before it to 300 ms after it.
function One($p, [string]$keys) {
    $c0 = Cpu $p
    [System.Windows.Forms.SendKeys]::SendWait($keys)
    Start-Sleep -Milliseconds 300
    (Cpu $p) - $c0
}
function Stats([double[]]$ms, [double]$idle) {
    $s = $ms | ForEach-Object { [math]::Max(0, $_ - $idle) } | Sort-Object
    "worst {0:N0} ms, median {1:N0} ms ({2} keystrokes)" -f $s[-1], $s[[int][math]::Floor(($s.Count - 1) / 2)], $s.Count
}
function Typed($p, [double]$idle) { Stats @("f", "i", "l", "e", "_", "1", "2", "3", "4" | ForEach-Object { One $p $_ }) $idle }

$env:GEZIK_CONFIG_DIR = Join-Path $env:TEMP "gezik-filter-perf-config"
New-Item -ItemType Directory -Force $env:GEZIK_CONFIG_DIR | Out-Null
$p = Start-Process $Exe -ArgumentList "`"$Dir`"" -PassThru
try {
    Start-Sleep -Seconds 5
    . "$PSScriptRoot\_window.ps1"
    $h = Find-GezikWindow $p.Id 10000
    if ($h -eq [IntPtr]::Zero) { throw "Gezik window not found" }
    [GezikFilter]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 500
    [System.Windows.Forms.SendKeys]::SendWait("^f"); Start-Sleep -Seconds 1
    $idle = @(1..5 | ForEach-Object { $c0 = Cpu $p; Start-Sleep -Milliseconds 300; (Cpu $p) - $c0 } | Sort-Object)[2]
    "idle with the bar open: {0:N0} ms per 300 ms" -f $idle
    "file_1234, nothing selected: " + (Typed $p $idle)
    "Esc, nothing selected: {0:N0} ms" -f [math]::Max(0, (One $p "{ESC}") - $idle)
    Start-Sleep -Seconds 1
    [System.Windows.Forms.SendKeys]::SendWait("{ESC}^a"); Start-Sleep -Seconds 1
    [System.Windows.Forms.SendKeys]::SendWait("^f"); Start-Sleep -Seconds 1
    "file_1234 after Ctrl+A on {0:N0}: " -f $Files + (Typed $p $idle)
    "Esc after Ctrl+A: {0:N0} ms" -f [math]::Max(0, (One $p "{ESC}") - $idle)
} finally {
    Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
    Remove-Item Env:\GEZIK_CONFIG_DIR -ErrorAction SilentlyContinue
}
