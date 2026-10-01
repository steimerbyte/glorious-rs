//! Application state, shared between the egui frontend and the HID worker.

use crate::device::DeviceState;

/// A profile the user is building: a resolution and a colour per step.
///
/// This lives in the window and not in the device profile because it is not a
/// setting. It is a list to be applied, and once applied the result is a normal
/// profile the mouse holds on its own. Nothing here is ever written as a
/// preset: the device has no field for one, and inventing one would mean
/// putting bytes at an offset nothing has measured.
#[derive(Debug, Clone)]
pub struct PresetEntry {
    pub dpi: u16,
    pub colour: [u8; 3],
}

impl Default for PresetEntry {
    fn default() -> Self {
        // Starts at a red that is on most of the built in lighting effects, so
        // the list is visible on the mouse as soon as it is applied rather than
        // as a row of blacks.
        PresetEntry {
            dpi: 800,
            colour: [255, 0, 0],
        }
    }
}

/// The list being built, and which presets the window offers.
///
/// The number of steps is the number of slots the mouse drives, so the list is
/// exactly as long as the thing it fills. A shorter list is allowed and applies
/// to the first slots only, which is how a two step profile is made on a mouse
/// with six.
#[derive(Debug, Clone)]
pub struct PresetDraft {
    pub entries: Vec<PresetEntry>,
}

impl Default for PresetDraft {
    fn default() -> Self {
        PresetDraft {
            entries: vec![PresetEntry::default()],
        }
    }
}

/// What the window owns. The mouse itself lives in the worker thread.
#[derive(Default)]
pub struct AppState {
    /// Last successful read, mirrored for rendering.
    pub device: Option<DeviceState>,
    /// Text shown when no mouse could be read.
    pub error: Option<String>,
    /// Set by the Reload button, consumed by the worker.
    pub reload_requested: bool,
    /// The profile being built, dropped rather than applied.
    pub draft: PresetDraft,
}

impl AppState {
    /// Drop a stale read so the window shows the loading state again.
    pub fn invalidate(&mut self) {
        self.device = None;
        self.reload_requested = true;
    }

    /// Show an error instead of the settings.
    pub fn fail(&mut self, message: String) {
        self.device = None;
        self.error = Some(message);
    }
}
