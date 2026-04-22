//! Phase 2 machine-level integration tests for the blitter.
//!
//! Closes tasks #142–#147: the blitter register bus, register
//! dispatch, run-to-completion on BLTSIZE write, and INT_BLIT on
//! completion. Per-slot contention pacing is deferred to a future
//! task (#147 refinement) — the current model runs blits
//! synchronously, which matches the Amiga semantics CPU code expects
//! (BBUSY clears before the next instruction).

use machine_commodore_amiga_ocs::{AmigaOcs, CiaExt, IntSource};

fn zero_rom() -> Vec<u8> {
    vec![0; 512 * 1024]
}

/// Disable the OVL overlay so read_word reads chip RAM, not ROM. OVL
/// = CIA-A PRA bit 0; the Amiga ROM drives this low immediately after
/// reset. Tests that exercise chip-RAM behaviour do the same.
fn disable_ovl(amiga: &mut AmigaOcs) {
    // DDRA bit 0 = output.
    amiga.poke_byte(0x00BF_E201, 0x03);
    // PRA bit 0 = 0 → OVL low → chip RAM at $0.
    amiga.poke_byte(0x00BF_E001, 0x00);
    assert!(!amiga.memory().overlay());
}

#[test]
fn bltsize_write_drives_a_one_word_copy_to_completion() {
    let mut amiga = AmigaOcs::new(zero_rom());
    disable_ovl(&mut amiga);
    amiga.poke_word(0x0000_1000, 0xCAFE);

    // Program BLTCON0 = USEA+USED + minterm $F0 (D = A).
    amiga.poke_word(0x00DF_F040, 0x0900 | 0xF0);
    // BLTCON1 = 0 (no fill, no line, no descend).
    amiga.poke_word(0x00DF_F042, 0x0000);
    // Masks wide open.
    amiga.poke_word(0x00DF_F044, 0xFFFF);
    amiga.poke_word(0x00DF_F046, 0xFFFF);
    // BLTAPT = $0000_1000, BLTDPT = $0000_2000.
    amiga.poke_word(0x00DF_F050, 0x0000); // BLTAPTH
    amiga.poke_word(0x00DF_F052, 0x1000); // BLTAPTL
    amiga.poke_word(0x00DF_F054, 0x0000); // BLTDPTH
    amiga.poke_word(0x00DF_F056, 0x2000); // BLTDPTL
    // BLTSIZE = 1 row × 1 word — writing this triggers the blit.
    amiga.poke_word(0x00DF_F058, (1 << 6) | 1);

    // After the synchronous completion model, the destination holds
    // the copied word and INT_BLIT is pending.
    assert_eq!(amiga.read_word(0x0000_2000), 0xCAFE);
    assert_ne!(
        amiga.intreq() & IntSource::Blit.mask(),
        0,
        "INT_BLIT should be raised when the blit completes"
    );
}

#[test]
fn bltsize_write_raises_blit_ipl_3_when_enabled() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09A, 0xC040); // SET INTEN + BLIT
    // Smallest valid blit: D-only, 1×1.
    amiga.poke_word(0x00DF_F040, 0x0100); // USED only, lf=0
    amiga.poke_word(0x00DF_F054, 0x0000);
    amiga.poke_word(0x00DF_F056, 0x3000);
    amiga.poke_word(0x00DF_F058, (1 << 6) | 1);

    // Blit has completed synchronously; one tick settles the IPL.
    amiga.tick();
    assert_eq!(amiga.cpu().ipl, 3, "INT_BLIT → IPL 3 per HRM");
}

#[test]
fn two_row_two_word_copy_exercises_amod_and_dmod_paths() {
    let mut amiga = AmigaOcs::new(zero_rom());
    disable_ovl(&mut amiga);
    // Source: 4 words at $1000..=$1006.
    for (i, w) in [0x1111u16, 0x2222, 0x3333, 0x4444].into_iter().enumerate() {
        amiga.poke_word(0x0000_1000 + (i as u32) * 2, w);
    }

    amiga.poke_word(0x00DF_F040, 0x0900 | 0xF0);
    amiga.poke_word(0x00DF_F042, 0x0000);
    amiga.poke_word(0x00DF_F044, 0xFFFF);
    amiga.poke_word(0x00DF_F046, 0xFFFF);
    amiga.poke_word(0x00DF_F050, 0x0000);
    amiga.poke_word(0x00DF_F052, 0x1000);
    amiga.poke_word(0x00DF_F054, 0x0000);
    amiga.poke_word(0x00DF_F056, 0x2000);
    // AMOD / DMOD both 0 — contiguous source + contiguous dest.
    amiga.poke_word(0x00DF_F064, 0); // BLTAMOD
    amiga.poke_word(0x00DF_F066, 0); // BLTDMOD
    amiga.poke_word(0x00DF_F058, (2 << 6) | 2);

    for (i, expected) in [0x1111u16, 0x2222, 0x3333, 0x4444].into_iter().enumerate() {
        assert_eq!(
            amiga.read_word(0x0000_2000 + (i as u32) * 2),
            expected,
            "word {i} should have been copied"
        );
    }
}
