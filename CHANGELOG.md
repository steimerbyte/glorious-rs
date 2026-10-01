# Changelog

## Unreleased

### Interface

- The window and the commands are two binaries. Starting the window on Windows
  opened a black console next to it, because the binary was built for the
  console subsystem. Marking it for the GUI subsystem would have removed the
  console but also removed the output of every command, since a binary in that
  subsystem has none to print to, and that output is the only evidence the
  device work in this project has. `glorious-rs` is now built for the GUI
  subsystem and opens the window and nothing else, `glorious-ctl` keeps its
  console and runs the commands, and the flags moved over with them.

- The window is drawn from a theme rather than from egui's defaults. Colours,
  corner radii, spacing and text sizes are set in one place, so a colour appears
  once in the source. Sections are cards with a hairline border and an accent
  stripe on the one card that is being worked on, and the dark scheme is chosen
  regardless of the system preference: a light window would put LED colours on
  white, which is a different picture from the one the user sees on the desk.
- A preview strip shows what the profile will look like, drawn from the same
  bytes the write is built from. It shows a solid colour for the effects that
  have one, moves through the slot colours for Glorious Mode, and pulses for the
  effects whose colour source this project has not found. It says which of those
  it is doing, because the device does not report whether a change took effect
  and a preview that guessed would be a claim rather than a reading.
- Sparks follow the pointer, and a starburst fires on every clicked control. The
  two are separate effects rather than one at two sizes: a spark says the
  pointer is here, a starburst says a button was pressed.
- A profile list: a resolution and a colour per step, reordered and removed
  individually, with one button that hands the whole list to the mouse. Steps
  land on the enabled slots from the top, because a disabled slot is storage the
  firmware keeps and does not light, so writing one produces a setting the user
  cannot see. A list longer than the number of enabled slots is truncated, with
  the window saying so, rather than refused: a profile being built is exactly
  when a partial result is useful. Clicking a slot number in the table adds a
  step with that resolution and the colour the slot already has.
- The single-value preset buttons are gone. They filled every slot that was
  switched off with one value, which is a different thing from mapping a list
  onto the slots in use, and keeping both meant two ways to write the same bytes
  with different rules.
- 10000 dpi is offered in the list. It was the one resolution in common use that
  could not be picked with one click.
- The slot the mouse is using is marked in the DPI table: a tinted row and an
  accent bar down its left edge, rather than a differently coloured slot number,
  which was too easy to miss in a table of six. The mark follows the device, and
  the window says in its hover text that the DPI button on the mouse is what
  changes it. There is no command that writes that byte, so offering a way to
  pick a different slot here would be a button that quietly does nothing.
- The brightness of a solid effect is a four step slider rather than a row of
  five buttons, because the values are a progression and equal-looking buttons
  do not say that. The value stays next to the handle, since it is the byte the
  device stores and the terminal and the window should not disagree about what
  a position means. `0xff` is no longer offered: it was on the row because byte
  60 lit at it, which says nothing about byte 56, and nothing here has a
  photograph of the solid effect at that brightness.

### Fixed

- Applying a profile list put the same resolution on every slot. Two faults
  fed each other. The write path rendered all eight slots, so bytes 19 and 20
  were given a number on every save even though a Model O drives six slots and
  never reads those two; and the window kept all eight slots in its own state,
  so a value that was never written to the mouse came back on the next read and
  looked like the mouse had taken it. Only the six slots the device drives get a
  resolution byte now. Which bytes those are depends on the X/Y flag, so the
  set is computed rather than listed, and bytes 25 to 28, the seventh and eighth
  slot in the X/Y layout, are left to the device in both layouts. The eight slot
  colours are still written: how many of them the lighting block reads was never
  measured, and Glorious Mode follows them.
- A resolution typed into the window was discarded, and the field then snapped
  back to what the mouse had. The save was gated on the mouse button being up,
  and a field that was opened by clicking still has the button down for as long
  as it is being typed into, so the value reported a change on every frame it
  was typed and was dropped on every one of them. A value typed or nudged is
  committed straight away now; only a drag waits for the release, because a drag
  reports on every frame it moves and writing each of those floods the mouse.
- A mouse whose sensor byte is not one of the four measured here was given a
  resolution field capped at 2000 dpi. That number is this tool's own fallback
  and was never measured for anything, and a field with a maximum clamps
  whatever is typed into it, so typing 10000 stored 2000. An unknown sensor
  now gets the highest of the measured sensors as a ceiling, which admits the
  value instead of silently replacing it, and the window says that the real
  maximum of that sensor is unknown rather than presenting a guess as a
  measurement.
- The resolution field accepted values the device cannot store. Its encoding is
  `raw * 100`, so 1234 is stored as raw 11 and reads back as 1200 on a PMW3360:
  the window showed one number and the mouse showed another, the second one
  arriving on the next read. The field takes whole hundreds now, which are the
  only values that survive the encoding on every sensor this tool knows.
- Switching a slot on or off changed the disabled mask without changing the slot
  count in byte 11. The count is what the device uses to decide how many slots
  exist, and a profile claiming more slots than the mouse has was the shape
  `--probe-slots` found being dropped whole. The two are brought back into
  agreement before anything is written, and the active slot is clamped only when
  the slot it names no longer exists.
- The single-value preset command was dead code, and it described the first fault
  exactly: one resolution on the slot the user picked and on every slot that is
  switched off. It had no caller left since the profile list replaced it and is
  gone rather than kept as a second way to write the same bytes by different
  rules.
- A pointer held still over a control would have kept laying sparks forever. The
  trail ages out after a fraction of a second, so a still pointer looked like a
  fresh arrival every frame, and the window never stopped repainting. Movement is
  the only trigger now, and a test holds it.

## 1.1.0

The lighting block of the configuration blob is measured further, and three
things that were assumed in 1.0.0 turned out to be wrong. Every claim was
confirmed on a real Model O by writing a value, photographing the mouse and
comparing frames, because the device reports the blob back unchanged after a
write and cannot confirm that anything landed.

### Fixed

- **Glorious Mode reads the DPI slot colours, not bytes 57 to 59.** With all six
  slots set to red the mouse went red, and with all six set to blue it showed the
  blend between them. With `ff8800` in the solid colour bytes and every slot blue,
  the mouse stayed blue. In 1.0.0 the effect was treated as playing colours of
  its own, and the solid colour editor was shown for it. It is now grouped with
  the seven colour breathing as an effect that follows the DPI list, and the
  editor is hidden for it.
- **The mode bytes are not speed and brightness packed into a nibble pair.** That
  comes from the ratbag driver. Byte 56 was swept at 16, 32 and 64 and lit the
  solid effect at rising brightness, with the red channel of the lit strip
  reading 231, 234 and 248, while its low nibble changed nothing visible at any
  of them. `rgb_mode_decode` and `rgb_mode_encode` are gone, replaced by a
  brightness accessor that returns the byte unchanged.
- **The defaults for bytes 54, 56 and 60 are what the hardware ships with.** They
  stood at `0x13` each, taken from the same driver guess. A real Model O stores
  `0x41`, `0x40` and `0x42` there. A default only bites when a blob cannot be
  read, because a readable one overwrites every field, but a guess that writes
  over the user's gradient direction is not a default worth keeping.

### Added

- Brightness control for the effects that have a measured brightness field. The
  solid effects write byte 56, the seven colour breathing writes byte 60, and an
  effect without a measured field refuses the value rather than sending it
  somewhere unmeasured. The device's low nibble is carried through.
- `--set-effect N [RRGGBB] [BB]` takes a brightness byte as a third argument.
- Brightness in the lighting section of the window, with only the steps that were
  measured: `0x00`, `0x10`, `0x20`, `0x40` and `0xff`.

### Measured and still not written

- **Byte 54 is the direction the gradient moves in**, and only under Glorious
  Mode: 0 puts blue at the front and green at the back, 128 reverses it, 255
  puts red at the front. It is still carried over untouched, because no path in
  the app changes it and the field would otherwise put a default over a value
  the vendor chose.
- **Byte 60 is the brightness of the seven colour breathing effect**, upper
  nibble again. Counting how many of eighteen photographs a second apart showed
  the mouse lit: `0x00` lit one with a peak of 1171 pixels, `0x40` lit two with
  820, and `0xff` lit four with 1508.

### Measured and refuted

- **Bytes 61 to 81 are not the seven colours of the breathing effect.** With all
  twenty-one at zero the effect still runs, cycling through blue, magenta and
  white. With all twenty-one at 255 it also runs and never once shows red or
  green, although every byte was at its maximum. An effect that ignores its own
  colour table is not reading that table, so the seven colours live somewhere
  that has not been found. The range controls the effect across its whole width:
  zeroing 61 to 120 lets it run for a few frames and then the LEDs go out, while
  setting 61 to 68 on its own still gives a lit, slowly changing mouse. No single
  byte in it was found to switch the effect on or off.

### Measurement

- `tools/measure.py` was rebuilt. It now picks the mouse out of the frame by
  brightness, which is what separates it from the desk, and then takes the
  average of the fifteen pixels whose channels disagree most, rather than all
  bright pixels. Averaging in more of them dilutes the colour towards grey,
  because the shell is lit by the LED beside it.
- It says what it cannot do. A blue, a green and a cyan mouse measure
  `(54, 167, 255)`, `(81, 230, 178)` and `(89, 204, 255)`, and the room's own
  blue cast sits on every pixel and is larger than the distance between the
  three of them. The tool reports those as one group rather than picking one.
- The exposure limit is written down: the camera sets its exposure from the
  frame, so an unlit mouse gives it a dark frame. In a bright room an unlit mouse
  is reported as unlit, in a dark one the tool reports no mouse in the frame.

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

- Every effect value the driver names was written and photographed. Values 1, 3,
  4, 5, 7 and 9 change colour over time, 10 changes brightness without changing
  hue, 2 holds one colour, and 0 and 6 turn the LEDs off. Ten of the eleven are
  offered; see below for the eleventh.
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
  they were written for. The band of the frame holding the mouse is a constant
  in `measure.py`, measured on a photograph of that setup.
- Wireless models are not covered. Everything here was measured on the wired
  Model O with firmware V103.