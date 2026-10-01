# glorious-rs

Open source configuration app for the **Glorious Model O and Model O- wired
mice** (2019). It replaces the vendor utility *Glorious Model O Software v1.0.9*,
which the manufacturer discontinued and which was a Windows-only application.

![The window, showing the eight DPI slots with their colours, the polling rate,
the lighting effect and the click debounce](docs/screenshot.png)

**This project was written entirely with AI assistance.** The protocol is
undocumented, so every offset in here was found by experiment against a real
mouse: writing a value, reading the configuration blob back, and in the case of
the lighting, photographing the mouse with the laptop camera to see what it
actually did. Several of the findings contradict what the code looked like when
it was first written, and the comments say so. Expect the same from further work
on it: treat it as hardware reverse engineering with an assistant, not as
reviewed production code. Every claim in this file was measured, but the
measurement setup is not reproduced here, so verify before relying on it.

The USB protocol is documented in [PROTOCOL.md](PROTOCOL.md), including which
parts were measured on real hardware and which are still open.

## What it does

- Reads sensor, report rate, debounce, the eight DPI slots and their LED colours
- Writes DPI, slot enable, slot colour, report rate, lighting effect, the colour
  of a solid lighting effect, and debounce
- Shows device name, USB ID, firmware and the active onboard profile
- Offers all eleven lighting effects with the names the vendor software uses

Button remapping, macros and lift-off distance are not implemented yet.

## Building

```sh
cargo build --release                 # native binary
cargo test                            # protocol tests, no hardware needed
./packaging/build-appimage.sh         # AppImage, needs appimagetool
```

Cross compiling to Windows needs the GNU target:

```sh
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

## Linux setup

The mouse exposes its configuration through `hidraw`, which is root-only by
default. Install the udev rule once:

```sh
sudo cp udev/70-glorious-oss.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Then unplug the mouse and plug it back in. The rule needs the `70` prefix: udev
applies rules in name order and the seat rules that grant `uaccess` run at `73`,
so a rule named later would have no effect.

## Command line

```
glorious-rs                        open the window
glorious-rs --dpi                  print the current settings and exit
glorious-rs --dump-config          print the raw 520 byte report with offsets
glorious-rs --set-effect N RRGGBB  set a lighting effect and its colour
glorious-rs --set-colour N RRGGBB  set the colour of a DPI slot
glorious-rs --calibrate-length     find the value the firmware wants in byte 3
```

## How it talks to the mouse

The protocol is not documented by the manufacturer. What follows was measured on
a real Model O, and each item exists because the obvious implementation fails in
a way the device does not report.

**The reports live on two different handles.** A SinoWealth mouse is a composite
HID device. The 6-byte command register and the 520-byte configuration blob are on
separate collections of the same interface, and both report the same VID and PID.
Opening only one handle gives reads that never change or writes that fail
silently, so both are opened and every transfer is routed by report id.

**A short transfer is dropped without an error.** The configuration report is
always 520 bytes. Sending only the meaningful part of it is accepted by the API
and ignored by the firmware, so the write silently does nothing.

**One transfer, not two.** Writing a command report to select the configuration
and then sending the blob makes the firmware treat the blob as the answer to that
command and discard it. The driver sends the blob on its own, and so does this.

**Byte 3 has to be 122.** The driver writes `config_size - 8`, which for the
largest model in the family is 159, not the 122 this device stores. The value
was found by writing one field, reading it back and restoring, for each
candidate. Several values are accepted and several blank the whole
configuration, without saying so.

**Only the bytes that are understood may be written.** A field whose meaning is
not known has to keep whatever the device reported. Writing zero into it clears
a setting the user never touched, and the device accepts that without comment:
this is how an early version of this tool cleared the lighting of a real mouse.

**The colour channels are red, blue, green.** The ratbag driver warns that the
order varies by device, and on this one it does. Writing `00ff00` produces a
blue mouse, writing `0000ff` produces a green one, and writing `ff0000` produces
a red one. Each of those was checked on a photograph of the device, not on a
read back from it.

Red is the trap in a test like this. It is the one channel a swap does not touch,
so a tool with the order backwards still shows red correctly and looks right.
Measuring red alone led to the opposite conclusion here, and the mistake showed
up as every green setting coming out blue. The conversion applies to writing
only: a colour read back from the device is already in the device's order, and
swapping it on the way in would undo the swap on the way out.

**Byte 56 is the brightness of the solid effect, and only the upper nibble of
it.** Measured by writing one value at a time and photographing the mouse after
each: values 1, 2, 4, 8 and 16 left the LEDs dark, 32 lit them at about half, and
64 lit them fully. The lower nibble made no visible difference at any of those
values. The field is packed as brightness in the high nibble, so a value of
`0x13` is brightness 1 and speed 3, and the pairing in `rgb_mode_decode` is the
other way round from what that ordering implies.

**Byte 54 sets the direction of the colour gradient, and byte 55 does
nothing that could be seen.** Both were swept one value at a time under the
glorious effect, with a photograph after each. Byte 54 moves the gradient along
the mouse: 0 puts blue at the front and green at the back, 128 reverses it, 255
puts red at the front. Byte 55 made no visible difference at 0, 64, 128 or 255,
and is carried over from the device untouched. Under the solid effects byte 54
makes no difference either, which is why it only means something in one of them.

**Byte 60 is the brightness of the seven colour breathing effect, upper nibble
again.** Counting photographs about a second apart, byte 60 at 0 lit the mouse in
one of eighteen with a peak of 1171 lit pixels, 0x40 in two with a peak of 820,
and 0xff in four with a peak of 1508.

**Bytes 61 to 81 are supposed to be the seven colours of that effect, and the
device does not read them as such.** The test that settles it: with all
twenty-one set to zero the breathing effect still runs, cycling through blue,
magenta and white. With all twenty-one set to 255 it also runs and never shows
red or green, though every byte was at its maximum. An effect that ignores its
own colour table is not reading that table.

What the range does do: zeroing 61 to 120 lets the effect run for a few frames
and then the LEDs go out, while setting 61 to 68 on its own, or 69 to 75 on its
own, still gives a lit and slowly changing mouse. So the effect is controlled
across the whole span rather than by one byte in it, and no single byte there was
found to switch it on or off. They are carried over from the device untouched,
which is what stops a colour the user has set from being written over by a
guess. A single photograph of a breathing effect is worth nothing anyway: the
lit strip measured between 32 and 1890 pixels on successive frames a second
apart.

**A read right after a write returns the old values.** The window therefore
reports what it just wrote, and a reload from the device confirms it.

## Verifying a change

The device does not confirm a write, and reading the blob back does not show
whether the change took effect. For anything that touches the LEDs, a camera is
the only witness: write a colour, photograph the mouse, and compare.

The scripts used for that are in [`tools/`](tools/):

| | |
|---|---|
| `measure.py BILD` | the colour the mouse is showing, from one photograph |
| `map-effects.sh [Bilder] [Pfad]` | writes each lighting effect in turn and photographs it several times |
| `effects.py [Ordner…]` | tells the recorded effects apart by how much their brightness and hue move |
| `cap.cmd` | takes one frame from the camera, run through `cmd.exe` |

They need WSL, a camera on the host and `ffmpeg` on the Windows side.

Two things about the camera had to be measured rather than assumed, and both
were wrong at first. The mouse is moved around on the desk, so a fixed window
around it eventually frames an empty patch and reports that as a dark mouse. The
scripts search the lower band of the frame for pixels where one channel clearly
leads the other two instead, which follows the mouse wherever it is. The window
behind the desk reflects into the whole picture and the desk carries that tint,
so both have to be excluded; a search that included them reported a red mouse as
pale blue. `measure.py` still reads green as cyan, because the reflected window
is blue and lands in the same channel the mouse is in. Red, blue and off are
reliable, which is what the effect mapping needs.

An earlier run of `map-effects.sh` produced a table of what each lighting effect
does, and that table was wrong: the mouse was not in the frame at all, so the
script measured an empty desk. The only claim from it that survived is the one
the user could see without a camera, that value 6 leaves the LEDs dark. The
descriptions of the other effects are not in this file because they have not been
measured against an image showing the mouse. Running the script writes to the
device and leaves the last effect it wrote selected.

## Keywords

Searching for a replacement for the vendor tool usually starts with the symptom
rather than the product name, so the terms below are the ones that describe the
problem rather than the project.

**German**

Glorious Model O Alternative, Glorious Model O Software Ersatz, Glorious Model
O Konfigurationssoftware, Glorious Model O open source, Glorious Model O
Einstellungen am PC, Glorious Maus DPI einstellen, Glorious Maus RGB Farbe
ändern, Glorious Model O Beleuchtung, Glorious Model O Software abgeschaltet,
Glorious Model O kein Effekt mehr, Glorious Model O Hersteller Software
abgeschafft, Glorious Model O Treiber funktioniert nicht, Glorious Model O
Farbe wird nicht übernommen, Glorious Model O Glorious Mode ausschalten, Maus
Beleuchtung einstellen Windows, Maus Makro Software, Glorious Model O ohne
Herstellersoftware einstellen, Maus DPI Stufen einstellen, Glorious Model O
Firmware 1.0.9, Glorious Model O anschließen Linux, Glorious Maus hidraw Linux,
HID Feature Report Maus auslesen.

**English**

Glorious Model O open source alternative, Glorious Model O configuration tool
Linux, Glorious Model O software discontinued, replace Glorious Model O
software, Glorious Model O settings on PC, Glorious mouse RGB colour per DPI
slot, Glorious Model O driver not working, Glorious Model O colour not
applying, Glorious Model O Glorious mode disable, Glorious Model O Protocol,
SinoWealth HID protocol, Model O mouse config Linux, mouse DPI profiles open
source, USB HID feature report mouse configuration, Model O replacement
software.

**Related hardware this was tested against**

Glorious Model O wired 258a:0036, Glorious Model O- wired, Glorious Model D,
Glorious Model O Wireless, Glorious Model O Eternal, SinoWealth 0027 and 0036
firmware, PMW3360 sensor.

## Licence

MIT.
