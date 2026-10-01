//! HID transport built on `hidapi`.
//!
//! SinoWealth mice are composite devices. Their vendor reports live on two
//! separate collections of the same interface, and hidapi exposes them as two
//! independent `HidDevice` handles that share one VID and PID:
//!
//! - one accepts the command register, report id 5
//! - the other accepts the configuration blob, report id 4
//!
//! Neither handle works alone, so both are opened and kept, and every transfer
//! is routed by report id. Which handle is which cannot be told from the
//! product string, so it is determined by probing: a handle that rejects the
//! report with `ERROR_INVALID_PARAMETER` does not own it.

use crate::protocol::*;
use std::fmt;

/// Anything that can move a feature report. Lets the tests run without hardware.
pub trait FeatureTransport: Send {
    /// USB ids of the opened device.
    fn vendor_id(&self) -> u16;
    fn product_id(&self) -> u16;
    fn get_feature_report(
        &mut self,
        report_id: u8,
        length: usize,
    ) -> Result<Vec<u8>, TransportError>;
    fn set_feature_report(&mut self, data: &[u8]) -> Result<(), TransportError>;
}

/// Errors surfaced to the user.
#[derive(Debug)]
pub enum TransportError {
    /// No mouse with a usable vendor interface was found.
    NoDevice,
    /// The device answered, but not with what the command asked for.
    Protocol(String),
    /// The platform layer failed.
    Hid(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::NoDevice => write!(
                f,
                "no Glorious mouse found\n\nOn Linux the udev rule grants access to the \
                 hidraw nodes:\n  \
                 sudo cp udev/70-glorious-oss.rules /etc/udev/rules.d/\n  \
                 sudo udevadm control --reload-rules && sudo udevadm trigger\n\n\
                 Then unplug the mouse and plug it back in."
            ),
            TransportError::Protocol(message) => write!(f, "protocol error: {message}"),
            TransportError::Hid(message) => write!(f, "hid error: {message}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// Routes report ids to the hidapi handle that owns them.
pub struct HidTransport {
    command: hidapi::HidDevice,
    config: hidapi::HidDevice,
    /// `HidDevice` does not expose the USB ids, so they are taken from the
    /// enumeration entry that the path came from.
    vendor_id: u16,
    product_id: u16,
}

impl HidTransport {
    /// Open the mouse, searching for `vendor_id` and `product_id` when given.
    pub fn open(vendor_id: Option<u16>, product_id: Option<u16>) -> Result<Self, TransportError> {
        let api = hidapi::HidApi::new().map_err(|e| TransportError::Hid(format!("{e}")))?;
        Self::open_with(&api, vendor_id, product_id)
    }

    fn open_with(
        api: &hidapi::HidApi,
        vendor_id: Option<u16>,
        product_id: Option<u16>,
    ) -> Result<Self, TransportError> {
        let wanted = match (vendor_id, product_id) {
            (Some(vid), Some(pid)) => vec![(vid, pid)],
            _ => KNOWN_DEVICES.iter().map(|d| (d.0, d.1)).collect(),
        };

        for (vid, pid) in wanted {
            // `device_list` hands out an iterator that borrows the api, so the
            // paths are collected first and the devices opened afterwards.
            let paths: Vec<std::ffi::CString> = api
                .device_list()
                .filter(|info| info.vendor_id() == vid && info.product_id() == pid)
                .map(|info| info.path().to_owned())
                .collect();

            let mut command = None;
            let mut config = None;
            for path in paths {
                let Ok(device) = api.open_path(&path) else {
                    continue;
                };
                if config.is_none()
                    && supports_report(&device, REPORT_ID_CONFIG, CONFIG_REPORT_SIZE)
                {
                    config = Some(device);
                } else if command.is_none()
                    && supports_report(&device, REPORT_ID_COMMAND, COMMAND_SIZE)
                {
                    command = Some(device);
                }
            }

            if let (Some(command), Some(config)) = (command, config) {
                return Ok(HidTransport {
                    command,
                    config,
                    vendor_id: vid,
                    product_id: pid,
                });
            }
        }
        Err(TransportError::NoDevice)
    }
}

impl FeatureTransport for HidTransport {
    fn vendor_id(&self) -> u16 {
        self.vendor_id
    }

    fn product_id(&self) -> u16 {
        self.product_id
    }

    fn get_feature_report(
        &mut self,
        report_id: u8,
        length: usize,
    ) -> Result<Vec<u8>, TransportError> {
        let handle = if report_id == REPORT_ID_COMMAND {
            &self.command
        } else {
            &self.config
        };
        let mut buf = vec![0u8; length];
        buf[0] = report_id;
        handle
            .get_feature_report(&mut buf)
            .map_err(|e| TransportError::Hid(format!("{e}")))?;
        Ok(buf)
    }

    fn set_feature_report(&mut self, data: &[u8]) -> Result<(), TransportError> {
        let report_id = data[0];
        let handle = if report_id == REPORT_ID_COMMAND {
            &self.command
        } else {
            &self.config
        };
        handle
            .send_feature_report(data)
            .map_err(|e| TransportError::Hid(format!("{e}")))
    }
}

/// Whether the collection defines a feature report of this size.
///
/// A collection that does not define the report rejects the write with
/// `ERROR_INVALID_PARAMETER`, which is the only ownership signal available
/// because hidapi does not expose the report descriptor. The probe writes a
/// command register of zeros, which the device treats as a no-op, so it cannot
/// change any setting. On Linux the same rejection shows up as `EINVAL`.
fn supports_report(device: &hidapi::HidDevice, report_id: u8, length: usize) -> bool {
    let mut data = vec![0u8; length];
    data[0] = report_id;
    device.send_feature_report(&data).is_ok()
}
