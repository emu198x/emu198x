//! Integration boot test for the Scorpion ZS-256.
//!
//! Loads the four Scorpion ROM banks from
//! `~/.emu198x/roms/scorpion-zs256/{scorpion-0..3}.rom` and verifies
//! that the Service ROM executes — CPU progresses past reset, runs
//! instructions, ends up somewhere in ROM with interrupts enabled in
//! IM 1, and writes scratch state into paged RAM banks.
//!
//! **Known gap.** The Scorpion's Service ROM does not currently paint
//! to standard screen RAM at $4000-$5AFF during boot, so this test
//! cannot use "nonzero bytes in screen RAM" as the boot signal like
//! the other variants do. See `runtime-sinclair-zx-spectrum`'s
//! `probe_scorpion_screen_ram` diagnostic for the full picture.
//!
//! `#[ignore]`d because not every developer has the ROMs locally — the
//! runner prints a path hint and skips when they're missing.

use common_sinclair_zx_spectrum::memory::MemoryBus;
use machine_scorpion_zs256::ScorpionZS256;
use std::path::PathBuf;

fn rom_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms/scorpion-zs256"))
}

#[test]
#[ignore = "requires local Scorpion ROMs at ~/.emu198x/roms/scorpion-zs256/{scorpion-0..3}.rom"]
fn boot_runs_service_rom() {
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

    for _ in 0..400 {
        machine.run_frame();
    }

    // Liveness check 1: CPU should have moved off the reset vector.
    let pc = machine.z80.regs.pc;
    assert_ne!(
        pc, 0x0000,
        "Scorpion CPU stuck at reset vector after 400 frames"
    );

    // Liveness check 2: interrupts should be enabled in IM 1, which the
    // Service ROM does early in its init.
    assert!(
        machine.z80.regs.iff1,
        "Service ROM should have enabled interrupts (IFF1) by now"
    );
    assert_eq!(
        machine.z80.regs.im, 1,
        "Service ROM should run in IM 1, found {}",
        machine.z80.regs.im
    );

    // Liveness check 3: at least one RAM bank should have non-trivial
    // scratch state. We sweep every bank since the standard screen
    // bank (5) is not painted during boot — a known upstream issue.
    let mut total = 0usize;
    for bank_idx in 0u8..16 {
        machine.memory.write_7ffd(bank_idx & 0x07);
        machine.memory.write_1ffd((bank_idx >> 3) & 0x01);
        total += (0xC000u16..=0xFFFF)
            .filter(|&addr| machine.memory.read(addr) != 0)
            .count();
    }
    assert!(
        total > 50,
        "Scorpion should have written scratch state to RAM after boot \
         (got {total} non-zero bytes across all 16 banks)"
    );
}
