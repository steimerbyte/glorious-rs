//! High level device access.

use crate::profile::Profile;
use crate::protocol::*;
use crate::transport::{FeatureTransport, TransportError};

/// Everything the app can read from one profile.
#[derive(Debug, Clone)]
pub struct DeviceState {
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
    pub firmware: String,
    pub active_profile: u8,
    pub profile: Profile,
    pub debounce_ms: Option<u8>,
}

impl DeviceState {
    /// Report rate in Hz of the current profile.
    pub fn report_rate(&self) -> u16 {
        self.profile.report_rate()
    }
}

/// A mouse reached over one transport.
pub struct Mouse<T: FeatureTransport> {
    /// The transport, public so tests can inspect what was sent to the device.
    pub transport: T,
    payload_len: usize,
    cached: Option<DeviceState>,
}

impl<T: FeatureTransport> Mouse<T> {
    pub fn new(transport: T) -> Self {
        Mouse {
            transport,
            payload_len: CONFIG_REPORT_SIZE,
            cached: None,
        }
    }

    /// Run a command and return its reply, which echoes the command in byte 1.
    fn command(&mut self, command: u8) -> Result<Vec<u8>, TransportError> {
        let mut request = vec![0u8; COMMAND_SIZE];
        request[0] = REPORT_ID_COMMAND;
        request[1] = command;
        self.transport.set_feature_report(&request)?;

        let reply = self
            .transport
            .get_feature_report(REPORT_ID_COMMAND, COMMAND_SIZE)?;
        if reply.len() < 2 || reply[1] != command {
            return Err(TransportError::Protocol(format!(
                "command 0x{command:02x} was answered for a different command"
            )));
        }
        Ok(reply)
    }

    /// Read a profile's configuration blob.
    ///
    /// Public so the layout can be dumped straight off the device.
    pub fn read_config(&mut self, command: u8) -> Result<Vec<u8>, TransportError> {
        let mut request = vec![0u8; COMMAND_SIZE];
        request[0] = REPORT_ID_COMMAND;
        request[1] = command;
        self.transport.set_feature_report(&request)?;
        self.transport
            .get_feature_report(REPORT_ID_CONFIG, CONFIG_REPORT_SIZE)
    }

    /// Length of the meaningful content in a configuration blob.
    ///
    /// The device does not state it: byte 3 is zero on the hardware tested and
    /// byte 1 only echoes the command. Scanning for the last non-zero byte is
    /// wrong, because trailing fields may legitimately be zero and truncating
    /// there would silently drop the fields after them.
    ///
    /// The driver's largest configuration is 167 bytes of content, of which 8
    /// are the header, so byte 3 addresses 159 on a full device. A Model O
    /// stores 130, which is what this reports.
    fn payload_length(blob: &[u8]) -> usize {
        MODEL_O_CONFIG_SIZE.min(blob.len())
    }

    /// Read the mouse's state, using the cache unless `refresh` is set.
    pub fn state(&mut self, refresh: bool) -> Result<&DeviceState, TransportError> {
        if self.cached.is_none() || refresh {
            let firmware_reply = self.command(CMD_FIRMWARE_VERSION)?;
            let firmware = String::from_utf8_lossy(&firmware_reply[2..6])
                .trim_end_matches('\0')
                .trim()
                .to_string();

            let profile_reply = self.command(CMD_PROFILE)?;
            let active_profile = profile_reply[2];

            let command = config_command(0);
            let blob = self.read_config(command)?;
            if blob.len() < MIN_CONFIG_SIZE || blob[0] != REPORT_ID_CONFIG {
                return Err(TransportError::Protocol(
                    "device returned an unusable configuration blob".into(),
                ));
            }
            if blob[1] != command {
                return Err(TransportError::Protocol(format!(
                    "configuration blob answers command 0x{:02x}, expected 0x{:02x}",
                    blob[1], command
                )));
            }
            self.payload_len = Self::payload_length(&blob);
            let profile = Profile::parse(&blob[..self.payload_len.max(MIN_CONFIG_SIZE)])
                .ok_or_else(|| {
                    TransportError::Protocol("configuration blob is too short to decode".into())
                })?;

            let debounce_ms = self.debounce().map(|value| value * 2);

            let (vendor_id, product_id) = (self.transport.vendor_id(), self.transport.product_id());
            let name = KNOWN_DEVICES
                .iter()
                .find(|d| d.0 == vendor_id && d.1 == product_id)
                .map(|d| d.2.to_string())
                .unwrap_or_else(|| "unknown".to_string());

            self.cached = Some(DeviceState {
                vendor_id,
                product_id,
                name,
                firmware,
                active_profile,
                profile,
                debounce_ms,
            });
        }
        Ok(self.cached.as_ref().expect("cache filled above"))
    }

    /// Debounce time as stored by the device, which keeps the value halved.
    pub fn debounce(&mut self) -> Option<u8> {
        let reply = self.command(CMD_DEBOUNCE).ok()?;
        reply.get(2).copied()
    }

    /// Set the debounce time in milliseconds.
    pub fn set_debounce(&mut self, milliseconds: u8) -> Result<(), TransportError> {
        if !DEBOUNCE_TIMES.contains(&milliseconds) {
            return Err(TransportError::Protocol(format!(
                "debounce must be one of {DEBOUNCE_TIMES:?} ms, got {milliseconds}"
            )));
        }
        let mut request = vec![0u8; COMMAND_SIZE];
        request[0] = REPORT_ID_COMMAND;
        request[1] = CMD_DEBOUNCE;
        request[2] = milliseconds / 2;
        self.transport.set_feature_report(&request)?;
        Ok(())
    }

    /// Value byte 3 is written with on a Model O.
    ///
    /// Not derivable from the data: the device accepts some values and silently
    /// blanks the whole configuration for others, without saying which. Found
    /// by writing one colour, reading it back and restoring, for each candidate.
    pub const MODEL_O_LENGTH_BYTE: u8 = 122;

    /// Write a profile back to the device.
    ///
    /// The transfer is always the full report size and byte 1 carries the
    /// command id, byte 3 the content length. A short buffer is dropped by the
    /// device without an error, so the size has to match exactly.
    ///
    /// Only the fields this tool models are overlaid on what the device
    /// reported. Everything else keeps the device's own value: writing zero
    /// into a field whose meaning is unknown would silently clear settings such
    /// as the lighting effects, and the device does not report the loss.
    pub fn write_profile(&mut self, profile: &Profile) -> Result<(), TransportError> {
        self.write_profile_with_length(profile, Self::MODEL_O_LENGTH_BYTE)
    }

    /// Write a profile with byte 3 set explicitly.
    ///
    /// The calibration command uses this to sweep the candidates.
    pub fn write_profile_with_length(
        &mut self,
        profile: &Profile,
        length_byte: u8,
    ) -> Result<(), TransportError> {
        let command = config_command(profile.index);
        let blob = self.read_config(command)?;
        if blob.len() < MIN_CONFIG_SIZE || blob[1] != command {
            return Err(TransportError::Protocol(
                "device returned an unusable configuration blob".into(),
            ));
        }
        self.payload_len = Self::payload_length(&blob);
        let payload_len = self.payload_len;

        let rendered = profile
            .serialize(payload_len)
            .map_err(TransportError::Protocol)?;

        // Start from the device's own bytes, then overlay only known fields.
        let mut payload = vec![0u8; CONFIG_REPORT_SIZE];
        let keep = payload_len.min(blob.len());
        payload[..keep].copy_from_slice(&blob[..keep]);

        // The byte set depends on the X/Y flag, because the resolutions occupy
        // twice as many bytes when it is set. Writing the ordinary layout's
        // bytes over an X/Y profile would leave the second value of each slot
        // at whatever the device had, so the two would disagree on the mouse.
        let known = config_bytes_to_write(profile.xy_independent);
        for index in known {
            if index < payload_len && index < rendered.len() {
                payload[index] = rendered[index];
            }
        }
        payload[0] = REPORT_ID_CONFIG;
        payload[1] = command;
        payload[3] = length_byte;

        // One transfer only. The driver does not prime the device with a command
        // report first, and sending one makes the firmware treat the following
        // blob as a reply to that command and ignore it.
        self.transport.set_feature_report(&payload)?;
        self.cached = None;
        Ok(())
    }

    /// Write one byte of the configuration report, leaving every other byte at
    /// whatever the device currently reports.
    ///
    /// This is a mapping tool, not a settings path. `write_profile` overlays
    /// only the fields it understands, which is what keeps it from clearing
    /// settings the user never touched, but it cannot be used to explore a field
    /// whose meaning is unknown. This one changes exactly one byte and carries
    /// the rest over untouched, so a change in what the mouse does can be
    /// attributed to that byte.
    pub fn write_single_byte(
        &mut self,
        profile: &Profile,
        offset: usize,
        value: u8,
    ) -> Result<(), TransportError> {
        let command = config_command(profile.index);
        let blob = self.read_config(command)?;
        if blob.len() < MIN_CONFIG_SIZE || blob[1] != command {
            return Err(TransportError::Protocol(
                "device returned an unusable configuration blob".into(),
            ));
        }
        let payload_len = Self::payload_length(&blob);
        if offset >= payload_len || offset >= CONFIG_REPORT_SIZE {
            return Err(TransportError::Protocol(format!(
                "offset {offset} is outside the payload of {payload_len} bytes"
            )));
        }

        let mut payload = vec![0u8; CONFIG_REPORT_SIZE];
        let keep = payload_len.min(blob.len());
        payload[..keep].copy_from_slice(&blob[..keep]);
        payload[offset] = value;
        payload[0] = REPORT_ID_CONFIG;
        payload[1] = command;
        payload[3] = Self::MODEL_O_LENGTH_BYTE;

        self.transport.set_feature_report(&payload)?;
        self.cached = None;
        Ok(())
    }
}

/// The mouse implementation used by the app.
pub type HidMouse = Mouse<crate::transport::HidTransport>;
