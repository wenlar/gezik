# Memory and handles with one, two and five windows of one process, and after closing the
# extra ones (spec 10 §9.3, §12, §13.1 10g). Windows only. Starts its own Gezik with its own
# config folder (one small folder as its tab), opens windows with Ctrl+N and closes each extra
# one with Ctrl+W on its only tab.
# The explicit test: after closing, private memory is within 0.3 MB of one window and the
# handles at most 5 more ("FAIL" otherwise; fewer handles than at the start is no leak). Each extra window's cost is shown beside its bound,
# its frame buffer (client width x height x 4, physical pixels, so the scale squared is in it)
# + 2.5 MB. -Cycles repeats open-to-five-and-close (a leak shows as a rising "after closing").
# Keys go only to Gezik's windows: before every key the script checks that the window in the
# foreground is one of this process's and stops if not ("Gezik lost the focus"). Needs an
# unlocked desktop.
# Usage: scripts/perf/windows.ps1 [-Wait 4] [-Cycles 3]
param([string]$Exe = "$PSScriptRoot\..\..\target\release\gezik.exe", [int]$Wait = 4, [int]$Cycles = 1)

. "$PSScriptRoot\_window.ps1"
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System; using System.Collections.Generic; using System.Runtime.InteropServices; using System.Text;
public class GezikWindows {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [StructLayout(LayoutKind.Sequential)] struct RECT { public int L, T, R, B; }
  const string EventTarget = "Winit Thread Event Target";
  // The process's visible windows with a client area (not winit's event target, nor a
  // zero-sized helper).
  public static List<IntPtr> Of(uint pid) {
    var list = new List<IntPtr>();
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid || !IsWindowVisible(h)) return true;
      var c = new StringBuilder(256); GetClassName(h, c, 256);
      var t = new StringBuilder(256); GetWindowText(h, t, 256);
      if (c.ToString().StartsWith(EventTarget) || t.ToString() == EventTarget) return true;
      RECT r; if (!GetClientRect(h, out r) || r.R - r.L < 100 || r.B - r.T < 100) return true;
      list.Add(h);
      return true;
    }, IntPtr.Zero);
    return list;
  }
  // The frame buffer of window h: client width x height x 4 bytes.
  public static long Buffer(IntPtr h) {
    RECT r; GetClientRect(h, out r);
    return (long)(r.R - r.L) * (r.B - r.T) * 4;
  }
}
"@

$root = Join-Path $env:TEMP "gezik-perf-windows"
New-Item -ItemType Directory -Force $root | Out-Null
for ($i = 0; $i -lt 20; $i++) { Set-Content -Path (Join-Path $root "file$i.txt") -Value "$i" }

$config = Join-Path $env:TEMP "gezik-perf-windows-config"
Remove-Item -Recurse -Force $config -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $config | Out-Null
# One tab on the folder; the F3 hint is marked shown so it writes nothing.
$lines = @("[session]", "active = 0", "", "[[session.tabs]]", ("path = '" + $root + "'"), "", "[hints]", "f3-moved = true")
# Without a BOM: Windows PowerShell 5.1's "utf8" writes one.
[IO.File]::WriteAllLines((Join-Path $config "state.toml"), [string[]]$lines, (New-Object Text.UTF8Encoding $false))

function Sample($id) {
    $mb = (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$id").WorkingSetPrivate / 1MB
    $proc = Get-Process -Id $id
    [pscustomobject]@{ Mb = $mb; Commit = $proc.PrivateMemorySize64 / 1MB; Handles = $proc.HandleCount }
}

function Show($label, $s, $base) {
    if ($base) {
        "{0,-14} {1,6:N1} MB private  {2,6:N1} MB commit  {3,5} handles  ({4:+0.0;-0.0} MB, {5:+0.0;-0.0} MB commit, {6:+0;-0} handles)" -f $label, $s.Mb, $s.Commit, $s.Handles, ($s.Mb - $base.Mb), ($s.Commit - $base.Commit), ($s.Handles - $base.Handles)
    } else {
        "{0,-14} {1,6:N1} MB private  {2,6:N1} MB commit  {3,5} handles" -f $label, $s.Mb, $s.Commit, $s.Handles
    }
}

$oldConfig = $env:GEZIK_CONFIG_DIR
$env:GEZIK_CONFIG_DIR = $config
$p = $null
try {
    $p = Start-Process $Exe -PassThru
    $first = Find-GezikWindow $p.Id 10000
    if ($first -eq [IntPtr]::Zero) { throw "no Gezik window within 10 s" }
    $exePath = (Get-Process -Id $p.Id).Path

    function Windows { @([GezikWindows]::Of([uint32]$p.Id)) }
    function Wait-Windows([int]$n) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        while ((Windows).Count -ne $n) {
            if ($sw.ElapsedMilliseconds -gt 10000) { throw "expected $n windows, there are $((Windows).Count)" }
            Start-Sleep -Milliseconds 50
        }
    }
    function Press($keys) {
        # Never type into another program's window (someone may have clicked elsewhere).
        $owner = [uint32]0
        [void][GezikWindows]::GetWindowThreadProcessId([GezikWindows]::GetForegroundWindow(), [ref]$owner)
        if ($owner -ne $p.Id) { throw "Gezik lost the focus" }
        [System.Windows.Forms.SendKeys]::SendWait($keys)
    }
    function Front($hwnd) {
        [void][GezikWindows]::SetForegroundWindow($hwnd)
        Start-Sleep -Milliseconds 300
        if ([GezikWindows]::GetForegroundWindow() -ne $hwnd) { throw "Gezik lost the focus" }
    }

    Start-Sleep -Seconds $Wait
    $base = Sample $p.Id
    Show "one window" $base
    $buffer = [GezikWindows]::Buffer($first) / 1MB
    "frame buffer   {0,6:N1} MB per window (bound per extra window: {1:N1} MB)" -f $buffer, ($buffer + 2.5)
    $fail = @()
    for ($c = 1; $c -le $Cycles; $c++) {
        if ($Cycles -gt 1) { "cycle $c" }
        Front $first
        Press "^n"
        Wait-Windows 2
        Start-Sleep -Seconds $Wait
        # Its own process and any it started, not every Gezik run from the same exe.
        $spawned = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($p.Id)" | Where-Object { $_.ExecutablePath -eq $exePath }).Count
        if ($spawned -ne 0 -or -not (Get-Process -Id $p.Id -ErrorAction SilentlyContinue)) {
            $fail += "two windows run in $($spawned + 1) processes (one expected)"
        }
        $two = Sample $p.Id
        Show "two windows" $two $base
        $extra = $two.Mb - $base.Mb
        "  extra window {0,6:N1} MB, bound {1:N1} MB {2}" -f $extra, ($buffer + 2.5), $(if ($extra -le $buffer + 2.5) { "ok" } else { "over" })
        for ($n = 3; $n -le 5; $n++) {
            # Ctrl+N in the window in front (the newest).
            Press "^n"
            Wait-Windows $n
        }
        Start-Sleep -Seconds $Wait
        $five = Sample $p.Id
        Show "five windows" $five $base
        "  per extra    {0,6:N1} MB, bound {1:N1} MB" -f (($five.Mb - $base.Mb) / 4), ($buffer + 2.5)
        # Ctrl+W on each extra window's only tab closes that window; the first one stays.
        foreach ($hwnd in (Windows | Where-Object { $_ -ne $first })) {
            $left = (Windows).Count - 1
            Front $hwnd
            Press "^w"
            Wait-Windows $left
        }
        Start-Sleep -Seconds $Wait
        $closed = Sample $p.Id
        Show "after closing" $closed $base
        if (-not (Get-Process -Id $p.Id -ErrorAction SilentlyContinue)) { throw "Gezik quit with its first window open" }
    }
    if ($closed.Mb - $base.Mb -gt 0.3) { $fail += "{0:N2} MB over one window after closing (0.3 allowed)" -f ($closed.Mb - $base.Mb) }
    if ($closed.Handles - $base.Handles -gt 5) { $fail += "{0:+0;-0} handles after closing (5 more allowed; fewer is fine)" -f ($closed.Handles - $base.Handles) }
    if ($fail.Count -eq 0) { "PASS" } else { $fail | ForEach-Object { "FAIL: $_" } }
} finally {
    if ($p) {
        Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
        Wait-Process -Id $p.Id -Timeout 10 -ErrorAction SilentlyContinue
    }
    Remove-Item -Recurse -Force $root, $config -ErrorAction SilentlyContinue
    # The calling shell keeps its own config folder (or none).
    if ($null -eq $oldConfig) { Remove-Item Env:GEZIK_CONFIG_DIR -ErrorAction SilentlyContinue } else { $env:GEZIK_CONFIG_DIR = $oldConfig }
}
