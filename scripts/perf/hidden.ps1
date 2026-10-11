# Private memory (working set / commit, MB) with the window shown, in the tray, shown again,
# minimized and restored, and how long after showing and restoring the window's pixels are the
# same as before hiding (the frame buffer is given back while hidden; see vendor/README.md).
# Windows only. Own config folder with the tray icon on; shows the window again by starting
# Gezik a second time (single instance). Brings the window to the front: run while the user is
# away or on another screen, and the window must be fully on screen.
# Usage: scripts/perf/hidden.ps1 [-Exe path\to\gezik.exe] [-Label name]
param([string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe", [string]$Label = "gezik")
. "$PSScriptRoot\_window.ps1"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class H {
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [StructLayout(LayoutKind.Sequential)] public struct L { public uint cb; public uint t; }
  [DllImport("user32.dll")] public static extern bool GetLastInputInfo(ref L l);
  public struct R { public int L, T, Rt, B; }
}
"@
[void][H]::SetProcessDPIAware()
# Waits until nobody has typed or moved the mouse for 5 s (the window is brought to the front).
function Idle { while ($true) { $l = New-Object H+L; $l.cb = 8; [void][H]::GetLastInputInfo([ref]$l); if (([Environment]::TickCount - $l.t) -ge 5000) { return }; Start-Sleep -Milliseconds 500 } }
function Mem($id) {
  Start-Sleep -Milliseconds 300
  $w = (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$id").WorkingSetPrivate / 1MB
  $c = (Get-Process -Id $id).PrivateMemorySize64 / 1MB
  "{0:N2}/{1:N2}" -f $w, $c
}
# The window below the tab strip and inside its frame (showing again adds a tab; the desktop
# shows through the frame).
function Shot($h) {
  $r = New-Object H+R; [void][H]::GetWindowRect($h, [ref]$r)
  $b = New-Object Drawing.Bitmap ($r.Rt - $r.L - 20), ($r.B - $r.T - 90)
  $g = [Drawing.Graphics]::FromImage($b); $g.CopyFromScreen($r.L + 10, $r.T + 70, 0, 0, $b.Size); $g.Dispose()
  $ms = New-Object IO.MemoryStream; $b.Save($ms, [Drawing.Imaging.ImageFormat]::Bmp); $b.Dispose()
  [Convert]::ToBase64String($ms.ToArray())
}
function Until-Same($h, $ref) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt 3000) { if ((Shot $h) -eq $ref) { return $sw.ElapsedMilliseconds }; Start-Sleep -Milliseconds 10 }
  return -1
}
$folder = Join-Path $env:TEMP "gezik-perf-hidden"
New-Item -ItemType Directory -Force $folder | Out-Null
for ($i = 0; $i -lt 20; $i++) { Set-Content -Path (Join-Path $folder "file$i.txt") -Value "$i" }
$dir = Join-Path $env:TEMP "gezik-perf-hidden-config"
Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $dir | Out-Null
[IO.File]::WriteAllText("$dir\state.toml", "[hints]`nf3-moved = true`n`n[tray]`ntold = true`n")
[IO.File]::WriteAllText("$dir\settings.toml", "[system]`ntray = true`n")
$env:GEZIK_CONFIG_DIR = $dir
$p = Start-Process $Exe -ArgumentList $folder -PassThru
try {
  $h = Find-GezikWindow $p.Id 10000
  if ($h -eq [IntPtr]::Zero) { throw "no window" }
  [void][H]::SetWindowPos($h, [IntPtr]::Zero, 200, 200, 1296, 839, 0x14)  # NOZORDER|NOACTIVATE
  Start-Sleep -Seconds 4
  $c = New-Object H+R; [void][H]::GetClientRect($h, [ref]$c)
  Idle; [void][H]::SetForegroundWindow($h); Start-Sleep -Seconds 2
  if ([H]::GetForegroundWindow() -ne $h) { throw "not in front" }
  $ref = Shot $h
  $shown = Mem $p.Id
  [void][H]::PostMessage($h, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)   # WM_CLOSE: to the tray
  Start-Sleep -Seconds 3
  if ([H]::IsWindowVisible($h)) { throw "still visible after close" }
  $tray = Mem $p.Id
  Idle
  Start-Process $Exe -ArgumentList $folder   # the running Gezik shows its window (a tab on the same folder)
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while (-not [H]::IsWindowVisible($h) -and $sw.ElapsedMilliseconds -lt 5000) { Start-Sleep -Milliseconds 5 }
  $t1 = Until-Same $h $ref
  $showMs = $sw.ElapsedMilliseconds
  Start-Sleep -Seconds 2
  $again = Mem $p.Id
  [void][H]::ShowWindow($h, 6)   # SW_MINIMIZE
  Start-Sleep -Seconds 3
  $min = Mem $p.Id
  Idle
  [void][H]::ShowWindow($h, 9)   # SW_RESTORE
  $t2 = Until-Same $h $ref
  Start-Sleep -Seconds 2
  $rest = Mem $p.Id
  "{0}: client {1}x{2} | shown {3} | tray {4} | shown again {5} (same pixels after {6} ms, {7} ms total) | minimized {8} | restored {9} (same pixels after {10} ms)" -f $Label, ($c.Rt - $c.L), ($c.B - $c.T), $shown, $tray, $again, $t1, $showMs, $min, $rest, $t2
} finally {
  Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
  Wait-Process -Id $p.Id -Timeout 10 -ErrorAction SilentlyContinue
  Remove-Item Env:GEZIK_CONFIG_DIR
  Remove-Item -Recurse -Force $folder, $dir -ErrorAction SilentlyContinue
}
