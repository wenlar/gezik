# scripts/perf/pdf.ps1 - PDF timings (5d): 100 generated 12 MP JPEGs and 5 12 MP PNGs made
# into PDFs in this process (time, size, peak memory); with -Pdfium (the pdfium library, as
# GEZIK_TEST_PDFIUM) also the 100-page PDF split into pages and its pages saved as 150 dpi
# PNGs by the worker, each followed by a count of worker processes left. -Worker runs another
# worker program instead of the bench itself (e.g. target\release\gezik.exe).
# Usage: powershell -ExecutionPolicy Bypass -File .\scripts\perf\pdf.ps1 [-Photos 100] [-Runs 1] [-Pdfium <pdfium.dll>] [-Worker <gezik.exe>]
param([int]$Photos = 100, [int]$Runs = 1, [string]$Pdfium = "", [string]$Worker = "")
$ErrorActionPreference = 'Stop'
$work = Join-Path $env:TEMP "gezik-perf-pdf"
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
$oldPdfium = $env:GEZIK_TEST_PDFIUM
$oldWorker = $env:PDF_BENCH_WORKER
try {
    $env:GEZIK_TEST_PDFIUM = $Pdfium
    $env:PDF_BENCH_WORKER = $Worker
    cargo build --release -q -p gezik-batch --example pdf_bench
    if ($LASTEXITCODE -ne 0) { throw "build failed" }
    for ($run = 1; $run -le $Runs; $run++) {
        Write-Host "run $run"
        cargo run --release -q -p gezik-batch --example pdf_bench -- $work $Photos
        if ($LASTEXITCODE -ne 0) { throw "pdf_bench failed" }
    }
} finally {
    $env:GEZIK_TEST_PDFIUM = $oldPdfium
    $env:PDF_BENCH_WORKER = $oldWorker
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
