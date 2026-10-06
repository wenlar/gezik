#!/usr/bin/env bash
# Tries Gezik's Linux clipboard and drag and drop in a container (see Dockerfile):
#   docker build -t gezik-linux scripts/linux
#   docker run --rm -v "$PWD:/src" -v gezik-target:/target -v gezik-cargo:/usr/local/cargo/registry \
#       -e CARGO_TARGET_DIR=/target gezik-linux bash scripts/linux/test.sh [x11|wayland|all|unit]
# Each check prints "ok" or "FAIL"; the exit code is the number of failures. `unit` runs
# `cargo test --workspace` instead (not part of `all`).
set -u
cd /src
cargo build -p gezik 2>&1 | tail -1
GEZIK=/target/debug/gezik
failures=0
pass() { echo "ok   $1"; }
fail() { echo "FAIL $1"; failures=$((failures + 1)); }
check() { if eval "$2"; then pass "$1"; else fail "$1"; fi; }
# A screenshot for looking at a failure (X11 only), in .superpowers/linux-shots.
shot() { mkdir -p /src/.superpowers/linux-shots && import -window root "/src/.superpowers/linux-shots/$1.png" 2>/dev/null; }
# Real key presses (XTEST) to the focused window; winit ignores synthetic ones.
keys() { xdotool windowfocus --sync "$1" 2>/dev/null; xdotool key "$2"; }

fresh_tree() {
    rm -rf /tmp/t /tmp/src /tmp/cfg && mkdir -p /tmp/t/D /tmp/src /tmp/cfg
    echo a > /tmp/t/a.txt; echo b > /tmp/t/b.txt
    echo z > /tmp/src/z.txt; echo y > /tmp/src/y.txt
}

x11() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    export DISPLAY=:99
    sleep 1
    fresh_tree
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/t >/tmp/gezik-x11.log 2>&1 &
    local gezik=$!
    sleep 3
    local win
    win=$(xdotool search --name "Gezik" | head -1)
    check "x11: the window opens" '[ -n "$win" ]'
    xdotool windowmove "$win" 0 0; sleep 0.5
    # Rows (no window manager, so no title bar): D at y 118, a.txt 144, b.txt 170.
    shot x11-start
    xdotool mousemove --window "$win" 260 144 click 1; sleep 0.5
    keys "$win" ctrl+c; sleep 0.5; shot x11-copied
    check "x11: Gezik's copy offers the GNOME format" \
        '[ "$(xclip -selection clipboard -t x-special/gnome-copied-files -o 2>/dev/null)" = "$(printf "copy\nfile:///tmp/t/a.txt")" ]'
    check "x11: and a URI list" \
        'xclip -selection clipboard -t text/uri-list -o 2>/dev/null | grep -q "^file:///tmp/t/a.txt"'
    keys "$win" ctrl+x; sleep 0.5
    check "x11: a cut says so" \
        'xclip -selection clipboard -t x-special/gnome-copied-files -o 2>/dev/null | head -1 | grep -qx cut'
    # Another program's copy pasted in Gezik.
    printf 'file:///tmp/src/z.txt\r\n' | xclip -selection clipboard -t text/uri-list -i; sleep 0.5
    keys "$win" ctrl+v; sleep 2
    check "x11: a URI list from xclip pastes as a copy" '[ -f /tmp/t/z.txt ] && [ -f /tmp/src/z.txt ]'
    printf 'cut\nfile:///tmp/src/y.txt' | xclip -selection clipboard -t x-special/gnome-copied-files -i; sleep 0.5
    keys "$win" ctrl+v; sleep 2
    check "x11: a GNOME cut pastes as a move" '[ -f /tmp/t/y.txt ] && [ ! -f /tmp/src/y.txt ]'

    # XDND between two Gezik windows: drag b.txt from the first onto folder D in the second.
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/t >/tmp/gezik-x11-b.log 2>&1 &
    local second=$!
    sleep 3
    local win2
    win2=$(xdotool search --name "Gezik" | grep -v "^$win$" | head -1)
    check "x11: a second window opens" '[ -n "$win2" ]'
    xdotool windowmove "$win2" 800 0; sleep 1
    # The first window lists D, a.txt, b.txt, y.txt, z.txt now; b.txt (third row) goes onto D,
    # the first row of the second window (at x 800).
    xdotool mousemove --window "$win" 260 170 mousedown 1
    for x in 280 400 600 750 900 1000 1050 1060; do xdotool mousemove $x 118; sleep 0.1; done
    sleep 0.5
    shot x11-xdnd-over
    xdotool mouseup 1; sleep 2
    shot x11-xdnd-after
    check "x11: XDND drop moves b.txt into D" '[ -f /tmp/t/D/b.txt ] && [ ! -f /tmp/t/b.txt ]'
    kill $gezik $second 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-x11*.log && fail "x11: no panic" || pass "x11: no panic"
    echo "--- gezik log:"; tail -5 /tmp/gezik-x11.log
}

wayland() {
    export XDG_RUNTIME_DIR=/tmp/xdg && mkdir -p $XDG_RUNTIME_DIR && chmod 700 $XDG_RUNTIME_DIR
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman sway -c /dev/null >/tmp/sway.log 2>&1 &
    local sway=$!
    sleep 2
    export WAYLAND_DISPLAY=$(ls $XDG_RUNTIME_DIR | grep -m1 '^wayland-[0-9]*$')
    export SWAYSOCK=$(ls $XDG_RUNTIME_DIR/sway-ipc.*.sock 2>/dev/null | head -1)
    unset DISPLAY
    check "wayland: sway runs" '[ -n "$WAYLAND_DISPLAY" ]'
    fresh_tree
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/t >/tmp/gezik-wl.log 2>&1 &
    local gezik=$!
    sleep 3
    # The new window has the keyboard: Down selects the first entry (D), Down again a.txt.
    # Each wtype is a new virtual keyboard: it waits first, so programs see the keyboard
    # before its keys, and presses a real Control key (-M only sets the modifier state, which
    # winit does not take as Ctrl held).
    wtype -s 1000 -k Down -s 300 -k Down -s 300 -P Control_L -k c -p Control_L -s 500
    check "wayland: Gezik's copy offers the GNOME format" \
        '[ "$(wl-paste -n -t x-special/gnome-copied-files 2>/dev/null)" = "$(printf "copy\nfile:///tmp/t/a.txt")" ]'
    printf 'file:///tmp/src/z.txt\r\n' | wl-copy -t text/uri-list; sleep 0.5
    wtype -s 1000 -P Control_L -k v -p Control_L -s 500; sleep 2
    check "wayland: a URI list from wl-copy pastes as a copy" '[ -f /tmp/t/z.txt ] && [ -f /tmp/src/z.txt ]'

    # Drag and drop between two Gezik windows (sway puts them side by side), with a virtual
    # pointer: b.txt from the first window onto folder D in the second.
    cargo build -p gezik-platform --example vpointer 2>&1 | tail -1
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/t >/tmp/gezik-wl-b.log 2>&1 &
    local second=$!
    sleep 3
    # Each window's content rectangle, left one first, and the output's size.
    local geometry
    geometry=$(swaymsg -t get_tree | python3 -c '
import json, sys
views = []
def walk(node):
    if node.get("pid") and "Gezik" in (node.get("name") or ""):
        r = node["rect"]; w = node["window_rect"]
        views.append((r["x"] + w["x"], r["y"] + w["y"]))
    for child in node.get("nodes", []) + node.get("floating_nodes", []):
        walk(child)
tree = json.load(sys.stdin)
walk(tree)
views.sort()
out = next(o for o in tree["nodes"] if o["name"] != "__i3")
print(out["rect"]["width"], out["rect"]["height"], *[c for v in views for c in v])
')
    set -- $geometry
    local fields=$#
    echo "     output and windows: $geometry"
    check "wayland: two windows side by side" '[ $fields -ge 6 ]'
    if [ $fields -ge 6 ]; then
        local w=$1 h=$2 x1=$3 y1=$4 x2=$5 y2=$6
        local fx=$((x1 + 260)) fy=$((y1 + 170)) tx=$((x2 + 260)) ty=$((y2 + 118))
        local path="move $fx $fy sleep 200 down sleep 200"
        for i in 1 2 3 4 5 6 7 8 9 10; do
            path="$path move $((fx + (tx - fx) * i / 10)) $((fy + (ty - fy) * i / 10)) sleep 100"
        done
        /target/debug/examples/vpointer $w $h $path sleep 500 up sleep 300
        sleep 2
        check "wayland: a drop from another window moves b.txt into D" '[ -f /tmp/t/D/b.txt ] && [ ! -f /tmp/t/b.txt ]'
        # A drag that leaves its window (the compositor takes it) and comes back: a.txt (second
        # row) out over the second window, then back onto D in its own window.
        local ax=$((x1 + 260)) ay=$((y1 + 144)) dx=$((x1 + 260)) dy=$((y1 + 118)) ox=$((x2 + 300)) oy=$((y2 + 300))
        path="move $ax $ay sleep 200 down sleep 200"
        for i in 1 2 3 4 5 6 7 8; do path="$path move $((ax + (ox - ax) * i / 8)) $((ay + (oy - ay) * i / 8)) sleep 100"; done
        for i in 1 2 3 4 5 6 7 8; do path="$path move $((ox + (dx - ox) * i / 8)) $((oy + (dy - oy) * i / 8)) sleep 100"; done
        /target/debug/examples/vpointer $w $h $path sleep 500 up sleep 300
        sleep 2
        check "wayland: a drag back into its own window drops there" '[ -f /tmp/t/D/a.txt ] && [ ! -f /tmp/t/a.txt ]'
    fi
    kill $second 2>/dev/null
    kill $gezik $sway 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-wl*.log && fail "wayland: no panic" || pass "wayland: no panic"
    echo "--- gezik logs:"; tail -n 3 /tmp/gezik-wl.log /tmp/gezik-wl-b.log
}

unit() {
    # Temporary files on the same drive as the home folder, whose trash (~/.local/share/Trash)
    # the undo tests use: /tmp is often a tmpfs, another drive with no trash. (In this image
    # / and /tmp are one overlay, so here it changes nothing.)
    export TMPDIR="$HOME/tmp"
    mkdir -p "$TMPDIR"
    cargo test --workspace >/tmp/unit.log 2>&1
    local status=$?
    grep -E "^(test result|failures:|    [a-z_:]+$)|panicked|Running " /tmp/unit.log
    check "unit: cargo test --workspace" '[ $status -eq 0 ]'
}

case "${1:-all}" in
    x11) x11 ;;
    wayland) wayland ;;
    unit) unit ;;
    *) x11; wayland ;;
esac
echo "failures: $failures"
exit $failures
