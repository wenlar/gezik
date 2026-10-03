# Startup time and idle memory, averaged over several runs. Windows only.
# Usage: scripts/perf/measure.ps1 [-Runs 3]
param([string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe", [int]$Runs = 3)

. "$PSScriptRoot\_window.ps1"
Get-Process gezik -ErrorAction SilentlyContinue | Stop-Process
$open = @(); $mem = @()
for ($i = 0; $i -lt $Runs; $i++) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process $Exe -PassThru
    if ((Find-GezikWindow $p.Id 10000) -eq [IntPtr]::Zero) { Write-Warning "no window after 10 s" }
    $open += $sw.ElapsedMilliseconds
    Start-Sleep -Seconds 3
    # Task Manager's "Memory" column is the private working set.
    $mem += (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$($p.Id)").WorkingSetPrivate / 1MB
    Stop-Process -Id $p.Id
    Start-Sleep -Milliseconds 500
}
"open {0:N0} ms | Task Manager memory {1:N1} MB | exe {2:N2} MB" -f ($open | Measure-Object -Average).Average, ($mem | Measure-Object -Average).Average, ((Get-Item $Exe).Length / 1MB)
