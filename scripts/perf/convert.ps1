# scripts/perf/convert.ps1 - conversion timings (5c): 100 generated 12 MP photos resized to
# 1920 px JPEGs on every core, and a 100 MB Windows-1254 text converted to UTF-8.
# Usage: powershell -ExecutionPolicy Bypass -File .\scripts\perf\convert.ps1 [-Photos 100] [-TextMB 100] [-Megapixels 12|24] [-Runs 1]
param([int]$Photos = 100, [int]$TextMB = 100, [ValidateSet(12, 24)][int]$Megapixels = 12, [int]$Runs = 1)
$ErrorActionPreference = 'Stop'
$work = Join-Path $env:TEMP "gezik-perf-convert"
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
try {
    cargo build --release -q -p gezik-batch --example convert_bench
    if ($LASTEXITCODE -ne 0) { throw "build failed" }
    for ($run = 1; $run -le $Runs; $run++) {
        Write-Host "run $run"
        cargo run --release -q -p gezik-batch --example convert_bench -- $work $Photos $TextMB $Megapixels
        if ($LASTEXITCODE -ne 0) { throw "convert_bench failed" }
    }
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
