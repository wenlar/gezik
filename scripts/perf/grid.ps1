# Memory with the grid view and thumbnails, in a folder of pictures. Windows only.
# Creates the pictures once (in %TEMP%), starts its own Gezik there in grid view with its
# own config folder, pages through the folder, and reports the private working set.
# Needs an unlocked, interactive desktop for the PgDn paging (SendKeys); on a locked
# desktop it still reports memory, but only for the first screenful of thumbnails.
# Usage: scripts/perf/grid.ps1 [-Count 1000] [-Wait 8]
param([string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe", [int]$Count = 1000, [int]$Wait = 8)

. "$PSScriptRoot\_window.ps1"
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -Name Focus -Namespace GezikGrid -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();'

$dir = Join-Path $env:TEMP "gezik-perf-grid-$Count"
if (-not (Test-Path $dir) -or (Get-ChildItem $dir).Count -ne $Count) {
    New-Item -ItemType Directory -Force $dir | Out-Null
    $bmp = New-Object System.Drawing.Bitmap 1600, 1200
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    for ($i = 0; $i -lt $Count; $i++) {
        $g.Clear([System.Drawing.Color]::FromArgb(($i * 37) % 256, ($i * 91) % 256, ($i * 53) % 256))
        $g.FillEllipse([System.Drawing.Brushes]::White, ($i % 40) * 30, ($i % 30) * 30, 400, 300)
        $bmp.Save((Join-Path $dir ("photo{0:D4}.jpg" -f $i)), [System.Drawing.Imaging.ImageFormat]::Jpeg)
    }
    $g.Dispose(); $bmp.Dispose()
}

$config = Join-Path $env:TEMP "gezik-perf-grid-config"
New-Item -ItemType Directory -Force $config | Out-Null
Set-Content -Encoding utf8 (Join-Path $config "settings.toml") "[view]`nmode = `"grid`"`n"
$env:GEZIK_CONFIG_DIR = $config
$p = Start-Process $Exe -ArgumentList "`"$dir`"" -PassThru
try {
    $hwnd = Find-GezikWindow $p.Id 10000
    if ($hwnd -eq [IntPtr]::Zero) { throw "no Gezik window within 10 s" }
    Start-Sleep -Seconds $Wait
    # Page through the whole folder so every thumbnail is loaded once.
    [void][GezikGrid.Focus]::SetForegroundWindow($hwnd)
    $paged = $true
    try {
        for ($i = 0; $i -lt 60; $i++) {
            # Never type into another window (someone may have clicked elsewhere).
            if ([GezikGrid.Focus]::GetForegroundWindow() -ne $hwnd) { throw "Gezik lost the focus" }
            [System.Windows.Forms.SendKeys]::SendWait("{PGDN}"); Start-Sleep -Milliseconds 150
        }
    } catch { $paged = $false; Write-Warning "PgDn paging did not run ($($_.Exception.Message)); is the desktop locked?" }
    Start-Sleep -Seconds $Wait
    $mb = (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$($p.Id)").WorkingSetPrivate / 1MB
    "grid, $Count pictures, paging {0}: Task Manager memory {1:N1} MB (target <= 50 MB)" -f $(if ($paged) { "ran" } else { "DID NOT RUN" }), $mb
} finally {
    Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
    Remove-Item Env:GEZIK_CONFIG_DIR -ErrorAction SilentlyContinue
}
