//! The Aquarius BIOS beeps at boot, and this test hears it.
//!
//! The BIOS prints BEL early in its startup, bit-banging the sound pin in a
//! tight loop at `$1E6A`/`$1E70`. That beep is the machine's own proof that
//! audio is wired to the right port: an earlier model drove the speaker from
//! `$FF` (the software-lock latch), so the boot beep produced silence while
//! every synthetic test still passed.
//!
//! Sound and cassette share one physical pin on `$FC`, per Commodore-era
//! Aquarius documentation distilled in
//! `reference/by-system/mattel-aquarius/mattel-aquarius-reference.md` §2-3:
//! "Sound and cassette port use a common pin ... Sound port is a simple one
//! bit I/O and therefore it must be toggled at a specific rate under software
//! control."
//!
//! Run with:
//! ```text
//! cargo test --release -p machine-mattel-aquarius \
//!     --test boot_beep -- --ignored --nocapture
//! ```

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_mattel_aquarius::{Aquarius, AquariusRegion};

/// First match wins: the env var, then the shared local ROM directory.
fn rom_path(var: &str, name: &str) -> Option<PathBuf> {
    if let Ok(p) = env::var(var) {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home)
        .join(".emu198x/roms/mattel-aquarius")
        .join(name);
    p.exists().then_some(p)
}

#[test]
#[ignore = "FIXTURE: needs Aquarius BASIC ROM (8 KB) — run with --ignored"]
fn the_bios_boot_beep_is_audible() {
    let Some(path) = rom_path("EMU198X_AQUARIUS_BIOS", "aquarius.rom") else {
        panic!(
            "Aquarius BIOS not found — set EMU198X_AQUARIUS_BIOS or place aquarius.rom \
             at ~/.emu198x/roms/mattel-aquarius/"
        );
    };
    let mut sys = Aquarius::new(fs::read(&path).expect("read BIOS"), 0, AquariusRegion::Ntsc);
    if let Some(chars) = rom_path("EMU198X_AQUARIUS_CHAR", "aquarius-char.rom") {
        sys.set_char_rom(fs::read(chars).expect("read char ROM"));
    }

    // The beep lands within the first fifth of a second; 60 frames is ample.
    let mut samples = Vec::new();
    for _ in 0..60 {
        sys.run_frame();
        samples.extend(sys.take_audio_buffer());
    }

    let transitions = samples.windows(2).filter(|w| w[0] != w[1]).count();
    assert!(
        transitions > 100,
        "the BIOS boot beep should swing the sound pin many times, saw {transitions} \
         transitions in {} samples — is the speaker wired to the wrong port?",
        samples.len()
    );
    assert!(
        samples.iter().any(|&s| s > 0.0) && samples.iter().any(|&s| s < 0.0),
        "the beep must drive the pin both ways"
    );
}
