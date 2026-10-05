# Builds the 7-Zip downloads Gezik offers from 7-Zip's official releases (7-zip.org), one
# per platform, and prints the Rust lines for MANIFEST in crates/gezik-core/src/batch/tools.rs
# (also written to <Out>\manifest.rs.txt). Windows only; see scripts/tools/README.md.
# Usage: scripts/tools/prepare.ps1 -Out <folder> [-Version 26.03] [-Release 1] [-UpdateSources]
# Every official file must match its size and SHA-256 in scripts/tools/sources.sha256, or the
# script stops; -UpdateSources rewrites that file from what was downloaded (for a new version,
# after checking the files by hand). Signed Windows installers must carry Igor Pavlov's valid
# signature (7-Zip's installers have so far been unsigned; then only the pinned hash counts).
# Needs the installed 7-Zip (to unpack the Windows installers and write the zips) and Git for
# Windows' GNU tar and xz (to write the tar.xz files). Safe to run again: it replaces what it
# made before and leaves nothing behind in the temp folder.
param(
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Version = "26.03",
    [string]$Release = "1",
    [switch]$UpdateSources,
    [string]$SevenZip = "$env:ProgramFiles\7-Zip\7z.exe",
    [string]$Tar = "$env:ProgramFiles\Git\usr\bin\tar.exe",
    [string]$Xz = "$env:ProgramFiles\Git\mingw64\bin\xz.exe"
)

$ErrorActionPreference = "Stop"

foreach ($tool in @($SevenZip, $Tar, $Xz)) {
    if (-not (Test-Path $tool)) { throw "$tool not found" }
}

# Runs a program and stops the script when it fails.
function Invoke-Tool([string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "$Exe $($Arguments -join ' ') failed ($LASTEXITCODE)" }
}

$short = $Version -replace '\.', ''
$tag = "7zip-$Version-$Release"
$base = "https://github.com/wenlar/gezik-tools/releases/download/$tag"

# What each platform gets: the official file, our file, the programs in it (the one to run
# first) and the platform's name in tools.rs.
$builds = @(
    @{ Source = "7z$short-x64.exe"; Name = "7zip-$Version-windows-x64.zip"; Programs = @("7z.exe", "7z.dll"); Platform = "WindowsX64" },
    @{ Source = "7z$short-arm64.exe"; Name = "7zip-$Version-windows-arm64.zip"; Programs = @("7z.exe", "7z.dll"); Platform = "WindowsArm64" },
    @{ Source = "7z$short-mac.tar.xz"; Name = "7zip-$Version-macos-arm64.tar.xz"; Programs = @("7zz"); Platform = "MacArm64" },
    @{ Source = "7z$short-mac.tar.xz"; Name = "7zip-$Version-macos-x64.tar.xz"; Programs = @("7zz"); Platform = "MacX64" },
    @{ Source = "7z$short-linux-x64.tar.xz"; Name = "7zip-$Version-linux-x64.tar.xz"; Programs = @("7zz"); Platform = "LinuxX64" },
    @{ Source = "7z$short-linux-arm64.tar.xz"; Name = "7zip-$Version-linux-arm64.tar.xz"; Programs = @("7zz"); Platform = "LinuxArm64" }
)

New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$work = Join-Path ([IO.Path]::GetTempPath()) ("gezik-tools-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory $work | Out-Null

$sourcesFile = Join-Path $PSScriptRoot "sources.sha256"
# The pinned official files: "<sha256> <size> <name>" per line, # starts a comment.
$pinned = @{}
if (Test-Path $sourcesFile) {
    foreach ($line in [IO.File]::ReadAllLines($sourcesFile)) {
        if ($line -match '^\s*(#|$)') { continue }
        $hash, $size, $name = $line -split '\s+'
        $pinned[$name] = @{ Sha256 = $hash; Size = [long]$size }
    }
}

$lines = @()
$sources = @()
try {
    foreach ($name in ($builds | ForEach-Object { $_.Source } | Select-Object -Unique)) {
        $source = Join-Path $work $name
        Write-Host "Downloading $name"
        Invoke-Tool "curl.exe" @("-sSfL", "--proto", "=https", "-o", $source, "https://www.7-zip.org/a/$name")
        $size = (Get-Item $source).Length
        $sha256 = (Get-FileHash -Algorithm SHA256 $source).Hash.ToLowerInvariant()
        if (-not $UpdateSources) {
            $pin = $pinned[$name]
            if (-not $pin) { throw "$name is not in $sourcesFile (run with -UpdateSources for a new version)" }
            if ($pin.Sha256 -ne $sha256 -or $pin.Size -ne $size) {
                throw "$name is $size bytes, SHA-256 $sha256; $sourcesFile pins $($pin.Size) bytes, $($pin.Sha256)"
            }
        }
        if ($name.EndsWith(".exe")) {
            $signature = Get-AuthenticodeSignature $source
            if ($signature.Status -eq "NotSigned") {
                Write-Host "  $name is not signed; the pinned hash is the check"
            } elseif ($signature.Status -ne "Valid" -or $signature.SignerCertificate.Subject -notmatch "Igor Pavlov") {
                throw "$name has a bad signature: $($signature.Status), $($signature.SignerCertificate.Subject)"
            }
        }
        $sources += "$sha256 $size $name"
    }
    if ($UpdateSources) {
        $header = "# The official 7-Zip $Version files prepare.ps1 repackages (https://www.7-zip.org/a/): sha256 size name"
        [IO.File]::WriteAllText($sourcesFile, ((@($header) + $sources) -join "`n") + "`n")
        Write-Host "Wrote $sourcesFile"
    }
    foreach ($build in $builds) {
        $source = Join-Path $work $build.Source
        $unpacked = Join-Path $work ($build.Platform)
        New-Item -ItemType Directory $unpacked | Out-Null
        $target = Join-Path $Out $build.Name
        if (Test-Path $target) { Remove-Item -Force $target }
        $files = @($build.Programs) + "License.txt"
        Push-Location $unpacked
        try {
            if ($build.Name.EndsWith(".zip")) {
                # The installer is a 7z archive: take the programs and the licence out of it.
                Invoke-Tool $SevenZip (@("x", "-y", $source) + $files)
                Invoke-Tool $SevenZip (@("a", "-tzip", "-mx=9", $target) + $files)
            } else {
                # tar is given plain tars only: it would look for xz on its own PATH.
                Copy-Item $source "upstream.tar.xz"
                Invoke-Tool $Xz @("-d", "-f", "upstream.tar.xz")
                Invoke-Tool $Tar (@("-xf", "upstream.tar", "--force-local") + $files)
                Remove-Item "upstream.tar"
                # Windows has no Unix modes: the programs get 755, the licence 644.
                $common = @("--format=ustar", "--owner=0", "--group=0", "--numeric-owner", "--force-local")
                Invoke-Tool $Tar (@("-cf", "package.tar", "--mode=0755") + $common + $build.Programs)
                Invoke-Tool $Tar (@("-rf", "package.tar", "--mode=0644") + $common + "License.txt")
                Invoke-Tool $Xz @("-9", "-T1", "-f", "package.tar")
                Move-Item "package.tar.xz" $target
            }
        } finally {
            Pop-Location
        }
        $size = (Get-Item $target).Length
        $sha256 = (Get-FileHash -Algorithm SHA256 $target).Hash.ToLowerInvariant()
        $kind = if ($build.Name.EndsWith(".zip")) { "zip" } else { "tar.xz" }
        $programs = ($build.Programs | ForEach-Object { "`"$_`"" }) -join ", "
        Write-Host ("{0}  {1,9}  {2}" -f $sha256, $size, $build.Name)
        $lines += @(
            "    ToolBuild {",
            "        tool: Tool::SevenZip,",
            "        platform: Platform::$($build.Platform),",
            "        version: `"$Version`",",
            "        url: `"$base/$($build.Name)`",",
            "        size: $size,",
            "        sha256: `"$sha256`",",
            "        programs: &[$programs],",
            "        kind: `"$kind`",",
            "    },"
        )
    }
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}

$manifest = Join-Path $Out "manifest.rs.txt"
$notes = @("    // Made from the official 7-Zip $Version files (https://www.7-zip.org/a/), sha256 size name:")
$notes += $sources | ForEach-Object { "    // $_" }
$lines = $notes + $lines
[IO.File]::WriteAllText($manifest, (($lines -join "`n") + "`n"))
Write-Host ""
$lines | ForEach-Object { Write-Host $_ }
Write-Host ""
Write-Host "Written to $manifest"
