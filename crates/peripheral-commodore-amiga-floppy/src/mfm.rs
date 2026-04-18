//! MFM track encoding for Amiga raw disk format.
//!
//! Each track contains 11 sectors in the Amiga-specific MFM format. The
//! encoding uses an odd/even bit-split: for each longword, odd-position
//! bits are transmitted first, then even-position bits. Each half is
//! MFM-encoded separately.
//!
//! Sector layout (per HRM Appendix C):
//! 1. Gap: 2 words $AAAA (filler between sync marks)
//! 2. Sync: 2 words $4489 (MFM-encoded $A1 with missing clock)
//! 3. Header info: format=$FF, track number, sector number, sectors-to-gap
//! 4. Sector label: 16 zero bytes
//! 5. Header checksum: XOR of MFM header + label longs
//! 6. Data checksum: XOR of MFM data longs
//! 7. Data: 512 bytes (odd/even split, MFM-encoded)

/// Size of one MFM-encoded track in bytes.
/// 11 sectors x (2+2+2+2+8+8+4+4+512+512 = 1,056 raw longs) ≈ various
/// estimates. The standard raw track size for an Amiga DD disk is 12,668
/// bytes (or 6,334 words / 3,167 longs). We use a fixed buffer to hold
/// the encoded track.
///
/// Per sector: 2 gap words + 2 sync words + 2 info words + 8 label words +
/// 2 hdr_cksum words + 2 data_cksum words + 512 data words = 530 words =
/// 1,060 bytes per sector. 11 sectors = 11,660 bytes. Plus inter-sector gaps.
/// In practice the track is ~12,668 bytes. We'll compute exactly.
///
/// Actual breakdown per sector (in MFM words, i.e. 16-bit units):
///   - Gap: 2 words
///   - Sync: 2 words
///   - Header info (odd + even): 4 bytes → 2 MFM longs = 4 words
///   - Label (odd + even): 16 bytes → 4 MFM longs = 8 words (odd) + 8 words (even) = 16 words
///   - Header checksum: 1 MFM long = 2 words (odd) + 2 words (even) = 4 words
///   - Data checksum: similarly 4 words
///   - Data (odd + even): 512 bytes → 256 words (odd) + 256 words (even) = 512 words
///   - Total: 2 + 2 + 4 + 16 + 4 + 4 + 512 = 544 words = 1,088 bytes per sector.
///     11 sectors = 11,968 bytes. Real tracks have additional gap filler.
pub const MFM_TRACK_BYTES: usize = 13_630;

/// Encode a full track of sector data into Amiga raw MFM format.
///
/// `track_sectors` must be exactly `sectors_per_track * 512` bytes.
/// `track_num` is `cyl * 2 + head`.
pub fn encode_mfm_track(track_sectors: &[u8], track_num: u8, sectors_per_track: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(MFM_TRACK_BYTES);

    // Carries the last data bit across long boundaries so MFM clock
    // bits at the start of each long are computed correctly.
    let mut last_bit: u32 = 0;

    for sector in 0..sectors_per_track {
        let sector_data = &track_sectors[sector as usize * 512..(sector as usize + 1) * 512];
        encode_sector(
            &mut buf,
            &mut last_bit,
            track_num,
            sector as u8,
            sectors_per_track as u8,
            sector_data,
        );
    }

    // Pad remaining space with MFM gap bytes ($AA = clock bits only)
    while buf.len() < MFM_TRACK_BYTES {
        buf.push(0xAA);
    }
    buf.truncate(MFM_TRACK_BYTES);

    buf
}

fn encode_sector(
    buf: &mut Vec<u8>,
    last_bit: &mut u32,
    track: u8,
    sector: u8,
    sectors_per_track: u8,
    data: &[u8],
) {
    // Helper: encode one long using the running `last_bit` state, emit
    // its bytes, and update `last_bit` to this long's LSB (the last
    // data bit transmitted, at bit position 0 of the MFM long).
    fn emit_encoded_long(buf: &mut Vec<u8>, last_bit: &mut u32, data: u32) {
        let mfm = mfm_encode_long_with_prev(data, *last_bit);
        buf.extend_from_slice(&mfm.to_be_bytes());
        *last_bit = mfm & 1;
    }
    // Helper: emit raw bytes (gap/sync) that contain fixed MFM patterns;
    // update `last_bit` to the LSB of the last byte.
    fn emit_raw(buf: &mut Vec<u8>, last_bit: &mut u32, bytes: &[u8]) {
        buf.extend_from_slice(bytes);
        if let Some(&last) = bytes.last() {
            *last_bit = u32::from(last) & 1;
        }
    }

    // 1. Gap: 2 words $AAAA — 1010 pattern, last bit = 0.
    emit_raw(buf, last_bit, &[0xAA, 0xAA, 0xAA, 0xAA]);

    // 2. Sync: 2 words $4489 — the Amiga's magic constant. Its last
    //    bit (LSB of $89 = 1) becomes the "previous data bit" for the
    //    header that follows.
    emit_raw(buf, last_bit, &[0x44, 0x89, 0x44, 0x89]);

    // 3. Header info: 4 bytes = [format, track, sector, sectors_to_gap]
    let sectors_to_gap = sectors_per_track - sector - 1;
    let info_bytes = [0xFF, track, sector, sectors_to_gap];
    let info_long = u32::from_be_bytes(info_bytes);
    // Accumulate the full pre-emit values so the checksum stays
    // stable regardless of the clock-bit carry.
    let info_odd_mfm = mfm_encode_long_with_prev(odd_bits(info_long), *last_bit);
    emit_encoded_long(buf, last_bit, odd_bits(info_long));
    let info_even_mfm = mfm_encode_long_with_prev(even_bits(info_long), *last_bit);
    emit_encoded_long(buf, last_bit, even_bits(info_long));
    let _ = (info_odd_mfm, info_even_mfm);

    // 4. Sector label: 16 zero bytes (4 longs), odd/even split + MFM
    let label_zeros = [0u32; 4];
    let mut label_mfm_odd = [0u32; 4];
    let mut label_mfm_even = [0u32; 4];
    for i in 0..4 {
        label_mfm_odd[i] = mfm_encode_long_with_prev(odd_bits(label_zeros[i]), *last_bit);
        emit_encoded_long(buf, last_bit, odd_bits(label_zeros[i]));
    }
    for i in 0..4 {
        label_mfm_even[i] = mfm_encode_long_with_prev(even_bits(label_zeros[i]), *last_bit);
        emit_encoded_long(buf, last_bit, even_bits(label_zeros[i]));
    }

    // 5. Header checksum: XOR of all MFM header longs (info + label).
    // Note: we must XOR the *actual emitted* MFM longs, since clock
    // bits are part of the stream Kickstart hashes. Recompute them
    // with the running last_bit state replayed — but simpler: re-run
    // the encode with a scratch state that mirrors what emit produced.
    // Since label_mfm_odd/even above were sampled before each emit,
    // they're accurate-for-checksum only if the encode didn't depend
    // on last_bit. To keep the checksum aligned with the stream,
    // Kickstart uses: hdr_cksum = XOR of stream longs AND $55555555,
    // which strips clock bits entirely. So it's safe to XOR the data
    // bits only — but the easiest way is to XOR the stream longs and
    // mask. Because the data bits are identical regardless of clock,
    // the mask removes the carry-dependent part.
    let mut hdr_cksum: u32 = 0;
    hdr_cksum ^= mfm_encode_long_with_prev(odd_bits(info_long), 0);
    hdr_cksum ^= mfm_encode_long_with_prev(even_bits(info_long), 0);
    for i in 0..4 {
        hdr_cksum ^= label_mfm_odd[i];
        hdr_cksum ^= label_mfm_even[i];
    }
    hdr_cksum &= 0x5555_5555;
    emit_encoded_long(buf, last_bit, odd_bits(hdr_cksum));
    emit_encoded_long(buf, last_bit, even_bits(hdr_cksum));

    // 6-7. Data: 512 bytes as 128 longs, odd/even split + MFM
    let mut data_longs = [0u32; 128];
    for (i, long) in data_longs.iter_mut().enumerate() {
        let offset = i * 4;
        *long = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
    }

    // First compute the data checksum by encoding each MFM long with a
    // fixed prev-bit of 0, then mask clock bits. The mask makes the
    // checksum independent of the running last_bit state, so the value
    // we write here matches what Kickstart recomputes when reading.
    let mut data_cksum: u32 = 0;
    for i in 0..128 {
        data_cksum ^= mfm_encode_long_with_prev(odd_bits(data_longs[i]), 0);
        data_cksum ^= mfm_encode_long_with_prev(even_bits(data_longs[i]), 0);
    }
    data_cksum &= 0x5555_5555;

    // Data checksum (odd/even + MFM) — written to the stream with the
    // running last_bit state so clock bits link correctly.
    emit_encoded_long(buf, last_bit, odd_bits(data_cksum));
    emit_encoded_long(buf, last_bit, even_bits(data_cksum));

    // Data odd longs first, then even longs — each emitted through the
    // running last_bit state.
    for i in 0..128 {
        emit_encoded_long(buf, last_bit, odd_bits(data_longs[i]));
    }
    for i in 0..128 {
        emit_encoded_long(buf, last_bit, even_bits(data_longs[i]));
    }
}

/// Extract odd-position bits from a longword (bits 31,29,27,...,1).
/// The result is packed into the low 16 bits.
fn odd_bits(val: u32) -> u32 {
    let mut result = 0u32;
    for i in 0..16 {
        let bit = (val >> (1 + i * 2)) & 1;
        result |= bit << i;
    }
    result
}

/// Extract even-position bits from a longword (bits 30,28,26,...,0).
/// The result is packed into the low 16 bits.
fn even_bits(val: u32) -> u32 {
    let mut result = 0u32;
    for i in 0..16 {
        let bit = (val >> (i * 2)) & 1;
        result |= bit << i;
    }
    result
}

/// MFM-encode a 16-bit data value into a 32-bit MFM longword, with the
/// last data bit of the previous long supplied as `prev_last_bit`.
/// Clock is 1 only when both the preceding data bit AND current data
/// bit are 0 — and the "preceding bit" for the first data bit in this
/// long is the last data bit of the previous long, not a fixed 0.
fn mfm_encode_long_with_prev(data: u32, prev_last_bit: u32) -> u32 {
    let data = data & 0xFFFF;
    let mut mfm = 0u32;
    for i in (0..16).rev() {
        let data_bit = (data >> i) & 1;
        let bit_pos = (15 - i) * 2;
        let prev_data = if i < 15 {
            (data >> (i + 1)) & 1
        } else {
            prev_last_bit & 1
        };
        let clock = if prev_data == 0 && data_bit == 0 { 1 } else { 0 };
        mfm |= clock << (31 - bit_pos);
        mfm |= data_bit << (30 - bit_pos);
    }
    mfm
}

#[cfg(test)]
fn mfm_encode_long(data: u32) -> u32 {
    mfm_encode_long_with_prev(data, 0)
}

/// A decoded sector from an MFM track.
pub struct DecodedSector {
    pub track: u8,
    pub sector: u8,
    pub data: [u8; 512],
}

/// Decode an MFM word stream (as captured by DMA) into sector data.
///
/// Scans for $4489 sync word pairs, then decodes the Amiga sector
/// structure: header info, label, checksums, and 512-byte data block.
/// Returns only sectors with valid data checksums.
pub fn decode_mfm_track(mfm_words: &[u16]) -> Vec<DecodedSector> {
    let mut sectors = Vec::new();
    let mut i = 0;

    while i + 1 < mfm_words.len() {
        // Scan for sync pair: $4489 $4489
        if mfm_words[i] != 0x4489 {
            i += 1;
            continue;
        }
        // Skip consecutive sync words
        while i < mfm_words.len() && mfm_words[i] == 0x4489 {
            i += 1;
        }

        // After sync: need at least 2 (info) + 8 (label odd) + 8 (label even)
        //   + 2 (hdr cksum) + 2 (data cksum) + 256 (data odd) + 256 (data even)
        //   = 534 words
        if i + 534 > mfm_words.len() {
            break;
        }

        // Read MFM longs as pairs of u16 words (big-endian)
        let read_mfm_long = |pos: usize| -> u32 {
            (u32::from(mfm_words[pos]) << 16) | u32::from(mfm_words[pos + 1])
        };

        // Header info: 1 longword as odd + even halves (2 MFM longs = 4 words)
        let info_odd_mfm = read_mfm_long(i);
        let info_even_mfm = read_mfm_long(i + 2);
        let info_odd = mfm_decode_long(info_odd_mfm);
        let info_even = mfm_decode_long(info_even_mfm);
        let info_long = reconstruct_long(info_odd, info_even);
        let info_bytes = info_long.to_be_bytes();
        let _format = info_bytes[0];
        let track = info_bytes[1];
        let sector = info_bytes[2];
        i += 4;

        // Label: 4 longs odd + 4 longs even (16 words) — skip
        i += 16;

        // Header checksum: 1 longword odd/even (4 words) — skip
        i += 4;

        // Data checksum: stored as odd/even MFM-encoded longword (4 words)
        let stored_cksum_odd_mfm = read_mfm_long(i);
        let stored_cksum_even_mfm = read_mfm_long(i + 2);
        let stored_data_cksum = reconstruct_long(
            mfm_decode_long(stored_cksum_odd_mfm),
            mfm_decode_long(stored_cksum_even_mfm),
        );
        i += 4;

        // Data: 128 longs, odd first (256 words), then even (256 words)
        let mut computed_data_cksum: u32 = 0;
        let mut data_odd_mfm = [0u32; 128];
        let mut data_even_mfm = [0u32; 128];
        for (j, odd) in data_odd_mfm.iter_mut().enumerate() {
            *odd = read_mfm_long(i + j * 2);
            computed_data_cksum ^= *odd;
        }
        i += 256;
        for (j, even) in data_even_mfm.iter_mut().enumerate() {
            *even = read_mfm_long(i + j * 2);
            computed_data_cksum ^= *even;
        }
        i += 256;

        // Verify: XOR of all raw MFM data longs, masked with $55555555
        // to strip clock bits, should equal stored checksum.
        computed_data_cksum &= 0x5555_5555;
        if computed_data_cksum != stored_data_cksum {
            continue; // Bad checksum — skip sector
        }

        // Decode data longs
        let mut data = [0u8; 512];
        for j in 0..128 {
            let odd_val = mfm_decode_long(data_odd_mfm[j]);
            let even_val = mfm_decode_long(data_even_mfm[j]);
            let long_val = reconstruct_long(odd_val, even_val);
            let bytes = long_val.to_be_bytes();
            data[j * 4] = bytes[0];
            data[j * 4 + 1] = bytes[1];
            data[j * 4 + 2] = bytes[2];
            data[j * 4 + 3] = bytes[3];
        }

        sectors.push(DecodedSector {
            track,
            sector,
            data,
        });
    }

    sectors
}

/// Reconstruct a 32-bit value from separate odd and even 16-bit halves.
fn reconstruct_long(odd: u32, even: u32) -> u32 {
    let mut result = 0u32;
    for i in 0..16 {
        result |= ((even >> i) & 1) << (i * 2);
        result |= ((odd >> i) & 1) << (i * 2 + 1);
    }
    result
}

/// Decode a 32-bit MFM longword back to a 16-bit data value.
/// Extracts data bits (odd positions in the MFM stream).
pub fn mfm_decode_long(mfm: u32) -> u32 {
    let mut data = 0u32;
    for i in 0..16 {
        let bit = (mfm >> (30 - i * 2)) & 1;
        data |= bit << (15 - i);
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_even_bits_reconstruct() {
        let val: u32 = 0xDEAD_BEEF;
        let odd = odd_bits(val);
        let even = even_bits(val);
        // Reconstruct: interleave odd (high) and even (low) bits
        let mut reconstructed = 0u32;
        for i in 0..16 {
            reconstructed |= ((even >> i) & 1) << (i * 2);
            reconstructed |= ((odd >> i) & 1) << (i * 2 + 1);
        }
        assert_eq!(reconstructed, val);
    }

    #[test]
    fn mfm_encode_decode_round_trip() {
        for data in [0x0000u32, 0xFFFF, 0xAAAA, 0x5555, 0xDEAD, 0x1234] {
            let mfm = mfm_encode_long(data);
            let decoded = mfm_decode_long(mfm);
            assert_eq!(decoded, data, "round-trip failed for ${data:04X}");
        }
    }

    #[test]
    fn mfm_zero_gives_clocks() {
        // All-zero data should produce alternating clock bits: $AAAA_AAAA
        let mfm = mfm_encode_long(0x0000);
        assert_eq!(mfm, 0xAAAA_AAAA);
    }

    #[test]
    fn mfm_ones_gives_no_clocks() {
        // All-one data means clock bits are all 0: $5555_5555
        let mfm = mfm_encode_long(0xFFFF);
        assert_eq!(mfm, 0x5555_5555);
    }

    #[test]
    fn encode_track_has_sync_marks() {
        let track_data = vec![0u8; 11 * 512]; // 11 sectors of zeros
        let mfm = encode_mfm_track(&track_data, 0, 11);

        // Each sector should have two $4489 sync words
        let sync_pattern = [0x44u8, 0x89, 0x44, 0x89];
        let mut sync_count = 0;
        for window in mfm.windows(4) {
            if window == sync_pattern {
                sync_count += 1;
            }
        }
        assert_eq!(sync_count, 11, "expected 11 sync marks (one per sector)");
    }

    #[test]
    fn encode_track_length() {
        let track_data = vec![0u8; 11 * 512];
        let mfm = encode_mfm_track(&track_data, 0, 11);
        assert_eq!(mfm.len(), MFM_TRACK_BYTES);
    }

    #[test]
    fn decode_mfm_track_round_trip() {
        // Encode a track with known data, then decode and verify
        let mut track_data = vec![0u8; 11 * 512];
        for (i, byte) in track_data.iter_mut().enumerate() {
            *byte = (i & 0xFF) as u8;
        }
        let track_num = 5u8;
        let encoded = encode_mfm_track(&track_data, track_num, 11);

        // Convert byte stream to u16 word stream (as DMA would capture)
        let mfm_words: Vec<u16> = encoded
            .chunks_exact(2)
            .map(|c| (u16::from(c[0]) << 8) | u16::from(c[1]))
            .collect();

        let decoded = decode_mfm_track(&mfm_words);
        assert_eq!(decoded.len(), 11, "should decode all 11 sectors");

        for ds in &decoded {
            assert_eq!(ds.track, track_num, "track number should match");
            let sector = ds.sector as usize;
            let expected = &track_data[sector * 512..(sector + 1) * 512];
            assert_eq!(&ds.data[..], expected, "sector {} data mismatch", sector);
        }
    }

    #[test]
    fn decode_empty_stream() {
        let decoded = decode_mfm_track(&[]);
        assert!(decoded.is_empty());
    }

    #[test]
    fn decode_corrupted_sync_skips_bad_sectors() {
        // Encode a valid track
        let track_data = vec![0u8; 11 * 512];
        let encoded = encode_mfm_track(&track_data, 0, 11);
        let mut mfm_words: Vec<u16> = encoded
            .chunks_exact(2)
            .map(|c| (u16::from(c[0]) << 8) | u16::from(c[1]))
            .collect();

        // Corrupt data in the first sector (after the first sync pair)
        // Find first sync and corrupt data words after it
        let mut found = 0;
        for i in 0..mfm_words.len() - 1 {
            if mfm_words[i] == 0x4489 && mfm_words[i + 1] != 0x4489 {
                // Corrupt some data words to invalidate checksum
                if found == 0 {
                    for j in i + 30..i + 40 {
                        if j < mfm_words.len() {
                            mfm_words[j] ^= 0xFFFF;
                        }
                    }
                }
                found += 1;
            }
        }

        let decoded = decode_mfm_track(&mfm_words);
        // First sector should be skipped due to bad checksum
        assert!(
            decoded.len() < 11,
            "corrupted sector should be skipped (got {} sectors)",
            decoded.len()
        );
        assert!(
            decoded.len() >= 9,
            "remaining sectors should still decode (got {})",
            decoded.len()
        );
    }

    #[test]
    fn decode_sector_numbers_match_encoded() {
        let track_data = vec![0u8; 11 * 512];
        let encoded = encode_mfm_track(&track_data, 10, 11);
        let mfm_words: Vec<u16> = encoded
            .chunks_exact(2)
            .map(|c| (u16::from(c[0]) << 8) | u16::from(c[1]))
            .collect();

        let decoded = decode_mfm_track(&mfm_words);
        let mut sector_nums: Vec<u8> = decoded.iter().map(|s| s.sector).collect();
        sector_nums.sort();
        assert_eq!(sector_nums, (0..11).collect::<Vec<u8>>());
    }
}
