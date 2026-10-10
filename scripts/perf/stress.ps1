# Opens a huge folder, scrolls it with the mouse wheel and reports memory and CPU.
# Creates the test folder on first use. Windows only.
# Usage: scripts/perf/stress.ps1 [-Files 100000] [-Group type|date|size] [-Exe target\gezik-base.exe]
param(
    [string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe",
    [int]$Files = 100000,
    [string]$Dir = "$env:TEMP\gezik-stress-$Files",
    [ValidateSet("", "type", "date", "size")][string]$Group = ""
)

if (-not (Test-Path $Dir) -or (Get-ChildItem $Dir).Count -ne $Files) {
    New-Item -ItemType Directory -Force $Dir | Out-Null
    for ($i = 0; $i -lt $Files; $i++) { [IO.File]::WriteAllBytes("$Dir\file_$i.txt", [byte[]]::new(0)) }
}

Add-Type @"
using System; using System.Runtime.InteropServices;
public class GezikStress {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int x, int y, int d, IntPtr e);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  public struct R { public int L, T, Rt, B; }
}
"@
[GezikStress]::SetProcessDPIAware() | Out-Null
function Mem($id) { (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$id").WorkingSetPrivate / 1MB }

# A grouped run: a settings.toml of its own with [view] group. Written without a BOM.
$config = $null
if ($Group) {
    $config = "$env:TEMP\gezik-stress-config-$Group"
    New-Item -ItemType Directory -Force $config | Out-Null
    [IO.File]::WriteAllText("$config\settings.toml", "[view]`ngroup = `"$Group`"`n")
    $env:GEZIK_CONFIG_DIR = $config
}

$p = Start-Process $Exe -ArgumentList "`"$Dir`"" -PassThru
try {
Start-Sleep -Seconds 5
$loaded = Mem $p.Id
. "$PSScriptRoot\_window.ps1"
$h = Find-GezikWindow $p.Id 10000
if ($h -eq [IntPtr]::Zero) { throw "Gezik window not found" }
[GezikStress]::SetForegroundWindow($h) | Out-Null
$r = New-Object GezikStress+R; [GezikStress]::GetWindowRect($h, [ref]$r) | Out-Null
[GezikStress]::SetCursorPos([int](($r.L + $r.Rt) / 2), [int](($r.T + $r.B) / 2)) | Out-Null
Start-Sleep -Milliseconds 300
$p.Refresh(); $cpu0 = $p.TotalProcessorTime.TotalMilliseconds
$sw = [Diagnostics.Stopwatch]::StartNew()
for ($i = 0; $i -lt 60; $i++) { [GezikStress]::mouse_event(0x0800, 0, 0, -360, [IntPtr]::Zero); Start-Sleep -Milliseconds 16 }
Start-Sleep -Milliseconds 500
$p.Refresh()
"[group: {5}] {0:N0} files | after load {1:N1} MB | after scroll {2:N1} MB | scroll CPU {3:N0} ms over {4:N0} ms" -f $Files, $loaded, (Mem $p.Id), ($p.TotalProcessorTime.TotalMilliseconds - $cpu0), $sw.ElapsedMilliseconds, $(if ($Group) { $Group } else { "none" })
} finally {
    Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
    if ($config) { Remove-Item Env:GEZIK_CONFIG_DIR -ErrorAction SilentlyContinue }
}
