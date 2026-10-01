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
    SetDpi {
        slot: usize,
        dpi: u16,
    },
    SetSlotEnabled {
        slot: usize,
        enabled: bool,
    },
    SetSlotColor {
        slot: usize,
        color: [u8; 3],
    },
    SetReportRate {
        hz: u16,
    },
    SetLighting {
        effect: crate::protocol::RgbEffect,
    },
    /// Colour for an effect that shows one colour for the whole mouse.
    SetEffectColor {
        color: [u8; 3],
    },
    /// Brightness of the effect that has one, as a whole mode byte.
    ///
    /// Bytes 56 and 60 were measured one value at a time on a real mouse: 16,
    /// 32 and 64 lit the solid effect at rising brightness, and the low nibble
    /// changed nothing visible at any of them. The byte is therefore carried
    /// over whole and only its high nibble is written.
    SetEffectBrightness {
        mode_byte: u8,
    },
    /// Set one resolution on the slot the user picked and the ones that are off.
    ///
    /// This is the old behaviour and it is a fill, not a mapping: every slot
    /// that is switched off ends up with the same value. It is kept because
    /// filling a profile from scratch is what it is good at, and because a
    /// device that was just plugged in has nothing set.
    ApplyPreset {
        slot: usize,
        dpi: u16,
    },
    /// Give a list of resolutions and colours to the enabled slots, in order.
    ///
    /// The target slot of each entry is decided by the device's own state rather
    /// than by the user naming it, so the list and the mouse cannot drift
    /// apart: entry one goes to the lowest enabled slot, and so on. Slots the
    /// list does not reach are left as they are, so a list of two against a
    /// profile of six does not wipe the other four.
    ApplyProfile {
        entries: Vec<(u16, [u8; 3])>,
    },
    SetDebounce {
        ms: u8,
    },
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

        Worker {
            commands,
            replies,
            pending: None,
            thread: Some(thread),
        }
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
            // The menu already leaves this effect out, but the command also
            // arrives from the terminal, and a value that puts the mouse into a
            // state indistinguishable from a hardware fault is worth refusing
            // at the point where it would be written.
            if !effect.is_writable() {
                return Err(format!("{} would leave the LEDs dark", effect.name()));
            }
            profile.rgb_effect = Some(effect);
            Ok(())
        }),
        Command::SetEffectColor { color } => with_profile(mouse, |profile| {
            profile.rgb_single_color = color;
            Ok(())
        }),
        // Which byte holds the brightness depends on the effect that is
        // selected, and the two measured fields are 56 and 60. An effect with
        // neither keeps whatever the device had, so a brightness chosen under
        // one effect cannot silently become a write to the other one.
        Command::SetEffectBrightness { mode_byte } => {
            with_profile(mouse, |profile| match profile.rgb_effect {
                Some(crate::protocol::RgbEffect::Breathing7) => {
                    profile.rgb_breathing7_mode = mode_byte;
                    Ok(())
                }
                Some(effect) if effect.has_solid_colour() => {
                    profile.rgb_single_mode = mode_byte;
                    Ok(())
                }
                Some(effect) => Err(format!(
                    "{} has no brightness field this tool has measured",
                    effect.name()
                )),
                None => Err("no lighting effect is selected".to_string()),
            })
        }
        // A list of resolutions and colours, mapped onto the enabled slots in
        // order.
        //
        // The mapping is by position among the slots the mouse drives, not by
        // the slot number the window shows. A disabled slot is storage the
        // firmware keeps and does not light, so writing one produces a setting
        // the user cannot see and the profile they asked for is not the profile
        // they get. Entry one therefore goes to the first enabled slot, entry
        // two to the second, and so on.
        //
        // Entries beyond the number of enabled slots are dropped rather than
        // refused. A profile with fewer slots than the list has entries is a
        // normal thing to hit while the mouse is set up, and refusing the whole
        // list would leave the user with nothing instead of with a partial
        // profile they can see.
        //
        // Colours arrive in the order the window uses and leave in the device's
        // own order, because that conversion belongs where the bytes are written.
        Command::ApplyProfile { entries } => {
            with_profile(mouse, |profile| apply_profile(profile, &entries))
        }
        // The old fill: one resolution on the slot the user picked and on every
        // slot that is switched off. Kept because filling a profile from
        // scratch is what it is for, and because a mouse that was just plugged
        // in has nothing set.
        Command::ApplyPreset { slot, dpi } => with_profile(mouse, |profile| {
            let slots = &mut profile.slots;
            if slot >= slots.len() {
                return Err(format!("slot {} does not exist", slot + 1));
            }
            slots[slot].dpi = dpi;
            for (index, other) in slots.iter_mut().enumerate() {
                if other.disabled && index != slot {
                    other.dpi = dpi;
                }
            }
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

/// Hand a list of resolutions and colours to the slots the mouse drives.
///
/// The mapping is by position among the enabled slots, not by the slot number
/// the window shows. A disabled slot is storage the firmware keeps and does not
/// light, so writing one produces a setting the user cannot see, and the
/// profile they asked for is not the profile they get. Entry one therefore goes
/// to the first enabled slot, entry two to the second, and so on.
///
/// Entries beyond the number of enabled slots are dropped rather than refused. A
/// profile with fewer slots than the list has entries is a normal thing to hit
/// while a mouse is being set up, and refusing the whole list would leave the
/// user with nothing instead of with a partial profile they can see and finish.
///
/// Colours arrive in the order the window uses and leave in the device's own
/// order, because that conversion belongs where the bytes are written. Doing it
/// here as well would be the same swap applied twice, and red is the case that
/// hides it: it is the one channel a swap does not touch.
pub fn apply_profile(
    profile: &mut crate::profile::Profile,
    entries: &[(u16, [u8; 3])],
) -> Result<(), String> {
    if entries.is_empty() {
        return Err("no profile step was given".to_string());
    }
    let enabled: Vec<usize> = (0..crate::protocol::USABLE_DPI_SLOTS)
        .filter(|index| !profile.slots[*index].disabled)
        .collect();
    if enabled.is_empty() {
        return Err(
            "every slot is switched off, so there is nowhere to put a profile step".to_string(),
        );
    }
    for (index, (dpi, colour)) in enabled.iter().zip(entries) {
        profile.slots[*index].dpi = *dpi;
        profile.slots[*index].color = *colour;
    }
    Ok(())
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
    let mut state = mouse
        .state(false)
        .map_err(|error| error.to_string())?
        .clone();
    let mut profile = state.profile.clone();
    change(&mut profile)?;
    mouse
        .write_profile(&profile)
        .map_err(|error| error.to_string())?;
    state.profile = profile;
    Ok(Some(state))
}
