#!/usr/bin/env bash
# Assembles Gezik.app from a built binary and signs it: with the Developer ID named by
# $SIGN_IDENTITY when set (release.yml), otherwise ad hoc.
#   scripts/macos/make-app.sh [binary] [version] [out folder]
# Defaults: target/release/gezik, the workspace version in Cargo.toml, target/release.
# The Info.plist is crates/gezik/macos/Info.plist (the one build.rs embeds in the binary) plus
# the version and, as spec 15 §10.2 asks, the languages AppKit's own menu items may use.
# The icon is made from crates/gezik/icons/gezik.png (1024 px) with sips and iconutil.
set -eu
cd "$(dirname "$0")/../.."
BIN=${1:-target/release/gezik}
VERSION=${2:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}
OUT=${3:-target/release}
APP=$OUT/Gezik.app
PB=/usr/libexec/PlistBuddy

[ -x "$BIN" ] || { echo "no binary at $BIN (cargo build --release -p gezik)" >&2; exit 1; }
[ -n "$VERSION" ] || { echo "no version" >&2; exit 1; }
# Apple wants plain numbers here: 0.1.0-rc.1 is written as 0.1.0.
BUNDLE_VERSION=${VERSION%%[-+]*}

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m 0755 "$BIN" "$APP/Contents/MacOS/gezik"
PLIST=$APP/Contents/Info.plist
cp crates/gezik/macos/Info.plist "$PLIST"
$PB -c "Add :CFBundleShortVersionString string $BUNDLE_VERSION" \
    -c "Add :CFBundleVersion string $BUNDLE_VERSION" \
    -c "Add :CFBundleInfoDictionaryVersion string 6.0" \
    -c "Add :CFBundleDevelopmentRegion string en" \
    -c "Add :CFBundleLocalizations array" \
    -c "Add :CFBundleLocalizations: string en" \
    -c "Add :CFBundleLocalizations: string tr" \
    -c "Add :CFBundleLocalizations: string de" \
    -c "Add :CFBundleLocalizations: string es" \
    -c "Add :CFBundleLocalizations: string fr" \
    -c "Add :CFBundleLocalizations: string zh-Hans" \
    "$PLIST"
printf 'APPL????' > "$APP/Contents/PkgInfo"

ICONSET=$(mktemp -d)/gezik.iconset
mkdir "$ICONSET"
for s in 16 32 128 256 512; do
    sips -z $s $s crates/gezik/icons/gezik.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    sips -z $((s * 2)) $((s * 2)) crates/gezik/icons/gezik.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/gezik.icns"
$PB -c "Add :CFBundleIconFile string gezik" "$PLIST"

if [ -n "${SIGN_IDENTITY:-}" ]; then
    # Developer ID: hardened runtime and a timestamp, as notarization asks.
    codesign --force --options runtime --timestamp \
        --entitlements scripts/macos/gezik.entitlements --sign "$SIGN_IDENTITY" "$APP"
else
    codesign --force --sign - "$APP"
fi

# Self-check: a valid plist with the version, a signature that verifies.
plutil -lint "$PLIST" >/dev/null
[ -s "$APP/Contents/Resources/gezik.icns" ]
[ "$($PB -c 'Print :CFBundleShortVersionString' "$PLIST")" = "$BUNDLE_VERSION" ]
codesign --verify --strict "$APP"
echo "$APP ($VERSION, $(lipo -archs "$APP/Contents/MacOS/gezik"))"
