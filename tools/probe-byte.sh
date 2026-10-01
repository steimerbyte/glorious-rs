#!/usr/bin/env bash
# Sweep one byte of the configuration report and photograph the mouse per value.
#
# Everything except the byte under test is carried over from the device's own
# blob, so a change in what the mouse shows can be attributed to that byte.
#
# The device reports back the state from before the write, so that read is not a
# confirmation. What the mouse shows is the answer.
#
# The mouse is left on the last value passed, so put the one it should end up
# with last.
#
# Usage: probe-byte.sh OFFSET WINDOWS-PFAD WERT...
set -uo pipefail

OFFSET="$1"
WIN_ROOT="$2"
shift 2
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MEASURE="$HERE/measure.py"

rest="${WIN_ROOT#/mnt/}"
drive="${rest%%/*}"
WIN="$(printf '%s' "${drive^}:\\${rest#*/}" | tr '/' '\\')"
OUT="$WIN_ROOT/byte$OFFSET"

rm -rf "$OUT"
mkdir -p "$OUT"

for value in "$@"; do
    echo "==> byte $OFFSET = $value"
    (cd "$WIN_ROOT" && ./glorious-ctl.exe --set-byte "$OFFSET" "$value" 2>&1 | head -1)
    sleep 4
    (cd "$WIN_ROOT" && cmd.exe /NoProfile /c "cd /d $WIN && cap.cmd" >/dev/null 2>&1)
    cp "$WIN_ROOT/cam0.jpg" "$OUT/$value.jpg" 2>/dev/null
    printf '    -> '
    python3 "$MEASURE" "$OUT/$value.jpg" 2>&1 | head -1
done

echo
echo "frames in $OUT"
