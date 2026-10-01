//! Protocol tests. No hardware needed.
//!
//! The blob in `REAL_MODEL_O_BLOB` was read from a real Glorious Model O,
//! `258a:0036`, firmware `V103`, with the vendor software defaults. It is the
//! ground truth every offset in this crate has to reproduce.

use glorious::device::{DeviceState, Mouse};
use glorious::profile::{DpiSlot, Profile};
use glorious::protocol::*;
use glorious::transport::{FeatureTransport, TransportError};

/// Configuration blob of a real Model O: sensor 0x06, 1000 Hz, only the sixth
/// slot enabled at 10000 dpi, exactly the vendor defaults.
const REAL_MODEL_O_BLOB: [u8; 130] = [
    0x04, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x06, 0x04, 0x11, 0xdf, 0x03, 0x07, 0x0f,
    0x1f, 0x31, 0x63, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00,
    0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0xff, 0x00, 0xff, 0x00, 0xff, 0xff, 0x00, 0xff, 0xff,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x41, 0x00, 0x40, 0xff, 0x00, 0x00, 0x42, 0x07, 0xff, 0x00,
    0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0xff, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0xff, 0x00, 0xff,
    0xff, 0xff, 0xff, 0xff, 0x42, 0x43, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x42, 0xff, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x42, 0x02, 0xff, 0x00,
    0x00, 0x01,
];

/// Transport that replays canned replies keyed by command.
struct FakeTransport {
    command_replies: Vec<(u8, Vec<u8>)>,
    config_reply: Vec<u8>,
    writes: Vec<Vec<u8>>,
}

impl FakeTransport {
    fn new() -> Self {
        let mut command_replies = Vec::new();
        let mut firmware = vec![REPORT_ID_COMMAND, CMD_FIRMWARE_VERSION];
        firmware.extend_from_slice(b"V103");
        command_replies.push((CMD_FIRMWARE_VERSION, firmware));

        let mut profile = vec![REPORT_ID_COMMAND, CMD_PROFILE, 1];
        profile.resize(COMMAND_SIZE, 0);
        command_replies.push((CMD_PROFILE, profile));

        // The device keeps the debounce value halved.
        let mut debounce = vec![REPORT_ID_COMMAND, CMD_DEBOUNCE, 4];
        debounce.resize(COMMAND_SIZE, 0);
        command_replies.push((CMD_DEBOUNCE, debounce));

        FakeTransport {
            command_replies,
            config_reply: REAL_MODEL_O_BLOB.to_vec(),
            writes: Vec::new(),
        }
    }
}

impl FeatureTransport for FakeTransport {
    fn vendor_id(&self) -> u16 {
        0x258a
    }

    fn product_id(&self) -> u16 {
        0x0036
    }

    fn get_feature_report(
        &mut self,
        report_id: u8,
        length: usize,
    ) -> Result<Vec<u8>, TransportError> {
        let payload = if report_id == REPORT_ID_COMMAND {
            let query = self.writes.last().and_then(|w| w.get(1).copied());
            self.command_replies
                .iter()
                .find(|(cmd, _)| Some(*cmd) == query)
                .map(|(_, reply)| reply.clone())
                .unwrap_or_else(|| vec![REPORT_ID_COMMAND; length])
        } else {
            self.config_reply.clone()
        };
        let mut buf = payload;
        buf.resize(length, 0);
        Ok(buf)
    }

    fn set_feature_report(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.writes.push(data.to_vec());
        // A real device stores what it receives, so reads after a write must
        // return the written bytes rather than the original ones.
        if data.first() == Some(&REPORT_ID_CONFIG) {
            self.config_reply = data.to_vec();
        }
        Ok(())
    }
}

#[test]
fn decodes_the_real_mouse_blob() {
    let profile = Profile::parse(&REAL_MODEL_O_BLOB).expect("blob should decode");
    assert_eq!(profile.sensor, 0x06, "PMW3360");
    assert_eq!(profile.sensor_name(), "PMW3360");
    assert_eq!(profile.report_rate(), 1000);
    // Byte 11 of the real device is 0x11: one slot counted as active.
    assert_eq!(profile.dpi_count, 1);
    assert_eq!(profile.active_slot, 1);
    // Vendor defaults from Cfg.ini: 400, 800, 1600, 3200, 5000, 10000.
    let dpis: Vec<u16> = profile.slots.iter().map(|s| s.dpi).collect();
    assert_eq!(dpis, vec![400, 800, 1600, 3200, 5000, 10000, 100, 100]);
}

#[test]
fn active_slot_counts_enabled_slots_not_physical_ones() {
    let profile = Profile::parse(&REAL_MODEL_O_BLOB).expect("blob should decode");
    // Only the sixth slot is enabled, and the device says "active 1".
    assert_eq!(profile.active_slot, 1);
    assert_eq!(profile.active_slot_index(), Some(5));
    assert_eq!(profile.active_dpi(), 10000);
}

#[test]
fn disabled_mask_matches_the_vendor_default() {
    let profile = Profile::parse(&REAL_MODEL_O_BLOB).expect("blob should decode");
    let disabled: Vec<bool> = profile.slots.iter().map(|s| s.disabled).collect();
    assert_eq!(
        disabled,
        vec![true, true, true, true, true, false, true, true],
        "only slot 6 is enabled by default"
    );
}

#[test]
fn dpi_encoding_is_sensor_specific() {
    // PMW3360 starts at raw 3, which is the vendor software's lowest step.
    assert_eq!(dpi_to_raw(400, 0x06), 3);
    assert_eq!(raw_to_dpi(3, 0x06), 400);
    // PMW3389 has no offset.
    assert_eq!(dpi_to_raw(800, 0x0f), 8);
    assert_eq!(raw_to_dpi(8, 0x0f), 800);
}

#[test]
fn dpi_round_trips_over_the_whole_sensor_range() {
    for (sensor, _, max) in SENSORS {
        let mut dpi = 100;
        while dpi <= max {
            assert_eq!(
                raw_to_dpi(dpi_to_raw(dpi, sensor), sensor),
                dpi,
                "{dpi} on {sensor:#04x}"
            );
            dpi += 100;
        }
    }
}

#[test]
fn serialisation_round_trips_the_real_blob() {
    let original = Profile::parse(&REAL_MODEL_O_BLOB).expect("blob should decode");
    let mut changed = original.clone();
    changed.slots[5].dpi = 3200;

    let rendered = changed
        .serialize(REAL_MODEL_O_BLOB.len())
        .expect("should serialise");
    let mut padded = rendered.clone();
    padded.resize(CONFIG_REPORT_SIZE, 0);
    let restored = Profile::parse(&padded).expect("rendered blob should decode");

    assert_eq!(restored.slots[5].dpi, 3200);
    assert_eq!(restored.active_slot_index(), Some(5));
    assert_eq!(restored.report_rate(), original.report_rate());
    assert_eq!(restored.sensor, original.sensor);
    for (left, right) in restored.slots.iter().zip(original.slots.iter()) {
        assert_eq!(left.disabled, right.disabled);
        assert_eq!(left.color, right.color);
    }
}

#[test]
fn undecodable_blobs_are_rejected() {
    assert!(Profile::parse(&[]).is_none());
    assert!(Profile::parse(&[0u8; 10]).is_none());
    let mut wrong_id = REAL_MODEL_O_BLOB.to_vec();
    wrong_id[0] = 0x09;
    assert!(Profile::parse(&wrong_id).is_none());
}

#[test]
fn payload_length_addressable_in_one_byte_is_enforced() {
    let profile = Profile::default();
    // Byte 3 holds length - 8, so anything above 263 cannot be addressed.
    assert!(profile.serialize(264).is_err());
    assert!(profile.serialize(123).is_ok());
    assert!(profile.serialize(263).is_ok());
}

#[test]
fn reads_the_mouse_through_the_command_sequence() {
    let transport = FakeTransport::new();
    let mut mouse = Mouse::new(transport);
    let state = mouse.state(true).expect("read should succeed").clone();
    assert_device_matches_real_mouse(&state);
}

#[test]
fn a_reused_read_does_not_hit_the_device_again() {
    let mut mouse = Mouse::new(FakeTransport::new());
    mouse.state(true).expect("first read");
    let before = mouse.state(false).expect("cached read").clone();
    let after_first = mouse.transport.writes.len();
    let again = mouse.state(false).expect("second cached read").clone();

    assert_eq!(before.firmware, again.firmware);
    assert_eq!(before.profile.active_dpi(), again.profile.active_dpi());
    assert_eq!(
        mouse.transport.writes.len(),
        after_first,
        "a cached read must not send anything"
    );
}

fn assert_device_matches_real_mouse(state: &DeviceState) {
    assert_eq!(state.firmware, "V103");
    assert_eq!(state.vendor_id, 0x258a);
    assert_eq!(state.product_id, 0x0036);
    assert_eq!(state.name, "Glorious Model O / O- (wired)");
    assert_eq!(state.profile.sensor_name(), "PMW3360");
    assert_eq!(state.report_rate(), 1000);
    assert_eq!(state.profile.active_dpi(), 10000);
    // The device stores 8 ms as 4.
    assert_eq!(state.debounce_ms, Some(8));
    assert_eq!(state.active_profile, 1);
}

#[test]
fn debounce_is_rejected_outside_the_vendor_selection() {
    let mut mouse = Mouse::new(FakeTransport::new());
    assert!(mouse.set_debounce(9).is_err(), "9 ms is not offered");
    assert!(mouse.set_debounce(5).is_err(), "5 ms is not offered");
    assert!(mouse.set_debounce(8).is_ok());
}

#[test]
fn writing_a_profile_sends_only_the_blob() {
    let transport = FakeTransport::new();
    let mut mouse = Mouse::new(transport);
    mouse.state(true).expect("read should succeed");

    let mut profile = mouse.state(false).expect("cached").profile.clone();
    profile.slots[5].dpi = 1600;
    mouse.write_profile(&profile).expect("write should succeed");

    let writes = &mouse.transport.writes;
    let command_writes: Vec<u8> = writes
        .iter()
        .filter(|w| w[0] == REPORT_ID_COMMAND)
        .map(|w| w[1])
        .collect();
    // Measured order: firmware, profile, config, debounce, then the re-read
    // that a write starts with so unknown fields are not dropped. There is no
    // command report between the re-read and the blob: the firmware treats a
    // blob that follows one as the answer to that command and drops it.
    assert_eq!(
        command_writes,
        vec![
            CMD_FIRMWARE_VERSION,
            CMD_PROFILE,
            CMD_GET_CONFIG,
            CMD_DEBOUNCE,
            CMD_GET_CONFIG,
        ],
        "reading needs the config command before the blob, and writing re-reads first"
    );

    let config_writes: Vec<&Vec<u8>> = writes.iter().filter(|w| w[0] == REPORT_ID_CONFIG).collect();
    assert_eq!(config_writes.len(), 1, "exactly one blob is pushed back");
    let pushed = config_writes[0];
    // The transfer is always the full report size: a short buffer is rejected
    // by the device without an error, so the length must be exact.
    assert_eq!(
        pushed.len(),
        CONFIG_REPORT_SIZE,
        "a short blob is silently dropped by the device"
    );
    assert_eq!(pushed[1], CMD_GET_CONFIG, "command id, not zero");
    assert_eq!(
        pushed[3],
        (REAL_MODEL_O_BLOB.len() - 8) as u8,
        "payload length"
    );
    assert_eq!(
        pushed[13 + 5],
        dpi_to_raw(1600, 0x06),
        "new dpi is in the blob"
    );
    assert_eq!(
        &pushed[REAL_MODEL_O_BLOB.len()..],
        &vec![0u8; CONFIG_REPORT_SIZE - REAL_MODEL_O_BLOB.len()][..],
        "the tail beyond the payload is zero"
    );
}

#[test]
fn writing_clears_the_cache_so_the_next_read_is_real() {
    let mut mouse = Mouse::new(FakeTransport::new());
    mouse.state(true).expect("read should succeed");
    let profile = mouse.state(false).expect("cached").profile.clone();
    mouse.write_profile(&profile).expect("write should succeed");
    // After a write the next read must go to the device, not the old cache.
    let state = mouse.state(false).expect("read after write");
    assert_eq!(state.profile.active_dpi(), 10000);
}

#[test]
fn a_blob_for_the_wrong_command_is_refused() {
    let mut transport = FakeTransport::new();
    transport.config_reply[1] = 0x99;
    let mut mouse = Mouse::new(transport);
    assert!(
        mouse.state(true).is_err(),
        "a blob answering another command must not be trusted"
    );
}

#[test]
fn every_effect_survives_the_round_trip_through_its_byte() {
    // Each value was confirmed on a real mouse by writing it and photographing
    // the result, so the byte and the name have to stay paired.
    let expected = [
        (0x00, RgbEffect::Off),
        (0x01, RgbEffect::Glorious),
        (0x02, RgbEffect::Single),
        (0x03, RgbEffect::Breathing7),
        (0x04, RgbEffect::Tail),
        (0x05, RgbEffect::Breathing),
        (0x06, RgbEffect::Constant),
        (0x07, RgbEffect::Rave),
        (0x08, RgbEffect::Random),
        (0x09, RgbEffect::Wave),
        (0x0a, RgbEffect::Breathing1),
    ];
    for (byte, effect) in expected {
        assert_eq!(
            effect.as_byte(),
            byte,
            "{} has the wrong byte",
            effect.name()
        );
        assert_eq!(RgbEffect::from_byte(byte), Some(effect), "byte {byte:#04x}");
    }
    assert_eq!(
        RgbEffect::from_byte(0xff),
        None,
        "the no-LED value is not an effect and must not be offered"
    );
}

#[test]
fn an_unknown_effect_byte_is_not_written_back_as_zero() {
    // 0xff is what a mouse without LEDs reports, and the driver warns it is not
    // a constant. Writing it back as zero would switch the lighting off on a
    // device that was merely reporting it cannot do LEDs.
    let mut profile = Profile::parse(&REAL_MODEL_O_BLOB).expect("blob should decode");
    profile.rgb_effect = None;
    let rendered = profile
        .serialize(REAL_MODEL_O_BLOB.len())
        .expect("should serialise");
    assert_eq!(
        rendered[53], REAL_MODEL_O_BLOB[53],
        "the device's own selector is kept when the value is not understood"
    );
}

#[test]
fn mode_bytes_split_into_speed_and_brightness() {
    assert_eq!(rgb_mode_decode(0x13), (3, 1));
    assert_eq!(rgb_mode_encode(3, 1), 0x13);
}

#[test]
fn default_slot_is_usable_without_a_device() {
    let slot = DpiSlot::default();
    assert_eq!(slot.dpi, 800);
    assert!(!slot.disabled);
}

#[test]
fn setting_debounce_writes_the_halved_value() {
    let mut mouse = Mouse::new(FakeTransport::new());
    mouse.state(true).expect("read should succeed");
    let before = mouse.transport.writes.len();

    mouse.set_debounce(6).expect("6 ms is offered");

    let sent = &mouse.transport.writes[before];
    assert_eq!(sent[0], REPORT_ID_COMMAND);
    assert_eq!(sent[1], CMD_DEBOUNCE);
    assert_eq!(sent[2], 3, "the device stores the value halved");
    assert_eq!(sent.len(), COMMAND_SIZE);
}

#[test]
fn a_write_leaves_bytes_it_does_not_model_alone() {
    // The lighting block is only partly mapped: bytes 53 to 59 are understood,
    // but 54 to 56 and 60 onwards are still a guess. A colour change must not
    // clear those. The device accepts a blob with zeros there and gives no hint
    // that a setting was lost, so this is the failure that cost this project's
    // first colour change.
    let before = REAL_MODEL_O_BLOB;
    let original = Profile::parse(&before).expect("blob should decode");
    let mut changed = original.clone();
    changed.slots[5].color = [1, 2, 3];

    let mut mouse = Mouse::new(FakeTransport::new());
    mouse
        .write_profile(&changed)
        .expect("the device should accept a colour change");

    // Read the blob back out of the fake device to see what it received.
    let blob = mouse.read_config(0x11).expect("read back what was written");
    // Slot colours are stored in the device's own order, which is what `parse`
    // produced, so they go back out unchanged. Swapping them on write as well as
    // on read would change every colour on every save.
    assert_eq!(
        &blob[29 + 5 * 3..29 + 5 * 3 + 3],
        &[1, 2, 3],
        "the colour that was changed is written as read",
    );
    assert_eq!(
        &blob[8..9],
        &before[8..9],
        "byte 8 holds a value this tool does not know, keep it"
    );
    // The transfer is 520 bytes, the payload only 130, so the comparison has
    // to stop at the end of the payload. The padding after it is always zero.
    for index in 54..=56 {
        assert_eq!(
            blob[index], before[index],
            "byte {index} is not mapped yet and must be carried over"
        );
    }
    assert_eq!(
        &blob[60..before.len()],
        &before[60..],
        "the lighting block from byte 60 on must survive a colour change"
    );

    let after = Profile::parse(&blob).expect("decode what came back");
    assert_eq!(after.rgb_effect, original.rgb_effect, "effect survives");
    assert_eq!(
        after.rgb_breathing7_colors, original.rgb_breathing7_colors,
        "breathing colours survive"
    );
}
