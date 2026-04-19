//! Round-trip test: encode the bootblock as an Amiga MFM track with
//! our own encoder, then decode it with our own decoder (and also walk
//! it as Paula DMA would). See where bytes diverge.

use emu198x_shell::{MediaKind, read_media_asset};
use std::path::Path;

fn main() {
    let disk_path = "/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip";
    let loaded = read_media_asset(Path::new(disk_path), MediaKind::Disk).unwrap();
    let adf = format_commodore_amiga_adf::Adf::from_bytes(loaded.bytes).unwrap();

    // Take track 0 (cyl 0, head 0) = first 11 × 512 bytes.
    let track_data = &adf.data()[..11 * 512];

    let mfm_bytes = peripheral_commodore_amiga_floppy::mfm::encode_mfm_track(track_data, 0, 11);
    println!("MFM track encoded: {} bytes", mfm_bytes.len());

    // Convert bytes to u16 words (big-endian pairs) for our decoder.
    let mut words: Vec<u16> = Vec::with_capacity(mfm_bytes.len() / 2);
    for chunk in mfm_bytes.chunks_exact(2) {
        words.push(u16::from_be_bytes([chunk[0], chunk[1]]));
    }

    let decoded = peripheral_commodore_amiga_floppy::mfm::decode_mfm_track(&words);
    println!("Decoded sectors: {}", decoded.len());

    for s in &decoded {
        let expected = &track_data[s.sector as usize * 512..(s.sector as usize + 1) * 512];
        let mismatches: Vec<usize> = (0..512).filter(|&i| s.data[i] != expected[i]).collect();
        println!(
            "  sector {} track {}: {} mismatches (of 512 bytes)",
            s.sector,
            s.track,
            mismatches.len()
        );
        for &m in mismatches.iter().take(5) {
            println!("    offset {m}: expected ${:02X} got ${:02X}", expected[m], s.data[m]);
        }
    }

    // Now simulate Paula DMA: scan MFM byte stream as words, suppress
    // sync words ($4489), write rest to a buffer, and decode from there.
    let mut paula_buf: Vec<u16> = Vec::new();
    let mut wordsync_waiting = true;
    for w in &words {
        let is_sync = *w == 0x4489;
        if wordsync_waiting {
            if is_sync { wordsync_waiting = false; }
            continue;
        }
        if is_sync { continue; }
        paula_buf.push(*w);
    }
    println!("\nPaula-style filtered stream: {} words", paula_buf.len());
    // Decode from paula_buf (simulating trackdisk reading from DMA buffer).
    // Prepend two sync words to allow the decoder's sync-scan to start.
    let mut for_decode: Vec<u16> = vec![0x4489, 0x4489];
    for_decode.extend(&paula_buf);
    let decoded2 = peripheral_commodore_amiga_floppy::mfm::decode_mfm_track(&for_decode);
    println!("Paula-filtered decoded sectors: {}", decoded2.len());
    for s in decoded2.iter().take(3) {
        let expected = &track_data[s.sector as usize * 512..(s.sector as usize + 1) * 512];
        let mismatches: Vec<usize> = (0..512).filter(|&i| s.data[i] != expected[i]).collect();
        println!(
            "  sector {} track {}: {} mismatches",
            s.sector,
            s.track,
            mismatches.len()
        );
        for &m in mismatches.iter().take(8) {
            println!("    offset {m}: expected ${:02X} got ${:02X}", expected[m], s.data[m]);
        }
    }
}
