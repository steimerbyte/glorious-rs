//! Open source configuration for Glorious Model O and related mice.
//!
//! Replaces the vendor utility "Glorious Model O Software v1.0.9", which is a
//! Windows-only application and no longer supported by the manufacturer. The
//! wire protocol is documented in `PROTOCOL.md`.

pub mod app;
pub mod cli;
pub mod confetti;
pub mod device;
pub mod preview;
pub mod profile;
pub mod protocol;
pub mod sparks;
pub mod theme;
pub mod transport;
pub mod ui;
pub mod worker;
