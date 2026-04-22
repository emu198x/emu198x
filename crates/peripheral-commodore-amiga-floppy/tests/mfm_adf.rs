//! Phase 1 characterisation — MFM track codec + ADF write-back.
//!
//! Covers task #168. Two layers:
//!   - `mfm::encode_mfm_track` + `mfm::decode_mfm_track` round-trip.
//!   - Drive-level write capture → `flush_write_capture` persists
//!     decoded sectors back to the ADF image.
//!
//! The archive's encode path is a direct port of vAmiga's
//! AmigaEncoder; the decode path matches the Paula DMA capture
//! format. Anything relying on MFM bit-for-bit output must be frozen
//! before Phase 2, so these tests lock the byte counts, sync-word
//! positions, and header-info layout that downstream code depends on.

use format_commodore_amiga_adf::{ADF_SIZE_DD, Adf};
use peripheral_commodore_amiga_floppy::{
    AmigaFloppyDrive,
    mfm::{MFM_TRACK_BYTES, SECTOR_MFM_BYTES, decode_mfm_track, encode_mfm_track},
};

/// Build one track of distinctive sector data (every byte unique mod
/// 256) so decode mismatches are easy to spot.
fn distinctive_track_bytes() -> Vec<u8> {
    (0..11 * 512).map(|i| ((i * 31 + 7) & 0xFF) as u8).collect()
}

#[test]
fn encoded_track_has_fixed_byte_count() {
    let track = distinctive_track_bytes();
    let mfm = encode_mfm_track(&track, 0, 11);
    assert_eq!(
        mfm.len(),
        MFM_TRACK_BYTES,
        "fixed 12,668-byte track layout for 11-sector DD"
    );
    assert_eq!(
        SECTOR_MFM_BYTES, 1088,
        "1088 MFM bytes per sector (header + data + interleave)"
    );
}

#[test]
fn every_sector_has_sync_word_pair_at_offset_four() {
    let track = vec![0u8; 11 * 512];
    let mfm = encode_mfm_track(&track, 0, 11);
    for s in 0..11 {
        let off = s * SECTOR_MFM_BYTES + 4;
        assert_eq!(
            &mfm[off..off + 4],
            &[0x44, 0x89, 0x44, 0x89],
            "missing $4489 $4489 sync pair at sector {s}"
        );
    }
}

#[test]
fn header_info_decodes_to_original_track_and_sector_numbers() {
    // Encode track 17 so non-zero values appear in the info field.
    let track = vec![0u8; 11 * 512];
    let mfm = encode_mfm_track(&track, 17, 11);

    // Info long = odd bytes at offset 8, even bytes at offset 12.
    let odd = u32::from_be_bytes([mfm[8], mfm[9], mfm[10], mfm[11]]);
    let even = u32::from_be_bytes([mfm[12], mfm[13], mfm[14], mfm[15]]);
    let info = ((odd & 0x5555_5555) << 1) | (even & 0x5555_5555);
    let b = info.to_be_bytes();

    assert_eq!(b[0], 0xFF, "format byte = $FF");
    assert_eq!(b[1], 17, "track number preserved");
    assert_eq!(b[2], 0, "sector 0 in the first slot");
    assert_eq!(b[3], 11, "sectors-until-gap countdown starts at 11");
}

#[test]
fn encode_then_decode_round_trips_all_11_sectors() {
    let track = distinctive_track_bytes();
    let mfm = encode_mfm_track(&track, 0, 11);

    // Repack as u16 words, big-endian — this is the format Paula DMA
    // captures into chip RAM and hands back to the decoder.
    let words: Vec<u16> = mfm
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    let decoded = decode_mfm_track(&words);

    assert_eq!(decoded.len(), 11, "all 11 sectors should decode");
    for s in &decoded {
        assert_eq!(s.track, 0, "track preserved through round-trip");
        let expected = &track[s.sector as usize * 512..(s.sector as usize + 1) * 512];
        assert_eq!(&s.data[..], expected, "sector {} data mismatch", s.sector);
    }
}

#[test]
fn decode_rejects_sector_with_bad_data_checksum() {
    let track = distinctive_track_bytes();
    let mfm = encode_mfm_track(&track, 0, 11);
    let mut words: Vec<u16> = mfm
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();

    // Corrupt one data byte in sector 0 (the data region starts at
    // offset 64 within each 1088-byte sector, = word 32).
    words[32] ^= 0x0100;

    let decoded = decode_mfm_track(&words);
    assert!(
        !decoded.iter().any(|s| s.sector == 0),
        "sector 0 must be rejected on bad data checksum"
    );
    assert_eq!(decoded.len(), 10, "other 10 sectors still decode");
}

#[test]
fn write_capture_flushes_through_to_adf_save() {
    let mut drive = AmigaFloppyDrive::new();
    let adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid blank ADF");
    drive.insert_disk(adf);

    // Author new sector-0 content; leave sectors 1..=10 zero so we
    // can spot-check sector 0 round-trips end-to-end.
    let mut track_data = vec![0u8; 11 * 512];
    for (i, b) in track_data[..512].iter_mut().enumerate() {
        *b = ((i * 5 + 0x40) & 0xFF) as u8;
    }

    let mfm_bytes = encode_mfm_track(&track_data, 0, 11);
    let mfm_words: Vec<u16> = mfm_bytes
        .chunks_exact(2)
        .map(|c| (u16::from(c[0]) << 8) | u16::from(c[1]))
        .collect();
    for &word in &mfm_words {
        drive.note_write_mfm_word(word);
    }

    let written = drive.flush_write_capture();
    assert_eq!(written, 11, "all 11 sectors persisted");

    let saved = drive.save_adf().expect("disk present");
    let expected: Vec<u8> = (0..512).map(|i| ((i * 5 + 0x40) & 0xFF) as u8).collect();
    assert_eq!(
        &saved[..512],
        &expected[..],
        "sector 0 data survives the write-capture -> decode -> ADF path"
    );
}

#[test]
fn flush_write_capture_returns_zero_without_disk() {
    let mut drive = AmigaFloppyDrive::new();
    drive.note_write_mfm_word(0x4489);
    drive.note_write_mfm_word(0x4489);
    assert_eq!(
        drive.flush_write_capture(),
        0,
        "no disk -> nothing persisted"
    );
}

#[test]
fn save_adf_returns_none_without_disk() {
    let drive = AmigaFloppyDrive::new();
    assert!(drive.save_adf().is_none());
}
