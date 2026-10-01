//! Wire protocol constants shared by every module.
//!
//! Derived from the vendor utility `Glorious Model O Software v1.0.9` and
//! verified against hardware, USB ID `258a:0036`.
//!
//! The command table is complete on purpose, including the parts this app does
//! not act on yet, so the protocol stays documented in one place. Unused
//! entries therefore do not warn.

#![allow(dead_code)]

/// Report id of the configuration blob.
pub const REPORT_ID_CONFIG: u8 = 0x04;
/// Report id of the command register.
pub const REPORT_ID_COMMAND: u8 = 0x05;
/// Report id of the longer configuration blob used by newer mice.
pub const REPORT_ID_CONFIG_LONG: u8 = 0x06;

/// Size of the command register as declared by the device.
pub const COMMAND_SIZE: usize = 6;
/// Size of the configuration transfer. Always the same, regardless of the
/// device's own payload length.
pub const CONFIG_REPORT_SIZE: usize = 520;

/// Shortest configuration payload seen so far. Used as a lower bound when
/// trimming trailing zero bytes.
/// Smallest payload this tool can decode.
///
/// The colour list alone needs 65 bytes, so a shorter blob has to be refused
/// before the parser reads past the end.
pub const MIN_CONFIG_SIZE: usize = 65;

/// Length of the configuration content on a Model O.
///
/// Measured: the device answers 520 bytes and stores 130 of them, the last
/// meaningful one at offset 129. The driver reserves room for up to 167 bytes
/// on larger models in the same report, so the transfer size is not the
/// content size.
pub const MODEL_O_CONFIG_SIZE: usize = 130;

pub const CMD_FIRMWARE_VERSION: u8 = 0x01;
pub const CMD_PROFILE: u8 = 0x02;
pub const CMD_GET_CONFIG: u8 = 0x11;
pub const CMD_GET_BUTTONS: u8 = 0x12;
pub const CMD_DEBOUNCE: u8 = 0x1a;
pub const CMD_LONG_ANGLE_LOD: u8 = 0x1b;
pub const CMD_GET_CONFIG2: u8 = 0x21;
pub const CMD_GET_BUTTONS2: u8 = 0x22;
pub const CMD_MACRO: u8 = 0x30;
pub const CMD_GET_CONFIG3: u8 = 0x31;
pub const CMD_GET_BUTTONS3: u8 = 0x32;

/// Number of DPI slots the hardware provides. The vendor software exposes
/// fewer, but the device always has this many.
pub const NUM_DPI_SLOTS: usize = 8;

/// Slots this device actually offers.
///
/// The report has room for eight and the driver writes all eight, but the
/// vendor configuration for this mouse lists six resolutions and six colours, and
/// a Model O stores no others: enabling more than six and writing it back was
/// refused outright, leaving every resolution at its previous value. The last two
/// slots are storage the firmware keeps but does not drive, which is why they
/// read back as 100 dpi.
pub const USABLE_DPI_SLOTS: usize = 6;

/// Offset of the first per-slot LED colour, three bytes per slot, RGB order.
///
/// Confirmed by the driver's `sinowealth_config_report`: `dpis` is a 16 byte
/// union starting at byte 13, so `dpi_color[8]` begins at 13 + 16 = 29. A
/// colour change made in the vendor software changed exactly one byte, at
/// 29 + slot * 3, which confirms the stride.
pub const COLOR_SLOT_BASE: usize = 29;

/// Lighting effects, with the byte that selects them.
///
/// The names come from the ratbag driver, which documents the values for this
/// family of mice. Every one of them was then confirmed on a real Model O by
/// writing the byte, photographing the mouse and comparing the frames: values 1,
/// 3, 4, 5, 7 and 9 change colour over time, 10 changes brightness without
/// changing hue, 2 holds one colour, and 0 and 6 turn the LEDs off.
///
/// Value 8 is the one that did not behave as its name suggests. `Random` was
/// expected to animate through colours and stays inside the blue to cyan range
/// across repeated photographs. It is still offered, because it does light the
/// mouse, but it is not the effect the name promises and no attempt is made
/// here to describe what it actually does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RgbEffect {
    Off,
    Glorious,
    Single,
    Breathing7,
    Tail,
    Breathing,
    /// Dark on a Model O: selecting it turns the LEDs off, and the vendor
    /// software does not offer it. Kept so that a stored 0x06 still reads back
    /// as what it is, but never offered and never written.
    Constant,
    Rave,
    Random,
    Wave,
    Breathing1,
    /// The value mice without LEDs report. It is not a constant, so it must
    /// never be written.
    NotSupported,
}

impl RgbEffect {
    /// The byte the device stores for this effect.
    pub fn as_byte(self) -> u8 {
        match self {
            RgbEffect::Off => 0x00,
            RgbEffect::Glorious => 0x01,
            RgbEffect::Single => 0x02,
            RgbEffect::Breathing7 => 0x03,
            RgbEffect::Tail => 0x04,
            RgbEffect::Breathing => 0x05,
            RgbEffect::Constant => 0x06,
            RgbEffect::Rave => 0x07,
            RgbEffect::Random => 0x08,
            RgbEffect::Wave => 0x09,
            RgbEffect::Breathing1 => 0x0a,
            RgbEffect::NotSupported => 0xff,
        }
    }

    /// The effect a stored byte stands for, if it is one this tool offers.
    pub fn from_byte(value: u8) -> Option<RgbEffect> {
        Some(match value {
            0x00 => RgbEffect::Off,
            0x01 => RgbEffect::Glorious,
            0x02 => RgbEffect::Single,
            0x03 => RgbEffect::Breathing7,
            0x04 => RgbEffect::Tail,
            0x05 => RgbEffect::Breathing,
            0x06 => RgbEffect::Constant,
            0x07 => RgbEffect::Rave,
            0x08 => RgbEffect::Random,
            0x09 => RgbEffect::Wave,
            0x0a => RgbEffect::Breathing1,
            _ => return None,
        })
    }

    /// Name as the vendor software shows it.
    pub fn name(self) -> &'static str {
        match self {
            RgbEffect::Off => "Aus",
            RgbEffect::Glorious => "Glorious Mode",
            RgbEffect::Single => "Single Color",
            RgbEffect::Breathing7 => "Breathing (7 Farben)",
            RgbEffect::Tail => "Tail",
            RgbEffect::Breathing => "Breathing",
            RgbEffect::Constant => "Constant",
            RgbEffect::Rave => "Rave",
            RgbEffect::Random => "Random",
            RgbEffect::Wave => "Wave",
            RgbEffect::Breathing1 => "Breathing (1 Farbe)",
            RgbEffect::NotSupported => "nicht unterstuetzt",
        }
    }

    /// Every effect offered in the menu, in the order the vendor lists them.
    ///
    /// `Constant` (0x06) is deliberately missing. It leaves the LEDs dark on a
    /// Model O, and the vendor software does not offer it either. It is kept in
    /// the enum so that a stored 0x06 still reads back as what it is instead of
    /// as an unknown byte, but it is never offered and never written.
    pub fn offered() -> [RgbEffect; 10] {
        [
            RgbEffect::Glorious,
            RgbEffect::Single,
            RgbEffect::Breathing7,
            RgbEffect::Tail,
            RgbEffect::Breathing,
            RgbEffect::Rave,
            RgbEffect::Random,
            RgbEffect::Wave,
            RgbEffect::Breathing1,
            RgbEffect::Off,
        ]
    }

    /// Whether writing this effect is worth doing.
    ///
    /// `Constant` is the one value that is not: it leaves the LEDs dark, so
    /// accepting it would let a profile written from the command line put the
    /// mouse into a state that looks like a hardware fault and cannot be told
    /// apart from one. `NotSupported` is excluded for the same reason.
    pub fn is_writable(self) -> bool {
        !matches!(self, RgbEffect::Constant | RgbEffect::NotSupported)
    }

    /// Whether this effect shows the per-slot colours, or its own instead.
    ///
    /// Measured on a Model O, by setting all six slot colours to one value and
    /// photographing the mouse under the effect:
    ///
    /// - all six slots red, Glorious Mode: a red mouse, fading towards the next
    ///   colour across three frames a second apart
    /// - all six slots blue, Glorious Mode: a warm yellow, which is the blend
    ///   between the blue slots and the colour the effect was already on
    ///
    /// So Glorious Mode reads the slot colours and moves through them. It does
    /// not read the single colour in bytes 57 to 59: with `ff8800` set there and
    /// every slot left blue, the mouse stayed blue, and under the solid effects
    /// the same value showed up as warm. That is why the solid colour editor is
    /// hidden for this effect while its colour still follows the DPI list.
    ///
    /// Breathing with seven colours is listed here because it is the one other
    /// effect that takes per-slot colours, and it does so for a different
    /// reason: the seven colours it is named for are not in the bytes this tool
    /// reads. See [`KNOWN_CONFIG_BYTES`] on bytes 61 to 81.
    pub fn uses_slot_colours(self) -> bool {
        matches!(self, RgbEffect::Glorious | RgbEffect::Breathing7)
    }

    /// Whether this effect shows one colour for the whole mouse.
    ///
    /// Measured on a Model O: only the solid effects take a colour from the
    /// profile. The others cycle through colours the tool does not hold.
    pub fn has_solid_colour(self) -> bool {
        matches!(
            self,
            RgbEffect::Single | RgbEffect::Breathing1 | RgbEffect::Breathing
        )
    }
}

/// Profiles whose config command differs only in the low nibble.
const PROFILE_OFFSETS: [u8; 3] = [0x00, 0x10, 0x20];

/// Whether this device stores colours as red, green, blue or red, blue, green.
///
/// The ratbag driver warns that the order varies by device, and on this one it
/// does. The measurement, all of it in the same solid effect and checked on a
/// photograph of the device rather than on a reading:
///
/// - writing `00ff00` produced a blue mouse
/// - writing `0000ff` produced a green mouse
/// - writing `ff0000` produced a red mouse
///
/// Red is the weak case in a test like this: it is the one channel a swap does
/// not touch, so a tool with the order wrong still shows red correctly. The first
/// attempt at this measurement tested red alone, concluded the order was the
/// ordinary one, and swapped green and blue in the process. Removing the
/// conversion made every green setting come out blue, which is the same wrong
/// result seen from the other side. The value below is what the device does.
pub const COLOUR_ORDER_RBG: bool = true;

/// Convert a colour from the tool's RGB order into the device's own order.
pub fn to_device_colour(colour: [u8; 3]) -> [u8; 3] {
    if COLOUR_ORDER_RBG {
        [colour[0], colour[2], colour[1]]
    } else {
        colour
    }
}

/// Byte offsets of the configuration blob this tool understands.
///
/// A write only touches these. Every other byte is left at whatever the device
/// reported, because a zero written into an unknown field clears a setting the
/// user never touched, and the device accepts that without complaining.
///
/// Offsets 53 to 82 are the lighting effects. Each field in here was confirmed
/// one at a time by writing it and photographing the mouse:
///
/// - 53 selects the effect
/// - 56 is the brightness of the solid effects
/// - 57 to 59 are the colour a solid effect shows
/// - 60 is the brightness of the seven colour breathing effect
///
/// Byte 54 is not in the list even though its direction was measured. It was
/// swept at 0, 128 and 255 under Glorious Mode and each value moved the
/// gradient along the mouse, but the device stores 0x41 there while a profile
/// this tool builds from scratch defaults to 0x13. A default that differs from
/// what the hardware ships with is not a measurement, so writing the byte would
/// silently replace a value the vendor chose. It is carried over instead.
///
/// Bytes 55 and 61 to 82 are carried over untouched as well. Byte 55 changed
/// nothing visible at 0, 64, 128 or 255. The range 61 to 81 was tested harder
/// than any other: all twenty-one bytes at zero still runs the breathing effect,
/// and all twenty-one at 255 never once shows red or green, so it is not the
/// seven colour table those offsets suggest.
pub const KNOWN_CONFIG_BYTES: &[usize] = &[
    9,  // sensor
    10, // report rate and X/Y independent flag
    11, // dpi slot count and active slot
    12, // disabled slot mask
    // 13..29 hold the DPI values: eight bytes, or sixteen when X and Y are
    // configured separately.
    13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28,
    // 29..53 hold one RGB colour per DPI slot.
    29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52,
    // Lighting: the effect selector, the two brightness fields and the colour a
    // solid effect shows.
    53, 56, 57, 58, 59, 60,
];

/// First and last byte offset covered by [`KNOWN_CONFIG_BYTES`], for tests.
pub const KNOWN_CONFIG_RANGE: std::ops::RangeInclusive<usize> = 9..=60;

/// Report rate stored in the low nibble of the rate byte.
pub const REPORT_RATES: [(u8, u16); 4] = [(0x1, 125), (0x2, 250), (0x3, 500), (0x4, 1000)];

/// Debounce values the vendor software offers.
pub const DEBOUNCE_TIMES: [u8; 7] = [4, 6, 8, 10, 12, 14, 16];

/// Set in the high nibble of the rate byte when X and Y DPI differ.
pub const CONFIG_FLAG_XY_INDEPENDENT: u8 = 0b1000;

/// Sensor identifier and its maximum resolution.
pub const SENSORS: [(u8, &str, u16); 4] = [
    (0x06, "PMW3360", 12000),
    (0x08, "PMW3212", 7200),
    (0x0e, "PMW3327", 10200),
    (0x0f, "PMW3389", 16000),
];

/// Value used when the sensor is not one of the known ones.
pub const DPI_FALLBACK_MAX: u16 = 2000;

/// Mice this tool speaks to.
pub const KNOWN_DEVICES: [(u16, u16, &str); 5] = [
    (0x258a, 0x0036, "Glorious Model O / O- (wired)"),
    (0x258a, 0x0027, "Glorious Model O (old firmware)"),
    (0x258a, 0x0033, "Glorious Model D / D-"),
    (0x258a, 0x2022, "Glorious Model O Wireless"),
    (0x3794, 0xa000, "Glorious Model O Eternal"),
];

/// Read command for a profile's configuration blob.
pub fn config_command(profile_index: usize) -> u8 {
    CMD_GET_CONFIG + PROFILE_OFFSETS[profile_index.min(PROFILE_OFFSETS.len() - 1)]
}

/// Read command for a profile's button map.
pub fn buttons_command(profile_index: usize) -> u8 {
    CMD_GET_BUTTONS + PROFILE_OFFSETS[profile_index.min(PROFILE_OFFSETS.len() - 1)]
}

/// Sensor name, falling back to a hex rendering for unknown values.
pub fn sensor_name(sensor: u8) -> String {
    match SENSORS.iter().find(|s| s.0 == sensor) {
        Some(entry) => entry.1.to_string(),
        None => format!("0x{sensor:02x}"),
    }
}

/// Maximum resolution the sensor accepts.
pub fn max_dpi_for_sensor(sensor: u8) -> u16 {
    SENSORS
        .iter()
        .find(|s| s.0 == sensor)
        .map(|s| s.2)
        .unwrap_or(DPI_FALLBACK_MAX)
}

/// Whether the sensor encodes DPI with a one-step offset.
fn sensor_has_offset(sensor: u8) -> bool {
    sensor == 0x0e || sensor == 0x06
}

/// Decode a raw slot value into DPI.
pub fn raw_to_dpi(raw: u8, sensor: u8) -> u16 {
    let raw = raw as u16 + if sensor_has_offset(sensor) { 1 } else { 0 };
    raw * 100
}

/// Encode DPI into a raw slot value.
pub fn dpi_to_raw(dpi: u16, sensor: u8) -> u8 {
    let raw = dpi / 100;
    let raw = raw.saturating_sub(if sensor_has_offset(sensor) { 1 } else { 0 });
    raw as u8
}

/// Report rate in Hz from its raw nibble.
pub fn report_rate_from_raw(raw: u8) -> u16 {
    REPORT_RATES
        .iter()
        .find(|entry| entry.0 == raw)
        .map(|entry| entry.1)
        .unwrap_or(0)
}

/// Raw nibble for a report rate in Hz.
pub fn report_rate_to_raw(hz: u16) -> Option<u8> {
    REPORT_RATES
        .iter()
        .find(|entry| entry.1 == hz)
        .map(|entry| entry.0)
}

/// Read the brightness out of a lighting mode byte.
///
/// Bytes 56 and 60 both hold the brightness of their effect in the high nibble,
/// and both were measured one value at a time on a real mouse, writing the byte
/// and photographing the result:
///
/// - byte 56 at 16, 32 and 64 lit the solid effect at rising brightness, and the
///   red channel of the lit strip went 231, 234, 248 across the three
/// - byte 60 at 0, 0x40 and 0xff lit the breathing effect in 1, 2 and 4 frames
///   out of eighteen taken a second apart, with peak lit pixel counts of 1171,
///   820 and 1508
///
/// The low nibble changed nothing visible at any of those values on either byte,
/// so the field is returned as the whole byte and left alone when written. It
/// is not a speed setting: that is a guess inherited from the ratbag driver,
/// which packs speed and brightness into one nibble pair.
pub fn rgb_brightness(byte: u8) -> u8 {
    byte
}

/// Combine a brightness back into a mode byte, keeping the low nibble.
pub fn rgb_brightness_encode(brightness: u8, low_nibble: u8) -> u8 {
    (brightness & 0xf0) | (low_nibble & 0x0f)
}
