# Compares Gezik's engine with Explorer on many small files (copy) and with rd /s on a delete.
# Release build; run on a quiet machine. Usage: scripts\perf\ops.ps1 [-Work <folder>] [-Files 10000]
param(
    [string]$Work = (Join-Path $env:TEMP "gezik-ops-perf"),
    [int]$Files = 10000
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Push-Location $root
try {
    cargo build -p gezik-ops --release --example ops_bench
    Write-Host "== Gezik engine"
    & "$root\target\release\examples\ops_bench.exe" $Work $Files

    Write-Host "== Explorer (Shell.Application CopyHere, no UI)"
    $src = Join-Path $Work "src"
    $dst = Join-Path $Work "explorer"
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $src | Out-Null
    New-Item -ItemType Directory -Force $dst | Out-Null
    $folders = [Math]::Max(1, [int]($Files / 100))
    for ($f = 0; $f -lt $folders; $f++) {
        $folder = Join-Path $src ("folder{0:D3}" -f $f)
        New-Item -ItemType Directory -Force $folder | Out-Null
        for ($i = 0; $i -lt [int]($Files / $folders); $i++) {
            $size = 4096 + (($i * 7919 + $f * 104729) % (60 * 1024))
            [IO.File]::WriteAllBytes((Join-Path $folder ("file{0:D4}.bin" -f $i)), (New-Object byte[] $size))
        }
    }
    $shell = New-Object -ComObject Shell.Application
    $watch = [Diagnostics.Stopwatch]::StartNew()
    # 4: no progress dialog, 16: yes to all, 1024: no error UI. CopyHere returns at once:
    # wait until every file is there.
    $shell.Namespace($dst).CopyHere($src, 1044)
    $target = Join-Path $dst "src"
    while (-not (Test-Path $target) -or (Get-ChildItem -Recurse -File $target).Count -lt $Files) {
        Start-Sleep -Milliseconds 50
    }
    $watch.Stop()
    Write-Host ("copy {0} files: {1} ms" -f $Files, $watch.ElapsedMilliseconds)

    Write-Host "== rd /s /q (delete baseline)"
    $watch = [Diagnostics.Stopwatch]::StartNew()
    cmd /c rd /s /q "$target"
    $watch.Stop()
    Write-Host ("delete {0} files: {1} ms" -f $Files, $watch.ElapsedMilliseconds)
}
finally {
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
    Pop-Location
}
