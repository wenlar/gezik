#!/usr/bin/env bash
# Installs many terminals in a throwaway gezik-linux container and checks that `term`'s
# working-folder flag reaches each one's shell (the shell writes `pwd` and exits).
set -u
export DEBIAN_FRONTEND=noninteractive
apt-get update >/dev/null 2>&1
for p in xterm xfce4-terminal mate-terminal lxterminal tilix terminator alacritty kitty konsole gnome-terminal \
         qterminal terminology rxvt-unicode gnome-console ptyxis libgl1-mesa-dri dbus-x11; do
  apt-get install -y --no-install-recommends $p >/dev/null 2>&1 && echo "installed $p" || echo "NOT installed $p"
done
B=/target/release
mkdir -p "/tmp/t/boşluk ş 日本"
D="/tmp/t/boşluk ş 日本"
printf '#!/bin/sh\npwd > /tmp/term-pwd.txt\nexit 0\n' > /tmp/pwdsh; chmod +x /tmp/pwdsh
echo /tmp/pwdsh >> /etc/shells; usermod -s /tmp/pwdsh root
Xvfb :99 -screen 0 1280x800x24 >/dev/null 2>&1 & export DISPLAY=:99; sleep 1
export SHELL=/tmp/pwdsh LANG=C.UTF-8
run() {
  local t=$1
  rm -f /tmp/term-pwd.txt
  cd /   # Gezik's own cwd is elsewhere: the flag (or current_dir) must do it
  local cmd; cmd=$(TERMINAL=$t $B/term --plan "$D" 2>&1 | head -1)
  ( TERMINAL=$t timeout 10 $B/term "$D" >/dev/null 2>&1 & )
  for i in $(seq 1 40); do [ -s /tmp/term-pwd.txt ] && break; sleep 0.25; done
  local got; got=$(cat /tmp/term-pwd.txt 2>/dev/null)
  if [ "$got" = "$D" ]; then echo "ok   $t  :: $cmd"; else echo "FAIL $t (got '$got') :: $cmd"; fi
  pkill -f "$t" 2>/dev/null; sleep 0.3
}
# The flag alone (current_dir set to / instead): proves the flag itself works.
flagonly() {
  local t=$1; shift
  rm -f /tmp/term-pwd.txt
  ( cd / && timeout 10 "$t" "$@" >/dev/null 2>&1 & )
  for i in $(seq 1 40); do [ -s /tmp/term-pwd.txt ] && break; sleep 0.25; done
  local got; got=$(cat /tmp/term-pwd.txt 2>/dev/null)
  if [ "$got" = "$D" ]; then echo "ok   flag only: $t $*"; else echo "FAIL flag only: $t $* (got '$got')"; fi
  pkill -f "$t" 2>/dev/null; sleep 0.3
}
eval $(dbus-launch --sh-syntax)
for t in xterm xfce4-terminal mate-terminal lxterminal tilix terminator alacritty kitty konsole gnome-terminal qterminal terminology urxvt kgx ptyxis; do
  command -v $t >/dev/null || { echo "skip $t (not installed)"; continue; }
  run $t
done
echo "--- flags alone (cwd /)"
command -v xfce4-terminal >/dev/null && flagonly xfce4-terminal "--working-directory=$D"
command -v mate-terminal >/dev/null && flagonly mate-terminal "--working-directory=$D"
command -v lxterminal >/dev/null && flagonly lxterminal "--working-directory=$D"
command -v tilix >/dev/null && flagonly tilix "--working-directory=$D"
command -v terminator >/dev/null && flagonly terminator "--working-directory=$D"
command -v alacritty >/dev/null && flagonly alacritty --working-directory "$D"
command -v kitty >/dev/null && flagonly kitty --directory "$D"
command -v konsole >/dev/null && flagonly konsole --workdir "$D"
command -v gnome-terminal >/dev/null && flagonly gnome-terminal "--working-directory=$D"
command -v qterminal >/dev/null && flagonly qterminal --workdir "$D"
command -v terminology >/dev/null && flagonly terminology "--current-directory=$D"
command -v urxvt >/dev/null && flagonly urxvt -cd "$D"
command -v kgx >/dev/null && flagonly kgx --working-directory "$D"
command -v ptyxis >/dev/null && flagonly ptyxis --new-window --working-directory "$D"
echo "--- help texts"
for t in kgx ptyxis qterminal terminology deepin-terminal; do command -v $t >/dev/null && { echo "# $t"; $t --help 2>&1 | grep -i -E "dir|cwd" | head -4; }; done
