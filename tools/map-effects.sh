#!/usr/bin/env bash
# Map every lighting effect, several frames per effect.
#
# One photograph is not enough. Breathing and the rainbow look different in
# every frame, so a single frame could be mistaken for a static colour or for an
# effect that was never applied. Taking a short series per effect shows two
# things at once: whether the colour changes over time, and whether the effect is
# steady.
#
# The device reports the selector byte back unchanged whether or not it acted on
# it, so the mouse itself is the only witness.
#
# Usage: map-effects.sh [FRAMES] [WINDOWS-PFAD]
#
# The second argument is the Windows directory holding the built tool and
# cap.cmd, for example /mnt/c/Users/you/Downloads/glorious-rs maps to
# C:\Users\you\Downloads\glorious-rs. It defaults to the current user.
set -u

FRAMES="${1:-6}"
FRAME_GAP=0.7
# The values the ratbag driver documents for the selector byte.
EFFECTS="0 1 2 3 4 5 6 7 8 9 10"

# Where the scripts and the Python helpers live, taken from this file's own
# location rather than written down, so the folder can be moved or cloned.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MEASURE="$HERE/measure.py"

WIN_ROOT="${2:-/mnt/c/Users/$USER/Downloads/glorious-rs}"
# The same place as WSL sees it, and the form cmd.exe wants: a WSL mount point
# names the drive as a lower case directory, and uses forward slashes, neither of
# which cmd.exe accepts.
LOCAL="$WIN_ROOT"
rest="${WIN_ROOT#/mnt/}"
drive="${rest%%/*}"
WIN="$(printf '%s' "${drive^}:\\${rest#*/}" | tr '/' '\\')"
OUT="$LOCAL/effects"

rm -rf "$OUT"
mkdir -p "$OUT"

capture() {
  cmd.exe /NoProfile /c "cd /d $WIN && cap.cmd" >/dev/null 2>&1
}

# The colours each frame reported, one per line, plus whether they differed.
summarise() {
  python3 - "$1" "$MEASURE" <<'PY'
import subprocess
import sys

folder, measure = sys.argv[1], sys.argv[2]
import glob
import os

frames = sorted(glob.glob(os.path.join(folder, "*.jpg")))
seen = []
for frame in frames:
    out = subprocess.run(
        ["python3", measure, frame], capture_output=True, text=True
    ).stdout.split()
    seen.append(out[0] if out else "??")

# How many distinct colours the frames showed. An effect that breathes or cycles
# gives more than one; a static colour gives exactly one.
distinct = sorted(set(seen))
moves = len(distinct) > 1
print(" ".join(seen), "->", "bewegt" if moves else "statisch",
      f"({len(distinct)} verschieden)")
PY
}

for effect in $EFFECTS; do
  # The blob prints bytes in hex, so ten is 0a. Both sides are normalised to
  # hex with two digits, otherwise the comparison fails for every value past
  # nine and the series is skipped without being recorded.
  wanted=$(printf '%02x' "$effect")
  folder="$OUT/effect-$effect"
  mkdir -p "$folder"

  # A white solid colour, so an effect that is not a static one still lights up
  # and can be told apart from "no effect at all".
  cmd.exe /NoProfile /c "cd /d $WIN && glorious-ctl.exe --set-effect $effect ffffff" \
    >/dev/null 2>&1
  # The mouse takes a moment to accept the profile, and a read straight after a
  # write still returns the old value, so the selector is checked from the blob
  # before any photograph is taken. Without this the series silently records
  # whatever the previous effect left lit.
  for _ in 1 2 3 4 5 6 7 8; do
    sleep 0.5
    stored=$(cmd.exe /NoProfile /c "cd /d $WIN && glorious-ctl.exe --dump-config" 2>&1 |
      grep '^ 48:' |
      # Byte 53 is the sixth byte of the row that starts at offset 48.
      awk '{ for (i = 2; i <= NF; i++) if ($i ~ /^[0-9a-f][0-9a-f]$/ && n++ == 5) { print $i; exit } }')
    [ "$stored" = "$wanted" ] && break
  done
  if [ "${stored:-}" != "$wanted" ]; then
    printf '%-3s nicht uebernommen, Blob zeigt %s\n' "$effect" "${stored:-?}"
    continue
  fi
  sleep 1.5

  for frame in $(seq 1 "$FRAMES"); do
    capture
    cp "$LOCAL/cam0.jpg" "$folder/frame-$frame.jpg"
    sleep "$FRAME_GAP"
  done

  printf '%-3s %s\n' "$effect" "$(summarise "$folder")"
done

echo
echo "Bilder in $OUT"
