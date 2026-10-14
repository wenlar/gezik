# Screen-test driver for Gezik on Windows: one entry point for launching a test Gezik and
# sending it input, so a single permission rule can cover screen tests. Each call is one verb:
#   & scripts/gui/gui.ps1 launch <exe> <config dir> [<start folder>]   starts it, remembers its PID
#   & scripts/gui/gui.ps1 place [<width> <height>]       moves it onto the primary monitor at (40,40)
#   & scripts/gui/gui.ps1 click <x> <y> [right|double]   screen coordinates
#   & scripts/gui/gui.ps1 drag <x1> <y1> <x2> <y2> [shift|ctrl]
#   & scripts/gui/gui.ps1 wheel <x> <y> <notches>       positive scrolls up, negative down
#   & scripts/gui/gui.ps1 key <key> [<key>...]          names: enter esc tab f2 up down ctrl+c shift+f10 …
#   & scripts/gui/gui.ps1 hold <key> <times>            key-downs with auto-repeat, then one key-up
#   & scripts/gui/gui.ps1 paste <text>                  puts text on the clipboard and sends Ctrl+V
#   & scripts/gui/gui.ps1 shot <name> [window|screen]   PNG into <config dir>\..\shots
#   & scripts/gui/gui.ps1 focus                         the focused element (UIA): type, name, value
#   & scripts/gui/gui.ps1 find <name substring>         visible UIA elements of the window with that name
#   & scripts/gui/gui.ps1 idle                          ms since the last input, and the foreground PID
#   & scripts/gui/gui.ps1 clip [save|restore|get]
#   & scripts/gui/gui.ps1 stop                          ends the launched Gezik (that PID only)
# Safety, enforced here: the exe and the config must be under %TEMP%\gezik-gui-*; input goes
# only to the launched Gezik when it (or a window of it) is in front; Ctrl+, (1Password's
# global hotkey) and Ctrl+A are refused; input stops if someone else typed in the last 1.5 s.
param([Parameter(Mandatory)][string]$Verb, [Parameter(ValueFromRemainingArguments)][string[]]$Rest)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing, System.Windows.Forms, UIAutomationClient, UIAutomationTypes
Add-Type @"
using System; using System.Runtime.InteropServices;
public class GuiIn {
  [StructLayout(LayoutKind.Sequential)] public struct LII { public uint cb; public uint t; }
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Rt, B; }
  [DllImport("user32.dll")] public static extern bool GetLastInputInfo(ref LII l);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int x, int y, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool rp);
  public static uint Idle() { var l = new LII(); l.cb = 8; GetLastInputInfo(ref l); return (uint)Environment.TickCount - l.t; }
  public static uint FgPid() { uint p; GetWindowThreadProcessId(GetForegroundWindow(), out p); return p; }
}
"@
[GuiIn]::SetProcessDPIAware() | Out-Null
. "$PSScriptRoot\..\perf\_window.ps1"

$Root = [IO.Path]::GetFullPath($env:TEMP).TrimEnd('\')
$State = Join-Path $Root 'gezik-gui-state.txt'   # "<pid>`n<config dir>`n<last sent ticks>"

function Inside-Test([string]$path) {
    $full = [IO.Path]::GetFullPath($path)
    $rel = $full.Substring(0, [Math]::Min($full.Length, $Root.Length + 1))
    $rest = $full.Substring([Math]::Min($full.Length, $Root.Length + 1))
    if ($rel -ne "$Root\" -or -not $rest.StartsWith('gezik-gui-', 'OrdinalIgnoreCase')) { throw "not under $Root\gezik-gui-*: $full" }
    if ($full -like "$env:APPDATA*") { throw "refusing the real settings folder" }
    $full
}
function Saved { if (-not (Test-Path $State)) { throw 'nothing launched' }; ,(Get-Content $State) }
function GPid { [int](Saved)[0] }
function Save([int]$p, [string]$cfg, [long]$sent) { Set-Content $State "$p`n$cfg`n$sent" }
function Window { $h = [GezikWin]::ForProcess([uint32](GPid)); if ($h -eq [IntPtr]::Zero) { throw 'Gezik has no window' }; $h }
function Guard {
    $p = GPid
    if (-not (Get-Process -Id $p -ErrorAction SilentlyContinue)) { throw "GUARD: Gezik $p is gone" }
    $fg = [GuiIn]::FgPid()
    if ($fg -ne $p) { throw "GUARD: the foreground window belongs to PID $fg, not Gezik $p" }
    $s = Saved; $sent = [long]$s[2]; $idle = [GuiIn]::Idle()
    $sinceOurs = [Environment]::TickCount - $sent
    if ($sent -ne 0 -and $idle -lt 1500 -and $sinceOurs -gt $idle + 300) { throw "GUARD: someone else gave input $idle ms ago" }
}
function Mark { $s = Saved; Save ([int]$s[0]) $s[1] ([Environment]::TickCount) }
function Down([byte]$vk) { [GuiIn]::keybd_event($vk, 0, 0, [UIntPtr]::Zero) }
function Up([byte]$vk) { [GuiIn]::keybd_event($vk, 0, 2, [UIntPtr]::Zero) }
$Named = @{ enter = 0x0D; esc = 0x1B; tab = 0x09; space = 0x20; back = 0x08; delete = 0x2E; home = 0x24; end = 0x23
    up = 0x26; down = 0x28; left = 0x25; right = 0x27; pageup = 0x21; pagedown = 0x22; apps = 0x5D; insert = 0x2D }
1..12 | ForEach-Object { $Named["f$_"] = 0x6F + $_ }
function Vk([string]$k) {
    if ($Named.ContainsKey($k)) { return [byte]$Named[$k] }
    if ($k.Length -eq 1 -and $k -match '[a-z0-9]') { return [byte][char]$k.ToUpper() }
    throw "unknown key: $k"
}
function Press([string]$chord) {
    $parts = $chord.ToLower().Split('+'); $key = $parts[-1]; $mods = $parts[0..($parts.Count - 2)] | Where-Object { $_ -ne $key }
    if ($parts.Count -eq 1) { $mods = @() }
    if ($mods -contains 'ctrl' -and ($key -eq ',' -or $key -eq 'a')) { throw "refused: ctrl+$key" }
    $mv = @{ ctrl = 0x11; shift = 0x10; alt = 0x12; win = 0x5B }
    Guard
    foreach ($m in $mods) { Down $mv[$m] }
    $vk = Vk $key; Down $vk; Up $vk
    [array]::Reverse($mods); foreach ($m in $mods) { Up $mv[$m] }
    Mark; Start-Sleep -Milliseconds 120
}

switch ($Verb) {
    'launch' {
        $exe = Inside-Test $Rest[0]; $cfg = Inside-Test $Rest[1]
        $start = if ($Rest.Count -gt 2) { Inside-Test $Rest[2] } else { $null }
        New-Item -ItemType Directory -Force $cfg | Out-Null
        $env:GEZIK_CONFIG_DIR = $cfg
        $argv = if ($start) { @("`"$start`"") } else { @() }
        $p = if ($argv) { Start-Process $exe -ArgumentList $argv -PassThru } else { Start-Process $exe -PassThru }
        Save $p.Id $cfg 0
        for ($i = 0; $i -lt 100 -and [GezikWin]::ForProcess([uint32]$p.Id) -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 100 }
        "pid $($p.Id)"
    }
    'place' {
        $w = if ($Rest.Count -ge 2) { [int]$Rest[0] } else { 1200 }; $h = if ($Rest.Count -ge 2) { [int]$Rest[1] } else { 800 }
        $hw = Window; [GuiIn]::MoveWindow($hw, 40, 40, $w, $h, $true) | Out-Null; [GuiIn]::SetForegroundWindow($hw) | Out-Null
        Start-Sleep -Milliseconds 300; $r = New-Object GuiIn+R; [GuiIn]::GetWindowRect($hw, [ref]$r) | Out-Null
        "window $($r.L),$($r.T) $($r.Rt - $r.L)x$($r.B - $r.T)"
    }
    'click' {
        Guard; [GuiIn]::SetCursorPos([int]$Rest[0], [int]$Rest[1]) | Out-Null; Start-Sleep -Milliseconds 80
        $how = if ($Rest.Count -gt 2) { $Rest[2] } else { 'left' }
        $n = if ($how -eq 'double') { 2 } else { 1 }
        for ($i = 0; $i -lt $n; $i++) {
            if ($how -eq 'right') { [GuiIn]::mouse_event(0x8, 0, 0, 0, [UIntPtr]::Zero); [GuiIn]::mouse_event(0x10, 0, 0, 0, [UIntPtr]::Zero) }
            else { [GuiIn]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero); [GuiIn]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero) }
        }
        Mark; Start-Sleep -Milliseconds 150
    }
    'drag' {
        Guard; $mod = if ($Rest.Count -gt 4) { @{ shift = 0x10; ctrl = 0x11 }[$Rest[4]] } else { $null }
        [GuiIn]::SetCursorPos([int]$Rest[0], [int]$Rest[1]) | Out-Null; Start-Sleep -Milliseconds 80
        if ($mod) { Down $mod }
        [GuiIn]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero)
        for ($i = 1; $i -le 20; $i++) {
            $x = [int]$Rest[0] + ([int]$Rest[2] - [int]$Rest[0]) * $i / 20; $y = [int]$Rest[1] + ([int]$Rest[3] - [int]$Rest[1]) * $i / 20
            [GuiIn]::SetCursorPos($x, $y) | Out-Null; Start-Sleep -Milliseconds 30
        }
        Start-Sleep -Milliseconds 300; [GuiIn]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero)
        if ($mod) { Up $mod }
        Mark
    }
    'wheel' {
        # wheel <x> <y> <notches>: positive scrolls up, negative down (120 per notch).
        Guard; [GuiIn]::SetCursorPos([int]$Rest[0], [int]$Rest[1]) | Out-Null; Start-Sleep -Milliseconds 80
        $delta = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]([int]$Rest[2] * 120)), 0)  # negative = down
        [GuiIn]::mouse_event(0x0800, 0, 0, $delta, [UIntPtr]::Zero)
        Mark; Start-Sleep -Milliseconds 150
    }
    'key' { foreach ($k in $Rest) { Press $k } }
    'hold' {
        Guard; $vk = Vk $Rest[0].ToLower()
        for ($i = 0; $i -lt [int]$Rest[1]; $i++) { Down $vk; Start-Sleep -Milliseconds 35 }
        Up $vk; Mark
    }
    'paste' {
        [Windows.Forms.Clipboard]::SetText(($Rest -join ' ')); Press 'ctrl+v'
    }
    'shot' {
        $dir = Join-Path (Split-Path (Saved)[1]) 'shots'; New-Item -ItemType Directory -Force $dir | Out-Null
        $screen = $Rest.Count -gt 1 -and $Rest[1] -eq 'screen'
        if ($screen) { $b = [Windows.Forms.Screen]::PrimaryScreen.Bounds; $L = $b.X; $T = $b.Y; $W = $b.Width; $H = $b.Height }
        else { $r = New-Object GuiIn+R; [GuiIn]::GetWindowRect((Window), [ref]$r) | Out-Null; $L = $r.L; $T = $r.T; $W = $r.Rt - $r.L; $H = $r.B - $r.T }
        $bmp = New-Object Drawing.Bitmap $W, $H
        [Drawing.Graphics]::FromImage($bmp).CopyFromScreen($L, $T, 0, 0, $bmp.Size)
        $f = Join-Path $dir "$($Rest[0]).png"; $bmp.Save($f); "$f (origin $L,$T)"
    }
    'focus' {
        $e = [Windows.Automation.AutomationElement]::FocusedElement
        $v = try { $e.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value } catch { '' }
        "pid $($e.Current.ProcessId) type $($e.Current.ControlType.ProgrammaticName) name '$($e.Current.Name)' value '$v'"
    }
    'find' {
        $w = [Windows.Automation.AutomationElement]::FromHandle((Window))
        $all = $w.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
        foreach ($e in $all) {
            if ($e.Current.Name -like "*$($Rest -join ' ')*" -and -not $e.Current.IsOffscreen) {
                $b = $e.Current.BoundingRectangle
                "$($e.Current.ControlType.ProgrammaticName) '$($e.Current.Name)' at $([int]($b.X + $b.Width / 2)),$([int]($b.Y + $b.Height / 2)) size $([int]$b.Width)x$([int]$b.Height)"
            }
        }
    }
    'idle' { "idle $([GuiIn]::Idle()) ms; foreground pid $([GuiIn]::FgPid())" }
    'clip' {
        $keep = Join-Path $Root 'gezik-gui-clipboard.txt'
        switch ($Rest[0]) {
            'save' { Set-Content $keep ([Windows.Forms.Clipboard]::GetText()) -NoNewline; 'saved' }
            'restore' { if (Test-Path $keep) { $t = Get-Content $keep -Raw; if ($t) { [Windows.Forms.Clipboard]::SetText($t) } else { [Windows.Forms.Clipboard]::Clear() }; 'restored' } }
            default { [Windows.Forms.Clipboard]::GetText() }
        }
    }
    'stop' {
        $p = GPid; Stop-Process -Id $p -ErrorAction SilentlyContinue; Remove-Item $State; "stopped $p"
    }
    default { throw "unknown verb: $Verb" }
}
