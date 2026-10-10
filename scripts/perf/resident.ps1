# Idle memory with the tray icon and the global shortcut off and on, and hidden (9b9). Windows only.
# Usage: scripts/perf/resident.ps1 [-Runs 3]
# Each case runs Gezik with a config folder of its own under %TEMP% (the user's settings are not
# touched); the shortcut is Ctrl+Shift+F11, held only while measuring. Run while the user is away.
param([string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe", [int]$Runs = 3)

. "$PSScriptRoot\_window.ps1"

function Measure-Case([string]$Name, [string]$Settings, [string[]]$Arguments, [bool]$Window) {
    $dir = Join-Path $env:TEMP ("gezik-resident-" + ($Name -replace '[^a-z]', ''))
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $dir | Out-Null
    Set-Content -Path (Join-Path $dir 'settings.toml') -Value $Settings -Encoding utf8
    $env:GEZIK_CONFIG_DIR = $dir
    $mem = @()
    try {
        for ($i = 0; $i -lt $Runs; $i++) {
            $p = if ($Arguments) { Start-Process $Exe -ArgumentList $Arguments -PassThru } else { Start-Process $Exe -PassThru }
            try {
                if ($Window -and (Find-GezikWindow $p.Id 10000) -eq [IntPtr]::Zero) { throw "no Gezik window ($Name)" }
                Start-Sleep -Seconds 4
                $mem += (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$($p.Id)").WorkingSetPrivate / 1MB
            } finally {
                Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
            }
            Start-Sleep -Milliseconds 500
        }
    } finally {
        Remove-Item Env:GEZIK_CONFIG_DIR -ErrorAction SilentlyContinue
    }
    "{0,-30} {1:N2} MB" -f $Name, ($mem | Measure-Object -Average).Average
}

Measure-Case "off (the defaults)" "" @() $true
Measure-Case "tray + shortcut, window shown" "[system]`ntray = true`nhotkey = `"ctrl+shift+f11`"`n" @() $true
Measure-Case "tray, --background (hidden)" "[system]`ntray = true`n" @('--background') $false
"Gezik was stopped with Stop-Process: its tray icon may stay until the pointer passes over it."
