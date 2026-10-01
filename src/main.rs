//! The window.

#![windows_subsystem = "windows"]

// The window is built for the Windows GUI subsystem, so starting it from
// Explorer or a shortcut opens the app and nothing else. The subsystem is a
// property of the binary and not of a function, so the command line could not
// have it too: a console binary prints, a GUI binary does not, and the printing
// is what every measurement in this project is read from. The commands live in
// their own binary, `glorious-ctl`, which keeps its console and its output.
// There is nothing to arrange between the two here, which is the point of
// splitting them: no branch on the arguments, and no flag that one start could
// want and the other could not have.

use eframe::egui;

use glorious::app::AppState;
use glorious::confetti;
use glorious::sparks;
use glorious::ui;
use glorious::worker::{Command, Reply, Worker};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(glorious::theme::WINDOW_SIZE)
            .with_min_inner_size(glorious::theme::WINDOW_MIN_SIZE),
        ..Default::default()
    };

    eframe::run_native(
        "Glorious Mouse",
        options,
        Box::new(|cc| {
            // Dark always, and not "follow the system": the window is about a
            // lit mouse on a desk, and a light scheme puts the LED colours on
            // white, which is a different picture from the one the user sees.
            cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
            // The style is installed once rather than per frame. egui keeps it,
            // and reapplying it every frame would be work for nothing. Going
            // through `style_mut_of` rather than assigning a whole style means
            // only the dark one is touched, so a light theme set anywhere else
            // would not be silently overwritten by the other one.
            cc.egui_ctx
                .style_mut_of(egui::Theme::Dark, |style| *style = glorious::theme::style());
            Ok(Box::new(GloriousApp::new()))
        }),
    )
}

/// The eframe application.
struct GloriousApp {
    state: AppState,
    worker: Worker,
    /// Set while a change is being written, so the window can show progress.
    saving: bool,
    /// The burst of paper shown when a colour is set.
    confetti: confetti::Confetti,
    /// Sparks under the pointer and starbursts on a clicked control.
    sparks: sparks::Sparks,
    /// Where the pointer was last frame, so a movement can be told from a still
    /// pointer. Without it a pointer held still over a control would keep laying
    /// sparks, because a hover is not a movement and the two are not the same
    /// thing here.
    last_pointer: Option<egui::Pos2>,
}

impl GloriousApp {
    fn new() -> Self {
        let worker = Worker::start();
        worker.request(Command::Read);
        GloriousApp {
            state: AppState::default(),
            worker,
            saving: false,
            confetti: confetti::Confetti::default(),
            sparks: sparks::Sparks::default(),
            last_pointer: None,
        }
    }

    /// Turn UI clicks into worker commands.
    fn dispatch(&mut self, actions: ui::UiActions) {
        let invalidate = actions.reload;

        if let Some((slot, dpi)) = actions.save_dpi {
            self.worker.request(Command::SetDpi { slot, dpi });
            self.saving = true;
        }
        if let Some((slot, enabled)) = actions.save_slot_enabled {
            self.worker
                .request(Command::SetSlotEnabled { slot, enabled });
            self.saving = true;
        }
        if let Some((slot, color)) = actions.save_color {
            self.worker.request(Command::SetSlotColor { slot, color });
            self.saving = true;
        }
        if let Some(hz) = actions.save_rate {
            self.worker.request(Command::SetReportRate { hz });
            self.saving = true;
        }
        if let Some(effect) = actions.save_rgb {
            self.worker.request(Command::SetLighting { effect });
            self.saving = true;
        }
        if let Some(color) = actions.save_effect_colour {
            self.worker.request(Command::SetEffectColor { color });
            self.saving = true;
        }
        if let Some(mode_byte) = actions.save_effect_brightness {
            self.worker
                .request(Command::SetEffectBrightness { mode_byte });
            self.saving = true;
        }
        // The profile list. It needs the device state as it is now: which slots
        // are on decides where each entry lands, and the worker reads that
        // itself rather than being told a slot number that could be stale.
        if let Some(entries) = actions.apply_profile {
            self.worker.request(Command::ApplyProfile { entries });
            self.saving = true;
        }
        // A slot clicked in the table starts a list entry, carrying the colour
        // that slot already has so the user only has to change what is wrong.
        if let Some((dpi, colour)) = actions.add_step_from_slot {
            self.state
                .draft
                .entries
                .push(glorious::app::PresetEntry { dpi, colour });
        }
        if let Some(ms) = actions.save_debounce {
            self.worker.request(Command::SetDebounce { ms });
            self.saving = true;
        }

        if invalidate {
            self.state.invalidate();
            self.worker.request(Command::Read);
        }
    }
}

impl eframe::App for GloriousApp {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Clamped, because a window that was in the background reports a delta of
        // many seconds at once, which would teleport every piece off screen.
        let delta_seconds = root.ctx().input(|input| input.stable_dt).clamp(0.0, 0.05);
        match self.worker.take_reply() {
            Some(Reply::State(state)) => {
                self.state.device = Some(*state);
                self.state.error = None;
                self.saving = false;
            }
            Some(Reply::Failed(error)) => {
                self.state.fail(error);
                self.saving = false;
            }
            None => {}
        }

        self.state.reload_requested = false;
        if self.saving || self.confetti.is_active() || self.sparks.is_active() {
            // Repainting is requested while anything is moving, because the
            // pieces move on their own: without this the window would only update
            // when the pointer moves and the burst would freeze mid-air.
            root.ctx().request_repaint();
        }

        let actions = ui::draw(root, &mut self.state);
        if !actions.saved_colours.is_empty() {
            // The burst comes from the middle of the window rather than from the
            // swatch that was clicked: the swatch sits near the top, and pieces
            // thrown upwards from there would immediately leave the window.
            self.confetti.burst(
                actions.saved_colours[0].1,
                root.ctx().content_rect().center(),
            );
        }
        // A starburst on every control that was clicked, at the pointer. A
        // window with two of these at once is a deliberate clatter rather than an
        // accident, and each is separate because two controls clicked in the
        // same frame are two things that happened.
        for position in &actions.clicks {
            self.sparks.starburst(*position);
        }

        // Sparks follow the pointer, so where it is has to be read from this
        // frame's input.
        //
        // egui reports the pointer in the window's own coordinates, which is
        // also what `layer_painter` draws in, so no conversion is needed here.
        // `hover_pos` is `None` when the pointer is outside the window or over
        // nothing, and that is when the trail stops.
        //
        // Read through `root`, not through the inner `Ui` of the scroll area:
        // the same context, but the outer one is the one that spans the window.
        let pointer = root.ctx().input(|i| i.pointer.hover_pos());
        let down = root.ctx().input(|i| i.pointer.any_down());
        match pointer {
            Some(position) => {
                let moved = match self.last_pointer {
                    Some(last) => last.distance(position) > 0.5,
                    None => false,
                };
                // A pointer held down over a button is dragging it, not
                // hovering, and laying sparks during a drag would draw over the
                // thing being dragged.
                self.sparks.at_pointer(position, moved, !down);
                self.last_pointer = Some(position);
            }
            None => {
                self.sparks.clear_pointer();
                self.last_pointer = None;
            }
        }

        self.sparks.draw(root, delta_seconds);
        self.confetti.draw(root, delta_seconds);
        self.dispatch(actions);
    }
}
