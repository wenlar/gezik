# Third-party code

Gezik's own code is under the PolyForm Noncommercial License 1.0.0 (see `LICENSE.md`). The
crates below are built into it for reading and writing archives; each keeps its own licence.

| Crate | Version | Licence | Source |
|---|---|---|---|
| zip | 8.6.0 | MIT | https://github.com/zip-rs/zip2 |
| sevenz-rust2 | 0.23.0 | Apache-2.0 | https://github.com/hasenbanck/sevenz-rust |
| tar | 0.4.46 | MIT OR Apache-2.0 | https://github.com/composefs/tar-rs |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | https://github.com/rust-lang/flate2-rs |
| lzma-rust2 | 0.21.0 | Apache-2.0 | https://github.com/hasenbanck/lzma-rust2 |
| bzip2 | 0.6.1 | MIT OR Apache-2.0 | https://github.com/trifectatechfoundation/bzip2-rs |
| ruzstd | 0.9.0 | MIT | https://github.com/KillingSpark/zstd-rs |
| unrar-ng, unrar-ng-sys | 0.7.7 | MIT OR Apache-2.0 (the bundled UnRAR 7.21: UnRAR licence, below) | https://github.com/ttys3/unrar.rs |
| cab | 0.6.0 | MIT | https://github.com/mdsteele/rust-cab |
| lzxd (through cab) | 0.2.7 | MIT OR Apache-2.0 | https://github.com/Lonami/lzxd |
| hadris-iso and its hadris-* parts | 2.5.0 | MIT | https://github.com/hxyulin/hadris |
| cpio | 0.4.1 | MIT | https://github.com/jcreekmore/cpio-rs |
| ar | 0.9.0 | MIT | https://github.com/mdsteele/rust-ar |

## UnRAR

RAR archives are read with the UnRAR source code by Alexander Roshal, which `unrar-ng-sys`
compiles in. Gezik only extracts RAR archives; it does not create them. Paragraph 2 of the
UnRAR licence (`license.txt` in the UnRAR sources):

> UnRAR source code may be used in any software to handle
> RAR archives without limitations free of charge, but cannot be
> used to develop RAR (WinRAR) compatible archiver and to
> re-create RAR compression algorithm, which is proprietary.
> Distribution of modified UnRAR source code in separate form
> or as a part of other software is permitted, provided that
> full text of this paragraph, starting from "UnRAR source code"
> words, is included in license, or in documentation if license
> is not available, and in source code comments of resulting package.

## 7-Zip (downloaded on request)

7-Zip is not built into Gezik. When the user asks for it, Gezik downloads an unmodified
official 7-Zip build (7-Zip 26.03 by Igor Pavlov, https://www.7-zip.org/) from
https://github.com/wenlar/gezik-tools and runs it as a separate program. Each download holds
the 7-Zip programs (`7z.exe` and `7z.dll` on Windows, `7zz` on macOS and Linux) and 7-Zip's
`License.txt`, which is installed next to them. 7-Zip is under the GNU LGPL 2.1 or later; some
code in `7z.dll` and `7zz` is under the BSD 3-clause and BSD 2-clause licences, and its RAR
code is under the GNU LGPL with the unRAR licence restriction (the unRAR sources cannot be used
to re-create the RAR compression algorithm, and modified ones may not be used to develop a
RAR (WinRAR) compatible archiver). The source is at
https://www.7-zip.org/download.html. How the downloads are made: `scripts/tools/README.md`.
