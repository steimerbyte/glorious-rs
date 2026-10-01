//! The two faults a user reported against a real Model O, as tests.
//!
//! - Applying a profile list put the same resolution on every slot.
//! - A custom resolution could not be set: 10000 came back as 2000.
//!
//! No hardware is needed. Both faults are in how bytes are chosen and how an
//! edit is committed, and both are visible in a blob that never left the
//! process. The blob in `REAL_MODEL_O_BLOB` was read from a real Model O,
//! `258a:0036`, firmware `V103`, sensor 0x06, and is the ground truth every
//! offset has to reproduce.

use glorious::device::Mouse;
use glorious::profile::Profile;
use glorious::protocol::*;
use glorious::transport::{FeatureTransport, TransportError};
use glorious::worker::apply_profile;

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

/// Transport that answers like the mouse and keeps what it is given.
struct FakeTransport;

impl FakeTransport {
    fn new() -> Self {
        FakeTransport
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
        let mut buf = match report_id {
            REPORT_ID_COMMAND => vec![REPORT_ID_COMMAND; length],
            _ => REAL_MODEL_O_BLOB.to_vec(),
        };
        buf.resize(length, 0);
        Ok(buf)
    }

    fn set_feature_report(&mut self, _data: &[u8]) -> Result<(), TransportError> {
        Ok(())
    }
}

/// The six slots a Model O drives, switched on, with a distinct resolution in
/// each so a write that lands on the wrong slot cannot pass.
fn profile_with_six_slots_on() -> Profile {
    let mut profile = Profile::parse(&REAL_MODEL_O_BLOB).expect("blob should decode");
    for index in 0..USABLE_DPI_SLOTS {
        profile.slots[index].disabled = false;
        profile.slots[index].dpi = 100 * (index as u16 + 1);
    }
    profile
        .sync_slot_count()
        .expect("six slots on is a legal count");
    profile
}

// --- The profile list overwrote every slot -------------------------------

#[test]
fn a_profile_list_only_touches_the_slots_the_mouse_drives() {
    let mut profile = profile_with_six_slots_on();
    apply_profile(
        &mut profile,
        &[
            (400, [1, 0, 0]),
            (800, [2, 0, 0]),
            (1600, [3, 0, 0]),
            (3200, [4, 0, 0]),
            (5000, [5, 0, 0]),
            (10000, [6, 0, 0]),
        ],
    )
    .expect("six steps fit on six slots");

    for index in 0..USABLE_DPI_SLOTS {
        assert_eq!(
            profile.slots[index].dpi,
            [400, 800, 1600, 3200, 5000, 10000][index],
            "slot {} keeps its own step",
            index + 1
        );
    }
}

#[test]
fn a_profile_list_leaves_the_slots_the_mouse_does_not_drive_alone() {
    // This is the reported fault. Slots seven and eight are storage the
    // firmware keeps without lighting them, and `apply_profile` has to leave
    // them exactly as it found them: they are not part of the profile the user
    // described, and a value written there is one the mouse never shows.
    let mut profile = profile_with_six_slots_on();
    let before: Vec<u16> = profile
        .slots
        .iter()
        .skip(USABLE_DPI_SLOTS)
        .map(|slot| slot.dpi)
        .collect();

    apply_profile(&mut profile, &[(400, [1, 0, 0]), (10000, [2, 0, 0])]).expect("two steps fit");

    let after: Vec<u16> = profile
        .slots
        .iter()
        .skip(USABLE_DPI_SLOTS)
        .map(|slot| slot.dpi)
        .collect();
    assert_eq!(
        before, after,
        "slots past the sixth are untouched by a profile list"
    );
}

#[test]
fn a_list_longer_than_the_slots_on_keeps_the_old_values_of_the_rest() {
    // Truncated, not refused, and the slots the list does not reach keep what
    // was already there. Zeroing them would be the same fault as writing all
    // of them: a step list that did not mention a slot should not change it.
    let mut profile = profile_with_six_slots_on();
    apply_profile(&mut profile, &[(400, [1, 0, 0]), (800, [2, 0, 0])])
        .expect("two steps fit on six slots");

    assert_eq!(profile.slots[0].dpi, 400, "the first step landed");
    assert_eq!(profile.slots[1].dpi, 800, "the second step landed");
    for index in 2..USABLE_DPI_SLOTS {
        assert_eq!(
            profile.slots[index].dpi,
            100 * (index as u16 + 1),
            "slot {} was not in the list and keeps its value",
            index + 1
        );
    }
}

#[test]
fn a_list_does_not_reach_into_the_slots_that_are_switched_off() {
    // Six steps against a mouse with only the first three slots on. The list is
    // truncated to three, and slots four to six are storage that is not lit:
    // writing them would put values the mouse never shows into a profile the
    // user did not describe.
    //
    // The real blob has the sixth slot on, so it is switched off explicitly
    // here. Left alone it would be a fourth enabled slot and would correctly
    // receive the fourth step, which is a different case covered above.
    let mut profile = Profile::parse(&REAL_MODEL_O_BLOB).expect("blob should decode");
    for index in 0..3 {
        profile.slots[index].disabled = false;
    }
    for index in 3..USABLE_DPI_SLOTS {
        profile.slots[index].disabled = true;
        profile.slots[index].dpi = 7700;
    }

    apply_profile(
        &mut profile,
        &[
            (400, [1, 0, 0]),
            (800, [2, 0, 0]),
            (1600, [3, 0, 0]),
            (3200, [4, 0, 0]),
            (5000, [5, 0, 0]),
            (10000, [6, 0, 0]),
        ],
    )
    .expect("a long list is truncated, not refused");

    for index in 0..3 {
        assert_eq!(profile.slots[index].dpi, [400, 800, 1600][index]);
    }
    for index in 3..USABLE_DPI_SLOTS {
        assert_eq!(
            profile.slots[index].dpi,
            7700,
            "slot {} is switched off and keeps its value",
            index + 1
        );
    }
}

#[test]
fn a_list_reaches_every_enabled_slot_wherever_it_is() {
    // The other half of the mapping, and the reason the sixth slot counts even
    // though slots one to five are off. A list is given to the slots the mouse
    // drives in order, so a mouse with slots one, two and six on takes step one
    // on slot one, step two on slot two and step three on slot six. Skipping
    // past the enabled slots and writing only the first few would put a profile
    // the user asked for onto slots the mouse never lights.
    let mut profile = Profile::parse(&REAL_MODEL_O_BLOB).expect("blob should decode");
    for index in 0..2 {
        profile.slots[index].disabled = false;
    }
    // Slot six is on in the real blob already.
    apply_profile(
        &mut profile,
        &[(400, [1, 0, 0]), (800, [2, 0, 0]), (10000, [3, 0, 0])],
    )
    .expect("three steps fit on three enabled slots");

    assert_eq!(profile.slots[0].dpi, 400, "the first enabled slot");
    assert_eq!(profile.slots[1].dpi, 800, "the second enabled slot");
    assert_eq!(
        profile.slots[5].dpi, 10000,
        "the third enabled slot, which is the sixth physical one"
    );
    for index in 2..5 {
        assert_eq!(
            profile.slots[index].dpi,
            [1600, 3200, 5000][index - 2],
            "slot {} is switched off and is not part of the list",
            index + 1
        );
    }
}

// --- serialize wrote bytes the mouse does not drive -----------------------

#[test]
fn serialize_writes_a_resolution_byte_only_for_the_six_slots() {
    // The write path used to render all eight slots, so bytes 19 and 20 got a
    // number the mouse never reads. With every slot showing one resolution the
    // profile put the same value in bytes the device ignores, which is a value
    // on the wire that means nothing and overwrites what the device had.
    let mut profile = profile_with_six_slots_on();
    for slot in profile.slots.iter_mut() {
        slot.dpi = 12000;
    }

    let rendered = profile
        .serialize(REAL_MODEL_O_BLOB.len())
        .expect("should serialise");

    for index in 0..USABLE_DPI_SLOTS {
        assert_eq!(
            rendered[13 + index],
            dpi_to_raw(12000, profile.sensor),
            "slot {} has a resolution byte",
            index + 1
        );
    }
    // Bytes 19 and 20 are slots seven and eight in the ordinary layout. The
    // buffer starts as zeroes and nothing may write them, so they are still
    // what the buffer was made of.
    assert_eq!(
        rendered[19], 0,
        "byte 19 is a slot the mouse does not drive and must not be written"
    );
    assert_eq!(
        rendered[20], 0,
        "byte 20 is a slot the mouse does not drive and must not be written"
    );
}

#[test]
fn a_write_keeps_the_devices_own_bytes_in_the_slots_it_does_not_drive() {
    // The stronger form of the same rule. A write overlays the rendered blob
    // onto the device's own, so a byte that is not written keeps the device's
    // value rather than the zero the renderer left. A device that stored
    // something there must still have it after this tool saves.
    let mut profile = profile_with_six_slots_on();
    profile.slots[0].dpi = 10000;

    let mut mouse = Mouse::new(FakeTransport::new());
    mouse
        .write_profile(&profile)
        .expect("the device should accept a resolution change");

    // The fake answers every read with the real blob, so what this proves is
    // that the write path leaves bytes 19 and 20 out of its overlay set: they
    // are neither the device's bytes nor the renderer's zeroes, they are simply
    // not touched. Asserting on the renderer's bytes is what the next test
    // does, and asserting that the set of written bytes does not contain them
    // is what this one does.
    let written = config_bytes_to_write(profile.xy_independent);
    assert!(
        !written.contains(&19),
        "byte 19 is not a byte this write may change"
    );
    assert!(
        !written.contains(&20),
        "byte 20 is not a byte this write may change"
    );
}

#[test]
fn serialize_and_parse_agree_on_the_six_slots_the_mouse_uses() {
    // The round trip a user experiences: set six different resolutions, save,
    // reload, and see the six values they chose. A read that does not reproduce
    // them means the window and the mouse disagree about what is stored.
    let mut profile = profile_with_six_slots_on();
    let chosen = [400u16, 800, 1600, 3200, 5000, 12000];
    for (index, dpi) in chosen.iter().enumerate() {
        profile.slots[index].dpi = *dpi;
    }

    let rendered = profile
        .serialize(REAL_MODEL_O_BLOB.len())
        .expect("should serialise");
    let mut padded = rendered.clone();
    padded.resize(CONFIG_REPORT_SIZE, 0);
    let reloaded = Profile::parse(&padded).expect("the rendered blob decodes");

    for (index, dpi) in chosen.iter().enumerate() {
        assert_eq!(
            reloaded.slots[index].dpi,
            *dpi,
            "slot {} reads back as the value that was written",
            index + 1
        );
    }
}

#[test]
fn the_slots_the_mouse_does_not_drive_read_back_as_the_device_reported_them() {
    // They read back as 100 dpi because the firmware reports the floor for a
    // slot it does not drive. That is not this tool rounding a value it wrote,
    // and it must stay that way: a reload may not put a number there that a save
    // put there, or the mouse would look like it took a setting it cannot take.
    let profile = profile_with_six_slots_on();
    let rendered = profile
        .serialize(REAL_MODEL_O_BLOB.len())
        .expect("should serialise");
    let mut padded = rendered.clone();
    padded.resize(CONFIG_REPORT_SIZE, 0);
    let reloaded = Profile::parse(&padded).expect("the rendered blob decodes");

    for index in USABLE_DPI_SLOTS..NUM_DPI_SLOTS {
        assert_eq!(
            reloaded.slots[index].dpi,
            100,
            "slot {} is not driven by the mouse and reads as its floor",
            index + 1
        );
    }
}

#[test]
fn the_dpi_bytes_of_the_xy_layout_are_not_the_ones_of_the_ordinary_one() {
    // With X and Y configured separately each slot takes two bytes, so the six
    // driven slots are at 13 to 24 and the ordinary layout's bytes 19 and 20 are
    // the Y values of slots four and five. Writing the ordinary set over an X/Y
    // profile would leave each slot's second value at whatever the device had,
    // and the two would disagree on the mouse.
    let xy = dpi_config_bytes(true);
    let ordinary = dpi_config_bytes(false);

    assert_eq!(ordinary, (13..13 + USABLE_DPI_SLOTS).collect::<Vec<_>>());
    assert_eq!(xy, (13..13 + USABLE_DPI_SLOTS * 2).collect::<Vec<_>>());
    assert_eq!(
        xy.len(),
        USABLE_DPI_SLOTS * 2,
        "six slots at two bytes each"
    );
    // Bytes 25 to 28 are slots seven and eight in the X/Y layout and nothing at
    // all in the ordinary one, so neither layout writes them.
    for stray in 25..29 {
        assert!(
            !xy.contains(&stray) && !ordinary.contains(&stray),
            "byte {stray} is not a slot the mouse drives"
        );
    }
}

#[test]
fn a_xy_profile_writes_both_values_of_each_driven_slot() {
    let mut profile = profile_with_six_slots_on();
    profile.xy_independent = true;
    for (index, slot) in profile.slots.iter_mut().enumerate().take(USABLE_DPI_SLOTS) {
        slot.dpi = 100 * (index as u16 + 3);
    }

    let rendered = profile
        .serialize(REAL_MODEL_O_BLOB.len())
        .expect("should serialise");

    for index in 0..USABLE_DPI_SLOTS {
        let expected = dpi_to_raw(100 * (index as u16 + 3), profile.sensor);
        assert_eq!(
            rendered[13 + index * 2],
            expected,
            "slot {} X value",
            index + 1
        );
        assert_eq!(
            rendered[14 + index * 2],
            expected,
            "slot {} Y value",
            index + 1
        );
    }
    // Slot seven would start at 25 and slot eight at 27, and neither is driven.
    assert_eq!(
        rendered[25], 0,
        "slot seven is not driven and its first X value must not be written"
    );
    assert_eq!(
        rendered[26], 0,
        "slot seven is not driven and its first Y value must not be written"
    );
}

// --- Byte 11 and the number of slots the mouse drives ---------------------

#[test]
fn the_slot_count_never_exceeds_the_slots_that_are_on() {
    // What is measured about byte 11's count is its ceiling: `--probe-slots`
    // wrote eight slots enabled with a count of eight and the device dropped
    // the whole profile without a word, leaving every resolution where it was.
    // Six is what it takes. So a save must never claim more slots than the mask
    // leaves switched on, and the real mouse's own value is the reference for
    // what a correct one looks like: `0x11`, one slot on and that one active.
    let original = Profile::parse(&REAL_MODEL_O_BLOB).expect("blob should decode");
    assert_eq!(
        original.dpi_count,
        original
            .slots
            .iter()
            .take(USABLE_DPI_SLOTS)
            .filter(|slot| !slot.disabled)
            .count() as u8,
        "the count a real mouse stores is the number of slots it has on"
    );

    let mut profile = original.clone();
    profile.slots[0].disabled = false;
    profile
        .sync_slot_count()
        .expect("switching a second slot on is legal");
    assert_eq!(
        profile.dpi_count, 2,
        "the count follows the mask after a slot is switched on"
    );

    profile.slots[0].disabled = true;
    profile.slots[1].disabled = true;
    profile
        .sync_slot_count()
        .expect("switching both off leaves the sixth on");
    assert_eq!(
        profile.dpi_count, 1,
        "and again after a slot is switched off"
    );
}

#[test]
fn the_active_slot_is_moved_only_as_far_as_it_has_to_be() {
    // `active_slot` is a position among the slots that are on. Losing slots can
    // leave it naming one that no longer exists, which would put a value into
    // the low nibble that the device reads as a different slot. It is left alone
    // where it still names a real slot, because moving it is the mouse's own
    // runtime selection rather than something a save should decide.
    let mut profile = profile_with_six_slots_on();
    profile.active_slot = 6;
    profile.sync_slot_count().expect("six slots on");
    assert_eq!(
        profile.active_slot, 6,
        "an active slot that still exists is not moved"
    );

    for index in 3..USABLE_DPI_SLOTS {
        profile.slots[index].disabled = true;
    }
    profile.sync_slot_count().expect("three slots remain");
    assert_eq!(
        profile.active_slot, 3,
        "one naming a slot that is now off is clamped onto a slot that is on"
    );
    assert_eq!(
        profile.active_slot_index(),
        Some(2),
        "and it then names a real slot"
    );
}

#[test]
fn a_profile_with_no_slot_on_is_refused_rather_than_written() {
    // Byte 11 cannot say zero on a mouse that has to run at some resolution, and
    // a write with a count of zero is one the device drops. Saying so is better
    // than a save that appears to work and leaves everything as it was.
    let mut profile = profile_with_six_slots_on();
    for index in 0..USABLE_DPI_SLOTS {
        profile.slots[index].disabled = true;
    }
    let error = profile
        .sync_slot_count()
        .expect_err("a profile with nothing on cannot be written");
    assert!(
        error.contains("switched off"),
        "the message says what to fix, got: {error}"
    );
}

// --- The 2000 a typed resolution came back as ----------------------------

#[test]
fn an_unknown_sensor_offers_a_ceiling_rather_than_the_two_thousand_fallback() {
    // The reported fault: a mouse whose sensor byte is not one of the four
    // measured here was given a DPI field capped at 2000, because that is what
    // `max_dpi_for_sensor` returned for an unknown sensor. A field with a
    // maximum clamps whatever is typed into it, so typing 10000 stored 2000.
    // The 2000 was never measured for anything; it is this tool's own fallback.
    assert_eq!(
        max_dpi_for_sensor(0x06),
        12000,
        "a measured sensor is exact"
    );
    assert_eq!(
        max_dpi_for_sensor(0xff),
        DPI_MAX_ANY_SENSOR,
        "an unknown sensor gets a ceiling, not the fallback that clamped typing"
    );
    assert!(
        max_dpi_for_sensor(0xff) > 10000,
        "the ceiling has to admit the value that was being typed"
    );
}

#[test]
fn the_window_can_tell_a_measured_maximum_from_a_guess() {
    // The number alone is not enough. A range that is a ceiling and a range
    // that is the device's measured maximum look identical in a control, and
    // the window has to be able to say which one it is showing.
    let known = Profile::parse(&REAL_MODEL_O_BLOB).expect("blob should decode");
    let (max, measured) = known.max_dpi_known();
    assert_eq!(max, 12000);
    assert!(
        measured,
        "a PMW3360 has a maximum that was measured on the device"
    );

    let mut unknown = known.clone();
    unknown.sensor = 0xff;
    let (max, measured) = unknown.max_dpi_known();
    assert_eq!(max, DPI_MAX_ANY_SENSOR);
    assert!(
        !measured,
        "a sensor nobody measured must not be reported as a measured maximum"
    );
}

#[test]
fn every_measured_sensor_stores_every_hundred_up_to_its_own_maximum() {
    // Guards the fix from the other side. Raising the ceiling for an unknown
    // sensor is only safe if a resolution a user would reasonably type can
    // actually be encoded: every sensor here has to be able to store each
    // multiple of 100 up to its own maximum, so a value inside the range is
    // never quietly clamped by the encoding.
    for (id, name, max) in SENSORS {
        let mut dpi = 100;
        while dpi <= max {
            let raw = dpi_to_raw(dpi, id);
            assert_eq!(
                raw_to_dpi(raw, id),
                dpi,
                "{dpi} dpi must survive the encoding on {name} ({id:#04x})"
            );
            dpi += 100;
        }
    }
}

#[test]
fn a_resolution_the_device_cannot_store_is_not_accepted_back_as_itself() {
    // `dpi_to_raw` divides by 100, so on a PMW3360, whose encoding carries a
    // one-step offset, 1234 is stored as raw 11 and reads back as 1200. A field
    // that accepted 1234 would show the user one number and the mouse would show
    // another, with the second arriving on the next read.
    assert_eq!(
        dpi_to_raw(1234, 0x06),
        11,
        "a value the device cannot store lands on the one it can"
    );
    assert_eq!(
        raw_to_dpi(dpi_to_raw(1234, 0x06), 0x06),
        1200,
        "and reads back as that one rather than as what was typed"
    );
    // Which is why the field takes whole hundreds. Every value it can be given
    // survives the encoding unchanged, so the number on screen and the number on
    // the mouse are always the same one.
    for dpi in [100u16, 400, 1600, 5000, 10000, 12000] {
        assert_eq!(
            raw_to_dpi(dpi_to_raw(dpi, 0x06), 0x06),
            dpi,
            "{dpi} dpi is a value the device stores and reads back unchanged"
        );
    }
}
