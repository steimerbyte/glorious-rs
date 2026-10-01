//! User interface: one settings page for the connected mouse.

use eframe::egui;

use crate::app::AppState;

/// Text and dark accents of the window, kept in one place.
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x6b, 0x8a, 0xfe);
const DANGER: egui::Color32 = egui::Color32::from_rgb(0xe0, 0x5f, 0x5f);

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
    /// Slot picked for the preset buttons.
    pub select_slot: Option<usize>,
    /// Resolution to hand to the slots the preset covers.
    pub apply_preset: Option<u16>,
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
/// strip reading 231, 234 and 248. The full high nibble is offered as well
/// because the breathing effect's own byte, 60, was lit at 0xff where 0x40 gave
/// a visibly dimmer result.
const BRIGHTNESS_STEPS: [u8; 5] = [0x00, 0x10, 0x20, 0x40, 0xff];

/// Resolutions offered as one click, the ones that come up in practice.
///
/// The mouse accepts 100 to 16000, so a drag control has to cover that whole
/// range, which makes it useless for picking an exact value. These are the ones
/// people set, and they are what the vendor software lists as well.
const PRESET_DPIS: [u16; 8] = [400, 800, 1600, 3200, 5000, 8000, 12000, 16000];

/// Draw the window into `root`, which is the panel `eframe` handed us.
pub fn draw(root: &mut egui::Ui, state: &mut AppState) -> UiActions {
    let mut actions = UiActions::default();

    egui::CentralPanel::default().show(root, |ui| {
        draw_header(ui, state);
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

fn draw_header(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.heading("Glorious Mouse");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let clicked = ui
                .button("Reload")
                .on_hover_text("Read the settings again from the mouse");
            if clicked.clicked() {
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

    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
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
    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
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

fn draw_dpi(ui: &mut egui::Ui, state: &mut AppState, actions: &mut UiActions) {
    let Some(device) = state.device.as_mut() else {
        return;
    };
    let max_dpi = device.profile.max_dpi();
    let active_index = device.profile.active_slot_index();

    section(ui, "DPI");

    // A click is only acted on when the pointer is released, so a drag across
    // the row does not write a value per frame it passes over.
    let released = root_released(ui);

    // A row of the resolutions people actually use, so setting up a profile does
    // not mean typing each value. The mouse stores 100 to 16000, but a DragValue
    // for that range is unusable, which is why the vendor software offers a
    // fixed list too.
    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.weak("Preset: assign these to the selected slots, or to the ones that are off.");
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for dpi in PRESET_DPIS {
                    if ui
                        .button(dpi.to_string())
                        .on_hover_text(format!("Set {dpi} dpi on the selected slots"))
                        .clicked()
                        && released
                    {
                        actions.apply_preset = Some(dpi);
                    }
                }
            });
        });

    // Edits are collected here and only handed to the device after the frame,
    // because writing needs a mutable borrow the borrow checker forbids mid UI.
    //
    // Drags are committed on release, not on every frame: the colour picker and
    // the DPI spinner report `changed` continuously while the pointer is down,
    // and writing each of those would flood the mouse. It would also rebuild the
    // widgets underneath the open popup, which closes it mid-drag.
    let mut changed: Vec<(usize, u16)> = Vec::new();
    let mut toggles: Vec<(usize, bool)> = Vec::new();
    let mut colors: Vec<(usize, [u8; 3])> = Vec::new();
    let mut selected: Option<usize> = None;

    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new("dpi_grid")
                .num_columns(4)
                .striped(true)
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
                        let response = ui
                            .label(
                                egui::RichText::new(format!("{}", index + 1))
                                    .color(if active { ACCENT } else { egui::Color32::GRAY })
                                    .strong(),
                            )
                            .on_hover_text(if active {
                                "The slot the mouse is using right now"
                            } else {
                                "Click to select this slot for the preset buttons above"
                            });
                        if response.clicked() {
                            selected = Some(index);
                        }

                        // Every slot is editable. The vendor software only lets the
                        // slot the mouse currently uses be changed, which means
                        // reconfiguring a profile means pressing the DPI button
                        // repeatedly. The device stores each slot independently, so
                        // there is no reason to make the user do that here.
                        let mut dpi = slot.dpi;
                        if ui
                            .add(
                                egui::DragValue::new(&mut dpi)
                                    .speed(50.0)
                                    .range(100..=max_dpi)
                                    .suffix(" dpi"),
                            )
                            .changed()
                        {
                            changed.push((index, dpi));
                        }

                        // egui edits the byte array directly, which is also how the
                        // device stores the colour.
                        let mut color = slot.color;
                        if ui.color_edit_button_srgb(&mut color).changed() {
                            colors.push((index, color));
                        }

                        let mut enabled = !slot.disabled;
                        if ui.checkbox(&mut enabled, "").changed() {
                            toggles.push((index, enabled));
                        }
                        ui.end_row();
                    }
                });
        });

    if let Some(index) = selected {
        actions.select_slot = Some(index);
    }

    if let Some((index, dpi)) = changed.into_iter().last().filter(|_| released) {
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
}

/// Whether the pointer was released this frame rather than held down.
///
/// A click on a checkbox reports `changed` on press, so it is passed through
/// immediately; drags and colour picks only report on release.
fn root_released(ui: &egui::Ui) -> bool {
    !ui.input(|i| i.pointer.any_down())
}

fn draw_polling(ui: &mut egui::Ui, state: &mut AppState, actions: &mut UiActions) {
    let Some(device) = state.device.as_mut() else {
        return;
    };
    section(ui, "Polling rate");

    let current = device.profile.report_rate();
    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
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

    egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
        ui.set_width(ui.available_width());

        // The effect is picked first, because what follows depends on it. Some
        // effects show a colour of their own, some follow the DPI slot colours,
        // and the rest cycle through colours held in bytes this tool does not
        // model.
        let Some(effect) = current else {
            ui.label("The mouse reports a lighting effect this tool does not know. Choosing one replaces it.");
            ui.horizontal_wrapped(|ui| {
                for effect in crate::protocol::RgbEffect::offered() {
                    if ui.button(effect.name()).clicked() {
                        actions.save_rgb = Some(effect);
                    }
                }
            });
            return;
        };

        ui.horizontal_wrapped(|ui| {
            for option in crate::protocol::RgbEffect::offered() {
                let selected = option == effect;
                if ui.selectable_label(selected, option.name()).clicked() && !selected {
                    actions.save_rgb = Some(option);
                }
            }
        });

        ui.add_space(6.0);
        if effect.has_solid_colour() {
            // egui edits the byte array directly, which is also how the device
            // stores the colour.
            let mut edited = device.profile.rgb_single_color;
            if ui
                .color_edit_button_srgb(&mut edited)
                .changed()
                && root_released(ui)
                && edited != device.profile.rgb_single_color
            {
                actions.save_effect_colour = Some(edited);
            }
            ui.label("Colour the whole mouse shows in this effect.");

            // Byte 56 was measured one value at a time on a real mouse: 16, 32
            // and 64 lit the effect at rising brightness, and the low nibble
            // changed nothing visible. The control offers those steps rather
            // than a continuous slider, because only those were observed and a
            // slider would offer values nothing here has a reading for.
            //
            // The low nibble of the device's byte is carried through, so
            // choosing a brightness here cannot clear a field this tool does
            // not model.
            let mode_byte = device.profile.rgb_single_mode;
            let (brightness, low_nibble) = (mode_byte & 0xf0, mode_byte & 0x0f);
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Brightness");
                ui.add_space(4.0);
                for value in BRIGHTNESS_STEPS {
                    let selected = value == brightness;
                    if ui
                        .selectable_label(selected, format!("{value:#04x}"))
                        .clicked()
                        && !selected
                    {
                        actions.save_effect_brightness = Some(
                            crate::protocol::rgb_brightness_encode(value, low_nibble),
                        );
                    }
                }
            });
            ui.label(
                egui::RichText::new("Measured on a Model O: 0x10, 0x20 and 0x40 light the effect at rising brightness.")
                    .color(egui::Color32::GRAY)
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
    });
}

fn draw_advanced(ui: &mut egui::Ui, state: &mut AppState, actions: &mut UiActions) {
    let Some(device) = state.device.as_mut() else {
        return;
    };
    section(ui, "Advanced");
    let debounce = device.debounce_ms;

    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
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
                    .color(egui::Color32::GRAY)
                    .small(),
            );
        });
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(4.0);
    ui.strong(title);
    ui.add_space(4.0);
}
