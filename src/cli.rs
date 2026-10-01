//! The command line side of the tool, kept apart from the window.
//!
//! Everything in here exists to find out how the device behaves, and each
//! command proves something by reading or writing the mouse and printing what
//! came back. That output is the only evidence there is: the device reports a
//! written value back unchanged, so it cannot confirm that a field did
//! anything, and a measurement nobody can read is not a measurement.
//!
//! That is why this is a module of its own and ends up in a binary of its own.
//! A Windows binary built for the GUI subsystem has no console, so everything
//! it printed would be lost the moment it was started from Explorer or a
//! shortcut. The window in `main.rs` is therefore built with the GUI subsystem
//! and opens no console window, `glorious-ctl` is built without that attribute
//! and keeps its console, and neither decision constrains the other. A single
//! attribute on a single entry point could only have bought one of the two.

use crate::device::{DeviceState, HidMouse};
use crate::protocol::{
    COLOR_SLOT_BASE, NUM_DPI_SLOTS, RgbEffect, config_command, raw_to_dpi, rgb_brightness_encode,
};
use crate::transport::HidTransport;

/// Run one command and exit.
///
/// A command line that names no command does not open the window: that is a
/// separate binary, so there is nothing here to fall through to. It prints
/// which names there are and exits with the usage status instead, because a
/// mistyped flag is the likeliest reason to be standing here, and a command
/// that exits silently cannot be told apart from one that worked.
pub fn run() -> eframe::Result<()> {
    let first = std::env::args().nth(1);
    // `--dpi` prints the settings and exits, for use from a terminal.
    if first.as_deref() == Some("--dpi") {
        return print_settings();
    }
    // `--set-effect N [RRGGBB] [BB]` writes the effect selector, optionally the
    // colour that effect uses, and optionally a brightness byte. The brightness
    // is a whole mode byte, because only its high nibble was measured: the low
    // one changed nothing visible at 16, 32 or 64 and is carried through.
    //
    // Both the colour and the brightness are read as hex, the colour because
    // that is how the tool prints and takes it everywhere, the brightness
    // because the values that were measured are written in hex everywhere else.
    // A brightness of `40` meaning `0x28` would be a trap, since 0x40 is one
    // of the three values with a reading behind it.
    if first.as_deref() == Some("--set-effect") {
        let mut rest = std::env::args().skip(2);
        let effect: u8 = rest.next().and_then(|a| a.parse().ok()).unwrap_or(2);
        let colour = rest
            .next()
            .and_then(|a| u32::from_str_radix(&a, 16).ok())
            .map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8]);
        let brightness = rest
            .next()
            .and_then(|a| u8::from_str_radix(a.trim_start_matches("0x"), 16).ok());
        return set_effect(effect, colour, brightness);
    }
    // `--probe-slots` writes a distinct DPI into every slot, which is how the
    // number of slots the device really keeps is established: the vendor
    // software only offers six, while the report has room for eight.
    if first.as_deref() == Some("--probe-slots") {
        return probe_slots();
    }
    // `--set-colour SLOT RRGGBB [BYTE3]` writes one colour and reports what the
    // device stored, which is how the colour bytes get mapped.
    if first.as_deref() == Some("--set-colour") {
        let mut rest = std::env::args().skip(2);
        let slot: usize = rest.next().and_then(|a| a.parse().ok()).unwrap_or(1);
        let colour = rest
            .next()
            .and_then(|a| u32::from_str_radix(&a, 16).ok())
            .map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8])
            .unwrap_or([255, 0, 255]);
        let length_byte: u8 = rest.next().and_then(|a| a.parse().ok()).unwrap_or(122);
        return set_colour(slot.saturating_sub(1), colour, length_byte);
    }
    // `--calibrate-length` finds the value byte 3 needs, which the device
    // silently depends on.
    if first.as_deref() == Some("--calibrate-length") {
        return calibrate_length();
    }
    // `--locate-colour` writes a distinct colour per slot and diffs the blob,
    // which is the only reliable way to find the colour offsets.
    if first.as_deref() == Some("--locate-colour") {
        return locate_colour();
    }
    // `--dump-config` prints the raw blob with its offset column, so the layout
    // can be read off the device instead of guessed.
    if first.as_deref() == Some("--dump-config") {
        return dump_config();
    }
    // `--probe-write` writes one field, reads it back and restores the original,
    // so a write can be verified without guessing what the device stored.
    if first.as_deref() == Some("--probe-write") {
        return probe_write();
    }

    // `--set-byte OFFSET WERT` writes one byte of the configuration report and
    // leaves every other byte at whatever the device reported. It exists for
    // mapping a field whose meaning is not known: one value at a time, with
    // everything else held still, which is the only way to tell a field that
    // does something from one that is simply stored.
    if first.as_deref() == Some("--set-byte") {
        let mut rest = std::env::args().skip(2);
        let offset: usize = match rest.next().and_then(|a| a.parse().ok()) {
            Some(offset) => offset,
            None => {
                eprintln!("usage: --set-byte OFFSET WERT");
                std::process::exit(2);
            }
        };
        let value: u8 = match rest.next().and_then(|a| a.parse().ok()) {
            Some(value) => value,
            None => {
                eprintln!("usage: --set-byte OFFSET WERT");
                std::process::exit(2);
            }
        };
        return set_byte(offset, value);
    }
    eprintln!("usage: glorious-ctl takes one of --dpi, --set-effect, --probe-slots,");
    eprintln!("       --set-colour, --calibrate-length, --locate-colour,");
    eprintln!("       --dump-config, --probe-write, --set-byte;");
    eprintln!("       run glorious-rs to open the window");
    // Never returns, so the tail expression is the exit and not a missing
    // result. Two is the usage status, the same one the argument error above
    // uses, because that is what this is.
    std::process::exit(2)
}

/// Read the mouse once, print its settings and exit.
pub fn print_settings() -> eframe::Result<()> {
    let state = match read_once() {
        Ok(state) => state,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    println!(
        "device      {}  ({:04x}:{:04x})",
        state.name, state.vendor_id, state.product_id
    );
    println!("firmware    {}", state.firmware);
    println!("sensor      {}", state.profile.sensor_name());
    println!("profile     {}", state.active_profile);
    println!("report rate {} Hz", state.report_rate());
    if let Some(debounce) = state.debounce_ms {
        println!("debounce    {debounce} ms");
    }
    println!("active DPI  {}", state.profile.active_dpi());
    let active = state.profile.active_slot_index();
    for (index, slot) in state.profile.slots.iter().enumerate() {
        let marker = if active == Some(index) { '*' } else { ' ' };
        let note = if slot.disabled { " (disabled)" } else { "" };
        println!(" {marker} slot {}: {} dpi{note}", index + 1, slot.dpi);
    }
    Ok(())
}

/// Verify the write path by changing one field, reading it back and putting the
/// original value back.
///
/// This is the only honest way to test a write: the device decides whether it
/// accepted the blob, so the only evidence is a second read. The original value
/// is always restored, so the mouse is left as it was found.
pub fn probe_write() -> eframe::Result<()> {
    let transport = match HidTransport::open(None, None) {
        Ok(transport) => transport,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let mut mouse = HidMouse::new(transport);

    let original = match mouse.state(true) {
        Ok(state) => state.profile.clone(),
        Err(error) => {
            eprintln!("read failed: {error}");
            std::process::exit(1);
        }
    };
    println!("original:");
    for (index, slot) in original.slots.iter().enumerate() {
        println!(
            "  slot {}: {:>5} dpi  colour {}  {}",
            index + 1,
            slot.dpi,
            hex_colour(slot.color),
            if slot.disabled { "off" } else { "on" }
        );
    }

    // Pick a target that cannot be confused with what is already stored: the
    // active slot's colour becomes pure magenta.
    let target = original.active_slot_index().unwrap_or(0);
    let mut changed = original.clone();
    changed.slots[target].color = [255, 0, 255];

    println!("\nwriting slot {} colour to ff00ff", target + 1);
    if let Err(error) = mouse.write_profile(&changed) {
        eprintln!("write failed: {error}");
        std::process::exit(1);
    }

    let after = match mouse.state(true) {
        Ok(state) => state.profile.clone(),
        Err(error) => {
            eprintln!("read after write failed: {error}");
            std::process::exit(1);
        }
    };
    println!("after write:");
    for (index, slot) in after.slots.iter().enumerate() {
        println!(
            "  slot {}: {:>5} dpi  colour {}  {}",
            index + 1,
            slot.dpi,
            hex_colour(slot.color),
            if slot.disabled { "off" } else { "on" }
        );
    }

    let written = after.slots[target].color;
    let accepted = written == [255, 0, 255];
    println!("\ncolour at slot {}: {}", target + 1, hex_colour(written));
    println!("write accepted: {}", if accepted { "yes" } else { "no" });

    if accepted {
        // Put the original back so the mouse keeps its settings.
        println!("restoring the original colour");
        if let Err(error) = mouse.write_profile(&original) {
            eprintln!("restore failed: {error}");
            std::process::exit(1);
        }
        let restored = mouse.state(true).map(|s| s.profile.slots[target].color);
        match restored {
            Ok(colour) if colour == original.slots[target].color => {
                println!("restored: {}", hex_colour(colour));
            }
            Ok(colour) => println!(
                "restored to {}, expected {}",
                hex_colour(colour),
                hex_colour(original.slots[target].color)
            ),
            Err(error) => println!("could not verify the restore: {error}"),
        }
    } else {
        println!("\nthe device did not store the value; the original is untouched");
    }
    Ok(())
}

/// Write one byte of the configuration report and report what the device kept.
///
/// Everything else is carried over from the device's own blob. Reading the byte
/// straight back does not say whether the setting took effect, so the report
/// here is about storage; what the mouse does with it is a question for the
/// camera.
pub fn set_byte(offset: usize, value: u8) -> eframe::Result<()> {
    let transport = match HidTransport::open(None, None) {
        Ok(transport) => transport,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let mut mouse = HidMouse::new(transport);
    let profile = match mouse.state(true) {
        Ok(state) => state.profile.clone(),
        Err(error) => {
            eprintln!("read failed: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = mouse.write_single_byte(&profile, offset, value) {
        eprintln!("write failed: {error}");
        std::process::exit(1);
    }
    println!("wrote {value:#04x} to byte {offset}");

    // The device answers a fresh read with the state from before the write, so
    // this is reported separately and not taken as confirmation.
    match mouse.read_config(config_command(profile.index)) {
        Ok(blob) if offset < blob.len() => println!(
            "byte {offset} reads back as {:#04x}{}",
            blob[offset],
            if blob[offset] == value {
                ""
            } else {
                " (not the value written)"
            }
        ),
        Ok(_) => {}
        Err(error) => println!("read back failed: {error}"),
    }
    Ok(())
}

/// Print the raw configuration blob with offsets, so the field layout can be
/// read off the device instead of inferred.
pub fn dump_config() -> eframe::Result<()> {
    let transport = match HidTransport::open(None, None) {
        Ok(transport) => transport,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let mut mouse = HidMouse::new(transport);
    let blob = match mouse.read_config(0x11) {
        Ok(blob) => blob,
        Err(error) => {
            eprintln!("read failed: {error}");
            std::process::exit(1);
        }
    };

    println!("{} bytes read", blob.len());
    let mut last_nonzero = 0;
    for (index, byte) in blob.iter().enumerate() {
        if *byte != 0 {
            last_nonzero = index;
        }
    }
    println!("last non-zero byte: {last_nonzero}\n");
    for (row, chunk) in blob.chunks(16).enumerate() {
        let base = row * 16;
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let ascii: String = chunk
            .iter()
            .map(|b| {
                if (32..127).contains(b) {
                    *b as char
                } else {
                    '.'
                }
            })
            .collect();
        println!("{base:3}: {}  {ascii}", hex.join(" "));
    }
    Ok(())
}

/// Find the value byte 3 needs, and whether a write works at all.
///
/// The device takes no notice of a write it dislikes: it either stores the
/// change or blanks the whole configuration, and reports neither. This writes
/// one colour with each candidate for byte 3, reads the blob back, and checks
/// three things: did the colour change, are the DPI values still there, and is
/// byte 8 still the value the device had. The original is restored after every
/// candidate.
pub fn calibrate_length() -> eframe::Result<()> {
    let transport = match HidTransport::open(None, None) {
        Ok(transport) => transport,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let mut mouse = HidMouse::new(transport);
    let original = match mouse.state(true) {
        Ok(state) => state.profile.clone(),
        Err(error) => {
            eprintln!("read failed: {error}");
            std::process::exit(1);
        }
    };
    let baseline = mouse.read_config(0x11).unwrap_or_default();

    // A value the device cannot already hold, so a match cannot be a leftover.
    let probe_colour: [u8; 3] = [1, 2, 3];
    println!(
        "writing slot 1 colour {} with each byte 3 value\n",
        hex_colour(probe_colour)
    );
    println!("byte 3   colour stored   DPI kept   byte 8 kept");
    for length in [
        0u8, 57, 64, 65, 96, 122, 123, 126, 127, 128, 129, 130, 131, 159, 167, 0xff,
    ] {
        let mut probe = original.clone();
        probe.slots[0].color = probe_colour;
        if let Err(error) = mouse.write_profile_with_length(&probe, length) {
            println!("{length:>6}   write failed: {error}");
            continue;
        }
        let after = mouse.read_config(0x11).unwrap_or_default();
        let stored = after.len() > COLOR_SLOT_BASE + 2
            && after[COLOR_SLOT_BASE..COLOR_SLOT_BASE + 3] == probe_colour;
        let dpi_ok = after.len() > 20 && after[13..21] == baseline[13..21];
        let byte8_ok = after.len() > 8 && after[8] == baseline[8];
        println!(
            "{length:>6}   {:<14} {:<9} {}",
            if stored { "yes" } else { "no" },
            if dpi_ok { "yes" } else { "LOST" },
            if byte8_ok { "yes" } else { "LOST" }
        );

        // Restore before the next candidate, always with a length byte the
        // device accepts: 122 was measured to work. Restoring with the rejected
        // candidate would leave the mouse holding a half written profile.
        let _ = mouse.write_profile(&original);
    }

    print!("\nrestoring the original");
    match mouse.write_profile(&original) {
        Ok(()) => println!(" done"),
        Err(error) => println!(" failed: {error}"),
    }
    Ok(())
}

/// Write the lighting effect selector, its colour and its brightness.
///
/// The user recognises the result by what the mouse shows. That is what maps
/// the fields: the device reports them back unchanged, so a read cannot confirm
/// which byte ended up driving the LEDs.
pub fn set_effect(
    effect_byte: u8,
    colour: Option<[u8; 3]>,
    brightness: Option<u8>,
) -> eframe::Result<()> {
    let transport = match HidTransport::open(None, None) {
        Ok(transport) => transport,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let mut mouse = HidMouse::new(transport);
    let mut profile = match mouse.state(true) {
        Ok(state) => state.profile.clone(),
        Err(error) => {
            eprintln!("read failed: {error}");
            std::process::exit(1);
        }
    };
    let previous = profile.rgb_effect;
    let effect = match RgbEffect::from_byte(effect_byte) {
        Some(effect) => effect,
        None => {
            eprintln!(
                "effect {effect_byte:#04x} is not one this tool offers: {}",
                RgbEffect::offered()
                    .iter()
                    .map(|e| format!("{}={:#04x}", e.name(), e.as_byte()))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            std::process::exit(1);
        }
    };
    if !effect.is_writable() {
        eprintln!(
            "{} would leave the LEDs dark and is not offered: {}",
            effect.name(),
            RgbEffect::offered()
                .iter()
                .map(|e| format!("{}={:#04x}", e.name(), e.as_byte()))
                .collect::<Vec<_>>()
                .join(", ")
        );
        std::process::exit(1);
    }
    profile.rgb_effect = Some(effect);
    let previous = previous.map(|e| e.name()).unwrap_or("unbekannt");
    if let Some(colour) = colour {
        profile.rgb_single_color = colour;
        println!(
            "effect was {previous}, writing {} with colour {}",
            effect.name(),
            hex_colour(colour)
        );
    } else {
        println!("effect was {previous}, writing {}", effect.name());
    }
    // The brightness lands in a different byte per effect, and only the two
    // effects that have one were measured. An effect without a measured field is
    // refused rather than written, because a brightness that quietly goes
    // somewhere else is a change the user did not ask for.
    if let Some(brightness) = brightness {
        let current_low = match effect {
            RgbEffect::Breathing7 => profile.rgb_breathing7_mode & 0x0f,
            other if other.has_solid_colour() => profile.rgb_single_mode & 0x0f,
            other => {
                eprintln!(
                    "{} has no brightness field that has been measured, so {brightness:#04x} is not written",
                    other.name()
                );
                std::process::exit(1);
            }
        };
        let mode_byte = rgb_brightness_encode(brightness, current_low);
        match effect {
            RgbEffect::Breathing7 => profile.rgb_breathing7_mode = mode_byte,
            _ => profile.rgb_single_mode = mode_byte,
        }
        println!("brightness {brightness:#04x}, low nibble {current_low:#04x} kept");
    }
    if let Err(error) = mouse.write_profile(&profile) {
        eprintln!("write failed: {error}");
        std::process::exit(1);
    }
    let blob = mouse.read_config(0x11).unwrap_or_default();
    let dump: Vec<String> = blob
        .iter()
        .skip(50)
        .take(14)
        .map(|b| format!("{b:02x}"))
        .collect();
    println!("bytes 50..63 now: {}", dump.join(" "));
    Ok(())
}

/// Find out how many DPI slots the device really keeps.
///
/// The vendor software offers six, while the configuration report has room for
/// eight, and byte 11 counts only the slots that are switched on. So all eight
/// are written with a distinct resolution and switched on, and the report is read
/// back: a value that survives is a slot the mouse can actually use.
///
/// The original settings are restored at the end.
pub fn probe_slots() -> eframe::Result<()> {
    let transport = match HidTransport::open(None, None) {
        Ok(transport) => transport,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let mut mouse = HidMouse::new(transport);
    let original = match mouse.state(true) {
        Ok(state) => state.profile.clone(),
        Err(error) => {
            eprintln!("read failed: {error}");
            std::process::exit(1);
        }
    };

    // Byte 11 carries the number of slots that are switched on, and the device
    // rejects the whole profile when that does not match the mask. Enabling all
    // eight at once was refused outright, so how many it accepts is established
    // by trying each count and seeing which one takes.
    let probes: [u16; NUM_DPI_SLOTS] = [300, 600, 900, 1200, 1500, 1800, 2100, 2400];
    for wanted in 1..=NUM_DPI_SLOTS as u8 {
        let mut probe = original.clone();
        for (index, probe_dpi) in probes.iter().enumerate() {
            probe.slots[index].dpi = *probe_dpi;
            probe.slots[index].disabled = (index as u8) >= wanted;
        }
        probe.dpi_count = wanted;
        if let Err(error) = mouse.write_profile(&probe) {
            eprintln!("{wanted} slots: write failed: {error}");
            continue;
        }
        let after = mouse.read_config(0x11).unwrap_or_default();
        let stored: Vec<u16> = (0..NUM_DPI_SLOTS)
            .map(|index| {
                raw_to_dpi(
                    after.get(13 + index).copied().unwrap_or(0),
                    after.get(9).copied().unwrap_or(0),
                )
            })
            .collect();
        let taken = stored
            .iter()
            .zip(probes.iter())
            .filter(|(stored, wanted)| stored == wanted)
            .count();
        println!(
            "{wanted} slots on: byte 11 = {:02x}, {} of {} resolutions took",
            after.get(11).copied().unwrap_or(0),
            taken,
            NUM_DPI_SLOTS
        );
        // Put the device back before the next candidate, so a rejected count
        // cannot leave a half written profile behind.
        let _ = mouse.write_profile(&original);
    }

    print!("\nrestoring the original");
    match mouse.write_profile(&original) {
        Ok(()) => println!(" done"),
        Err(error) => println!(" failed: {error}"),
    }
    Ok(())
}

/// Write one slot's colour, leaving everything else alone.
///
/// Used to map the colour bytes: the user reads the result off the mouse.
pub fn set_colour(slot: usize, colour: [u8; 3], length_byte: u8) -> eframe::Result<()> {
    let transport = match HidTransport::open(None, None) {
        Ok(transport) => transport,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let mut mouse = HidMouse::new(transport);
    let mut profile = match mouse.state(true) {
        Ok(state) => state.profile.clone(),
        Err(error) => {
            eprintln!("read failed: {error}");
            std::process::exit(1);
        }
    };
    if slot >= profile.slots.len() {
        eprintln!("slot {slot} does not exist");
        std::process::exit(1);
    }
    let previous = profile.slots[slot].color;
    profile.slots[slot].color = colour;
    println!(
        "slot {} was {}, writing {} with byte 3 = {length_byte}",
        slot + 1,
        hex_colour(previous),
        hex_colour(colour)
    );
    if let Err(error) = mouse.write_profile_with_length(&profile, length_byte) {
        eprintln!("write failed: {error}");
        std::process::exit(1);
    }
    let after = mouse.read_config(0x11).unwrap_or_default();
    let stored = after.len() > COLOR_SLOT_BASE + slot * 3 + 2
        && after[COLOR_SLOT_BASE + slot * 3..COLOR_SLOT_BASE + slot * 3 + 3] == colour;
    println!(
        "device reports {}",
        if stored {
            hex_colour(colour)
        } else if after.len() > COLOR_SLOT_BASE + slot * 3 + 2 {
            let o = COLOR_SLOT_BASE + slot * 3;
            hex_colour([after[o], after[o + 1], after[o + 2]])
        } else {
            "nothing, the blob is too short".to_string()
        }
    );
    Ok(())
}

/// Locate the colour bytes by changing one and diffing the blob.
///
/// The layout is not derivable by inspection: the real device stores colours
/// with a stride that does not match a simple three byte per slot array, and
/// several offsets look plausible. Writing one distinctive value into every
/// slot and diffing against the original shows exactly which byte each slot
/// owns. Every changed value is written back afterwards.
pub fn locate_colour() -> eframe::Result<()> {
    let transport = match HidTransport::open(None, None) {
        Ok(transport) => transport,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let mut mouse = HidMouse::new(transport);
    let original = match mouse.state(true) {
        Ok(state) => state.profile.clone(),
        Err(error) => {
            eprintln!("read failed: {error}");
            std::process::exit(1);
        }
    };
    let before = match mouse.read_config(0x11) {
        Ok(blob) => blob,
        Err(error) => {
            eprintln!("read failed: {error}");
            std::process::exit(1);
        }
    };

    // Every slot gets a distinct colour: slot i is filled with the byte i.
    let mut probe = original.clone();
    for index in 0..NUM_DPI_SLOTS {
        probe.slots[index].color = [index as u8, 0xA0, 0x5F];
    }
    if let Err(error) = mouse.write_profile(&probe) {
        eprintln!("write failed: {error}");
        std::process::exit(1);
    }
    let after = match mouse.read_config(0x11) {
        Ok(blob) => blob,
        Err(error) => {
            eprintln!("read after write failed: {error}");
            std::process::exit(1);
        }
    };

    println!("slot i -> colour i a0 5f");
    println!("\nbytes that changed, and the value they now hold:\n");
    for index in 0..before.len().min(after.len()) {
        if before[index] != after[index] {
            println!(
                "  offset {index:3}: {:02x} -> {:02x}",
                before[index], after[index]
            );
        }
    }

    // Put the original back.
    print!("\nrestoring the original settings");
    if let Err(error) = mouse.write_profile(&original) {
        println!(" failed: {error}");
        std::process::exit(1);
    }
    let restored = mouse.read_config(0x11).unwrap_or_default();
    let identical = restored[..before.len().min(restored.len())] == before[..];
    println!(
        "{}",
        if identical {
            " done, blob matches the original"
        } else {
            " done, blob differs from the original"
        }
    );
    if !identical {
        for index in 0..before.len().min(restored.len()) {
            if before[index] != restored[index] {
                println!(
                    "  offset {index}: {:02x} vs {:02x}",
                    before[index], restored[index]
                );
            }
        }
    }
    Ok(())
}

/// Colour as `rrggbb`, matching how the device stores it.
pub fn hex_colour(colour: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2])
}

pub fn read_once() -> Result<DeviceState, String> {
    let transport = HidTransport::open(None, None).map_err(|e| e.to_string())?;
    let mut mouse = HidMouse::new(transport);
    // The state is taken by value, not cloned: the call owns the device and
    // the state borrows from it, so a clone would copy the whole struct out of
    // a borrow that is about to end anyway.
    let state = mouse.state(true).map_err(|e| e.to_string())?;
    Ok(state.clone())
}
