#!/usr/bin/env bash
# Builds Gezik's portable Linux release (x86_64) in the gezik-linux container and packs it as
# gezik-<short-sha>-linux-x64.tar.gz: the binary, LICENSE.md, THIRD-PARTY.md and README-linux.txt.
#   docker build -t gezik-linux scripts/linux
#   docker run --rm -v "$PWD:/src" -v gezik-target:/target -v gezik-cargo:/usr/local/cargo/registry \
#       -v "<output folder>:/out" -e CARGO_TARGET_DIR=/target -e SHA=$(git rev-parse --short HEAD) \
#       gezik-linux bash scripts/linux/package.sh
# SHA is needed when /src is a git worktree, whose .git points outside the container. The script
# prints the tarball's size and the glibc it needs, then unpacks it and starts the binary under
# Xvfb to check that the window opens. The exit code is 1 if that fails.
set -eu
cd /src
OUT=${OUT:-/out}
SHA=${SHA:-$(git -c safe.directory='*' rev-parse --short HEAD 2>/dev/null || echo unknown)}
NAME=gezik-$SHA-linux-x64

cargo build --release -p gezik 2>&1 | tail -1
BIN=${CARGO_TARGET_DIR:-/src/target}/release/gezik

# glibc: the highest version any symbol asks for, and the highest one the loader insists on
# (weak references, such as std's optional pidfd_spawnp, are skipped when missing).
versions=$(objdump -p "$BIN" | awk '/required from libc.so|required from libm.so/ {on=1; next} /required from/ {on=0} on && $4 ~ /^GLIBC_/ {print $2, $4}')
glibc_any=$(echo "$versions" | awk '{print $2}' | sort -uV | tail -1)
glibc_hard=$(echo "$versions" | awk '$1 == "0x00" {print $2}' | sort -uV | tail -1)
glibcxx=$(objdump -p "$BIN" | grep -o 'GLIBCXX_[0-9.]*' | sort -uV | tail -1)

stage=$(mktemp -d)
mkdir "$stage/$NAME"
install -m 0755 "$BIN" "$stage/$NAME/gezik"
install -m 0644 LICENSE.md THIRD-PARTY.md "$stage/$NAME/"
cat > "$stage/$NAME/README-linux.txt" <<EOF
Gezik $SHA for Linux x86_64 (portable build, not an installer)

Run
  tar xzf $NAME.tar.gz
  ./$NAME/gezik [folder]
To keep the settings apart from a normal install (~/.config/gezik), set a config folder:
  GEZIK_CONFIG_DIR=/tmp/gezik-cfg ./$NAME/gezik ~/test
Downloaded tools (7-Zip, ffmpeg, pdfium) go to <config folder>/tools/.

Needs
- glibc ${glibc_hard#GLIBC_} or newer (Ubuntu 22.04, Debian 12, Fedora 36 or later), libstdc++ with
  ${glibcxx#GLIBCXX_} symbols (GCC 11 or later), libgcc_s.
  With a glibc older than ${glibc_any#GLIBC_}, the loader prints "weak version '${glibc_any}' not
  found" at start. That is harmless (checked on Ubuntu 22.04).
- fontconfig (libfontconfig1) and at least one font. Fonts with CJK glyphs (e.g. fonts-noto-cjk)
  for Chinese, Japanese or Korean names and PDFs.
- X11: libX11, libX11-xcb, libXcursor, libXi, libxkbcommon, libxkbcommon-x11.
  Wayland: libwayland-client and libxkbcommon. They are loaded at start, for the session in
  use. To run it through XWayland in a Wayland session: env -u WAYLAND_DISPLAY ./gezik
- curl or wget, to download 7-Zip, ffmpeg and pdfium when Gezik offers to.
- A trash: ~/.local/share/Trash for the home drive (made when needed), .Trash-<uid> at the top
  of other drives. Without one, Delete asks to delete permanently.
- Optional: an xdg-desktop-portal, so that "auto" follows the desktop's light/dark setting.
  Without one the theme is light. D-Bus messages about xdg desktop settings at start are
  harmless.

No GPU or OpenGL is needed: Gezik draws with its software renderer.
Licence: PolyForm Noncommercial 1.0.0 (LICENSE.md); third-party notices in THIRD-PARTY.md.
EOF

mkdir -p "$OUT"
tar -C "$stage" --owner=0 --group=0 --sort=name -czf "$OUT/$NAME.tar.gz" "$NAME"
size=$(stat -c %s "$OUT/$NAME.tar.gz")
echo "tarball:  $OUT/$NAME.tar.gz ($size bytes, $(numfmt --to=iec-i --suffix=B "$size"))"
echo "binary:   $(stat -c %s "$BIN") bytes"
echo "glibc:    $glibc_hard required ($glibc_any highest, weak); $glibcxx"
echo "needed:   $(objdump -p "$BIN" | awk '/NEEDED/ {print $2}' | tr '\n' ' ')"

# Smoke run: the unpacked binary under Xvfb, with an empty config folder.
rm -rf "$stage/run" && mkdir -p "$stage/run/cfg" "$stage/run/home"
tar -C "$stage/run" -xzf "$OUT/$NAME.tar.gz"
Xvfb :97 -screen 0 1280x800x24 >/dev/null 2>&1 &
xvfb=$!
sleep 1
DISPLAY=:97 GEZIK_CONFIG_DIR="$stage/run/cfg" "$stage/run/$NAME/gezik" "$stage/run/home" >"$stage/run/log" 2>&1 &
gezik=$!
win=
for _ in $(seq 1 20); do
    win=$(DISPLAY=:97 xdotool search --name Gezik 2>/dev/null | head -1) && [ -n "$win" ] && break
    sleep 0.5
done
alive=0; kill -0 $gezik 2>/dev/null && alive=1
kill $gezik $xvfb 2>/dev/null || true
wait 2>/dev/null || true
if [ -n "$win" ] && [ $alive = 1 ] && ! grep -qi panicked "$stage/run/log"; then
    echo "smoke:    ok (window $win opened under Xvfb)"
    status=0
else
    echo "smoke:    FAIL"; cat "$stage/run/log"
    status=1
fi
rm -rf "$stage"
exit $status
