//! M3: OVL clear via CIA-A.
//!
//! Per `wiki/decisions/amiga-restart-plan.md`. The very early KS 1.3
//! boot does the OVL handshake at $FC00FE-$FC010C:
//!
//! ```
//! $FC00FE: MOVE.B #$03, $00BFE201  ; CIA-A DDRA = bits 0+1 outputs
//! $FC0106: MOVE.B #$02, $00BFE001  ; CIA-A PRA = bit 1 high (LED off),
//!                                  ;             bit 0 LOW (OVL clear)
//! ```
//!
//! After this handshake, Agnus deasserts the overlay and chip RAM
//! becomes visible at `$00000000`. Reads from `$0` no longer return
//! the Kickstart SSP cookie ($11), they return zero (cleared chip
//! RAM).

use std::path::PathBuf;
use machine_commodore_amiga_ocs::AmigaOcs;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

#[test]
fn boot_clears_overlay_via_cia_a() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Sanity: at construction OVL=1 so $0 reads ROM byte $11 (high
    // byte of the SSP cookie $11144EF9).
    assert_eq!(amiga.read_word(0x000000) >> 8, 0x11);
    assert!(amiga.memory().overlay());

    // Run past the diag-ROM probe (which has the busy-wait delay)
    // and into the CIA-A setup. The boot reaches $FC0106 within
    // ~2.5M CCKs (delay loop dominates).
    for _ in 0..3_000_000 {
        amiga.tick();
    }

    assert!(
        !amiga.memory().overlay(),
        "OVL should be cleared after boot's CIA-A PRA write"
    );

    // With OVL clear, $0 reads cleared chip RAM (not ROM SSP byte).
    assert_eq!(
        amiga.read_word(0x000000),
        0,
        "After OVL clear, $0 should return chip-RAM zero, not ROM"
    );
}

#[test]
fn cia_a_register_dispatch() {
    // Synthetic test of CIA-A address decoding. CIA-A is on the
    // low byte (D0-7) at ODD addresses; registers selected by
    // addr bits 8-11.
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Set DDRA = $03 first (enable bit 0 + 1 as outputs).
    amiga.poke_byte(0x00BFE201, 0x03);
    assert_eq!(amiga.cia_a_ddra(), 0x03);

    // PRA = $02: bit 0 LOW (OVL clear), bit 1 HIGH.
    amiga.poke_byte(0x00BFE001, 0x02);
    assert_eq!(amiga.cia_a_pra(), 0x02);
    assert!(!amiga.memory().overlay());

    // PRA = $03: bit 0 HIGH (OVL set again).
    amiga.poke_byte(0x00BFE001, 0x03);
    assert!(amiga.memory().overlay());
}
