# scripts/perf/batch.ps1 — batch rename timings (5a). Usage: .\scripts\perf\batch.ps1
$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:TEMP "gezik-perf-rename"
Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
try {
    New-Item -ItemType Directory $dir | Out-Null
    1..10000 | ForEach-Object { [IO.File]::WriteAllBytes((Join-Path $dir ("IMG_{0:D5}.jpg" -f $_)), [byte[]]@()) }
    cargo run --release -q -p gezik-batch --example rename_bench
    cargo run --release -q -p gezik-ops --example rename_many -- $dir
} finally {
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
}
