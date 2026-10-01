//! Decoded configuration of one profile.

use crate::protocol::*;

/// One DPI slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpiSlot {
    pub dpi: u16,
    pub disabled: bool,
    pub color: [u8; 3],
}

impl Default for DpiSlot {
    fn default() -> Self {
        DpiSlot {
            dpi: 800,
            disabled: false,
            color: [255, 0, 0],
        }
    }
}

/// Settings of one onboard profile.
#[derive(Debug, Clone)]
pub struct Profile {
    pub index: usize,
    pub sensor: u8,
    pub report_rate_raw: u8,
    pub xy_independent: bool,
    /// Counts enabled slots from 1, not physical slot indices.
    pub active_slot: u8,
    pub dpi_count: u8,
    pub slots: [DpiSlot; NUM_DPI_SLOTS],
    /// Lighting effect. `None` when the device reports a value this tool
    /// does not offer, which must not be written back unchanged by accident.
    pub rgb_effect: Option<RgbEffect>,
    /// The selector byte as the device stored it, kept so an effect this tool
    /// does not know can be written back unchanged.
    pub raw_effect_selector: u8,
    pub rgb_glorious_mode: u8,
    pub rgb_glorious_direction: u8,
    pub rgb_single_mode: u8,
    pub rgb_single_color: [u8; 3],
    pub rgb_breathing7_mode: u8,
    pub rgb_breathing7_colors: [[u8; 3]; 7],
    pub lift_off_distance: u8,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            index: 0,
            sensor: 0x06,
            report_rate_raw: 0x4,
            xy_independent: false,
            active_slot: 1,
            dpi_count: 0,
            slots: [DpiSlot::default(); NUM_DPI_SLOTS],
            // Single Color is the default: it is the one effect whose colour the
            // user actually chooses, so a profile this tool has never seen shows
            // a colour picker rather than a rainbow it cannot control.
            rgb_effect: Some(RgbEffect::Single),
            raw_effect_selector: 0x02,
            rgb_glorious_mode: 0x13,
            rgb_glorious_direction: 0x01,
            rgb_single_mode: 0x13,
            rgb_single_color: [255, 0, 0],
            rgb_breathing7_mode: 0x13,
            rgb_breathing7_colors: [[0, 0, 0]; 7],
            lift_off_distance: 0xff,
        }
    }
}

impl Profile {
    /// Sensor name, or a hex rendering when unknown.
    pub fn sensor_name(&self) -> String {
        sensor_name(self.sensor)
    }

    /// Maximum resolution the sensor accepts.
    pub fn max_dpi(&self) -> u16 {
        max_dpi_for_sensor(self.sensor)
    }

    /// Report rate in Hz.
    pub fn report_rate(&self) -> u16 {
        report_rate_from_raw(self.report_rate_raw)
    }

    /// Physical index of the active slot, or `None` when nothing is selected.
    ///
    /// The device counts the active slot over enabled slots only, so this
    /// cannot be read straight from the wire value.
    pub fn active_slot_index(&self) -> Option<usize> {
        let mut seen = 0u8;
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.disabled {
                continue;
            }
            seen += 1;
            if seen == self.active_slot {
                return Some(index);
            }
        }
        None
    }

    /// DPI currently selected, or 0 when no slot is active.
    pub fn active_dpi(&self) -> u16 {
        self.active_slot_index()
            .map(|index| self.slots[index].dpi)
            .unwrap_or(0)
    }

    /// Decode a configuration blob read from the device.
    ///
    /// Offsets follow the wire layout, whose first bytes are the report id, the
    /// command id and two unused bytes before the payload begins.
    pub fn parse(blob: &[u8]) -> Option<Profile> {
        if blob.len() < MIN_CONFIG_SIZE || blob[0] != REPORT_ID_CONFIG {
            return None;
        }
        let mut profile = Profile {
            sensor: blob[9],
            report_rate_raw: blob[10] & 0x0f,
            xy_independent: blob[10] & CONFIG_FLAG_XY_INDEPENDENT != 0,
            dpi_count: (blob[11] >> 4) & 0x0f,
            active_slot: blob[11] & 0x0f,
            ..Default::default()
        };
        let disabled_mask = blob[12];

        for i in 0..NUM_DPI_SLOTS {
            let raw = if profile.xy_independent {
                blob[13 + i * 2]
            } else {
                blob[13 + i]
            };
            profile.slots[i] = DpiSlot {
                dpi: raw_to_dpi(raw, profile.sensor),
                disabled: disabled_mask & (1 << i) != 0,
                // Read as stored: the device's bytes already are its colours.
                color: [
                    blob[COLOR_SLOT_BASE + i * 3],
                    blob[COLOR_SLOT_BASE + i * 3 + 1],
                    blob[COLOR_SLOT_BASE + i * 3 + 2],
                ],
            };
        }

        profile.raw_effect_selector = blob[53];
        profile.rgb_effect = RgbEffect::from_byte(blob[53]);
        profile.rgb_glorious_mode = blob[54];
        profile.rgb_glorious_direction = blob[55];
        profile.rgb_single_mode = blob[56];
        profile.rgb_single_color = [blob[57], blob[58], blob[59]];
        profile.rgb_breathing7_mode = blob[60];
        for i in 0..7 {
            profile.rgb_breathing7_colors[i] =
                [blob[61 + i * 3], blob[62 + i * 3], blob[63 + i * 3]];
        }
        // A short blob has no lift-off byte. Reading past the end would panic,
        // so the field keeps its default instead.
        if blob.len() > 125 {
            profile.lift_off_distance = blob[125];
        }
        Some(profile)
    }

    /// Render the profile into a writable blob of the given payload length.
    ///
    /// The buffer is always `CONFIG_REPORT_SIZE` long, because that is the
    /// transfer size. Byte 3 addresses the payload and therefore limits how
    /// much data can be written.
    pub fn serialize(&self, payload_len: usize) -> Result<Vec<u8>, String> {
        if !(MIN_CONFIG_SIZE..=CONFIG_REPORT_SIZE).contains(&payload_len) {
            return Err(format!(
                "payload length {payload_len} outside {MIN_CONFIG_SIZE}..={CONFIG_REPORT_SIZE}"
            ));
        }
        if payload_len - 8 > 0xff {
            return Err(format!(
                "payload of {payload_len} bytes cannot be addressed in one byte"
            ));
        }
        // The buffer is the full report, not the payload: the device rejects a
        // short transfer without reporting an error, so the size must match the
        // report exactly while only the leading `payload_len` bytes carry data.
        let mut out = vec![0u8; CONFIG_REPORT_SIZE];
        out[0] = REPORT_ID_CONFIG;
        out[1] = 0;
        out[3] = (payload_len - 8) as u8;
        out[9] = self.sensor;
        out[10] = (self.report_rate_raw & 0x0f)
            | if self.xy_independent {
                CONFIG_FLAG_XY_INDEPENDENT
            } else {
                0
            };
        out[11] = (self.dpi_count << 4) | (self.active_slot & 0x0f);

        let mut disabled_mask = 0u8;
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.disabled {
                disabled_mask |= 1 << i;
            }
            let raw = dpi_to_raw(slot.dpi, self.sensor);
            if self.xy_independent {
                out[13 + i * 2] = raw;
                out[14 + i * 2] = raw;
            } else {
                out[13 + i] = raw;
            }
            // Slot colours are stored in the order the device reads them, which
            // is what `parse` already produced, so they are written back as they
            // are. Swapping them here would turn every colour into a different
            // one on every save.
            let base = COLOR_SLOT_BASE + i * 3;
            out[base..base + 3].copy_from_slice(&slot.color);
        }
        out[12] = disabled_mask;

        // The lighting block, offsets as in the driver's config report:
        // effect selector, then one field per effect, then the colours.
        //
        // Byte 54 and byte 55 were both swept one value at a time, with a
        // photograph of the mouse after each, under the glorious effect. Byte
        // 54 moves the colour gradient along the mouse: 0 lays it out blue at
        // the front and green at the back, 128 reverses that, and 255 puts red
        // at the front. Under the solid effects it changes nothing, so the name
        // says what the driver calls it and the measurement says it is really
        // the direction of the gradient.
        //
        // Byte 55 made no visible difference at any of 0, 64, 128 and 255. Its
        // meaning is unknown and it is carried over for the same reason as
        // every other field here: writing a zero into a field whose meaning is
        // not known clears whatever the user had there, and the device accepts
        // that without comment.
        //
        // A selector this tool does not recognise keeps whatever the device
        // reported, because 0xff is what a mouse without LEDs stores and writing
        // it as zero would switch the lighting off on a device that merely said
        // it cannot do LEDs. `write_profile` merges onto the device's own blob,
        // so the value is put back into the one field that needs preserving.
        out[53] = match self.rgb_effect {
            Some(effect) => effect.as_byte(),
            None => self.raw_effect_selector,
        };
        out[54] = self.rgb_glorious_mode;
        out[55] = self.rgb_glorious_direction;
        out[56] = self.rgb_single_mode;
        out[57..60].copy_from_slice(&to_device_colour(self.rgb_single_color));
        // Byte 60 belongs to the seven colour breathing effect, and its upper
        // nibble is the brightness, the same packing as byte 56. Measured by
        // counting, out of eighteen photographs about a second apart, how many
        // showed the mouse lit: byte 60 at 0 lit one of them with a peak of
        // 1171 pixels, 0x40 lit two with a peak of 820, and 0xff lit four with a
        // peak of 1508. The lower nibble made no difference.
        //
        // Bytes 61 to 81 are the seven colours, and they are not understood.
        // Setting one of them to 255 and the rest to zero did not produce a
        // red breathing mouse: the effect cycled through blue and green, and
        // setting all twenty-one bytes to 255 produced a blue mouse rather than
        // a white one. Either the addresses are wrong or the effect takes its
        // colours from somewhere else. Until that is settled they are carried
        // over from the device untouched, which is also what keeps a profile
        // the user has set from being overwritten.
        out[60] = self.rgb_breathing7_mode;
        for i in 0..7 {
            let base = 61 + i * 3;
            out[base..base + 3].copy_from_slice(&to_device_colour(self.rgb_breathing7_colors[i]));
        }
        if payload_len > 125 {
            out[125] = self.lift_off_distance;
        }
        Ok(out)
    }
}
