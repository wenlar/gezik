#!/usr/bin/env bash
# Drives Gezik's GUI on Linux through file operations, batch rename and archives, with real
# input: xdotool (XTEST) under Xvfb, wtype and the vpointer example under a headless sway.
#   docker build -t gezik-linux scripts/linux
#   docker run --rm -v "$PWD:/src" -v gezik-target:/target -v gezik-cargo:/usr/local/cargo/registry \
#       -e CARGO_TARGET_DIR=/target gezik-linux bash scripts/linux/gui.sh [x11|wayland|popups|tabdrag|all]
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

    # B. Batch rename in Proj: a.txt, b.txt, photo1-3.txt.
    dclick 255 170; sleep 0.5
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

setup_7zip
case "${1:-all}" in
    x11) x11 ;;
    wayland) wayland ;;
    popups) popups ;;
    tabdrag) tabdrag ;;
    *) x11; wayland; popups; tabdrag ;;
esac
echo "failures: $failures"
exit $failures
