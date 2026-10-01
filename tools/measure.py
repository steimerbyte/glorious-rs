#!/usr/bin/env python3
"""Report the colour the mouse is showing, from a camera frame.

The device reports the blob back unchanged after a write, so a read cannot
confirm that anything landed. The camera is the only witness.

The camera cannot be told to raise saturation or exposure: the DirectShow pin
offers resolutions only. Instead the pixels are pushed apart here, which is more
precise than any camera setting would be, because it happens after the sensor's
own processing.

Two things make the LEDs stand out from the mouse's white shell:

  - The shell is matte, so it clips to near grey once saturation is raised. Any
    pixel that survives is lit plastic.
  - The LEDs are small, so only a handful of pixels are strongly coloured.
    Everything with a weak colour is noise from the auto white balance and gets
    discarded before the result is formed.
"""

import sys

import numpy as np
from PIL import Image

# Region of the frame holding the mouse, as fractions of width and height.
# Checked against captured frames: the mouse sits lower centre, and its lit
# edges are the only saturated pixels in there.
BOX = (0.45, 0.76, 0.62, 0.90)

# How far colours are pushed apart. Two is enough to grey out the white shell
# without clipping the LEDs themselves.
SATURATION_GAIN = 2.2

# A pixel only counts as a lit LED once it is clearly coloured rather than warm
# or cool grey. The auto white balance leaves a slight tint on everything.
MIN_SATURATION = 0.30

# The LEDs are the brightest coloured pixels, but the auto exposure drifts, so
# this many are averaged rather than a single peak pixel.
TOP_PIXELS = 250


def dominant_colour(path: str) -> tuple[str, int, int, int]:
    image = Image.open(path).convert("RGB")
    width, height = image.size
    left, top, right, bottom = BOX
    crop = np.asarray(
        image.crop(
            (
                int(left * width),
                int(top * height),
                int(right * width),
                int(bottom * height),
            )
        ),
        dtype=np.float64,
    )

    # Push the colours apart around their own luminance. This is what a camera's
    # saturation control would do, done on the pixels instead of the sensor.
    luminance = crop @ np.array([0.2126, 0.7152, 0.0722])
    boosted = luminance[..., None] + (crop - luminance[..., None]) * SATURATION_GAIN
    boosted = np.clip(boosted, 0, 255)

    flat = boosted.reshape(-1, 3)
    peak = flat.max(axis=1)
    spread = flat.max(axis=1) - flat.min(axis=1)
    saturation = spread / np.maximum(peak, 1)

    lit = flat[saturation >= MIN_SATURATION]
    if lit.size == 0:
        return ("keine LED sichtbar", 0, 0, 0)

    # The brightest of the saturated pixels: the LEDs outshine the shell, so
    # brightness is what separates a lit strip from an unlit one.
    order = np.argsort(-lit.max(axis=1))[: min(TOP_PIXELS, len(lit))]
    top = lit[order]
    red, green, blue = (int(round(v)) for v in top.mean(axis=0))
    return (f"{red:02x}{green:02x}{blue:02x}", red, green, blue)


def name(red: int, green: int, blue: int) -> str:
    """Plain German colour name, coarse on purpose."""
    peak = max(red, green, blue)
    if peak < 45:
        return "aus oder schwarz"
    share = [v / peak for v in (red, green, blue)]
    if min(share) > 0.85:
        return "weiss"
    if share[0] > 0.75 and share[1] < 0.45 and share[2] < 0.45:
        return "rot"
    if share[1] > 0.75 and share[0] < 0.45 and share[2] < 0.45:
        return "gruen"
    if share[2] > 0.75 and share[0] < 0.45 and share[1] < 0.45:
        return "blau"
    if share[0] > 0.65 and share[1] > 0.65 and share[2] < 0.45:
        return "gelb"
    if share[1] > 0.65 and share[2] > 0.65 and share[0] < 0.45:
        return "cyan"
    if share[0] > 0.65 and share[2] > 0.65 and share[1] < 0.45:
        return "magenta"
    return f"gemischt ({red},{green},{blue})"


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: measure.py BILD", file=sys.stderr)
        return 2
    hexcode, red, green, blue = dominant_colour(sys.argv[1])
    print(f"{hexcode}  rgb({red}, {green}, {blue})  {name(red, green, blue)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
