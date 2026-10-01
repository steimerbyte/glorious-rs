//! Which row of the DPI table the window marks as the live one.
//!
//! The marking is the one place where the window states something about the
//! device that the device did not report on request: the active slot comes out
//! of byte 11, but the grid has a header row above the slots and an off-by-one
//! there marks the header instead of a slot, or marks slot 1 twice.

use glorious::ui::active_row;

#[test]
fn the_header_is_never_marked() {
    // Row zero is the header, whatever the device says is active.
    for active in 0..8 {
        assert!(!active_row(0, Some(active)), "row 0 is the header");
    }
    assert!(!active_row(0, None));
}

#[test]
fn slot_n_sits_on_row_n_plus_one() {
    for slot in 0..6 {
        assert!(
            active_row(slot + 1, Some(slot)),
            "slot {slot} belongs on row {}",
            slot + 1
        );
    }
}

#[test]
fn exactly_one_row_is_marked_and_it_is_the_right_one() {
    for active in 0..6 {
        let marked: Vec<usize> = (0..7)
            .filter(|row| active_row(*row, Some(active)))
            .collect();
        assert_eq!(
            marked,
            vec![active + 1],
            "active slot {active} marked the wrong rows"
        );
    }
}

#[test]
fn no_active_slot_marks_nothing() {
    // The device can report a byte 11 whose low nibble names no enabled slot.
    // Marking slot 1 then would be showing a resolution the mouse is not using.
    for row in 0..7 {
        assert!(
            !active_row(row, None),
            "row {row} marked with no active slot"
        );
    }
}

#[test]
fn an_active_slot_outside_the_table_marks_nothing() {
    // Slots seven and eight are storage the firmware keeps and does not drive.
    // If the byte names one, the window has nothing to point at, and pointing
    // at a row that is not there would be a mark on the wrong slot.
    for slot in 6..12 {
        assert!(
            (0..7).all(|row| !active_row(row, Some(slot))),
            "active slot {slot} is not in the table but marked a row"
        );
    }
}
