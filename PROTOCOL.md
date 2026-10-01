# SinoWealth / Glorious Model O wire protocol

Reverse engineered from Glorious Model O Software v1.0.9 (`OemDrv.exe`, Inno Setup,
2019-09-16) and verified against a physical Glorious Model O, USB ID `258a:0036`.

The old Windows software was a Windows-specific MFC application that talked to the
mouse through vendor-defined HID feature reports. The protocol is not exotic: it is
plain `SET_FEATURE` / `GET_FEATURE` traffic with a small command register in front
of a configuration blob.

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

## Collections

The mouse is a composite HID device. Its vendor reports are split across two
collections of the same interface, and both report the same VID and PID:

- one collection accepts the command register, report ID 5
- the other collection accepts the configuration blob, report ID 4

Neither one alone works. Commands are written to the first and read back from it;
the configuration blob is read from and written to the second. The collections must
therefore be distinguished by probing, not by index or interface number.

## Command register: report ID 5

Six bytes: report ID, command ID, three payload bytes.

| Command | Meaning |
|---|---|
| `0x01` | firmware version, replies `V103` |
| `0x02` | active profile, replies with a 1-based index |
| `0x11` | read configuration of profile 1 |
| `0x12` | read button map of profile 1 |
| `0x1A` | debounce time, payload is the value in half milliseconds |
| `0x21` / `0x22` | profile 2 configuration and buttons |
| `0x31` / `0x32` | profile 3 configuration and buttons |
| `0x75` | enter DFU mode |

Reading a command is a two step operation: write the command register first, then
read it back. The reply echoes the command ID in byte 1, which is the only way to
confirm the answer belongs to the query.

Report 5 is declared as 5 bytes. Windows rejects a 4 byte buffer with
`ERROR_INVALID_PARAMETER` and accepts a 6 byte one, so a buffer of exactly the
declared size is the reliable choice.

## Configuration blob: report ID 4

Read length 520. The device fills byte 3 with the length of its own configuration
data, which is 123 or 137 bytes on the mice tested. A length that does not match
the device causes the firmware to return zeros instead of data, so the length must
be probed per device rather than assumed.

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | report ID, `0x04` |
| 1 | 1 | command ID the blob answers |
| 3 | 1 | length of the configuration data, from 123 |
| 9 | 1 | sensor, `0x06` for PMW3360 |
| 10 | 1 | report rate in the low nibble, `0x8` in the high nibble is XY independent DPI |
| 11 | 1 | DPI count in the high nibble, active slot in the low nibble |
| 12 | 1 | bitmask of disabled slots |
| 13 | 8 | DPI values, one per slot |
| 21 | 24 | per slot LED colour, three bytes each |
| 45 | 1 | RGB effect |
| 46 | 1 | glorious mode speed and brightness nibbles |
| 48 | 1 | single colour mode |
| 49 | 3 | single colour |
| 52 | 1 | breathing7 mode |
| 53 | 21 | seven breathing colours |
| 125 | 1 | lift off distance, `0xFF` when changed by a dedicated command |

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

Report `0x1A`. Stored halved, so 8 ms is stored as 4. The old software offers 4, 6,
8, 10, 12, 14 and 16 ms. Values above 16 ms are rejected, and 2 ms is technically
reachable although the vendor software never exposes it.

## Writing

Writing is read, patch, push back:

1. write the command register for the target profile
2. read the 520 byte blob
3. set byte 3 to the blob length minus 8
4. write the whole blob back

`Glorious Model O Software` needs administrator rights to save, which is a Windows
file-access artefact, not a protocol requirement.

## What is verified and what is not

Verified against hardware: firmware version, active profile, debounce, sensor, DPI
slots, active slot, report rate, RGB effect and LED colours.

Not yet verified: writing settings back, button remapping, macros, the `0x1B` angle
snapping and lift off command, and the `0x06` long configuration report used by the
newer mice.
