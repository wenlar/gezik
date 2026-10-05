# Test data

- `exif.jpg`: a 16×16 red JPEG made with ffmpeg, with a hand-written EXIF block
  (`DateTimeOriginal = 2024:07:01 09:30:00`); see `docs/superpowers/plans/2026-10-05-toplu-islemler-5a.md`, Task 4.
- `exif.png`: the same picture as a PNG, with the same date in an `eXIf` chunk after the image data.

## Pictures for conversion

None are kept here: `tests/convert_image.rs` makes its pictures itself (a "camera" JPEG with
EXIF orientation 6, GPS, a thumbnail, an ICC profile and XMP; PNGs with alpha and 16 bits; a
GIF). The samples the library notes used (`DSCN0010.jpg`, `landscape_6.jpg` from
github.com/ianare/exif-samples) are not copied: that repository has no licence file, and its
README puts only user-contributed pictures under CC BY-SA 4.0. The HEIC test downloads
libheif's `examples/example.heic` into a temporary folder when `GEZIK_TEST_FFMPEG` names an
ffmpeg 9 or later, and is skipped offline.

## RAR archives (`rar/`)

From libarchive's test suite (BSD 2-clause licence, https://github.com/libarchive/libarchive,
`COPYING`), decoded from the uuencoded `.uu` files at
`https://raw.githubusercontent.com/libarchive/libarchive/master/libarchive/test/<name>.uu`.
Contents and passwords as `test_read_format_rar.c`, `test_read_format_rar_encryption_data.c`
and `test_read_format_rar_encryption.c` there state them, checked with 7-Zip 25.

| File | Format | Password | Content |
|---|---|---|---|
| `test_read_format_rar.rar` | RAR4, packed on Unix | none | `test.txt` and `testdir/test.txt` ("test text document\r\n", 20 bytes each), `testlink` (symbolic link to `test.txt`), folders `testdir` and `testemptydir` |
| `test_read_format_rar_encryption_data.rar` | RAR4, encrypted data, plain headers | `12345678` | `foo.txt` ("data of foo.txt\n"), `bar.txt` ("data of bar.txt\n") |
| `test_read_format_rar5_multiple_files.rar` | RAR5 | none | `test1.bin` … `test4.bin`, 4096 bytes each (binary) |
| `test_read_format_rar5_encrypted_filenames.rar` | RAR5, encrypted headers (`rar -hp`) | `password` | `a.txt` … `d.txt` ("This is from a.txt" …, 18 bytes each) |
| `test_rar_multivolume_single_file.part1.rar` … `part3.rar` | RAR4, three volumes | none | `LibarchiveAddingTest.html` (20111 bytes) across the three |
| `test_read_format_rar5_hardlink.rar` | RAR5 | none | `file.txt` ("1234\n"), `hardlink.txt` (a hard link to `file.txt`) |
| `test_read_format_rar5_symlink.rar` | RAR5 | none | `file.txt`, `symlink.txt` (link to `file.txt`), `dirlink` (link to `dir`), folder `dir` |
| `test_read_format_rar_multivolume.part0001.rar` … `part0004.rar` | RAR4, four volumes | none | `ppmd_lzss_conversion_test.txt` (241,647,978 bytes, PPMd) first, then smaller files; used only to cancel inside a large entry |

The volume test uses `test_rar_multivolume_single_file`; `test_read_format_rar_multivolume`
(first entry 241 MB) serves only the cancel test, which stops after 1 MiB.
