# Tool downloads

Gezik offers to download tools it does not build in (7-Zip and ffmpeg now; pdfium later). The
downloads live as GitHub release files in the public repository `wenlar/gezik-tools`;
`crates/gezik-core/src/batch/tools.rs` (`MANIFEST`) pins each file's address, size and SHA-256,
and Gezik refuses a download whose hash differs.

## Repository layout

`wenlar/gezik-tools` holds no code, only releases. One release per tool version, tagged
`<tool>-<version>-<n>` (`n` counts repackagings of the same version), e.g. `7zip-26.03-1`:

| File | Platform | Inside |
|---|---|---|
| `7zip-26.03-windows-x64.zip` | Windows x64 | `7z.exe`, `7z.dll`, `License.txt` |
| `7zip-26.03-windows-arm64.zip` | Windows arm64 | `7z.exe`, `7z.dll`, `License.txt` |
| `7zip-26.03-macos-arm64.tar.xz` | macOS arm64 | `7zz` (universal), `License.txt` |
| `7zip-26.03-macos-x64.tar.xz` | macOS x64 | the same file as macos-arm64 |
| `7zip-26.03-linux-x64.tar.xz` | Linux x64 | `7zz`, `License.txt` |
| `7zip-26.03-linux-arm64.tar.xz` | Linux arm64 | `7zz`, `License.txt` |

The programs are 7-Zip's official, unmodified builds from https://www.7-zip.org/ (the
Windows ones come out of the official installers); only the packaging is ours.

## Making the files

On Windows, with 7-Zip installed and Git for Windows (its GNU tar and xz):

```powershell
powershell -ExecutionPolicy Bypass -File scripts/tools/prepare.ps1 -Out D:\Work\gezik-tools\7zip-26.03-1 -Version 26.03
```

It downloads the official files from `https://www.7-zip.org/a/` and checks each against its
size and SHA-256 in `sources.sha256` (committed), stopping on any difference. A signed Windows
installer must also carry Igor Pavlov's valid Authenticode signature; the 26.03 installers are
unsigned, so for them the pinned hash is the only check. It then writes the six packages to
`-Out`, prints their sizes and SHA-256s and the Rust lines for `MANIFEST`, and writes those
lines, headed by the official files' hashes, to `<Out>\manifest.rs.txt`.

The packages are reproducible: for 26.03, two runs into fresh folders gave byte-identical
files (all six compared with `cmp`) and the same `manifest.rs.txt`. They depend on the tools'
versions too (7-Zip 24.09's zip writer, Git for Windows' GNU tar 1.35 and xz), so a run on
another machine may differ; publish the files of one run and pin those.

The 26.03 files are kept in `D:\Work\gezik-tools\7zip-26.03-1\` (outside the repository; no
binaries are committed).

Check them with Gezik's own reader (installs every package in the folder through the download
task):

```bash
GEZIK_TOOLS_DIR='D:\Work\gezik-tools\7zip-26.03-1' cargo test -p gezik-batch --test tools prepared
```

## Publishing

With the GitHub CLI (`gh auth status` signed in as an owner of `wenlar`). Once:

```bash
gh repo create wenlar/gezik-tools --public --description "Tool downloads for Gezik (official builds, repackaged)"
```

Each release, from the folder `prepare.ps1` wrote (for 26.03, `D:\Work\gezik-tools\7zip-26.03-1\`):

```bash
cd /d/Work/gezik-tools/7zip-26.03-1
gh release create 7zip-26.03-1 --repo wenlar/gezik-tools --title "7-Zip 26.03" \
  7zip-26.03-windows-x64.zip 7zip-26.03-windows-arm64.zip \
  7zip-26.03-macos-arm64.tar.xz 7zip-26.03-macos-x64.tar.xz \
  7zip-26.03-linux-x64.tar.xz 7zip-26.03-linux-arm64.tar.xz \
  --notes "7-Zip 26.03 by Igor Pavlov, official unmodified builds from https://www.7-zip.org/, repackaged for Gezik. 7-Zip is under the GNU LGPL 2.1 or later; some code is under the BSD 3-clause and BSD 2-clause licences, and its RAR code is under the GNU LGPL with the unRAR licence restriction. The full licence is License.txt in each file. Source: https://www.7-zip.org/download.html"
```

Then check that what GitHub serves is what `MANIFEST` pins:

```bash
for f in windows-x64.zip windows-arm64.zip macos-arm64.tar.xz macos-x64.tar.xz linux-x64.tar.xz linux-arm64.tar.xz; do
  curl -sSfL "https://github.com/wenlar/gezik-tools/releases/download/7zip-26.03-1/7zip-26.03-$f" | sha256sum
done
```

Each hash must equal the `sha256` of that platform in `MANIFEST` (and `manifest.rs.txt`).

## Updating to a new version

1. Run `prepare.ps1 -Version <new> -UpdateSources -Out D:\Work\gezik-tools\7zip-<new>-1` (take
   the version from https://www.7-zip.org/download.html). It rewrites `sources.sha256` from
   what it downloaded: compare those hashes with the ones on 7-Zip's GitHub release
   (https://github.com/ip7z/7zip/releases) before committing the file.
2. Put the printed lines in `MANIFEST` in place of the old ones and run
   `cargo test -p gezik-core tools` and the `GEZIK_TOOLS_DIR` test above.
3. Publish the release `7zip-<new>-1` as above and check the hashes.
4. Update the version in `THIRD-PARTY.md`, commit and release Gezik.

Never replace the files of a published release: Gezik versions already out pin their hashes.
To repackage the same 7-Zip version, publish `7zip-<version>-2` and point `MANIFEST` at it.
Gezik removes an older installed version once a newer one is in place.

## ffmpeg

ffmpeg must be 9.0 or newer: only 9.x turns iPhone grid HEICs into the whole picture (older
versions give one 512x512 tile without an error). The release `ffmpeg-9.0.2-1` holds one solid
7z per platform (`kind = "7z"`), each with `ffmpeg`, `ffprobe` (`.exe` on Windows), `LICENSE`
(FFmpeg's GPL v3 text) and `SOURCE.txt` (the FFmpeg commit, the builder's scripts and the
upstream files with their SHA-256):

| File | Platform | Build | Size |
|---|---|---|---|
| `ffmpeg-9.0.2-windows-x64.7z` | Windows x64 | gyan.dev 9.0.2 essentials | 31,594,971 B |
| `ffmpeg-9.0.2-windows-arm64.7z` | Windows arm64 | BtbN `n9.0.2-17-g2a571b6068` winarm64 gpl | 31,490,863 B |
| `ffmpeg-9.0.2-macos-arm64.7z` | macOS arm64 | Martin Riedl 9.0.2 (Developer ID signed) | 21,142,760 B |
| `ffmpeg-9.0.2-macos-x64.7z` | macOS x64 | Martin Riedl 9.0.2 (Developer ID signed) | 26,971,531 B |
| `ffmpeg-9.0.2-linux-x64.7z` | Linux x64 | BtbN `n9.0.2-17-g2a571b6068` linux64 gpl (glibc 2.28+) | 53,607,273 B |
| `ffmpeg-9.0.2-linux-arm64.7z` | Linux arm64 | BtbN `n9.0.2-17-g2a571b6068` linuxarm64 gpl (glibc 2.28+) | 45,627,266 B |

BtbN builds the release branch, not the tag: `n9.0.2-17-g2a571b6068` is 9.0.2 plus 17 fixes of
the 9.0 branch (`ffmpeg -version` prints `n9.0.2-17-g2a571b6068`, read as 9.0). Its month-end autobuilds (here
`autobuild-2026-09-30-13-08`) are kept for two years; the daily ones only for two weeks, so
always take a month-end one. The programs are not changed: Martin Riedl's macOS programs keep
his signature (never re-sign them), and Gezik makes the programs executable after unpacking on
macOS and Linux (7-Zip on Windows stores no Unix modes).

Make the files (Windows, with 7-Zip installed; about 600 MB of downloads and 15 minutes of
packing):

```powershell
powershell -ExecutionPolicy Bypass -File scripts/tools/prepare-ffmpeg.ps1 -Out D:\Work\gezik-tools\ffmpeg-9.0.2-1
```

It downloads the upstream files (addresses at the top of the script; `-Cache <folder>` keeps
them for the next run) and checks each against its size and SHA-256 in `sources.sha256` and
against its publisher's hash: GitHub's asset digest for gyan.dev and BtbN (and gyan.dev's own
`.sha256`), the `.sha256` beside each of Martin Riedl's zips. It checks that the builders'
licence is FFmpeg's `COPYING.GPLv3` (pinned too) and that the macOS programs carry Martin
Riedl's Developer ID signature, then packs each platform with
`7z a -t7z -mx=9 -ms=on -mmt=1` (Windows x64 adds `-mf=BCJ`: for x86 programs 7-Zip picks
the BCJ2 filter at `-mx=9`, which Gezik's 7z reader does not decode; the plain BCJ filter costs
1.3 MB). One thread keeps the files the same on any machine (with the same 7-Zip version;
24.09 here) and lets ffmpeg and ffprobe share one LZMA2 stream: with several threads 7-Zip
splits it into blocks and the Linux x64 file grows from 54 to 70 MB. Two runs gave
byte-identical files. It prints sizes, SHA-256s and the `MANIFEST`
lines, also written to `<Out>\manifest.rs.txt`.

Check them as above (`GEZIK_TOOLS_DIR='D:\Work\gezik-tools\ffmpeg-9.0.2-1'`); this also runs
this machine's ffmpeg from the install and checks it is 9.0 or newer (about a minute in a
debug build).

Publish (the notes name the licence, FFmpeg's source and the builders):

```bash
cd /d/Work/gezik-tools/ffmpeg-9.0.2-1
gh release create ffmpeg-9.0.2-1 --repo wenlar/gezik-tools --title "ffmpeg 9.0.2" \
  ffmpeg-9.0.2-windows-x64.7z ffmpeg-9.0.2-windows-arm64.7z \
  ffmpeg-9.0.2-macos-arm64.7z ffmpeg-9.0.2-macos-x64.7z \
  ffmpeg-9.0.2-linux-x64.7z ffmpeg-9.0.2-linux-arm64.7z \
  --notes "ffmpeg 9.0.2 (with ffprobe), unmodified third-party builds repackaged for Gezik, one solid 7z per platform: Windows x64 from the gyan.dev essentials build (https://www.gyan.dev/ffmpeg/builds/, https://github.com/GyanD/codexffmpeg); Windows arm64 and Linux x64/arm64 from BtbN's GPL builds n9.0.2-17-g2a571b6068, the 9.0 branch with fixes after 9.0.2 (https://github.com/BtbN/FFmpeg-Builds, autobuild-2026-09-30-13-08); macOS arm64/x64 from Martin Riedl's builds, Developer ID signed, signature kept (https://ffmpeg.martin-riedl.de/, build scripts https://git.martin-riedl.de/ffmpeg/build-script). These builds are under the GNU General Public License version 3. The full licence is LICENSE in each file; SOURCE.txt names the FFmpeg commit, the builder's scripts and the upstream files with their SHA-256. FFmpeg source: https://ffmpeg.org/download.html (tag n9.0.2: https://github.com/FFmpeg/FFmpeg/tree/n9.0.2; BtbN's commit: https://github.com/FFmpeg/FFmpeg/commit/2a571b606854520cf89804d8030c8b328e621689)"
```

and check the served files against `MANIFEST` as for 7-Zip (`ffmpeg-9.0.2-$f` for `$f` in
`windows-x64.7z windows-arm64.7z macos-arm64.7z macos-x64.7z linux-x64.7z linux-arm64.7z`).

For a new version: set `-Version` and the build parameters at the top of the script (BtbN's
month-end tag, build name and commit; the tag's commit; Martin Riedl's two folders, from
https://ffmpeg.martin-riedl.de/), run it with `-UpdateSources` (it rewrites only the ffmpeg
lines of `sources.sha256`; the publishers' hashes are still checked), then follow steps 2-4 of
"Updating to a new version" with `ffmpeg-<new>-1`. Never go below 9.0.
