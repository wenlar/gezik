# 9b11 probe: which program answers "show in folder" (read-only; writes no registry).
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/probe/show-in-folder.ps1 -Mode api|api-full|select|open -Target <file>
#   api      SHOpenFolderAndSelectItems(parent folder, 1 child)  (what Chromium/Electron call)
#   api-full SHOpenFolderAndSelectItems(full pidl, 0 children)
#   select   %SystemRoot%\explorer.exe /select,"<file>"
#   open     ShellExecute of the parent folder (control: the folder verb's handler)
# Prints the HRESULT and the top-level windows that appeared or changed title, with their process.
param([Parameter(Mandatory)][string]$Mode, [Parameter(Mandatory)][string]$Target)
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices; using System.Collections.Generic;
public static class P {
  [DllImport("shell32.dll", CharSet=CharSet.Unicode)] public static extern int SHParseDisplayName(string n, IntPtr bc, out IntPtr pidl, uint sfgao, out uint o);
  [DllImport("shell32.dll")] public static extern int SHOpenFolderAndSelectItems(IntPtr f, uint c, IntPtr[] a, uint flags);
  [DllImport("shell32.dll")] public static extern IntPtr ILFindLastID(IntPtr p);
  [DllImport("shell32.dll")] public static extern IntPtr ILClone(IntPtr p);
  [DllImport("shell32.dll")] public static extern bool ILRemoveLastID(IntPtr p);
  public delegate bool EW(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EW f, IntPtr l);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public static List<string> Wins() {
    var r = new List<string>();
    EnumWindows((h, l) => {
      if (!IsWindowVisible(h)) return true;
      var s = new System.Text.StringBuilder(512); GetWindowText(h, s, 512);
      if (s.Length == 0) return true;
      uint pid; GetWindowThreadProcessId(h, out pid); string n = "?";
      try { n = System.Diagnostics.Process.GetProcessById((int)pid).ProcessName; } catch {}
      r.Add(n + "|" + s); return true; }, IntPtr.Zero);
    return r; }
}
"@
$before = [P]::Wins()
$pidl = [IntPtr]::Zero; $o = 0
switch ($Mode) {
  'api' {
    "parse hr=" + [P]::SHParseDisplayName($Target, [IntPtr]::Zero, [ref]$pidl, 0, [ref]$o)
    $dir = [P]::ILClone($pidl); [P]::ILRemoveLastID($dir) | Out-Null
    "SHOpenFolderAndSelectItems(folder, 1 child) hr=0x{0:X8}" -f [P]::SHOpenFolderAndSelectItems($dir, 1, @([P]::ILFindLastID($pidl)), 0)
  }
  'api-full' {
    "parse hr=" + [P]::SHParseDisplayName($Target, [IntPtr]::Zero, [ref]$pidl, 0, [ref]$o)
    "SHOpenFolderAndSelectItems(full pidl, 0 children) hr=0x{0:X8}" -f [P]::SHOpenFolderAndSelectItems($pidl, 0, $null, 0)
  }
  'select' { Start-Process "$env:SystemRoot\explorer.exe" "/select,`"$Target`""; 'explorer /select started' }
  'open' { Start-Process (Split-Path $Target); 'ShellExecute of the folder started' }
  default { throw "unknown mode $Mode" }
}
Start-Sleep -Seconds 4
"New or retitled windows: " + ((@([P]::Wins()) | Where-Object { $before -notcontains $_ }) -join ' || ')
