# Copies one large file with Gezik's engine and with Explorer's (Shell.Application CopyHere,
# no UI), taking turns, and prints each time: until the copy says it is done, and until its
# data is on the disk (a copy through the cache is done before its data is written).
# Release build; run on a quiet machine.
# Usage: scripts\perf\ops_big.ps1 [-Work <folder>] [-GB 4] [-Rounds 3]
# The work folder is deleted and recreated: it must not exist yet or must have been created by
# this tool (it holds a .gezik-ops-bench marker file); any other folder is refused.
param(
    [string]$Work = (Join-Path $env:TEMP "gezik-ops-big"),
    [int]$GB = 4,
    [int]$Rounds = 3
)
$ErrorActionPreference = "Stop"
$marker = ".gezik-ops-bench"
# Writes what the system still holds of `path` to the disk.
function Flush-ToDisk([string]$path) {
    $file = [IO.File]::Open($path, 'Open', 'ReadWrite', 'ReadWrite')
    try { $file.Flush($true) } finally { $file.Close() }
}
function Test-Ours { Test-Path (Join-Path $Work $marker) }
if ((Test-Path $Work) -and -not (Test-Ours)) {
    throw "Refusing to use ${Work}: it exists and was not created by this tool (no $marker inside). Pick a new folder."
}
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Push-Location $root
try {
    cargo build -p gezik-ops --release --example ops_bench
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    $bench = "$root\target\release\examples\ops_bench.exe"
    if (Test-Ours) { Remove-Item -Recurse -Force $Work }
    New-Item -ItemType Directory -Force $Work | Out-Null
    New-Item -ItemType File -Force (Join-Path $Work $marker) | Out-Null
    $source = Join-Path $Work "big.bin"
    # Random-looking data (a block of 64 MB, each copy marked differently).
    $block = New-Object byte[] (64MB); (New-Object Random 1).NextBytes($block)
    $stream = [IO.File]::Create($source)
    for ($i = 0; $i -lt $GB * 16; $i++) { $block[0] = [byte]$i; $block[1] = [byte]($i -shr 8); $stream.Write($block, 0, $block.Length) }
    $stream.Close()
    $size = (Get-Item $source).Length
    $shell = New-Object -ComObject Shell.Application
    for ($round = 1; $round -le $Rounds; $round++) {
        $gezikDst = Join-Path $Work "gezik"; $explorerDst = Join-Path $Work "explorer"
        foreach ($d in $gezikDst, $explorerDst) { if (Test-Path $d) { Remove-Item -Recurse -Force $d }; New-Item -ItemType Directory $d | Out-Null }

        $watch = [Diagnostics.Stopwatch]::StartNew()
        $line = & $bench copyfile $source $gezikDst
        if ($LASTEXITCODE -ne 0) { throw "ops_bench failed (exit $LASTEXITCODE)" }
        $gezik = [int]($line -replace '\D', '')
        $flush = [Diagnostics.Stopwatch]::StartNew(); Flush-ToDisk (Join-Path $gezikDst "big.bin"); $gezikOnDisk = $gezik + $flush.ElapsedMilliseconds

        $target = Join-Path $explorerDst "big.bin"
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $deadline = (Get-Date).AddMinutes(10)
        # 4: no progress dialog, 16: yes to all, 1024: no error UI. CopyHere returns at once,
        # and the copy gets its full size at the start: done once it can be opened alone.
        $shell.Namespace($explorerDst).CopyHere($source, 1044)
        while ($true) {
            if ((Get-Date) -gt $deadline) { throw "Explorer had not copied the file after 10 minutes" }
            if ((Test-Path $target) -and (Get-Item $target).Length -eq $size) {
                try { [IO.File]::Open($target, 'Open', 'Read', 'None').Close(); break } catch { }
            }
            Start-Sleep -Milliseconds 20
        }
        $explorer = $watch.ElapsedMilliseconds
        Flush-ToDisk $target
        $explorerOnDisk = $watch.ElapsedMilliseconds
        Write-Host ("round {0}: copy {1} GB file: done Gezik {2} ms, Explorer {3} ms ({4:N2}x); on disk Gezik {5} ms, Explorer {6} ms ({7:N2}x)" -f $round, $GB, $gezik, $explorer, ($explorer / [Math]::Max(1, $gezik)), $gezikOnDisk, $explorerOnDisk, ($explorerOnDisk / [Math]::Max(1, $gezikOnDisk)))
    }
}
finally {
    if (Test-Ours) { Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue }
    Pop-Location
}
