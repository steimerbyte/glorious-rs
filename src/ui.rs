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
}

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

    egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
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
    egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
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
    // Edits are collected here and only handed to the device after the frame,
    // because writing needs a mutable borrow the borrow checker forbids mid UI.
    //
    // Drags are committed on release, not on every frame: the colour picker and
    // the DPI spinner report `changed` continuously while the pointer is down,
    // and writing each of those would flood the mouse. It would also rebuild the
    // widgets underneath the open popup, which closes it mid-drag.
    let released = root_released(ui);
    let mut changed: Vec<(usize, u16)> = Vec::new();
    let mut toggles: Vec<(usize, bool)> = Vec::new();
    let mut colors: Vec<(usize, [u8; 3])> = Vec::new();

    egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new("dpi_grid")
            .num_columns(4)
            .striped(true)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                ui.strong("Slot");
                ui.strong("DPI");
                ui.strong("LED");
                ui.strong("Enabled");
                ui.end_row();

                for index in 0..device.profile.slots.len() {
                    let slot = device.profile.slots[index];
                    let active = active_index == Some(index);
                    ui.label(
                        egui::RichText::new(format!("{}", index + 1))
                            .color(if active { ACCENT } else { egui::Color32::GRAY }),
                    );

                    // Only the active slot is editable, mirroring the vendor
                    // software where you pick the slot with the DPI button.
                    let mut dpi = slot.dpi;
                    let editable = active && !slot.disabled;
                    if editable {
                        let response = ui.add(
                            egui::DragValue::new(&mut dpi)
                                .speed(50.0)
                                .range(100..=max_dpi)
                                .suffix(" dpi"),
                        );
                        if response.changed() {
                            changed.push((index, dpi));
                        }
                    } else {
                        ui.label(format!("{} dpi", slot.dpi))
                            .on_hover_text(if slot.disabled {
                                "this slot is disabled"
                            } else {
                                "select this slot on the mouse to edit it"
                            });
                    }

                    // egui 0.36 edits the byte array directly, which is also
                    // how the device stores the colour.
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

    if let Some((index, dpi)) = changed.into_iter().last().filter(|_| released) {
        actions.save_dpi = Some((index, dpi));
    }
    if let Some((index, enabled)) = toggles.into_iter().last().filter(|_| released) {
        actions.save_slot_enabled = Some((index, enabled));
    }
    for (index, color) in colors {
        if released {
            actions.save_color = Some((index, color));
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
    egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
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

        // The effect is picked first, because what follows depends on it: a
        // solid effect shows its own colour, while the others cycle colours the
        // tool does not control.
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
        } else if effect.uses_slot_colours() {
            ui.label("Each lit slot shows its own colour. Set them in the DPI list above.");
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

    egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
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
            egui::RichText::new(
                "Lower values cut click latency but risk double clicks.",
            )
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
