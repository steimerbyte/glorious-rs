//! Application state, shared between the egui frontend and the HID worker.

use crate::device::DeviceState;

/// What the window owns. The mouse itself lives in the worker thread.
#[derive(Default)]
pub struct AppState {
    /// Last successful read, mirrored for rendering.
    pub device: Option<DeviceState>,
    /// Text shown when no mouse could be read.
    pub error: Option<String>,
    /// Set by the Reload button, consumed by the worker.
    pub reload_requested: bool,
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
