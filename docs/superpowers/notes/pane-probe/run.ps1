# Runs pane-probe in one mode and samples it: Task Manager memory (private working set),
# working set and CPU time, with the probe's phase marks. Windows only.
# Usage: run.ps1 -Exe <pane-probe.exe> -Mode "bench 2 both" [-Runs 3]
param([string]$Exe, [string]$Mode = "bench 2 both", [int]$Runs = 3)

$out = Join-Path $env:TEMP "pane-probe-out.txt"
for ($r = 1; $r -le $Runs; $r++) {
    $p = Start-Process $Exe -ArgumentList $Mode -PassThru -RedirectStandardOutput $out
    $samples = @()
    while (-not $p.HasExited) {
        $ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $cim = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$($p.Id)" -ErrorAction SilentlyContinue
        $p.Refresh()
        if ($cim) { $samples += [pscustomobject]@{ ms = $ms; wsp = $cim.WorkingSetPrivate / 1MB; cpu = $p.TotalProcessorTime.TotalMilliseconds } }
        Start-Sleep -Milliseconds 150
    }
    $phases = @{}
    Get-Content $out | ForEach-Object { if ($_ -match '^phase (\S+) (\d+)') { $phases[$Matches[1]] = [int64]$Matches[2] } else { "  $_" } }
    $line = "run $r [$Mode]:"
    # Memory: the last samples before each later phase begins (the state has settled).
    $order = $phases.GetEnumerator() | Sort-Object Value
    $prev = $null
    foreach ($ph in $order) {
        if ($prev) {
            $s = $samples | Where-Object { $_.ms -gt $ph.Value - 1200 -and $_.ms -lt $ph.Value - 100 }
            if ($s) { $line += " {0} {1:N2} MB;" -f $prev.Name, ($s | Measure-Object wsp -Average).Average }
        }
        $prev = $ph
    }
    if ($phases.ContainsKey("scroll")) {
        $a = $samples | Where-Object { $_.ms -ge $phases["scroll"] } | Select-Object -First 1
        $b = $samples | Where-Object { $_.ms -le $phases["after"] } | Select-Object -Last 1
        $line += " scroll CPU {0:N1}% of one core" -f (100 * ($b.cpu - $a.cpu) / ($b.ms - $a.ms))
    }
    $line
}
