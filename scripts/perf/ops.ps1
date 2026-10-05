# Compares Gezik's engine with Explorer on many small files: copy, and a permanent delete
# (also against rd /s).
# Release build; run on a quiet machine. Usage: scripts\perf\ops.ps1 [-Work <folder>] [-Files 10000]
# The work folder is deleted and recreated: it must not exist yet or must have been created by
# this tool (it holds a .gezik-ops-bench marker file); any other folder is refused.
param(
    [string]$Work = (Join-Path $env:TEMP "gezik-ops-perf"),
    [int]$Files = 10000
)
$ErrorActionPreference = "Stop"
$marker = ".gezik-ops-bench"
function Test-Ours { Test-Path (Join-Path $Work $marker) }
if ((Test-Path $Work) -and -not (Test-Ours)) {
    throw "Refusing to use ${Work}: it exists and was not created by this tool (no $marker inside). Pick a new folder."
}
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Push-Location $root
try {
    cargo build -p gezik-ops --release --example ops_bench
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    Write-Host "== Gezik engine"
    & "$root\target\release\examples\ops_bench.exe" $Work $Files
    if ($LASTEXITCODE -ne 0) { throw "ops_bench failed (exit $LASTEXITCODE)" }

    Write-Host "== Explorer (Shell.Application CopyHere, no UI)"
    $src = Join-Path $Work "src"
    $dst = Join-Path $Work "explorer"
    if ((Test-Path $Work) -and (Test-Ours)) { Remove-Item -Recurse -Force $Work }
    New-Item -ItemType Directory -Force $src | Out-Null
    New-Item -ItemType Directory -Force $dst | Out-Null
    New-Item -ItemType File -Force (Join-Path $Work $marker) | Out-Null
    $folders = [Math]::Max(1, [int][Math]::Floor($Files / 100))
    for ($f = 0; $f -lt $folders; $f++) {
        $folder = Join-Path $src ("folder{0:D3}" -f $f)
        New-Item -ItemType Directory -Force $folder | Out-Null
        # Exactly $Files in all: the first ($Files % $folders) folders get one more.
        $count = [int][Math]::Floor($Files / $folders) + $(if ($f -lt $Files % $folders) { 1 } else { 0 })
        for ($i = 0; $i -lt $count; $i++) {
            $size = 4096 + (($i * 7919 + $f * 104729) % (60 * 1024))
            [IO.File]::WriteAllBytes((Join-Path $folder ("file{0:D4}.bin" -f $i)), (New-Object byte[] $size))
        }
    }
    $shell = New-Object -ComObject Shell.Application
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $deadline = (Get-Date).AddMinutes(10)
    # 4: no progress dialog, 16: yes to all, 1024: no error UI. CopyHere returns at once:
    # wait until every file is there.
    $shell.Namespace($dst).CopyHere($src, 1044)
    $target = Join-Path $dst "src"
    while (-not (Test-Path $target) -or (Get-ChildItem -Recurse -File $target).Count -lt $Files) {
        if ((Get-Date) -gt $deadline) { throw "Explorer had not copied $Files files after 10 minutes" }
        Start-Sleep -Milliseconds 50
    }
    $watch.Stop()
    Write-Host ("copy {0} files: {1} ms" -f $Files, $watch.ElapsedMilliseconds)

    # A second, identical tree for the rd baseline.
    $second = "$target-rd"
    robocopy $target $second /E /MT:16 /NFL /NDL /NJH /NJS /NP | Out-Null
    if ($LASTEXITCODE -ge 8) { throw "robocopy failed (exit $LASTEXITCODE)" }

    Write-Host "== Explorer permanent delete (the Shell's delete, as Shift+Del, no UI)"
    Add-Type -AssemblyName Microsoft.VisualBasic
    $watch = [Diagnostics.Stopwatch]::StartNew()
    [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($target, 'OnlyErrorDialogs', 'DeletePermanently')
    $watch.Stop()
    if (Test-Path $target) { throw "the Shell did not delete $target" }
    Write-Host ("delete {0} files: {1} ms" -f $Files, $watch.ElapsedMilliseconds)

    Write-Host "== rd /s /q (delete baseline)"
    $watch = [Diagnostics.Stopwatch]::StartNew()
    cmd /c rd /s /q "$second"
    $watch.Stop()
    Write-Host ("delete {0} files: {1} ms" -f $Files, $watch.ElapsedMilliseconds)
}
finally {
    if (Test-Ours) { Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue }
    Pop-Location
}
