#!/usr/bin/env bash
# Drives Gezik's GUI on Linux through file operations, batch rename and archives, with real
# input: xdotool (XTEST) under Xvfb, wtype and the vpointer example under a headless sway.
#   docker build -t gezik-linux scripts/linux
#   docker run --rm -v "$PWD:/src" -v gezik-target:/target -v gezik-cargo:/usr/local/cargo/registry \
#       -e CARGO_TARGET_DIR=/target gezik-linux bash scripts/linux/gui.sh [x11|wayland|popups|tabdrag|pdf|pdfnote|filter|select|tabs|keyboard|all]
#   (not in `all`, release builds: `filterperf` times the filter at 100,000 files, `memory [exe]`
#   gives idle memory)
# Needs the network (apt for 7-Zip, Gezik's own 7-Zip download). Screenshots go to
# .superpowers/linux-shots/gui-*.png. Each check prints "ok" or "FAIL"; the exit code is the
# number of failures. Coordinates are the window's (no window manager under Xvfb, so it is
# at 0,0); dialogs are found where Gezik put them at the window sizes set below.
set -u
cd /src
cargo build -p gezik 2>&1 | tail -1
GEZIK=/target/debug/gezik
export HOME=/root
failures=0
pass() { echo "ok   $1"; }
fail() { echo "FAIL $1"; failures=$((failures + 1)); }
check() { if eval "$2"; then pass "$1"; else fail "$1"; fi; }

# 7-Zip from Debian makes the fixtures (AES zip, wim); Gezik must not find it afterwards, so
# that it offers its own download.
setup_7zip() {
    if [ ! -x /usr/lib/7zip/7z ]; then
        apt-get update -qq >/dev/null && DEBIAN_FRONTEND=noninteractive \
            apt-get install -y -qq --no-install-recommends 7zip grim >/dev/null 2>&1
    fi
    command -v grim >/dev/null || { apt-get install -y -qq --no-install-recommends grim >/dev/null 2>&1; }
    SEVEN=/usr/lib/7zip/7z
    echo "     Debian's $($SEVEN | sed -n 2p | cut -c1-30)"
}
hide_7zip() { for f in 7z 7za 7zr; do [ -e /usr/bin/$f ] && mv /usr/bin/$f /usr/bin/$f.off; done; }

fresh_files() {
    rm -rf /tmp/t /tmp/cfg /root/.local/share/Trash && mkdir -p /tmp/t/Docs/sub /tmp/t/Proj /tmp/cfg
    for f in a b c; do echo $f > /tmp/t/$f.txt; done
    echo inner > /tmp/t/Docs/inner.txt
    for i in 1 2 3; do echo $i > /tmp/t/Proj/photo$i.txt; done
    echo AAA > /tmp/t/Proj/a.txt; echo BBB > /tmp/t/Proj/b.txt
}

fresh_archives() {
    rm -rf /tmp/a /tmp/mk && mkdir -p /tmp/a /tmp/mk/one/inner /tmp/mk/m
    (
        cd /tmp/mk
        echo hello > one/hello.txt; echo deep > one/inner/deep.txt
        echo m1 > m/m1.txt; echo m2 > m/m2.txt
        $SEVEN a -tzip /tmp/a/one.zip one >/dev/null
        tar czf /tmp/a/tg.tar.gz one
        cd m
        $SEVEN a -tzip /tmp/a/multi.zip m1.txt m2.txt >/dev/null
        $SEVEN a -tzip -mem=AES256 -psecret /tmp/a/aes.zip m1.txt >/dev/null
        $SEVEN a -twim /tmp/a/w.wim m1.txt m2.txt >/dev/null
        mkdir -p ../big
        for i in 1 2 3 4 5 6; do head -c 300M /dev/urandom > ../big/r$i.bin; done
        cd .. && $SEVEN a -tzip -mx0 /tmp/a/big.zip big >/dev/null && rm -rf big
    )
    echo z1 > /tmp/a/z1.txt; echo z2 > /tmp/a/z2.txt
}

x11() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    fresh_files
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/t >/tmp/gezik-gui-x11.log 2>&1 &
    local gezik=$!
    sleep 3
    check "x11: the window opens" '[ -n "$(win)" ]'
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    shot gui-x11-start
    title() { xdotool getwindowname "$(win)"; }

    # A. Basics. Rows: Docs 118, Proj 144, a.txt 170, b.txt 196, c.txt 222.
    dclick 255 118
    check "x11: double click opens a folder" '[ "$(title)" = "Docs — Gezik" ]'
    key alt+Left; sleep 0.5
    check "x11: Alt+Left goes back" '[ "$(title)" = "t — Gezik" ]'
    dclick 255 118; key alt+Up; sleep 0.5
    check "x11: Alt+Up goes up" '[ "$(title)" = "t — Gezik" ]'
    click 255 170; key ctrl+c; dclick 255 118; key ctrl+v; sleep 1.5
    check "x11: copy and paste" '[ -f /tmp/t/Docs/a.txt ] && [ -f /tmp/t/a.txt ]'
    key alt+Left; sleep 0.5; click 255 196; key ctrl+x; dclick 255 118; key ctrl+v; sleep 1.5
    check "x11: cut and paste moves" '[ -f /tmp/t/Docs/b.txt ] && [ ! -f /tmp/t/b.txt ]'
    key ctrl+z; sleep 1.5
    check "x11: Ctrl+Z undoes the move" '[ -f /tmp/t/b.txt ] && [ ! -f /tmp/t/Docs/b.txt ]'
    key ctrl+y; sleep 1.5
    check "x11: Ctrl+Y redoes it" '[ -f /tmp/t/Docs/b.txt ] && [ ! -f /tmp/t/b.txt ]'
    key alt+Left; sleep 0.5
    # Now Docs, Proj, a.txt (170), c.txt (196).
    click 255 196; key Delete; sleep 1.5
    shot gui-x11-trashed
    check "x11: Delete moves c.txt to the trash" \
        '[ ! -f /tmp/t/c.txt ] && [ -f ~/.local/share/Trash/files/c.txt ] && grep -q "^Path=/tmp/t/c.txt$" ~/.local/share/Trash/info/c.txt.trashinfo'
    key ctrl+z; sleep 1.5
    check "x11: Ctrl+Z restores it from the trash" '[ -f /tmp/t/c.txt ] && [ ! -e ~/.local/share/Trash/files/c.txt ]'
    key ctrl+shift+n; sleep 1; shot gui-x11-new-folder; typ Neu; key Return; sleep 1
    check "x11: Ctrl+Shift+N makes a folder named in place" '[ -d /tmp/t/Neu ] && [ ! -e "/tmp/t/New folder" ]'
    # Docs, Neu, Proj, a.txt (196), c.txt.
    click 255 196; key F2; sleep 0.5; shot gui-x11-f2; typ alpha; key Return; sleep 1
    check "x11: F2 renames in place (the name without its extension)" '[ -f /tmp/t/alpha.txt ] && [ ! -f /tmp/t/a.txt ]'
    key ctrl+z; sleep 1
    check "x11: Ctrl+Z undoes the rename" '[ -f /tmp/t/a.txt ] && [ ! -f /tmp/t/alpha.txt ]'
    # Another window takes the focus while the name editor is open, then Gezik gets it back:
    # the editor is gone, and the keyboard should still reach the list (F2 again).
    xev -geometry 200x100+1200+100 >/dev/null 2>&1 &
    local other=$!
    sleep 1
    click 255 196; key F2; sleep 0.5
    xdotool windowfocus --sync "$(xdotool search --name "Event Tester" | head -1)"; sleep 0.5
    shot gui-x11-focus-away
    key F2; sleep 0.5; shot gui-x11-focus-back; typ beta; key Return; sleep 1
    check "x11: after the window loses the focus during F2, the keyboard still reaches the list" \
        '[ -f /tmp/t/beta.txt ]'
    [ -f /tmp/t/beta.txt ] && mv /tmp/t/beta.txt /tmp/t/a.txt; sleep 1
    # The same with the new folder's name field, then F2 on the new folder.
    click 255 196; key ctrl+shift+n; sleep 1
    xdotool windowfocus --sync "$(xdotool search --name "Event Tester" | head -1)"; sleep 0.5
    key F2; sleep 0.5; typ Neu2; key Return; sleep 1
    check "x11: after the window loses the focus while naming a new folder, F2 renames it" '[ -d /tmp/t/Neu2 ]'
    rm -rf /tmp/t/Neu2 "/tmp/t/New folder"; sleep 1
    # And with the batch layer open: Esc still closes it, then F2 renames in place.
    # Docs, Neu, Proj, a.txt (196), c.txt (222).
    click 255 196; xdotool keydown shift; click 255 222; xdotool keyup shift
    key F2; sleep 1
    xdotool windowfocus --sync "$(xdotool search --name "Event Tester" | head -1)"; sleep 0.5
    key Escape End F2; sleep 0.5; typ gamma; key Return; sleep 1
    check "x11: after the window loses the focus with the batch layer open, Esc then F2 work" '[ -f /tmp/t/gamma.txt ]'
    kill $other 2>/dev/null
    [ -f /tmp/t/gamma.txt ] && mv /tmp/t/gamma.txt /tmp/t/c.txt

    # B. Batch rename in Proj: a.txt, b.txt, photo1-3.txt. The rename back just above makes
    # the listing refresh, so a row may move under a double-click: go there by its path and
    # wait until it is shown.
    sleep 1
    key ctrl+l; typ /tmp/t/Proj; key Return
    for _ in $(seq 1 20); do [ "$(title)" = "Proj — Gezik" ] && break; sleep 0.25; done
    sleep 0.5
    click 255 170; xdotool keydown shift; click 255 222; xdotool keyup shift
    key F2; sleep 1; shot gui-x11-batch
    # The layer dims the list behind it: the tab's folder icon fades.
    check "x11: F2 on 3 items opens the batch rename layer" '[ "$(px gui-x11-batch 14 20)" != "$(px gui-x11-start 14 20)" ]'
    click 248 406; typ photo; click 248 442; typ img; sleep 0.5; shot gui-x11-batch-rule
    click 824 549; sleep 1.5
    check "x11: Find/Replace renames on disk" '[ -f /tmp/t/Proj/img1.txt ] && [ -f /tmp/t/Proj/img3.txt ] && [ ! -e /tmp/t/Proj/photo2.txt ]'
    key ctrl+z; sleep 1.5
    check "x11: Ctrl+Z undoes the batch" '[ -f /tmp/t/Proj/photo1.txt ] && [ -f /tmp/t/Proj/photo3.txt ] && [ ! -e /tmp/t/Proj/img2.txt ]'
    # a.txt (118) and b.txt (144) swap names by typing them by hand.
    click 255 118; xdotool keydown ctrl; click 255 144; xdotool keyup ctrl
    key F2; sleep 1
    click 600 153; click 600 506; key ctrl+a; typ b.txt; key Return
    click 600 179; click 600 506; key ctrl+a; typ a.txt; key Return; sleep 0.5
    shot gui-x11-batch-swap
    click 824 549; sleep 1.5
    check "x11: a swap a.txt <-> b.txt" '[ "$(cat /tmp/t/Proj/a.txt)" = BBB ] && [ "$(cat /tmp/t/Proj/b.txt)" = AAA ]'
    key ctrl+z; sleep 1.5
    check "x11: Ctrl+Z undoes the swap" '[ "$(cat /tmp/t/Proj/a.txt)" = AAA ] && [ "$(cat /tmp/t/Proj/b.txt)" = BBB ]'
    key F2; sleep 1; shot gui-x11-batch-open; key Escape; sleep 0.5; shot gui-x11-batch-closed
    # The layer dims the list: the pixel at a row's name is lighter while it is open.
    check "x11: Esc closes the layer" \
        '[ "$(px gui-x11-batch-open 14 20)" != "$(px gui-x11-start 14 20)" ] && [ "$(px gui-x11-batch-closed 14 20)" = "$(px gui-x11-start 14 20)" ]'
    ls -A /tmp/t/Proj | grep -q "^\.gezik" && fail "x11: no .gezik leftovers in Proj" || pass "x11: no .gezik leftovers in Proj"

    # C. Archives, in /tmp/a, in a bigger window.
    kill $gezik 2>/dev/null; wait $gezik 2>/dev/null
    fresh_archives
    hide_7zip
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/a >>/tmp/gezik-gui-x11.log 2>&1 &
    gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 1100 700; sleep 0.5
    shot gui-x11-archives
    # Gezik's own menu: item 2 is "Extract here" on an archive.
    menu /tmp/a one.zip 2; sleep 2
    check "x11: Extract here, one root: one/ as it is" '[ "$(cat /tmp/a/one/inner/deep.txt 2>/dev/null)" = deep ] && [ ! -e /tmp/a/one/one ]'
    rm -rf /tmp/a/one; sleep 1.5
    menu /tmp/a multi.zip 2; sleep 2
    check "x11: Extract here, several roots: into multi/" '[ -f /tmp/a/multi/m1.txt ] && [ -f /tmp/a/multi/m2.txt ] && [ ! -e /tmp/a/m1.txt ]'
    rm -rf /tmp/a/multi; sleep 1.5
    menu /tmp/a tg.tar.gz 2; sleep 2
    check "x11: Extract here, tar.gz" '[ "$(cat /tmp/a/one/hello.txt 2>/dev/null)" = hello ]'
    rm -rf /tmp/a/one; sleep 1.5
    menu /tmp/a aes.zip 2; sleep 1.5
    shot gui-x11-password; typ nope; xdotool mousemove 0 0; sleep 0.5; shot gui-x11-password-typed
    # Something shows for what was typed; that it is dots is checked by eye in the shot.
    check "x11: the password field shows what is typed (as dots: see the shot)" \
        'awk "BEGIN { exit !($(dark gui-x11-password-typed 160 20 344 258) > 0.03) }"'
    key Return; sleep 1.5; shot gui-x11-password-wrong
    check "x11: a wrong password extracts nothing" '[ ! -e /tmp/a/m1.txt ]'
    typ secret; key Return; sleep 1.5
    check "x11: the right one extracts" '[ "$(cat /tmp/a/m1.txt 2>/dev/null)" = m1 ]'
    rm -f /tmp/a/m1.txt; sleep 1.5
    sel2() {
        local y1 y2; y1=$(row /tmp/a z1.txt); y2=$(row /tmp/a z2.txt)
        click 260 "$y1"; xdotool keydown ctrl; click 260 "$y2"; xdotool keyup ctrl; rclick 260 "$y2"; sleep 0.3
        echo "$y2" > /tmp/y2
    }
    # Compress… is item 1 on two files. The layer for a fresh config shows zip.
    sel2; click 330 $(( $(cat /tmp/y2) + 52 )); sleep 1
    click 411 262; sleep 0.5        # 7z: the layer grows (password, split)
    click 596 159; key ctrl+a; typ sec.7z; click 442 315; typ pw7; click 329 352; sleep 0.3
    shot gui-x11-compress-7z
    click 826 463; sleep 2.5
    check "x11: Compress… 7z with a password and encrypted names" \
        '! $SEVEN l -pbad /tmp/a/sec.7z >/dev/null 2>&1 && $SEVEN t -ppw7 /tmp/a/sec.7z 2>&1 | grep -q "Everything is Ok"'
    sleep 1; sel2; click 330 $(( $(cat /tmp/y2) + 52 )); sleep 1
    shot gui-x11-compress-again
    click 349 237; sleep 0.5        # back to zip
    click 596 184; key ctrl+a; typ zz.zip; sleep 0.3; shot gui-x11-compress-zip
    click 826 414; sleep 2
    check "x11: Compress… zip" '$SEVEN l /tmp/a/zz.zip 2>/dev/null | grep -q " 2 files"'
    sleep 1; sel2; shot gui-x11-compress-to-menu; click 330 $(( $(cat /tmp/y2) + 84 )); sleep 2
    check "x11: Compress to \"a.zip\"" '$SEVEN l /tmp/a/a.zip 2>/dev/null | grep -q " 2 files"'
    # Drag z1.txt onto zz.zip: "Add to zz.zip", then Keep both for the name already there.
    sleep 1
    local ys yd
    ys=$(row /tmp/a z1.txt); yd=$(row /tmp/a zz.zip)
    click 260 "$ys"; xdotool mousemove 260 "$ys" mousedown 1
    for d in 5 10 20 30 40 50; do xdotool mousemove 262 $((ys + d)); sleep 0.1; done
    xdotool mousemove 265 "$yd"; sleep 0.6; shot gui-x11-drag-onto-zip
    xdotool mouseup 1; sleep 1.5; shot gui-x11-add-conflict
    click 638 281; sleep 2
    check "x11: drop onto a zip adds to it (Keep both)" '$SEVEN l /tmp/a/zz.zip | grep -q "z1 (2).txt"'
    # A .wim needs 7-Zip: Gezik offers its download, then goes on.
    menu /tmp/a w.wim 2; sleep 1.5; shot gui-x11-needs-7zip
    click 437 291
    for _ in $(seq 1 60); do [ -f /tmp/a/w/m1.txt ] && break; sleep 1; done
    shot gui-x11-after-download
    check "x11: Download puts 7-Zip in the config dir's tools/7zip-26.03" '[ -x /tmp/cfg/tools/7zip-26.03/7zz ]'
    check "x11: and it runs" '/tmp/cfg/tools/7zip-26.03/7zz | grep -q "7-Zip (z) 26.03"'
    check "x11: and the wim is extracted" '[ "$(cat /tmp/a/w/m1.txt 2>/dev/null)" = m1 ]'
    rm -rf /tmp/a/w; sleep 1.5
    # Dismiss finished operations so the next one is on the panel's last line.
    for _ in 1 2 3 4 5 6; do click 1078 656; done
    menu /tmp/a big.zip 2; sleep 1.2; shot gui-x11-big-extract
    click 1078 656; sleep 4; shot gui-x11-big-cancelled
    check "x11: cancelling a big extract leaves nothing" \
        '[ ! -e /tmp/a/big ] && [ -z "$(find /tmp/a -maxdepth 1 -name ".gezik*")" ]'

    kill $gezik 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-x11.log && fail "x11: no panic" || pass "x11: no panic"
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
    cargo build -p gezik-platform --example vpointer 2>&1 | tail -1
    SHOTS=/src/.superpowers/linux-shots
    wshot() { mkdir -p $SHOTS && grim "$SHOTS/$1.png" 2>/dev/null; }
    # Each wtype is a new virtual keyboard that goes away at its end, which Gezik sees as the
    # window losing the keyboard: an inline name editor must get its keys from the same wtype.
    wk() { wtype -s 600 "$@" -s 300; sleep 0.5; }
    ctrl() { wk -P Control_L -k "$1" -p Control_L; }
    # The headless output is 1280x720 and the one window fills it below sway's title bar:
    # list rows at y 143, 26 apart. The virtual pointer clicks in output pixels.
    vclick() { /target/debug/examples/vpointer 1280 720 move "$1" "$2" sleep 150 down sleep 100 up sleep 300; sleep 0.3; }
    wrow() { echo $(( $(row "$1" "$2") + 25 )); }
    . /src/scripts/linux/gui-lib.sh
    wtitle() { swaymsg -t get_tree | python3 -c '
import json, sys
def walk(n):
    if n.get("pid") and n.get("focused"): print(n.get("name")); return
    for c in n.get("nodes", []) + n.get("floating_nodes", []): walk(c)
walk(json.load(sys.stdin))'; }
    fresh_files
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/t >/tmp/gezik-gui-wl.log 2>&1 &
    local gezik=$!
    sleep 3
    wshot gui-wl-start
    check "wayland: the window opens" '[ "$(wtitle)" = "t — Gezik" ]'
    # Rows: Docs, Proj, a.txt, b.txt, c.txt. Down selects the first.
    wk -k Down -k Return; sleep 0.5
    check "wayland: Enter opens a folder" '[ "$(wtitle)" = "Docs — Gezik" ]'
    wk -P Alt_L -k Left -p Alt_L; sleep 0.5
    check "wayland: Alt+Left goes back" '[ "$(wtitle)" = "t — Gezik" ]'
    vclick 260 "$(wrow /tmp/t a.txt)"; ctrl c
    vclick 260 "$(wrow /tmp/t Docs)"; wk -k Return; ctrl v; sleep 1.5
    check "wayland: copy and paste" '[ -f /tmp/t/Docs/a.txt ] && [ -f /tmp/t/a.txt ]'
    wk -P Alt_L -k Up -p Alt_L; sleep 0.5
    check "wayland: Alt+Up goes up" '[ "$(wtitle)" = "t — Gezik" ]'
    vclick 260 "$(wrow /tmp/t c.txt)"; wk -k Delete; sleep 1.5
    check "wayland: Delete moves c.txt to the trash" '[ ! -f /tmp/t/c.txt ] && [ -f ~/.local/share/Trash/files/c.txt ]'
    ctrl z; sleep 1.5
    check "wayland: Ctrl+Z restores it" '[ -f /tmp/t/c.txt ] && [ ! -e ~/.local/share/Trash/files/c.txt ]'
    vclick 260 "$(wrow /tmp/t a.txt)"
    wk -P Control_L -P Shift_L -k n -p Shift_L -p Control_L -s 1000 Neu -k Return; sleep 1
    check "wayland: Ctrl+Shift+N makes a folder named in place" '[ -d /tmp/t/Neu ] && [ ! -e "/tmp/t/New folder" ]'
    vclick 260 "$(wrow /tmp/t a.txt)"; wk -k F2 -s 800 alpha -k Return; sleep 1
    check "wayland: F2 renames in place" '[ -f /tmp/t/alpha.txt ] && [ ! -f /tmp/t/a.txt ]'
    ctrl z; sleep 1
    check "wayland: Ctrl+Z undoes it" '[ -f /tmp/t/a.txt ] && [ ! -f /tmp/t/alpha.txt ]'
    # F2 alone: the end of its wtype takes the keyboard away, which ends the editor; the next
    # wtype's keys must still reach the list.
    vclick 260 "$(wrow /tmp/t a.txt)"; wk -k F2; wk -k F2 -s 800 beta -k Return; sleep 1
    check "wayland: after the keyboard leaves during F2, the keyboard still reaches the list" '[ -f /tmp/t/beta.txt ]'
    [ -f /tmp/t/beta.txt ] && mv /tmp/t/beta.txt /tmp/t/a.txt; sleep 1
    # Batch rename: the three photos in Proj, Find/Replace typed into fields clicked first.
    vclick 260 "$(wrow /tmp/t Proj)"; wk -k Return; sleep 0.5
    vclick 260 "$(wrow /tmp/t/Proj photo1.txt)"; vclick 260 "$(wrow /tmp/t/Proj photo3.txt)"
    wk -P Shift_L -k Up -k Up -p Shift_L -k F2; sleep 1; wshot gui-wl-batch
    vclick 256 524; wk photo; vclick 256 560; wk img; wshot gui-wl-batch-typed
    vclick 1202 667; sleep 1.5
    check "wayland: batch Find/Replace renames on disk" '[ -f /tmp/t/Proj/img1.txt ] && [ -f /tmp/t/Proj/img3.txt ]'
    # The keyboard is back in the list once the layer closes (the next wtype is the window
    # getting the keyboard again): Home, F2 and a name rename the first entry, a.txt.
    wk -k Home -k F2 -s 800 q -k Return; sleep 1; wshot gui-wl-f2-after-batch
    check "wayland: F2 right after a batch (keyboard back in the list)" '[ -f /tmp/t/Proj/q.txt ]'
    [ -f /tmp/t/Proj/q.txt ] && { ctrl z; sleep 1; }
    ctrl z; sleep 1.5
    check "wayland: Ctrl+Z undoes the batch" '[ -f /tmp/t/Proj/photo2.txt ] && [ ! -e /tmp/t/Proj/img2.txt ]'
    vclick 260 "$(wrow /tmp/t/Proj photo3.txt)"
    wk -P Shift_L -k Up -k Up -p Shift_L -k F2; sleep 1; wshot gui-wl-batch-open; wk -k Escape; sleep 0.5; wshot gui-wl-batch-closed
    check "wayland: Esc closes the layer" '[ "$(px gui-wl-batch-closed 16 45)" != "$(px gui-wl-batch-open 16 45)" ]'

    # Archives by keyboard: the menu key opens Gezik's own menu on the selection; Down moves
    # onto its first item.
    kill $gezik 2>/dev/null; wait $gezik 2>/dev/null
    fresh_archives
    hide_7zip
    rm -rf /tmp/cfg/tools
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/a >>/tmp/gezik-gui-wl.log 2>&1 &
    gezik=$!
    sleep 3
    extract_here() { vclick 260 "$(wrow /tmp/a "$1")"; wk -k Menu -s 800 -k Down -k Down -k Down -k Return; sleep 2; }
    vclick 260 "$(wrow /tmp/a one.zip)"; wk -k Menu; sleep 0.5; wshot gui-wl-menu; wk -k Escape
    extract_here one.zip
    check "wayland: Extract here from the menu key" '[ "$(cat /tmp/a/one/inner/deep.txt 2>/dev/null)" = deep ]'
    rm -rf /tmp/a/one; sleep 1.5
    extract_here multi.zip
    check "wayland: Extract here, several roots: into multi/" '[ -f /tmp/a/multi/m2.txt ]'
    rm -rf /tmp/a/multi; sleep 1.5
    # The password goes in with the keys that open the dialog (see wk).
    vclick 260 "$(wrow /tmp/a aes.zip)"
    wk -k Menu -s 800 -k Down -k Down -k Down -k Return -s 1500 nope -s 500 -k Return -s 1500 secret -s 300 -k Return
    sleep 2; wshot gui-wl-password-done
    check "wayland: AES zip: wrong, then right password" '[ "$(cat /tmp/a/m1.txt 2>/dev/null)" = m1 ]'
    rm -f /tmp/a/m1.txt; sleep 1.5
    extract_here w.wim; sleep 1; wshot gui-wl-needs-7zip
    # Download is the dialog's default button.
    wk -k Return
    for _ in $(seq 1 60); do [ -f /tmp/a/w/m1.txt ] && break; sleep 1; done
    wshot gui-wl-after-download
    check "wayland: the wim after downloading 7-Zip" '[ -x /tmp/cfg/tools/7zip-26.03/7zz ] && [ -f /tmp/a/w/m1.txt ]'
    kill $gezik $sway 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-wl.log && fail "wayland: no panic" || pass "wayland: no panic"
}

# Gezik's own menus (Linux has no system ones for Slint) stay inside the window: flipped up
# or left at the window's edges, and scrolling when taller than it. X11, 900x600.
popups() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    # After the Wayland run: X11 again.
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/p /tmp/cfg /root/.local/share/Trash && mkdir -p /tmp/p /tmp/cfg
    printf 'Привет
' > /tmp/p/cyr.txt
    for i in $(seq -w 1 30); do echo "line $i" > /tmp/p/f$i.txt; done
    printf '[[commands]]
name = "Copy it"
run = ["cp", "{in}", "{out}"]
output = "{name}-copy.{ext}"
' > /tmp/cfg/settings.toml
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/p >/tmp/gezik-gui-popups.log 2>&1 &
    local gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    shot pop-start
    # Bottom right, on f17.txt (y 560): the menu ends at the pointer. Its last items are on
    # screen: "Move to Trash" is the second from the bottom (lines of 32, 6 of padding).
    local y; y=$(row /tmp/p f17.txt)
    rclick 820 "$y"; sleep 0.5; shot pop-menu-bottom-right
    check "popups: near the bottom right the menu opens up and left"         '[ "$(px pop-menu-bottom-right 700 $((y - 40)))" != "$(px pop-start 700 $((y - 40)))" ] && [ "$(px pop-menu-bottom-right 830 300)" = "$(px pop-start 830 300)" ]'
    click 700 $((y - 1 - 6 - 32 - 16)); sleep 1.5
    check "popups: its lower items can be chosen (Move to Trash)" '[ ! -f /tmp/p/f17.txt ] && [ -f ~/.local/share/Trash/files/f17.txt ]'
    # The Commands submenu (its 6th line) at the right edge opens to the left of the menu.
    local sub=$((180 + 1 + 6 + 32 * 5 + 16))
    rclick 820 180; sleep 0.5; xdotool mousemove 760 "$sub"; sleep 0.8
    shot pop-submenu-left
    check "popups: a submenu at the right edge opens to the left"         '[ "$(px pop-submenu-left 560 $sub)" != "$(px pop-start 560 $sub)" ]'
    key Escape Escape; sleep 0.5
    # The Presets menu of the rename layer, at the window's right edge: it ends at the
    # button's right edge.
    click 260 144; xdotool keydown shift; click 260 170; xdotool keyup shift; key F2; sleep 1
    click 820 51; sleep 0.8; shot pop-presets
    click 760 88; sleep 0.8; typ P1; key Return; sleep 1
    check "popups: its item can be chosen (Save current rules as…)" 'grep -q "P1" /tmp/cfg/settings.toml'
    click 820 51; sleep 0.8; shot pop-presets-saved; key Escape; sleep 0.3; key Escape; sleep 0.5
    # The encodings (40) are taller than the window: the list fits in it and scrolls.
    rclick 260 118; sleep 0.5; click 330 $((118 + 20 + 32 * 4)); sleep 1; shot pop-convert
    click 301 221; sleep 0.8; shot pop-encodings
    check "popups: a list taller than the window stays inside it"         '[ "$(px pop-encodings 300 8)" != "$(px pop-convert 300 8)" ] && [ "$(px pop-encodings 300 592)" != "$(px pop-convert 300 592)" ]'
    xdotool mousemove 300 400; for _ in 1 2 3 4 5 6 7 8 9 10; do xdotool click 5; sleep 0.1; done; sleep 0.5
    shot pop-encodings-scrolled
    check "popups: the wheel scrolls it" '! cmp -s "$SHOTS/pop-encodings.png" "$SHOTS/pop-encodings-scrolled.png"'
    # Scrolled to its end, the list ends at the window's bottom: EUC-KR at 573, Mac Cyrillic
    # four lines above.
    click 300 $((573 - 4 * 32)); sleep 0.5; shot pop-encoding-chosen
    key ctrl+Return; sleep 2
    cyrillic() {
        local f
        for f in $(find /tmp/p -name "cyr*"); do
            [ "$(iconv -f MACCYRILLIC -t UTF-8 "$f" 2>/dev/null)" = "Привет" ] && return 0
        done
        return 1
    }
    check "popups: an encoding at the end of the list is chosen (cyr.txt in Mac Cyrillic)" cyrillic
    kill $gezik 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-popups.log && fail "popups: no panic" || pass "popups: no panic"
}

# A drag that rests on a tab (the tab opens, the list under the drag is rebuilt) and then
# leaves the window: it must still drop on another Gezik window's folder. X11, two windows.
tabdrag() {
    Xvfb :99 -screen 0 1900x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/s /tmp/s2 /tmp/dst /tmp/cfg /tmp/cfg2 && mkdir -p /tmp/s /tmp/s2 /tmp/dst/in /tmp/cfg /tmp/cfg2
    echo d > /tmp/s/d.txt; echo f > /tmp/s/f.txt; echo e > /tmp/s2/e.txt
    GEZIK_CONFIG_DIR=/tmp/cfg2 $GEZIK /tmp/dst >/tmp/gezik-gui-tabdrag2.log 2>&1 &
    local pb=$!
    sleep 3
    local b; b=$(xdotool search --pid $pb 2>/dev/null | tail -1)
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/s >/tmp/gezik-gui-tabdrag.log 2>&1 &
    local pa=$!
    sleep 3
    local a; a=$(xdotool search --pid $pa 2>/dev/null | tail -1)
    xdotool windowmove "$b" 950 0; xdotool windowsize "$b" 900 600
    xdotool windowmove "$a" 0 0; xdotool windowsize "$a" 900 600; sleep 0.5
    # A second tab on /tmp/s2, then back to the first (/tmp/s: d.txt 118, f.txt 144).
    xdotool windowfocus --sync "$a"; xdotool key ctrl+t; sleep 1; xdotool key ctrl+l; sleep 0.3
    xdotool type --delay 50 /tmp/s2; xdotool key Return; sleep 1
    click 100 20; sleep 0.8
    shot tabdrag-start
    # d.txt: rest on the s2 tab, then out onto the other window's "in" folder (y 118).
    click 260 118; xdotool mousemove 260 118 mousedown 1
    for d in 5 10 20 40; do xdotool mousemove 262 $((118 - d)); sleep 0.1; done
    xdotool mousemove 330 20; sleep 0.3; xdotool mousemove 332 21; sleep 1.2
    shot tabdrag-rested
    for x in 500 700 880 940 1000 1100 1200; do xdotool mousemove $x 118; sleep 0.15; done
    xdotool mousemove 1210 118; sleep 0.6; shot tabdrag-over-other-window
    xdotool mouseup 1; sleep 2; shot tabdrag-dropped
    check "tabdrag: rest on a tab, then drop on a folder in another window"         '[ -f /tmp/dst/in/d.txt ] && [ ! -f /tmp/s/d.txt ]'
    # The window is not wedged: a plain drag afterwards (f.txt onto the other window's folder).
    click 100 20; sleep 0.8
    click 260 118; xdotool mousemove 260 118 mousedown 1
    for x in 270 300 500 880 940 1100 1200; do xdotool mousemove $x 118; sleep 0.15; done
    xdotool mousemove 1210 118; sleep 0.6; xdotool mouseup 1; sleep 2
    check "tabdrag: a plain drag afterwards still drops" '[ -f /tmp/dst/in/f.txt ]'
    kill $pa $pb 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-tabdrag.log /tmp/gezik-gui-tabdrag2.log && fail "tabdrag: no panic" || pass "tabdrag: no panic"
}

# 5d's menus through Gezik's own popups: the right-click "Images to PDF…" on pictures, and
# the Convert layer's preset menu with its PDF group. X11, 900x600.
pdfpopups() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    local PDF_PRESET_X=371 PDF_PRESET_Y=164
    rm -rf /tmp/q /tmp/cfg && mkdir -p /tmp/q /tmp/cfg
    convert -size 64x48 xc:red /tmp/q/a.png
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/q >/tmp/gezik-gui-pdf.log 2>&1 &
    local gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    shot pdf-start
    # On a.png (y 118) the menu's 6th line is "Images to PDF…" (after Convert…).
    rclick 260 118; sleep 0.5; shot pdf-menu
    click 330 $((118 + 20 + 32 * 5)); sleep 1.5; shot pdf-layer
    key ctrl+Return; sleep 2; shot pdf-made
    check "pdf popups: the right-click \"Images to PDF…\" makes a.pdf" '[ "$(head -c 5 /tmp/q/a.pdf 2>/dev/null)" = "%PDF-" ]'
    # The preset menu: Convert… (5th line), then the Preset button (371, 164). Image's six
    # choices, then the PDF group (a greyed heading, "Images to PDF").
    rm -f /tmp/q/a.pdf; sleep 1
    rclick 260 118; sleep 0.5; click 330 $((118 + 20 + 32 * 4)); sleep 1.5; shot pdf-convert
    click "$PDF_PRESET_X" "$PDF_PRESET_Y"; sleep 0.8; shot pdf-presets
    check "pdf popups: the preset menu opens" '! cmp -s "$SHOTS/pdf-convert.png" "$SHOTS/pdf-presets.png"'
    # Another preset first (Convert to JPEG, 266), then back to the PDF group's line. The
    # image layer is taller, so its Preset button is higher (105) and the line is at 399.
    click 300 266; sleep 0.8; shot pdf-jpeg
    click "$PDF_PRESET_X" 105; sleep 0.8; shot pdf-presets-again
    click 300 399; sleep 0.8; shot pdf-chosen
    check "pdf popups: the choice changes the layer" '! cmp -s "$SHOTS/pdf-jpeg.png" "$SHOTS/pdf-chosen.png"'
    key ctrl+Return; sleep 2; shot pdf-made-again
    check "pdf popups: \"Images to PDF\" chosen from the preset menu makes a.pdf" '[ "$(head -c 5 /tmp/q/a.pdf 2>/dev/null)" = "%PDF-" ]'
    kill $gezik 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-pdf.log && fail "pdf popups: no panic" || pass "pdf popups: no panic"
}

# "PDF to images" on a page too large for 300 dpi: the dpi is lowered and the job, done before
# its row came up, still leaves the row with the note. Needs the network (Gezik's pdfium build).
pdfnote() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/h /tmp/cfg && mkdir -p /tmp/h /tmp/cfg/tools/pdfium-8086
    curl -sSL -o /tmp/pdfium.7z \
        https://github.com/wenlar/gezik-tools/releases/download/pdfium-8086-1/pdfium-8086-linux-x64.7z
    $SEVEN x -y -o/tmp/cfg/tools/pdfium-8086 /tmp/pdfium.7z >/dev/null
    chmod +x /tmp/cfg/tools/pdfium-8086/libpdfium.so
    # One empty 14,400 pt page, as huge.pdf in the Windows run.
    python3 - <<'PY'
objs = [b"<< /Type /Catalog /Pages 2 0 R >>", b"<< /Type /Pages /Kids [4 0 R] /Count 1 >>",
        b"<< /Length 0 >>\nstream\n\nendstream",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 14400 14400] /Resources << >> /Contents 3 0 R >>"]
out, offs = bytearray(b"%PDF-1.7\n"), []
for i, o in enumerate(objs, 1):
    offs.append(len(out)); out += b"%d 0 obj\n" % i + o + b"\nendobj\n"
xref = len(out)
out += b"xref\n0 5\n0000000000 65535 f \n" + b"".join(b"%010d 00000 n \n" % o for o in offs)
out += b"trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % xref
open("/tmp/h/huge.pdf", "wb").write(out)
PY
    printf '[convert]\nlast-preset = "pdf:pdf-to-images"\npdf = "op=to-images dpi=300 image=jpeg"\n' >/tmp/cfg/state.toml
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/h >/tmp/gezik-gui-pdfnote.log 2>&1 &
    local gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    shot pdfnote-start
    # On huge.pdf (y 118) Convert… is the menu's 5th line; the layer opens on "PDF to images".
    rclick 260 118; sleep 0.5; shot pdfnote-menu
    click 330 $((118 + 20 + 32 * 4)); sleep 1.5; shot pdfnote-layer
    key ctrl+Return
    for _ in $(seq 60); do [ -f "/tmp/h/huge - page 1.jpg" ] && break; sleep 0.5; done
    # Past the row's "show after" and "done for" times: a row that stays has a note.
    sleep 5; shot pdfnote-done
    check "pdf note: the huge page is made" '[ -f "/tmp/h/huge - page 1.jpg" ]'
    check "pdf note: the picture is made at a lower dpi" '[ "$(identify -format %w "/tmp/h/huge - page 1.jpg")" -lt 60000 ]'
    # The panel's row is the strip above the status bar (y 545-570), empty without a row.
    check "pdf note: the panel keeps the job's row"         'awk "BEGIN { exit !($(dark pdfnote-done 860 24 10 545) > 0.01 && $(dark pdfnote-start 860 24 10 545) < 0.001) }"'
    click 823 557; sleep 1; shot pdfnote-details
    check "pdf note: Details opens" '! cmp -s "$SHOTS/pdfnote-done.png" "$SHOTS/pdfnote-details.png"'
    kill $gezik 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-pdfnote.log && fail "pdf note: no panic" || pass "pdf note: no panic"
}

# 6a's filter bar: Ctrl+F, typing narrows the list (only what shows is acted on), an error
# keeps the list, Esc closes it, the hidden files toggle and a tab switch keep it, a move to
# another folder (and back) drops it, and "This PC" says it has none. Then `/`, the typing
# mode ([keyboard] typing = "filter") and the saved filters of the ▾ menu: Save as… (a name
# already there, in any case, is replaced after asking), kept in settings.toml over a
# restart, chosen, deleted. X11, 900x600.
filter() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/f /tmp/cfg /root/.local/share/Trash && mkdir -p /tmp/f/sub /tmp/cfg
    for n in a.jpg b.txt c.JPG d.png e.txt .h.jpg; do echo $n > /tmp/f/$n; done
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/f >/tmp/gezik-gui-filter.log 2>&1 &
    local gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    title() { xdotool getwindowname "$(win)"; }
    trashed() { [ -f "$HOME/.local/share/Trash/files/$1" ]; }
    shot filter-start
    # Rows: sub 118, .h.jpg 144 (hidden files show on Linux), a.jpg 170 ...
    click 255 118; key ctrl+f; sleep 0.3; shot filter-open
    check "filter: Ctrl+F opens the bar (the rows move down)" '[ "$(px filter-open 255 122)" != "$(px filter-start 255 122)" ]'
    typ jpg; shot filter-jpg           # 3 / 7: .h.jpg, a.jpg, c.JPG
    key ctrl+h; sleep 1.5; shot filter-jpg-unhidden   # the reload keeps it: 2 / 6
    key Down; key ctrl+a Delete; sleep 1.5; shot filter-trashed
    check "filter: Ctrl+A and Delete trash only what the filter shows" \
        'trashed a.jpg && trashed c.JPG && [ -f /tmp/f/b.txt ] && [ -f /tmp/f/.h.jpg ] && [ -f /tmp/f/d.png ]'
    key ctrl+z; sleep 1.5
    check "filter: Ctrl+Z brings them back" '[ -f /tmp/f/a.jpg ] && [ -f /tmp/f/c.JPG ]'
    # A half-typed part: red line and text, the list stays as "jpg" showed it.
    key ctrl+f; sleep 0.3; key End; typ ';!'; shot filter-error
    key Escape; sleep 0.5; shot filter-closed
    check "filter: Esc in the field closes the bar" '[ "$(px filter-closed 255 122)" = "$(px filter-start 255 122)" ] || [ "$(px filter-closed 255 122)" != "$(px filter-open 255 122)" ]'
    # The list has the keyboard and shows everything again.
    key ctrl+a; sleep 0.3; shot filter-all-selected
    key Escape; sleep 0.3; shot filter-selection-cleared
    # Esc on the list: first the filter, then the selection.
    key ctrl+f; typ txt; key Down; key Escape; sleep 0.3; shot filter-esc-list
    key ctrl+a Delete; sleep 1.5
    check "filter: Esc on the list closes the filter (everything is acted on)" 'trashed a.jpg && trashed b.txt && trashed d.png'
    key ctrl+z; sleep 1.5
    # A tab switch keeps it.
    key ctrl+f; typ txt; sleep 0.3
    key ctrl+t; sleep 1; click 100 20; sleep 1; shot filter-tab-back
    key ctrl+f Down; key ctrl+a Delete; sleep 1.5
    check "filter: a tab switch keeps the filter" 'trashed b.txt && trashed e.txt && [ -f /tmp/f/a.jpg ]'
    key ctrl+z; sleep 1.5
    # Into a folder and back: no filter.
    key ctrl+f ctrl+a; typ sub; key Down Return; sleep 1; shot filter-in-sub
    check "filter: Enter opens the folder the filter shows" '[ "$(title)" = "sub — Gezik" ]'
    key alt+Left; sleep 1; shot filter-back
    key ctrl+a Delete; sleep 1.5
    check "filter: back in the folder, no filter" 'trashed a.jpg && trashed b.txt'
    key ctrl+z; sleep 1.5
    # "This PC": no filter there.
    key ctrl+l; typ /; key Return; sleep 1; key alt+Up; sleep 1.5
    key ctrl+f; sleep 0.5; shot filter-this-pc
    check "filter: no bar in This PC" '[ "$(px filter-this-pc 255 122)" != "$(px filter-open 255 122)" ]'

    # `/` opens it, whatever the typing mode. Rows (no bar): sub 118, then the files.
    key ctrl+l; typ /tmp/f; key Return; sleep 1.5; click 255 118; shot filter-slash-before
    key slash; sleep 0.3; typ txt; shot filter-slash
    check "filter: / opens the bar" '[ "$(px filter-slash 255 122)" != "$(px filter-slash-before 255 122)" ]'
    key Down; key ctrl+a Delete; sleep 1.5
    check "filter: / then txt: only the .txt files are acted on" \
        'trashed b.txt && trashed e.txt && [ -f /tmp/f/a.jpg ] && [ -f /tmp/f/d.png ]'
    key ctrl+z; sleep 1.5; key Escape; sleep 0.5
    # "jump" (the default): a letter goes to a name, no bar.
    typ c; sleep 0.5; shot filter-jump
    check "filter: in jump mode a letter opens no bar" '[ "$(px filter-jump 255 122)" = "$(px filter-slash-before 255 122)" ] || [ "$(px filter-jump 255 122)" != "$(px filter-slash 255 122)" ]'
    # typing = "filter": a letter opens the bar with it, the next ones go on in the field.
    printf '[keyboard]\ntyping = "filter"\n' >/tmp/cfg/settings.toml; sleep 2
    click 255 118; typ jp; sleep 0.5; shot filter-typed
    check "filter: in filter mode a letter opens the bar" '[ "$(px filter-typed 255 122)" != "$(px filter-slash-before 255 122)" ]'
    key Down; key ctrl+a Delete; sleep 1.5
    check "filter: the typed letters filter (jp)" 'trashed a.jpg && trashed c.JPG && [ -f /tmp/f/b.txt ] && [ -f /tmp/f/d.png ]'
    key ctrl+z; sleep 1.5
    # Save as… (the first item with nothing saved): the bar's text under a name.
    key ctrl+f ctrl+a; typ txt; sleep 0.3; shot filter-menu-before
    click 879 96; sleep 0.8; shot filter-menu-empty
    check "filter: the ▾ menu opens" '! cmp -s "$SHOTS/filter-menu-empty.png" "$SHOTS/filter-menu-before.png"'
    click 800 131; sleep 0.8; typ Resimler; key Return; sleep 1.5
    check "filter: Save as… writes [[filters]] to settings.toml" \
        'grep -q "^\[\[filters\]\]" /tmp/cfg/settings.toml && grep -q "name = \"Resimler\"" /tmp/cfg/settings.toml && grep -q "pattern = \"txt\"" /tmp/cfg/settings.toml'
    # The same name in another case: asked, Replace (the first button) replaces it.
    key ctrl+f ctrl+a; typ jpg; sleep 0.3
    click 879 96; sleep 0.8; shot filter-menu-one     # Resimler 131, Save as… 163, Delete 195
    click 800 163; sleep 0.8; typ resimler; key Return; sleep 0.8; shot filter-replace
    key Return; sleep 1.5
    check "filter: a name already there (any case) is replaced after asking" \
        '[ "$(grep -c "^\[\[filters\]\]" /tmp/cfg/settings.toml)" = 1 ] && grep -q "name = \"resimler\"" /tmp/cfg/settings.toml && grep -q "pattern = \"jpg\"" /tmp/cfg/settings.toml'
    # Kept over a restart; chosen from the menu, it fills the bar and the list keeps the keyboard.
    kill $gezik 2>/dev/null; wait $gezik 2>/dev/null
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/f >>/tmp/gezik-gui-filter.log 2>&1 &
    gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    click 255 118; key slash; sleep 0.3
    click 879 96; sleep 0.8; shot filter-menu-restart
    click 800 131; sleep 0.8; shot filter-chosen
    key ctrl+a Delete; sleep 1.5
    check "filter: the saved filter, chosen after a restart, filters (the list has the keyboard)" \
        'trashed a.jpg && trashed c.JPG && [ -f /tmp/f/b.txt ] && [ -f /tmp/f/d.png ]'
    key ctrl+z; sleep 1.5
    # Delete "resimler".
    click 879 96; sleep 0.8; click 800 195; sleep 1.5
    check "filter: Delete takes it out of settings.toml" \
        '! grep -q "^\[\[filters\]\]" /tmp/cfg/settings.toml && ! grep -q "resimler" /tmp/cfg/settings.toml'
    click 879 96; sleep 0.8; shot filter-menu-deleted
    check "filter: the menu no longer lists it" '! cmp -s "$SHOTS/filter-menu-deleted.png" "$SHOTS/filter-menu-restart.png"'
    key Escape; sleep 0.3
    # A new folder with the filter on: the filter closes, the name is edited in place.
    key ctrl+f ctrl+a; typ '*.jpg'; key Down; sleep 0.3; shot filter-before-new-folder
    key ctrl+shift+n; sleep 1.5; shot filter-new-folder
    typ Yeni; key Return; sleep 1
    check "filter: Ctrl+Shift+N closes the filter and names the folder in place"         '[ -d /tmp/f/Yeni ] && [ ! -e "/tmp/f/New folder" ]'
    check "filter: the bar is closed after it" '[ "$(px filter-new-folder 255 122)" != "$(px filter-before-new-folder 255 122)" ]'
    # A paste the filter hides: the filter stays, the status bar says so.
    echo x > /tmp/f/sub/x.txt
    key ctrl+l; typ /tmp/f/sub; key Return; sleep 1; click 255 118; key ctrl+c
    key alt+Left; sleep 1
    key ctrl+f; typ '*.jpg'; key Down ctrl+v; sleep 1.5; shot filter-paste-hidden
    check "filter: a paste the filter hides still lands (the note: see the shot)" '[ -f /tmp/f/x.txt ]'
    kill $gezik 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-filter.log && fail "filter: no panic" || pass "filter: no panic"
}

# 6a's selection tools: the pattern box (Ctrl+= and the keypad's -) with its live count,
# select and deselect, invert (Ctrl+Shift+I), the same type (Alt+keypad +), none of them while
# the path box or the filter bar has the keyboard, the selection back (keypad /) after a
# delete, a copy in the folder and a Ctrl+C here pasted elsewhere, and
# the last pattern kept in state.toml over a restart. What is selected is seen by Delete
# (what lands in the trash), then Ctrl+Z. X11, 900x600.
selection() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/s /tmp/cfg /root/.local/share/Trash && mkdir -p /tmp/s/other /tmp/s/sub /tmp/cfg
    for n in a.jpg b.JPG c.png d.txt e.txt; do echo $n > /tmp/s/$n; done
    echo z > /tmp/s/sub/z.txt
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/s >/tmp/gezik-gui-select.log 2>&1 &
    local gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    trashed() { [ -f "$HOME/.local/share/Trash/files/$1" ]; }
    here() { [ -f "/tmp/s/$1" ]; }
    undo() { key ctrl+z; sleep 1.5; }
    # The box, where the line under the field is.
    box() { convert "$SHOTS/$1.png" -crop 460x260+220+60 +repage "$SHOTS/$1-box.png"; }
    same_box() { box "$1"; box "$2"; cmp -s "$SHOTS/$1-box.png" "$SHOTS/$2-box.png"; }
    # Rows: other 118, sub 144, a.jpg 170, b.JPG 196, c.png 222, d.txt 248, e.txt 274.
    shot select-start

    # The box: the count follows the text; an error is said.
    click 255 170; key ctrl+equal; sleep 0.8; shot select-box-empty      # "7 items match"
    typ '*.jpg'; sleep 0.3; shot select-box-jpg                          # "2 items match"
    key ctrl+a; typ '*.gif'; sleep 0.3; shot select-box-none             # "No items match"
    key ctrl+a; typ '!'; sleep 0.3; shot select-box-error                # red: Type a name after "!"
    check "select: the box opens with a count" '! cmp -s "$SHOTS/select-box-empty.png" "$SHOTS/select-start.png"'
    check "select: the count follows the text" '! same_box select-box-empty select-box-jpg && ! same_box select-box-jpg select-box-none'
    check "select: a bad pattern is said under the field" '! same_box select-box-none select-box-error'
    key ctrl+a; typ '*.jpg'; key Return; sleep 0.5; shot select-jpg
    key Delete; sleep 1.5
    check "select: Select adds the matching names (both cases)" \
        'trashed a.jpg && trashed b.JPG && here c.png && here d.txt && here e.txt'
    undo
    # Keypad /: the selection before the delete comes back.
    click 255 248; key KP_Divide; sleep 0.5; shot select-restored-delete
    key Delete; sleep 1.5
    check "select: keypad / brings back the selection of the last delete" \
        'trashed a.jpg && trashed b.JPG && here d.txt'
    undo

    # Deselect: the keypad's -, the box starts with the last pattern.
    click 255 222; key ctrl+a KP_Subtract; sleep 0.8; shot select-deselect-box
    key Return; sleep 0.5
    key Delete; sleep 1.5
    check "select: Deselect (keypad -, the last pattern) leaves out the matches" \
        'here a.jpg && here b.JPG && trashed c.png && trashed d.txt && trashed e.txt'
    undo

    # Invert.
    click 255 222; key ctrl+shift+i; sleep 0.5; shot select-inverted
    key Delete; sleep 1.5
    check "select: Ctrl+Shift+I selects all but what was selected" \
        'here c.png && trashed a.jpg && trashed b.JPG && trashed d.txt && trashed e.txt'
    undo
    # Only the folders again (the inverted delete restored them): rows as at the start.

    # The same type.
    click 255 170; key alt+KP_Add; sleep 0.5; shot select-same-type
    key Delete; sleep 1.5
    check "select: Alt+keypad + adds the focused entry's type" \
        'trashed a.jpg && trashed b.JPG && here c.png && here d.txt'
    undo
    # A text field with the keyboard keeps Alt+keypad + (the path box, the filter bar: "jpg;"
    # with or without a "+" typed after it shows the same).
    click 255 248; key ctrl+l; sleep 0.3; key alt+KP_Add; sleep 0.3; key Escape; sleep 0.3
    key Delete; sleep 1.5
    check "select: Alt+keypad + does nothing while the path box has the keyboard"         'trashed d.txt && here e.txt'
    undo
    click 255 170; key ctrl+f; sleep 0.3; typ 'jpg;'; key alt+KP_Add; sleep 0.3; shot select-same-type-filter
    key Down Delete; sleep 1.5
    check "select: Alt+keypad + does nothing while the filter bar has the keyboard"         'trashed a.jpg && here b.JPG'
    undo; key Escape; sleep 0.5

    # The other selection keys wait too: Ctrl+Shift+I from the path box and the filter bar.
    click 255 248; key ctrl+l; sleep 0.3; key ctrl+shift+i; sleep 0.3; key Escape; sleep 0.3
    key Delete; sleep 1.5
    check "select: Ctrl+Shift+I does nothing while the path box has the keyboard"         'trashed d.txt && here e.txt && here a.jpg'
    undo
    click 255 170; key ctrl+f; sleep 0.3; typ 'jpg;'; key ctrl+shift+i; sleep 0.3
    key Down Delete; sleep 1.5
    check "select: Ctrl+Shift+I does nothing while the filter bar has the keyboard"         'trashed a.jpg && here b.JPG'
    undo; key Escape; sleep 0.5

    # Ctrl+C here, Ctrl+V in sub with z.txt selected there (the paste must not remember sub's
    # selection; then Ctrl+Z there, which must not count either), back, the selection cleared,
    # keypad /: this folder's selection at the copy.
    click 255 170; xdotool keydown ctrl; click 255 222; xdotool keyup ctrl
    key ctrl+c; dclick 255 144; sleep 0.5; click 255 118; key ctrl+v; sleep 1.5
    check "select: the copy into sub lands" '[ -f /tmp/s/sub/a.jpg ] && [ -f /tmp/s/sub/c.png ]'
    key ctrl+z; sleep 1.5
    check "select: Ctrl+Z takes it back" '[ ! -e /tmp/s/sub/a.jpg ] && [ ! -e /tmp/s/sub/c.png ]'
    key alt+Left; sleep 1; key Escape KP_Divide; sleep 0.5; shot select-restored-cross
    key Delete; sleep 1.5
    check "select: keypad / brings back the selection copied from here and pasted elsewhere"         'trashed a.jpg && trashed c.png && here b.JPG && here d.txt && here e.txt'
    undo

    # After a copy: pasted into the same folder (the copies are new names), the selection
    # cleared, keypad /.
    click 255 170; xdotool keydown ctrl; click 255 222; xdotool keyup ctrl
    key ctrl+c ctrl+v; sleep 1.5
    check "select: the copy lands" '[ "$(ls /tmp/s | wc -l)" = 9 ]'
    key Escape KP_Divide; sleep 0.5; shot select-restored-copy
    key Delete; sleep 1.5
    check "select: keypad / brings back the selection of the last copy"         'trashed a.jpg && trashed c.png && here e.txt && here b.JPG && [ "$(ls /tmp/s | wc -l)" = 7 ]'
    undo
    for f in /tmp/s/*; do case "$f" in /tmp/s/[a-e].*|/tmp/s/b.JPG|/tmp/s/other|/tmp/s/sub) ;; *) rm -rf "$f" ;; esac; done
    sleep 1

    # The last pattern, over a restart.
    click 255 222; key Escape ctrl+equal; sleep 0.8; typ '*.txt;!e*'; key Return; sleep 2
    kill $gezik 2>/dev/null; wait $gezik 2>/dev/null
    check "select: state.toml keeps the last pattern" 'grep -q "last-pattern = \"\*.txt;!e\*\"" /tmp/cfg/state.toml'
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/s >>/tmp/gezik-gui-select.log 2>&1 &
    gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    click 255 222; key Escape KP_Add; sleep 0.8; shot select-box-restart
    key Return; sleep 0.5; key Delete; sleep 1.5
    check "select: after a restart the box starts with the last pattern" \
        'trashed d.txt && here e.txt && here c.png && here a.jpg'
    kill $gezik 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-select.log && fail "select: no panic" || pass "select: no panic"
}

# 6a's tabs: Ctrl+1..9, Ctrl+Shift+T (in its old place, with its history and its filter), the
# lock (the tab's menu, its icon, Ctrl+W and middle-click say no, "Close other tabs" keeps it
# and says so) and the tab picker (Ctrl+Shift+A: typing filters, Up/Down, Enter, Esc).
# X11, 900x600: three tabs 220 px wide (centres 110, 330, 550), the bar at y 20.
tabs() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/tb /tmp/cfg /root/.local/share/Trash && mkdir -p /tmp/tb/one /tmp/tb/two/sub /tmp/tb/three /tmp/cfg
    echo 1 > /tmp/tb/one/1.txt; echo 3 > /tmp/tb/three/3.txt
    echo x > /tmp/tb/two/sub/x1.txt; echo y > /tmp/tb/two/sub/y1.txt
    GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/tb/one >/tmp/gezik-gui-tabs.log 2>&1 &
    local gezik=$!
    sleep 3
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    title() { xdotool getwindowname "$(win)"; }
    is() { [ "$(title)" = "$1 — Gezik" ]; }
    trashed() { [ -f "$HOME/.local/share/Trash/files/$1" ]; }
    # A part of a screenshot, to compare: crop NAME W H X Y.
    crop() { convert "$SHOTS/$1.png" -crop "$2x$3+$4+$5" +repage "$SHOTS/$1-part.png"; }
    same_part() { crop "$1" $3 $4 $5 $6; crop "$2" $3 $4 $5 $6; cmp -s "$SHOTS/$1-part.png" "$SHOTS/$2-part.png"; }
    goto() { key ctrl+l; typ "$1"; key Return; sleep 1; }
    click 255 300
    key ctrl+t; sleep 1; goto /tmp/tb/two
    key ctrl+t; sleep 1; goto /tmp/tb/three
    shot tabs-three

    # Ctrl+1..9.
    key ctrl+1; sleep 0.8; check "tabs: Ctrl+1 shows the first tab" 'is one'
    key ctrl+2; sleep 0.8; check "tabs: Ctrl+2 shows the second" 'is two'
    key ctrl+9; sleep 0.8; check "tabs: Ctrl+9 shows the last" 'is three'
    key ctrl+1; sleep 0.8; key ctrl+5; sleep 0.8
    check "tabs: Ctrl+5 with three tabs does nothing" 'is one'

    # The picker: typing filters, Enter switches; Up/Down move; Esc closes.
    key ctrl+shift+a; sleep 0.8; shot tabs-picker
    check "tabs: Ctrl+Shift+A opens the picker" '! same_part tabs-picker tabs-three 480 300 210 60'
    typ thr; sleep 0.5; shot tabs-picker-thr
    check "tabs: typing filters the picker" '! same_part tabs-picker-thr tabs-picker 480 300 210 60'
    key Return; sleep 1; shot tabs-picked
    check "tabs: Enter switches to the tab typed" 'is three'
    check "tabs: the picker closes" '! same_part tabs-picked tabs-picker 480 300 210 60'
    key ctrl+shift+a; sleep 0.8; key Up; key Return; sleep 1
    check "tabs: Up then Enter picks the tab before" 'is two'
    key ctrl+shift+a; sleep 0.8; key Down; key Escape; sleep 0.8; shot tabs-picker-esc
    check "tabs: Esc closes the picker and switches nowhere" 'is two'
    check "tabs: the picker is gone after Esc" '! same_part tabs-picker-esc tabs-picker 480 300 210 60'
    key ctrl+shift+a; sleep 0.8; typ /tmp/tb/one; sleep 0.3; shot tabs-picker-path; key Return; sleep 1
    check "tabs: the picker matches paths" 'is one'
    key ctrl+shift+a; sleep 0.8; click 450 218; sleep 1
    check "tabs: a click on a row picks it" 'is two'
    # An exclusion holds for the path as well as the title: "two" and "three" have no
    # "\tmp\tb\t" in their titles, but in their paths.
    key ctrl+shift+a; sleep 0.8; typ '!/tmp/tb/t'; sleep 0.3; key Return; sleep 1
    check "tabs: an exclusion in the picker hides by path too" 'is one'
    # Enter with no rows keeps the picker open; Esc then closes it.
    key ctrl+shift+a; sleep 0.8; typ zzz; sleep 0.3; key Return; sleep 0.5; shot tabs-picker-none
    key Escape; sleep 0.5; shot tabs-picker-none-closed
    check "tabs: Enter with no rows keeps the picker open" \
        '! same_part tabs-picker-none tabs-picker-none-closed 480 300 210 60 && is one'
    key ctrl+2; sleep 0.8

    # Ctrl+Shift+T: tab two, into sub, filtered by x1, closed; back in its place with all that.
    dclick 255 118; sleep 1
    check "tabs: into sub" 'is sub'
    key ctrl+f; typ x1; key Down; sleep 0.3; shot tabs-sub-filtered
    key ctrl+w; sleep 1
    check "tabs: Ctrl+W closed it (the next tab shows)" 'is three'
    key ctrl+shift+t; sleep 1.5; shot tabs-reopened
    check "tabs: Ctrl+Shift+T brings it back" 'is sub'
    key ctrl+1; sleep 0.8; key ctrl+2; sleep 1
    check "tabs: in its old place" 'is sub'
    key ctrl+a Delete; sleep 1.5
    check "tabs: with its filter (only x1.txt went)" 'trashed x1.txt && [ -f /tmp/tb/two/sub/y1.txt ]'
    key ctrl+z; sleep 1.5
    key alt+Left; sleep 1
    check "tabs: with its history (Back goes to two)" 'is two'
    key ctrl+shift+t; sleep 1
    check "tabs: nothing more to reopen: nothing happens" 'is two'

    # The lock, from the tab's menu: Duplicate, Lock tab, Close, Close other tabs.
    key ctrl+1; sleep 0.8; shot tabs-unlocked
    rclick 110 20; sleep 0.5; shot tabs-menu-unlocked
    click 180 72; sleep 0.5; click 450 300; sleep 0.3; shot tabs-locked
    check "tabs: a lock shows on the locked tab" '! same_part tabs-locked tabs-unlocked 220 36 0 2'
    key ctrl+w; sleep 1; shot tabs-locked-ctrlw
    check "tabs: Ctrl+W leaves a locked tab open" 'is one'
    click 110 20 2; sleep 1
    key ctrl+3; sleep 0.8
    check "tabs: middle-click leaves it open too (still three tabs)" 'is three'
    key ctrl+1; sleep 0.8
    rclick 110 20; sleep 0.5; shot tabs-menu-locked
    check "tabs: the locked tab's menu differs (Unlock tab, no Close)" '! same_part tabs-menu-locked tabs-menu-unlocked 300 200 100 30'
    key Escape; sleep 0.5

    # Close other tabs on the second tab (not the active one): the locked first tab stays.
    rclick 330 20; sleep 0.5; shot tabs-menu-second
    click 400 136; sleep 2; shot tabs-closed-others
    key ctrl+1; sleep 0.8; check "tabs: the locked tab stayed" 'is one'
    key ctrl+3; sleep 0.8; check "tabs: only two tabs are left" 'is one'
    key ctrl+2; sleep 1.5; shot tabs-second-reloaded
    check "tabs: the second tab is the one kept" 'is two'
    check "tabs: the status bar said a locked tab stayed (see the shot)" \
        '! same_part tabs-closed-others tabs-second-reloaded 450 30 0 565'

    # Unlocked again: it closes.
    key ctrl+1; sleep 0.8; rclick 110 20; sleep 0.5; click 180 72; sleep 0.5
    key ctrl+w; sleep 1
    check "tabs: unlocked, Ctrl+W closes it" 'is two'
    kill $gezik 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-tabs.log && fail "tabs: no panic" || pass "tabs: no panic"
}

# 6a end to end, by keys only: the filter (Ctrl+F, `/`, Esc on the list and in the field,
# Turkish İ), the pattern box (Ctrl+=, keypad + and -, Ctrl+Shift+I), the selection back
# (keypad /), tabs by number, the view keys (Ctrl+Shift+1/2), reopening a closed tab with its
# history, a lock bound in settings.toml, the tab picker and `typing = "filter"`. What is
# selected is read from the clipboard after Ctrl+C (the names, in any order). X11, 900x600.
keyboard() {
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    rm -rf /tmp/k /tmp/cfg /root/.local/share/Trash && mkdir -p /tmp/k/Docs /tmp/cfg
    for n in a.jpg b.JPG c.png d.txt İSTANBUL.txt; do echo "$n" > "/tmp/k/$n"; done
    # Something in Docs, so that its grid differs from its list.
    for n in 1 2 3; do echo $n > /tmp/k/Docs/note$n.txt; done
    printf '[shortcuts]\ntoggle-tab-lock = "ctrl+shift+l"\n' >/tmp/cfg/settings.toml
    start_k() {
        GEZIK_CONFIG_DIR=/tmp/cfg $GEZIK /tmp/k >>/tmp/gezik-gui-keyboard.log 2>&1 &
        gezik=$!
        sleep 3
        xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 0.5
    }
    : >/tmp/gezik-gui-keyboard.log
    local gezik
    start_k
    title() { xdotool getwindowname "$(win)"; }
    is() { [ "$(title)" = "$1 — Gezik" ]; }
    # The names Ctrl+C put on the clipboard, sorted, on one line.
    copied() {
        xclip -selection clipboard -t x-special/gnome-copied-files -o 2>/dev/null | tail -n +2 | python3 -c '
import sys, os, urllib.parse
names = [os.path.basename(urllib.parse.unquote(l.strip()[len("file://"):])) for l in sys.stdin if l.strip()]
print(" ".join(sorted(names)))'
    }
    sorted() { printf '%s\n' "$@" | LC_ALL=C sort | paste -sd' '; }
    copies() { key ctrl+c; sleep 0.4; [ "$(copied)" = "$(sorted "$@")" ]; }
    # Rows (folders first): Docs 118, a.jpg 144, b.JPG 170, c.png 196, d.txt 222, İSTANBUL.txt 248.
    click 255 144

    # The filter.
    key ctrl+f; typ jpg; key Down ctrl+a
    check "keyboard: filter: only the shown items are selected" 'copies a.jpg b.JPG'
    key Escape; sleep 0.5; key ctrl+a
    check "keyboard: Esc on the list closes the filter (all six)" \
        'copies Docs a.jpg b.JPG c.png d.txt İSTANBUL.txt'
    key slash; typ istanbul; key Down ctrl+a
    check "keyboard: / and istanbul show İSTANBUL.txt" 'copies İSTANBUL.txt'
    key ctrl+f; sleep 0.3; key Escape; sleep 0.5; key ctrl+a
    check "keyboard: Esc in the field closes it" 'copies Docs a.jpg b.JPG c.png d.txt İSTANBUL.txt'

    # The pattern box, invert.
    key Escape ctrl+equal; sleep 0.8; typ '*.png;*.txt'; key Return; sleep 0.5
    check "keyboard: Ctrl+= selects by pattern" 'copies c.png d.txt İSTANBUL.txt'
    key ctrl+shift+i; sleep 0.3
    check "keyboard: Ctrl+Shift+I inverts" 'copies Docs a.jpg b.JPG'
    key Escape; sleep 0.3; shot keyboard-no-box
    key KP_Add; sleep 0.8; shot keyboard-box
    check "keyboard: keypad + opens the box" '! cmp -s "$SHOTS/keyboard-box.png" "$SHOTS/keyboard-no-box.png"'
    key ctrl+a; typ 'd*'; key Return; sleep 0.5
    check "keyboard: keypad + selects d*" 'copies Docs d.txt'
    key KP_Subtract; sleep 0.8; key ctrl+a; typ '*.txt'; key Return; sleep 0.5
    check "keyboard: keypad - deselects *.txt" 'copies Docs'

    # The selection back after a delete.
    click 255 222; key Delete; sleep 1.5; key ctrl+z; sleep 2
    key KP_Divide; sleep 0.5
    check "keyboard: keypad / brings back the selection of the delete" 'copies d.txt'

    # Tabs by number, the view keys.
    key ctrl+t; sleep 1; key ctrl+l; typ /tmp/k/Docs; key Return; sleep 1
    key ctrl+1; sleep 0.8; check "keyboard: Ctrl+1 shows the first tab" 'is k'
    key ctrl+2; sleep 0.8; check "keyboard: Ctrl+2 shows the second" 'is Docs'
    key ctrl+1 ctrl+9; sleep 0.8; check "keyboard: Ctrl+9 shows the last" 'is Docs'
    click 255 300; shot keyboard-list
    key ctrl+shift+2; sleep 0.8; shot keyboard-grid
    check "keyboard: Ctrl+Shift+2 shows the grid" '! cmp -s "$SHOTS/keyboard-grid.png" "$SHOTS/keyboard-list.png"'
    key ctrl+shift+1; sleep 0.8; shot keyboard-list-again
    check "keyboard: Ctrl+Shift+1 the list again" '! cmp -s "$SHOTS/keyboard-list-again.png" "$SHOTS/keyboard-grid.png"'

    # Close and reopen, with the history.
    key ctrl+w; sleep 1; check "keyboard: Ctrl+W closes the Docs tab" 'is k'
    key ctrl+shift+t; sleep 1.5; check "keyboard: Ctrl+Shift+T reopens it" 'is Docs'
    key alt+Left; sleep 1; check "keyboard: with its history (Back leaves Docs)" '! is Docs'
    key alt+Right; sleep 1

    # The lock, bound in settings.toml.
    key ctrl+shift+l; sleep 0.5; key ctrl+w; sleep 1
    check "keyboard: locked tab stays" 'is Docs'
    key ctrl+shift+l; sleep 0.5

    # The tab picker.
    key ctrl+1; sleep 0.8; key ctrl+shift+a; sleep 0.8; typ docs; key Return; sleep 1
    check "keyboard: the tab picker goes to Docs" 'is Docs'

    # typing = "filter".
    kill $gezik 2>/dev/null; wait $gezik 2>/dev/null
    printf '[shortcuts]\ntoggle-tab-lock = "ctrl+shift+l"\n\n[keyboard]\ntyping = "filter"\n' >/tmp/cfg/settings.toml
    rm -f /tmp/cfg/state.toml
    start_k
    click 255 144; typ c.; key Down ctrl+a
    check "keyboard: typing = filter: c. filters" 'copies c.png'
    kill $gezik 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-gui-keyboard.log && fail "keyboard: no panic" || pass "keyboard: no panic"
}

# Not in `all`. The filter in a folder of 100,000 files, release build: the process's CPU
# time (all threads, /proc/<pid>/task/*/schedstat) from before each keystroke to 300 ms after
# it, less the idle CPU of 300 ms with the bar open. Twice: nothing selected, then after
# Ctrl+A. Esc (closing the bar) too. X11, 900x600.
filterperf() {
    cargo build --release -p gezik 2>&1 | tail -1
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    local dir=/tmp/gezik-stress-100000
    if [ "$(ls $dir 2>/dev/null | wc -l)" != 100000 ]; then
        rm -rf $dir && mkdir -p $dir && (cd $dir && seq 0 99999 | sed 's/.*/file_&.txt/' | xargs touch)
    fi
    rm -rf /tmp/cfg && mkdir -p /tmp/cfg
    GEZIK_CONFIG_DIR=/tmp/cfg /target/release/gezik $dir >/tmp/gezik-filterperf.log 2>&1 &
    local gezik=$!
    sleep 6
    xdotool windowmove "$(win)" 0 0; xdotool windowsize "$(win)" 900 600; sleep 1
    cpu() { cat /proc/$gezik/task/*/schedstat 2>/dev/null | awk '{s += $1} END {printf "%d", s / 1000}'; }   # µs
    # The CPU of one key (xdotool key NAME) in ms, from before it to 300 ms after it.
    one() { local c0 c1; c0=$(cpu); xdotool key "$1"; sleep 0.3; c1=$(cpu); echo $(( (c1 - c0) / 1000 )); }
    stats() { sort -n | awk '{a[NR] = $1} END {printf "worst %d ms, median %d ms (%d)", a[NR], a[int((NR + 1) / 2)], NR}'; }
    idle() { for _ in 1 2 3 4 5; do local c0 c1; c0=$(cpu); sleep 0.3; c1=$(cpu); echo $(( (c1 - c0) / 1000 )); done | stats; }
    typed() { for k in f i l e underscore 1 2 3 4; do one $k; done | stats; }
    xdotool windowfocus --sync "$(win)"; click 255 118
    key ctrl+f; sleep 1
    echo "idle with the bar open: $(idle)"
    echo "file_1234, nothing selected: $(typed)"; shot filterperf-typed
    echo "Esc, nothing selected: $(one Escape) ms"
    sleep 1; key Escape ctrl+a; sleep 1; key ctrl+f; sleep 1
    echo "file_1234 after Ctrl+A on 100,000: $(typed)"; shot filterperf-typed-all
    echo "Esc after Ctrl+A: $(one Escape) ms"
    kill $gezik 2>/dev/null
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
    grep -i "panicked" /tmp/gezik-filterperf.log && fail "filterperf: no panic" || pass "filterperf: no panic"
}

# Not in `all`. Idle memory of a release Gezik (EXE, default the branch's) on its home folder
# with an empty config, 900x600, 3 s after it opened: RSS, its anonymous part and the private
# (unshared) memory, the Linux side of Task Manager's figure. Five runs.
memory() {
    local exe=${1:-/target/release/gezik}
    [ -x "$exe" ] || cargo build --release -p gezik 2>&1 | tail -1
    Xvfb :99 -screen 0 1600x900x24 >/dev/null 2>&1 &
    local xvfb=$!
    unset WAYLAND_DISPLAY
    export DISPLAY=:99
    sleep 1
    . /src/scripts/linux/gui-lib.sh
    for run in 1 2 3 4 5; do
        rm -rf /tmp/cfg && mkdir -p /tmp/cfg
        GEZIK_CONFIG_DIR=/tmp/cfg "$exe" >/dev/null 2>&1 &
        local pid=$!
        sleep 2; xdotool windowsize "$(win)" 900 600; sleep 3
        printf 'run %s: ' $run
        awk '/^VmRSS|^RssAnon/ {printf "%s %.1f MiB, ", $1, $2 / 1024}' /proc/$pid/status
        # AnonHugePages: with transparent huge pages on "always", a fresh mapping (a thread's
        # arena) may be backed by a whole 2 MiB page now and then.
        awk '/^Private/ {p += $2} /^AnonHugePages/ {h = $2} END {printf "Private %.1f MiB, AnonHugePages %.1f MiB\n", p / 1024, h / 1024}' /proc/$pid/smaps_rollup
        kill $pid; wait $pid 2>/dev/null
    done
    kill $xvfb 2>/dev/null
    wait 2>/dev/null
}

setup_7zip
case "${1:-all}" in
    x11) x11 ;;
    wayland) wayland ;;
    popups) popups ;;
    tabdrag) tabdrag ;;
    pdf) pdfpopups; pdfnote ;;
    pdfnote) pdfnote ;;
    filter) filter ;;
    select) selection ;;
    tabs) tabs ;;
    keyboard) keyboard ;;
    filterperf) filterperf ;;
    memory) memory "${2:-}" ;;
    *) x11; wayland; popups; tabdrag; pdfpopups; pdfnote; filter; selection; tabs; keyboard ;;
esac
echo "failures: $failures"
exit $failures
