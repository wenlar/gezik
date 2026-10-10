# Gezik on Linux: portable build and manual test

So far Gezik has run on Linux only in a container: Xvfb and a headless sway, driven by `scripts/linux/test.sh` and `gui.sh` (see `2026-10-06-linux-ekran-testi.md`). It has never run on a real desktop, with Nautilus or Dolphin, a real compositor, a portal, HiDPI or a real trash. This file is for a person testing the state after sub-project 5 (5a-5d) on a real Linux desktop, from the portable build.

Write the results into `docs/superpowers/notes/linux-test-results.md`, in the same style as `macos-test-results.md`:
- Start with the date, the build's SHA, the distribution and version, the desktop and its version, the session (`echo $XDG_SESSION_TYPE`), the display scale, the glibc (`ldd --version | head -1`) and the keyboard layout.
- Add a summary table, then a section per item.
- Give PASS, FAIL or NOT TESTED for every item. Items 9-14, 33 and 61 need **both** X11 and Wayland: give one result for each.
- For a FAIL, give the steps, what you expected, what happened, Gezik's terminal output, and a screenshot path. Don't commit screenshots.

## Get and start the build

The build is `gezik-<sha>-linux-x64.tar.gz`, from `D:\Work\gezik-tools\test-builds\` on the Windows machine. Copy it over on a USB stick or by scp. It is a plain x86_64 binary with LICENSE.md, THIRD-PARTY.md and README-linux.txt. It is not an installer and needs no root. To build it again, see `scripts/linux/package.sh`.

```sh
tar xzf gezik-<sha>-linux-x64.tar.gz
cd gezik-<sha>-linux-x64
rm -rf /tmp/gezik-cfg && mkdir -p /tmp/gezik-cfg ~/gezik-test
GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./gezik ~/gezik-test
```

**GEZIK_CONFIG_DIR** is the folder for `settings.toml`, `state.toml`, themes and the downloaded tools (`<dir>/tools/`).
- Without it, Gezik uses `~/.config/gezik` (or `$XDG_CONFIG_HOME/gezik`).
- Start every run from an empty `/tmp/gezik-cfg`, so that the 7-Zip, ffmpeg and pdfium download offers appear.
- If `/tmp` is mounted `noexec` (`findmnt -no OPTIONS /tmp`), downloaded tools can't run from there. Then use `GEZIK_CONFIG_DIR=~/gezik-cfg`, and note it in the results.

Keep `~/gezik-test` on the home drive. Use a USB stick for the "other drive" cases.

### Runtime dependencies

The binary links only to glibc (2.35 or newer), libstdc++, libgcc_s, libm and **libfontconfig**. It loads the window system libraries itself when it starts, so `ldd` doesn't show them. What the container needed, and what a desktop normally has already:

| Need | Debian / Ubuntu | Fedora | Why |
|---|---|---|---|
| fontconfig and a font | `libfontconfig1 fonts-dejavu-core` | `fontconfig dejavu-sans-fonts` | Text. Without it Gezik doesn't start. |
| X11 | `libx11-6 libx11-xcb1 libxcursor1 libxi6 libxkbcommon0 libxkbcommon-x11-0` | `libX11 libX11-xcb libXcursor libXi libxkbcommon libxkbcommon-x11` | X11 session or XWayland |
| Wayland | `libwayland-client0 libxkbcommon0` | `libwayland-client libxkbcommon` | Wayland session |
| curl **or** wget | `curl` | `curl` | 7-Zip, ffmpeg and pdfium downloads. Without both, the box tells you to install curl. |
| a trash | none (Gezik makes `~/.local/share/Trash`) | none | Delete. On other drives Gezik uses `.Trash-<uid>` at the drive's top. |
| xdg-desktop-portal (optional) | `xdg-desktop-portal-gnome` / `-kde` | the same | The `auto` theme follows light/dark. Without it the theme is light. |
| CJK fonts (optional) | `fonts-noto-cjk` | `google-noto-sans-cjk-fonts` | Chinese, Japanese and Korean names, and PDFs without embedded fonts (item 30) |

No GPU or OpenGL is needed, because Gezik draws in software.

Messages Gezik prints at start that are harmless:
- `weak version 'GLIBC_2.39' not found` on a glibc older than 2.39 (checked on Ubuntu 22.04).
- `Error watching for xdg desktop settings: …` when there is no D-Bus session bus.

Note in the results any other line in the terminal.

### Keys on Linux

| Action | Key |
|---|---|
| Open | Enter or double-click |
| Back / forward / up | Alt+Left / Alt+Right / Alt+Up |
| Rename | F2 |
| Move to trash / delete for good | Delete / Shift+Delete |
| Undo / redo | Ctrl+Z / Ctrl+Y |
| Refresh | F5 |
| Show or hide dotfiles | Ctrl+H |
| Path field | Ctrl+L |
| Preview pane | Alt+P |
| Start a layer's job | Ctrl+Enter |
| Filter the list | Ctrl+F, or `/` while the list has the keyboard |
| Select / deselect by pattern | Ctrl+= / Ctrl+-, or keypad + / keypad - |
| Select the same type | Alt+keypad + |
| Invert the selection | Ctrl+Shift+I |
| Restore the last selection | keypad / |
| Tabs | Ctrl+1…Ctrl+8, Ctrl+9 (the last), Ctrl+Shift+T (reopen a closed tab), Ctrl+Shift+A (tab picker) |
| List / grid view | Ctrl+Shift+1 / Ctrl+Shift+2 |
| Address bar suggestions | ↓ ↑ to choose, Tab or → to write the chosen one, Enter to go, Esc to close the list (a second Esc ends typing) |
| Clear folder history | `clear-history` (no key by default) |
| Open terminal here | Shift+F4, Ctrl+Alt+T (GNOME/Ubuntu take Ctrl+Alt+T themselves) |
| Copy path | Ctrl+Shift+C |
| Pinned folders 1-9 | Alt+1…Alt+9 (in the sidebar's order; note whether the desktop takes Alt+digit itself) |
| Command keys | `shortcut` in a `[[commands]]` entry (Ctrl, Alt or an F key is needed) |

All of these are listed in `settings.toml` under `[shortcuts]`.

### Test data

- **Files:** a few files and folders in `~/gezik-test`, including names with spaces, Turkish letters (`ılık İstanbul.txt`) and one starting with a dot.
- **Archives:** zip, tar.gz and 7z (`7z` or `7zz` if installed). A RAR file is in the repo under `crates/gezik-batch/tests/data/`, if you have a clone. Make an AES zip with `7z a -tzip -mem=AES256 -psecret aes.zip file`, and a `.wim` with `7z a -twim w.wim files`.
- **Pictures:**
  - JPEGs, one with EXIF orientation 6 (a portrait phone photo).
  - A PNG with transparency.
  - A real iPhone HEIC; these are tiled. If there is none, use a HEIC from the web and say so.
- **Text** in Windows-1254: `printf 'ılık İstanbul\r\n' | iconv -f UTF-8 -t WINDOWS-1254 > tr1254.txt`.
- **Audio and video:** any video (a phone .mov or .mp4) and an audio file.
- **PDFs:**
  - A few small PDFs (print to PDF from the browser).
  - One with 200+ pages.
  - An encrypted one: `qpdf --encrypt user owner 256 -- in.pdf enc.pdf`, or LibreOffice's Export as PDF ▸ Security.
  - A PDF whose CJK fonts are **not embedded**, for item 30:

    ```sh
    python3 - cjk-not-embedded.pdf <<'EOF'
    import sys
    fonts = [("F1", "HeiseiMin-W3", "UniJIS-UCS2-H", "Japan1", 2, "日本語のテキスト"),
             ("F2", "STSong-Light", "UniGB-UCS2-H", "GB1", 2, "简体中文文本"),
             ("F3", "HYSMyeongJo-Medium", "UniKS-UCS2-H", "Korea1", 1, "한국어 텍스트")]
    objs = {1: "<< /Type /Catalog /Pages 2 0 R >>", 2: "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            9: "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"}
    res = " ".join(f"/{r} {10 + 3 * i} 0 R" for i, (r, *_) in enumerate(fonts))
    objs[3] = f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << {res} /H 9 0 R >> >> /Contents 4 0 R >>"
    lines = ["BT /H 18 Tf 72 760 Td (Fonts not embedded: Japanese, Chinese, Korean) Tj ET"]
    for i, (r, base, cmap, order, sup, text) in enumerate(fonts):
        lines.append(f"BT /{r} 36 Tf 72 {680 - 80 * i} Td <{text.encode('utf-16-be').hex()}> Tj ET")
        n = 10 + 3 * i
        objs[n] = f"<< /Type /Font /Subtype /Type0 /BaseFont /{base} /Encoding /{cmap} /DescendantFonts [{n + 1} 0 R] >>"
        objs[n + 1] = (f"<< /Type /Font /Subtype /CIDFontType0 /BaseFont /{base} /CIDSystemInfo << /Registry (Adobe) "
                       f"/Ordering ({order}) /Supplement {sup} >> /FontDescriptor {n + 2} 0 R /DW 1000 >>")
        objs[n + 2] = (f"<< /Type /FontDescriptor /FontName /{base} /Flags 6 /FontBBox [0 -141 1000 859] "
                       "/ItalicAngle 0 /Ascent 859 /Descent -141 /CapHeight 700 /StemV 80 >>")
    stream = "\n".join(lines).encode()
    out, offsets = bytearray(b"%PDF-1.4\n"), {}
    for n in sorted(list(objs) + [4]):
        offsets[n] = len(out)
        body = (f"<< /Length {len(stream)} >>\nstream\n".encode() + stream + b"\nendstream") if n == 4 else objs[n].encode()
        out += f"{n} 0 obj\n".encode() + body + b"\nendobj\n"
    size, xref = max(offsets) + 1, len(out)
    out += f"xref\n0 {size}\n0000000000 65535 f \n".encode()
    for n in range(1, size):
        out += (f"{offsets[n]:010d} 00000 n \n" if n in offsets else "0000000000 65535 f \n").encode()
    out += f"trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    open(sys.argv[1], "wb").write(out)
    EOF
    ```

## Checklist

Start and desktop
1. **Start:**
   - The tarball unpacks, and `./gezik` starts from a terminal. The window opens at a sensible size, with the title "Gezik".
   - Write down every line Gezik prints at start.
   - Quit and start again: the window size and position come back from `state.toml`.
2. **Theme following:**
   - With the desktop in dark mode (GNOME Settings ▸ Appearance, or KDE's colour scheme), Gezik starts dark.
   - Switching the desktop's mode while Gezik runs switches Gezik.
   - Note whether `xdg-desktop-portal` runs (`ps -e | grep xdg-desktop-portal`). Without a portal, a light theme is known.
3. **Navigation:**
   - Double-click and Enter open a folder. Alt+Left, Alt+Right and Alt+Up go back, forward and up.
   - Ctrl+T opens a tab and Ctrl+W closes it. Ctrl+L opens the path field. F5 refreshes.
   - The sidebar shows Home, File System and a mounted USB stick (under `/media` or `/run/media`).
   - Double-clicking a file opens it in the desktop's default app (`xdg-open`).
4. **Ctrl+H:** Linux shows dotfiles by default. Ctrl+H hides them and Ctrl+H again shows them. Note what Ctrl+H does while the path field or a rename field has the focus.
5. **Rename, new folder, undo:**
   - F2 renames in place, with the stem selected.
   - Ctrl+Shift+N makes a folder and opens its name.
   - Ctrl+Z and Ctrl+Y undo and redo a move and a rename.
   - Live refresh: `touch ~/gezik-test/canli.txt` from a terminal shows up at once.
6. **Focus after Alt+Tab** (F1 in the container run, fixed in 5c):
   - Press F2 on a file, Alt+Tab to another app, then Alt+Tab back.
   - Down and F2 work in the list without a click.
   - Do the same with the batch rename layer open.
7. **Large copy:** copy 1-2 GB to the USB stick. Progress shows, and pause, resume and cancel work. After cancel, no `.gezik-*` file is left.
8. **Conflict list:** paste over existing files. Replace, Skip and Keep both work, and Ctrl+Z undoes them.

Desktop integration (do 9-14 in an X11 session and in a Wayland session)

9. **Clipboard with the desktop file manager** (Nautilus on GNOME, Dolphin on KDE):
   - Copy in Gezik, paste in Nautilus/Dolphin: a copy.
   - Cut in Gezik, paste there: a move, and the row in Gezik was faded.
   - Copy in Nautilus/Dolphin, paste in Gezik: a copy.
   - Cut there, paste in Gezik: a move. Gezik reads GNOME's `x-special/gnome-copied-files` and KDE's `application/x-kde-cutselection`.
   - Copy in Gezik, paste in a text editor: the paths, as text.
   - Note: on Wayland the clipboard only works while Gezik's window has the focus. That is known.
10. **Trash (XDG):**
    - Delete puts the file in `~/.local/share/Trash/files`, with `info/<name>.trashinfo` (the right `Path=`, `DeletionDate=`). It shows in Nautilus/Dolphin's Trash (`trash:///`).
    - Ctrl+Z brings it back, and the `.trashinfo` is gone.
    - Then delete again and restore it from Nautilus/Dolphin's Trash instead. It goes back to the right place.
    - On the USB stick (if it is a Linux file system, or FAT mounted for your user): Delete uses `.Trash-$(id -u)` at the stick's top, and the desktop's Trash shows it.
    - Shift+Delete asks first, then deletes for good.
    - On a drive with no trash (a read-only top folder, for example), Gezik asks whether to delete for good.
11. **Drag out, to Nautilus/Dolphin:**
    - Drag a file from Gezik into a Nautilus or Dolphin window.
      - X11: Gezik offers a copy, or a move with Shift.
      - Wayland: the target decides.
    - Gezik never deletes the source itself. If the target moves the file, check that it is in one place only.
    - Also drag onto the desktop (if it has icons) and into a text editor.
12. **Drag in, from Nautilus/Dolphin:**
    - Onto the list: the file lands in the open folder.
    - Onto a folder row: the row is highlighted, the label says "Move to …" or "Copy to …", and the file goes there.
    - Onto a zip row: "Add to x.zip".
    - Between drives the default is copy; on the same drive it is move.
    - X11: Ctrl/Shift while dragging change it.
    - Wayland: Gezik can't see the modifier keys during a drag from another app, so the drive rule decides, and the cursor may say copy for a move. That is known.
13. **Drags inside Gezik:**
    - Between two tabs: rest on a tab until it opens, then drop on the tab, and again over the new list.
    - A drag out of the window and back in drops in Gezik.
    - Esc while outside the window cancels it.
    - Right-drag, rest on a tab and release: only the Copy here / Move here menu appears.
    - Two Gezik windows: drag a file from one onto a folder in the other.
14. **Gezik's own popup menus.** On Linux Gezik draws its menus itself and keeps them inside the window. Test with a normal (not maximized) window.
    - A right-click on a row at the bottom right: the menu flips up and left, and every item is visible.
    - Commands ▸ near the right edge: the submenu opens to the left.
    - The View menu; Presets in the rename layer; Preset, From and To in the Convert layer (From/To are long lists): a list taller than the window is cut to it and scrolls.
    - Up and Down skip greyed lines. Esc and a click outside close the menu, and the Menu key opens the context menu.
    - A name with `&` shows the `&` as typed.
    - Take a full-screen screenshot of each.

Batch rename (5a)
15. Select 3+ files and press F2 to open "Rename N items".
    - Replace, Number and Case preview live. Ctrl+Enter renames, and Ctrl+Z restores.
    - Swap `a.txt` and `b.txt` by typing their names, then undo.
    - Presets: save, apply and delete. `[[rename-presets]]` appears in `settings.toml`.
    - Turkish casing: start with `LANG=tr_TR.UTF-8` (if that locale exists, see `locale -a`). UPPER gives `İ`.

Archives (5b)
16. **Extract here:**
    - A single-root zip goes in as it is; a multi-root zip goes into `<name>/`. Ctrl+Z removes it.
    - tar.gz, 7z and rar extract.
    - AES zip: the password box shows dots, and Show reveals the text. A wrong password asks again.
    - Cancel a big extraction: no `.gezik-*` is left (`ls -a`).
17. **Compress… layer:** a zip, a 7z with a password and "Encrypt file names", and a split 7z.
    - The layer selects only the stem. With 7z chosen, typing `sifreli` makes `sifreli.7z`. Typing only `.zip` makes `<item name>.zip`.
    - "Compress to "x.zip"" is always a plain zip, even after a 7z.
    - The layer opens with Split off.
    - Drag a file onto a zip row to add it. Ctrl+Z restores the old zip.
18. **7-Zip:** check the system's version first (`7z | head -2`, `7zz | head -2`). Gezik uses a system 7-Zip only if it is 25.00 or newer: Debian 13 has 25.01, Ubuntu 24.04 has 23.01.
    - Open the `.wim` with Extract. With no 7-Zip 25+, the "7-Zip needed" box appears. Download it.
    - `ls -l /tmp/gezik-cfg/tools/7zip-26.03/`: `7zz` is **0755** (`stat -c %a`). `./7zz i | head -3` runs, and the extraction continues by itself.
    - With a 7-Zip 25+ on PATH (if your distribution has one), no box appears and that one is used. Write down which.
    - `.wim` and `x.7z.001` show the archive icon and the types "WIM archive" and "Split 7Z archive".

Convert and user commands (5c)
19. **Pictures:** right-click a JPEG, then Convert…
    - "Resize photos (JPEG 1920 px)" to a subfolder: the orientation-6 photo comes out upright.
    - "Remove metadata (incl. location)" removes the EXIF (`exiftool` or `identify -verbose`).
    - PNG with transparency → JPEG has a white background.
    - "Replace originals": the originals go to the trash, and Ctrl+Z brings them back.
    - "Remove location data" removes the GPS tags.
20. **Text:**
    - `tr1254.txt` says "Detected: Windows-1254", and → UTF-8 is right.
    - UTF-8 `ı` → Windows-1252 gives an error line, and the file is unchanged.
21. **ffmpeg download and HEIC:**
    - Hide any system ffmpeg first (`which ffmpeg`, then start Gezik with `PATH` without its folder). Distribution ffmpeg packages are older than 9, which HEIC needs. Ubuntu 24.04 has 6.1.
    - Convert the HEIC to JPEG. The "ffmpeg needed" box appears, and "Where does it come from?" opens the browser. Download it.
    - `/tmp/gezik-cfg/tools/ffmpeg-9.0.2/ffmpeg` and `ffprobe` are **0755**. The conversion continues by itself.
    - The result is the full picture, not one 512 px tile, and upright.
    - Also try WebP (lossy) and AVIF output.
    - Then put the old system ffmpeg back on PATH and convert a HEIC again. Gezik still uses its own 9.0.2. For MP3, any ffmpeg will do.
22. **Downloads with wget only, and with neither:**
    - Run `mkdir /tmp/wonly && ln -s "$(command -v wget)" /tmp/wonly/`. Start Gezik with `PATH=/tmp/wonly`, on a fresh config folder, and download pdfium or ffmpeg. It works.
    - With `PATH=/nonexistent`: the box says to install curl, and nothing hangs.
23. **Audio and video:**
    - MP3, M4A, "Smaller video", "Remux to MP4" and "GIF from video" play in the desktop's player.
    - Cancel halfway: no half file is left, and `pgrep -a ffmpeg` is empty.
    - Pause: ffmpeg ends. Resume starts that item again and finishes it.
24. **User commands.** Add these to `/tmp/gezik-cfg/settings.toml` (needs ImageMagick and LibreOffice; leave out any you don't have):
    ```toml
    [[commands]]
    name = "Half size (ImageMagick)"
    run = ["convert", "{in}", "-resize", "50%", "{out}"]
    output = "{name}-small.{ext}"
    types = ["jpg", "jpeg", "png"]
    parallel = 4

    [[commands]]
    name = "Office to PDF (LibreOffice, outdir)"
    run = ["soffice", "--headless", "--convert-to", "pdf", "--outdir", "{outdir}", "{in}"]
    output = "{name}.pdf"
    types = ["docx", "odt", "xlsx"]

    [[commands]]
    name = "Rotate in place (mogrify)"
    run = ["mogrify", "-rotate", "90", "{in}"]
    types = ["jpg", "png"]

    [[commands]]
    name = "Missing program"
    run = ["no-such-tool", "{in}"]
    ```
    - Gezik picks up the change live. Commands ▸ in the right-click menu and in the Convert layer lists them by type, and "Missing program" is grey.
    - `{out}` and `{outdir}`: the output lands next to the input under its `output` name. Nothing half-made shows in the folder while it runs. Running it again opens the conflict list.
    - In place: Ctrl+Z gives back the original, from the copy in the trash.
    - Pause and cancel a long `{outdir}` run (several Office files): `soffice` ends.
    - A name with spaces and quotes works.

PDF (5d)
25. **Images to PDF** (no pdfium needed):
    - Select 3 JPEGs (one orientation 6), a PNG and a HEIC. Right-click, then "Images to PDF…". The HEIC is left out with a note.
    - Reorder by dragging. Choose A4 and Small, then Ctrl+Enter.
    - The desktop's PDF viewer shows the right order, and the portrait page is upright.
26. **pdfium download:**
    - Merge 2 PDFs. The "pdfium needed" box appears. Download it.
    - `/tmp/gezik-cfg/tools/pdfium-8086/libpdfium.so` is there. Write down its mode (`stat -c %a`).
    - The merge continues by itself.
27. **Split, extract, to images:**
    - Split, Each page: "Makes N files".
    - Ranges `1-2, 5`, and a range past the end (live error).
    - Extract to a Turkish name.
    - PDF to images, PNG 150 dpi and JPEG 72 dpi.
    - Running a job again opens the conflict list. Ctrl+Z removes the outputs.
28. **Encrypted PDF:**
    - The password box: a wrong password asks again, and the right one works.
    - The output isn't encrypted, and the layer said so.
    - Skip in a merge leaves the merge out.
29. **No worker left:**
    - After every PDF job, `pgrep -af -- --pdf-worker` is empty.
    - PDF to PNG at 300 dpi on the 200+ page PDF: pause, resume, then cancel. No worker and no `.gezik-*` are left.
    - `grep pdfium /proc/$(pgrep -x gezik)/maps` is empty.
    - Write down the RSS (`ps -o rss= -p $(pgrep -x gezik)`) before and after.
30. **Fonts in PDFs (CJK):**
    - PDF to images (PNG, 150 dpi) on `cjk-not-embedded.pdf`, first **without** CJK fonts installed (`fc-list :lang=ja` is empty; on a desktop you may have to test this in a fresh user or skip it).
    - Then again **with** `fonts-noto-cjk`. The Japanese, Chinese and Korean lines render as real glyphs, not boxes or blanks.
    - Write down what the first run gives (boxes, blank, or a fallback font), and that nothing crashed.
    - Also split a PDF with embedded CJK text (any Chinese or Japanese PDF from the web), and check a page in the viewer.
    - File names in CJK (`日本語.pdf`) show in the list and survive a split.
31. **Last choices and keys:** close and reopen the Convert layer on a PDF and on a picture. Each comes back with its last choices; the range text doesn't. Esc closes the layer and Ctrl+Enter starts it.

Display
32. **HiDPI:** at 200 % and at a fractional scale (125 % or 150 %; GNOME Wayland: `gsettings set org.gnome.mutter experimental-features "['scale-monitor-framebuffer']"`):
    - Text is sharp.
    - Clicks land where they should.
    - The menu positions and drop highlights match the pointer.
33. **Wayland vs X11 summary:** repeat 9-14 in the other session type (GNOME: "GNOME on Xorg" at login, where available; KDE: Plasma (X11)). Also run once with `env -u WAYLAND_DISPLAY ./gezik` (XWayland) in a Wayland session. Fill a small table of what differs.

Keyboard (6a: filter, selection, tabs)
34. **Filter:** in `~/gezik-test`, Ctrl+F opens a bar above the list with the field focused. Type `jpg`: only names with `jpg` show, and the counter says shown / all (`2 / 14`).
    - Down gives the list the keyboard and the bar stays. Ctrl+A then Delete trashes only the shown items. Ctrl+Z brings them back, and the filter stays.
    - Esc on the list closes the filter, and a second Esc clears the selection. Esc in the field closes it too.
    - `/` on the list opens the bar. `istanbul` shows `ılık İstanbul.txt` (Turkish İ), and so does `ILIK`.
    - A second tab keeps its own filter (Ctrl+Tab back and forth). Going into a folder or Back opens it without a filter.
    - Copy a file into the folder from Nautilus/Dolphin while it is filtered. The list refreshes by itself and the counter follows.
    - `!` alone shows a red line and "Type a name after "!"", and the list stays.
35. **Pattern box:** Ctrl+= and keypad + open "Select by pattern", starting with the last pattern. "N items match" follows the text, and a bad pattern is said in red. Select adds the matches. Ctrl+- and keypad - deselect.
    - The main-row `+` (Shift+=) does not open the box.
    - Ctrl+Shift+I inverts. Alt+keypad + on a `.jpg` adds every `.jpg`.
    - Select two files, Delete, Ctrl+Z, then keypad /: the two are selected again.
    - On a non-US layout where `=` needs Shift (Turkish Q: Shift+0), Ctrl+= doesn't work. `select-pattern = ["num+", "ctrl+shift+0"]` under `[shortcuts]` makes it work. Ctrl+- works.
36. **Keypad on Wayland:** repeat 35's keypad + / - / / and Alt+keypad + in a Wayland session, and in an X11 session with NumLock on and off. Write down which work.
37. **Tabs:** open four tabs. Ctrl+1…Ctrl+4 switch, Ctrl+7 does nothing, Ctrl+9 shows the last. Ctrl+Shift+2 shows the grid and Ctrl+Shift+1 the list (also on Turkish Q).
    - Go two folders deep in a tab, filter it, then Ctrl+W and Ctrl+Shift+T. The tab is back in its place with its filter, and Alt+Left goes to the folder before.
    - Right-click a tab, then Lock tab: a lock shows and the × goes. Ctrl+W, a middle-click and "Close other tabs" leave it open, and the status bar says so. Unlock tab.
    - Ctrl+Shift+A opens the tab picker with the active tab selected. Typing filters it, Up/Down move, Enter switches and Esc closes.
38. **Typing mode:** add `[keyboard]` `typing = "filter"` to `settings.toml` (picked up live). Typing a letter on the list opens the filter with it. Back to `"jump"`, a letter jumps to a name again.
39. **Saved filters:** ▾ in the bar, Save as… "Resimler". `settings.toml` gets `[[filters]]`. ▾ then Resimler fills the bar. ▾, Delete "Resimler" takes it out.

Keyboard (6b: path completion, folder history, command keys)
40. **Path suggestions:** Ctrl+L, then type `/us`. After a short pause, a list under the field shows the matching sub-folders (`usr`), never files.
    - ↓ marks a row and ↑ goes back up. Tab writes the marked row (or the first) with a `/` at the end, and the list shows its sub-folders. Enter on a marked row goes there; with none marked, where the text says. A click on a row goes there.
    - Esc closes the list and typing goes on; a second Esc ends typing.
    - `~/Desktop`, `$HOME/Downloads` and `${HOME}/Downloads` go there. `$GEZIK_NOPE/x` stays as typed and opens nothing; `~veli` is not expanded.
    - On a mounted network share (NFS or SMB) that stops answering (unplug the network): typing stays smooth, the list is empty after about 1 s, and `ls /proc/$(pgrep -x gezik)/task | wc -l` doesn't grow with each key.
41. **Suggestions on Wayland:** repeat 40 in a Wayland session. The list shows under the field and the keys work the same. (The container tried this on X11 only.)
42. **Folder history:**
    - Go to a few folders. Ctrl+L, then Backspace: the empty field lists "Recent" (the last 5) and "Frequent" (the most visited). Typing a part of a visited folder's name lists it under a "History" heading.
    - Delete a visited folder in Nautilus/Dolphin, then open the empty list: it goes within 1 s. Write down the file system of the folder (`stat -f -c %T <folder>`): on btrfs, tmpfs or overlay the folder may stay listed (known, see the 6b notes).
    - Bind `clear-history = "ctrl+shift+h"` under `[shortcuts]`. Ctrl+Shift+H empties the list, and the status bar says "Folder history cleared".
    - `[history]` `remember = false`: `state.toml` loses its `[history]` and new visits are not kept.
43. **Command keys.** Add to `settings.toml`:
    ```toml
    [[commands]]
    name = "Zip together"
    run = ["zip", "-r", "together.zip", "{files}"]
    folders = true
    shortcut = "ctrl+alt+z"
    menu = "Archives"
    ask = true

    [[commands]]
    name = "Clash"
    run = ["true"]
    shortcut = "ctrl+f"
    ```
    - Select some files and a folder, then Ctrl+Alt+Z: "Run Zip together on N items?". Cancel runs nothing; Run makes one `together.zip`, and the panel row says "Done · can't be undone".
    - Right-click, then Commands ▸: a greyed "Archives" heading over "Zip together". The heading can't be chosen.
    - The notice says `commands[2]: shortcut "ctrl+f" is already used by filter; the command has no key`, and Ctrl+F still opens the filter.
    - Ctrl+Alt+Z in the path field does nothing. Note whether the desktop takes Ctrl+Alt+<letter> keys itself (GNOME and KDE bind some, such as Ctrl+Alt+T).

44. **Open terminal** (`Shift+F4`; Ctrl+Alt+T too, but GNOME/Ubuntu take Ctrl+Alt+T for themselves: note which one wins).
    - In a folder: the desktop's own terminal (GNOME: Ptyxis or Console; KDE: Konsole) opens in that folder, and comes to the front. Right-click a folder ▸ "Open terminal here", and empty space ▸ "Open terminal here" (the folder shown).
    - `TERMINAL=xterm gezik` (or the terminal you like): that one comes first.
    - `[terminal]` `command = ["xterm", "-e", "bash"]`-style entries: `{dir}` is put in, the command runs in the folder.
    - Quit Gezik: the terminal stays.
45. **Copy path** (Ctrl+Shift+C; "Copy path as ▸" in the right-click menu: Full path, Quoted, Name, Folder path, file:// URL).
    - Select `it's ş #1.txt`, Ctrl+Shift+C, paste into bash: the path. "Quoted for the shell", pasted into bash: the same file (`ls <paste>`). "file:// URL", pasted into Firefox's address bar: opens the file.
    - Several items: one per line. Nothing selected: the folder shown. On Wayland as well (note it).
    - Cut a file (Ctrl+X), then copy a path: Ctrl+V in a folder moves nothing.
46. **Session and tab sets.** Open three tabs, lock one, put another in front, close Gezik, open it again: the same tabs, the same one in front, the lock in place. `kill -9` it and open it again: the same. `gezik <folder>` opens that folder after the saved ones, in front. `[session]` `restore = false`: `state.toml` loses `[session]`. Right-click a tab ▸ "Save tabs as…" ▸ a name, "Open tab set ▸" opens it after the tabs, "Replace tabs with…" keeps a locked tab, "Delete…" removes it. With `save-tab-set` bound under `[shortcuts]`, the key does the same.
47. **Pinned folders 1-9 and AltGr.** Pin three folders; Alt+1…Alt+3 go to them in the sidebar's order, also while typing in the path field; Alt+4 does nothing. Note whether GNOME/KDE take Alt+digit for themselves (switching workspaces or apps). With a layout that has AltGr (Turkish Q, German): AltGr+7 in the path field types `{` (or the layout's character) and goes to no pin. If the layout reports AltGr as plain Alt (some X11 setups do), AltGr+digit goes to a pin instead: note it, it is a known risk of the Alt+digit rule.
48. **Pinned groups and their menus.** Write a list with an alias and two groups by hand (see `gui.sh pins`): headings as written, "PINNED" only above ungrouped pins. Right-click a heading: Move group up/down, Rename group…, Ungroup. Right-click a pin: Rename…, Move to group ▸ (other groups, New group…, No group). Drag a pin into another group: the line shows where it lands. Rest on a pin: a tip with the full path and "Alt+1".
49. **View options.** Ctrl+H writes `show-hidden` in `settings.toml` and the View menu's "Show hidden items" follows. View ▸ Hide extensions, Folders first, Single-click to open, Date format ▸ Relative ("1 min ago"), Size format ▸ Decimal: a 1,500-byte file is `1.5 kB`, the same as GNOME Files' size. Change the `[view]` lines by hand: they apply at once.
50. **New ▸ and templates.** Right-click empty space: "New ▸" holds Folder, Text file, Markdown file, then the files and folders in `~/.config/gezik/templates/` (put `Report.odt` and a folder `Project/` with a file in it there; "Open templates folder" opens it in a new tab), then "Open templates folder". Each makes its item in the folder shown, numbered `(2)` when the name is taken, and starts renaming it; Ctrl+Z takes it away. Dot files in the folder are not listed. A new template shows up in the menu within a second, without restarting Gezik.
51. **New folder with selection.** Select three files, Ctrl+Alt+N (and the right-click item "New folder with selection"): they are in "New folder", which is selected and being renamed. Ctrl+Z puts them back and the folder goes to the trash; Ctrl+Y (redo) moves them in again. With "New folder" already there, the new one is `New folder (2)`. Note whether the desktop takes Ctrl+Alt+N.
52. **Paste as file.** Copy a picture (a screenshot to the clipboard; an image in Firefox: "Copy Image") and press Ctrl+V on the list: `Pasted image <date> <time>.png` opens in an image viewer and looks right. Right-click empty space says "Paste image as file" while a picture is on the clipboard. Copy text in a text editor: right-click empty space says "Paste text as file", and Ctrl+V makes `Pasted text … .txt` with that text (UTF-8, Turkish letters intact). Copy files in Nautilus/Dolphin: Ctrl+V pastes the files, not a text file. Repeat on X11 and on Wayland; a large picture (4K screenshot) too, and once with a clipboard owner that is slow or gone (copy, then close the app that copied): the menu opens within a second.
53. **Links.** Right-click a file ▸ "Create link": `Link to <name>` (a symbolic link, `ls -l` shows the full target path). The same on a folder and on three items at once. Drag a file with Ctrl+Shift inside Gezik: a link in the target folder, the label says "Create link in …". Drag from Nautilus/Dolphin with Ctrl+Shift onto Gezik: X11 makes a link; Wayland copies (no link action in the protocol, a known gap). Right-drag ▸ "Create link here". Ctrl+Z on a link to a folder: the link goes to the trash, the folder's files stay (check the trash too: it holds the link, not the files). With an AltGr layout (Turkish Q), AltGr held while dragging does not make a link.
54. **Drop stack.** Ctrl+Shift+S adds the selection; the strip opens above the status bar. Adding the same file again does not add it twice. Drag files from Nautilus/Dolphin onto it (the source keeps them). Go to another folder: "Copy here" copies them (one Ctrl+Z), "Move here" moves them and they leave the strip. A file that can't be moved (no write permission on its folder) stays on the strip. Delete one stacked file in a terminal: it fades and is left out. With more items than fit, the wheel scrolls the strip and "+N more" shows past 100. Drag an item from the strip to another folder in Gezik and to another app (it leaves the window). "Clear" empties it. View ▸ Drop stack hides it.
55. **History.** After a few jobs (one failing, one cancelled), the status bar shows "History": each job with its time, newest first, and its result ("Done", "1 failed", "Cancelled"); "Show in folder" goes there with the results selected (also from another folder); "Details" lists what failed. View ▸ Operation history opens it while the panel is empty.
56. **Search.** In your home folder press Ctrl+Shift+F (and F3): the search bar opens above the list ("in <folder> ▾", the name field, Content, Filters, Search). Type `*.pdf`: within a few seconds results show while you type (the name cache), in a tab titled "Search: *.pdf" whose address bar ends in `Search "*.pdf"`; Back returns to the folder, Forward to the results without searching again. Folder, Modified and Size columns; sort by Folder; Ctrl+F filters the results. The status bar says "N results in X s" and "Skipped N folders (search.skip)" (`.git`, `node_modules`). Note whether the desktop takes Ctrl+Shift+F or F3 for itself.
57. **Content.** "Content" ▸ type a word that is in a few text files (UTF-8, and one saved as UTF-16 by a Windows editor), Enter: those files, with the line number and the line in the Match column. Pictures and archives are not read. Content safety: make a FIFO (`mkfifo ~/gezik-test/pipe`) and a symlink to a text file outside the scope (`ln -s /etc/os-release ~/gezik-test/os-link`), then a content search in `~/gezik-test`: it ends at once (no hang on the FIFO), the link is a row by its name only and its target is not read (search for `VERSION_ID`, which is only in the target: no result). `Match case` on: the word in capitals is not found.
58. **Scope `/` and mounts.** Scope menu ▸ "Whole drive (/)" with a name that exists in `/usr` and on a mounted second disk (a USB stick under `/media` or `/run/media`): `/proc`, `/sys`, `/dev`, `/run` and the other disk are not walked (one file system, like `find -xdev`; the status bar's folder count stays modest, no result from the stick). Scope "This PC" (every mount): the stick is walked once, even when it is mounted under another mount (outermost roots only; no result appears twice). Esc stops a long search within a moment ("Stopped · N results"); the window never freezes, scrolling and switching tabs stay smooth.
59. **Network.** On an NFS or SMB mount (`/mnt/...`, or a GVfs mount under `/run/user/<uid>/gvfs`): the search works (slower), the bar does not follow typing (no cache: Enter searches), the status bar has no error. While it runs, copy a big file on the local disk: the copy speed hardly drops (the search threads run at `ioprio` idle and low CPU priority). A folder you may not read (`chmod 000`) counts as "1 folder could not be read"; the bar's ▾ ▸ "N folders could not be read…" lists it.
60. **Flat view.** Ctrl+B in a project folder: every file under it in one list (no folders), the Folder column; Ctrl+B again on a file goes to its folder with it selected. View ▸ Show hidden items rebuilds it. A symlink to a folder (`ln -s ~/Documents ~/gezik-test/docs-link`) is a row and is not walked into; a symlink loop (`ln -s . ~/gezik-test/loop`) does not hang it.
61. **Results like a folder.** In the results: Copy, then Ctrl+V in another folder (flat); right-click ▸ "Copy with folders" and paste: the folders under the scope are made; one Ctrl+Z takes the copies and the made folders away. Delete a result (Trash): it leaves the list; Ctrl+Z brings it back. F2 renames in place. Rename two results from different folders with the rename layer. "Show in folder" (Ctrl+Shift+E) goes to the file's folder with it selected. Close and open Gezik with a search tab open: the tab comes back and searches again. On X11 and on Wayland: the bar's fields take the keyboard, Esc gives it back to the list, and nothing else about the window changed (drag and drop, clipboard, the theme).

### Design round 1 (Graphite)

62. **Themes.** The four built-in themes (`light`, `dark`, `classic-light`, `classic-dark`) and `auto` following the desktop's light/dark switch (GNOME and KDE); `reduce-motion = true` stops the hover and popup fades; `density = "compact"`; the classic themes are flat (rows edge to edge, no rounded sheet).
63. **Drawing at 200 %.** On a HiDPI screen: 1 px lines and the sheet's rounded corner are crisp; the 11 px small text (column header, status bar, sidebar section labels) is readable in the font the system picks; the glyphs (Refresh and History arcs, the dot on Drive), the sidebar icons; View ▾ and a long popup menu.
64. **Thin scroll bar.** In a folder with thousands of files: a 6 px bar, no track, the theme's color (never the system's), darker on hover and drag, at least 24 px tall; dragging it follows the pointer to both ends; a click above or below scrolls a page; touchpad two-finger scrolling stays smooth; the sidebar, a long popup menu, the conflict list and the rename layer have the same bar.

### 8b, folder sizes, command palette, saved searches

65. **Folder sizes.** In your home folder the Size column fills in for folders (`…` while worked out, then a size; the ones on screen first). A folder you may not read (`chmod 000`) inside one makes it `≥ …`, and the preview says "Some folders could not be read". A folder with a symlink loop (`ln -s . loop`) gets a size that ends (the link is not followed). Under `/` the walk does not go into other mounted disks (`/media`, `/run/media`, `/proc`). Sort by Size: `…` folders stay last, the focused row stays in place.
66. **Network.** On an NFS or SMB mount with `folder-sizes = "local"` (the default): no folder sizes. Row menu ▸ Calculate folder sizes (and View ▸ Calculate folder sizes): they are worked out, one thread at a time (a local copy at the same time hardly slows down).
67. **Palette and Quick Open.** Ctrl+Shift+P opens the picker with `>` and the caret after it; Ctrl+P lists places first, Alt+Enter opens in a new tab, the last line `Search for "x" in <folder>` starts a search. On X11 and on Wayland: the field takes the keyboard at once (type without clicking), Esc gives it back to the list. Note whether the desktop takes Ctrl+P or Ctrl+Shift+P.
68. **Saved searches.** The search bar's ▾ ▸ Save search…; the sidebar's SEARCHES section runs it (click, middle-click for a new tab, right-click ▸ Rename… / Delete); `~/.config/gezik/settings.toml` has the `[[searches]]` entry.

### 9b1, command line and single instance

Do 70-73 once on X11 and once on Wayland.

69. **Tests first.** In the clone: `cargo test -p gezik-platform instance`. These tests (socket, stale socket, hung and huge callers, peer uid) only compiled on Windows; they never ran on Linux before.
70. **Hand-over.** With Gezik open, from a terminal: `gezik ~/Documents` opens in the same window (if a Documents tab is open, it switches to it) and the terminal gets its prompt back at once. `gezik ~/Documents/x.pdf` opens the folder with `x.pdf` selected; no PDF viewer starts.
71. **Bring to front.** Minimize Gezik, then `gezik ~`. On X11 the window should come back and to the front. On Wayland the activation token is carried but not applied yet: note whether the window comes to the front or only asks for attention (**to be confirmed**). This non-Windows arm has not run anywhere yet.
72. **Help, version, new window.** `gezik --help` and `gezik --version` print to the terminal. `gezik --new-window ~` opens a second window, 32 px offset; Ctrl+N does the same. Close the first window, then the second; open Gezik again: the first window's tabs come back.
73. **Hung or crashed Gezik.** `kill -STOP <pid>`, then `gezik ~`: its own window opens after about 2 s; `kill -CONT <pid>`. `kill -9 <pid>`, then `gezik ~`: a new first Gezik; `gezik /tmp` then goes to it.
74. **Socket folder.** `ls -l $XDG_RUNTIME_DIR/gezik-*` shows an `srw-------` socket and an `-rw-------` lock. `env -u XDG_RUNTIME_DIR gezik ~` (with no Gezik running) creates `/tmp/gezik-$UID` with mode 0700. With `chmod 755 /tmp/gezik-$UID` (and no `XDG_RUNTIME_DIR`), single instance is off: every call opens its own window.
75. **Other users.** `sudo -u <other> env DISPLAY=$DISPLAY gezik /tmp` never reaches your Gezik: it opens its own window (or fails to), but no tab opens in yours.
76. **Off.** `[system] single-instance = false` in settings.toml, restart Gezik: every call opens its own window.

### 9b2, the Trash

Do 78-84 on GNOME (Nautilus) and on KDE (Dolphin).

77. **Tests first.** In the clone: `cargo test -p gezik-platform trash`, `cargo test -p gezik-platform fs`, `cargo test -p gezik-ops delete` and `cargo test -p gezik-ops restore`. The Unix path rules only compiled on Windows and never ran: `freedesktop_original` (absolute `Path=` only in the home trash, relative only in a volume trash, `..` refused), `shared_trash_ok`/`own_trash_ok`, `only_items_in_a_bin_with_their_own_record_count` (only `files/x` with its own `info/x.trashinfo` may be deleted for good), `put_back_never_goes_through_a_link` with a symlink, `only_a_plain_file_is_read_as_a_record` and `a_fifo_or_a_link_is_never_opened` with a FIFO, the `.trashinfo` removal tests. Then `cargo test --release -p gezik-platform ten_thousand -- --ignored` (10,000 items in under 1 s).
78. **One list.** Trash a file in the home folder and one on a USB stick (ext4, and a FAT stick mounted for your user) with Nautilus/Dolphin. The sidebar's Trash (under the drives) shows both: the home trash (`~/.local/share/Trash`) and the stick's `.Trash-$(id -u)`, each with its own name, Original location and Date deleted. Turkish and spaced names (`çğış ad.txt`, stored as `%C3%A7…%20ad.txt` in `Path=`) show decoded.
79. **Put Back.** The item goes back to its folder; `info/<name>.trashinfo` is gone and `files/<name>` is gone; the desktop's Trash no longer shows it. Folder deleted meanwhile: made again. A file of the same name there: the conflict list (Keep both `a (2).txt`; Replace puts the existing one in the trash). Ctrl+Z puts it back in the trash. An item whose `.trashinfo` you broke by hand (`Path=` removed) shows a blank place and Put Back asks for a folder.
80. **Delete for good and Empty.** Delete and Shift+Delete in the trash ask `Delete N items permanently?` / `This cannot be undone.`. `Empty Trash…` (background menu, sidebar row menu, palette) asks `Empty the Trash?` with the count. After it, `files/` and `info/` of every bin are empty and no orphan `.trashinfo` is left. Cancel an empty with a big folder: what is left is still in the desktop's Trash, each with its `.trashinfo`.
81. **Bin rules.** On a stick: `sudo mkdir -m 1777 /media/$USER/usb/.Trash; mkdir /media/$USER/usb/.Trash/$(id -u)` and trash a file there by hand (a `files/x` plus `info/x.trashinfo`): it is listed. `sudo chmod -t /media/$USER/usb/.Trash` (no sticky bit): that `.Trash/$UID` is no longer read. Remove `.Trash-$UID` and `ln -s /tmp /media/$USER/usb/.Trash-$(id -u)`: it is not read. Clean up both afterwards.
82. **Other home trash.** `XDG_DATA_HOME=/tmp/x gezik` with a `/tmp/x/Trash/files` + `info` made by hand: its Trash shows that bin, not `~/.local/share/Trash`.
83. **Live.** With the trash open, trash a file in Nautilus/Dolphin: it shows within about 1 s. Go to another folder, then trash another file: Gezik reads nothing (`strace -f -e trace=openat -p $(pidof gezik)` shows no `Trash` path).
84. **Refused.** In the trash: Ctrl+C, Ctrl+X, Ctrl+V, Ctrl+Shift+N, Ctrl+D, F2 and a `[[commands]]` key do nothing and the status bar says `Not available in the Trash`; Enter/double-click opens nothing, with a note; rows can't be dragged out.

### 9b3, the command line and --unregister

Do 86-92 on GNOME and on KDE.

85. **Tests first.** In the clone: `cargo test -p gezik-platform system` and `cargo test -p gezik system_changes`. The Unix arms (`replace_symlink` refusing a file or a foreign link, the sweep by exe name) only compiled on Windows; they never ran on Linux.
86. **Panel.** Palette ▸ `System Integration…` and View ▸ `System Integration…` (last item): a box with three rows (`Command line (PATH)` Off/Add, `Changes made: 0`, `Undo all system changes`). Esc closes it; typing in the field does nothing; Up/Down and click work.
87. **Add, no `~/.local/bin`.** Move `~/.local/bin` away first if you have one. `Add gezik to PATH` asks first and names every place it writes. After it: `~/.local` and `~/.local/bin` exist with mode 0755, `ls -la ~/.local/bin` shows the `gezik` link pointing at the real binary. If `~/.local/bin` is not in Gezik's own `PATH`, the hint box shows `export PATH="$HOME/.local/bin:$PATH"` and `Copy` puts it on the clipboard. When `~/.local/bin` is already on `PATH` (most distributions' `~/.profile` adds it once it exists; log out and in) the hint does not show. A new terminal's `gezik .` opens the folder in the running Gezik.
88. **Someone else's file, dotfiles.** Remove gezik from PATH, then `echo hi > ~/.local/bin/gezik` and Add again: the file is untouched and the panel says `<place> was not made by Gezik; left alone` (Off, no button); the same with `ln -s /usr/bin/true ~/.local/bin/gezik`. Clean up. Then make `~/.local/bin` itself a link (`mv ~/.local/bin ~/dots-bin; ln -s ~/dots-bin ~/.local/bin`): Add works (the `gezik` link lands in `~/dots-bin`), and Remove and `--unregister` take the `gezik` link back but never touch the `~/.local/bin` link.
89. **Gezik moved, odd path.** Copy the build to `~/Uygulamalar/gé zik/gezik` and run it from there: the panel says `Gezik's exe moved; gezik still starts <old path>` with `Update`; after Update, `ls -la ~/.local/bin/gezik` shows the new path (Turkish letters and the space intact) and `gezik .` starts it.
90. **`--unregister`.** `gezik --unregister` prints one line per change and `echo $?` gives 0. The link is gone; `~/.local/bin` and `~/.local` are removed if Gezik made them and they are empty. With another file in `~/.local/bin`: the folder stays, `left (not empty)`, exit 1. A second run: `No system-changes.toml: …`, `Nothing of Gezik's was found.` (0).
91. **No journal.** Add, delete `~/.config/gezik/system-changes.toml`, then `gezik --unregister`: only the link that points at this Gezik is swept; `~/.local/bin` stays. Write `version = 9` into a fresh `system-changes.toml`: the panel says `Fix or delete system-changes.toml first`, Add fails, `--unregister` exits 2 and the file is unchanged.
92. **Two at once.** With Gezik open and gezik added, run `gezik --unregister` from a terminal: the report is complete; opening the panel again shows Off.

### 9a3, the Info window

93. **Opening.** Alt+Enter on a file (the list has the keyboard), right-click ▸ Properties, and the command palette's "Get Info": the panel on the right with Kind, Size, Where, Modified, Last opened (Created only on file systems that keep it: ext4, btrfs, xfs). No Hidden/Locked row, no Open with row. Alt+Enter in the search bar's field still searches in a new tab. Esc and Done close it.
94. **Permissions, owner, group.** As macOS items 103-104 with `ls -l`: ticking boxes, Octal 600, 4755 refused, Group ▾ lists your groups, a group you are not in and owner `root` say "Requires administrator: 1 item not changed". A setuid file you own (`chmod 4755`) keeps `rws` after a group change. A file replaced from another terminal while the window is open is not changed ("changed since"). A setgid folder (`chmod g+s`) shows "Special: setgid (not changed here)". `setfacl -m u:nobody:r <file>` (if `acl` is installed): the access control note shows.
95. **Apply to enclosed items.** As macOS item 106 (with a symlink inside pointing outside): the link and its target are untouched, the script stays runnable, Cancel in the operations panel stops a big one, Ctrl+Z puts every item back.
96. **Links.** Get Info on a symlink: permission boxes greyed; Group ▾ changes the link's own group (`ls -l` on it), `stat -L` shows the target's group unchanged. Write down the glibc version (`ldd --version | head -1`): below 2.32 permission changes use the fallback. Also run `cargo test -p gezik-platform attrs` and `cargo test -p gezik-ops attrs` and report failures.

### 9b5, cloud drives

97. **Probe first.** `cargo test -p gezik-platform cloud` and `cargo run -p gezik-platform --example cloud_probe`; paste the output. The Linux readers (`/proc/mounts`, gvfs names) only compiled on Windows.
98. **Sidebar.** With `~/Dropbox` and `~/OneDrive` (make empty folders if you have none), an rclone mount `rclone mount gdrive: ~/Drive\ Boşluk` and GNOME Online Accounts' Google Drive: the `CLOUD` heading after the pinned items lists them; the rclone one shows `Drive Boşluk` (the space and Turkish letters intact); the GNOME one shows `Google Drive` with `user@gmail.com` in its tip. A click goes there. `[sidebar] cloud = false`: the heading goes away.
99. **No state, no commands.** Rows in those folders have no badge and the status bar says no cloud words. The palette has no `Always Keep on This Device` / `Free Up Space`, and the row menu has no cloud items.
100. **Unmount.** `fusermount -u ~/Drive\ Boşluk`, then plug a USB stick in or out (the drive change reloads the places): the rclone row goes away; nothing crashes.

### 9b4, the default file manager

Do 103-109 on GNOME and on KDE, under X11 and Wayland. `Restore` (or `gezik --unregister`) always gives folders back.

101. **Build first.** The app crate's Linux code for 9b4 never compiled: `single_instance.rs`'s FileManager1 wiring (start/stop, the name-taken flag) and `main.rs`'s `--dbus` start. The app crate cannot be cross-built on the Windows machine (fontconfig). In the clone: `cargo build --release -p gezik` and `cargo clippy -p gezik --all-targets -- -D warnings`. Report any error before going on.
102. **Tests.** `cargo test -p gezik-platform linux::`, `cargo test -p gezik-platform system`, `cargo test -p gezik system_changes`. The D-Bus codec and FileManager1 tests ran on Windows; the socket side (auth, `Hello`, `RequestName`) never ran against a real bus.
103. **Make default.** `sha256sum ~/.config/mimeapps.list` first (note if there is none). The panel's first row is `Default file manager` (Off, `Make default`); its confirmation names `~/.local/share/applications/gezik.desktop`, `~/.local/share/dbus-1/services/org.freedesktop.FileManager1.service` and `mimeapps.list`. Run the binary once from `/media/…` or `/run/media/…`: the first paragraph warns about the place. After it: `xdg-mime query default inode/directory` gives `gezik.desktop`; `xdg-open ~` opens the home folder in Gezik, with Gezik open (same window) and closed. A `~/.config/gnome-mimeapps.list` (or `kde-mimeapps.list`) that has `inode/directory` is changed too; one without it is left alone.
104. **FileManager1.** `busctl --user introspect org.freedesktop.FileManager1 /org/freedesktop/FileManager1` lists `ShowFolders`, `ShowItems`, `ShowItemProperties`. Firefox ▸ Downloads ▸ "Show in Folder": the folder opens in Gezik with the file selected, with Gezik open and with Gezik quit (the bus starts `gezik --dbus` through the `.service`). Does the user's `.service` win over the system one (Nautilus's or Dolphin's) under dbus-daemon and under dbus-broker? Write down which bus you have (`systemctl --user status dbus-broker`). `busctl --user call org.freedesktop.FileManager1 /org/freedesktop/FileManager1 org.freedesktop.FileManager1 ShowFolders ass 1 trash:/// ""` shows the Trash view.
105. **Another file manager has the name.** Start Gezik while Nautilus (or Dolphin) already owns `org.freedesktop.FileManager1` (`busctl --user list | grep FileManager1`): the panel row ends with `Another file manager answers "Show in folder"`; Gezik keeps working.
106. **Links and someone else's files.** Make `~/.config/mimeapps.list` a link (as dotfile managers do): `Make default` refuses and writes nothing. A `gezik.desktop` in `~/.local/share/applications` whose `Exec` is not a gezik binary (`Exec=/usr/bin/true`): refused, untouched.
107. **Gezik moved.** Make default, quit, copy the build to `~/Uygulamalar/gé zik/gezik` and run it: about 2 s later the `Gezik moved` box offers `Repair` / `Later`; after `Repair`, `xdg-open ~` starts the new path (`grep Exec ~/.local/share/applications/gezik.desktop` has the Turkish letters and the space intact). When Gezik is the default, the same 2 s check starts FileManager1 (item 104 works without `--dbus`).
108. **Restore and `--unregister`.** `Restore`: `sha256sum ~/.config/mimeapps.list` is the same as before 103 (or the file is gone if Gezik made it), `gezik.desktop` and the `.service` are gone, `busctl --user list | grep FileManager1` no longer shows Gezik. Make default again, quit, `gezik --unregister`: the same, `echo $?` gives 0.
109. **Idle.** With Gezik not the default, `busctl --user list` shows no connection of Gezik's (no D-Bus while off).

### 9b6, eject and Connect to Server

You need a USB stick and an SMB share (a NAS, or a folder shared from another machine; one guest share and one with a user and password if you can).

110. **Build and tests first.** The app crate's 9b6 code (Connect to Server, the user name and password questions, Eject in Gezik's menus) never compiled for Linux. In the clone: `cargo build --release -p gezik`, `cargo clippy -p gezik --all-targets -- -D warnings`, `cargo test -p gezik-platform network`, `cargo test -p gezik-platform eject`, `cargo run -p gezik-platform --example eject_probe`; paste the output. The gio and udisksctl arms ran only against a fake runner on Windows.
111. **Eject with gio.** A folder on a USB stick open in two tabs, sidebar ▸ `Eject` (or palette ▸ Eject): the tabs go to This PC, the stick leaves the desktop too, the status says `… can be removed`. With a file on the stick open in `less`: `The drive is in use…`. The `/` or `/mnt/x` row has no `Eject`.
112. **Eject without gio.** Start Gezik with gio hidden from `PATH` (a `PATH` without gio's folder, or a machine without gio; write down how): the `udisksctl unmount` + `power-off` route works; with no `udisksctl` either, `Ejecting needs gio or udisksctl`.
113. **Connect to Server.** Ctrl+K, `smb://nas/foto`: a guest share opens at once; on a share with a password Gezik first asks the user name (`$USER` suggested), then the password (dots), the gvfs folder opens, the sidebar shows `foto on nas`. **Question order (deviation 9):** check that gio's user / domain / password order is right (if not, write what gio asked). `ps aux | grep gio` while connecting shows no password. Without gio: `Connecting to servers needs gvfs (the gio command).`
114. **Disconnect.** `foto on nas` ▸ `Disconnect`: the gvfs mount goes away (`gio mount -l`), its tabs go to This PC. A share mounted from the file manager (outside Gezik) shows in DRIVES only after the next drive change or a restart (known).

### 9b7, the administrator helper

You need a desktop session with a polkit agent (GNOME, KDE, …) and `pkexec` (`ls -l /usr/bin/pkexec`). The test folder: `sudo mkdir /opt/gt && sudo chmod 755 /opt/gt` (root's). The automatic tests never ask for a password; every prompt below is one you answer by hand. Remove `/opt/gt` at the end.

115. **Build and tests.** In the clone: `cargo build --release -p gezik`, `cargo clippy -p gezik --all-targets -- -D warnings`, `cargo test -p gezik-core elevated` (includes `sh_words_survive_a_real_sh`, the real `sh` round trip; it must show `ok`), `cargo test -p gezik-platform secure`, `cargo test -p gezik-platform elevate`, `cargo test -p gezik-ops elevated`, `cargo test -p gezik elevated::`, `cargo test -p gezik admin::`; paste the output. The Linux arms (`SYS_renameat2` with `RENAME_NOREPLACE`, opening a FIFO with `O_NONBLOCK`, dropping setuid/setgid in copies, the hard-link refusal, `others_can_change`, the pkexec launch) were only compiled on Windows: these are their first runs. The app crate's 9b7 code was never compiled for Linux.
116. **pkexec.** Paste a file into `/opt/gt`: `Access denied` → Details → `Retry as administrator` → Gezik's own box `Copy 1 item as administrator` with `Gezik asks the system for administrator rights to do exactly this:` and `Copy /…/f to /opt/gt/f`, buttons `Continue` / `Cancel` → the desktop's polkit window (`Authentication is needed to run '/…/gezik' as the super user`); after the password the file is there, root's (`ls -l`), the row says `Done`, History `Copy 1 item as administrator`. Cancel in the polkit window: `Not done: the administrator prompt was cancelled`. `ps ax | grep "gezik --elevated" | grep -v grep` is empty once the row is done. Ctrl+Z: `Undoing this needs administrator rights.` with its list → pkexec → the file is gone; Ctrl+Y asks again and brings it back.
117. **The "other programs" warning.** Run from the clone (your own folder): the box of 116 ends with `Gezik is in a folder other programs can change.` `sudo install -D target/release/gezik /usr/local/lib/gezik-test/gezik` and run that one: no such line. Write down if it shows, or is missing, where it should not. Remove `/usr/local/lib/gezik-test` afterwards.
118. **Get Info and chown.** `sudo touch /opt/gt/f`; in the Info window make yourself the owner → `Change as administrator…` → the box lists `Set the owner of /opt/gt/f to user <uid> and group <gid>` (numbers) → pkexec → changed (`ls -ln`); Ctrl+Z → pkexec → root again.
119. **No agent / no pkexec.** Stop the polkit agent on the desktop (e.g. `pkill -f polkit-gnome-authentication-agent` / `polkit-kde-authentication-agent`) and Retry as administrator: `No polkit authentication agent is running…`. On a machine without pkexec: `Administrator operations need pkexec (polkit).`; without one, write "not tried". Log out and in to get the agent back.
120. **Links.** `sudo mkdir /opt/gt/d && sudo ln -s /etc /opt/gt/d/etc-link`; delete `/opt/gt/d` → Retry as administrator → `This will delete 1 item permanently as administrator. It cannot be undone.` (`Cancel` is the Enter button) → pkexec → `/opt/gt/d` is gone, `/etc` is where it was (`ls /etc | head`). Ctrl+Z does not bring it back.
121. **Refusals.** (a) Hard links: `sudo sh -c 'echo x > /opt/gt/h && ln /opt/gt/h /opt/gt/h2'`; change `h`'s permissions in the Info window → `Change as administrator…` → pkexec: `It has several names; Gezik does not change it as administrator`, `ls -l` unchanged; deleting `h` as administrator works and `h2` still says `x`. (b) setuid: `cp /bin/ls ~/s && chmod 4755 ~/s`, copy it into `/opt/gt` as administrator: `ls -l` shows no `s`, owner root. (c) FIFO: `mkfifo ~/fifo`, copy it into `/opt/gt` as administrator: `A device, pipe or socket; left alone`, nothing hangs. (d) Rename as administrator in `/opt/gt` to a free name works (`renameat2` with `RENAME_NOREPLACE`); write down the file system (`df -T /opt`). (e) Delete `/opt` itself (select it in `/`) → Retry as administrator: the status bar says `Not done as administrator: /opt (…)`, no polkit window.

### 9b10, icon theme, thumbnails and Open With

A GNOME or KDE session. Have a folder with a few PDFs, videos and pictures, a `.txt`, a `.tar.gz` and a file with no extension. (Numbers go on from 9b7's 121. If another part's list reaches master first with the same numbers, the branch that merges second renumbers its items to follow.)

122. **Build and tests first.** The app crate's 9b10 code (Open With ▸ and Other… on Linux, the Info window's row and Change All…) has never been compiled for Linux. In the clone: `cargo build --release -p gezik`, `cargo clippy -p gezik --all-targets -- -D warnings`, `cargo test -p gezik-platform linux::`, `cargo test -p gezik-platform open_with`, `cargo test -p gezik finder_menu`, `cargo test -p gezik system_changes` (on Linux this also runs the symlink tests and `links_in_the_cache_are_not_followed`); paste the output. Write down `stat -c %s target/release/gezik` here and on master (the size this part adds on Linux).
123. **Theme icons.** Open the folder in the list and the grid with each theme you have; at least the default one (GNOME: Adwaita; KDE: Breeze) and Papirus (`sudo apt install papirus-icon-theme`). Adwaita, Breeze and Papirus are mostly SVG, so most items keep Gezik's own icons: that is expected; write down which items show theme icons. Then a PNG theme: `sudo apt install oxygen-icon-theme` (or your distro's), `gsettings set org.gnome.desktop.interface icon-theme oxygen` (KDE: System Settings ▸ Icons ▸ Oxygen), restart Gezik: `.txt`, `.pdf`, `.tar.gz`, pictures and folders show Oxygen's icons at 16 px (list) and in the grid (bigger); nothing is blurry or cut. The file with no extension shows a generic document icon. Set the theme back afterwards.
124. **Special folders and drives.** In the PNG theme: Home, Desktop, Documents, Downloads, Music, Pictures, Videos show their own folder icons in their parent; This PC: `/` a hard disk, a USB stick a removable drive, an SMB share (9b6) a remote folder. With `[view] icons = "gezik"` everything is Gezik's own again.
125. **The theme name.** `gsettings get org.gnome.desktop.interface icon-theme` (or kdeglobals' `[Icons] Theme`) names the theme Gezik used (123). With an unknown name set (`gsettings set … icon-theme nosuch`), Gezik starts and falls back (Adwaita, then hicolor) without errors. Write down your desktop (`echo $XDG_CURRENT_DESKTOP`).
126. **Thumbnail cache.** Open the folder in Nautilus (or Dolphin) in its grid first so it makes thumbnails (`ls ~/.cache/thumbnails/*/ | wc -l` grows). Gezik's grid (thumbnails on) shows the same thumbnails for PDFs and videos, and the preview pane too. `touch` a PDF: Gezik shows its icon (the thumbnail is stale) until Nautilus makes a new one and Gezik reloads (F5). Names with `ş ğ`, a space, `;`, `'`, `!`, `(1)`: their thumbnails show too. **The `;` name matters most** (Gezik escapes it as `%3B`, like GLib; never checked against a real cache): write down whether `a;b.pdf` shows its thumbnail after Nautilus and after Dolphin, and any other name whose Dolphin thumbnail Gezik does not find. A file on an SMB share or an rclone mount: no download happens (watch the network) and its cached thumbnail shows if there is one.
127. **Open With.** Right-click a `.png`: `Open With ▸` lists the default first as `… (default)`, then the others; the same as Nautilus' "Open With" (write down any difference). Choose one: it opens the file. Select a `.png` and a `.jpg`: only the apps that open both. A folder, or a selection with a folder: no Open With. **Hostile name:** make a file named `a b $(touch ~/pwned) ;'x' %f.txt` (`touch -- 'a b $(touch ~/pwned) ;'"'"'x'"'"' %f.txt'`) and open it with Text Editor (or Kate) from Open With: the editor shows that exact name as one file (no second file, no `%f` turned into something else) and `ls ~/pwned` finds nothing. Terminal apps (vim, nano) are not listed.
128. **Other….** Open With ▸ `Other…`: the `Open With` box; typing `gimp` (or any installed app) says `Opens with GIMP`, `zzz` says `No app has that name`, an empty field says `Type part of the app's name`; Enter opens the file with it; Esc does nothing.
129. **Info window and Change All….** Alt+Enter on a `.txt`: the "Open with" row shows the default; choose another app: the note says `Change All… opens every file of this type with …`; `Change All…` ▸ `Change All`: the note says `… files open with … now`, and `xdg-mime query default text/plain` gives that app's id; double-clicking a `.txt` (Gezik's Open) uses it. `~/.config/mimeapps.list` kept its other lines (diff with a copy made before). With `~/.config/gnome-mimeapps.list` (or `kde-mimeapps.list`) holding `text/plain`, that one changed too; one without it was not made. **Symlinked list:** make `~/.config/mimeapps.list` a link to a file elsewhere (`mv ~/.config/mimeapps.list ~/dots/ && ln -s ~/dots/mimeapps.list ~/.config/`): after Change All it is still a link (`ls -l`) and the file in `~/dots` holds the change. `gezik --unregister` does not undo this (the user's own choice). Set the default back afterwards.
130. **Idle and speed.** A folder of 2,000 mixed files in the grid scrolls as before (thumbnail lookups are one `stat` each when there is none). The Open With submenu opens without a visible wait (or shows `Loading…` and fills). Idle memory as master (`ps -o rss= -p $(pidof gezik)` after a minute, both builds).

### 10d, grouping

Numbers stay as they are when branches merge: if this range meets one merged earlier, the later branch moves its items to the next free ten and writes the old number in brackets in the results file; gaps are not filled. 10d starts at 150 as its plan decided (other open branches take the numbers before it).

150. **Tests first.** `cargo test -p gezik-core group layout selection drag`, `cargo test -p gezik view::`; paste.
151. **Group by Date and keys** (GNOME and KDE): View ▸ Group by ▸ Date in `~/Downloads`; headers, ↑/↓ skip them, Home/End, PgUp/PgDn; a header click closes/opens; right-click menu (`Collapse All Groups`, `Expand All Groups`, `Group by ▸`).
152. **Grid and rubber band.** Ctrl+Shift+2; a rubber band across a header; Ctrl+A with a group closed counts only the shown items.
153. **Trash.** The Trash grouped by Date shows the deletion dates (`~/.local/share/Trash/info/*.trashinfo` `DeletionDate`). Do not empty or restore anything.
154. **Remembered.** Restart: the folder keeps its grouping (`~/.config/gezik/views.toml` has `group = "date"`).

### 10c, the sidebar folder tree

Numbers stay as they are when branches merge: if this range meets one merged earlier, the later branch moves its items to the next free ten and writes the old number in brackets in the results file; gaps are not filled. 10c starts at 160 as its plan decided (other open branches take the numbers before it). GNOME and KDE, X11 and Wayland.

160. **Tests first.** `cargo test -p gezik-core tree`, `cargo test -p gezik sidebar`; paste.
161. **`/` and home.** Open the arrows of `/` (DRIVES) and Home: folders only, natural order, dot folders only with Ctrl+H; `/proc` opens without hanging.
162. **Link loop.** `mkdir -p ~/loop/a && ln -s ~/loop ~/loop/a/back`; pin `~/loop`, open `a` then `back`: no arrow on `back`, nothing hangs.
163. **Huge branch.** As macOS 212 under `/tmp/gezik-tree`; the window stays usable while it reads.
164. **Keyboard and Orca.** As macOS 217 with Ctrl+Enter; Orca reads expanded/collapsed.
165. **Follow.** As macOS 218.

### 10f, view rules

Numbers stay as they are when branches merge: if this range meets one merged earlier, the later branch moves its items to the next free ten and writes the old number in brackets in the results file; gaps are not filled. 10f starts at 170 as its plan decided (other open branches take the numbers before it). GNOME and KDE, X11 and Wayland.

170. **Tests first.** As macOS 220; paste.
171. **Path, own view, content.** As macOS 221–223 (Ctrl instead of ⌘; Ctrl+H for hidden items; `/tmp/gezik-rules` the same).
172. **Kinds.** `kind = "removable"` with a USB stick under /media or /run/media; `kind = "trash"` (open only); a share mounted by the file manager (gvfs) is not `network` (a known limit) — note what happens.
173. **Mistakes and reloads.** As macOS 225.

### 9b9, tray icon, global shortcut, start at login

Numbers start at 131: 122–130 are 9b10's (PR #56), so whichever of the two merges second keeps these numbers as they are. Try KDE (X11 and Wayland) and GNOME if you can; write down the desktop and session for each item.

131. **Build and tests.** In the clone: `cargo build --release -p gezik`, `cargo clippy -p gezik --all-targets -- -D warnings`, `cargo test -p gezik-platform sni portal hotkey_x11 tray:: hotkey::`, `cargo test -p gezik resident:: integration:: system_changes::`; paste the output. The StatusNotifierItem server, the portal client and the X11 grab were only compiled on Windows: these are their first runs.
132. **Tray on KDE.** System Integration ▸ `Tray icon` ▸ `Turn on`: an icon (the theme's `system-file-manager`) in the system tray, tooltip `Gezik`. Click: the window hides; click again: it comes back. Right click does nothing (no menu, by design). Close the window: the first time `Gezik keeps running in the tray` with `OK` / `Quit Gezik`; OK hides it. **Wayland:** Gezik minimizes the window instead of hiding it (a hidden window loses its surface; deviation 20): write down whether it goes to the task bar and comes back on a click. `busctl --user list | grep StatusNotifierItem` shows `org.kde.StatusNotifierItem-<pid>-1`; after `Turn off` it is gone (the connection closed).
133. **The panel restarts.** `plasmashell --replace &` (or `kquitapp6 plasmashell; plasmashell &`): Gezik's icon comes back by itself.
134. **GNOME.** Without the AppIndicator extension: `Turn on` says `No tray on this desktop (it needs a StatusNotifier host …)`, the row stays `Off` and closing the window quits. Turn the extension on and press `Turn on` again: the icon comes (Gezik tries again). With the extension (Ubuntu has it): as 132. `gezik --background` with the tray on but no host: the window opens.
135. **Shortcut on X11.** An X11 session: `Turn on` ▸ `super+shift+e`: from another app it shows Gezik; with Gezik in front it hides (tray on) or minimizes (tray off); holding the keys does not flicker the window (repeats dropped). CapsLock and NumLock on: still works. `ctrl+alt+e` is refused (`uses Ctrl+Alt, which types AltGr characters`), `super+l` too (`belongs to the system …`). A chord another app grabbed (e.g. one set in the desktop's own shortcut settings for a command): `… is used by another app.` with `Choose Another` (write down if the desktop's own shortcuts are not seen as taken: they often are not X grabs).
136. **Shortcut on Wayland.** KDE 6 or GNOME 48+: `Turn on` ▸ `super+shift+e`: the desktop shows its own shortcut dialog (write down what it shows and whether it kept Super+Shift+E); after you accept, the shortcut works. Cancel the desktop's dialog: write down what Gezik says. A desktop without the GlobalShortcuts portal: `This desktop does not support global shortcuts.`
137. **Start at login and --background.** `Turn on` in `Start at login`: `~/.config/autostart/gezik.desktop` (`desktop-file-validate` clean, `Exec="…/gezik" --background` with the absolute path, `NoDisplay=true`); log out and in: Gezik in the tray without a window (tray on and a host) or with its window. `Turn off` removes the file; `gezik --unregister` removes it too. Memory (`ps -o rss= -p $(pgrep -x gezik)`): defaults, tray + shortcut on, hidden; write the three numbers (spec: the D-Bus connection ≤ +0.3 MB).

## Known gaps (not bugs)

- **Folder sizes (8b):** a change deep inside a subfolder made outside Gezik shows the old size for up to 5 minutes (F5 works it out again). Search results and the flat view show no folder sizes.
- **Search (8a):**
  - Search results and the flat view don't follow changes made outside Gezik until F5 (Gezik's own jobs do update them).
  - Content search always reads the disk (no index); the Folder and Match columns are only in result lists.
  - "Whole drive (/)" stays on one file system, like `find -xdev`: other disks are searched from "This PC" or their own folder.
- **Icons and type names:**
  - Theme icons are PNG only: a theme that has an icon only as SVG (Adwaita, Breeze, Papirus mostly) keeps Gezik's own icon for it.
  - Thumbnails come from the freedesktop cache (made by Nautilus, Dolphin …; Gezik makes none) and Gezik's own decoders.
  - Types read like Windows ("TXT File").
  - The root crumb reads "This PC", and the sidebar sections are "FOLDERS" and "DRIVES".
- **Context menu:** the right-click menu is Gezik's own: Open With ▸ lists the MIME apps (terminal apps like vim are left out), but no desktop's scripts or actions.
- **Open With:** Linux keeps no per-file app: the Info window's "Open with" row shows the type's default, and choosing another app only marks it until `Change All…` (a reload shows the default again). `Change All…` is not in History and Ctrl+Z does not undo it.
- **Clipboard:**
  - On X11, a very long file list (above ~256 KB) isn't handed to other apps (no INCR when writing).
  - On Wayland, the clipboard works only while Gezik has the focus.
- **Drag and drop on Wayland:**
  - Drops from other apps don't see the modifier keys.
  - Gezik always reports "copy" to the source and does a move itself, so the cursor may say copy.
- **Trash:** Delete puts items only in the home trash and `.Trash-<uid>`; the admin trash `$topdir/.Trash/<uid>` is listed (9b2) but never written to. Items can't be dragged or cut out of the trash (Put Back does it), and files can't be dropped onto it.
- **Theme:** with no xdg-desktop-portal, `auto` stays light.
- **Tray (9b9):** the tray icon has no menu (a click shows or hides Gezik; quit from the first close question or by turning the tray off). On Wayland closing to the tray minimizes the window instead of hiding it.
- **No .desktop file, icon or MIME integration:** it is a bare binary.
- Folders dropped from outside can't be pinned by dragging.
- On Wayland, drops from other apps can't make links (no link action in the protocol); Ctrl+Shift drags inside Gezik do.
- Drags from Gezik to other apps never offer a link, only copy or move.
- Double-clicking `.wim` doesn't open it; use Extract.

## When you are done

Commit `linux-test-results.md` (and any fixes) in English, with no `Co-Authored-By` or Claude lines, and push to the branch you tested (`feat/batch-ops-5d`, or master after the merge). A Claude Code session on the Linux machine can do this. Without a clone there, send the file back to be committed.
