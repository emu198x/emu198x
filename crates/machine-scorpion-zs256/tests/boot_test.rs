//! Integration boot test for the Scorpion ZS-256.
//!
//! Loads the four Scorpion ROM banks from
//! `~/.emu198x/roms/scorpion-zs256/{scorpion-0..3}.rom` and verifies the
//! machine reaches its boot menu: ROM 0 resets into the Service monitor
//! (ROM 2), which clears the upper RAM banks, probes the Beta Disk through
//! TR-DOS (ROM 3, overlay only), and returns to ROM 0 to draw the menu.
//! Every step of that path depends on the `$7FFD`/`$1FFD` decoding, so a
//! wrong paging bit leaves the screen blank or striped instead.
//!
//! `#[ignore]`d because not every developer has the ROMs locally — the
//! runner prints a path hint and skips when they're missing.

use machine_scorpion_zs256::ScorpionZS256;
use std::path::PathBuf;

fn rom_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms/scorpion-zs256"))
}

/// Address of one pixel line of a character cell in the screen bank.
fn cell_line_addr(row: u16, col: u16, line: u16) -> u16 {
    0x4000 | ((row & 0x18) << 8) | (line << 8) | ((row & 0x07) << 5) | col
}

/// Decodes one character row of the displayed screen against the 48 BASIC
/// font at `$3D00` of ROM 1. Inverse-video cells match too; unmatched cells
/// read as `?`.
fn screen_row_text(machine: &ScorpionZS256, row: u16) -> String {
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    (0..32)
        .map(|col| {
            let cell: Vec<u8> = (0..8)
                .map(|line| machine.memory.read_screen(cell_line_addr(row, col, line)))
                .collect();
            (0x20u8..0x80)
                .find(|&ch| {
                    let glyph: Vec<u8> = (0..8)
                        .map(|line| {
                            machine
                                .memory
                                .read_rom_byte(1, 0x3D00 + (u16::from(ch) - 0x20) * 8 + line)
                        })
                        .collect();
                    glyph == cell || glyph.iter().zip(&cell).all(|(g, c)| !g == *c)
                })
                .map_or('?', char::from)
        })
        .collect()
}

#[test]
#[ignore = "FIXTURE: requires local Scorpion ROMs at ~/.emu198x/roms/scorpion-zs256/{scorpion-0..3}.rom"]
fn boot_reaches_the_scorpion_menu() {
    let Some(dir) = rom_dir() else {
        emu198x_test_skip::skip!("HOME not set — cannot locate Scorpion ROMs");
    };
    let roms: [PathBuf; 4] = std::array::from_fn(|i| dir.join(format!("scorpion-{i}.rom")));
    for rom in &roms {
        if !rom.exists() {
            emu198x_test_skip::skip!("Scorpion ROM not found at {}", rom.display());
        }
    }

    let mut machine = ScorpionZS256::new();
    for (i, rom) in roms.iter().enumerate() {
        machine
            .memory
            .load_rom(i, rom)
            .unwrap_or_else(|e| panic!("Scorpion ROM {i} should load: {e}"));
    }

    for _ in 0..300 {
        machine.run_frame();
    }

    let screen: Vec<String> = (0..24).map(|row| screen_row_text(&machine, row)).collect();
    let shows = |text: &str| screen.iter().any(|line| line.contains(text));
    for item in [
        "Scorpion ZS 256",
        "128 TR-DOS",
        "128 BASIC",
        "48 BASIC",
        "48 TR-DOS",
    ] {
        assert!(
            shows(item),
            "Scorpion boot menu should show {item:?} after 300 frames; screen reads:\n{}",
            screen.join("\n")
        );
    }

    assert_eq!(machine.memory.current_rom(), 0, "menu runs from ROM 0");
    assert!(
        machine.z80.regs.iff1 && machine.z80.regs.im == 1,
        "menu waits for keys with interrupts on in IM 1"
    );
}
