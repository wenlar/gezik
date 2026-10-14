# Releasing Gezik

The packages are built by `.github/workflows/release.yml`. They are signed only once the
signing secrets are set up ([Windows](#windows-signing), [macOS](#macos-signing)); until then
Windows and macOS warn on first start, see [What users see](#what-users-see-without-signing).

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

**macOS.** Since 0.1.0-rc.3 the app is signed with a Developer ID and notarized (see
[macOS signing](#macos-signing)), so it opens without a warning. Builds made without the
signing secrets (a fork, a local `make-app.sh`) are signed ad hoc: opening one shows "Apple
could not verify "Gezik" is free of malware…"; click **Done**, then System Settings ▸ Privacy &
Security ▸ **Open Anyway**, or `xattr -dr com.apple.quarantine /Applications/Gezik.app`.
Run Gezik from Applications, not from the dmg or Downloads: macOS runs a quarantined unsigned
app from a random read-only copy (App Translocation), where portable mode can't work.

**Linux.** No warning: `tar xzf gezik-<v>-linux-x64.tar.gz && ./gezik-<v>-linux-x64/gezik`.

## Windows signing

`release.yml` signs `gezik.exe` with [Azure Artifact Signing](https://learn.microsoft.com/azure/trusted-signing/quickstart)
(Wenlar LLC, a US company, is eligible) before zipping it, so `SHA256SUMS.txt` covers the
signed file. Until the repository variables below exist, the two signing steps are skipped and
the build stays unsigned (it is in 0.1.0-rc.3). Signing puts "Wenlar LLC" in place of "Unknown publisher" at once;
SmartScreen's "Windows protected your PC" may still show until the certificate earns
reputation through downloads. Once signed builds ship, update the Windows line of
`docs/release-notes-unsigned.md` and [What users see](#what-users-see-without-signing).

One-time setup in the Azure portal:

1. Create a subscription and a resource group, register the `Microsoft.CodeSigning` resource
   provider, and create an **Artifact Signing account** (Basic, $9.99/month). Note its region's
   endpoint, e.g. `https://eus.codesigning.azure.net/`.
2. In the account, give yourself the **Artifact Signing Identity Verifier** role, then
   Identity validation ▸ New ▸ **Organization**, public trust, with Wenlar LLC's legal details.
   Microsoft checks them; this can take days.
3. When it is approved, Certificate profiles ▸ create a **Public Trust** profile on that
   identity.
4. Microsoft Entra ID ▸ App registrations ▸ new registration (e.g. `gezik-release`). In it,
   Certificates & secrets ▸ Federated credentials ▸ add **GitHub Actions**: organization
   `wenlar`, repository `gezik`, entity **Environment**, name `release`.
5. In the signing account's Access control (IAM), give that app the
   **Artifact Signing Certificate Profile Signer** role.
6. In GitHub, Settings ▸ Secrets and variables ▸ Actions ▸ **Variables** (none of them is
   secret): `AZURE_CLIENT_ID` (the app's client ID), `AZURE_TENANT_ID`,
   `AZURE_SUBSCRIPTION_ID`, `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT`,
   `AZURE_CERT_PROFILE`.

Check a run by hand (`gh workflow run release.yml -f version=0.0.0-sign`): in the downloaded
zip, `Get-AuthenticodeSignature gezik.exe` should say `Valid` with Wenlar LLC.

## macOS signing

`release.yml` signs `Gezik.app` with the Developer ID certificate (hardened runtime, timestamp,
`scripts/macos/gezik.entitlements`), has Apple notarize it and staples the ticket, before the
dmg and the update tarball are made. Until the secrets below exist, the steps are skipped and
`make-app.sh` signs ad hoc as before. 
The entitlement `com.apple.security.cs.disable-library-validation` is there because Gezik loads
libraries it did not ship with at run time: Homebrew's or the downloaded ad hoc signed libmtp for
phones, and the downloaded pdfium in the PDF worker. Without it the hardened runtime refuses them
(`dlopen` fails with "code signature … not valid for use in process").

One-time setup (Wenlar LLC's Apple Developer account):

1. On a Mac, Keychain Access ▸ Certificate Assistant ▸ Request a Certificate from a Certificate
   Authority (saved to disk). At developer.apple.com ▸ Certificates ▸ **+** ▸
   **Developer ID Application**, upload the request, download the certificate and open it.
2. In Keychain Access, under My Certificates, export "Developer ID Application: …" with its key
   as `gezik.p12` with a password. `base64 -i gezik.p12 | pbcopy` copies it for step 4.
3. App Store Connect ▸ Users and Access ▸ Integrations ▸ App Store Connect API ▸ team key with
   the **Developer** role. Note the key ID and the issuer ID; download the `.p8` (only once).
4. In GitHub, Settings ▸ Secrets and variables ▸ Actions ▸ **Secrets**: `MACOS_CERT_P12` (the
   base64 text), `MACOS_CERT_PASSWORD`, `APPLE_API_KEY` (the whole `.p8` text),
   `APPLE_API_KEY_ID`, `APPLE_API_ISSUER`.

Check a run by hand (`gh workflow run release.yml -f version=0.0.0-sign`): the Notarize step
ends with `source=Notarized Developer ID`; on a Mac, the downloaded dmg's app opens without the
"Apple could not verify" box.

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
