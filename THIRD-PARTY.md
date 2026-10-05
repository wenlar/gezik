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
