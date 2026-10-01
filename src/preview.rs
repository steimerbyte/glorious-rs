//! A preview of what the mouse will look like, drawn from the profile.
//!
//! The device does not confirm a write and reading the blob back does not show
//! whether it took effect, so a user setting a colour has no way to know what
//! they asked for until they look at the mouse. This draws the answer in the
//! window instead, from the same bytes the write is built from.
//!
//! What it can honestly show is limited by what the protocol is understood to
//! be. The strip and the slot colours are the same bytes the mouse reads, so
//! those are shown as they are. The effect name is a selector this project
//! decoded, and the animation is a guess at what that selector does, drawn as
//! motion rather than as a claim: the one thing measured about several of these
//! effects is that they do not sit still, and not how they move.

use crate::protocol::{RgbEffect, USABLE_DPI_SLOTS};
use crate::theme::slot_colour;
use eframe::egui;
use eframe::egui::{Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};

/// Height of the drawn strip, in points. Wide and thin, because the lit part of
/// a Model O is a line along the top edge rather than a panel.
const STRIP_HEIGHT: f32 = 22.0;

/// Corner radius of the strip, small enough to read as the edge of a mouse
/// rather than as a rounded rectangle floating in a card.
const STRIP_RADIUS: u8 = 5;

/// Draw the preview for a profile.
///
/// `effect` is the selector the profile would write, which is not always one
/// this tool can name: an unknown value is shown as the raw byte, because
/// pretending it is a known effect would be a claim nothing here supports.
pub fn show(ui: &mut egui::Ui, profile: &crate::profile::Profile) {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), STRIP_HEIGHT + 18.0),
        egui::Sense::hover(),
    );
    response.on_hover_text(hover_text(profile));

    // Ask for a repaint whenever the effect is one that moves. Without this the
    // animation would only advance while the pointer moves or something else
    // changes, which reads as a glitch rather than as a preview. An effect that
    // holds one colour is left alone, so a static window is not repainting
    // sixty times a second for nothing.
    if paint_is_animated(profile.rgb_effect) {
        ui.ctx().request_repaint();
    }

    let strip = Rect::from_min_size(rect.min, Vec2::new(rect.width(), STRIP_HEIGHT));
    let slot_colours: Vec<Color32> = (0..USABLE_DPI_SLOTS)
        .map(|index| {
            let c = profile.slots[index].color;
            slot_colour([c[0], c[1], c[2]])
        })
        .collect();

    // What the strip shows depends on the effect, because that is what the
    // mouse does. A solid effect ignores the slots, Glorious Mode moves through
    // them, and the rest have no colour source this project has found, so they
    // are drawn as the slot colours pulsing rather than invented.
    let solid = slot_colour([
        profile.rgb_single_color[0],
        profile.rgb_single_color[1],
        profile.rgb_single_color[2],
    ]);
    let paint = paint_for(profile.rgb_effect, &slot_colours, solid, ui);

    // Background first: a mouse with the lighting off is a dark object, and
    // drawing the lit colours on the window background would make "off" look
    // like a colour rather than like nothing.
    ui.painter().rect_filled(
        strip,
        CornerRadius::same(STRIP_RADIUS),
        Color32::from_rgb(0x0e, 0x10, 0x15),
    );
    ui.painter().rect_stroke(
        strip,
        CornerRadius::same(STRIP_RADIUS),
        Stroke::new(1.0, Color32::from_rgb(0x2e, 0x33, 0x40)),
        egui::StrokeKind::Inside,
    );

    if let Some(colour) = &paint {
        // Segments, one per usable slot, so the strip shows the same structure
        // the DPI list does. A smooth gradient would look better and would hide
        // which slot is which, which is the thing a user checks.
        let width = strip.width() / USABLE_DPI_SLOTS as f32;
        for index in 0..USABLE_DPI_SLOTS {
            let left = strip.left() + width * index as f32;
            let mut segment =
                Rect::from_min_size(Pos2::new(left, strip.top()), Vec2::new(width, STRIP_HEIGHT));
            // Round only the outer ends of the whole strip, so it reads as one
            // bar while the seams between slots stay visible.
            let first = index == 0;
            let last = index == USABLE_DPI_SLOTS - 1;
            let radius = CornerRadius {
                nw: if first { STRIP_RADIUS } else { 0 },
                sw: if first { STRIP_RADIUS } else { 0 },
                ne: if last { STRIP_RADIUS } else { 0 },
                se: if last { STRIP_RADIUS } else { 0 },
            };
            let colour = colour(index);
            // A one pixel gap between segments, the same way the seams in the DPI
            // list separate rows, so the two read as the same table.
            if index > 0 {
                segment.min.x += 0.5;
                segment.max.x -= 0.5;
            }
            ui.painter().rect_filled(segment, radius, colour);
        }
    }

    draw_caption(ui, profile, rect, paint.is_some());
}

/// How the strip is painted, as a function of slot index.
///
/// Returns `None` when the effect has the lighting off, which is drawn as the
/// empty background rather than as a dark colour: an unlit mouse and a mouse
/// showing a very dark blue do not look alike, and only the first was measured.
fn paint_for(
    effect: Option<RgbEffect>,
    slots: &[Color32],
    solid: Color32,
    ui: &egui::Ui,
) -> Option<Box<dyn Fn(usize) -> Color32>> {
    let effect = effect?;
    // egui reports the frame time as f64; everything drawn here works in f32
    // because the result goes into a rectangle.
    let time = ui.input(|i| i.time) as f32;

    Some(match effect {
        // One colour for the whole mouse, from the bytes the device reads for
        // it. Those bytes are in the device's own order, which is why the
        // conversion happens before they reach here.
        RgbEffect::Single | RgbEffect::Breathing1 | RgbEffect::Breathing => {
            Box::new(move |_| solid)
        }
        // Measured: all six slots red and the mouse went red; all six blue and it
        // showed the blend between them. So this moves through the slot colours
        // rather than showing one of them. The speed is chosen, not measured.
        RgbEffect::Glorious => {
            let slots = slots.to_vec();
            Box::new(move |index: usize| {
                let phase = (time * 0.35) % 1.0;
                let span = slots.len() as f32;
                let position = (index as f32 / span + phase) * span;
                let a = position.floor() as usize % slots.len();
                let b = (a + 1) % slots.len();
                let t = position - position.floor();
                lerp(slots[a], slots[b], t)
            })
        }
        // Breathing is a brightness cycle in the one colour the effect is given,
        // and the effect named for seven colours has no colour source this
        // project has found. Both are drawn as a pulse of the slot colours, which
        // is a picture of "it changes over time" and not a claim about which
        // colours it uses.
        RgbEffect::Breathing7
        | RgbEffect::Tail
        | RgbEffect::Rave
        | RgbEffect::Random
        | RgbEffect::Wave => {
            let slots = slots.to_vec();
            Box::new(move |index: usize| {
                let pulse = 0.45 + 0.55 * (0.5 + 0.5 * (time * 1.6 + index as f32 * 0.4).sin());
                slots[index].gamma_multiply(pulse)
            })
        }
        RgbEffect::Off => return None,
        // Constant leaves the LEDs dark on this device and is never written, so
        // it is drawn the same as off rather than given a colour it does not show.
        RgbEffect::Constant | RgbEffect::NotSupported => return None,
    })
}

/// Whether the preview for this effect moves, and so needs continuous repaints.
///
/// The solid effects hold one colour, and the two that draw nothing are not
/// moving either. Everything else is drawn as motion, because what was measured
/// about those effects is that they change over time.
fn paint_is_animated(effect: Option<RgbEffect>) -> bool {
    matches!(
        effect,
        Some(
            RgbEffect::Glorious
                | RgbEffect::Breathing7
                | RgbEffect::Tail
                | RgbEffect::Rave
                | RgbEffect::Random
                | RgbEffect::Wave
                | RgbEffect::Breathing1
                | RgbEffect::Breathing
        )
    )
}

/// Blend two colours, for the gradient between two slots.
fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t).round() as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t).round() as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t).round() as u8,
        255,
    )
}

/// The line under the strip: which effect, and what it means for the colours.
fn draw_caption(ui: &mut egui::Ui, profile: &crate::profile::Profile, rect: Rect, lit: bool) {
    let effect_text = match profile.rgb_effect {
        Some(effect) if lit => effect.name().to_string(),
        Some(RgbEffect::Off) | Some(RgbEffect::Constant) | Some(RgbEffect::NotSupported) => {
            format!("{} — the lighting is off", effect_name(profile))
        }
        Some(effect) => format!("{} — the lighting is off", effect.name()),
        None => format!(
            "unknown effect {} — the lighting is off",
            profile.raw_effect_selector
        ),
    };
    let note = match profile.rgb_effect {
        Some(effect) if effect.uses_slot_colours() => {
            "Preview moves through the slot colours, which is what the mouse does."
        }
        Some(effect) if effect.has_solid_colour() => {
            "Preview shows the colour written to the whole mouse."
        }
        Some(_) => {
            "This effect has no colour source this tool has found. \
                    The strip shows the slot colours dimmed, not what the mouse will do."
        }
        None => "The device reports an effect this tool does not know.",
    };

    ui.painter().text(
        rect.left_top() + Vec2::new(0.0, STRIP_HEIGHT + 2.0),
        egui::Align2::LEFT_TOP,
        &effect_text,
        egui::FontId::proportional(12.0),
        crate::theme::MUTED,
    );
    ui.painter().text(
        rect.left_top() + Vec2::new(0.0, STRIP_HEIGHT + 2.0) + Vec2::new(0.0, 14.0),
        egui::Align2::LEFT_TOP,
        note,
        egui::FontId::proportional(11.0),
        crate::theme::MUTED.gamma_multiply(0.8),
    );
}

/// The name for an effect the profile has selected, for the caption's fallback.
fn effect_name(profile: &crate::profile::Profile) -> &'static str {
    match profile.rgb_effect {
        Some(effect) => effect.name(),
        None => "unknown",
    }
}

/// Text for the hover tooltip, saying what the preview is derived from.
fn hover_text(profile: &crate::profile::Profile) -> String {
    match profile.rgb_effect {
        Some(effect) => format!(
            "Preview of {}.\nDrawn from the bytes in the profile, the same ones the write is built from. \
             The device does not report whether a change took effect, so this is what was asked for, \
             not a reading back.",
            effect.name()
        ),
        None => format!(
            "The device reports effect {}, which this tool does not know.",
            profile.raw_effect_selector
        ),
    }
}
