# Steps 12-14 for scripts/linux/gui.sh (sourced): tags, preview and playback, the settings
# window, themes, settings export/import and portable mode, each run on X11 (Xvfb, xdotool)
# and on Wayland (headless sway, wtype, the vpointer example) with the same coordinates:
# the X11 window is set to 1280x695 at 0,0, about what sway gives the one tiled window, and
# every click, crop and pixel adds the window's content origin on Wayland.
#   gui.sh steps-x11 | steps-wayland | steps   (needs the network: pdfium, a theme)
# Each check prints "ok" or "FAIL" with the session and step (x11: 12b …); screenshots go to
# .superpowers/linux-shots/<session>-*.png.

STEPS_TOOLS="attr ffmpeg pulseaudio pulseaudio-utils libimage-exiftool-perl tesseract-ocr tesseract-ocr-eng grim curl"

steps_setup() {
    local missing="" p
    for p in getfattr ffmpeg pulseaudio pactl exiftool tesseract grim curl; do command -v $p >/dev/null || missing=1; done
    if [ -n "$missing" ]; then
        apt-get update -qq >/dev/null && DEBIAN_FRONTEND=noninteractive \
            apt-get install -y -qq --no-install-recommends $STEPS_TOOLS >/dev/null 2>&1
    fi
    # A PulseAudio with only a null sink: playback opens a stream there, nothing is heard.
    if ! PULSE_SERVER=unix:/tmp/pulse.sock pactl info >/dev/null 2>&1; then
        XDG_RUNTIME_DIR=/tmp/pulse-run pulseaudio -D --exit-idle-time=-1 -n \
            -L "module-native-protocol-unix socket=/tmp/pulse.sock auth-anonymous=1" -L module-null-sink 2>/dev/null
        mkdir -p /tmp/pulse-run; sleep 1
    fi
    # pdfium for the PDF pages, as Gezik's own download puts it.
    if [ ! -f /tmp/pdfium/libpdfium.so ]; then
        curl -sSL -o /tmp/pdfium.7z \
            https://github.com/wenlar/gezik-tools/releases/download/pdfium-8086-1/pdfium-8086-linux-x64.7z &&
            $SEVEN x -y -o/tmp/pdfium /tmp/pdfium.7z >/dev/null
    fi
    steps_media
}

# Test files of step 13, made here: code, a binary, two tagged MP3s, an MKV, a JPEG with
# EXIF and a three-page PDF ("Page 1" … "Page 3").
steps_media() {
    local m=/tmp/steps-media
    [ -f $m/doc.pdf ] && return
    rm -rf $m && mkdir -p $m
    cp /src/crates/gezik-core/src/sort.rs $m/code.rs
    head -c 4096 /usr/lib/x86_64-linux-gnu/libc.so.6 > $m/lib.bin
    ffmpeg -loglevel error -f lavfi -i sine=f=440:d=5 -metadata artist="Şebnem Ferah" -metadata title="Ağıt" \
        -metadata album="Kalp" -metadata track=3 -id3v2_version 3 $m/song1.mp3
    ffmpeg -loglevel error -f lavfi -i sine=f=660:d=8 -metadata artist="Other Band" -metadata title="Rain" \
        -metadata album="Kalp" -metadata track=12 -id3v2_version 3 $m/song2.mp3
    ffmpeg -loglevel error -f lavfi -i testsrc=size=320x240:rate=25:d=6 -f lavfi -i sine=f=330:d=6 \
        -c:v libx264 -c:a aac -shortest $m/clip.mkv
    convert -size 640x480 gradient:blue-yellow $m/photo.jpg
    exiftool -q -overwrite_original -Make=Canon -Model="Canon EOS 5D" -DateTimeOriginal="2024:05:01 10:00:00" \
        -ExposureTime=1/125 -FNumber=5.6 -ISO=200 $m/photo.jpg
    python3 - <<'PY'
objs = [b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [4 0 R 6 0 R 8 0 R] /Count 3 >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"]
for i in range(3):
    text = b"BT /F1 48 Tf 72 700 Td (Page %d) Tj ET" % (i + 1)
    objs.append(b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents %d 0 R >>" % (5 + 2 * i))
    objs.append(b"<< /Length %d >>\nstream\n%s\nendstream" % (len(text), text))
out, offs = bytearray(b"%PDF-1.7\n"), []
for i, o in enumerate(objs, 1):
    offs.append(len(out)); out += b"%d 0 obj\n" % i + o + b"\nendobj\n"
xref, n = len(out), len(objs) + 1
out += b"xref\n0 %d\n0000000000 65535 f \n" % n + b"".join(b"%010d 00000 n \n" % o for o in offs)
out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (n, xref)
open("/tmp/steps-media/doc.pdf", "wb").write(out)
PY
}

# ---- The input driver: K (keys and text), C (click), S (screenshot), title, names. ----

# K TOKEN...: a chord in xdotool's names (ctrl+shift+l, Return, alt+4) or =text to type. On
# Wayland the whole call is one wtype run (a new virtual keyboard each run, which Gezik sees
# as the window losing the keyboard: a dialog's text goes in the same K as what opened it).
# wtype gives each new keysym the next key code (the first is Escape's, the second the 1
# key's …), and Gezik matches Alt+Shift+digit and the picker's Alt+digit by the physical key:
# the digit has to be the keysym of its own place (Alt_L 1st, the 1 key 2nd: K alt+1 alt+shift+1;
# Control_L, Shift_L, l, Alt_L, then the 4 key 5th: K ctrl+shift+l alt+4).
x11_K() {
    xdotool windowfocus --sync "$FOCUS" 2>/dev/null
    local t
    for t in "$@"; do
        case $t in =*) xdotool type --delay 50 "${t#=}" ;; *) xdotool key "$t" ;; esac
        sleep 0.25
    done
    sleep 0.3
}
wl_K() {
    local args=(-s 600) t part mods k
    for t in "$@"; do
        case $t in
            =*) args+=("${t#=}" -s 200); continue ;;
        esac
        mods=()
        IFS=+ read -ra part <<<"$t"
        k=${part[-1]}
        for m in "${part[@]:0:${#part[@]}-1}"; do
            case $m in ctrl) mods+=(Control_L) ;; shift) mods+=(Shift_L) ;; alt) mods+=(Alt_L) ;; super) mods+=(Super_L) ;; esac
        done
        for m in "${mods[@]}"; do args+=(-P "$m"); done
        args+=(-k "$k")
        for m in "${mods[@]}"; do args+=(-p "$m"); done
        args+=(-s 250)
    done
    wtype "${args[@]}" -s 300
    sleep ${WL_PAUSE:-0.5}
}
x11_C() { xdotool mousemove "$1" "$2" sleep 0.15 click "${3:-1}"; sleep 0.5; }
wl_C() {
    local r=; [ "${3:-1}" = 3 ] && r=r
    /target/debug/examples/vpointer 1280 720 move $(($1 + OX)) $(($2 + OY)) sleep 150 ${r}down sleep 100 ${r}up sleep 300
    sleep ${WL_PAUSE:-0.5}
}
x11_S() { import -window root "$SHOTS/$BK-$1.png" 2>/dev/null; }
wl_S() { grim "$SHOTS/$BK-$1.png" 2>/dev/null; }
# A sway window's content origin and name, by a Python regex on its title.
wl_geom() {
    swaymsg -t get_tree | python3 -c '
import json, re, sys
pat = re.compile(sys.argv[1])
def walk(n):
    if n.get("pid") and pat.search(n.get("name") or ""):
        r, w = n["rect"], n["window_rect"]
        print(r["x"] + w["x"], r["y"] + w["y"], n["id"]); sys.exit()
    for c in n.get("nodes", []) + n.get("floating_nodes", []): walk(c)
walk(json.load(sys.stdin))' "$1"
}
x11_title() { xdotool getwindowname "$MAIN"; }
wl_title() {
    swaymsg -t get_tree | python3 -c '
import json, sys
def walk(n):
    if n.get("pid") and (n.get("name") or "").endswith("Gezik") and not n["name"].startswith("Settings"):
        print(n["name"]); sys.exit()
    for c in n.get("nodes", []) + n.get("floating_nodes", []): walk(c)
walk(json.load(sys.stdin))'
}
x11_main() { MAIN=$(xdotool search --name "Gezik" 2>/dev/null | head -1); FOCUS=$MAIN; OX=0; OY=0; }
wl_main() {
    local g; g=$(wl_geom '^(?!Settings).*Gezik$')
    set -- $g; OX=${1:-2}; OY=${2:-25}; [ -n "${3:-}" ] && swaymsg -q "[con_id=$3] focus"
}
x11_settings() { FOCUS=$(xdotool search --name "^Settings" 2>/dev/null | head -1); OX=0; OY=0; }
wl_settings() {
    local g; g=$(wl_geom '^Settings')
    set -- $g; OX=${1:-0}; OY=${2:-0}; [ -n "${3:-}" ] && swaymsg -q "[con_id=$3] focus"
}
x11_clip() { xclip -selection clipboard -t x-special/gnome-copied-files -o 2>/dev/null; }
wl_clip() { wl-paste -n -t x-special/gnome-copied-files 2>/dev/null; }
# The names Ctrl+A, Ctrl+C put on the clipboard, sorted, on one line.
names() {
    K ctrl+a ctrl+c; sleep 0.4
    clip | tail -n +2 | python3 -c '
import sys, os, urllib.parse
print(" ".join(sorted(os.path.basename(urllib.parse.unquote(l.strip()[7:])) for l in sys.stdin if l.strip())))'
}
# OCR of a W x H box at X,Y of the window in screenshot NAME (Tesseract, 4x, gray).
ocr() {
    convert "$SHOTS/$BK-$1.png" -crop "$2x$3+$(($4 + OX))+$(($5 + OY))" +repage -scale 400% -colorspace gray /tmp/ocr.png
    tesseract /tmp/ocr.png - --psm 6 2>/dev/null | tr '\n' ' '
}
# How many pixels of a W x H box at X,Y of screenshot NAME are green (the highlight rule's).
greens() {
    convert "$SHOTS/$BK-$1.png" -crop "$2x$3+$(($4 + OX))+$(($5 + OY))" +repage txt:- |
        awk -F'[(,)]' 'NR > 1 && $4 > 110 && $3 < 90 && $5 < 90 { n++ } END { print n + 0 }'
}
# Whether Gezik's UI thread is still busy: more than 8 of the next 10 seconds of CPU.
# ensure TEXT W H X Y CMD…: CMD (a click), up to three times until the box shows TEXT. The
# settings window of a debug build is slow to lay out and now and then misses a click.
ensure() {
    local text=$1 w=$2 h=$3 x=$4 y=$5 i; shift 5
    for i in 1 2 3; do
        "$@"; sleep 2; S ensure
        ocr ensure $w $h $x $y | grep -q "$text" && return 0
    done
    return 1
}
busy() {
    local pid=$1 a b
    a=$(awk '{print $14}' /proc/$pid/stat); sleep 10; b=$(awk '{print $14}' /proc/$pid/stat)
    [ $(( (b - a) / $(getconf CLK_TCK) )) -ge 8 ]
}

drv() {
    BK=$1
    local f
    for f in K C S title main settings clip; do eval "$f() { ${BK}_$f \"\$@\"; }"; done
    [ "$BK" = wl ] && BK=wayland
}

# start CFG DIR [env…]: a fresh Gezik, its window 1280x695 at 0,0 on X11.
start() {
    local cfg=$1 dir=$2; shift 2
    pkill -x gezik 2>/dev/null; sleep 0.5
    env GEZIK_CONFIG_DIR=$cfg PULSE_SERVER=unix:/tmp/pulse.sock "$@" $GEZIK $dir >>/tmp/gezik-steps-$BK.log 2>&1 &
    GPID=$!
    sleep 4
    if [ "$BK" = x11 ]; then
        xdotool windowmove "$(xdotool search --name Gezik | head -1)" 0 0
        xdotool windowsize "$(xdotool search --name Gezik | head -1)" 1280 695; sleep 1
    fi
    main
}
alive() { kill -0 $GPID 2>/dev/null; }

# ---- 12a-12f: tags, ratings, comments, filters, the TAGS sidebar, highlight rules ----
part_tags() {
    local d=/tmp/s12 cfg=/tmp/s12cfg
    rm -rf $d $cfg && mkdir -p $d/Sub $cfg
    for f in a b c d; do echo $f > $d/$f.txt; done
    setfattr -n user.xdg.tags -v "Blue,Proj" $d/d.txt
    setfattr -n user.baloo.rating -v 6 $d/d.txt
    printf '[session]\nrestore = false\n' > $cfg/settings.toml
    start $cfg $d
    S tags-start
    # Rows: Sub 118, a.txt 144, b.txt 170, c.txt 196, d.txt 222.
    C 260 144; K shift+Down
    K ctrl+shift+l alt+4 Return; sleep 0.5; S tags-picker; K Escape; sleep 1.5
    check "$BK: 12b picker (Ctrl+Shift+L, Return on Red, Alt+4) writes user.xdg.tags and user.baloo.rating" \
        '[ "$(getfattr --only-values -n user.xdg.tags $d/a.txt 2>/dev/null)" = Red ] &&
         [ "$(getfattr --only-values -n user.baloo.rating $d/b.txt 2>/dev/null)" = 8 ]'
    check "$BK: 12b the first tag sets [tags] used in state.toml" 'grep -q "^used = true" $cfg/state.toml'
    K ctrl+z; sleep 1.5
    check "$BK: 12b Ctrl+Z takes the picker's job back at once" \
        '! getfattr -n user.xdg.tags $d/a.txt >/dev/null 2>&1 && ! getfattr -n user.baloo.rating $d/b.txt >/dev/null 2>&1'
    K ctrl+y; sleep 1.5
    C 260 196; K alt+1 alt+shift+1; sleep 1.5
    check "$BK: 12b Alt+Shift+1 tags c.txt Red" '[ "$(getfattr --only-values -n user.xdg.tags $d/c.txt 2>/dev/null)" = Red ]'
    K ctrl+shift+l Down Down Down Down Down Down Down Down Return "=Ready for review" Return; sleep 1.5
    check "$BK: 12d Comment… writes user.xdg.comment" \
        '[ "$(getfattr --only-values -n user.xdg.comment $d/c.txt 2>/dev/null)" = "Ready for review" ]'
    S tags-dots
    filt() { K ctrl+f "=$1"; sleep 1.5; K Down; names; K Escape Escape; sleep 0.5; }
    check "$BK: 12c filter tag:red (case ignored)" '[ "$(filt tag:red)" = "a.txt b.txt c.txt" ]'
    check "$BK: 12c filter tag:blue finds a tag written by setfattr (Dolphin's attribute)" '[ "$(filt tag:blue)" = d.txt ]'
    check "$BK: 12c filter tag:none" '[ "$(filt tag:none)" = Sub ]'
    check "$BK: 12d filter rating:>=3 (Baloo's 6 is three stars)" '[ "$(filt "rating:>=3")" = "a.txt b.txt d.txt" ]'
    C 64 175; sleep 2.5; S tags-sidebar
    check "$BK: 12e sidebar TAGS ▸ Red opens a \"Tag: Red\" search" '[ "$(title)" = "Tag: Red — Gezik" ]'
    C 260 153
    check "$BK: 12e ...that finds the three Red files" '[ "$(names)" = "a.txt b.txt c.txt" ]'
    K Escape; K ctrl+l "=$d" Return; sleep 1.5
    S tags-hl-before
    printf '\n[[highlight-rules]]\ntag = "Blue"\ncolor = "#00a000"\n' >> $cfg/settings.toml; sleep 2.5
    C 640 500; sleep 0.5; S tags-hl
    # The list may keep the search bar of the tag search (rows 34 px lower): count the
    # green pixels of the whole list, one row's worth (d.txt) and none before.
    local g0 g1; g0=$(greens tags-hl-before 1040 400 220 105); g1=$(greens tags-hl 1040 400 220 105)
    echo "     green pixels in the list: before $g0, after $g1"
    check "$BK: 12f highlight rule tag=Blue tints one row (d.txt) green" '[ "$g0" -lt 5 ] && [ "$g1" -gt 50 ] && [ "$g1" -lt 900 ]'
    alive && pass "$BK: 12 Gezik still runs" || fail "$BK: 12 Gezik still runs"
    unset -f filt
}

# ---- 13a-13f: preview (text, hex, PDF, properties), property columns, filters, search,
# name templates; 13c playback through the null sink. ----
part_preview() {
    local d=/tmp/s13 cfg=/tmp/s13cfg
    rm -rf $d $cfg && mkdir -p $cfg/tools && cp -r /tmp/steps-media $d && cp -r /tmp/pdfium $cfg/tools/pdfium-8086 && chmod +x $cfg/tools/pdfium-8086/libpdfium.so
    printf '[session]\nrestore = false\n' > $cfg/settings.toml
    start $cfg $d
    # Rows: clip.mkv 118, code.rs 144, doc.pdf 170, lib.bin 196, photo.jpg 222, song1 248, song2 274.
    K alt+p; sleep 1
    C 260 144; sleep 2; S pv-code
    check "$BK: 13a a .rs file shows as text with line numbers" 'ocr pv-code 270 60 1000 85 | grep -q "Sorting"'
    C 260 196; sleep 1.5; C 1025 523; sleep 1.5; S pv-hex
    check "$BK: 13a Hex on a binary shows its bytes (7F 45 4C 46)" 'ocr pv-hex 100 22 1105 86 | grep -q "7F 45"'
    C 260 170; sleep 3; S pv-pdf
    C 1130 300; K Page_Down; sleep 4; S pv-pdf2
    check "$BK: 13b the first Page Down in a PDF shows \"Page 2\"" 'ocr pv-pdf2 270 120 1000 85 | grep -q "Page 2"'
    K Page_Down; sleep 3; C 1087 485; sleep 3; S pv-pdf3
    check "$BK: 13b more Page Down and › turn pages through pdfium (\"Page 2\" or \"Page 3\")" \
        'ocr pv-pdf3 270 120 1000 85 | grep -q "Page [23]"'
    check "$BK: 13b ...in the --pdf-view worker" 'pgrep -f -- "--pdf-view" >/dev/null'
    C 260 248; sleep 2.5; S pv-mp3
    check "$BK: 13d MP3 properties: Artist and Album rows" \
        'ocr pv-mp3 270 250 1000 410 | grep -q "Ferah" && ocr pv-mp3 270 250 1000 410 | grep -q "Kalp"'
    C 260 222; sleep 2.5; S pv-jpg
    check "$BK: 13d JPEG properties: Camera and Date Taken" \
        'ocr pv-jpg 270 160 1000 520 | grep -q "Canon EOS" && ocr pv-jpg 270 160 1000 520 | grep -q "2024-05-01"'
    C 260 118; sleep 2.5; S pv-mkv
    check "$BK: 13d MKV properties: Video Codec H.264 and 320 x 240" \
        'ocr pv-mkv 270 230 1000 430 | grep -q "H.264" && ocr pv-mkv 270 230 1000 430 | grep -q "320"'
    # 13c: ▶ under the poster of an MP3 (no picture: the bar is at y 287).
    C 260 274; sleep 2; C 1017 287; sleep 1.5; S pv-play
    check "$BK: 13c ▶ starts ffmpeg" 'pgrep -x ffmpeg >/dev/null'
    check "$BK: 13c ...and a PulseAudio stream (null sink)" \
        '[ "$(PULSE_SERVER=unix:/tmp/pulse.sock pactl list short sink-inputs | wc -l)" -ge 1 ]'
    C 260 144; sleep 1.5
    check "$BK: 13c selecting another file stops ffmpeg and the stream" \
        '! pgrep -x ffmpeg >/dev/null && [ "$(PULSE_SERVER=unix:/tmp/pulse.sock pactl list short sink-inputs | wc -l)" = 0 ]'
    K alt+p; sleep 1
    # 13e: header ▸ More Columns… ▸ "art" ▸ Artist.
    C 400 91 3; sleep 0.8; S pv-header
    C 480 338; sleep 1; K "=art" Return; sleep 0.5; K Escape; sleep 2; S pv-artist
    check "$BK: 13e More Columns… adds an Artist column with the tag" 'ocr pv-artist 180 190 1100 85 | grep -q "Ferah"'
    filt() { K ctrl+f "=$1"; sleep 2; K Down; names; K Escape Escape; sleep 0.5; }
    check "$BK: 13e filter artist:ŞEBNEM finds Şebnem (Turkish case)" '[ "$(filt "artist:ŞEBNEM")" = song1.mp3 ]'
    check "$BK: 13e filter duration>7s" '[ "$(filt "duration>7s")" = "song2.mp3" ]'
    # 13f: Ctrl+Shift+F, Filters ▸ Properties… (artist:şebnem), *.mp3.
    K ctrl+shift+f; sleep 1; C 1105 95; sleep 0.8; C 950 293; sleep 1
    K "=artist:şebnem" Return; sleep 1; C 640 95; K "=*.mp3" Return; sleep 3; S pv-search
    C 260 153
    check "$BK: 13f search with Properties… artist:şebnem finds only song1.mp3" '[ "$(names)" = song1.mp3 ]'
    K Escape; K ctrl+l "=$d" Return; sleep 1.5
    # 13f: batch rename, Add rule ▸ Name from template, {track:02} {artist}.
    K ctrl+f "=song" Down ctrl+a F2; sleep 1.5; C 107 465; sleep 0.8; C 138 395; sleep 1
    C 214 517; K ctrl+a "={track:02} {artist}"; sleep 2.5; S pv-template
    C 1204 644; sleep 2
    check "$BK: 13f template {track:02} {artist} renames to \"03 Şebnem Ferah.mp3\" and \"12 Other Band.mp3\"" \
        '[ -f "$d/03 Şebnem Ferah.mp3" ] && [ -f "$d/12 Other Band.mp3" ]'
    K ctrl+z; sleep 1.5
    check "$BK: 13f Ctrl+Z undoes the template rename" '[ -f $d/song1.mp3 ] && [ -f $d/song2.mp3 ]'
    alive && pass "$BK: 13 Gezik still runs" || fail "$BK: 13 Gezik still runs"
    unset -f filt
}

# ---- 14a-14d: the settings window (palette, density), Set Key…, themes, export/import/Undo;
# then the Keyboard page. ----
part_settings() {
    # The settings window is slow to lay out in a debug build: Wayland input waits longer.
    local WL_PAUSE=2
    local d=/tmp/s14 cfg=/tmp/s14cfg
    rm -rf $d $cfg /tmp/s14-*.zip && mkdir -p $d $cfg && echo a > $d/a.txt
    # The full template, so that a change is one line among the others.
    sed 's/^restore = true/restore = false/' /src/crates/gezik-config/templates/settings.toml > $cfg/settings.toml
    start $cfg $d
    cp $cfg/settings.toml /tmp/s14-before.toml
    K ctrl+shift+p "=settings" Return; sleep 3
    [ "$BK" = wayland ] && swaymsg -q '[title="^Settings"] floating enable, resize set width 824 px height 627 px, move position center'
    sleep 1; settings; S set-open
    check "$BK: 14a the palette's Settings opens \"Settings — Gezik\"" \
        '[ "$BK" = x11 ] && xdotool search --name "^Settings" >/dev/null || wl_geom "^Settings" | grep -q .'
    # Appearance ▸ Density ▸ compact.
    C 63 56; sleep 1; C 667 336; sleep 0.8; C 255 385; sleep 2; S set-density
    check "$BK: 14a Density ▸ compact changes only that line of settings.toml" \
        '[ "$(diff /tmp/s14-before.toml $cfg/settings.toml | grep -c "^[<>]")" = 2 ] &&
         diff /tmp/s14-before.toml $cfg/settings.toml | grep -q "^> density = \"compact\""'
    # From here the window is compact: the search box at y 20, the first found row at 54.
    # 14b: Set Key… of Get Theme… (found by the search), Ctrl+W: "… used by Close Tab — Replace?".
    C 500 20; sleep 1; K ctrl+a "=get theme"; sleep 2
    cp $cfg/settings.toml /tmp/s14-prekey.toml
    C 758 54; sleep 1; K ctrl+w; sleep 1; S set-replace
    check "$BK: 14b Set Key… with Ctrl+W asks Replace? (Close Tab has it)" \
        'ocr set-replace 640 24 0 572 | grep -qi "Close Tab"'
    C 240 117; sleep 1
    check "$BK: 14b Cancel leaves settings.toml alone and Gezik running" 'cmp -s /tmp/s14-prekey.toml $cfg/settings.toml && alive'
    C 758 54; sleep 1; K ctrl+alt+k; sleep 1.5
    check "$BK: 14b a free key (Ctrl+Alt+K) is written as install-theme = \"mod+alt+k\"" \
        'grep -q "^install-theme = \"mod+alt+k\"" $cfg/settings.toml'
    # 14d on the Sync and Backup page: export, change a line, import (Import from ▾ ▸ Import),
    # Undo Import ▾ ▸ Undo: the changed file comes back byte for byte.
    C 500 20; sleep 1; K ctrl+a BackSpace; sleep 4
    ensure "Export Settings" 300 30 180 40 C 72 211
    C 677 54; sleep 1; K ctrl+a "=/tmp/s14-exp.zip" Return; sleep 3; S set-export
    check "$BK: 14d Export Settings writes the zip" 'unzip -l /tmp/s14-exp.zip 2>/dev/null | grep -q settings.toml'
    sed -i 's/^confirm-trash = false/confirm-trash = true/' $cfg/settings.toml; sleep 2
    cp $cfg/settings.toml /tmp/s14-changed.toml
    C 677 239; sleep 1; K ctrl+a "=/tmp/s14-exp.zip" Return; sleep 3; S set-import
    check "$BK: 14d Import lists the change (files.confirm-trash: true → false)" 'ocr set-import 620 40 190 318 | grep -q "confirm-trash"'
    C 677 371; sleep 0.8; C 240 414; sleep 2.5; S set-imported
    check "$BK: 14d Import writes the zip's settings.toml" 'grep -q "^confirm-trash = false" $cfg/settings.toml'
    C 677 384; sleep 0.8; C 237 440; sleep 2; S set-undone
    check "$BK: 14d Undo Import puts settings.toml back byte for byte" 'cmp -s /tmp/s14-changed.toml $cfg/settings.toml'
    # Nothing should run while the settings window just sits there.
    local t0 t1; t0=$(awk '{print $14}' /proc/$GPID/stat); sleep 10; t1=$(awk '{print $14}' /proc/$GPID/stat)
    echo "     UI CPU with the settings window idle on Sync and Backup: $(( (t1 - t0) * 10 / $(getconf CLK_TCK) ))% of one core"
    check "$BK: 14a the settings window is idle when nothing happens (under 5% CPU)" \
        '[ $(( (t1 - t0) * 10 / $(getconf CLK_TCK) )) -lt 5 ]'
    # 14c: Get Theme refuses a local path, then the repository's nord.toml blob address.
    C 500 20; sleep 1; K ctrl+a "=get theme"; sleep 2
    K Tab Tab ctrl+a "=/tmp/s14/nord.toml" Return; sleep 3; S set-theme-local
    check "$BK: 14c Get Theme refuses a local path" \
        'ocr set-theme-local 640 24 0 572 | grep -qi "https" && [ ! -e $cfg/themes/nord.toml ]'
    K ctrl+a "=https://github.com/wenlar/gezik/blob/master/crates/gezik-config/tests/themes/nord.toml" Return
    sleep 8; C 500 20; sleep 1
    ensure "Nord" 300 30 190 105 K ctrl+a "=nord"
    ensure "Install" 300 30 190 150 C 677 120
    S set-theme-found; C 238 163; sleep 3
    check "$BK: 14c the blob address installs themes/nord.toml" 'grep -q "^name = \"Nord\"" $cfg/themes/nord.toml'
    # The Keyboard page last: on this build it keeps the UI thread busy (see the notes).
    C 500 20; sleep 1; K ctrl+a BackSpace; sleep 3; C 46 139; sleep 2
    if busy $GPID; then
        S set-keyboard
        fail "$BK: 14b the Keyboard page opens (the UI thread is still busy 10 s later)"
    else
        S set-keyboard
        check "$BK: 14b the Keyboard page opens (its Set Key… buttons show)" 'ocr set-keyboard 620 500 180 40 | grep -q "Set Key"'
    fi
    pkill -x gezik
}

# ---- 14e: a gezik-portable file beside the binary keeps everything in gezik-data. ----
part_portable() {
    local p=/target/steps-portable
    rm -rf $p /root/.config/gezik.steps-bak && mkdir -p $p && ln $GEZIK $p/gezik && touch $p/gezik-portable
    [ -d /root/.config/gezik ] && mv /root/.config/gezik /root/.config/gezik.steps-bak
    pkill -x gezik 2>/dev/null; sleep 0.5
    env -u GEZIK_CONFIG_DIR $p/gezik /tmp >>/tmp/gezik-steps-$BK.log 2>&1 &
    GPID=$!
    sleep 5
    check "$BK: 14e the portable marker keeps settings in gezik-data beside the binary" \
        '[ -f $p/gezik-data/settings.toml ] && [ ! -e /root/.config/gezik ]'
    kill $GPID 2>/dev/null; wait $GPID 2>/dev/null
    rm -rf $p
    [ -d /root/.config/gezik.steps-bak ] && mv /root/.config/gezik.steps-bak /root/.config/gezik
}

steps_x11() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99 LC_ALL=C.UTF-8
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    drv x11
    : >/tmp/gezik-steps-$BK.log
    for p in ${STEPS_PARTS:-tags preview settings portable}; do part_$p; done
    pkill -x gezik; kill $xvfb 2>/dev/null; wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-steps-x11.log && fail "x11: steps: no panic" || pass "x11: steps: no panic"
}

steps_wayland() {
    export XDG_RUNTIME_DIR=/tmp/xdg LC_ALL=C.UTF-8 && mkdir -p $XDG_RUNTIME_DIR && chmod 700 $XDG_RUNTIME_DIR
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman sway -c /dev/null >/tmp/sway.log 2>&1 &
    local sway=$!
    sleep 2
    export WAYLAND_DISPLAY=$(ls $XDG_RUNTIME_DIR | grep -m1 '^wayland-[0-9]*$')
    export SWAYSOCK=$(ls $XDG_RUNTIME_DIR/sway-ipc.*.sock 2>/dev/null | head -1)
    unset DISPLAY
    . /src/scripts/linux/gui-lib.sh
    drv wl
    # A virtual keyboard that stays for the whole run: without it the seat has no keyboard
    # between two wtype runs, Gezik's window loses the keyboard each time and gets its old
    # focus back on the next run, so a click on another text field in between is undone.
    wtype -s 86400000 &
    local keep=$!
    : >/tmp/gezik-steps-$BK.log
    for p in ${STEPS_PARTS:-tags preview settings portable}; do part_$p; done
    pkill -x gezik; kill $keep $sway 2>/dev/null; wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-steps-wayland.log && fail "wayland: steps: no panic" || pass "wayland: steps: no panic"
}
