# Releasing Gezik

The packages are built by `.github/workflows/release.yml`. They are **not signed** yet (no
Authenticode certificate, no Apple Developer ID), so Windows and macOS warn on first start;
see [What users see](#what-users-see-without-signing).

## Cutting a release

1. Set the version in the root `Cargo.toml` (`[workspace.package] version`), update the README
   if needed, and merge to `master`. Run CI first (`gh workflow run ci.yml --ref master`).
2. Tag the merge and push the tag:
   ```sh
   git tag -a v0.1.0 -m "Gezik 0.1.0"
   git push origin v0.1.0
   ```
   The tag must be `v` + the `Cargo.toml` version, or the workflow stops at once.
3. The workflow builds the packages below, writes `SHA256SUMS.txt` and opens a **draft**
   GitHub Release with all of them and generated notes.
4. Download the packages from the draft and try them on each system (for macOS the
   [release checklist](superpowers/notes/checklists/release-macos.md)). Edit the notes, then
   publish the draft. **Publishing sends the update to every Gezik** (see
   [Automatic updates](#automatic-updates)): don't publish packages you have not tried.

To build the packages without a tag or a release (a test run), start the workflow by hand:
`gh workflow run release.yml --ref <branch> -f version=0.1.0`. The packages are then the run's
artifacts (Actions ▸ the run ▸ Artifacts).

## The packages

| File | Contents |
|---|---|
| `gezik-<v>-windows-x64.zip` | `gezik.exe`, `LICENSE.md`, `THIRD-PARTY.md`, `README.md`. Settings in `%APPDATA%\gezik`. |
| `gezik-<v>-windows-x64-portable.zip` | The same plus an empty `gezik-portable` file: settings go to `gezik-data\` next to `gezik.exe` (a USB stick, say). |
| `gezik-<v>-macos-universal.dmg` | `Gezik.app` (arm64 + x86_64, ad-hoc signed), the licence files and a link to Applications. |
| `gezik-<v>-macos-universal.app.tar.gz` | The same `Gezik.app` alone, for the automatic update (simpler to unpack than a disk image). |
| `gezik-<v>-linux-x64.tar.gz` | `gezik`, the licence files and `README-linux.txt` (what it needs). Built on Debian trixie by `scripts/linux/package.sh`, which also starts it under Xvfb. |
| `gezik-<v>-linux-x64.AppImage` | The same binary as one file, made by `scripts/linux/appimage.sh` with pinned `appimagetool` 1.9.1 and type2 runtime 20251108 (SHA-256 checked). Needs FUSE 2 or 3 to start (`--appimage-extract-and-run` works without). |
| `SHA256SUMS.txt` | SHA-256 of every file above. |

`Gezik.app` is made by `scripts/macos/make-app.sh`, which also runs on a Mac by hand
(`cargo build --release -p gezik && scripts/macos/make-app.sh`). Portable mode on macOS: put a
`gezik-portable` file next to `Gezik.app` (not inside it).

Portable mode for the AppImage: put a `gezik-portable` file next to the `.AppImage`. Its icon is
`crates/gezik/icons/gezik-256.png`, a 256 px render of `gezik.svg`.

## Automatic updates

Gezik looks at `https://api.github.com/repos/wenlar/gezik/releases` at most once a day and
installs a newer release by itself (`[update]` in settings.toml; see
[Configuration](configuration.md#updates)). Only a **published** release counts: a pushed tag
or a draft reaches no one. Gezik takes the file of its own kind (the zip, the portable zip,
`.app.tar.gz`, the AppImage or the tarball), checks it against `SHA256SUMS.txt` and runs it with
`--version` before it swaps it in at exit. So:

- **Publishing the draft sends the update to everyone** on that channel (a pre-release to those
  running a pre-release, a release to everyone). Try the packages first.
- To withdraw a release, delete it or turn it back into a draft. A Gezik that has not
  downloaded it yet never will; one that has installs it when it closes.
- To undo a release, publish a newer one (a higher version number). The updater never goes to
  a lower version.
- The file names above are what Gezik looks for: renaming a package, or leaving one or
  `SHA256SUMS.txt` out, stops the update for that kind of install (those users only see that a
  new version is out).
- Updates start with the first release that has the updater (0.1.0-rc.3): older ones are
  updated by hand once.

## What users see without signing

**Windows.** A downloaded zip carries the "from the internet" mark. On first start of
`gezik.exe` SmartScreen says "Windows protected your PC". Click **More info**, then
**Run anyway**. To avoid it for the whole folder: right-click the zip ▸ Properties ▸ tick
**Unblock** before extracting, or in PowerShell `Unblock-File gezik.exe`.

**macOS.** The app is signed ad hoc, not notarized. Opening it from the dmg (or after copying to
Applications) shows "Apple could not verify "Gezik" is free of malware…". Click **Done**, then
System Settings ▸ Privacy & Security ▸ scroll to "Gezik was blocked…" ▸ **Open Anyway**, and
confirm with your password. This is needed once. (On older macOS: right-click ▸ Open.)
In Terminal the same is `xattr -dr com.apple.quarantine /Applications/Gezik.app`.
Run Gezik from Applications, not from the dmg or Downloads: macOS runs a quarantined app from a
random read-only copy (App Translocation), where portable mode can't work.

**When macOS signing comes** (Developer ID, hardened runtime, notarization): Gezik loads
libraries it did not ship with at run time: Homebrew's or the downloaded ad hoc signed libmtp
for phones, and the downloaded pdfium in the PDF worker. The hardened runtime's library
validation refuses those (`dlopen` fails with "code signature … not valid for use in process").
The signed app then needs the `com.apple.security.cs.disable-library-validation` entitlement,
or every such library re-signed with Gezik's team identity (impossible for Homebrew's).

**Linux.** No warning: `tar xzf gezik-<v>-linux-x64.tar.gz && ./gezik-<v>-linux-x64/gezik`.

## Checking a download

Download `SHA256SUMS.txt` next to the package, then:

```sh
sha256sum -c --ignore-missing SHA256SUMS.txt      # Linux
shasum -a 256 -c --ignore-missing SHA256SUMS.txt  # macOS
```
```powershell
Get-FileHash gezik-0.1.0-windows-x64.zip -Algorithm SHA256   # Windows: compare with the file
```

The checksums show that a file arrived as GitHub built it; without signing they don't show who
built it.
