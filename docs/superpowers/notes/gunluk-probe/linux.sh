#!/usr/bin/env bash
# Runs the Linux probes in the gezik-linux image:
#   docker run --rm -v D:/Work/gezik:/src:ro -v <probe7>:/probe:ro -v gezik-cargo:/usr/local/cargo/registry \
#       -v probe7-target:/target gezik-linux bash /probe/linux.sh
set -u
rm -rf /tmp/p && mkdir /tmp/p && cp -r /probe/src /probe/Cargo.toml /probe/Cargo.lock /tmp/p/
sed -i 's#D:/Work/gezik#/src#' /tmp/p/Cargo.toml
cd /tmp/p && CARGO_TARGET_DIR=/target cargo build --release --bins 2>&1 | grep -E "^(error|warning: unused)|Finished" ; B=/target/release
$B/clip /tmp/fake4k.png && ls -l /tmp/fake4k.png
printf 'Merhaba ğüşİı 日本 🙂\nikinci satır' > /tmp/text.txt

echo "=== X11 (Xvfb + xclip)"
Xvfb :99 -screen 0 1280x800x24 >/dev/null 2>&1 & XVFB=$!; export DISPLAY=:99; sleep 1
xclip -selection clipboard -t image/png -i /tmp/fake4k.png; sleep 0.3
$B/clip_x11
xclip -selection clipboard -i /tmp/text.txt; sleep 0.3
$B/clip_x11
echo "--- a 4K real-content PNG (ImageMagick 'rose' tiled + noise)"
convert -size 3840x2160 plasma:fractal -blur 0x1 /tmp/plasma.png 2>/dev/null; ls -l /tmp/plasma.png
xclip -selection clipboard -t image/png -i /tmp/plasma.png; sleep 0.3
$B/clip_x11
kill $XVFB; unset DISPLAY

echo "=== Wayland (headless sway + wl-copy)"
export XDG_RUNTIME_DIR=/tmp/xdg && mkdir -p $XDG_RUNTIME_DIR && chmod 700 $XDG_RUNTIME_DIR
WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman sway -c /dev/null >/tmp/sway.log 2>&1 & SWAY=$!; sleep 2
export WAYLAND_DISPLAY=$(ls $XDG_RUNTIME_DIR | grep -m1 '^wayland-[0-9]*$')
wl-copy -t image/png < /tmp/fake4k.png; sleep 0.3
$B/clip_wayland
wl-copy -t image/png < /tmp/plasma.png; sleep 0.3
$B/clip_wayland
wl-copy < /tmp/text.txt; sleep 0.3
$B/clip_wayland

echo "=== Terminal"
ls -l /usr/bin/x-terminal-emulator /etc/alternatives/x-terminal-emulator
mkdir -p "/tmp/t/boşluk ş 日本" "/tmp/t/[köşeli] it's"
printf '#!/bin/sh\npwd > /tmp/term-pwd.txt\n' > /tmp/pwdsh; chmod +x /tmp/pwdsh
for d in "/tmp/t/boşluk ş 日本" "/tmp/t/[köşeli] it's"; do
  rm -f /tmp/term-pwd.txt
  SHELL=/tmp/pwdsh $B/term "$d"
  sleep 1; echo "  pwd seen by the shell: $(cat /tmp/term-pwd.txt 2>/dev/null)"
done
echo "--- TERMINAL=foot / TERMINAL='foot --app-id x' / TERMINAL=nonexistent / empty PATH"
TERMINAL=foot $B/term --plan /tmp
TERMINAL="foot --app-id x" $B/term --plan /tmp
TERMINAL=nonexistent $B/term --plan /tmp
PATH=/nonexistent /target/release/term --plan /tmp
kill $SWAY
echo "=== attrs"
$B/attrs /usr/lib /tmp
