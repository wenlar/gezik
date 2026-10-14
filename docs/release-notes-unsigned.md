> [!NOTE]
> **The Windows build is not signed yet.** Windows warns you the first time you open Gezik. The files are the ones built by this repository's GitHub Actions; check them against `SHA256SUMS.txt`.
>
> - **Windows:** SmartScreen may say "Windows protected your PC". Choose **More info ▸ Run anyway**. Some antivirus programs may also ask.
> - **macOS:** signed with a Developer ID and notarized by Apple; it opens without a warning.
> - **Linux:** no warning; make the file executable if needed (`chmod +x gezik-*.AppImage`).
>
> Windows code signing will come in a later release.
