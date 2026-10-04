# Startup time and idle memory, averaged over several runs. Windows only.
# Starts its own Gezik each run and stops only that one; other Gezik windows are left alone.
# Usage: scripts/perf/measure.ps1 [-Runs 3] [-Config <folder>]
# -Config runs Gezik with that config folder (GEZIK_CONFIG_DIR), e.g. one whose
# settings.toml has [view] icons = "gezik"; without it Gezik uses its usual config.
param([string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe", [int]$Runs = 3, [string]$Config = "")

if ($Config) { $env:GEZIK_CONFIG_DIR = (Resolve-Path $Config).Path }

. "$PSScriptRoot\_window.ps1"
$open = @(); $mem = @()
for ($i = 0; $i -lt $Runs; $i++) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process $Exe -PassThru
    try {
        if ((Find-GezikWindow $p.Id 10000) -eq [IntPtr]::Zero) { throw "no Gezik window within 10 s (run $($i + 1))" }
        $open += $sw.ElapsedMilliseconds
        Start-Sleep -Seconds 3
        # Task Manager's "Memory" column is the private working set.
        $mem += (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$($p.Id)").WorkingSetPrivate / 1MB
    } finally {
        Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
    }
    Start-Sleep -Milliseconds 500
}
"open {0:N0} ms | Task Manager memory {1:N1} MB | exe {2:N2} MB" -f ($open | Measure-Object -Average).Average, ($mem | Measure-Object -Average).Average, ((Get-Item $Exe).Length / 1MB)
