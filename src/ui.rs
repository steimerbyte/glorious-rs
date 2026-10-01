//! User interface: one settings page for the connected mouse.
//!
//! The colours and the shape of everything here come from [`crate::theme`], so
//! this file is about layout and about what a control does, not about how it
//! looks.

use eframe::egui;

use crate::app::AppState;
use crate::theme::{ACCENT, DANGER, MUTED};

/// Where a click landed, returned so `main` can run the blocking HID work.
#[derive(Debug, Default)]
pub struct UiActions {
    pub reload: bool,
    pub save_dpi: Option<(usize, u16)>,
    pub save_slot_enabled: Option<(usize, bool)>,
    pub save_color: Option<(usize, [u8; 3])>,
    pub save_rate: Option<u16>,
    pub save_debounce: Option<u8>,
    pub save_rgb: Option<crate::protocol::RgbEffect>,
    /// Colour for an effect that shows one colour for the whole mouse.
    pub save_effect_colour: Option<[u8; 3]>,
    /// Brightness for the effect that has a measured brightness byte.
    pub save_effect_brightness: Option<u8>,
    /// A whole profile: resolutions and colours, in the order they should land
    /// on the enabled slots.
    pub apply_profile: Option<Vec<(u16, [u8; 3])>>,
    /// A slot the user clicked, so it can be added to the list above.
    ///
    /// Clicking a slot number puts that resolution into the list rather than
    /// selecting a target for a button: the buttons that needed a target are
    /// gone, and a click that did nothing would look broken.
    pub add_step_from_slot: Option<(u16, [u8; 3])>,
    /// Where a control was clicked this frame, so the window can mark it.
    ///
    /// Collected here rather than fired at each control, because egui has no
    /// global "something was clicked" event: a `Response` knows about its own
    /// widget and about nothing else. Every control that should mark itself
    /// reports through this, and the window turns the list into one burst.
    pub clicks: Vec<egui::Pos2>,
    /// Colours that were just written, so the window can mark the change. Filled
    /// only when a write actually goes out, not while the picker is being
    /// dragged: a burst per frame of a drag would be a solid sheet of paper.
    pub saved_colours: Vec<(usize, [u8; 3])>,
}

/// Brightness values offered for a lighting effect.
///
/// Byte 56 was written one value at a time on a real Model O and the mouse
/// photographed after each. Values 1, 2, 4, 8 and 16 left the LEDs dark, 32 lit
/// them at about half and 64 lit them fully, with the red channel of the lit
/// strip reading 231, 234 and 248.
///
/// Only those four are offered. The high nibble also takes 0x80 and 0xf0, but
/// nothing here has a photograph of either, and a control that offered a
/// brightness nobody had looked at would be guessing at what the mouse does
/// with it.
const BRIGHTNESS_STEPS: [u8; 4] = [0x00, 0x10, 0x20, 0x40];

/// Resolutions offered as one click, the ones that come up in practice.
///
/// The mouse accepts 100 to 16000, so a drag control has to cover that whole
/// range, which makes it useless for picking an exact value. These are the ones
/// people set, and they are what the vendor software lists as well.
const PRESET_DPIS: [u16; 9] = [400, 800, 1600, 3200, 5000, 8000, 10000, 12000, 16000];

/// Draw the window into `root`, which is the panel `eframe` handed us.
pub fn draw(root: &mut egui::Ui, state: &mut AppState) -> UiActions {
    let mut actions = UiActions::default();

    egui::CentralPanel::default().show(root, |ui| {
        draw_header(ui, state, &mut actions);
        ui.add_space(12.0);

        // A vertical scroll area hands its child the width of the widest line
        // rather than the panel width, which lets the selection buttons grow
        // past the window edge. Clamping first keeps every row inside.
        let width = ui.available_width();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(width);
                draw_device(ui, state);
                ui.add_space(16.0);
                draw_dpi(ui, state, &mut actions);
                ui.add_space(16.0);
                draw_polling(ui, state, &mut actions);
                ui.add_space(16.0);
                draw_lighting(ui, state, &mut actions);
                ui.add_space(16.0);
                draw_advanced(ui, state, &mut actions);
            });
    });

    actions
}

fn draw_header(ui: &mut egui::Ui, state: &mut AppState, actions: &mut UiActions) {
    ui.horizontal(|ui| {
        ui.heading("Glorious Mouse");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let button = ui
                .button("Reload")
                .on_hover_text("Read the settings again from the mouse");
            if mark(ui, &button, actions) {
                state.reload_requested = true;
            }
        });
    });
}

fn draw_device(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(device) = state.device.as_ref() else {
        show_status(ui, state);
        return;
    };

    crate::theme::card(ui, false, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new("device_grid")
            .num_columns(2)
            .striped(true)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                ui.label("Model");
                ui.label(&device.name);
                ui.end_row();

                ui.label("USB ID");
                ui.label(format!(
                    "{:04x}:{:04x}",
                    device.vendor_id, device.product_id
                ));
                ui.end_row();

                ui.label("Firmware");
                ui.label(&device.firmware);
                ui.end_row();

                ui.label("Sensor");
                ui.label(device.profile.sensor_name());
                ui.end_row();

                ui.label("Profile");
                ui.label(format!("{}", device.active_profile));
                ui.end_row();
            });
    });
}

/// Either the error text, or a spinner while the mouse is being read.
fn show_status(ui: &mut egui::Ui, state: &AppState) {
    crate::theme::card(ui, false, |ui| {
        if let Some(error) = state.error.as_deref() {
            ui.colored_label(DANGER, "No mouse");
            ui.add_space(6.0);
            ui.label(error);
        } else {
            ui.spinner();
            ui.label("Looking for a mouse...");
        }
    });
}

/// The list of steps being built: a resolution and a colour each, with a button
/// that hands the whole list to the mouse.
///
/// This is separate from the slot table below it on purpose. The table is what
/// the mouse holds; this is what the user is about to hand it. Keeping them
/// apart means applying a list and editing a slot never fight over the same
/// bytes, and the table stays readable as a readout of the current state.
fn draw_preset_builder(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut UiActions,
    enabled_count: usize,
    released: bool,
) {
    section(ui, "Profile steps");
    let enabled_count = enabled_count.max(1);

    // Collected here rather than inside the card, because the card's content is
    // a closure and these are acted on once it has returned.
    let mut remove: Option<usize> = None;
    let mut moved: Option<(usize, usize)> = None;

    crate::theme::card(ui, true, |ui| {
        ui.set_width(ui.available_width());
        ui.weak(
            "Set the resolution and the colour of each step, then apply. \
             Steps land on the enabled slots from the top.",
        );
        ui.add_space(6.0);

        let draft = &mut state.draft.entries;

        egui::Grid::new("preset_grid")
            .num_columns(4)
            .striped(true)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                ui.strong("Step");
                ui.strong("DPI");
                ui.strong("LED");
                ui.strong("");
                ui.end_row();

                for index in 0..draft.len() {
                    ui.label(format!("{}", index + 1));

                    // The resolutions that come up in practice, as a list, rather
                    // than as a drag: the mouse takes 100 to 16000 and a
                    // DragValue across that range is useless for picking a
                    // value, which is why the vendor software offers a list too.
                    ui.horizontal(|ui| {
                        let entry = &mut draft[index];
                        egui::ComboBox::from_id_salt(("preset_dpi", index))
                            .selected_text(format!("{} dpi", entry.dpi))
                            .width(100.0)
                            .show_ui(ui, |ui| {
                                for dpi in PRESET_DPIS {
                                    ui.selectable_value(&mut entry.dpi, dpi, format!("{dpi} dpi"));
                                }
                            });
                    });

                    let mut colour = draft[index].colour;
                    if ui.color_edit_button_srgb(&mut colour).changed() {
                        draft[index].colour = colour;
                    }

                    ui.horizontal(|ui| {
                        let up = ui.small_button("up").on_hover_text("Move this step up");
                        if mark(ui, &up, actions) && index > 0 {
                            moved = Some((index, index - 1));
                        }
                        let down = ui.small_button("down").on_hover_text("Move this step down");
                        if mark(ui, &down, actions) && index + 1 < draft.len() {
                            moved = Some((index, index + 1));
                        }
                        let remove_button =
                            ui.small_button("remove").on_hover_text("Remove this step");
                        if mark(ui, &remove_button, actions) {
                            remove = Some(index);
                        }
                    });
                    ui.end_row();
                }
            });

        // A step with no slot to go to would be silently dropped when the list
        // is applied, so the row is called out rather than the button quietly
        // doing less than it says.
        if draft.len() > enabled_count {
            ui.label(
                egui::RichText::new(format!(
                    "{} steps, {enabled_count} enabled slots: the extra ones are kept in the \
                     list but not written. Switching more slots on brings them back.",
                    draft.len()
                ))
                .color(MUTED)
                .small(),
            );
        }

        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            let add = ui.button("Add step");
            if mark(ui, &add, actions) {
                state.draft.entries.push(Default::default());
            }
            let can_apply = !state.draft.entries.is_empty() && released;
            let apply = ui
                .add_enabled(can_apply, egui::Button::new("Apply to the mouse"))
                .on_hover_text(
                    "Write these resolutions and colours to the enabled slots, from the top",
                );
            if mark(ui, &apply, actions) {
                actions.apply_profile = Some(
                    state
                        .draft
                        .entries
                        .iter()
                        .map(|entry| (entry.dpi, entry.colour))
                        .collect(),
                );
                // The burst is fired from the colour of the first step, which is
                // the one the mouse shows in the slot the user is most likely to
                // be on.
                if let Some(first) = state.draft.entries.first() {
                    actions.saved_colours.push((0, first.colour));
                }
            }
            let reset = ui.button("Reset").on_hover_text("Empty the list");
            if mark(ui, &reset, actions) {
                state.draft = Default::default();
            }
        });
    });

    // Applied after the grid closes, because reordering during a `show` would
    // move rows out from under the iterator that is drawing them.
    if let Some((from, to)) = moved {
        let draft = &mut state.draft.entries;
        let entry = draft.remove(from);
        draft.insert(to, entry);
    }
    // The last step is not removable: a list with no steps in it has nothing to
    // apply, and a button that removes the only row leaves the card in a state
    // the user has to guess their way out of.
    if let Some(index) = remove
        && state.draft.entries.len() > 1
    {
        state.draft.entries.remove(index);
    }
}

/// Width of the accent bar drawn to the left of the row the mouse is using.
///
/// Four points, the same as the stripe on a marked card: three reads as a
/// rendering artefact on screen rather than as a mark. It is drawn ending on
/// the left edge of the row's first cell, so it occupies the grid's own left
/// overhang instead of covering the slot number, whose label is allocated its
/// own text width. The card's padding is 14 points, so the bar stays well clear
/// of the card's border even in the narrowest window.
const ACTIVE_ROW_STRIPE: f32 = 4.0;

/// How far the accent is dimmed for the marked row's background.
///
/// 0.28, which is exactly what `theme::style` sets for a hovered, active or
/// open button and for the selection fill, so the marked row is painted in a
/// colour the window already uses for "this is the active one" and not in a
/// new shade of its own. It is written as a number rather than read back from
/// the style because the row colour is computed outside any `Ui`, in the
/// closure egui hands the row index to, and taking a `&Visuals` from there
/// would mean passing the live style in by hand. The value is a duplicate that
/// can drift, which is the trade for the tint being derived from `ACCENT` and
/// not from a hard-coded triple.
const ACTIVE_ROW_FILL: f32 = 0.28;

/// The row colour of the DPI grid, for a row at `row`.
///
/// `None` for `active` leaves the grid exactly as it was before: the device
/// reporting no active slot is a thing it can do, and marking slot 1 in that
/// case would be a claim the bytes do not support. `row` counts grid rows from
/// zero, and row zero is the header, so slot `active` is drawn on row
/// `active + 1`.
///
/// The alternating stripe stays on the rows around the marked one, and the
/// marked row replaces its stripe rather than sitting on top of it: a tint that
/// is dark enough for four controls to be readable over it is darker than the
/// stripe, and painting over would need an opaque fill where the stripe used to
/// be, which would read as a hole in the table. That is also why this returns
/// `Option` for every row and not just the marked one: with nothing marked, the
/// closure reproduces egui's own `striped_row_color` exactly.
fn dpi_row_fill(row: usize, active: Option<usize>, style: &egui::Style) -> Option<egui::Color32> {
    if active_row(row, active) {
        // The same fill egui gives a hovered, active or open button in this
        // theme, and the same value it uses for a selected item. The window
        // marks states with outlines and hairlines rather than with fills
        // behind text, so a fill that dark enough to leave the slot number, the
        // spinner and the checkbox readable is the one thing that fits under
        // them. Reusing the value rather than picking one means the row looks
        // like every other "this is active" in the window instead of
        // introducing a third kind of emphasis.
        //
        // Derived from `ACCENT` rather than a new RGB triple, so the row and
        // the number in it cannot drift apart if the accent ever changes.
        return Some(ACCENT.gamma_multiply(ACTIVE_ROW_FILL));
    }
    // egui's own stripe rule, for the rows that are not marked. Reproduced here
    // rather than delegating to `Grid::striped` because the two cannot both be
    // in effect, and the marked row has to win.
    //
    // This resolves to the card's own fill, because the theme sets
    // `faint_bg_color` to the same value: the stripes in this grid are drawn but
    // cannot be seen, and the marked row is therefore the only tinted one. That
    // is left as it is rather than fixed here. A visible stripe is a change to
    // how the whole table reads and is not what was asked for, and the marked
    // row does not depend on it: it is marked by the tint and the bar, not by
    // standing out from its neighbours.
    (row % 2 == 1).then_some(style.visuals.faint_bg_color)
}

/// Whether grid row `row` is the one the mouse is using.
///
/// `row` counts rows from zero and row zero is the header, so slot `n` is on
/// row `n + 1`. The offset is why the header is excluded explicitly: with a
/// bare `active == Some(row - 1)`, wrapping the subtraction would let slot 1
/// mark the header, and `0usize - 1` in a debug build is a panic rather than a
/// missed row.
///
/// `None` marks nothing, which is the whole point of the type. The device can
/// report a byte 11 whose low nibble matches no enabled slot, and falling back
/// to the first slot there would be showing the user a resolution the mouse is
/// not using.
pub fn active_row(row: usize, active: Option<usize>) -> bool {
    row > 0 && active == Some(row - 1)
}

fn draw_dpi(ui: &mut egui::Ui, state: &mut AppState, actions: &mut UiActions) {
    // Read before the mutable borrow of the device: the builder needs the whole
    // state to hold the list, and holding a borrow on the device across the call
    // would be two mutable borrows of the same struct at once.
    let enabled_count = state
        .device
        .as_ref()
        .map(|device| {
            device
                .profile
                .slots
                .iter()
                .take(crate::protocol::USABLE_DPI_SLOTS)
                .filter(|slot| !slot.disabled)
                .count()
        })
        .unwrap_or(0);

    // A click is only acted on when the pointer is released, so a drag across
    // a row does not write a value per frame it passes over.
    let released = root_released(ui);
    draw_preset_builder(ui, state, actions, enabled_count, released);

    let Some(device) = state.device.as_mut() else {
        return;
    };
    let (max_dpi, max_dpi_is_measured) = device.profile.max_dpi_known();
    let active_index = device.profile.active_slot_index();

    section(ui, "DPI");

    // Edits are collected here and only handed to the device after the frame,
    // because writing needs a mutable borrow the borrow checker forbids mid UI.
    //
    // Drags are committed on release, not on every frame: the colour picker and
    // the DPI spinner report `changed` continuously while the pointer is down,
    // and writing each of those would flood the mouse. It would also rebuild the
    // widgets underneath the open popup, which closes it mid-drag.
    //
    // A value that was not dragged is a different case and is kept apart from
    // the dragged ones, because it is not committed on a pointer release at all.
    // See the commit below.
    let mut changed: Vec<(usize, u16)> = Vec::new();
    let mut typed: Vec<(usize, u16)> = Vec::new();
    let mut toggles: Vec<(usize, bool)> = Vec::new();
    let mut colors: Vec<(usize, [u8; 3])> = Vec::new();
    let mut selected: Option<usize> = None;
    // The slot whose value was dragged with the pointer, and whose edit is
    // therefore held back until the pointer is let go.
    let mut dragged: Option<usize> = None;

    crate::theme::card(ui, false, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new("dpi_grid")
            .num_columns(4)
            // Replaces `striped(true)`. The stripe is reproduced for every row
            // that is not marked, so nothing is lost, and the row the mouse is
            // using gets the accent tint instead of a stripe that looks like
            // every other row.
            .with_row_color(move |row, style| dpi_row_fill(row, active_index, style))
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                ui.strong("Slot");
                ui.strong("DPI");
                ui.strong("LED");
                ui.strong("On");
                ui.end_row();

                // Only the slots the mouse actually drives. The report has room
                // for eight, but the vendor configuration lists six and the
                // device refuses a profile that switches on more, so showing the
                // last two would offer settings that cannot be saved.
                for index in 0..crate::protocol::USABLE_DPI_SLOTS {
                    let slot = device.profile.slots[index];
                    let active = active_index == Some(index);
                    let number = ui
                        .label(
                            egui::RichText::new(format!("{}", index + 1))
                                .color(if active { ACCENT } else { MUTED })
                                .strong(),
                        )
                        .on_hover_text(if active {
                            // Three claims, each of which the code can stand
                            // behind. Byte 11 carries the active slot and was
                            // read back from a real mouse. Nothing in the tool
                            // ever assigns it: no command writes it and no
                            // control offers it, so this row reports rather than
                            // sets. And the window reads the mouse at startup
                            // and on Reload, never on a timer, so a slot changed
                            // on the mouse itself is not noticed until one of
                            // those two.
                            "The slot the mouse is using right now. The DPI button on the mouse \
                             moves it, and this window cannot set it: press Reload after changing \
                             it on the mouse."
                        } else {
                            "Click to select this slot for the preset buttons above"
                        });
                    if mark(ui, &number, actions) {
                        selected = Some(index);
                    }

                    // Every slot is editable. The vendor software only lets the
                    // slot the mouse currently uses be changed, which means
                    // reconfiguring a profile means pressing the DPI button
                    // repeatedly. The device stores each slot independently, so
                    // there is no reason to make the user do that here.
                    //
                    // Zero decimals, because the device's own encoding is
                    // `raw * 100`: 1234 dpi is stored as raw 11 on a PMW3360 and
                    // reads back as 1200. A field that accepts a value the
                    // hardware then rounds to a different one shows the user two
                    // numbers for one setting, and the second one arrives on the
                    // next read.
                    let mut dpi = slot.dpi;
                    let spinner = ui.add(
                        egui::DragValue::new(&mut dpi)
                            .speed(50.0)
                            .range(100..=max_dpi)
                            .fixed_decimals(0)
                            .suffix(" dpi"),
                    );
                    if spinner.changed() {
                        changed.push((index, dpi));
                    }
                    // Dragging is the one case that has to wait for the pointer.
                    // A drag reports on every frame it moves, so writing each of
                    // those is what the release check is for.
                    if spinner.dragged() || spinner.drag_stopped() {
                        dragged = Some(index);
                    } else if spinner.changed() {
                        // Typed, or nudged with the arrow keys, which egui
                        // reports the same way a typed value is reported. Both
                        // are one deliberate change by the user, so both are
                        // committed straight away rather than dropped until the
                        // pointer happens to be up.
                        typed.push((index, dpi));
                    }

                    // egui edits the byte array directly, which is also how the
                    // device stores the colour.
                    let mut color = slot.color;
                    let picker = ui.color_edit_button_srgb(&mut color);
                    if picker.changed() {
                        colors.push((index, color));
                    }

                    let mut enabled = !slot.disabled;
                    let checkbox = ui.checkbox(&mut enabled, "");
                    if checkbox.changed() {
                        toggles.push((index, enabled));
                    }

                    // The bar that marks the row as the live one. Kept next to
                    // the row tint rather than left to it, because a tinted row
                    // on its own reads as a selected table row, and nothing here
                    // is selected. The bar says which row the mouse is on, the
                    // tint says the row is different, and the two together read
                    // as a marker instead of as a selection the user made.
                    if active {
                        // From the widgets the row actually laid out, not from
                        // the grid cursor: at this point the cursor is where the
                        // checkbox ended up, which is a column rather than a
                        // row. The colour picker is left out of it, because a
                        // widget sitting in a cell is not the row's edge.
                        let top = number.rect.top().min(spinner.rect.top());
                        let bottom = checkbox.rect.bottom().max(spinner.rect.bottom());
                        // Ending on the number's left edge rather than starting
                        // a fixed distance left of it. The label is allocated
                        // its own text width, so its left edge is the digit
                        // itself, and a bar anchored any other way either
                        // overlaps the first stroke of the number or leaves a
                        // gap that moves with the font.
                        let right = number.rect.left();
                        ui.painter().rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(right - ACTIVE_ROW_STRIPE, top),
                                egui::pos2(right, bottom),
                            ),
                            egui::CornerRadius::ZERO,
                            ACCENT,
                        );
                    }
                    ui.end_row();
                }
            });
    });

    if let Some(index) = selected {
        // Clicking a slot number starts a list entry with that resolution and the
        // colour the slot already has, so the common case is one click rather
        // than picking a value from a list and then a colour.
        let slot = device.profile.slots[index];
        actions.add_step_from_slot = Some((slot.dpi, slot.color));
    }

    // A typed value is committed without waiting for a pointer release, because
    // a keyboard is not a pointer: `root_released` asks whether any mouse button
    // is down, which is true for the entire time the user is typing in a field
    // they opened by clicking, and false afterwards. A value typed into that
    // field therefore reported `changed` on the frames it was typed and was
    // dropped on every one of them, because the pointer was still considered
    // down. The field showed the new number, the write never went out, and the
    // next read put the old value back: the value a user typed came to nothing
    // and the field appeared to snap back on its own.
    //
    // A drag is still committed on release, and only that one. Writing per frame
    // would flood the mouse, and it rebuilds the widgets under the open field,
    // which closes it mid-edit.
    let last_drag = changed
        .last()
        .copied()
        .filter(|(index, _)| Some(*index) == dragged);
    if let Some((index, dpi)) = last_drag.filter(|_| released) {
        actions.save_dpi = Some((index, dpi));
    } else if let Some((index, dpi)) = typed.last().copied() {
        actions.save_dpi = Some((index, dpi));
    }
    if let Some((index, enabled)) = toggles.into_iter().last().filter(|_| released) {
        actions.save_slot_enabled = Some((index, enabled));
    }
    for (index, color) in colors {
        if released {
            actions.save_color = Some((index, color));
            actions.saved_colours.push((index, color));
        }
    }

    // A range that is a guess is said to be one. The sensor byte is the only
    // thing that decides what the highest resolution is, and a mouse whose
    // sensor is not one of the four measured here gets the highest of those four
    // as a ceiling rather than its own maximum. Without saying so, that ceiling
    // is indistinguishable from a measurement, and a user who is told 16000 is
    // the mouse's limit will believe a value the hardware never took.
    if !max_dpi_is_measured {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(format!(
                "Sensor 0x{:02x} is not one measured here, so its real maximum is unknown. \
                 Values up to {max_dpi} dpi can be set; the mouse may take less.",
                device.profile.sensor
            ))
            .color(MUTED)
            .small(),
        );
    }
}

/// Whether the pointer was released this frame rather than held down.
///
/// A click on a checkbox reports `changed` on press, so it is passed through
/// immediately; drags and colour picks only report on release.
fn root_released(ui: &egui::Ui) -> bool {
    !ui.input(|i| i.pointer.any_down())
}

/// Report a click on `response` so the window can mark it, and pass the click
/// through unchanged.
///
/// Returns `clicked`, so it can sit at the end of a condition without the caller
/// repeating itself. The burst goes where the pointer is rather than at the
/// centre of the control: one in the middle of a wide button looks wrong, and
/// one under the cursor looks like the button threw sparks.
///
/// egui has no global "something was clicked" event. A `Response` knows about
/// its own widget and about nothing else, so every control that should mark
/// itself reports through here, and the window turns the collected positions
/// into bursts.
///
/// Only a click counts. Hovering produces no burst, because a window that lights
/// up as the pointer passes over it is showing feedback for something the user
/// did not do.
pub fn mark(ui: &egui::Ui, response: &egui::Response, actions: &mut UiActions) -> bool {
    let clicked = response.clicked();
    if clicked {
        actions
            .clicks
            .push(ui.pointer_hover_pos().unwrap_or(response.rect.center()));
    }
    clicked
}

fn draw_polling(ui: &mut egui::Ui, state: &mut AppState, actions: &mut UiActions) {
    let Some(device) = state.device.as_mut() else {
        return;
    };
    section(ui, "Polling rate");

    let current = device.profile.report_rate();
    crate::theme::card(ui, false, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            for (_, hz) in crate::protocol::REPORT_RATES {
                let selected = hz == current;
                if ui.selectable_label(selected, format!("{hz} Hz")).clicked() && !selected {
                    actions.save_rate = Some(hz);
                }
            }
        });
    });
}

fn draw_lighting(ui: &mut egui::Ui, state: &mut AppState, actions: &mut UiActions) {
    let Some(device) = state.device.as_mut() else {
        return;
    };
    section(ui, "Lighting");
    let current = device.profile.rgb_effect;

    crate::theme::card(ui, true, |ui| {
        ui.set_width(ui.available_width());

        // The effect is picked first, because what follows depends on it. Some
        // effects show a colour of their own, some follow the DPI slot colours,
        // and the rest cycle through colours held in bytes this tool does not
        // model.
        let Some(effect) = current else {
            ui.label("The mouse reports a lighting effect this tool does not know. Choosing one replaces it.");
            ui.horizontal_wrapped(|ui| {
                for effect in crate::protocol::RgbEffect::offered() {
                    let button = ui.button(effect.name());
                    if mark(ui, &button, actions) {
                        actions.save_rgb = Some(effect);
                    }
                }
            });
            return;
        };

        ui.horizontal_wrapped(|ui| {
            for option in crate::protocol::RgbEffect::offered() {
                let selected = option == effect;
                let label = ui.selectable_label(selected, option.name());
                if mark(ui, &label, actions) && !selected {
                    actions.save_rgb = Some(option);
                }
            }
        });

        ui.add_space(6.0);
        if effect.has_solid_colour() {
            // egui edits the byte array directly, which is also how the device
            // stores the colour.
            let mut edited = device.profile.rgb_single_color;
            if ui.color_edit_button_srgb(&mut edited).changed()
                && root_released(ui)
                && edited != device.profile.rgb_single_color
            {
                actions.save_effect_colour = Some(edited);
            }
            ui.label("Colour the whole mouse shows in this effect.");

            // Byte 56 was measured one value at a time on a real mouse: 0x10,
            // 0x20 and 0x40 light the effect at rising brightness, 0x00 is off,
            // and the low nibble changed nothing visible at any of them.
            //
            // A slider over those four steps rather than a row of buttons,
            // because they are a progression and a row of equal-looking buttons
            // does not say that. The numbers stay next to the handle because
            // they are the values the device stores, and a slider that hid them
            // would leave the terminal and this window disagreeing about what a
            // position means.
            //
            // The device byte holds more states than were observed, and a drag
            // past the last step would write a value nothing here has a reading
            // for, so the range stops at 0x40. A device already set to a byte
            // outside the four is reported as such rather than snapped to a step
            // it is not on, because that value may well be one more of the real
            // device's states and rewriting it on open would be a change the
            // user never asked for.
            //
            // The low nibble of the device's byte is carried through, so moving
            // the slider cannot clear a field this tool does not model.
            let mode_byte = device.profile.rgb_single_mode;
            let (brightness, low_nibble) = (mode_byte & 0xf0, mode_byte & 0x0f);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Brightness");
                ui.add_space(4.0);
                let last = (BRIGHTNESS_STEPS.len() - 1) as f32;
                let position = BRIGHTNESS_STEPS
                    .iter()
                    .position(|value| *value == brightness)
                    .map(|index| index as f32);
                let mut step = position.unwrap_or(0.0);
                // A fixed width rather than a fraction of the row, so the hex
                // value next to it stays where the eye expects it instead of
                // drifting with the window.
                let response = ui.add_sized(
                    [150.0, 18.0],
                    egui::Slider::new(&mut step, 0.0..=last)
                        .step_by(1.0)
                        .show_value(false),
                );
                let chosen = BRIGHTNESS_STEPS[step.round().clamp(0.0, last) as usize];
                if response.changed() && chosen != brightness {
                    actions.save_effect_brightness =
                        Some(crate::protocol::rgb_brightness_encode(chosen, low_nibble));
                }
                ui.add_space(4.0);
                ui.monospace(format!("{chosen:#04x}"));
                if position.is_none() {
                    ui.label(
                        egui::RichText::new(format!("device has {brightness:#04x}")).color(MUTED),
                    )
                    .on_hover_text(
                        "This brightness was not one of the four measured on a Model O. \
                         Moving the slider writes a measured one.",
                    );
                }
            });
            ui.label(
                egui::RichText::new(
                    "Measured on a Model O: 0x10, 0x20 and 0x40 light the effect at rising \
                     brightness. 0x00 is off.",
                )
                .color(MUTED)
                .small(),
            );
        } else if effect.uses_slot_colours() {
            // Glorious Mode and the seven colour breathing both follow the DPI
            // list, so the hint says which. They are the same only in that they
            // read the slot colours: measured, Glorious Mode lit red when all
            // six slots were set red, and stayed the slot colour when the solid
            // colour in the bytes behind this editor was set to a different
            // value.
            match effect {
                crate::protocol::RgbEffect::Glorious => {
                    ui.label("Moves through the DPI slot colours. Set them in the list above.");
                }
                _ => {
                    ui.label("Each lit slot shows its own colour. Set them in the DPI list above.");
                }
            }
        } else {
            ui.label(match effect {
                crate::protocol::RgbEffect::Off => "The lighting is off.",
                _ => "This effect plays its own colours.",
            });
        }

        // The preview goes last in this card, after the controls that change it,
        // so what it shows is the result of what was just chosen rather than
        // something the user has to remember to scroll back up to.
        ui.add_space(10.0);
        crate::preview::show(ui, &device.profile);
    });
}

fn draw_advanced(ui: &mut egui::Ui, state: &mut AppState, actions: &mut UiActions) {
    let Some(device) = state.device.as_mut() else {
        return;
    };
    section(ui, "Advanced");
    let debounce = device.debounce_ms;

    crate::theme::card(ui, false, |ui| {
        ui.set_width(ui.available_width());
        // Wrapped, because seven buttons plus two labels do not fit on one line
        // in a narrow window.
        ui.horizontal_wrapped(|ui| {
            ui.label("Debounce");
            let current = debounce.unwrap_or(0);
            ui.label(format!("{current} ms"));
            ui.add_space(8.0);
            for value in crate::protocol::DEBOUNCE_TIMES {
                let selected = value == current;
                if ui.selectable_label(selected, format!("{value}")).clicked() && !selected {
                    actions.save_debounce = Some(value);
                }
            }
        });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Lower values cut click latency but risk double clicks.")
                .color(MUTED)
                .small(),
        );
    });
}

/// A section heading, above a card.
///
/// The heading carries no frame of its own. The card below it is what marks the
/// section, and a heading inside a box as well would put a border around a
/// label.
fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.strong(title);
    ui.add_space(2.0);
}
