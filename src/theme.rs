//! Look and feel of the window.
//!
//! Everything visual is set here rather than at the call sites, so a colour
//! appears in one place and changing the scheme does not mean walking through
//! the drawing code.
//!
//! The scheme is a dark one built around a single accent. That is a choice about
//! the subject: this is a tool for setting a mouse up, and the window sits next
//! to whatever the user is playing, so it stays out of the way. There is no
//! theme switch, because there is no second theme to switch to, and a control
//! for it would be a control for nothing.

use eframe::egui;
use eframe::egui::{Color32, CornerRadius, Stroke, Visuals};

/// The accent the interface marks the active thing with.
///
/// Blue rather than the colour of the LEDs, because the slot colours are user
/// data and are shown as themselves. An interface that borrowed the LED colour
/// would change its own colours every time the lighting changed.
pub const ACCENT: Color32 = Color32::from_rgb(0x6b, 0x8a, 0xfe);

/// Text colour for a setting that is not what the mouse is using.
pub const MUTED: Color32 = Color32::from_rgb(0x8b, 0x91, 0xa3);

/// Text colour for a failure. Distinct from `ACCENT` in hue and in lightness, so
/// it does not read as "selected" in a list where both could appear.
pub const DANGER: Color32 = Color32::from_rgb(0xe0, 0x5f, 0x5f);

/// Colour of the hairline around a card.
const CARD_BORDER: Color32 = Color32::from_rgb(0x2e, 0x33, 0x40);

/// Rounding of the accent stripe on the marked card. Matched to the card's own
/// radius so the two do not disagree at the corner.
const STRIPE_RADIUS: u8 = 8;

/// The window's default size and its floor.
///
/// Tall enough that the lighting card and its preview are both on screen
/// without scrolling on a 1080p display, because the preview is the part of that
/// card worth looking at. The floor is smaller because the content reflows into
/// a scroll area rather than clipping.
pub const WINDOW_SIZE: [f32; 2] = [580.0, 860.0];
pub const WINDOW_MIN_SIZE: [f32; 2] = [440.0, 520.0];

/// Build the style the window runs with.
///
/// Called once, from the app constructor. Everything below is a deviation from
/// egui's own dark default, and the values are chosen so the window reads as
/// one set of rules rather than a collection of tweaks: one accent, one
/// background, one hairline, and corner radii that are the same everywhere.
pub fn style() -> egui::Style {
    let mut style = egui::Style::default();

    let mut visuals = Visuals::dark();

    // Frames. The default puts a light fill behind everything, which on a dark
    // window makes every card look like a button.
    visuals.window_fill = Color32::from_rgb(0x16, 0x19, 0x20);
    visuals.panel_fill = Color32::from_rgb(0x16, 0x19, 0x20);
    visuals.extreme_bg_color = Color32::from_rgb(0x0f, 0x11, 0x17);
    visuals.faint_bg_color = Color32::from_rgb(0x1a, 0x1e, 0x27);
    visuals.code_bg_color = Color32::from_rgb(0x1a, 0x1e, 0x27);

    // The accent, where egui would otherwise use its own blue. Applied to the
    // four states of a button, the selection fill and the progress bar, so
    // "this is active" looks the same wherever it appears.
    for fill in [
        &mut visuals.widgets.hovered.bg_fill,
        &mut visuals.widgets.active.bg_fill,
        &mut visuals.widgets.open.bg_fill,
    ] {
        *fill = ACCENT.gamma_multiply(0.28);
    }
    visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
    visuals.widgets.active.weak_bg_fill = visuals.widgets.active.bg_fill;
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x24, 0x29, 0x34);
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    // Corners. egui's default is square, which on a dark window reads as
    // unfinished next to the rounded window frame the system draws.
    let radius = CornerRadius::same(6);
    visuals.widgets.noninteractive.corner_radius = radius;
    visuals.widgets.inactive.corner_radius = radius;
    visuals.widgets.hovered.corner_radius = radius;
    visuals.widgets.active.corner_radius = radius;
    visuals.widgets.open.corner_radius = radius;

    // Hairlines instead of fills to mark a state, so a selected button is
    // outlined rather than filled and the text under it keeps its contrast.
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, CARD_BORDER);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, CARD_BORDER);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.7));
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT);

    // Text colours are left to egui's own defaults here on purpose. Setting
    // `override_text_color` would dim every label in the window, headings and
    // selected values included, and egui 0.36 has no per-widget text colour to
    // set instead. The muted colour is used where it is wanted, by the widgets
    // that want it, rather than as a blanket.

    visuals.hyperlink_color = ACCENT;
    visuals.error_fg_color = DANGER;
    visuals.warn_fg_color = Color32::from_rgb(0xe0, 0xa5, 0x5f);

    // Text sizes. The default body size sits small next to a 560 pixel window,
    // and this is a tool that is read at a glance rather than studied.
    style.text_styles = text_styles();

    // Spacing. More room between items than the default, which is tuned for
    // dense forms. A mouse profile is set in a handful of actions a year, so
    // there is nothing to gain from fitting more on screen.
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 26.0;
    style.spacing.scroll.floating = true;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.window_margin = egui::Margin::same(14);

    style.visuals = visuals;
    style
}

/// Text sizes, as a scale over egui's defaults rather than as absolutes.
///
/// Scaling keeps the relationship between the sizes, which is what makes a
/// heading read as a heading rather than merely as bigger text.
fn text_styles() -> std::collections::BTreeMap<egui::TextStyle, egui::FontId> {
    let base = egui::TextStyle::Body;
    let mut styles = egui::Style::default().text_styles;
    let body = styles[&base].size;
    for (style, factor) in [
        (egui::TextStyle::Heading, 1.45),
        (egui::TextStyle::Monospace, 1.0),
        (egui::TextStyle::Button, 1.0),
        (egui::TextStyle::Small, 0.88),
    ] {
        let family = styles[&style].family.clone();
        styles.insert(style, egui::FontId::new(body * factor, family));
    }
    styles
}

/// Draw the content inside a card: a hairline border, rounded corners, and a
/// tinted fill a step above the window behind it.
///
/// This is what replaces `Frame::group` everywhere. The default group frame has
/// no border and relies on a fill that is nearly the same colour as the panel,
/// so a section is marked by a gap rather than by being a section. The stripe
/// on the left is the part that does the work: it is what makes a card read as
/// a card at a glance when five of them are stacked.
pub fn card(ui: &mut egui::Ui, accent: bool, add_contents: impl FnOnce(&mut egui::Ui)) {
    let frame = egui::Frame::new()
        .fill(Color32::from_rgb(0x1a, 0x1e, 0x27))
        .stroke(Stroke::new(1.0, CARD_BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(14)
        .outer_margin(egui::Margin::symmetric(0, 2));

    if !accent {
        frame.show(ui, add_contents);
        return;
    }

    // A stripe on the left edge only. `Margin` takes all four sides at once, so
    // the stripe is drawn over the finished card rather than as padding inside
    // it, which keeps the content aligned with the cards that have no stripe.
    //
    // Four points wide, because at three it reads as a rendering artefact on
    // screen rather than as a mark. It is the only part of the window in the
    // accent colour that is not a selection, so it carries the meaning on its
    // own. Only the two outer corners are rounded, because the card's inner
    // corners meet the stripe rather than the background.
    let stripe = 4.0;
    let response = frame.show(ui, add_contents).response;
    let rect = response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_max(
            rect.left_top(),
            egui::pos2(rect.left() + stripe, rect.bottom()),
        ),
        CornerRadius {
            nw: STRIPE_RADIUS,
            sw: STRIPE_RADIUS,
            ne: 0,
            se: 0,
        },
        ACCENT,
    );
}

/// The colour a slot's LED is set to, in the form the interface paints it.
///
/// The profile stores colours in the device's own order, which is red, blue,
/// green on this device and not the usual red, green, blue. The window shows
/// them as the user typed them, so a green chosen in the picker is a green swatch
/// here; that conversion is in the protocol module and is not repeated.
pub fn slot_colour(colour: [u8; 3]) -> Color32 {
    Color32::from_rgb(colour[0], colour[1], colour[2])
}
