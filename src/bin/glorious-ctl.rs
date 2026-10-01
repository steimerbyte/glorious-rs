//! Command line entry point.
//!
//! A separate binary from the window on purpose. A Windows GUI subsystem
//! binary has no console attached, so `--dpi` and the mapping commands would
//! print into nothing when launched from Explorer or a shortcut, and their
//! output is the only way the work in this project is checked. The window gets
//! no console window because it is built with the Windows subsystem; this one
//! keeps its console because it is built without it, and the two decisions do
//! not interfere with each other.

// The result is handed to `main` rather than thrown away: a command that could
// not be run has to be reported on the console this binary has, and dropping the
// value would print it under the console-less GUI subsystem instead.
fn main() -> eframe::Result<()> {
    glorious::cli::run()
}
