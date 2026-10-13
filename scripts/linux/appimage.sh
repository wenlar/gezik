#!/usr/bin/env bash
# Packs a built Linux binary (x86_64) as <out>/gezik-<version>-linux-x64.AppImage with a pinned,
# checksummed appimagetool and type2 runtime, then runs the AppImage's `--version` through
# --appimage-extract-and-run (no FUSE needed). The release workflow's linux job calls it:
#   bash scripts/linux/appimage.sh <binary> <version> <out folder>
# Gezik looks for a `gezik-portable` file next to the .AppImage (the APPIMAGE variable).
set -eu
BIN=$1 VERSION=$2 OUT=$3
src=$(cd "$(dirname "$0")/../.." && pwd)

# appimagetool 1.9.1 and type2-runtime 20251108 (github.com/AppImage, release assets).
TOOL_URL=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
TOOL_SHA=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
RUNTIME_URL=https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64
RUNTIME_SHA=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl -fsSL -o "$work/appimagetool" "$TOOL_URL"
curl -fsSL -o "$work/runtime" "$RUNTIME_URL"
printf '%s  %s\n' "$TOOL_SHA" "$work/appimagetool" "$RUNTIME_SHA" "$work/runtime" | sha256sum -c -
chmod +x "$work/appimagetool"

app=$work/Gezik.AppDir
mkdir -p "$app/usr/bin"
install -m 0755 "$BIN" "$app/usr/bin/gezik"
install -m 0644 "$src/crates/gezik/icons/gezik-256.png" "$app/gezik.png"
ln -s usr/bin/gezik "$app/AppRun"
cat > "$app/gezik.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Gezik
Comment=File manager
Exec=gezik %F
Icon=gezik
Categories=System;FileTools;FileManager;
MimeType=inode/directory;
EOF
if command -v desktop-file-validate >/dev/null; then desktop-file-validate "$app/gezik.desktop"; fi

mkdir -p "$OUT"
image=$OUT/gezik-$VERSION-linux-x64.AppImage
ARCH=x86_64 "$work/appimagetool" --appimage-extract-and-run --no-appstream \
    --runtime-file "$work/runtime" "$app" "$image"
size=$(stat -c %s "$image")
echo "appimage: $image ($size bytes, $(numfmt --to=iec-i --suffix=B "$size"))"

# Self-check: the packed binary starts and prints its version.
out=$(cd "$work" && "$image" --appimage-extract-and-run --version)
echo "version:  $out"
case "$out" in gezik*) ;; *) echo "appimage: FAIL (unexpected --version output)"; exit 1 ;; esac
