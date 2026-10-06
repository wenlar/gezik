# Third-party code

Gezik's own code is under the PolyForm Noncommercial License 1.0.0 (see `LICENSE.md`). The
crates below are built into it for reading and writing archives, for converting pictures
and text, and for reading and writing PDFs; each keeps its own licence.

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
| image | 0.25.10 | MIT OR Apache-2.0 | https://github.com/image-rs/image |
| tiff (through image) | 0.11.3 | MIT | https://github.com/image-rs/image-tiff |
| fax (through tiff) | 0.2.7 | MIT | https://github.com/pdf-rs/fax |
| jpeg-encoder | 0.7.1 | (MIT OR Apache-2.0) AND IJG (below) | https://github.com/vstroebel/jpeg-encoder |
| fast_image_resize | 6.1.0 | MIT OR Apache-2.0 | https://github.com/cykooz/fast_image_resize |
| encoding_rs | 0.8.42 | (Apache-2.0 OR MIT) AND BSD-3-Clause (the WHATWG data, below) | https://github.com/hsivonen/encoding_rs |
| chardetng | 1.0.0 | Apache-2.0 OR MIT | https://github.com/hsivonen/chardetng |
| core_detect (through encoding_rs) | 1.0.0 | MIT OR Apache-2.0 | https://github.com/thomcc/core_detect |
| simdutf8 (through encoding_rs) | 0.1.5 | MIT OR Apache-2.0 | https://github.com/rusticstuff/simdutf8 |
| pdf-writer | 0.15.0 | MIT OR Apache-2.0 | https://github.com/typst/pdf-writer |
| ryu (through pdf-writer) | 1.0.23 | Apache-2.0 OR BSL-1.0 | https://github.com/dtolnay/ryu |
| pdfium-render | 0.9.4 | MIT OR Apache-2.0 | https://github.com/ajrcarey/pdfium-render |
| maybe-owned (through pdfium-render) | 0.3.4 | MIT OR Apache-2.0 | https://github.com/rustonaut/maybe-owned |
| utf16string (through pdfium-render) | 0.2.0 | MIT OR Apache-2.0 | https://github.com/getsentry/utf16string |
| vecmath (through pdfium-render) | 1.0.0 | MIT | https://github.com/pistondevelopers/vecmath |
| piston-float (through vecmath) | 1.0.1 | MIT | https://github.com/pistondevelopers/float |

## Independent JPEG Group

JPEG pictures are written with jpeg-encoder, whose forward DCT is ported from mozjpeg. This
software is based in part on the work of the Independent JPEG Group.

## WHATWG Encoding Standard data

Text is converted with encoding_rs, whose encoding tables are generated from the index files
of the WHATWG Encoding Standard (https://encoding.spec.whatwg.org/). Their licence
(`LICENSE-WHATWG` in the encoding_rs sources):

> Copyright © WHATWG (Apple, Google, Mozilla, Microsoft).
>
> Redistribution and use in source and binary forms, with or without
> modification, are permitted provided that the following conditions are met:
>
> 1. Redistributions of source code must retain the above copyright notice, this
>    list of conditions and the following disclaimer.
>
> 2. Redistributions in binary form must reproduce the above copyright notice,
>    this list of conditions and the following disclaimer in the documentation
>    and/or other materials provided with the distribution.
>
> 3. Neither the name of the copyright holder nor the names of its
>    contributors may be used to endorse or promote products derived from
>    this software without specific prior written permission.
>
> THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
> AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
> IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
> DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
> FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
> DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
> SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
> CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
> OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
> OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

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

## ffmpeg (downloaded on request)

ffmpeg is not built into Gezik. When the user asks for it, Gezik downloads an unmodified
ffmpeg 9.0.2 build from https://github.com/wenlar/gezik-tools and runs `ffmpeg` and `ffprobe`
as separate programs. The builds come from third parties and are only repackaged: on Windows
x64 the essentials build by Gyan Doshi (https://www.gyan.dev/ffmpeg/builds/); on Windows arm64
and Linux BtbN's GPL builds (https://github.com/BtbN/FFmpeg-Builds; FFmpeg `n9.0.2-17-g2a571b6068`,
the 9.0 branch with fixes after 9.0.2); on macOS Martin Riedl's builds
(https://ffmpeg.martin-riedl.de/, signed with his Developer ID, which is kept). These builds
include GPL code (x264, x265 and others) and are distributed under the GNU General Public
License version 3. Each download holds the two programs, the licence text (`LICENSE`) and
`SOURCE.txt`, which names the FFmpeg commit, the builder's scripts and the upstream files with
their SHA-256. FFmpeg's source is at https://ffmpeg.org/download.html. How the downloads are
made: `scripts/tools/README.md`.
