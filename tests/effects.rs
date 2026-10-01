//! Tests for the two window effects: sparks under the pointer and a starburst on
//! a clicked control.
//!
//! A screenshot cannot settle either one. A trail lasts a frame or two and a
//! burst a few tenths of a second, so a picture of the window catches it or it
//! does not depending entirely on when the shutter fired. What is observable
//! from outside the engine is whether it still has work to do, because that is
//! what makes the window keep repainting, and how long that lasts.
//!
//! The engine deliberately does not expose its parts: a getter that exists only
//! for a test is a way of asserting on the implementation rather than on the
//! behaviour, and would be a second thing to keep in step with the first.

use eframe::egui;
use glorious::sparks::Sparks;

/// A throwaway `Ui` to draw into, since the effects need one to paint on.
fn scratch() -> egui::Context {
    egui::Context::default()
}

fn ui_for(ctx: &egui::Context) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        egui::Id::new("test-ui"),
        egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(800.0, 600.0),
        )),
    )
}

#[test]
fn moving_the_pointer_over_the_window_leaves_sparks() {
    let ctx = scratch();
    let mut sparks = Sparks::default();
    sparks.at_pointer(egui::pos2(100.0, 100.0), true, true);
    assert!(
        sparks.is_active(),
        "a pointer entering the window must leave something to draw"
    );
    sparks.draw(&mut ui_for(&ctx), 0.016);
}

#[test]
fn a_still_pointer_lays_nothing_new() {
    let ctx = scratch();
    let mut sparks = Sparks::default();
    // First visit lays the opening pair.
    sparks.at_pointer(egui::pos2(100.0, 100.0), true, true);
    // Then the pointer sits still for long enough for that pair to fade. Without
    // the movement check the engine would keep firing and the window would
    // repaint forever under a mouse that is not moving.
    for _ in 0..30 {
        sparks.at_pointer(egui::pos2(100.0, 100.0), false, true);
        sparks.draw(&mut ui_for(&ctx), 0.016);
    }
    assert!(
        !sparks.is_active(),
        "a pointer held still must stop producing work"
    );
}

#[test]
fn a_starburst_outlives_a_spark_and_then_stops() {
    let ctx = scratch();
    let mut sparks = Sparks::default();
    sparks.starburst(egui::pos2(200.0, 200.0));
    assert!(sparks.is_active(), "a click must leave a burst");

    // Long enough to be read as an event rather than as a flash, and no longer.
    // Both bounds are checked because a burst that never dies pins a repaint and
    // a burst that dies at once is a flicker.
    sparks.draw(&mut ui_for(&ctx), 0.1);
    assert!(
        sparks.is_active(),
        "the burst must still be visible a tenth of a second in"
    );
    for _ in 0..40 {
        sparks.draw(&mut ui_for(&ctx), 0.05);
    }
    assert!(
        !sparks.is_active(),
        "the burst must end, or the window never stops repainting"
    );
}

#[test]
fn a_drag_does_not_spark() {
    let mut sparks = Sparks::default();
    // The third argument is the hover flag, and it is false while the button is
    // down. Sparking through a drag would draw over the thing being dragged.
    sparks.at_pointer(egui::pos2(10.0, 10.0), true, false);
    assert!(
        !sparks.is_active(),
        "a drag is not a hover and must not lay sparks"
    );
}

#[test]
fn a_pointer_that_leaves_the_window_stops_the_trail_but_not_the_sparks() {
    let ctx = scratch();
    let mut sparks = Sparks::default();
    sparks.at_pointer(egui::pos2(50.0, 50.0), true, true);
    sparks.clear_pointer();
    // A spark already struck keeps going. Cutting it off when the pointer left
    // would make the effect stop at the window edge, which looks like a bug
    // rather than like a decision.
    assert!(
        sparks.is_active(),
        "a spark in flight survives the pointer leaving the window"
    );
    for _ in 0..40 {
        sparks.draw(&mut ui_for(&ctx), 0.05);
    }
    assert!(!sparks.is_active(), "and then it ends on its own");
}

#[test]
fn a_second_click_does_not_replace_the_first() {
    let ctx = scratch();
    let mut sparks = Sparks::default();
    sparks.starburst(egui::pos2(100.0, 100.0));
    // A short while later, but before the first burst has finished. The second
    // one has to add to it rather than reset it: an engine that started a new
    // burst by clearing would make two clicks in quick succession look like one,
    // which is the exact case where the second click is worth confirming.
    sparks.draw(&mut ui_for(&ctx), 0.15);
    let after_first = sparks.is_active();
    sparks.starburst(egui::pos2(300.0, 100.0));
    assert!(
        after_first && sparks.is_active(),
        "the first burst is still running when the second click lands"
    );
    for _ in 0..40 {
        sparks.draw(&mut ui_for(&ctx), 0.05);
    }
    assert!(!sparks.is_active(), "and both end afterwards");
}
