#!/usr/bin/env bash
# Build the AppImage for Linux. Requires cargo and appimagetool.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v appimagetool >/dev/null; then
    echo "appimagetool not found." >&2
    echo "Get it from https://github.com/AppImage/AppImageKit/releases" >&2
    exit 1
fi

# appimagetool is itself an AppImage, and mounting one needs FUSE. Under WSL
# there is no fusermount, and in a container there usually is none either. Both
# ship a self-extracting fallback that unpacks into a directory and puts
# mksquashfs next to it, which is all that is needed here: the tool is only ever
# run to write an image, never mounted.
APPIMAGETOOL="$(command -v appimagetool)"
if ! "$APPIMAGETOOL" --version >/dev/null 2>&1; then
    echo "==> appimagetool cannot run here, unpacking it instead"
    WORK="$(mktemp -d)"
    trap 'rm -rf "$WORK"' EXIT
    (cd "$WORK" && "$APPIMAGETOOL" --appimage-extract >/dev/null)
    APPIMAGETOOL="$WORK/squashfs-root/usr/bin/appimagetool"
    export PATH="$WORK/squashfs-root/usr/bin:$PATH"
fi

echo "==> release build"
cargo build --release

BIN="target/release/glorious-rs"
CTL="target/release/glorious-ctl"
[ -x "$BIN" ] || { echo "missing $BIN" >&2; exit 1; }
# The command line is not optional on Linux. The window is built for the GUI
# subsystem on Windows, and a binary in that subsystem has no console to print
# to, so the commands are the only way to read the mouse from a terminal here.
# An image without them would leave Linux with a window and no way to check what
# it wrote.
[ -x "$CTL" ] || { echo "missing $CTL" >&2; exit 1; }

APPDIR="target/glorious-rs.AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/glorious-rs/udev" "$APPDIR/usr/lib"

install -Dm755 "$BIN" "$APPDIR/usr/bin/glorious-rs"
install -Dm755 "$CTL" "$APPDIR/usr/bin/glorious-ctl"

# hidapi links against libudev, and the AppImage does not inherit the host's
# libraries. Without this the image starts on any machine that happens to have
# libudev and fails on a minimal one, which is exactly the machine where someone
# would want a self-contained file. Both the soname and its symlink are copied,
# because the binary asks for the soname and the loader resolves through it.
for lib in libudev.so.1 libgcc_s.so.1; do
    # No `exit` in the awk program: it would close the pipe early, and with
    # `set -o pipefail` the resulting SIGPIPE fails the whole build.
    path="$(ldconfig -p 2>/dev/null | awk -v l="$lib" '$1 == l && !found { print $NF; found = 1 }')"
    if [ -z "$path" ] || [ ! -e "$path" ]; then
        echo "missing $lib" >&2
        exit 1
    fi
    # The loader resolves the soname, and for a library whose soname is a
    # symlink that symlink is what it opens, so both are copied under their own
    # names. Copying the symlink itself would leave the image pointing outside
    # it, at a host file that may not be there.
    install -Dm755 "$path" "$APPDIR/usr/lib/$lib"
    real="$(readlink -f "$path")"
    if [ "$real" != "$path" ]; then
        install -Dm755 "$real" "$APPDIR/usr/lib/$(basename "$real")"
    fi
done
install -Dm644 udev/70-glorious-oss.rules "$APPDIR/usr/share/glorious-rs/udev/70-glorious-oss.rules"
install -Dm644 PROTOCOL.md "$APPDIR/usr/share/glorious-rs/PROTOCOL.md"
install -Dm644 README.md "$APPDIR/usr/share/glorious-rs/README.md"
install -Dm644 LICENSE "$APPDIR/usr/share/glorious-rs/LICENSE"
install -Dm644 packaging/AppRun "$APPDIR/AppRun"
install -Dm644 packaging/glorious-rs.desktop "$APPDIR/glorious-rs.desktop"

# Writes need the hidraw nodes, which the udev rule grants to the local seat.
cat > "$APPDIR/glorious-rs.png" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128">
  <rect width="128" height="128" rx="24" fill="#1c1f26"/>
  <path d="M64 24c-18 0-30 12-30 28v24c0 16 12 28 30 28s30-12 30-28V52c0-16-12-28-30-28z" fill="#3d4351"/>
  <path d="M64 24v52" stroke="#6b8afe" stroke-width="3"/>
</svg>
SVG

echo "==> appimage"
"$APPIMAGETOOL" "$APPDIR" target/glorious-rs-x86_64.AppImage

echo
echo "built: target/glorious-rs-x86_64.AppImage ($(du -h target/glorious-rs-x86_64.AppImage | cut -f1))"
