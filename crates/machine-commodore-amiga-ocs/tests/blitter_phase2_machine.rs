//! Phase 2 machine-level integration tests for the blitter.
//!
//! Closes tasks #142–#147: the blitter register bus, register
//! dispatch, run-to-completion, and INT_BLIT on completion.
//!
//! Since #31 the blitter is **incremental**: writing BLTSIZE arms the
//! scheduler. The first two granted CCKs consume startup, followed by at
//! most one DMA operation per grant in the tick loop (BBUSY stays set in
//! DMACONR until it finishes) rather than completing on the register
//! write. Progress is granted only when blitter DMA is enabled (DMACON
//! DMAEN + BLTEN) and a bus slot is free, so each test enables blitter
//! DMA and then ticks the machine until the blit raises INT_BLIT. (The
//! earlier synchronous-completion model these tests first assumed
//! predated #31.)

use machine_commodore_amiga_ocs::{AmigaOcs, IntSource};

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

/// Enable blitter DMA: DMACON master DMAEN (bit 9) + BLTEN (bit 6). The
/// incremental scheduler is granted bus slots only when both are set.
fn enable_blitter_dma(amiga: &mut AmigaOcs) {
    amiga.poke_word(0x00DF_F096, 0x8000 | 0x0200 | 0x0040); // SET DMAEN|BLTEN
}

/// Tick until the internal final-D pipeline drains. INT_BLIT is allowed to
/// precede that drain on pre-AGA Agnus.
fn run_blit_to_completion(amiga: &mut AmigaOcs) {
    for _ in 0..100_000 {
        if !amiga.agnus().blitter_busy {
            assert_ne!(
                amiga.intreq() & IntSource::Blit.mask(),
                0,
                "blit must emit INT_BLIT before or at pipeline drain",
            );
            return;
        }
        amiga.tick();
    }
    panic!("blit did not raise INT_BLIT within the tick budget");
}

#[test]
fn bltsize_write_drives_a_one_word_copy_to_completion() {
    let mut amiga = AmigaOcs::new(zero_rom());
    disable_ovl(&mut amiga);
    enable_blitter_dma(&mut amiga);
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
    // BLTSIZE = 1 row × 1 word — writing this arms the blit.
    amiga.poke_word(0x00DF_F058, (1 << 6) | 1);

    run_blit_to_completion(&mut amiga);

    // The destination holds the copied word and INT_BLIT is pending.
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
    enable_blitter_dma(&mut amiga);
    // Smallest valid blit: D-only, 1×1.
    amiga.poke_word(0x00DF_F040, 0x0100); // USED only, lf=0
    amiga.poke_word(0x00DF_F054, 0x0000);
    amiga.poke_word(0x00DF_F056, 0x3000);
    amiga.poke_word(0x00DF_F058, (1 << 6) | 1);

    run_blit_to_completion(&mut amiga);
    // One more tick settles the IPL from the now-pending INT_BLIT.
    amiga.tick();
    assert_eq!(amiga.cpu().ipl, 3, "INT_BLIT → IPL 3 per HRM");
}

#[test]
fn two_row_two_word_copy_exercises_amod_and_dmod_paths() {
    let mut amiga = AmigaOcs::new(zero_rom());
    disable_ovl(&mut amiga);
    enable_blitter_dma(&mut amiga);
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

    run_blit_to_completion(&mut amiga);

    for (i, expected) in [0x1111u16, 0x2222, 0x3333, 0x4444].into_iter().enumerate() {
        assert_eq!(
            amiga.read_word(0x0000_2000 + (i as u32) * 2),
            expected,
            "word {i} should have been copied"
        );
    }
}
