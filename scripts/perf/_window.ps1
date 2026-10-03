# Shared helper (dot-sourced by the other scripts): finds a process's window by PID,
# so it keeps working when the title changes ("<place> - Gezik").
# Picks the largest visible top-level window owned by the process.
Add-Type @"
using System; using System.Collections.Generic; using System.Runtime.InteropServices;
public class GezikWin {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
  [StructLayout(LayoutKind.Sequential)] struct RECT { public int L, T, R, B; }
  public static IntPtr ForProcess(uint pid) {
    IntPtr best = IntPtr.Zero; long bestArea = 0;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid || !IsWindowVisible(h)) return true;
      RECT r; if (!GetWindowRect(h, out r)) return true;
      long a = (long)(r.R - r.L) * (r.B - r.T);
      if (a > bestArea) { bestArea = a; best = h; }
      return true;
    }, IntPtr.Zero);
    return best;
  }
}
"@

# Waits up to $TimeoutMs for the process to show a window; returns IntPtr.Zero on timeout.
function Find-GezikWindow([int]$ProcessId, [int]$TimeoutMs = 10000) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    do {
        $h = [GezikWin]::ForProcess([uint32]$ProcessId)
        if ($h -ne [IntPtr]::Zero) { return $h }
        Start-Sleep -Milliseconds 5
    } while ($sw.ElapsedMilliseconds -lt $TimeoutMs)
    return [IntPtr]::Zero
}
