# Gezik on Linux: portable build and manual test

So far Gezik has run on Linux only in a container: Xvfb and a headless sway, driven by `scripts/linux/test.sh` and `gui.sh` (see `2026-10-06-linux-ekran-testi.md`). It has never run on a real desktop, with Nautilus or Dolphin, a real compositor, a portal, HiDPI or a real trash. This file is for a person testing the state after sub-project 5 (5a-5d) on a real Linux desktop, from the portable build.

Write the results into `docs/superpowers/notes/linux-test-results.md`, in the same style as `macos-test-results.md`:
- Start with the date, the build's SHA, the distribution and version, the desktop and its version, the session (`echo $XDG_SESSION_TYPE`), the display scale, the glibc (`ldd --version | head -1`) and the keyboard layout.
- Add a summary table, then a section per item.
- Give PASS, FAIL or NOT TESTED for every item. Items 9-14 and 33 need **both** X11 and Wayland: give one result for each.
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

## Known gaps (not bugs)

- **Icons and type names:**
  - No system icons: every item has Gezik's own icon, with no theme icons.
  - Thumbnails come only from Gezik's own decoders.
  - Types read like Windows ("TXT File").
  - The root crumb reads "This PC", and the sidebar sections are "FOLDERS" and "DRIVES".
- **Context menu:** the right-click menu is Gezik's own: no "Open With" app list, and no desktop's scripts or actions.
- **Clipboard:**
  - On X11, a very long file list (above ~256 KB) isn't handed to other apps (no INCR when writing).
  - On Wayland, the clipboard works only while Gezik has the focus.
- **Drag and drop on Wayland:**
  - Drops from other apps don't see the modifier keys.
  - Gezik always reports "copy" to the source and does a move itself, so the cursor may say copy.
- **Trash:** only the home trash and `.Trash-<uid>`. The admin trash `$topdir/.Trash/<uid>` isn't used.
- **Theme:** with no xdg-desktop-portal, `auto` stays light.
- **No .desktop file, icon or MIME integration:** it is a bare binary.
- Folders dropped from outside can't be pinned by dragging.
- Double-clicking `.wim` doesn't open it; use Extract.

## When you are done

Commit `linux-test-results.md` (and any fixes) in English, with no `Co-Authored-By` or Claude lines, and push to the branch you tested (`feat/batch-ops-5d`, or master after the merge). A Claude Code session on the Linux machine can do this. Without a clone there, send the file back to be committed.
