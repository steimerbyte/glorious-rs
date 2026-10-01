# Changelog

## 1.0.0

First release. Every claim below was confirmed against a real Model O, either by
reading back what the device stored or, for anything touching the LEDs, by
photographing the mouse and comparing frames.

### Reading and writing

- DPI, report rate, debounce and the lighting effect are read from the device
  and written back. The configuration report is 520 bytes, confirmed by a length
  sweep, because a shorter request returns zeros without saying so.
- Colour byte offsets were mapped by writing one colour at a time and seeing
  which byte changed: a colour change moves exactly one byte, at `29 + slot * 3`.
- The device stores colours as red, blue, green. Writing green as `00ff00` made
  the mouse show blue and writing blue made it show green, while red was
  unaffected, which is why this had to be measured rather than assumed.
- The mouse keeps eight DPI slots. The vendor software only offers six, so the
  count was established by writing a distinct value into each and seeing which
  ones stick.

### Lighting

- All eleven effect values were written and photographed. Values 1, 3, 4, 5, 7
  and 9 change colour over time, 10 changes brightness without changing hue, 2
  holds one colour, and 0 and 6 turn the LEDs off.
- `Constant` (6) is not offered: it leaves the mouse dark, and the vendor
  software does not offer it either. It is still read back as what it is, so a
  profile written elsewhere is shown honestly rather than as an unknown byte.
- Value 8 was expected to change colour and does not: it stays within the blue
  to cyan range. The driver does not offer it either.

### Interface

- Every slot is editable, six are shown, and a colour set on a slot sets the
  mouse immediately rather than waiting for a save.
- Setting a colour throws a burst of paper in that colour across the window.
- The lighting effect list, the colour picker and the frame rate controls are
  wired to the device through a worker thread, so the window never blocks on a
  HID transfer.

### Packaging

- AppImage for Linux, single Windows executable for Windows. Both are built by
  a tagged release, so the artefacts match the source they were cut from.
- Windows builds use the GNU toolchain, which produces a single file that runs
  without the Visual C runtime being installed first.

### Known limits

- The device does not confirm a write and reading the blob back does not show
  whether it took effect. For the LEDs, a camera is the only witness.
- The measurement scripts in `tools/` expect a camera on a desk like the one
  they were written for. The frame region holding the mouse is a constant in
  `measure.py` and `effects.py`.
- Wireless models are not covered. Everything here was measured on the wired
  Model O with firmware V103.