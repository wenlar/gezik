# 9b4 probe: reads the file manager registrations (never writes them), checks the .reg
# forms Gezik will emit under HKCU\Software\GezikTest-probe, and writes a
# restore.reg of the current state for the screen tests. GUI-free: safe with the user present.
# Saved as UTF-8 with BOM: Windows PowerShell 5.1 reads a BOM-less script in the ANSI code page.
param([string]$Out = "$env:TEMP\gezik-9b4-probe")
New-Item -ItemType Directory -Force $Out | Out-Null
$clsid = '{52205fd8-5dfb-447d-801a-d0b52f2e83e1}'
$classes = 'Directory', 'Drive', 'Folder'
"== Windows $([Environment]::OSVersion.Version) $((Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion').DisplayVersion)"
foreach ($k in ($classes | ForEach-Object { "$_\shell" }) + "CLSID\$clsid" + 'Folder\shell\open' + 'CompressedFolder\shell') {
    "== HKCU\Software\Classes\$k"; reg query "HKCU\Software\Classes\$k" /s 2>&1 | Select-Object -First 30
    "== HKCR\$k"; reg query "HKCR\$k" /s 2>&1 | Select-Object -First 30
}

# Backups (reading only) and restore.reg: Gezik's keys deleted, the three defaults as now.
foreach ($k in $classes + "CLSID\$clsid") {
    reg export "HKCU\Software\Classes\$k" "$Out\backup-$($k -replace '[\\{}]', '_').reg" /y 2>&1 | Out-Null
}
$restore = @('Windows Registry Editor Version 5.00', '')
foreach ($c in $classes) {
    $now = (Get-ItemProperty -LiteralPath "HKCU:\Software\Classes\$c\shell" -ErrorAction SilentlyContinue).'(default)'
    $restore += "[HKEY_CURRENT_USER\Software\Classes\$c\shell]"
    $restore += if ($null -eq $now) { '@=-' } else { '@="' + ($now -replace '\\', '\\' -replace '"', '\"') + '"' }
    $restore += '', "[-HKEY_CURRENT_USER\Software\Classes\$c\shell\gezik]", ''
}
if (-not (Test-Path -LiteralPath "HKCU:\Software\Classes\CLSID\$clsid")) {
    $restore += "[-HKEY_CURRENT_USER\Software\Classes\CLSID\$clsid]", ''
} else {
    $restore += "; HKCU had $clsid before: import backup-CLSID_$($clsid -replace '[{}]', '_').reg instead", ''
}
$restore | Set-Content -Encoding Unicode "$Out\restore.reg"

# The .reg forms Gezik emits, on a throwaway key: values with \ " and Unicode, @=-, "x"=-, [-key].
$t = 'HKEY_CURRENT_USER\Software\GezikTest-probe'
@"
Windows Registry Editor Version 5.00

[$t\Directory\shell\gezik\command]
@="\"D:\\Araçlar\\Gezik Ç\\gezik.exe\" --shell \"%1\""
"DelegateExecute"=""

[$t\Directory\shell]
@="gezik"
"@ | Set-Content -Encoding Unicode "$Out\t-apply.reg"
reg import "$Out\t-apply.reg"; "== after t-apply"; reg query $t /s
@"
Windows Registry Editor Version 5.00

[$t\Directory\shell]
@=-

[$t\Directory\shell\gezik\command]
"DelegateExecute"=-

[-$t\Directory\shell\gezik]
"@ | Set-Content -Encoding Unicode "$Out\t-restore.reg"
reg import "$Out\t-restore.reg"; "== after t-restore (expect: shell key, no values, no gezik)"; reg query $t /s
reg delete 'HKCU\Software\GezikTest-probe' /f | Out-Null

"== restore.reg: $Out\restore.reg"
