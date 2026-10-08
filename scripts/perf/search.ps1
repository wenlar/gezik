# Search timings (spec 12): a tree of 1,000,000 empty files (1,000 folders x 1,000, three
# levels), 10,000 small text files under text\ (one says "needle"), and half\ (400,000 files,
# under the name cache's limit). Made once in %TEMP%\gezik-search-tree; -Fresh makes it again.
param([switch]$Fresh, [string]$Pattern = "file_12*")
$ErrorActionPreference = "Stop"
$tree = Join-Path $env:TEMP "gezik-search-tree"
if ($Fresh -and (Test-Path $tree)) { Remove-Item -Recurse -Force $tree }
if (-not (Test-Path $tree)) {
    Write-Host "Making the tree (once; a few minutes)..."
    foreach ($a in 0..9) { foreach ($b in 0..9) { foreach ($c in 0..9) {
        $dir = Join-Path $tree "d$a\d$b\d$c"
        [void][System.IO.Directory]::CreateDirectory($dir)
        foreach ($f in 0..999) { [System.IO.File]::WriteAllBytes((Join-Path $dir "file_$f.dat"), @()) }
    } } }
    $text = Join-Path $tree "text"
    [void][System.IO.Directory]::CreateDirectory($text)
    foreach ($i in 0..9999) {
        $body = if ($i -eq 7777) { "line one`nthe needle is here`n" } else { "line one`nnothing`n" }
        [System.IO.File]::WriteAllText((Join-Path $text "t$i.txt"), $body)
    }
    $half = Join-Path $tree "half"
    foreach ($a in 0..399) {
        $dir = Join-Path $half "h$a"
        [void][System.IO.Directory]::CreateDirectory($dir)
        foreach ($f in 0..999) { [System.IO.File]::WriteAllBytes((Join-Path $dir "file_$f.dat"), @()) }
    }
}
cargo build --release -p gezik-search --example search_bench -j 8
$exe = Join-Path $PSScriptRoot "..\..\target\release\examples\search_bench.exe"
# Warm the cache once, then measure.
& $exe $tree $Pattern | Out-Null
$p = Start-Process -FilePath $exe -ArgumentList @($tree, $Pattern) -NoNewWindow -PassThru -RedirectStandardOutput "$env:TEMP\gezik-search-bench.txt"
$peak = 0
# A process that has ended no longer reports its peak: sample it while it runs.
while (-not $p.HasExited) {
    $p.Refresh()
    if ($p.PeakWorkingSet64 -gt $peak) { $peak = $p.PeakWorkingSet64 }
    Start-Sleep -Milliseconds 5
}
Get-Content "$env:TEMP\gezik-search-bench.txt"
"peak working set: {0:N1} MB" -f ($peak / 1MB)
