//! The 800XL booting real firmware — one nobody needs permission for.
//!
//! `os_boot.rs` beside this file boots Atari's own OS ROM, which cannot be
//! distributed, so it only ever runs on a machine that already has it. This
//! runs the same machine against [AltirraOS](https://www.virtualdub.org/altirra.html):
//! Avery Lee's reimplementation of the XL/XE OS, written so that emulators
//! need no Atari ROM, together with Altirra BASIC in place of Atari BASIC.
//!
//! ## Licence
//!
//! The emulator is GPLv2, but the kernel ROM carries its own and more
//! permissive notice, stated in `src/Kernel/source/main.xasm` upstream:
//!
//! > Copying and distribution of this file, with or without modification,
//! > are permitted in any medium without royalty provided the copyright
//! > notice and this notice are preserved. This file is offered as-is,
//! > without any warranty.
//!
//! It is **not** Atari's OS. A title reaching past the documented entry
//! points may behave differently. For "does this machine start", that does
//! not matter.
//!
//! Provisioned from the corpora store; `EMU198X_ROMS_ROOT` is the firmware
//! root and this joins `atari-800xl/` onto it.

use std::path::PathBuf;

use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

fn rom_dir() -> Option<PathBuf> {
    let root = std::env::var_os("EMU198X_ROMS_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms")))?;
    Some(root.join("atari-800xl"))
}

/// The text window as characters. Atari stores *internal* codes rather than
/// ATASCII: `$00-$3F` are ATASCII `$20-$5F`, so a letter is not its own
/// value plus a constant the way a C64 screen code is. Getting this wrong
/// renders a perfectly good boot screen as line noise, which is how the
/// first reading of this test's output was misdiagnosed as a hang.
fn screen_text(system: &Atari800xl) -> String {
    // SAVMSC ($58/$59) points at the top-left of the text window.
    let savmsc = u16::from(system.peek(0x58)) | (u16::from(system.peek(0x59)) << 8);
    let mut out = String::with_capacity(24 * 41);
    for row in 0..24u16 {
        for col in 0..40u16 {
            let code = system.peek(savmsc.wrapping_add(row * 40 + col)) & 0x7F;
            out.push(match code {
                0x00..=0x3F => (code + 0x20) as char,
                0x60..=0x7F => code as char,
                _ => ' ',
            });
        }
        out.push('\n');
    }
    out
}

#[test]
#[ignore = "FIXTURE: needs AltirraOS at <EMU198X_ROMS_ROOT>/atari-800xl/{altirraos_xl,altirra_basic}.rom"]
fn altirraos_cold_starts_to_a_basic_prompt() {
    let Some(dir) = rom_dir() else {
        emu198x_test_skip::skip!("neither EMU198X_ROMS_ROOT nor HOME is set");
    };
    let os_path = dir.join("altirraos_xl.rom");
    if !os_path.exists() {
        emu198x_test_skip::skip!("AltirraOS not staged at {}", dir.display());
    }

    let os = std::fs::read(&os_path).expect("AltirraOS should read");
    let basic = std::fs::read(dir.join("altirra_basic.rom")).expect("Altirra BASIC should read");
    assert_eq!(os.len(), 0x4000, "the XL kernel is 16 KiB");
    assert_eq!(basic.len(), 0x2000, "Altirra BASIC is 8 KiB");

    let mut system = Atari800xl::new(
        Some(os),
        Some(basic),
        None,
        Atari800xlRegion::Ntsc,
        true, // BASIC enabled — without it there is no prompt to reach
    )
    .expect("AltirraOS should initialise an 800XL");

    // Reaches the prompt between 200 and 400 frames, measured rather than
    // guessed. 1000 is a deliberately loose allowance: this machine takes
    // around 6 seconds of emulated time to get there where real hardware
    // takes two or three, so the margin absorbs that without anyone having
    // to re-measure if the gap narrows. A boot test wants to fail because
    // the machine did not start, never because the budget was tight.
    for _ in 0..1000 {
        system.run_frame();
    }

    let screen = screen_text(&system);

    // Two assertions. The banner proves Altirra BASIC started and
    // identified itself, rather than the machine landing on a blank editor
    // screen; `Ready` proves it got to its prompt. A machine that hung
    // shows neither.
    assert!(
        screen.contains("Altirra") && screen.contains("BASIC"),
        "Altirra BASIC should print its banner; screen was:\n{screen}"
    );
    assert!(
        screen.contains("Ready"),
        "Altirra BASIC should reach its Ready prompt; screen was:\n{screen}"
    );
}
