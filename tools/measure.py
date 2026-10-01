#!/usr/bin/env python3
"""Report the colour the mouse is showing, from a camera frame.

The device reports the blob back unchanged after a write, so a read cannot
confirm that anything landed. The camera is the only witness.

The camera cannot be told to raise saturation or exposure: the DirectShow pin
offers resolutions only, and pushing the pixels apart in software turns out to
be the wrong approach anyway. Two things decide what is measured here:

  - The mouse is a bright object on a dark desk, so brightness picks it out of
    the frame before colour is looked at. The shell measures up to 151 where
    the wood around it stays under 30.
  - Within the bright part, the LED is the pixel whose channels disagree most.
    The shell is white however it is lit, and the blue cast of the room in it
    is the same blue a blue LED makes, so brightness alone cannot say whether
    the mouse is on. A red mouse has a most saturated pixel of (169, 76, 61)
    and an unlit one (87, 89, 112), which is grey with a faint cast.

The result is the average of the fifteen strongest of those, not of all of them.
The shell is lit by the LED next to it and contributes a white cast to every
pixel, so averaging in more pixels dilutes the colour towards grey.

One limit is worth knowing before trusting a reading: the camera sets its
exposure from the frame, so an unlit mouse gives it a dark frame and there is
nothing above the brightness line to find. In a bright room an unlit mouse is
reported as unlit. In a dark one the tool reports no mouse in the frame, which
is a statement about what the camera can see rather than about what is there.

The band of the frame is a constant below, placed on a photograph of the setup
this was measured on. Move the mouse to a different part of the desk and the
measurement follows the frame rather than the mouse, so look at what is in it
before trusting a reading.
"""

import sys

import numpy as np
from PIL import Image

# The band of the frame the mouse is in, as fractions of width and height.
#
# Measured on a photograph of the current setup. The mouse sits between y 0.45
# and y 0.58 and x 0.40 and x 0.60 of a 1280 by 720 frame, its white shell
# reaching a brightness of 151 where the desk around it stays under 30.
#
# It is set from the mouse, not from a lit strip, because a lit strip moves: it
# runs along the lower edge for a solid colour and up over the perforated top
# for one of the others. The shell is always in the same place.
#
# The band starts below the desk line on purpose. The wood is warm and
# saturated, and reaching into it turns a dark mouse into a reported brown one,
# which is how a switched off mouse once measured as magenta.
BAND = (0.40, 0.45, 0.60, 0.58)

# How bright a pixel has to be to belong to the mouse at all.
#
# This is the whole difficulty of measuring a mouse that lights itself. The
# camera sets its exposure from the frame, so a mouse with the lighting off
# gives it a dark frame, and the shell that measures 151 in a lit frame
# measures barely anything in an unlit one. A fixed line therefore cannot
# separate "unlit" from "camera gave up": with the lighting off the best
# candidate on the shell measured (87, 89, 112) under a bright exposure and
# there was no pixel above the line at all under a dark one, where the frame
# itself is almost black.
#
# The consequence is worth stating rather than hiding: an unlit mouse is only
# reported as unlit while the rest of the scene is bright enough to hold the
# exposure. In a dark room the tool says there is no mouse in the frame, which
# is true of what it can see and not of what is there.
MIN_PEAK = 95

# How far the leading channel has to be above the weakest one for a bright pixel
# to count as lit. Measured on both ends: the most saturated pixel on a mouse lit
# red measures (169, 76, 61) and the most saturated one on a mouse with the
# lighting off measures (87, 89, 112), a grey with a faint cast. 45 sits between
# the two and clears the grey.
MIN_SPREAD = 45

# How bright a pixel has to be at all. The desk is lit and the shell is white,
# so brightness alone separates nothing; it is here to keep the dark corners of
# the band out, where noise decides which channel is the largest.
MIN_PEAK = 140

# The LEDs are the brightest coloured pixels, but the auto exposure drifts, so
# a short run of them is averaged rather than a single peak pixel. It has to
# stay short: a lit edge is a few hundred pixels at most, and averaging in more
# than that reaches the desk around the mouse, which is lit by the same window
# and carries the same tint as the reflection in it.
TOP_PIXELS = 15


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

    # The mouse is a bright object on a dark desk, so the band is reduced to
    # what is bright first. The desk is warm and dim and never gets past the
    # brightness line, while the shell measures up to 151.
    luminance = crop.mean(axis=2)
    bright = crop[luminance >= MIN_PEAK]
    if bright.size == 0:
        return ("keine Maus im Bild", 0, 0, 0)

    # Of the bright pixels, the LED is the one whose channels disagree most.
    # The shell is white however it is lit, and the room's blue cast in it is
    # the same blue that a blue LED produces, so brightness alone cannot say
    # whether the mouse is on. The difference between the channels can: a white
    # pixel has none, a lit one has a lot. On a red mouse the strongest such
    # pixel measures (169, 76, 61), and on an unlit one the best is (87, 89,
    # 112), which is grey with a faint cast and not a colour anything shows.
    flat = bright
    channel = np.argmax(flat, axis=1)
    order = np.argsort(flat, axis=1)
    strongest = np.take_along_axis(flat, order[:, 2:3], axis=1)[:, 0]
    weakest = np.take_along_axis(flat, order[:, 0:1], axis=1)[:, 0]
    spread = strongest - weakest

    lit = spread >= MIN_SPREAD
    if not lit.any():
        return ("kein Licht sichtbar", 0, 0, 0)

    # Whichever channel led across the band is the colour, and the brightest
    # such pixels give its value. Averaging over all of them would mix in the
    # dimmer ones, which are mostly the shell lit by the same LED.
    winning = channel[lit]
    dominant = int(np.bincount(winning, minlength=3).argmax())
    candidates = flat[lit]
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

    Which channel leads is decided by index rather than by comparing the
    divided values for equality, because a cyan of (79, 150, 207) gives shares
    of 0.382, 0.725 and 1.0, and no two of those are ever quite equal after the
    rounding that got them there.
    """
    peak = max(red, green, blue)
    if peak < 60:
        return "aus oder schwarz"
    share = [v / peak for v in (red, green, blue)]
    order = sorted(range(3), key=lambda i: share[i], reverse=True)
    top, second, lowest = order
    # The name is decided by which two channels are high and by how far the third
    # one sits below them. Everything below is measured, not reasoned: the
    # figures quoted are what a mouse set to that colour actually gave.
    #
    #   colour    first  second  third   gap 1st-3rd   gap 2nd-3rd
    #   rot       1.00   0.57    0.48    0.52          0.09
    #   gelb      1.00   0.93    0.76    0.24          0.17
    #   gruen     1.00   0.77    0.35    0.65          0.42
    #   cyan      1.00   0.80    0.35    0.65          0.45
    #   blau      1.00   0.65    0.21    0.79          0.44
    #   magenta   1.00   0.77    0.60    0.40          0.17
    #   weiss     1.00   0.76    0.66    0.34          0.10
    #
    # White, yellow and magenta are the ones where the third channel is not far
    # below the second: the colour is genuinely made of all three. Red is the
    # odd one out among the plain colours, because the room's light is warm and
    # adds red to the shell, so a red mouse keeps green and blue close together.
    top_index, second_index, third_index = order
    gap_first = share[top_index] - share[third_index]
    gap_second = share[second_index] - share[third_index]

    if gap_first < 0.28:
        return "weiss oder grau"
    if top_index == 0 and second_index == 1:
        # Red, or a yellow that lost its top. Both are warm, and a warm mouse
        # under a warm light is a red mouse with green added to it.
        return "rot" if gap_second < 0.30 else "gelb"
    if top_index == 1 and second_index == 0:
        return "gelb"
    if (top_index == 0 and second_index == 2) or (top_index == 2 and second_index == 0):
        return "magenta"
    # Blue, green and cyan cannot all be told apart, and the reason is the
    # camera rather than the arithmetic. Their measured shares are close enough
    # that the room's own blue cast, which sits on every pixel, moves a reading
    # from one to another: a blue mouse gives (54, 167, 255) with green second,
    # which is the same shape as a cyan at (89, 204, 255), and a white mouse
    # gives (165, 189, 249), which is a yellow in everything but the top.
    #
    # What is left is reported honestly rather than guessed at. Red and magenta
    # are reliable, and so is the difference between a colour whose third
    # channel is far down and one whose is not: that is what separates a blue
    # mouse from a white one, and it is what the brightness sweep relies on.
    if 1 in (top_index, second_index):
        return "gruen, cyan oder blau"
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
