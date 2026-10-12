# Release check on macOS (Run 3)

A focused pre-release pass on a Mac, chosen by risk from steps 9–14 (items 147 onward of
`../macos-test.md` and `14-macos.md`), weighted towards macOS code that has never run on a
Mac: AudioQueue playback, the pdfium worker, Finder tags and Spotlight, the field readers
(HEIF included), property search and name templates, the settings window, themes from
GitHub, settings export and import, and portable mode. A short smoke pass of the core comes
first. Each item keeps its original number in brackets (`[13c 373]` means item 373 of the
13c part; numbers repeat between parts, so the part is part of the reference).

Written for the maintainer or for Claude Code on the Mac (Apple M2 Pro, macOS 26.5,
Turkish-QWERTY-PC). Claude Code can do the Terminal parts and drive the window with
synthetic events as in Run 2; items marked **(hand)** need a person (ears, eyes on motion,
a password, Gatekeeper's Open Anyway).

## 1. Build and run

```sh
xcode-select -p || xcode-select --install       # C/C++ compiler (UnRAR is C++)
rustup --version || curl https://sh.rustup.rs -sSf | sh
cd ~/gezik 2>/dev/null || git clone https://github.com/wenlar/gezik.git ~/gezik && cd ~/gezik
git fetch origin && git checkout master && git pull --ff-only
git log -1 --oneline                            # write this SHA into the results
cargo build --release -p gezik
export REPO=$PWD GZ=$PWD/target/release/gezik
rm -rf /tmp/gezik-cfg /tmp/gezik-rel && mkdir -p /tmp/gezik-cfg /tmp/gezik-rel
GEZIK_CONFIG_DIR=/tmp/gezik-cfg "$GZ" /tmp/gezik-rel
```

- Always start Gezik with `GEZIK_CONFIG_DIR=/tmp/gezik-cfg` (except the portable items, which
  say so). Start from an empty `/tmp/gezik-cfg` so that the ffmpeg and pdfium download offers
  appear; Gezik puts them in `/tmp/gezik-cfg/tools/`.
- To make sure Gezik downloads its own ffmpeg, start it with a PATH that leaves Homebrew out:
  `PATH=/usr/bin:/bin:/usr/sbin:/sbin GEZIK_CONFIG_DIR=/tmp/gezik-cfg "$GZ" /tmp/gezik-rel`.
- Note `ls -l target/release/gezik` (exe size) in the results.

### Results

Write a new section **`## Run 3 (release check)`** at the end of
`docs/superpowers/notes/macos-test-results.md` (do not change Run 1 or Run 2). Start it with:
date, branch and short SHA, machine (model, chip), macOS version and build (`sw_vers`),
system languages, keyboard layout, `rustc --version`, exe size, who ran it (person or Claude
Code) and how the window was driven. Then the summary table and one entry per item (see §6).
Commit on a new branch `test/macos-run3` from master; fixes on `fix/macos-3`.

## 2. Safety rules (for an automated runner too)

- **Only test places.** Work in `/tmp/gezik-rel`, `/tmp/gezik-cfg` (and `/tmp/gezik-cfg2`),
  and the one home folder `~/GezikReleaseTest` (Spotlight does not index `/tmp`, so the
  Spotlight and portable items need a folder under home). Never select, move, rename, tag or
  delete anything else. Remove `~/GezikReleaseTest` and `~/gezik-tag-probe.txt` at the end.
- **Never the real config.** Do not open, edit or delete `~/Library/Application Support/gezik`.
  Only R52 can write to it (App Translocation falls back to it); that item backs it up first
  and puts it back byte for byte. Before anything else, take a fingerprint:
  ```sh
  AS=~/Library/Application\ Support/gezik
  ( [ -d "$AS" ] && cd "$AS" && find . -type f -exec shasum {} + | sort ) > /tmp/as-before.txt
  ```
  At the end the same command into `/tmp/as-after.txt` and `diff /tmp/as-before.txt /tmp/as-after.txt`
  must print nothing (write it in the results).
- **No system changes.** Do not change System Settings (appearance, keyboard layouts or input
  sources, Mission Control keys, login items, privacy, sound output), `~/.zshrc`, or the
  default file manager. Do not turn on Start at login, VoiceOver or the global shortcut. Where
  an original item needs one of these, this list says what to do instead or leaves it out.
- **Confirm trash stays on.** Do not set `confirm-trash = false` or `[panes] confirm = false`;
  never use Delete Permanently or Empty Trash.
- **Undo what a test changes:**
  - *System Integration (PATH):* the items here only read the question and press Cancel. If
    `Add to PATH` was pressed by mistake: `GEZIK_CONFIG_DIR=/tmp/gezik-cfg "$GZ" --unregister`,
    then `ls -la ~/.local/bin` must not list `gezik` (and `~/.local/bin` is gone if Gezik made
    it). Put back a `~/.local/bin` you moved away.
  - *Finder tags, ratings, comments:* only on files under `/tmp/gezik-rel`, `~/GezikReleaseTest`
    and `~/gezik-tag-probe.txt`; they go with the files. Check with
    `mdfind 'kMDItemUserTags == "*"' | grep -v -e /GezikReleaseTest -e gezik-tag-probe` that no
    other file got a tag from the test (the list should match one taken before the tag items).
  - *Disk images:* `hdiutil detach /Volumes/GZEXFAT` (and the other test volumes) at the end.
  - *Gezik.app copies:* delete them and unregister them from LaunchServices:
    `/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -u <path>/Gezik.app`.
  - *Trash:* files trashed by the tests stay in the Trash; tell the maintainer (do not empty it).
- **Privacy prompts** (Desktop, Documents, Downloads, Photos): answer Allow for the test only if
  the maintainer is there; otherwise write down the prompt and leave the item NOT TESTED.

## 3. Test data

Run once, in Terminal (bash or zsh). Uses `sips`, `hdiutil`, `cupsfilter`, `plutil`, `xattr`
from macOS, and ffmpeg (Homebrew's, or Gezik's own after R30 downloads it:
`find /tmp/gezik-cfg/tools -name ffmpeg -type f`).

```sh
T=/tmp/gezik-rel; D=$REPO/crates/gezik-batch/tests/data; cd $T
mkdir -p nav/a/b nav/c copy tags media/music photos docs code
printf 'a\n' > nav/a/one.txt; printf 'b\n' > nav/c/two.txt; seq 1 200 > copy/lines.txt
for i in 1 2 3 4 5 6; do echo "file $i" > tags/f$i.txt; done
cp $D/fields/*.mp3 $D/fields/tags.m4a $D/fields/tags.flac media/music/
cp $D/fields/photo.jpg photos/iphone.jpg                     # iPhone 15 Pro EXIF, GPS, orientation 6
sips -s format heic photos/iphone.jpg --out photos/iphone.heic   # write "HEIC made with sips" in results
sips -s format jpeg -z 3000 4000 photos/iphone.jpg --out photos/big12mp.jpg
cp $D/fields/report.docx $D/fields/info.pdf $D/pdf/enc_aes256.pdf docs/   # enc password: pw
seq 1 3000 | sed 's/^/Line /' > /tmp/p50.txt && cupsfilter /tmp/p50.txt > docs/p50.pdf 2>/dev/null
ls -l docs/p50.pdf     # empty or missing: print a long page from Safari to PDF instead
cp $REPO/crates/gezik/src/main.rs code/main.rs; cat $REPO/crates/gezik/src/*.rs > code/big.rs
cp "$(find /Applications -name "*.dylib" -size +100k 2>/dev/null | head -1)" code/   # any real Mach-O dylib
mkdir -p ~/GezikReleaseTest/spot && echo s1 > ~/GezikReleaseTest/spot/s1.txt && echo s2 > ~/GezikReleaseTest/spot/s2.txt
hdiutil create -size 200m -fs ExFAT -volname GZEXFAT /tmp/gzexfat.dmg && hdiutil attach /tmp/gzexfat.dmg
```

Media (needs ffmpeg as `$FF`):

```sh
FF=$(command -v ffmpeg || find /tmp/gezik-cfg/tools -name ffmpeg -type f | head -1); cd $T/media
# 30 s clip: a counter on screen and a beep each second, to judge picture/sound sync by eye and ear
$FF -y -f lavfi -i testsrc2=size=1280x720:rate=30:duration=30 -f lavfi -i "sine=frequency=880:beep_factor=4:duration=30" -c:v mpeg4 -q:v 4 -c:a aac -shortest av.mp4
$FF -y -i av.mp4 -c copy av.mkv
$FF -y -i av.mp4 -c:v libvpx -c:a libopus av.webm || echo "no VP8/Opus encoder: leave .webm out"
$FF -y -f lavfi -i color=c=orange:s=300x300 -frames:v 1 cover.png
# a 200 s song with tags and a cover (Turkish values)
$FF -y -f lavfi -i sine=frequency=440:duration=200 -i cover.png -map 0 -map 1 -c:a libmp3lame -c:v png \
  -id3v2_version 3 -metadata artist="Çiğdem Öztürk" -metadata title="Ağıt Şarkısı" -metadata album="Gezik Test" \
  -metadata track=3 -metadata date=2024 -disposition:v attached_pic music/long.mp3 \
  || $FF -y -f lavfi -i sine=frequency=440:duration=200 -i cover.png -map 0 -map 1 -c:a aac -c:v png \
  -metadata artist="Çiğdem Öztürk" -metadata title="Ağıt Şarkısı" -metadata album="Gezik Test" -metadata track=3 \
  -disposition:v attached_pic music/long.m4a
printf 'not a video' > broken.mp4
```

Finder tags from the shell (Finder colour numbers: 1 Gray, 2 Green, 3 Purple, 4 Blue,
5 Yellow, 6 Red, 7 Orange). Use the `tag` tool if it is already installed
(`tag -a Red file`); otherwise this helper:

```sh
settag() { f=$1; shift; x='<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><array>'
  for t in "$@"; do x="$x<string>$t</string>"; done
  printf '%s</array></plist>' "$x" > /tmp/t.plist && plutil -convert binary1 /tmp/t.plist &&
  xattr -wx com.apple.metadata:_kMDItemUserTags "$(xxd -p /tmp/t.plist | tr -d '\n')" "$f"; }
settag ~/GezikReleaseTest/spot/s1.txt $'Red\n6'      # read back: mdls -name kMDItemUserTags <file>
```

## 4. Checklist

### A. Tests and probes (Terminal)

R1. **[Run 2 build] Workspace tests.** `cargo test --workspace 2>&1 | tail -60`. Run 2 ended
with every suite passing after `fix/macos-2`; paste any failure with its output.

R2. **[340–342] Tag storage on a real Mac.** `cargo test -p gezik-core meta`,
`cargo test -p gezik-platform meta`, `cargo test -p gezik-ops meta`, `cargo test -p gezik-ops tags`,
then `cargo test -p gezik-platform meta::tests::finder_probe -- --ignored`. Expected: all pass;
`~/gezik-tag-probe.txt` shows a red dot in Finder and Get Info ▸ Tags lists `Gezik Probe` and
`Red`; `xattr -px com.apple.FinderInfo ~/gezik-tag-probe.txt`: byte at offset 9 is `0C`;
`mdls -name kMDItemStarRating -name kMDItemFinderComment ~/gezik-tag-probe.txt` gives `3` and
`Written by Gezik`; the file's modified date (`ls -lT`) is unchanged by a second probe run.
Then add Green in Finder (**hand**, or `settag ~/gezik-tag-probe.txt Gezik\ Probe $'Red\n6' $'Green\n2'`)
and `cargo test -p gezik-platform meta::tests::finder_wrote_green -- --ignored` passes.

R3. **[343] exFAT keeps tags.** `GEZIK_TEST_TAG_DIR=/Volumes/GZEXFAT cargo test -p gezik-platform meta::tests::a_copy_to_an_exfat_disk_keeps_its_tags -- --ignored`
passes (the disk image from §3 stands in for a stick).

R4. **[13a–13f, 14e] Part tests.** `cargo test -p gezik-core syntax`, `cargo test -p gezik preview::`,
`cargo test -p gezik-core pdf_view`, `cargo test -p gezik-batch fields`, `cargo test -p gezik-core fields`,
`cargo test -p gezik field`, `cargo test -p gezik-core template`, `cargo test -p gezik-search properties`,
`cargo test -p gezik-config paths`, `cargo test -p gezik-platform audio`. After R26 has
downloaded pdfium: `GEZIK_TEST_PDFIUM=$(find /tmp/gezik-cfg/tools -name libpdfium.dylib | head -1) cargo test -p gezik-batch --test pdf_view --test pdf_worker --test pdf_tasks`.
Paste the summaries; every test passes (a filter that matches nothing is fine, say so).

### B. Smoke pass of the core

R5. **[1] Start.** Gezik opens on `/tmp/gezik-rel` with no error line. In
`/tmp/gezik-cfg/settings.toml` set `theme = "dark"`, save: the window turns dark within about
2 s; `theme = "light"`: light; back to `auto`. (This also re-checks Run 2's live-reload fix
through the `/tmp` link.)

R6. **[2] Navigation.** Double-click `nav`, then ⌘↓ on `a`, ⌘[ back, ⌘] forward, ⌘↑ up; ⌘T
opens a tab, ⌘W closes it. The sidebar shows Home and the drives. No beachball.

R7. **[5, 6] Copy, cut, Finder.** In `copy`: ⌘C on `lines.txt`, go to `nav/c`, ⌘V: a copy;
⌘X on `nav/a/one.txt` shows it faded, ⌘V in `nav/c` moves it; ⌘Z puts it back. ⌘⌥V pastes a
copied file as a move. Copy `lines.txt` in Gezik, ⌘V in a Finder window on `/tmp/gezik-rel/nav/b`:
it arrives (Finder interop both ways: copy in Finder, paste in Gezik).

R8. **[7, 11] Trash and conflicts.** ⌘⌫ on `nav/c/two.txt` asks (confirm on), moves it to the
Trash; ⌘Z puts it back to `nav/c` (Run 2 bug A: this was refused under `/tmp`). Paste
`lines.txt` twice into `nav/b`: the conflict list; Keep both gives `lines (2).txt`; ⌘Z removes it.

R9. **[9, 98] Quick Look and views.** Select `photos/iphone.jpg`, Space: the system Quick Look
panel, upright (Run 2's 88 fix); Esc closes it and **no** Gezik preview window opens after it
(98 fix); three files selected and Space: the panel pages through three. ⌘2 grid, ⌘1 list, ⌘A.

R10. **[72] Search.** ⌘⇧F in `/tmp/gezik-rel`, `*.txt`, Return: results stream in with a Folder
column; Content search for `file 3` finds `tags/f3.txt`. Esc ends it.

R11. **[231, 235] Two panes.** ⌃⌘P: a second pane at the same folder, active. Go to `nav/b`
there, Tab back to the left, select `copy/lines.txt`, fn+F5: `Copy 1 item to …nav/b?`, Return
copies, the right pane shows it; ⌘Z takes it back. ⌃⌘P closes the pane.

R12. **[261, 263, Run 2 finding 1] Windows and quitting.** ⌘N: a second window in the same
process (`pgrep -l gezik`: one). Close it with the red button, then the last window with the red
button: Gezik quits **without** "gezik quit unexpectedly" and with no new
`~/Library/Logs/DiagnosticReports/gezik*.ips` (`ls -lt ~/Library/Logs/DiagnosticReports | head`).
Start again: the tabs come back. ⌘Q also quits cleanly.

### C. Finder tags, ratings, Spotlight (12b–12f)

R13. **[344] The picker.** Select `tags/f1.txt`–`f3.txt`, ⌃⌘L (and File ▸ Tags ▸ Tags…):
`Tags for 3 items` with seven colours and ⌃1…⌃7 at the right. Return on Red: `✓`. Give `f1`
Orange from the shell (`settag tags/f1.txt $'Orange\n7'`) before opening: Orange shows `–`.
Type `Proj`, Return on `Create "Proj"`: `[tags] list` in settings.toml gains it with its
comments kept. Esc: one job; ⌘Z takes all of it back.

R14. **[345, 346] Keys and menus.** ⌃1 on the selection: `Red added to 3 items`; ⌃1 again
takes it off; ⌃0 clears all tags (a rating stays: `mdls -name kMDItemStarRating`). If ⌃1 is
taken by Mission Control's "Switch to Desktop 1", do **not** change that setting: write it down
and use File ▸ Tags ▸ Red instead. File ▸ Tags lists the names with ⌃1…⌃7, `Tags…`, `Clear Tags`;
a row's right-click has them under a greyed `Tags` (`• Red` on a row that has it); a drive row
and the Trash have none.

R15. **[347] Finder both ways.** Tags given in Gezik show in Finder (list and Get Info) in the same
colours; `settag tags/f4.txt $'Blue\n4'` (or a tag given in Finder) shows as a dot in Gezik after
⌘R. `ls -lT` before and after: the modified dates do not change.

R16. **[348, 349] Dots and the Tags column.** List, grid (⌘2) and columns (⌃⌘3): up to three dots
after the name, `+2` past three (give `f5` five tags with `settag`), a ring for a tag without a
colour, a white edge on a selected row; dots crisp on Retina. Header right-click ▸ Show Tags: a
Tags column after Size (`Red, Proj`); a header click sorts by tags. View ▸ Tag Dots off: no dots,
`dots = false` in settings.toml.

R17. **[350, 351] Sort, group, filter.** View ▸ Group By ▸ Tag: `● Red (n)` …, `No Tags (n)` last.
⌘F filter: `tag:Red`, `tag:red` (case ignored), `tag:none`, `tag:any`, `*.txt tag:Red` each show
the right rows.

R18. **[353–356] Ratings and comments.** ⌃⌘L on `f1`–`f3`, click the third star, Esc: ★★★, one
job (⌘Z undoes); `mdls -name kMDItemStarRating tags/f1.txt` gives 3. File ▸ Tags ▸ Comment…
`Ready for review`, Save: `mdls -name kMDItemFinderComment` shows it (Finder's Get Info may not:
known). Two files with different comments: the field starts empty with `Comments differ across 2 items`.
Header ▸ Show Rating: `★★★☆☆`, sorts most stars first. Filter `rating:>=3`, `rating:none`.

R19. **[357, 358] TAGS sidebar and Spotlight.** First `mdfind 'kMDItemUserTags == "Red"cd' | wc -l`
and `mdfind 'kMDItemUserTags == "Red"cd' | grep GezikReleaseTest` (the `s1.txt` of §3 must be
there; wait a minute for Spotlight if not). The sidebar has a TAGS section between SEARCHES and
LOCATIONS with coloured dots. Click Red: a search tab `Tag: Red` lists as many items as `mdfind`
within about 1 s, `~/GezikReleaseTest/spot/s1.txt` among them, the status bar ends
`via Spotlight (drives it does not index are not in it)`. Do not act on any result outside the
test folders. The tag's menu ▸ Search this folder (in `/tmp/gezik-rel/tags`) finds the `/tmp`
files without Spotlight. `[sidebar] tags = false` hides the section; true brings it back.

R20. **[359] Drop to tag.** Drag `tags/f5.txt` and `f6.txt` onto Purple in the sidebar: the label
says `Tag 2 items Purple`, nothing moves, `mdls -name kMDItemUserTags` shows Purple; ⌘Z takes it off.
Drag `~/GezikReleaseTest/spot/s2.txt` from Finder onto Green: it gets Green.

R21. **[361] Search words.** Scope This Mac: `tag:Red rating:3` and `comment:review` use Spotlight
(give `~/GezikReleaseTest/spot/s1.txt` 3 stars and the comment in Gezik first, then wait for
`mdfind 'kMDItemStarRating >= 3' | grep GezikReleaseTest`); on `/tmp/gezik-rel/tags` they walk
the folder. Save the search: `[[searches]]` gets `tags`, `min-rating`, `comment`.

R22. **[362, 363] Highlight rules.** Uncomment the four `[[highlight-rules]]` examples in
`/tmp/gezik-cfg/settings.toml`: `.tmp`/`.bak` grey (`touch tags/x.tmp`), folders changed today
green, Red files bold, at once in list, grid, columns and search results. `min-size = "lots"` in
one rule: a notice naming `highlight-rules[n]`, the others still work. `theme = "dark"` (not the
system setting): the colours lighten, the folder rule uses its `dark-color`; bold names crisp.

### D. Preview, playback, properties (13a–13f)

Set `[system] quick-look = "gezik"` in settings.toml for this part, and open the preview pane
(⌥P).

R23. **[13a 366] Code.** `code/big.rs` in the pane and with Space: coloured, line numbers, a
column of `|` stays straight on Retina; Page Down, End (last line whole), Home; `W` wraps and
unwraps, the choice survives a restart (`state.toml [preview] wrap-code`).

R24. **[13a 367] A 3 GB log.** `yes "$(date) a line of a log file" | head -c 3000000000 > /tmp/gezik-rel/big.log`;
select it: the first lines at once; End: the last line within a moment, `2.8 GB · 100%`;
drag the scroll bar to the middle: `· 50%`. Memory (Activity Monitor or
`ps -o rss= -p $(pgrep gezik)`) within +5 MB of before. Delete `big.log` afterwards (`rm`).

R25. **[13a 368] Hex.** The `.dylib` in `code/`: the icon and a `Hex` button; Space, `H`: rows
`00000000  CF FA ED FE …`; End goes to the last row; `H` back. View ▸ Show Preview as Hex does
the same.

R26. **[13b 369] PDF pages (pdfium worker).** Select `docs/p50.pdf`: page 1, the bar `‹ 1 ›`,
`−`, `Fit`, `+`. If pdfium is missing the bar offers `Download`: click it, it lands in
`/tmp/gezik-cfg/tools/pdfium-*/libpdfium.dylib`, then pages work. `›`: page 2 within a moment
(`2 / 50`); Page Down, Page Up, End (`50 / 50`), Home; click the page number, `25`, Return. The
same in Space's window. Pages sharp on Retina at Fit. A narrow pane (≈220 px) keeps ‹ › − + Fit.
If the dylib is refused (`codesign`/library validation), paste the error and
`codesign -dv <dylib>`.

R27. **[13b 370] Zoom.** `+` to `400%` (or ⌘+wheel over a word: it stays under the pointer);
drag the page: it follows, edges fill in after you stop. `Fit` ↔ `Fit Width`; `0` fits, `1` is
`100%`; the zoom stays across pages. `photos/big12mp.jpg`: double-click shows `100%`, drag works,
`0` goes back and memory comes back down.

R28. **[13b 371] Password.** `docs/enc_aes256.pdf`: `Password protected · Enter Password…`; a
wrong one: `Wrong password`; `pw`: page 1, then `›` page 2.

R29. **[13b 372] The worker goes.** After R26 `ps ax | grep -- --pdf-view | grep -v grep`: one
`gezik --pdf-view`. Select a text file: it is gone within about 10 s (not before). PDFs one after
another within 10 s: still one process. Selecting a PDF without turning pages starts none.

R30. **[13c 373] Play (AudioQueue).** Start Gezik with the Homebrew-free PATH of §1 if ffmpeg
should be downloaded. Select `media/av.mp4`: poster, `▶`, the bar `Hex`, `▶`, `--:--`, `Mute`;
`ps ax | grep ffmpeg | grep -v grep`: none. Click `▶` (without ffmpeg: `Playing needs ffmpeg ·
Download`; download, it then plays by itself): first frame within half a second, the counter
moves, the time and seek bar move. **(hand)** the beep falls on each new second of the counter
(sync), and it plays through the Mac's current output. Click (or `K`): pause, picture and sound
stop; again: it goes on. The same in Space's window. An automated runner checks that sound is
produced with `log stream --predicate 'subsystem == "com.apple.coreaudio"' --style compact`
for a few seconds while playing, or writes the sound part NOT TESTED.

R31. **[13c 374] Seek.** Drag the seek bar: only the time changes while dragging, on release it
plays from there (counter matches). `L` +10 s, `J` −10 s; right-click has Back/Forward 10 Seconds.
Pause, wait over a minute: the two ffmpeg processes are gone; `K`: it plays on from the same place.

R32. **[13c 375] Volume.** Note `osascript -e 'output volume of (get volume settings)'` (read
only). The bar right of `Mute` sets Gezik's volume while dragging; `M` mutes and back. The
osascript value is unchanged afterwards. Quit and start: volume and mute as left
(`state.toml [preview] volume`, `muted`). A pane under 300 px drops Hex and the volume bar.

R33. **[13c 376, 377] Other files.** `av.mkv` and `av.webm` (if made) play with sound; `broken.mp4`:
`Cannot play this file`, the poster stays. `music/long.mp3` (or `.m4a`): the cover as poster, `▶`
plays, it stops at the full length (or seek near the end) and `▶` starts again from the start.

R34. **[13c 378] Processes go.** While `av.mp4` plays, select another file: within a second no
ffmpeg process, the sound stops, memory back within a few MB. Close the pane (⌥P) while playing:
the same. Quit Gezik while playing: no crash report.

R35. **[13d 379] Photo fields, HEIF.** `photos/iphone.jpg` and `photos/iphone.heic` (and a real
iPhone HEIC if one is at hand, **hand**: AirDrop): a `Photo` group with Date Taken, Camera
(`Apple iPhone 15 Pro`), Exposure `1/250 s`, Aperture, ISO 100, Focal Length, Location
(`… N, … E`), `Picture` Dimensions upright (portrait: smaller number first). Write which rows the
sips HEIC lost, if any (then compare with `sips -g all photos/iphone.heic`). Right-click a row ▸
`Copy Value`. The same rows in Space's window and Get Info (⌘I).

R36. **[13d 380, 381] Music and documents.** `media/music/id3v24.mp3`, `tags.m4a`, `tags.flac`:
`Media` Duration, codec, Bit Rate, Sample Rate; `Music` Title, Artist, Album, Track, Year
(Turkish letters right); the bar shows the length before `▶`. `docs/report.docx`: `Document`
Title, Author (Pages and Words only if the file has them); `docs/info.pdf`: its Title and Pages.
`[preview] properties = false`: no Properties rows in the pane, Get Info still has them.

R37. **[13e 382] Property columns.** In `media/music`: header right-click ▸ `More Columns…`, `art`,
Enter on Artist; then Album and Duration. They fill for the rows on screen; Duration's header
sorts shortest first (files without one last), again longest first. Widths survive a restart.
View ▸ Group By ▸ Property… ▸ Album: one group per album, `No Album` last.

R38. **[13e 383] Property filter.** In `photos` ⌘F `taken>=2024 camera:iphone` (the test photo's
date decides: write it); rows faint while `Reading properties… n of m`, then the matching ones.
`duration>3x`: `"3x" is not a duration (3m, 1:30)`, the list stays.

R39. **[13f 385] Search by properties.** ⌘⇧F in `media/music`, `*.mp3`, Filters ▸ `Properties…`:
typing `art` offers `artist, album-artist`; `artist:Çiğdem duration>3m`, OK: only `long.mp3`;
Filters shows `Properties… (artist:Çiğdem duration>3m)`. `duration>3x`: error, OK does nothing.
Save search…: `where = "artist:Çiğdem duration>3m"` in settings.toml; the sidebar entry runs it
again.

R40. **[13f 386] Name templates.** Copy the music files to `media/rename/`, select them ▸ Batch
Rename ▸ + Add rule ▸ Template; `Fields ▾` ▸ Music ▸ Track inserts `{track}`; make it
`{track:02} {artist} - {title}`: the preview reads `03 Çiğdem Öztürk - Ağıt Şarkısı.mp3`-like after
`reading properties…`; a file without an artist gets ⚠ and `1 name with an empty field`;
`{artist|Unknown}` fixes it. Rename; ⌘Z undoes. (Optional: an artist `AC/DC`, made with
`$FF -i long.mp3 -c copy -metadata artist=AC/DC acdc.mp3`, becomes `AC_DC`.)

### E. Settings window and shortcuts (14a, 14b)

R41. **[14a 365] ⌘, opens it.** ⌘, and View ▸ Settings… open `Settings — Gezik` with eight pages;
a second ⌘, (main window in front) brings the same window forward, not a second one. Close and
reopen: fresh. Memory back near where it was after closing.

R42. **[14a 366] One line per change.** `cp /tmp/gezik-cfg/settings.toml /tmp/before.toml`;
Appearance ▸ Density ▸ compact: rows tighten at once; `diff /tmp/before.toml /tmp/gezik-cfg/settings.toml`
shows only the `density` line. General ▸ Max fps `5`, Return: the bottom line red with the
reason, the file unchanged. Pointing at themes previews them; moving away goes back.

R43. **[14a 367] Broken file.** Append `[layout` to settings.toml with the window open: the
bottom line says `settings.toml line N: …` and the controls do nothing; remove it: they work.

R44. **[14b 369] Shortcuts page.** Settings ▸ Keyboard: keys show as ⌘⌥C, ⌃⌘C …; Quit Gezik ⌘Q,
Hide Gezik ⌘H, Hide Others ⌥⌘H and Minimize ⌘M have no Set Key… button. Copy Path ▸ Set Key…,
press ⌘⌥K: the row says ⌥⌘K (write what Turkish-QWERTY-PC gives if not), settings.toml has one
new line `copy-path = "mod+alt+k"` under `[shortcuts]`, the menu bar's Copy Path shows ⌥⌘K and the
key works. Set Key… on New Tab, ⌘W: "⌘W is used by Close Tab — Replace?"; Esc cancels; Replace:
New Tab is ⌘W, Close Tab none. Reset to Default on both brings ⌘T and ⌘W back (Reset asks when
another action holds the default). Write whether ⇧⌘3 reaches the capture field (it should not
take a screenshot shortcut away).

### F. Themes, export and import (14c, 14d)

R45. **[14c 370] Get a theme.** Settings ▸ Appearance ▸ Get Theme (and the palette's Get Theme…,
which puts the keyboard in its field). Paste
`https://github.com/wenlar/gezik/blob/master/crates/gezik-config/tests/themes/nord.toml`, Return:
the bottom line counts the download, a `Nord` row with `Not installed`. Point at `Install`: every
window previews it; away: back. `Install`: `/tmp/gezik-cfg/themes/nord.toml` with first line
`# Installed from …`, an `Installed` row at once with `Use Now`, `Check for Updates`,
`Open Source Page`, `Delete Theme`. `Use Now` switches; `Check for Updates` says unchanged;
`Open Source Page` opens the page in the default browser; install it again: `Replace`/`Keep Both`
(`nord-2.toml`, row reads `Nord (nord-2.toml)`).

R46. **[14c 370] Refusals and delete.** `http://github.com/a/b`: refused, red line. The whole
repository `https://github.com/wenlar/gezik/tree/master/crates/gezik-config/tests/themes`: refused
as more than 5 MB with the hint to paste a `.toml` address (the repo zip is ≈ 6.5 MB). If a small
public theme repository is known, paste it: each `.toml` at its top or in `themes/` is a row;
otherwise write "no multi-theme repository". `Delete Theme` on `nord-2`: confirm, the file is in
the Trash; ⌘Z brings it back.

R47. **[14d 372] Export and import.** Make a second config with something to carry:
```sh
mkdir -p /tmp/gezik-cfg2/themes /tmp/gezik-cfg2/templates
cp $REPO/crates/gezik-config/tests/themes/nord.toml /tmp/gezik-cfg2/themes/
echo 'hello' > /tmp/gezik-cfg2/templates/Note.txt
```
Quit the main Gezik, start `GEZIK_CONFIG_DIR=/tmp/gezik-cfg2 "$GZ" /tmp`, pin a folder under
home, choose theme Nord, then Settings ▸ Sync and Backup ▸ Export Settings: type
`/tmp/gezik-export.zip` (**not** the default under Documents), Return: `Exported …`. Quit it. A
zip exported on Windows by the maintainer is better still (**hand**); write which one was used.
In the main Gezik (`/tmp/gezik-cfg`): Import Settings, `/tmp/gezik-export.zip`, Return: rows say
`replaces`/`new` and how many settings change; Import: the theme, the pin and the template
(New ▸) work; `/tmp/gezik-cfg/backups/gezik-settings-<time>.zip` exists. Undo Import: settings,
theme and template as before. `ls ~/Documents/gezik-settings-*` lists nothing new.

R48. **[14d 373] The red row.** Add to `/tmp/gezik-cfg2/settings.toml`:
`[[commands]]` `name = "Say hi"` `run = ["echo", "hi"]`, export again to `/tmp/gezik-export2.zip`,
preview it in the main Gezik: a red "These run programs on this computer" row names the command,
the bottom line is red; Cancel: nothing changes (`diff` against a copy taken before). Restore
Backup lists R47's backup; choosing it shows what it would put back (Cancel).

### G. Portable mode and App Translocation (14e)

These start Gezik **without** `GEZIK_CONFIG_DIR`. The fingerprint of §2 must be unchanged after
R49–R52.

R49. **[14e 374] Next to Gezik.app.** Quit every Gezik. Make a bundle by hand (there is no
packaging script):
```sh
P=~/GezikReleaseTest/Portable; mkdir -p $P/Gezik.app/Contents/MacOS
cp "$GZ" $P/Gezik.app/Contents/MacOS/gezik && cp $REPO/crates/gezik/macos/Info.plist $P/Gezik.app/Contents/
codesign --force -s - $P/Gezik.app && touch $P/gezik-portable && ls -lT ~/Library/Application\ Support/gezik
```
Open `$P/Gezik.app` from Finder (or `open $P/Gezik.app`): `$P/gezik-data/` appears with
`settings.toml`, `themes/example.toml`, `templates/`; Settings ▸ Sync and Backup's first row says
`Portable: settings are kept in …/gezik-data` and `Open Data Folder` opens it in a tab. Change the
theme, pin a folder, quit: only `gezik-data` changed; `ls -lT` of Application Support and the
fingerprint are unchanged. The app menu reads "Gezik" (not "gezik"). `codesign --verify $P/Gezik.app`
passes.

R50. **[14e 374] Two at once.** With the portable Gezik open, start
`GEZIK_CONFIG_DIR=/tmp/gezik-cfg "$GZ" /tmp/gezik-rel`: two processes, two windows, each with its
own settings; `"$GZ" --help` is unaffected. In the portable one, System Integration…: the
questions (Add to PATH) begin with `Gezik is running in portable mode from …`; press **Cancel**.
A Compress… 7-Zip download, if offered, lands in `gezik-data/tools/macos-aarch64/` (write the
folder name; do not download if the maintainer is away and it is large).

R51. **[14e, Linux 272] Read-only data folder.** Quit the portable Gezik.
`hdiutil create -srcfolder ~/GezikReleaseTest/Portable -format UDRO -volname GZPORT /tmp/gzport.dmg && hdiutil attach /tmp/gzport.dmg`,
then `open /Volumes/GZPORT/Gezik.app`: it opens with the portable settings, the status bar says
`Portable data folder is read-only: changes are not saved`; moving a column or pinning a folder
does not crash and writes nothing. Detach afterwards.

R52. **[14e 375] App Translocation.** **(hand** for Gatekeeper; otherwise NOT TESTED.) Back up the
real config first: `ditto ~/Library/Application\ Support/gezik /tmp/as-backup` (if it exists).
Then:
```sh
cd ~/GezikReleaseTest && ditto -c -k --keepParent Portable /tmp/GezikPortable.zip
mkdir -p ~/Downloads/GezikTranslocate && cd ~/Downloads/GezikTranslocate
xattr -w com.apple.quarantine "0081;$(printf %x $(date +%s));Safari;" /tmp/GezikPortable.zip
cp /tmp/GezikPortable.zip . && open GezikPortable.zip   # Archive Utility passes the flag on
xattr -l Portable/Gezik.app | grep quarantine
```
Double-click `Portable/Gezik.app` in Finder. Gatekeeper will block an ad-hoc signed app: the
maintainer chooses Open Anyway in Privacy & Security (no setting is changed beyond this app).
Expected: the status bar says `Move Gezik.app out of Downloads (or open it once from Finder
after moving) to use portable mode`, and the usual settings are used (not `gezik-data`). Quit.
Move the `Portable` folder elsewhere under `~/GezikReleaseTest` in Finder and open it again:
portable mode as in R49, no note. Then put the real config back:
`rm -rf ~/Library/Application\ Support/gezik && ditto /tmp/as-backup ~/Library/Application\ Support/gezik`
(or just `rm -rf` it if there was none) and check the fingerprint. Delete `~/Downloads/GezikTranslocate`
and unregister both bundles with `lsregister -u` (§2).

### H. Clean up

R53. **Clean-up record.** Detach `GZEXFAT` and `GZPORT`; `rm -rf ~/GezikReleaseTest ~/gezik-tag-probe.txt /tmp/gezik-rel /tmp/gezik-cfg2 /tmp/*.zip`;
unregister the bundles; `ls -la ~/.local/bin 2>/dev/null` has no `gezik` link made by the run;
the Application Support fingerprint `diff` is empty; `mdfind 'kMDItemUserTags == "*"'` shows no
new tagged file outside the deleted folders. Write each check's output in the results.

## 5. Not in this pass

Left out on purpose (ran in Run 2, need hardware or accounts, or change the system): 9b4 default
file manager (macOS refuses `public.folder`), 9b6 SMB and eject, 9b7 administrator helper,
9b8 promised files from Mail/Photos, 9b9 tray, global shortcut and start at login, iCloud items,
11b–11i tools (no macOS-specific code beyond what Run 2's tests covered), 10c–10g beyond the
smoke items, 14a VoiceOver (needs VoiceOver turned on), 14c proxy. Run them from
`../macos-test.md` when there is time.

## 6. Report back

At the top of Run 3, a table, then one block per item:

```markdown
| Item | Result | Note |
|---|---|---|
| R1 [build] | PASS | 1,234 tests |
| R30 [13c 373] | FAIL | no sound; see below |
| R52 [14e 375] | NOT TESTED | no one to answer Gatekeeper |

### R30 [13c 373] Play: FAIL
- Steps: …
- Expected: …
- Seen: …
- Screenshot: ~/Desktop/gezik-run3/r30.png (not committed)
- Log: what the Terminal that started Gezik printed
```

- Every item gets PASS, FAIL or NOT TESTED (with the reason). Partial passes are FAIL with the
  failing sub-step named.
- Screenshots go to `~/Desktop/gezik-run3/` (name them by item); do not commit them.
- Crashes: `ls -lt ~/Library/Logs/DiagnosticReports | grep -i gezik`; paste the first 60 lines
  of each new `.ips` (the `crashed thread` part) and the panic line from the Terminal. With a
  panic, rerun once with `RUST_BACKTRACE=1` and paste the backtrace.
- Memory numbers: `ps -o rss= -p $(pgrep -n gezik)` in KB, before / during / after.
- End with the §2 clean-up checks and their output.
