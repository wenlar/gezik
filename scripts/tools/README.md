# Tool downloads

Gezik offers to download tools it does not build in (7-Zip now; ffmpeg and pdfium later). The
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
powershell -ExecutionPolicy Bypass -File scripts/tools/prepare.ps1 -Out C:\temp\gezik-tools -Version 26.03
```

It downloads the official files from `https://www.7-zip.org/a/`, writes the six packages to
`-Out`, prints their sizes and SHA-256s and the Rust lines for `MANIFEST`, and writes those
lines to `<Out>\manifest.rs.txt`. Running it again gives byte-identical files.

Check them with Gezik's own reader (installs every package through the download task):

```bash
GEZIK_TOOLS_DIR='C:\temp\gezik-tools' cargo test -p gezik-batch --test tools prepared
```

## Publishing

With the GitHub CLI (`gh auth status` signed in as an owner of `wenlar`). Once:

```bash
gh repo create wenlar/gezik-tools --public --description "Tool downloads for Gezik (official builds, repackaged)"
```

Each release (from the `-Out` folder):

```bash
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

1. Run `prepare.ps1 -Version <new>` (take the version from https://www.7-zip.org/download.html).
2. Put the printed lines in `MANIFEST` in place of the old ones and run
   `cargo test -p gezik-core tools` and the `GEZIK_TOOLS_DIR` test above.
3. Publish the release `7zip-<new>-1` as above and check the hashes.
4. Update the version in `THIRD-PARTY.md`, commit and release Gezik.

Never replace the files of a published release: Gezik versions already out pin their hashes.
To repackage the same 7-Zip version, publish `7zip-<version>-2` and point `MANIFEST` at it.
Gezik removes an older installed version once a newer one is in place.
