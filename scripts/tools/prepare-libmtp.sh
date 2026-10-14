#!/bin/bash
# Builds libmtp and the libusb it uses from source as one universal (arm64 + x86_64) pair for
# Gezik's macOS download (Tool::Libmtp, spec 16 §10.3, §19 question 4), on a Mac with the
# Xcode Command Line Tools (clang, make, lipo, install_name_tool, codesign; curl and shasum
# come with macOS). pkg-config is not needed: libusb's flags are passed to libmtp's configure.
# Nothing from Homebrew is linked in.
#
#   bash scripts/tools/prepare-libmtp.sh <out folder>
#
# Writes <out>/libmtp-<version>-macos-universal.tar.xz (libmtp.9.dylib, libusb-1.0.0.dylib,
# COPYING, SOURCE.txt) and <out>/manifest.rs.txt, and prints the package's size and SHA-256.
# Publishing is by hand (scripts/tools/README.md, "libmtp").
set -euo pipefail

LIBMTP=1.1.23
LIBUSB=1.0.30
N=1
MIN_MACOS=11.0
LIBMTP_URL="https://github.com/libmtp/libmtp/releases/download/v$LIBMTP/libmtp-$LIBMTP.tar.gz"
LIBUSB_URL="https://github.com/libusb/libusb/releases/download/v$LIBUSB/libusb-$LIBUSB.tar.bz2"

[ $# -eq 1 ] || { echo "usage: $0 <out folder>" >&2; exit 2; }
[ "$(uname -s)" = Darwin ] || { echo "run this on a Mac" >&2; exit 2; }
for tool in clang make lipo install_name_tool otool codesign curl shasum; do
    command -v "$tool" >/dev/null || { echo "$tool is missing (xcode-select --install)" >&2; exit 2; }
done
here="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$1"
out="$(cd "$1" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The source archives, each checked against its size and SHA-256 in sources.sha256.
fetch() {
    local url="$1" name="${1##*/}"
    curl -sSfL -o "$work/$name" "$url"
    local line
    line="$(grep " $name\$" "$here/sources.sha256")" || { echo "$name is not in sources.sha256" >&2; exit 1; }
    local want_sha want_size
    read -r want_sha want_size _ <<<"$line"
    local sha size
    sha="$(shasum -a 256 "$work/$name" | cut -d' ' -f1)"
    size="$(stat -f %z "$work/$name")"
    if [ "$sha" != "$want_sha" ] || [ "$size" != "$want_size" ]; then
        echo "$name: got $sha $size, expected $want_sha $want_size" >&2
        exit 1
    fi
    echo "$name: $sha $size (as pinned)"
}
fetch "$LIBUSB_URL"
fetch "$LIBMTP_URL"
tar -xjf "$work/libusb-$LIBUSB.tar.bz2" -C "$work"
tar -xzf "$work/libmtp-$LIBMTP.tar.gz" -C "$work"

export MACOSX_DEPLOYMENT_TARGET="$MIN_MACOS"
jobs="$(sysctl -n hw.ncpu)"
for arch in arm64 x86_64; do
    prefix="$work/prefix-$arch"
    host="$([ "$arch" = arm64 ] && echo aarch64 || echo x86_64)-apple-darwin"
    flags="-arch $arch -mmacosx-version-min=$MIN_MACOS -O2"
    for project in "libusb-$LIBUSB" "libmtp-$LIBMTP"; do
        build="$work/build-$project-$arch"
        mkdir -p "$build"
        # libmtp without MTPZ (Zune's encryption; it would need libgcrypt).
        extra=""
        [ "${project%%-*}" = libmtp ] && extra="--disable-mtpz"
        (
            cd "$build"
            # libusb is handed to libmtp's configure directly: no pkg-config needed, and none of
            # Homebrew's libraries can be picked up by one.
            LIBUSB_CFLAGS="-I$prefix/include/libusb-1.0" LIBUSB_LIBS="-L$prefix/lib -lusb-1.0" \
                PKG_CONFIG=false CC=clang CFLAGS="$flags" LDFLAGS="-arch $arch" \
                "$work/$project/configure" --host="$host" --prefix="$prefix" \
                --enable-shared --disable-static $extra >/dev/null
            make -j "$jobs" >/dev/null
            make install >/dev/null
        )
    done
done

stage="$work/stage"
mkdir -p "$stage"
lipo -create "$work/prefix-arm64/lib/libusb-1.0.0.dylib" "$work/prefix-x86_64/lib/libusb-1.0.0.dylib" \
    -output "$stage/libusb-1.0.0.dylib"
lipo -create "$work/prefix-arm64/lib/libmtp.9.dylib" "$work/prefix-x86_64/lib/libmtp.9.dylib" \
    -output "$stage/libmtp.9.dylib"

# Each finds the other beside it, wherever Gezik unpacks them.
chmod u+w "$stage"/*.dylib
install_name_tool -id @loader_path/libusb-1.0.0.dylib "$stage/libusb-1.0.0.dylib"
install_name_tool -id @loader_path/libmtp.9.dylib "$stage/libmtp.9.dylib"
for arch in arm64 x86_64; do
    old="$(otool -arch "$arch" -L "$stage/libmtp.9.dylib" | awk '/libusb-1.0/ {print $1}')"
    install_name_tool -change "$old" @loader_path/libusb-1.0.0.dylib "$stage/libmtp.9.dylib" 2>/dev/null || true
done
# install_name_tool breaks the linker's signature; Apple silicon runs no unsigned code.
codesign --force --sign - "$stage/libusb-1.0.0.dylib" "$stage/libmtp.9.dylib"

# Checks: both architectures, nothing linked but the system and each other.
for lib in libusb-1.0.0.dylib libmtp.9.dylib; do
    archs=" $(lipo -archs "$stage/$lib") "
    case "$archs" in *" arm64 "*" x86_64 "* | *" x86_64 "*" arm64 "*) ;; *) echo "$lib:$archs" >&2; exit 1 ;; esac
    if otool -L "$stage/$lib" | tail -n +2 | grep -v -e '@loader_path/' -e '/usr/lib/' -e '/System/Library/'; then
        echo "$lib links something outside the package or the system" >&2
        exit 1
    fi
    codesign --verify "$stage/$lib"
done

cp "$work/libmtp-$LIBMTP/COPYING" "$stage/COPYING"
cat >"$stage/SOURCE.txt" <<EOF
libmtp $LIBMTP and libusb $LIBUSB, built from source for Gezik by scripts/tools/prepare-libmtp.sh
(https://github.com/wenlar/gezik) as universal (arm64, x86_64) libraries for macOS $MIN_MACOS or newer.
Gezik loads libmtp.9.dylib when a phone is opened; it is not linked into Gezik.

Both are under the GNU Lesser General Public License version 2.1 or later; COPYING is its text.

libmtp: https://github.com/libmtp/libmtp (configure --disable-mtpz)
  $LIBMTP_URL
  sha256 $(shasum -a 256 "$work/libmtp-$LIBMTP.tar.gz" | cut -d' ' -f1)
libusb: https://github.com/libusb/libusb
  $LIBUSB_URL
  sha256 $(shasum -a 256 "$work/libusb-$LIBUSB.tar.bz2" | cut -d' ' -f1)

The libraries' install names were set to @loader_path and they were signed ad hoc; nothing else
was changed.
EOF

package="libmtp-$LIBMTP-macos-universal.tar.xz"
(cd "$stage" && COPYFILE_DISABLE=1 tar --uid 0 --gid 0 --uname "" --gname "" -cJf "$out/$package" \
    libmtp.9.dylib libusb-1.0.0.dylib COPYING SOURCE.txt)
size="$(stat -f %z "$out/$package")"
sha="$(shasum -a 256 "$out/$package" | cut -d' ' -f1)"
echo "$package: $size B, sha256 $sha"

url="https://github.com/wenlar/gezik-tools/releases/download/libmtp-$LIBMTP-$N/$package"
{
    for platform in MacArm64 MacX64; do
        cat <<EOF
    ToolBuild {
        tool: Tool::Libmtp,
        platform: Platform::$platform,
        version: "$LIBMTP",
        url: "$url",
        size: $size,
        sha256: "$sha",
        programs: &["libmtp.9.dylib", "libusb-1.0.0.dylib"],
        kind: "tar.xz",
    },
EOF
    done
} >"$out/manifest.rs.txt"
echo "MANIFEST lines: $out/manifest.rs.txt"
