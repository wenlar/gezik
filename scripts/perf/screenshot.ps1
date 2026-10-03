# Captures the Gezik window to a PNG and prints its size and position.
# Usage: scripts/perf/screenshot.ps1 -Out shot.png
param([Parameter(Mandatory)] [string]$Out, [string]$Title = "Gezik")

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class GezikShot {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  public struct R { public int L, T, Rt, B; }
}
"@
[GezikShot]::SetProcessDPIAware() | Out-Null
$h = [IntPtr]::Zero
for ($i = 0; $i -lt 100 -and $h -eq [IntPtr]::Zero; $i++) {
    # [NullString]::Value: PowerShell would pass $null as "" and FindWindow would fail.
    $h = [GezikShot]::FindWindow([NullString]::Value, $Title)
    Start-Sleep -Milliseconds 50
}
if ($h -eq [IntPtr]::Zero) { throw "Window '$Title' not found" }
[GezikShot]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 400
$r = New-Object GezikShot+R
[GezikShot]::GetWindowRect($h, [ref]$r) | Out-Null
$bmp = New-Object Drawing.Bitmap ($r.Rt - $r.L), ($r.B - $r.T)
[Drawing.Graphics]::FromImage($bmp).CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
$bmp.Save($Out)
"{0}x{1} at {2},{3} -> {4}" -f ($r.Rt - $r.L), ($r.B - $r.T), $r.L, $r.T, $Out
