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

# The band of the frame the mouse is in, as fractions of width and height.
# The bottom of the frame only, and nothing else. The window behind the desk
# reflects a blue tint that survives every colour the mouse is set to, and the
# white chair upholstery is more saturated than a dark unlit mouse. Both were
# measured before being excluded: a search that included them reported a red
# mouse as pale blue, because the reflected window dominated the average.
BAND = (0.30, 0.72, 0.78, 0.94)

# How far one channel has to lead the weakest one before a pixel counts as an
# LED. The window, the shell and the desk all sit well under this; a lit strip
# clears it by a wide margin even when the colour is a dim one.
MIN_LEAD = 60

# The LEDs are the brightest coloured pixels, but the auto exposure drifts, so
# a short run of them is averaged rather than a single peak pixel. It has to
# stay short: a lit edge is a few hundred pixels at most, and averaging in more
# than that reaches the desk around the mouse, which is lit by the same window
# and carries the same tint as the reflection in it.
TOP_PIXELS = 80


def dominant_colour(path: str) -> tuple[str, int, int, int]:
    image = Image.open(path).convert("RGB")
    width, height = image.size
    # The lower part of the frame, where the mouse is. The mouse is not nailed
    # down: it was measured at one set of coordinates and then found a few
    # centimetres away on the next run, so a fixed window around it measured an
    # empty patch of desk and reported that as the mouse being dark. Searching
    # the whole band for the most saturated pixels follows it instead.
    left, top, right, bottom = BAND
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

    flat = crop.reshape(-1, 3)

    # A pixel is only part of the answer if one channel clearly leads. This is
    # what separates the LEDs from everything around them: the window behind the
    # desk reflects into the whole frame, the white shell is barely coloured, and
    # the desk carries the window's tint. None of those has a channel that runs
    # far ahead of the other two, and an LED does.
    peak = flat.max(axis=1)
    channel = np.argmax(flat, axis=1)
    order = np.argsort(flat, axis=1)
    weakest = order[:, 0]
    lead = peak - np.take_along_axis(flat, weakest[:, None], axis=1)[:, 0]

    lit = flat[lead >= MIN_LEAD]
    if lit.size == 0:
        return ("keine LED sichtbar", 0, 0, 0)

    # Whichever channel led across the band is the colour, and the brightest
    # such pixels give its value. Averaging over all of them would mix in the
    # dimmer ones, which are mostly the shell lit by the same LED.
    winning = channel[lead >= MIN_LEAD]
    dominant = int(np.bincount(winning, minlength=3).argmax())
    candidates = flat[lead >= MIN_LEAD]
    ranked = candidates[np.argsort(-candidates[:, dominant])][: min(TOP_PIXELS, len(candidates))]
    red, green, blue = (int(round(v)) for v in ranked.mean(axis=0))
    return (f"{red:02x}{green:02x}{blue:02x}", red, green, blue)


def name(red: int, green: int, blue: int) -> str:
    """Plain German colour name, coarse on purpose.

    The thresholds are looser than the colours look on screen. A lit strip is
    never a pure channel: light scatters inside the diffuser and the camera
    applies a white balance, so a mouse set to green measured as (46, 156, 90)
    rather than (0, 255, 0). The names below were set from those measurements
    rather than from what the colours should be.
    """
    peak = max(red, green, blue)
    if peak < 60:
        return "aus oder schwarz"
    share = [v / peak for v in (red, green, blue)]
    lead = sorted(share, reverse=True)
    if lead[0] - lead[2] < 0.35:
        return "weiss oder grau"
    if share[0] == lead[0] and share[1] > 0.6:
        return "gelb"
    if share[1] == lead[0] and share[2] > 0.6:
        return "cyan"
    if share[2] == lead[0] and share[0] > 0.6:
        return "magenta"
    if share[0] == lead[0]:
        return "rot"
    if share[1] == lead[0]:
        return "gruen"
    return "blau"


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: measure.py BILD", file=sys.stderr)
        return 2
    hexcode, red, green, blue = dominant_colour(sys.argv[1])
    print(f"{hexcode}  rgb({red}, {green}, {blue})  {name(red, green, blue)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
