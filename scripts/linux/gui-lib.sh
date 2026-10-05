# Helpers for scripts/linux/gui.sh (sourced): real input (XTEST) and screenshots under Xvfb.
SHOTS=/src/.superpowers/linux-shots
shot() { mkdir -p $SHOTS && import -window root "$SHOTS/$1.png" 2>/dev/null; }
win() { xdotool search --name "Gezik" 2>/dev/null | head -1; }
key() { xdotool windowfocus --sync "$(win)" 2>/dev/null; for k in "$@"; do xdotool key "$k"; sleep 0.25; done; }
typ() { xdotool windowfocus --sync "$(win)" 2>/dev/null; xdotool type --delay 60 "$1"; sleep 0.3; }
click() { xdotool mousemove "$1" "$2" sleep 0.15 click "${3:-1}"; sleep 0.5; }
dclick() { xdotool mousemove "$1" "$2" sleep 0.15 click --repeat 2 --delay 80 1; sleep 0.8; }
rclick() { click "$1" "$2" 3; }
# The y of NAME's row in the list of DIR (folders first, then files, by name; no
# window manager, so the window is at 0,0 and row 0 is at y 118, 26 apart).
row() {
    local dir=$1 name=$2 i=0 entry
    while read -r entry; do
        [ "$entry" = "$name" ] && { echo $((118 + 26 * i)); return; }
        i=$((i + 1))
    done < <( (cd "$dir" && find . -mindepth 1 -maxdepth 1 -type d ! -name '.*' -printf '%f\n' | LC_ALL=C sort -f;
               find . -mindepth 1 -maxdepth 1 ! -type d ! -name '.*' -printf '%f\n' | LC_ALL=C sort -f) )
    echo 0
}
# Right-clicks NAME in DIR and picks the menu item ITEM (0-based) of Gezik's own menu.
menu() { local y; y=$(row "$1" "$2"); rclick 260 "$y"; click 330 $((y + 20 + 32 * $3)); }
# The colour of the pixel at X,Y of screenshot NAME.
px() { convert "$SHOTS/$1.png" -crop "1x1+$2+$3" txt:- | tail -1 | grep -o "#[0-9A-F]\{6\}"; }
# The share (0-1) of dark pixels in a W x H box at X,Y of screenshot NAME.
dark() { convert "$SHOTS/$1.png" -crop "$2x$3+$4+$5" -colorspace gray -threshold 50% -negate -format "%[fx:mean]" info:; }
