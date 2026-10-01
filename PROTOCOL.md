# SinoWealth / Glorious Model O wire protocol

Reverse engineered from Glorious Model O Software v1.0.9 (`OemDrv.exe`, Inno Setup,
2019-09-16), taken over from the ratbag driver where the driver is named as the
source, and then verified against a physical Glorious Model O, USB ID `258a:0036`,
firmware `V103`.

The old Windows software was a Windows-specific MFC application that talked to the
mouse through vendor-defined HID feature reports. The protocol is not exotic: it is
plain `SET_FEATURE` / `GET_FEATURE` traffic with a small command register in front
of a configuration blob.

Two things are worth knowing before reading anything below.

The device does not confirm a write. It reports the configuration blob back
unchanged after a write, so nothing in the wire traffic says whether a change
landed. Anything that touches the LEDs is measured with a camera; see
[How a claim was verified here](#how-a-claim-was-verified-here).

The vendor driver describes several fields of this mouse in a way that measurement
contradicts. Those are listed in [Where the driver is wrong](#where-the-driver-is-wrong),
and the offset table below carries the corrected layout.

## Device identity

| Property | Value |
|---|---|
| Vendor | `0x258A` (SinoWealth) |
| Product | `0x0036` after the 1.0.9 firmware update, `0x0027` before it |
| Firmware seen | `V103` |
| Sensor | `0x06` = PMW3360, reported by the config blob |
| Manufacturer string | `SinoWealth` |
| Product string | `Wired Gaming Mouse` |

Note that `0x2489` is sometimes quoted for these mice and is wrong. `0x0027` is
shared with unrelated devices, so a PID alone does not identify a Model O: read the
firmware version to disambiguate.

## Composite HID: two collections, one VID and PID

The mouse is a composite HID device. Its vendor reports are split across two
collections of the same interface, and both collections report the same VID, the
same PID and the same product string. `hidapi` exposes them as two independent
handles:

| Collection | Report ID | Report size | Carries |
|---|---|---|---|
| command | `0x05` | 6 bytes | the command register |
| configuration | `0x04` | 520 bytes | the configuration blob |

Neither collection works alone. The command register is written to the command
collection and read back from it; the configuration blob is read from and written
to the configuration collection. Opening only one handle gives reads that never
change or writes that fail silently, with no error reported.

Nothing in the enumeration distinguishes the two: same interface number, same VID
and PID, same product string, and `hidapi` does not expose the report descriptor.
The assignment is made by probing. Each opened handle is sent a zeroed feature
report of the candidate size, and the handle that rejects it with
`ERROR_INVALID_PARAMETER` (EINVAL on Linux) does not own that report. The probe
writes zeros, which the device treats as a no-op, so it cannot change a setting.

Once both handles are known, every transfer is routed by report ID: report ID `0x05`
goes to the command collection, everything else to the configuration collection.

## Command register: report ID 5

Six bytes: report ID, command ID, three payload bytes.

| Command | Meaning | Status |
|---|---|---|
| `0x01` | firmware version, replies `V103` | measured |
| `0x02` | active profile, replies with a 1-based index | measured |
| `0x11` | read configuration of profile 1 | measured |
| `0x12` | read button map of profile 1 | from the driver, not used |
| `0x1A` | debounce time, payload is the value in half milliseconds | measured |
| `0x1B` | angle snapping / lift off | from the driver, not used |
| `0x21` / `0x22` | profile 2 configuration and buttons | in the code, not exercised |
| `0x31` / `0x32` | profile 3 configuration and buttons | in the code, not exercised |
| `0x30` | macro | from the driver, not used |
| `0x75` | enter DFU mode | from the driver, not in this code |

Reading a command is a two step operation: write the command register first, then
read it back. The reply echoes the command ID in byte 1, which is the only way to
confirm the answer belongs to the query.

Report 5 is declared as 5 bytes in the descriptor. Windows rejects a 4 byte buffer
with `ERROR_INVALID_PARAMETER` and accepts a 6 byte one, so a buffer of exactly 6
bytes is the reliable choice. That is a statement about the driver, not a
measurement taken here.

## Configuration blob: report ID 4

The read request is 520 bytes. A shorter request is accepted by the API and
answered with zeros without an error being reported, so the request length has to
be exactly the report size.

The content is 130 bytes on a Model O, with the last meaningful byte at offset 129.
The report reserves room for up to 167 bytes on larger models in the same family,
so the transfer size is not the content size.

Byte 0 carries the report ID, byte 1 the command the blob answers, and byte 3 the
content length that a write has to declare. The device does not fill byte 3 in: it
reads as zero on the hardware tested, and byte 1 only echoes the command. Scanning
for the last non-zero byte instead is wrong, because trailing fields may
legitimately be zero and truncating there would silently drop the fields after them.

### Byte 3 is 122, not 159

The driver writes `config_size - 8`, where `config_size` is a compile-time constant
for the largest model in the family: 167 bytes of content, of which 8 are the
header, so byte 3 addresses 159. A Model O stores 130 bytes, so the same arithmetic
gives 122 on this mouse. The formula is not wrong; the constant behind it belongs to
a different device.

159 is not what this device wants. It was found by writing one slot colour with each
candidate in `0, 57, 64, 65, 96, 122, 123, 126, 127, 128, 129, 130, 131, 159, 167,
0xff`, then reading the blob back and checking three things: did the colour land, are
the DPI values still there, and is byte 8 still what the device had. The original
profile is restored after every candidate.

The device takes no notice of a value it dislikes: it either stores the change or
blanks the whole configuration, and reports neither. Several candidates were
accepted and several blanked the configuration. 122 is the value measured to work on
a Model O and is the one written.

That the device answers 130 bytes of content and wants 122 written back is the
consistency check that makes this a measurement rather than a magic number: 122 + 8
is exactly the content length it stores.

### Layout from byte 9

Offsets are absolute, counted from the start of the 520 byte report.

| Offset | Size | Field | Status |
|---|---|---|---|
| 9 | 1 | sensor, `0x06` for PMW3360 | measured |
| 10 | 1 | report rate in the low nibble, `0x8` in the high nibble is XY independent DPI | measured |
| 11 | 1 | DPI count in the high nibble, active slot in the low nibble | measured |
| 12 | 1 | bitmask of disabled slots | measured |
| 13 | 8 | DPI values, one per slot, 16 when X and Y are configured separately | measured |
| 29 | 24 | per slot LED colour, three bytes each, in the device's own channel order | measured |
| 53 | 1 | lighting effect selector | measured |
| 54 | 1 | gradient direction under Glorious Mode | measured, not written |
| 55 | 1 | no visible effect found | unknown |
| 56 | 1 | brightness of the solid effects, high nibble | measured |
| 57 | 3 | the colour a solid effect shows | measured |
| 60 | 1 | brightness of the seven colour breathing effect, high nibble | measured |
| 61 | 21 | not the seven colour table, see below | refuted as a colour table |
| 82 | 1 | not a field anything here names; `0xFF` on the measured mouse | unknown |
| 125 | 1 | lift off distance; reads `0x02` on the measured mouse | field from the driver, meaning not measured |

Two rows in this table were wrong before and have been moved rather than patched.
The per-slot colours were listed at offset 21, which is inside the DPI values: the
driver's `sinowealth_config_report` has `dpis` as a 16 byte union starting at byte
13, so `dpi_color[8]` begins at 13 + 16 = 29, and a colour change made in the vendor
software moved exactly the byte at `29 + slot * 3`, which confirms the stride. The
lighting block was listed at 45 to 73, which is the same block counted without the
8 byte header; on the wire it starts at 53.

The DPI counter starts at 1 and counts only enabled slots, so it is not a physical
slot index. With slots 0 to 4 and 6 to 7 disabled, an active value of 1 means the
sixth physical slot.

### DPI encoding

The value is the sensor's own register encoding, `DPI = raw * 100`, with the PMW3360
and PMW3327 starting at raw 3 rather than 0. The old software's `Cfg.ini` lists
`400, 800, 1600, 3200, 5000, 10000` for the Model O, which the read path reproduces
exactly.

### Report rate

Low nibble of offset 10: `0x1` = 125 Hz, `0x2` = 250, `0x3` = 500, `0x4` = 1000.

### Debounce

Report `0x1A`. Stored halved, so 8 ms is stored as 4. The vendor software offers 4,
6, 8, 10, 12, 14 and 16 ms; 2 ms is reachable on the wire although the vendor
software never exposes it. This tool accepts exactly the seven values the vendor
software offers and refuses anything else itself. The device's behaviour above 16 ms
was not measured.

## Lighting block, offsets 53 to 82

Every field in this block was measured one at a time, by writing the byte with
everything else carried over from the device's own blob, photographing the mouse and
comparing the frames. `tools/probe-byte.sh` does that sweep.

| Offset | Bedeutung | Status | Beleg |
|---|---|---|---|
| 53 | Effect selector | measured, written | Values 0 to 10 written one at a time and photographed. 0 and 6 leave the LEDs dark; which value produces which of the other nine is not confirmed, see below. |
| 54 | Direction of the colour gradient | measured, not written | 0 = blue at the front and green at the back, 128 = reversed, 255 = red at the front. Only visible under Glorious Mode. |
| 55 | unknown | unknown | No visible difference at 0, 64, 128 or 255. |
| 56 | Brightness of the solid effects | measured, written | 1, 2, 4, 8 and 16 leave the LEDs dark, 32 lights them at about half, 64 lights them fully. Red channel of the lit strip 231, 234, 248. |
| 57 to 59 | Colour a solid effect shows | measured, written | Stored red, blue, green. See below. |
| 60 | Brightness of the seven colour breathing | measured, written | Of eighteen frames a second apart: `0x00` lit one with a peak of 1171 lit pixels, `0x40` lit two with 820, `0xFF` lit four with 1508. |
| 61 to 81 | claimed to be the seven colours of the breathing effect | refuted | All 21 at zero still runs the effect, cycling blue, magenta and white. All 21 at 255 still runs and never once shows red or green. |
| 82 | not named by anything here | unknown | `0xFF` on the measured mouse. |

### Byte 53: the effect selector

Writing byte 53 and photographing the mouse confirms that the byte selects the
lighting effect, and that the values are 0 to 10.

The names below are the driver's, which documents the values for this family, and
they are the names the vendor software lists: 1 = Glorious Mode, 2 = Single Color,
3 = Breathing (7 Farben), 4 = Tail, 5 = Breathing, 6 = Constant, 7 = Rave,
8 = Random, 9 = Wave, 10 = Breathing (1 Farbe). What each of the other values
actually looks like has not been re-measured; see the paragraph below this list.

Value 6 is decoded but never offered and never written: it leaves the LEDs dark on a
Model O, and a profile written from the command line that puts the mouse into that
state produces something indistinguishable from a hardware fault. It is still kept in
the enumeration so that a stored `0x06` reads back as what it is rather than as an
unknown byte.

What each of the other ten values looks like is not reproduced here. An earlier run of
the effect mapping produced a table describing them, and that table is wrong: the
camera frames it was taken from did not contain the mouse, so it measured an empty
desk. The only claim from it that survives is the one visible without a camera at
all, that value 6 leaves the LEDs dark.

### Bytes 56 and 60: brightness in the upper nibble

Both bytes carry the brightness of their effect in the high nibble, and both were
swept one value at a time.

Byte 56, the solid effects: 1, 2, 4, 8 and 16 leave the LEDs dark; 32 lights the
strip at about half; 64 lights it fully. The red channel of the lit strip went 231,
234 and 248 across 16, 32 and 64. A real Model O with the vendor defaults stores
`0x40` here.

Byte 60, the seven colour breathing: counting how many of eighteen photographs a
second apart showed the mouse lit, `0x00` lit one with a peak of 1171 lit pixels,
`0x40` lit two with 820, and `0xFF` lit four with 1508. A real Model O with the
vendor defaults stores `0x42` here.

The low nibble changed nothing visible at any of those values on either byte. It is
therefore carried over from the device and written back unchanged, and there is no
speed field in either byte.

Byte 60 had to be measured with a series of frames, not with one: the lit strip
measures between 32 and 1890 pixels on successive frames a second apart, so a single
photograph of a breathing effect says nothing.

### Bytes 57 to 59: the colour a solid effect shows

These three bytes hold the colour the solid effects (Single Color, Breathing, and
Breathing with one colour) show. They are stored red, blue, green; see
[Colour channel order](#colour-channel-order-red-blue-green).

### Bytes 61 to 81: not the seven colour table

The driver names these 21 bytes as the seven colours of the seven colour breathing
effect, three bytes each. On this mouse they are not that.

The test that settles it: with all 21 bytes set to zero the effect still runs,
cycling through blue, magenta and white. With all 21 set to 255 it also runs, and
never once shows red or green, although every byte is at its maximum. An effect that
ignores its own colour table is not reading that table, so the seven colours live
somewhere that has not been found, and the range means something else.

The structure is still visible in the vendor defaults, and that is where the reading
comes from: the blob of a Model O with the vendor software defaults fills 61 to 81
with seven triplets, `07ff00`, `0000ff`, `000000`, `ff00ff`, `ffffff`, `00ff00`,
`ffffff`. Five of them are distinct colours, one is zero and one repeats. That is
what a seven colour table would look like, and the behaviour above does not support
reading them as the colours the effect shows.

What the range does do: zeroing bytes 61 to 120 lets the effect run for a few frames
and then the LEDs go out, while setting 61 to 68 on its own still produces a lit,
slowly changing mouse. So the effect is controlled across that whole span rather than
by one byte in it, and no single byte there was found to switch it on or off.

They are carried over from the device untouched. That is the only safe handling
while their meaning is unknown: writing zeros clears whatever the user had there,
and the device accepts that without comment.

### Glorious Mode reads the slot colours

Byte 53 = 1 is Glorious Mode, and it takes its colours from the DPI list at offsets
29 to 52, not from bytes 57 to 59:

- all six slot colours red, Glorious Mode: a red mouse, fading towards the next
  colour across three frames a second apart
- all six slot colours blue, Glorious Mode: a warm yellow, the blend between the blue
  slots and the colour the effect was already on
- `ff8800` in bytes 57 to 59 with every slot blue: the mouse stays blue

Byte 54, the gradient direction, is the field that belongs to this effect, and it too
is measured and still not written: the device stores `0x41` there, and a profile
built from scratch has no measurement to put in that byte, so a write would silently
replace a value the vendor chose with a guess.

## Colour channel order: red, blue, green

The driver warns that the colour order varies by device. On this one it is red, blue,
green. The measurement, all of it in the same solid effect and checked on a
photograph of the device rather than on a reading:

| Written | Mouse showed |
|---|---|
| `00ff00` | blue |
| `0000ff` | green |
| `ff0000` | red |

Red is the trap in a test like this. It is the one channel a swap does not touch, so a
tool with the order backwards still shows red correctly and looks right. The first
attempt at this measurement tested red alone, drew the opposite conclusion from it,
and swapped green and blue in the process; removing that conversion made every green
setting come out blue, which is the same wrong result seen from the other side. The
proof needs a pair of channels that a swap does move, which is why green and blue are
both in the table above and each was confirmed on an image.

The conversion applies when writing only. A colour read back from the device is
already in the device's order, and converting it on the way in would undo the swap on
the way out and turn every saved colour into a different one.

## Where the driver is wrong

Three assumptions carried over from the ratbag driver do not hold on this mouse. All
three were found by measurement, and each of them was in the code before it was in
the measurement.

**Speed and brightness are not packed into a nibble pair.** The driver reads byte 56
as speed in the low nibble and brightness in the high nibble, and this tool inherited
that reading, including `0x13` as the default value for bytes 54, 56 and 60. Sweeping
byte 56 at 16, 32 and 64 lit the solid effect at rising brightness, with the red
channel of the lit strip at 231, 234 and 248, and the low nibble changed nothing
visible at any of them. Byte 60 behaves the same way. There is no speed field in
either byte here, and the defaults are now what the hardware ships with: `0x41`,
`0x00`, `0x40` and `0x42` at bytes 54, 55, 56 and 60.

**Bytes 61 to 81 are not the seven colour table.** Covered in
[Bytes 61 to 81](#bytes-61-to-81-not-the-seven-colour-table). The consequence for a
write is that the range is carried over untouched rather than serialised from a
colour list.

**Byte 3 is not `config_size - 8` with the driver's constant.** Covered in
[Byte 3 is 122, not 159](#byte-3-is-122-not-159). The formula is right, the constant
is for a larger model in the same family, and 159 blanks the configuration here.

## Writing

Writing is read, patch, push back:

1. write the command register for the target profile (`0x11`, `0x21` or `0x31`) to
   the command collection, and read the 520 byte blob from the configuration
   collection
2. overlay only the fields this tool models on the bytes the device reported
3. set byte 0 to `0x04`, byte 1 to the command ID, byte 3 to `122`
4. write the whole 520 byte blob back in **exactly one** transfer, with no command
   report in front of it

Each of those four points is a place where the obvious implementation fails without
the device saying so.

**The transfer is the full 520 bytes.** Sending only the meaningful part is accepted
by the API and ignored by the firmware, so the write silently does nothing. The
buffer is always the report size, with only the leading content carrying data.

**Exactly one transfer.** Priming the device with a command report and then sending
the blob makes the firmware treat the blob as the answer to that command and discard
it. The driver sends the blob on its own and so does this.

**Only known bytes may be overwritten.** A field whose meaning is not known keeps
whatever the device reported. Writing zero into it clears a setting the user never
touched and the device accepts that without comment; this is how an early version of
this tool cleared the lighting of a real mouse. The bytes overlaid are sensor,
report rate, DPI count and active slot, the disabled slot mask, the eight or sixteen
DPI values, the 24 bytes of slot colours, and among the lighting block byte 53, byte
56, bytes 57 to 59 and byte 60.

**A read right after a write returns the old values.** The device answers with the
state from before the write, so a read-back is not a confirmation of it: reading a
second time returns the value the first call wrote. The window therefore reports what
it just wrote, and only a reload from the device later confirms it.

`Glorious Model O Software` needs administrator rights to save, which is a Windows
file-access artefact, not a protocol requirement.

## How a claim was verified here

The device reports the blob back unchanged after a write, so nothing on the wire
says that a change landed. A camera is the only witness for anything that touches
the LEDs. The scripts are in `tools/`:

| | |
|---|---|
| `measure.py BILD` | the colour the mouse is showing, from one photograph |
| `probe-byte.sh OFFSET WINDOWS-PFAD WERT...` | writes one byte of the report per value and photographs the mouse after each, everything else carried over |
| `map-effects.sh [Bilder] [Pfad]` | writes each lighting effect in turn and photographs it several times |
| `effects.py [Ordner…]` | tells the recorded effects apart by how much their brightness and hue move |
| `cap.cmd` | takes one frame from the camera, run through `cmd.exe` |

The measurement rests on two measured properties of the setup, and both were wrong
before they were measured.

The mouse is a bright object on a dark desk, so brightness picks it out of the frame
before colour is looked at: the white shell measures up to 151 where the wood around
it stays under 30. Within the bright part the LED is the pixel whose channels
disagree most, because the shell is white however it is lit. The result is the mean
of the fifteen strongest such pixels, not of all of them, because the shell is lit by
the LED next to it and contributes a white cast that dilutes every pixel towards grey.

The mouse is not nailed down on the desk. It was measured at one set of coordinates
and found a few centimetres away on the next run, and a search window fixed around
those coordinates then framed an empty patch of desk and reported that as a dark
mouse. The scripts search a band of the frame for the most saturated pixels instead,
which follows the mouse wherever it is.

What the camera cannot do is written down rather than guessed at:

- **Blue, green and cyan are not separable.** They measure `(54, 167, 255)`,
  `(81, 230, 178)` and `(89, 204, 255)`. The blue cast of the room sits on every
  pixel and is larger than the distance between the three of them, so the tool
  reports them as one group instead of picking one. Red and magenta are reliable.
- **Exposure follows the frame.** An unlit mouse gives the camera a dark frame, and
  there is nothing above the brightness line to find. In a bright room an unlit
  mouse is reported as unlit, in a dark one the tool reports no mouse in the frame,
  which is a statement about what the camera can see rather than about what is
  there.
- **One frame of a time-dependent effect is worth nothing.** The lit strip of the
  breathing effect measures between 32 and 1890 pixels on successive frames a second
  apart.

Red is the one colour the measurement gives as a stable reference: five out of five
measurements agreed, with the red channel between 247 and 252.

## What is verified and what is not

Verified against hardware by reading back what the device stored: firmware version,
active profile, debounce, sensor, DPI slots, active slot, report rate and the effect
selector.

Verified against hardware by camera: that byte 53 selects the lighting effect at all,
that 0 and 6 leave the LEDs dark, the gradient direction of byte 54, both brightness
fields, the colour a solid effect shows, and the colour channel order. Writing a
profile back is verified the same way: a colour written through the configuration
report reaches the mouse.

Not measured: what bytes 55, 61 to 82 mean; the button map and macro reports; the
`0x1B` angle snapping and lift off command; lift off distance at offset 125; and the
`0x06` long configuration report used by the newer mice. Whether 2 ms debounce is
accepted is also untested.

## Open questions

- What bytes 61 to 82 do. The seven colours of the breathing effect are not there.
  The range controls the effect across its whole width, and zeroing 61 to 120 makes
  the effect run a few frames and stop, but no single byte in it switches the effect
  on or off. Whether the colours live in a different report, further into the same
  one, or are compiled into the firmware has not been established.
- What byte 55 is. Swept at 0, 64, 128 and 255 under Glorious Mode with no visible
  difference, and not tried against a colour field yet.
- What byte 82 is. It sits one byte past the 21 byte range the driver names, reads
  `0xFF` on the measured mouse, and nothing here gives it a meaning.
- Where the seven colours of the breathing effect come from. It cycles through blue,
  magenta and white with its colour range at zero, so whatever drives it is either
  elsewhere in the report or built into the firmware.
- Which byte 3 values besides 122 are accepted, and which blank the configuration.
  The sweep covered 0, 57, 64, 65, 96, 122, 123, 126, 127, 128, 129, 130, 131, 159,
  167 and 0xFF and showed that several are accepted and several are not, without
  saying which is which.
- Whether the same byte 3 applies to profiles 2 and 3. It was only calibrated on
  profile 1.
- The meaning of the low nibble of bytes 56 and 60. It changes nothing visible, but
  it is carried over rather than understood.
- Whether the effect behaviour table can be rebuilt. The earlier one was measured
  against frames that did not contain the mouse, so the descriptions of the ten
  offered effects still have to be taken again.
- Whether a mouse without LEDs reports `0xFF` as its effect selector, and what that
  value does if it is written. It is never written, because a guess is not a
  measurement.
