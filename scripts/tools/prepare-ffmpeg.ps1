# Builds the ffmpeg downloads Gezik offers, one solid 7z per platform holding ffmpeg, ffprobe,
# LICENSE (the GNU GPL v3) and SOURCE.txt (where the build and its source come from), and
# prints the Rust lines for MANIFEST in crates/gezik-core/src/batch/tools.rs (also written to
# <Out>\manifest.rs.txt). Windows only; see scripts/tools/README.md.
# Usage: scripts/tools/prepare-ffmpeg.ps1 -Out <folder> [-Cache <folder>] [-UpdateSources]
# The builds (unmodified; only ffmpeg, ffprobe and the licence are kept):
#   Windows x64     gyan.dev essentials .7z (https://github.com/GyanD/codexffmpeg)
#   Windows arm64   BtbN winarm64 gpl, a month-end autobuild (kept two years)
#   Linux x64/arm64 BtbN linux64/linuxarm64 gpl, the same autobuild (static, glibc >= 2.28)
#   macOS arm64/x64 Martin Riedl's ffmpeg.zip + ffprobe.zip (Developer ID signed; kept as is)
# Every upstream file must match its size and SHA-256 in scripts/tools/sources.sha256 and the
# hash its publisher gives (GitHub's asset digest, or the .sha256 beside the file), or the
# script stops. -UpdateSources rewrites the ffmpeg lines of sources.sha256 from what was
# downloaded (the publisher's hash is still checked). -Cache keeps the downloads in a folder
# and reuses them on the next run (they are checked again). Needs the installed 7-Zip.
param(
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Version = "9.0.2",
    [string]$Release = "1",
    [switch]$UpdateSources,
    [string]$Cache,
    # BtbN: the month-end autobuild, the 9.0 branch build in it and its FFmpeg commit.
    [string]$BtbnTag = "autobuild-2026-09-30-13-08",
    [string]$BtbnBuild = "n9.0.2-17-g2a571b6068",
    [string]$BtbnCommit = "2a571b606854520cf89804d8030c8b328e621689",
    # The FFmpeg commit of the release tag (gyan.dev and Martin Riedl build the tag).
    [string]$TagCommit = "946fcce07b6dcd0331c8cc609192aeff5e1924f8",
    # Martin Riedl's build folders (`<timestamp>_<version>` on https://ffmpeg.martin-riedl.de/).
    [string]$RiedlArm64 = "1789931890_9.0.2",
    [string]$RiedlX64 = "1789931006_9.0.2",
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

# The SHA-256 in a `<file>.sha256` beside a download (`<hex>` or `<hex>  <name>`).
function Get-PublishedSha256([string]$Url) {
    $text = & curl.exe -sSfL --proto "=https" $Url
    if ($LASTEXITCODE -ne 0) { throw "could not read $Url" }
    if (($text -join " ") -notmatch '^\s*([0-9a-fA-F]{64})\b') { throw "$Url holds no SHA-256: $text" }
    $Matches[1].ToLowerInvariant()
}

$tag = "ffmpeg-$Version-$Release"
# BtbN names a build by its branch ("9.0").
$branch = $Version -replace '\.\d+$', ''
$base = "https://github.com/wenlar/gezik-tools/releases/download/$tag"
$gyanName = "ffmpeg-$Version-essentials_build"
$btbn = "https://github.com/BtbN/FFmpeg-Builds/releases/download/$BtbnTag"
$riedl = "https://ffmpeg.martin-riedl.de/download/macos"
$copying = "ffmpeg-n$Version-COPYING.GPLv3"

# The upstream files: our name for each (all start with "ffmpeg-", which marks this script's
# lines in sources.sha256), where it is, and how its publisher's hash is had.
$sources = @(
    @{ Name = "$gyanName.7z"; Url = "https://github.com/GyanD/codexffmpeg/releases/download/$Version/$gyanName.7z";
       Repo = "GyanD/codexffmpeg"; Tag = $Version; Sha256Url = "https://www.gyan.dev/ffmpeg/builds/packages/$gyanName.7z.sha256" },
    @{ Name = "ffmpeg-$BtbnBuild-winarm64-gpl-$branch.zip"; Repo = "BtbN/FFmpeg-Builds"; Tag = $BtbnTag },
    @{ Name = "ffmpeg-$BtbnBuild-linux64-gpl-$branch.tar.xz"; Repo = "BtbN/FFmpeg-Builds"; Tag = $BtbnTag },
    @{ Name = "ffmpeg-$BtbnBuild-linuxarm64-gpl-$branch.tar.xz"; Repo = "BtbN/FFmpeg-Builds"; Tag = $BtbnTag },
    @{ Name = "ffmpeg-$Version-riedl-macos-arm64-ffmpeg.zip"; Url = "$riedl/arm64/$RiedlArm64/ffmpeg.zip" },
    @{ Name = "ffmpeg-$Version-riedl-macos-arm64-ffprobe.zip"; Url = "$riedl/arm64/$RiedlArm64/ffprobe.zip" },
    @{ Name = "ffmpeg-$Version-riedl-macos-x64-ffmpeg.zip"; Url = "$riedl/amd64/$RiedlX64/ffmpeg.zip" },
    @{ Name = "ffmpeg-$Version-riedl-macos-x64-ffprobe.zip"; Url = "$riedl/amd64/$RiedlX64/ffprobe.zip" },
    # FFmpeg's own GPL v3 text: the LICENSE in every package (the builders' copies equal it).
    @{ Name = $copying; Url = "https://raw.githubusercontent.com/FFmpeg/FFmpeg/n$Version/COPYING.GPLv3" }
)
foreach ($source in $sources) {
    if ($source.Repo -and -not $source.Url) { $source.Url = "$btbn/$($source.Name)" }
    if ($source.Url.StartsWith("https://ffmpeg.martin-riedl.de/")) { $source.Sha256Url = "$($source.Url).sha256" }
}
$byName = @{}
foreach ($source in $sources) { $byName[$source.Name] = $source }

$btbnSource = "FFmpeg $BtbnBuild (the release/$branch branch after tag n$Version), commit $BtbnCommit`n" +
    "  https://github.com/FFmpeg/FFmpeg/commit/$BtbnCommit`n" +
    "Build scripts: https://github.com/BtbN/FFmpeg-Builds (GPL variant, static)"
$tagSource = "FFmpeg $Version (tag n$Version), commit $TagCommit`n" +
    "  https://github.com/FFmpeg/FFmpeg/commit/$TagCommit"
# What each platform gets: its upstream files and the paths of ffmpeg, ffprobe and the
# builder's licence in them (none: FFmpeg's), what SOURCE.txt says, and the platform's name
# in tools.rs. Windows x64 is packed with the plain x86 branch filter: for x86 programs 7-Zip
# picks BCJ2 at -mx=9, which Gezik's 7z reader does not decode (it would hand the archive to
# 7-Zip, which the user may not have).
$btbnDir = "ffmpeg-$BtbnBuild"
$builds = @(
    @{ Platform = "WindowsX64"; Label = "Windows x64"; Filter = "-mf=BCJ"; Name = "ffmpeg-$Version-windows-x64.7z"; Exe = ".exe"; Archive = "$gyanName.7z";
       Inside = @("$gyanName\bin\ffmpeg.exe", "$gyanName\bin\ffprobe.exe"); License = "$gyanName\LICENSE";
       Builder = "the essentials build by Gyan Doshi, https://www.gyan.dev/ffmpeg/builds/ (static, GPL v3)"; Source = "$tagSource`n" +
       "Builds: https://github.com/GyanD/codexffmpeg/releases/tag/$Version (the build configuration is in the builder's README.txt; no build script is published)" },
    @{ Platform = "WindowsArm64"; Label = "Windows arm64"; Name = "ffmpeg-$Version-windows-arm64.7z"; Exe = ".exe"; Archive = $sources[1].Name;
       Inside = @("$btbnDir-winarm64-gpl-$branch\bin\ffmpeg.exe", "$btbnDir-winarm64-gpl-$branch\bin\ffprobe.exe");
       License = "$btbnDir-winarm64-gpl-$branch\LICENSE.txt"; Builder = "BtbN's FFmpeg-Builds, $BtbnTag"; Source = $btbnSource },
    @{ Platform = "MacArm64"; Label = "macOS arm64"; Name = "ffmpeg-$Version-macos-arm64.7z"; Exe = ""; Zips = @($sources[4].Name, $sources[5].Name);
       Builder = "Martin Riedl, https://ffmpeg.martin-riedl.de/ (signed with his Developer ID)"; Source = "$tagSource`n" +
       "Build scripts: https://git.martin-riedl.de/ffmpeg/build-script" },
    @{ Platform = "MacX64"; Label = "macOS x64"; Name = "ffmpeg-$Version-macos-x64.7z"; Exe = ""; Zips = @($sources[6].Name, $sources[7].Name);
       Builder = "Martin Riedl, https://ffmpeg.martin-riedl.de/ (signed with his Developer ID)"; Source = "$tagSource`n" +
       "Build scripts: https://git.martin-riedl.de/ffmpeg/build-script" },
    @{ Platform = "LinuxX64"; Label = "Linux x64"; Name = "ffmpeg-$Version-linux-x64.7z"; Exe = ""; Archive = $sources[2].Name; Tar = $true;
       Inside = @("$btbnDir-linux64-gpl-$branch\bin\ffmpeg", "$btbnDir-linux64-gpl-$branch\bin\ffprobe");
       License = "$btbnDir-linux64-gpl-$branch\LICENSE.txt"; Builder = "BtbN's FFmpeg-Builds, $BtbnTag"; Source = $btbnSource },
    @{ Platform = "LinuxArm64"; Label = "Linux arm64"; Name = "ffmpeg-$Version-linux-arm64.7z"; Exe = ""; Archive = $sources[3].Name; Tar = $true;
       Inside = @("$btbnDir-linuxarm64-gpl-$branch\bin\ffmpeg", "$btbnDir-linuxarm64-gpl-$branch\bin\ffprobe");
       License = "$btbnDir-linuxarm64-gpl-$branch\LICENSE.txt"; Builder = "BtbN's FFmpeg-Builds, $BtbnTag"; Source = $btbnSource }
)

New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$work = Join-Path ([IO.Path]::GetTempPath()) ("gezik-ffmpeg-" + [guid]::NewGuid().ToString("N"))
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
        $mine = $line.StartsWith("# The ffmpeg")
        if ($line -notmatch '^\s*(#|$)') {
            $hash, $size, $name = $line -split '\s+'
            $pinned[$name] = @{ Sha256 = $hash; Size = [long]$size }
            $mine = $name.StartsWith("ffmpeg-")
        }
        if (-not $mine) { $kept += $line }
    }
}

$lines = @()
$sourceLines = @()
try {
    foreach ($source in $sources) {
        $name = $source.Name
        $file = Join-Path $downloads $name
        $pin = $pinned[$name]
        $reuse = (Test-Path $file) -and ($UpdateSources -or ($pin -and (Get-Item $file).Length -eq $pin.Size -and (Get-Sha256 $file) -eq $pin.Sha256))
        if ($reuse) {
            Write-Host "Using the downloaded $name"
        } else {
            Write-Host "Downloading $name"
            Invoke-Tool "curl.exe" @("-sSfL", "--proto", "=https", "--retry", "3", "-o", $file, $source.Url)
        }
        $size = (Get-Item $file).Length
        $sha256 = Get-Sha256 $file
        if (-not $UpdateSources) {
            if (-not $pin) { throw "$name is not in $sourcesFile (run with -UpdateSources for a new version)" }
            if ($pin.Sha256 -ne $sha256 -or $pin.Size -ne $size) {
                throw "$name is $size bytes, SHA-256 $sha256; $sourcesFile pins $($pin.Size) bytes, $($pin.Sha256)"
            }
        }
        # The publisher's own hashes, every time.
        if ($source.Repo) {
            $digest = Get-GitHubDigest $source.Repo $source.Tag ($source.Url -split '/')[-1]
            if ($digest -ne $sha256) { throw "$name has SHA-256 $sha256; GitHub's digest is $digest" }
            Write-Host "  matches GitHub's digest"
        }
        if ($source.Sha256Url) {
            $published = Get-PublishedSha256 $source.Sha256Url
            if ($published -ne $sha256) { throw "$name has SHA-256 $sha256; $($source.Sha256Url) says $published" }
            Write-Host "  matches $($source.Sha256Url)"
        }
        $sourceLines += "$sha256 $size $name"
    }
    if ($UpdateSources) {
        $header = "# The ffmpeg $Version builds prepare-ffmpeg.ps1 repackages (addresses in the script): sha256 size name"
        while ($kept.Count -gt 0 -and $kept[-1] -match '^\s*$') { $kept = $kept[0..($kept.Count - 2)] }
        [IO.File]::WriteAllText($sourcesFile, ((@($kept) + $header + $sourceLines) -join "`n") + "`n")
        Write-Host "Wrote $sourcesFile"
    }
    $license = Join-Path $downloads $copying
    $licenseSha256 = Get-Sha256 $license

    foreach ($build in $builds) {
        $dir = Join-Path $work $build.Platform
        New-Item -ItemType Directory $dir | Out-Null
        $programs = @("ffmpeg$($build.Exe)", "ffprobe$($build.Exe)")
        if ($build.Zips) {
            foreach ($zip in $build.Zips) { Invoke-Tool $SevenZip @("e", "-y", "-o$dir", (Join-Path $downloads $zip)) }
            # Martin Riedl's programs carry his Developer ID signature, which must stay intact.
            foreach ($program in $programs) {
                $bytes = [IO.File]::ReadAllBytes((Join-Path $dir $program))
                if (-not [Text.Encoding]::ASCII.GetString($bytes).Contains("Developer ID Application: Martin Riedl")) {
                    throw "$program of $($build.Platform) is not signed by Martin Riedl"
                }
            }
        } else {
            $archive = Join-Path $downloads $build.Archive
            if ($build.Tar) {
                Invoke-Tool $SevenZip @("x", "-y", "-o$work", $archive)
                $archive = Join-Path $work ([IO.Path]::GetFileNameWithoutExtension($build.Archive))
            }
            Invoke-Tool $SevenZip (@("e", "-y", "-o$dir", $archive) + $build.Inside + $build.License)
            if ($build.Tar) { Remove-Item -Force $archive }
            # The builder's licence must be FFmpeg's GPL v3 text, which every package carries.
            $theirs = Join-Path $dir ($build.License -split '\\')[-1]
            if ((Get-Sha256 $theirs) -ne $licenseSha256) { throw "$($build.License) is not FFmpeg's COPYING.GPLv3" }
            Remove-Item -Force $theirs
        }
        foreach ($program in $programs) {
            if (-not (Test-Path (Join-Path $dir $program))) { throw "$program is missing for $($build.Platform)" }
        }
        $upstream = if ($build.Zips) { $build.Zips } else { @($build.Archive) }
        $text = "ffmpeg and ffprobe for $($build.Label), unmodified, from $($build.Builder).`n" +
            "$($build.Source)`n" +
            "FFmpeg's source: https://ffmpeg.org/download.html (git: https://git.ffmpeg.org/ffmpeg.git)`n" +
            "Licence: GNU General Public License version 3 (LICENSE).`n" +
            "Upstream files (SHA-256):`n"
        foreach ($name in $upstream) {
            $text += "  $($byName[$name].Url)`n    $(($sourceLines | Where-Object { $_.EndsWith(" $name") }) -replace ' .*$', '')`n"
        }
        $text += "Repackaged for Gezik (https://github.com/wenlar/gezik-tools, release $tag) with only these files kept.`n"
        [IO.File]::WriteAllText((Join-Path $dir "SOURCE.txt"), $text)
        Copy-Item $license (Join-Path $dir "LICENSE")
        # The same dates on every run, so the package comes out the same.
        $when = (Get-Item (Join-Path $dir $programs[0])).LastWriteTimeUtc
        foreach ($extra in @("LICENSE", "SOURCE.txt")) { (Get-Item (Join-Path $dir $extra)).LastWriteTimeUtc = $when }

        $target = Join-Path $Out $build.Name
        if (Test-Path $target) { Remove-Item -Force $target }
        Push-Location $dir
        try {
            # One thread: the same file on any machine, and ffmpeg and ffprobe in one LZMA2
            # stream (with more, 7-Zip cuts it into blocks that cannot share matches).
            $pack = @("a", "-t7z", "-mx=9", "-ms=on", "-mmt=1")
            if ($build.Filter) { $pack += $build.Filter }
            Invoke-Tool $SevenZip ($pack + $target + $programs + @("LICENSE", "SOURCE.txt"))
        } finally {
            Pop-Location
        }
        Remove-Item -Recurse -Force $dir
        $size = (Get-Item $target).Length
        $sha256 = Get-Sha256 $target
        $list = ($programs | ForEach-Object { "`"$_`"" }) -join ", "
        Write-Host ("{0}  {1,9}  {2}" -f $sha256, $size, $build.Name)
        $lines += @(
            "    ToolBuild {",
            "        tool: Tool::Ffmpeg,",
            "        platform: Platform::$($build.Platform),",
            "        version: `"$Version`",",
            "        url: `"$base/$($build.Name)`",",
            "        size: $size,",
            "        sha256: `"$sha256`",",
            "        programs: &[$list],",
            "        kind: `"7z`",",
            "    },"
        )
    }
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}

$manifest = Join-Path $Out "manifest.rs.txt"
$notes = @("    // Made from these ffmpeg $Version builds (addresses in scripts/tools/prepare-ffmpeg.ps1), sha256 size name:")
$notes += $sourceLines | ForEach-Object { "    // $_" }
$lines = $notes + $lines
[IO.File]::WriteAllText($manifest, (($lines -join "`n") + "`n"))
Write-Host ""
$lines | ForEach-Object { Write-Host $_ }
Write-Host ""
Write-Host "Written to $manifest"
