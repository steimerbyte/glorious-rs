#!/usr/bin/env bash
# Build the AppImage for Linux. Requires cargo and appimagetool.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v appimagetool >/dev/null; then
    echo "appimagetool not found." >&2
    echo "Get it from https://github.com/AppImage/AppImageKit/releases" >&2
    exit 1
fi

echo "==> release build"
cargo build --release

BIN="target/release/glorious-rs"
[ -x "$BIN" ] || { echo "missing $BIN" >&2; exit 1; }

APPDIR="target/glorious-rs.AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/glorious-rs/udev"

install -Dm755 "$BIN" "$APPDIR/usr/bin/glorious-rs"
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
appimagetool "$APPDIR" target/glorious-rs-x86_64.AppImage

echo
echo "built: target/glorious-rs-x86_64.AppImage ($(du -h target/glorious-rs-x86_64.AppImage | cut -f1))"
