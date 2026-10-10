# Single instance: how long a second `gezik <folder>` takes to hand its folder to the
# running Gezik and exit, over many calls one after another (p50, p95; spec 12, goal 300 ms).
# Windows only. Starts its own Gezik with an empty config folder and stops only that one.
# Usage: scripts/perf/instance.ps1 [-Calls 100]
param([string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe", [int]$Calls = 100)

. "$PSScriptRoot\_window.ps1"
$config = Join-Path $env:TEMP "gezik-instance-$PID"
New-Item -ItemType Directory -Force $config | Out-Null
$env:GEZIK_CONFIG_DIR = $config
$folders = @($env:TEMP, $env:USERPROFILE, $env:WINDIR)
$p = Start-Process $Exe -PassThru
try {
    if ((Find-GezikWindow $p.Id 10000) -eq [IntPtr]::Zero) { throw "no Gezik window within 10 s" }
    $times = @()
    for ($i = 0; $i -lt $Calls; $i++) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        # Process.Start + WaitForExit: Start-Process -Wait adds ~1 s of its own polling on PowerShell 5.1.
        $c = [Diagnostics.Process]::Start($Exe, "`"$($folders[$i % 3])`"")
        $c.WaitForExit()
        $times += $sw.Elapsed.TotalMilliseconds
        if ($c.ExitCode -ne 0) { throw "call $($i + 1) exited with $($c.ExitCode)" }
        if (-not (Get-Process -Id $p.Id -ErrorAction SilentlyContinue)) { throw "the running Gezik is gone after call $($i + 1)" }
    }
    $sorted = $times | Sort-Object
    "p50 {0:N1} ms | p95 {1:N1} ms | the window should show 3 tabs (+ start folder)" -f $sorted[[int]($Calls * 0.5)], $sorted[[int]($Calls * 0.95) - 1]
} finally {
    Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $config -ErrorAction SilentlyContinue
}
