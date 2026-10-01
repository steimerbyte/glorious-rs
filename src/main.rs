//! Window and command line entry point.

use eframe::egui;

use glorious::app::AppState;
use glorious::confetti;
use glorious::device::{DeviceState, HidMouse};
use glorious::protocol::{
    COLOR_SLOT_BASE, NUM_DPI_SLOTS, RgbEffect, raw_to_dpi, rgb_brightness_encode,
};
use glorious::sparks;
use glorious::transport::HidTransport;
use glorious::ui;
use glorious::worker::{Command, Reply, Worker};

fn main() -> eframe::Result<()> {
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

/// Read the mouse once, print its settings and exit.
fn print_settings() -> eframe::Result<()> {
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
fn probe_write() -> eframe::Result<()> {
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
fn set_byte(offset: usize, value: u8) -> eframe::Result<()> {
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
    match mouse.read_config(glorious::protocol::config_command(profile.index)) {
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
fn dump_config() -> eframe::Result<()> {
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
fn calibrate_length() -> eframe::Result<()> {
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
fn set_effect(
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
fn probe_slots() -> eframe::Result<()> {
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
fn set_colour(slot: usize, colour: [u8; 3], length_byte: u8) -> eframe::Result<()> {
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
fn locate_colour() -> eframe::Result<()> {
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
fn hex_colour(colour: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2])
}

fn read_once() -> Result<DeviceState, String> {
    let transport = HidTransport::open(None, None).map_err(|e| e.to_string())?;
    let mut mouse = HidMouse::new(transport);
    // The state is taken by value, not cloned: the call owns the device and
    // the state borrows from it, so a clone would copy the whole struct out of
    // a borrow that is about to end anyway.
    let state = mouse.state(true).map_err(|e| e.to_string())?;
    Ok(state.clone())
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
