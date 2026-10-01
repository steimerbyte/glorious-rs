#!/usr/bin/env python3
"""Tell the lighting effects apart from the photographs taken of each one.

The device reports the effect selector back unchanged whether or not it acted on
it, so the pictures are the only evidence. Two things separate one effect from
another:

  - Brightness. A breathing effect fades in and out, so its frames differ in
    brightness even when the hue stays put.
  - Hue. A cycling effect changes colour over the series.

The camera's own noise moves the hue by a few degrees between frames, so a
threshold is needed: a series counts as moving only when the hue changes by more
than that noise does.

Usage: effects.py EFFEKTORDNER...
"""

import glob
import os
import sys

import numpy as np
from PIL import Image

# Region of the frame holding the mouse, matching measure.py.
BOX = (0.45, 0.76, 0.62, 0.90)

# Only strongly coloured pixels count; the shell is matte white.
MIN_SATURATION = 0.30
TOP_PIXELS = 250

# Hue changes below this are the camera's noise rather than the effect moving.
NOISE_DEGREES = 35


def hue_of(colour) -> float | None:
    """Hue in degrees, or None when the colour is too grey to have one."""
    red, green, blue = [value / 255 for value in colour]
    peak, trough = max(red, green, blue), min(red, green, blue)
    if peak - trough < 0.08:
        return None
    spread = peak - trough
    if peak == red:
        hue = ((green - blue) / spread) % 6
    elif peak == green:
        hue = (blue - red) / spread + 2
    else:
        hue = (red - green) / spread + 4
    return hue * 60


def read_led(path: str) -> tuple[float, float | None]:
    """Mean brightness and hue of the lit LEDs in one frame."""
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
    flat = crop.reshape(-1, 3)
    peak = flat.max(axis=1)
    saturation = (flat.max(axis=1) - flat.min(axis=1)) / np.maximum(peak, 1)
    lit = flat[saturation >= MIN_SATURATION]
    if lit.size == 0:
        return (0.0, None)
    top_pixels = lit[np.argsort(-lit.max(axis=1))[: min(TOP_PIXELS, len(lit))]]
    mean = top_pixels.mean(axis=0)
    return (float(mean.mean()), hue_of([int(round(v)) for v in mean]))


def hue_distance(first: float, second: float) -> float:
    """Distance between two hues, the short way round the circle."""
    return abs((second - first + 180) % 360 - 180)


def describe(folder: str) -> dict:
    frames = sorted(
        glob.glob(os.path.join(folder, "*.jpg")),
        key=lambda p: int("".join(c for c in os.path.basename(p) if c.isdigit()) or 0),
    )
    brightness, hues = [], []
    for frame in frames:
        value, hue = read_led(frame)
        brightness.append(value)
        if hue is not None:
            hues.append(hue)
    if not brightness:
        return {"frames": 0, "brightness": 0.0, "bright_span": 0.0, "hue_span": 0.0}

    # Hue moves are measured between consecutive frames, so a slow fade that
    # never wraps past the 0 degree mark is still seen as moving.
    steps = [hue_distance(hues[i], hues[i + 1]) for i in range(len(hues) - 1)]
    return {
        "frames": len(frames),
        "brightness": float(np.mean(brightness)),
        # Relative to the brightest frame, so a dim effect is judged on its own
        # scale rather than against an absolute one.
        "bright_span": (max(brightness) - min(brightness)) / max(max(brightness), 1),
        "hue_span": max(steps) if steps else 0.0,
    }


def classify(stats: dict) -> str:
    if stats["frames"] == 0:
        return "keine Bilder"
    if stats["brightness"] < 12:
        return "aus"
    moving = stats["hue_span"] > NOISE_DEGREES
    pulsing = stats["bright_span"] > 0.35
    if moving and pulsing:
        return "Farbwechsel mit Helligkeit"
    if moving:
        return "Farbwechsel"
    if pulsing:
        return "nur Helligkeit"
    return "statisch"


def main(argv: list[str]) -> int:
    folders = argv[1:]
    if not folders:
        folders = sorted(glob.glob("effect-*"), key=lambda p: int(p.split("-")[1]))

    print(f"{'effekt':<6}{'bilder':<7}{'helligk':<10}{'hell-spanne':<13}{'hue-spanne':<12}befund")
    for folder in folders:
        stats = describe(folder)
        print(
            f"{folder.split('-')[-1]:<6}{stats['frames']:<7}"
            f"{stats['brightness']:<10.0f}{stats['bright_span']:<13.0%}"
            f"{stats['hue_span']:<12.0f}{classify(stats)}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
