//! CLXDAT ($DFF00E) read dispatch — regression for #457.
//!
//! The A1200 (AGA) machine shared the OCS/ECS gap: no `$00E` arm in the
//! custom-register read dispatch, so CLXDAT fell through to the
//! `_ => 0xFFFF` open-bus default (every collision bit set, every read).
//! The fix routes `$00E` to AGA Lisa's collision latch (via the shared
//! OCS core). At reset, with nothing latched, CLXDAT must read `$0000`.

use machine_commodore_amiga_a1200::AmigaA1200;

#[test]
fn clxdat_read_is_wired_not_open_bus() {
    // A dummy (zero) Kickstart suffices: this drives no CPU, only the
    // custom-register read dispatch.
    let amiga = AmigaA1200::new(vec![0u8; 512 * 1024]);
    let clxdat = amiga.read_word(0x00DFF00E);
    assert_eq!(
        clxdat, 0x0000,
        "CLXDAT must route to Denise (clear at reset), not open-bus $FFFF"
    );
    assert_ne!(
        clxdat, 0xFFFF,
        "CLXDAT must not be the open-bus fallthrough"
    );
}
