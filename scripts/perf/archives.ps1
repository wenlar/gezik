# scripts/perf/archives.ps1 - zip create/extract timings (5b): Gezik against Windows'
# Compress-Archive / Expand-Archive and the Shell (Shell.Application CopyHere, what "Send to
# > Compressed folder" and Explorer's zip folder do).
# Usage: powershell -ExecutionPolicy Bypass -File .\scripts\perf\archives.ps1 [-SizeMB 1024] [-SkipShell]
param([int]$SizeMB = 1024, [switch]$SkipShell)
$ErrorActionPreference = 'Stop'
# Plain digits in the output whatever the system language.
[Threading.Thread]::CurrentThread.CurrentCulture = [Globalization.CultureInfo]::InvariantCulture
$root = Join-Path $env:TEMP "gezik-perf-archives"
$src = Join-Path $root "src"
$work = Join-Path $root "work"
Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue

# 1000 small files (compressible text), 10 big compressible and 10 big incompressible ones.
function New-Tree {
    New-Item -ItemType Directory $src | Out-Null
    $small = New-Item -ItemType Directory (Join-Path $src "small")
    $line = [Text.Encoding]::ASCII.GetBytes("The quick brown fox jumps over the lazy dog. 0123456789`r`n")
    for ($i = 0; $i -lt 1000; $i++) {
        $bytes = New-Object byte[] (2048 + ($i % 7) * 1024)
        for ($o = 0; $o -lt $bytes.Length; $o++) { $bytes[$o] = $line[($o + $i) % $line.Length] }
        [IO.File]::WriteAllBytes((Join-Path $small.FullName ("f{0:D4}.txt" -f $i)), $bytes)
    }
    $big = New-Item -ItemType Directory (Join-Path $src "big")
    $each = [int](($SizeMB - 8) / 20) * 1MB
    $rng = New-Object Security.Cryptography.RNGCryptoServiceProvider
    $chunk = New-Object byte[] (1MB)
    for ($i = 0; $i -lt 20; $i++) {
        $compressible = ($i % 2 -eq 0)
        $stream = [IO.File]::Create((Join-Path $big.FullName ("big{0:D2}{1}.bin" -f $i, $(if ($compressible) { "-text" } else { "-noise" }))))
        try {
            for ($written = 0; $written -lt $each; $written += $chunk.Length) {
                if ($compressible) {
                    # Text-like: words picked from a small list, so deflate gets ~3-4x.
                    $words = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu".Split(' ')
                    $rand = New-Object Random ($i * 1000 + ($written / 1MB))
                    $sb = New-Object Text.StringBuilder
                    while ($sb.Length -lt $chunk.Length) { [void]$sb.Append($words[$rand.Next($words.Length)]).Append(' ') }
                    $data = [Text.Encoding]::ASCII.GetBytes($sb.ToString(0, $chunk.Length))
                    $stream.Write($data, 0, $data.Length)
                } else {
                    $rng.GetBytes($chunk)
                    $stream.Write($chunk, 0, $chunk.Length)
                }
            }
        } finally { $stream.Dispose() }
    }
}

function Get-Count($dir) { @(Get-ChildItem -Recurse -File $dir).Count }
function Get-Bytes($dir) { (Get-ChildItem -Recurse -File $dir | Measure-Object Length -Sum).Sum }

# Shell copies run on another thread: wait until the size stops changing for 3 polls.
function Wait-Stable([scriptblock]$measure, [scriptblock]$ready) {
    $last = -1; $same = 0
    while ($same -lt 3) {
        Start-Sleep -Milliseconds 200
        $now = & $measure
        if ((& $ready) -and $now -eq $last) { $same++ } else { $same = 0 }
        $last = $now
    }
}

try {
    New-Item -ItemType Directory $work | Out-Null
    Write-Host "Making the test folder..."
    New-Tree
    $files = Get-Count $src
    $bytes = Get-Bytes $src
    Write-Host ("{0} files, {1:N0} MB" -f $files, ($bytes / 1MB))

    cargo build --release -q -p gezik-batch --example archive_bench
    Write-Host "--- Gezik (zip, Normal)"
    cargo run --release -q -p gezik-batch --example archive_bench -- $src (Join-Path $work "gezik")

    Write-Host "--- Compress-Archive / Expand-Archive"
    $zip = Join-Path $work "ps.zip"
    $t = Measure-Command { Compress-Archive -Path (Join-Path $src "*") -DestinationPath $zip -CompressionLevel Optimal }
    Write-Host ("Compress-Archive: {0:N0} ms ({1:N0} bytes)" -f $t.TotalMilliseconds, (Get-Item $zip).Length)
    $out = Join-Path $work "ps-out"
    $t = Measure-Command { Expand-Archive -Path $zip -DestinationPath $out }
    Write-Host ("Expand-Archive: {0:N0} ms ({1} files)" -f $t.TotalMilliseconds, (Get-Count $out))

    if (-not $SkipShell) {
        Write-Host "--- Shell (CopyHere)"
        $shell = New-Object -ComObject Shell.Application
        $szip = Join-Path $work "shell.zip"
        [IO.File]::WriteAllBytes($szip, [byte[]](80, 75, 5, 6 + 0) + (New-Object byte[] 18))
        $flags = 4 -bor 16 -bor 1024
        $sw = [Diagnostics.Stopwatch]::StartNew()
        foreach ($item in @("small", "big")) {
            $ns = $shell.NameSpace($szip)
            $before = $ns.Items().Count
            $ns.CopyHere($shell.NameSpace((Join-Path $src $item)).Self, $flags)
            # The zip folder lists the new item once its copy is done.
            while ($shell.NameSpace($szip).Items().Count -le $before) { Start-Sleep -Milliseconds 200 }
        }
        Wait-Stable { (Get-Item $szip).Length } { $true }
        $sw.Stop()
        # The stable-size wait is 3 polls of 200 ms.
        Write-Host ("Shell compress: {0:N0} ms ({1:N0} bytes)" -f ($sw.ElapsedMilliseconds - 600), (Get-Item $szip).Length)
        $sout = Join-Path $work "shell-out"
        New-Item -ItemType Directory $sout | Out-Null
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $shell.NameSpace($sout).CopyHere($shell.NameSpace($zip).Items(), $flags)
        Wait-Stable { Get-Bytes $sout } { (Get-Count $sout) -ge $files }
        $sw.Stop()
        Write-Host ("Shell extract: {0:N0} ms ({1} files)" -f ($sw.ElapsedMilliseconds - 600), (Get-Count $sout))
    }
} finally {
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}
