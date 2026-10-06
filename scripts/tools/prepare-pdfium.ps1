# Builds the pdfium downloads Gezik offers, one solid 7z per platform holding the pdfium library
# (pdfium.dll, libpdfium.dylib or libpdfium.so), LICENSE (the build scripts' MIT licence),
# licenses/ (PDFium's and its bundled libraries' licences) and SOURCE.txt (where it comes from),
# and prints the Rust lines for MANIFEST in crates/gezik-core/src/batch/tools.rs (also written
# to <Out>\manifest.rs.txt). Windows only; see scripts/tools/README.md.
# Usage: scripts/tools/prepare-pdfium.ps1 -Out <folder> [-Build 8086] [-Release 1] [-Cache <folder>] [-UpdateSources]
# The builds: Benoit Blanchon's pdfium-binaries, release chromium/<Build>, unmodified
# (https://github.com/bblanchon/pdfium-binaries), the plain ones (no V8, no XFA):
#   https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F<Build>/pdfium-<p>.tgz
#   for p in win-x64, win-arm64, mac-arm64, mac-x64, linux-x64, linux-arm64 (glibc)
# Every upstream file must match its size and SHA-256 in scripts/tools/sources.sha256 (where
# it is named pdfium-<Build>-<p>.tgz) and GitHub's asset digest, or the script stops.
# -UpdateSources rewrites the pdfium lines of sources.sha256 from what was downloaded (GitHub's
# digest is still checked). -Cache keeps the downloads in a folder and reuses them on the next
# run (they are checked again). Needs the installed 7-Zip.
param(
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Build = "8086",
    [string]$Release = "1",
    [string]$Cache,
    [switch]$UpdateSources,
    # PDFium's full version for this chromium build (the release's title).
    [string]$FullVersion = "157.0.8086.0",
    [string]$SevenZip = "$env:ProgramFiles\7-Zip\7z.exe"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $SevenZip)) { throw "$SevenZip not found" }

# Runs a program and stops the script when it fails.
function Invoke-Tool([string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "$Exe $($Arguments -join ' ') failed ($LASTEXITCODE)" }
}

function Get-Sha256([string]$Path) {
    (Get-FileHash -Algorithm SHA256 $Path).Hash.ToLowerInvariant()
}

# The SHA-256 GitHub gives for a release file (`assets[].digest`, "sha256:<hex>").
$releases = @{}
function Get-GitHubDigest([string]$Repo, [string]$Tag, [string]$Name) {
    $key = "$Repo/$Tag"
    if (-not $releases.ContainsKey($key)) {
        $json = & curl.exe -sSfL --proto "=https" -H "Accept: application/vnd.github+json" "https://api.github.com/repos/$Repo/releases/tags/$Tag"
        if ($LASTEXITCODE -ne 0) { throw "could not read the release $key" }
        $releases[$key] = ($json -join "`n") | ConvertFrom-Json
    }
    $asset = $releases[$key].assets | Where-Object { $_.name -eq $Name }
    if (-not $asset -or -not $asset.digest) { throw "$key has no digest for $Name" }
    if ($asset.digest -notmatch '^sha256:([0-9a-f]{64})$') { throw "$Name has an odd digest: $($asset.digest)" }
    $Matches[1]
}

$tag = "pdfium-$Build-$Release"
$base = "https://github.com/wenlar/gezik-tools/releases/download/$tag"
$upstreamTag = "chromium%2F$Build"
$upstreamPage = "https://github.com/bblanchon/pdfium-binaries/releases/tag/$upstreamTag"
$upstreamBase = "https://github.com/bblanchon/pdfium-binaries/releases/download/$upstreamTag"
# The licences every build carries in licenses/ (PDFium's and its bundled libraries').
$licenseCount = 17

# What each platform gets: the upstream file (`pdfium-<Upstream>.tgz`, ours is named
# `pdfium-<Build>-<Upstream>.tgz`; all start with "pdfium-", which marks this script's lines in
# sources.sha256), the library's path in it, and the platform's name in tools.rs. The x64
# libraries (PE, Mach-O and ELF alike) are packed with the plain x86 branch filter: for x86
# code 7-Zip picks BCJ2 at -mx=9, which Gezik's 7z reader does not decode. For arm64 it picks
# the ARM64 filter, which the reader decodes.
$packages = @(
    @{ Platform = "WindowsX64"; Label = "Windows x64"; Upstream = "win-x64"; Ours = "windows-x64"; Library = "bin/pdfium.dll"; Filter = "-mf=BCJ" },
    @{ Platform = "WindowsArm64"; Label = "Windows arm64"; Upstream = "win-arm64"; Ours = "windows-arm64"; Library = "bin/pdfium.dll" },
    @{ Platform = "MacArm64"; Label = "macOS arm64"; Upstream = "mac-arm64"; Ours = "macos-arm64"; Library = "lib/libpdfium.dylib" },
    @{ Platform = "MacX64"; Label = "macOS x64"; Upstream = "mac-x64"; Ours = "macos-x64"; Library = "lib/libpdfium.dylib"; Filter = "-mf=BCJ" },
    @{ Platform = "LinuxX64"; Label = "Linux x64"; Upstream = "linux-x64"; Ours = "linux-x64"; Library = "lib/libpdfium.so"; Filter = "-mf=BCJ" },
    @{ Platform = "LinuxArm64"; Label = "Linux arm64"; Upstream = "linux-arm64"; Ours = "linux-arm64"; Library = "lib/libpdfium.so" }
)
foreach ($pkg in $packages) {
    $pkg.File = "pdfium-$($pkg.Upstream).tgz"
    $pkg.Url = "$upstreamBase/$($pkg.File)"
    $pkg.Source = "pdfium-$Build-$($pkg.Upstream).tgz"
    $pkg.Name = "pdfium-$Build-$($pkg.Ours).7z"
}

New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$work = Join-Path ([IO.Path]::GetTempPath()) ("gezik-pdfium-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory $work | Out-Null
if ($Cache) {
    New-Item -ItemType Directory -Force $Cache | Out-Null
    $Cache = (Resolve-Path $Cache).Path
}
$downloads = if ($Cache) { $Cache } else { Join-Path $work "downloads" }
New-Item -ItemType Directory -Force $downloads | Out-Null

$sourcesFile = Join-Path $PSScriptRoot "sources.sha256"
# The pinned upstream files: "<sha256> <size> <name>" per line, # starts a comment.
$pinned = @{}
$kept = @()
if (Test-Path $sourcesFile) {
    foreach ($line in [IO.File]::ReadAllLines($sourcesFile)) {
        $mine = $line.StartsWith("# The pdfium")
        if ($line -notmatch '^\s*(#|$)') {
            $hash, $size, $name = $line -split '\s+'
            $pinned[$name] = @{ Sha256 = $hash; Size = [long]$size }
            $mine = $name.StartsWith("pdfium-")
        }
        if (-not $mine) { $kept += $line }
    }
}

$lines = @()
$sourceLines = @()
try {
    foreach ($pkg in $packages) {
        $name = $pkg.Source
        $file = Join-Path $downloads $name
        $pin = $pinned[$name]
        $reuse = (Test-Path $file) -and ($UpdateSources -or ($pin -and (Get-Item $file).Length -eq $pin.Size -and (Get-Sha256 $file) -eq $pin.Sha256))
        if ($reuse) {
            Write-Host "Using the downloaded $name"
        } else {
            Write-Host "Downloading $name"
            Invoke-Tool "curl.exe" @("-sSfL", "--proto", "=https", "--retry", "3", "-o", $file, $pkg.Url)
        }
        $size = (Get-Item $file).Length
        $sha256 = Get-Sha256 $file
        if (-not $UpdateSources) {
            if (-not $pin) { throw "$name is not in $sourcesFile (run with -UpdateSources for a new build)" }
            if ($pin.Sha256 -ne $sha256 -or $pin.Size -ne $size) {
                throw "$name is $size bytes, SHA-256 $sha256; $sourcesFile pins $($pin.Size) bytes, $($pin.Sha256)"
            }
        }
        # The publisher's own hash, every time.
        $digest = Get-GitHubDigest "bblanchon/pdfium-binaries" $upstreamTag $pkg.File
        if ($digest -ne $sha256) { throw "$name has SHA-256 $sha256; GitHub's digest is $digest" }
        Write-Host "  matches GitHub's digest"
        $pkg.Sha256 = $sha256
        $sourceLines += "$sha256 $size $name"
    }
    if ($UpdateSources) {
        $header = "# The pdfium chromium/$Build builds prepare-pdfium.ps1 repackages (addresses in the script): sha256 size name"
        while ($kept.Count -gt 0 -and $kept[-1] -match '^\s*$') { $kept = $kept[0..($kept.Count - 2)] }
        [IO.File]::WriteAllText($sourcesFile, ((@($kept) + $header + $sourceLines) -join "`n") + "`n")
        Write-Host "Wrote $sourcesFile"
    }

    foreach ($pkg in $packages) {
        $up = Join-Path $work "$($pkg.Platform)-upstream"
        $dir = Join-Path $work $pkg.Platform
        New-Item -ItemType Directory $up, $dir | Out-Null
        # tgz -> tar -> files.
        Invoke-Tool $SevenZip @("x", "-y", "-o$up", (Join-Path $downloads $pkg.Source))
        $tars = @(Get-ChildItem -File $up)
        if ($tars.Count -ne 1) { throw "$($pkg.Source) does not hold one tar" }
        $tar = $tars[0].FullName
        $files = Join-Path $up "files"
        Invoke-Tool $SevenZip @("x", "-y", "-o$files", $tar)

        # The build must be the one asked for, without JavaScript (V8) and XFA.
        $version = [IO.File]::ReadAllText((Join-Path $files "VERSION"))
        if ($version -notmatch "(?m)^BUILD=$Build\s*$") { throw "$($pkg.Source) is not chromium/$($Build): $version" }
        $gn = [IO.File]::ReadAllText((Join-Path $files "args.gn"))
        foreach ($off in @("pdf_enable_v8", "pdf_enable_xfa")) {
            if ($gn -notmatch "(?m)^\s*$off\s*=\s*false\s*$") { throw "$($pkg.Source) does not have $off = false: $gn" }
        }
        $licenses = Join-Path $files "licenses"
        $count = @(Get-ChildItem -File $licenses).Count
        if ($count -ne $licenseCount) { throw "$($pkg.Source) has $count licences in licenses/, not $licenseCount" }
        if (-not (Test-Path (Join-Path $licenses "pdfium.txt"))) { throw "$($pkg.Source) has no licenses/pdfium.txt" }

        # The library is moved, never changed (the macOS arm64 one carries its ad-hoc signature).
        $library = ($pkg.Library -split '/')[-1]
        $from = Join-Path $files ($pkg.Library -replace '/', '\')
        $librarySha256 = Get-Sha256 $from
        Move-Item $from (Join-Path $dir $library)
        Move-Item (Join-Path $files "LICENSE") (Join-Path $dir "LICENSE")
        Move-Item $licenses (Join-Path $dir "licenses")
        if ((Get-Sha256 (Join-Path $dir $library)) -ne $librarySha256) { throw "$library of $($pkg.Platform) changed" }

        $text = "PDFium $FullVersion (chromium/$Build) for $($pkg.Label), unmodified, from $upstreamPage (build scripts: MIT, LICENSE).`n" +
            "PDFium: https://pdfium.googlesource.com/pdfium/ (BSD-3-Clause and Apache-2.0, licenses/pdfium.txt); bundled libraries' licences in licenses/.`n" +
            "V8 and XFA are off.`n" +
            "Upstream file (SHA-256): $($pkg.Url)`n" +
            "  $($pkg.Sha256)`n" +
            "Repackaged for Gezik (https://github.com/wenlar/gezik-tools, release $tag).`n"
        [IO.File]::WriteAllText((Join-Path $dir "SOURCE.txt"), $text)
        # The same dates on every run, so the package comes out the same.
        $when = (Get-Item (Join-Path $dir $library)).LastWriteTimeUtc
        (Get-Item (Join-Path $dir "SOURCE.txt")).LastWriteTimeUtc = $when
        (Get-Item (Join-Path $dir "licenses")).LastWriteTimeUtc = $when

        $target = Join-Path $Out $pkg.Name
        if (Test-Path $target) { Remove-Item -Force $target }
        Push-Location $dir
        try {
            # One thread: the same file on any machine (with the same 7-Zip version).
            $pack = @("a", "-t7z", "-mx=9", "-ms=on", "-mmt=1")
            if ($pkg.Filter) { $pack += $pkg.Filter }
            Invoke-Tool $SevenZip ($pack + $target + @($library, "LICENSE", "licenses", "SOURCE.txt"))
        } finally {
            Pop-Location
        }
        Remove-Item -Recurse -Force $dir, $up
        $size = (Get-Item $target).Length
        $sha256 = Get-Sha256 $target
        Write-Host ("{0}  {1,9}  {2}" -f $sha256, $size, $pkg.Name)
        $lines += @(
            "    ToolBuild {",
            "        tool: Tool::Pdfium,",
            "        platform: Platform::$($pkg.Platform),",
            "        version: `"$Build`",",
            "        url: `"$base/$($pkg.Name)`",",
            "        size: $size,",
            "        sha256: `"$sha256`",",
            "        programs: &[`"$library`"],",
            "        kind: `"7z`",",
            "    },"
        )
    }
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}

$manifest = Join-Path $Out "manifest.rs.txt"
$notes = @("    // Made from these pdfium chromium/$Build builds (addresses in scripts/tools/prepare-pdfium.ps1), sha256 size name:")
$notes += $sourceLines | ForEach-Object { "    // $_" }
$lines = $notes + $lines
[IO.File]::WriteAllText($manifest, (($lines -join "`n") + "`n"))
Write-Host ""
$lines | ForEach-Object { Write-Host $_ }
Write-Host ""
Write-Host "Written to $manifest"
