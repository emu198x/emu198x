//! #97 — a Workbench SAVE is a disk WRITE DMA, and it must land real
//! bytes in the ADF instead of being silently dropped.
//!
//! Drives the machine's write-DMA glue end-to-end without booting:
//! chip RAM → Paula's write slot → the drive's MFM write capture →
//! flush/decode → the disk image. A zero-filled "halt ROM" parks the
//! CPU in a `BRA.S *` self-loop up in ROM space (where OVL can't reach
//! it), so the DMA runs in isolation while the CPU does nothing.

use format_commodore_amiga_adf::{ADF_SIZE_DD, Adf};
use machine_commodore_amiga_ocs::AmigaOcs;
use peripheral_commodore_amiga_floppy::mfm::encode_mfm_track;

/// 512 KiB ROM whose reset vector parks the CPU on an infinite
/// `BRA.S *` at $F8_0008 — in ROM, immune to the OVL overlay — so it
/// never wanders into the chip RAM we drive the DMA through.
fn halt_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 512 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes()); // reset SSP
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes()); // reset PC
    rom[8] = 0x60; // BRA.S
    rom[9] = 0xFE; // -2 → branch to self
    rom
}

/// Drop the ROM overlay so chip RAM is visible at low addresses
/// (CIA-A PRA bit 0 = OVL; clear it after enabling the DDR bit).
fn disable_ovl(amiga: &mut AmigaOcs) {
    amiga.poke_byte(0x00BF_E201, 0x03);
    amiga.poke_byte(0x00BF_E001, 0x00);
}

#[test]
fn write_dma_persists_a_track_to_the_adf() {
    let mut amiga = AmigaOcs::new(halt_rom());
    disable_ovl(&mut amiga);
    amiga.insert_adf(Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid blank ADF"));

    // Build a track-0 image with a known pattern in sector 0, then
    // MFM-encode the whole track the way trackdisk would before a write.
    let mut track = vec![0u8; 11 * 512];
    for (i, b) in track[..512].iter_mut().enumerate() {
        *b = (i & 0xFF) as u8;
    }
    let mfm_bytes = encode_mfm_track(&track, 0, 11);
    let words: Vec<u16> = mfm_bytes
        .chunks_exact(2)
        .map(|c| (u16::from(c[0]) << 8) | u16::from(c[1]))
        .collect();
    assert!(words.len() <= 0x3FFF, "track must fit the 14-bit DSKLEN");

    // Lay the MFM track in chip RAM at $1_0000 (clear of the reset SSP).
    let base = 0x0001_0000u32;
    for (i, &w) in words.iter().enumerate() {
        amiga.poke_word(base + (i as u32) * 2, w);
    }

    // Point DSKPT at the buffer and arm a WRITE DMA of the whole track
    // (DSKLEN double-write with DMAEN | WRITE | length).
    amiga.poke_word(0x00DF_F020, (base >> 16) as u16); // DSKPTH
    amiga.poke_word(0x00DF_F022, (base & 0xFFFF) as u16); // DSKPTL
    let dsklen = 0x8000 | 0x4000 | (words.len() as u16);
    amiga.poke_word(0x00DF_F024, dsklen);
    amiga.poke_word(0x00DF_F024, dsklen);
    assert!(
        amiga.paula().disk_dma_write_active(),
        "write DMA should be armed"
    );

    // Tick until the transfer drains (56 CCK/word × 2 ticks/CCK × ~6k
    // words → well under the cap).
    let mut ticks = 0u64;
    while amiga.paula().disk_dma_write_active() && ticks < 5_000_000 {
        amiga.tick();
        ticks += 1;
    }
    assert!(
        !amiga.paula().disk_dma_write_active(),
        "write DMA should have drained"
    );
    assert_eq!(
        amiga.intreq() & 0x0002,
        0x0002,
        "DSKBLK should fire on write completion"
    );

    // The decoded sector 0 must now be in the saved ADF.
    let saved = amiga.drive().save_adf().expect("disk present");
    let expected: Vec<u8> = (0..512).map(|i| (i & 0xFF) as u8).collect();
    assert_eq!(
        &saved[..512],
        &expected[..],
        "sector 0 must persist through the write-DMA path"
    );
}

/// Build a track-0 MFM word stream with a known sector-0 pattern.
fn encoded_track_words() -> Vec<u16> {
    let mut track = vec![0u8; 11 * 512];
    for (i, b) in track[..512].iter_mut().enumerate() {
        *b = (i & 0xFF) as u8;
    }
    encode_mfm_track(&track, 0, 11)
        .chunks_exact(2)
        .map(|c| (u16::from(c[0]) << 8) | u16::from(c[1]))
        .collect()
}

#[test]
fn write_protected_mount_drops_the_save() {
    let mut amiga = AmigaOcs::new(halt_rom());
    disable_ovl(&mut amiga);
    // Read-only (archive) mount.
    amiga.insert_adf_writable(Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid"), false);

    let words = encoded_track_words();
    let base = 0x0001_0000u32;
    for (i, &w) in words.iter().enumerate() {
        amiga.poke_word(base + (i as u32) * 2, w);
    }
    amiga.poke_word(0x00DF_F020, (base >> 16) as u16);
    amiga.poke_word(0x00DF_F022, (base & 0xFFFF) as u16);
    let dsklen = 0x8000 | 0x4000 | (words.len() as u16);
    amiga.poke_word(0x00DF_F024, dsklen);
    amiga.poke_word(0x00DF_F024, dsklen);

    let mut ticks = 0u64;
    while amiga.paula().disk_dma_write_active() && ticks < 5_000_000 {
        amiga.tick();
        ticks += 1;
    }
    assert!(
        !amiga.paula().disk_dma_write_active(),
        "DMA still drains to the head"
    );

    // The transfer completes (the head "wrote") but a write-protected
    // mount persists nothing — the ADF stays blank.
    let saved = amiga.drive().save_adf().expect("disk present");
    assert!(
        saved[..512].iter().all(|&b| b == 0),
        "a write-protected SAVE must not reach the image"
    );
}
