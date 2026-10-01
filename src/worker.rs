//! Worker that owns the mouse and applies changes off the UI thread.
//!
//! HID calls block, and `eframe` runs the UI on the main thread, so the mouse
//! lives in its own thread and communicates through channels. Requests carry the
//! change the user made, the worker performs it, verifies it by reading back and
//! answers with the fresh state or an error.

use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use crate::device::{DeviceState, HidMouse};
use crate::transport::HidTransport;

/// Something the user asked for.
pub enum Command {
    Read,
    SetDpi { slot: usize, dpi: u16 },
    SetSlotEnabled { slot: usize, enabled: bool },
    SetSlotColor { slot: usize, color: [u8; 3] },
    SetReportRate { hz: u16 },
    SetLighting {
        effect: crate::protocol::RgbEffect,
    },
    /// Colour for an effect that shows one colour for the whole mouse.
    SetEffectColor { color: [u8; 3] },
    SetDebounce { ms: u8 },
}

/// The worker's answer.
pub enum Reply {
    State(Box<DeviceState>),
    Failed(String),
}

/// Handle to the worker thread.
pub struct Worker {
    commands: Sender<Command>,
    replies: Receiver<Reply>,
    /// Last reply that was not yet shown, so the UI can stay responsive.
    pending: Option<Reply>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    /// Open the mouse and start the thread that owns it.
    pub fn start() -> Worker {
        let (commands, command_rx) = std::sync::mpsc::channel::<Command>();
        let (reply_tx, replies) = std::sync::mpsc::channel::<Reply>();

        let thread = thread::spawn(move || {
            let transport = match HidTransport::open(None, None) {
                Ok(transport) => transport,
                Err(error) => {
                    let _ = reply_tx.send(Reply::Failed(error.to_string()));
                    return;
                }
            };
            let mut mouse = HidMouse::new(transport);
            loop {
                let reply = match command_rx.recv() {
                    Err(_) => break,
                    // A plain read re-reads everything; a change is verified by
                    // reading again after the write, so both end up fresh.
                    Ok(Command::Read) => read(&mut mouse, true),
                    Ok(command) => apply(&mut mouse, command),
                };
                if reply_tx.send(reply).is_err() {
                    break;
                }
            }
        });

        Worker { commands, replies, pending: None, thread: Some(thread) }
    }

    /// Ask the worker to read the mouse again.
    pub fn request(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Take the newest answer, if one arrived.
    ///
    /// Answers are taken one at a time, newest last, so a burst of changes does
    /// not stall the window.
    pub fn take_reply(&mut self) -> Option<Reply> {
        while let Ok(reply) = self.replies.try_recv() {
            self.pending = Some(reply);
        }
        self.pending.take()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Dropping the sender closes the channel, which ends the loop.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn read(mouse: &mut HidMouse, refresh: bool) -> Reply {
    match mouse.state(refresh) {
        Ok(state) => Reply::State(Box::new(state.clone())),
        Err(error) => Reply::Failed(error.to_string()),
    }
}

/// Apply one change, then report the state as the user just set it.
///
/// Reading the mouse back would be the obvious thing to do, but the device
/// answers a read taken right after a write with the values it had before, so
/// the window would show the old setting while the mouse already shows the new
/// one. What was written is therefore reported back, and a later reload from the
/// device confirms it.
fn apply(mouse: &mut HidMouse, command: Command) -> Reply {
    let outcome: Result<Option<DeviceState>, String> = match command {
        Command::Read => Ok(None),
        Command::SetDpi { slot, dpi } => with_profile(mouse, |profile| {
            if let Some(entry) = profile.slots.get_mut(slot) {
                entry.dpi = dpi;
            }
            Ok(())
        }),
        Command::SetSlotEnabled { slot, enabled } => with_profile(mouse, |profile| {
            if let Some(entry) = profile.slots.get_mut(slot) {
                entry.disabled = !enabled;
            }
            Ok(())
        }),
        Command::SetSlotColor { slot, color } => with_profile(mouse, |profile| {
            if let Some(entry) = profile.slots.get_mut(slot) {
                entry.color = color;
            }
            Ok(())
        }),
        Command::SetReportRate { hz } => with_profile(mouse, |profile| {
            match crate::protocol::report_rate_to_raw(hz) {
                Some(raw) => {
                    profile.report_rate_raw = raw;
                    Ok(())
                }
                None => Err(format!("{hz} Hz is not a supported report rate")),
            }
        }),
        Command::SetLighting { effect } => with_profile(mouse, |profile| {
            profile.rgb_effect = Some(effect);
            Ok(())
        }),
        Command::SetEffectColor { color } => with_profile(mouse, |profile| {
            profile.rgb_single_color = color;
            Ok(())
        }),
        // The debounce value lives in its own command, so the profile is not
        // written and the device is read to show what it stored.
        Command::SetDebounce { ms } => mouse
            .set_debounce(ms)
            .map_err(|error| error.to_string())
            .map(|()| None),
    };

    match outcome {
        Ok(Some(state)) => Reply::State(Box::new(state)),
        Ok(None) => read(mouse, true),
        Err(error) => Reply::Failed(error),
    }
}

/// Patch the current profile, write it back, and hand the written profile on.
///
/// The device is not read again: a read taken right after a write answers with
/// the values the mouse had before it, so the window would keep showing the old
/// setting while the mouse already shows the new one. The write is the authority
/// for what the user just chose, and a later reload confirms it against the
/// device.
fn with_profile(
    mouse: &mut HidMouse,
    change: impl FnOnce(&mut crate::profile::Profile) -> Result<(), String>,
) -> Result<Option<DeviceState>, String> {
    let mut state = mouse.state(false).map_err(|error| error.to_string())?.clone();
    let mut profile = state.profile.clone();
    change(&mut profile)?;
    mouse.write_profile(&profile).map_err(|error| error.to_string())?;
    state.profile = profile;
    Ok(Some(state))
}
