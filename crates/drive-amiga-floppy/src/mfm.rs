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
/// Per sector: 2 gap words + 2 sync words + 2 info words + 8 label words
///   + 2 hdr_cksum words + 2 data_cksum words + 512 data words = 530 words
///   = 1,060 bytes per sector. 11 sectors = 11,660 bytes. Plus inter-sector
///   gaps. In practice the track is ~12,668 bytes. We'll compute exactly.
///
/// Actual breakdown per sector (in MFM words, i.e. 16-bit units):
///   - Gap: 2 words
///   - Sync: 2 words
///   - Header info (odd + even): 4 bytes → 2 MFM longs = 4 words
///   - Label (odd + even): 16 bytes → 4 MFM longs = 8 words (odd) + 8 words (even) = 16 words
///   - Header checksum: 1 MFM long = 2 words (odd) + 2 words (even) = 4 words
///   - Data checksum: similarly 4 words
///   - Data (odd + even): 512 bytes → 256 words (odd) + 256 words (even) = 512 words
///   Total: 2 + 2 + 4 + 16 + 4 + 4 + 512 = 544 words = 1,088 bytes per sector.
///   11 sectors = 11,968 bytes. Real tracks have additional gap filler.
pub const MFM_TRACK_BYTES: usize = 13_630;

/// Encode a full track of sector data into Amiga raw MFM format.
///
/// `track_sectors` must be exactly `sectors_per_track * 512` bytes.
/// `track_num` is `cyl * 2 + head`.
pub fn encode_mfm_track(
    track_sectors: &[u8],
    track_num: u8,
    sectors_per_track: u32,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(MFM_TRACK_BYTES);

    for sector in 0..sectors_per_track {
        let sector_data = &track_sectors[sector as usize * 512..(sector as usize + 1) * 512];
        encode_sector(&mut buf, track_num, sector as u8, sectors_per_track as u8, sector_data);
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
    track: u8,
    sector: u8,
    sectors_per_track: u8,
    data: &[u8],
) {
    // 1. Gap: 2 words $AAAA
    buf.extend_from_slice(&[0xAA, 0xAA, 0xAA, 0xAA]);

    // 2. Sync: 2 words $4489
    buf.extend_from_slice(&[0x44, 0x89, 0x44, 0x89]);

    // 3. Header info: 4 bytes = [format, track, sector, sectors_to_gap]
    // Format byte $FF means standard AmigaDOS.
    // sectors_to_gap = number of sectors remaining before the gap
    // (decrements from sectors_per_track-1 to 0).
    let sectors_to_gap = sectors_per_track - sector - 1;
    let info_bytes = [0xFF, track, sector, sectors_to_gap];

    // The header info is encoded as one longword with odd/even split + MFM.
    let info_long = u32::from_be_bytes(info_bytes);
    let info_odd = mfm_encode_long(odd_bits(info_long));
    let info_even = mfm_encode_long(even_bits(info_long));
    buf.extend_from_slice(&info_odd.to_be_bytes());
    buf.extend_from_slice(&info_even.to_be_bytes());

    // 4. Sector label: 16 zero bytes (4 longs), odd/even split + MFM
    let label_zeros = [0u32; 4];
    let mut label_mfm_odd = [0u32; 4];
    let mut label_mfm_even = [0u32; 4];
    for i in 0..4 {
        label_mfm_odd[i] = mfm_encode_long(odd_bits(label_zeros[i]));
        label_mfm_even[i] = mfm_encode_long(even_bits(label_zeros[i]));
    }
    for &l in &label_mfm_odd { buf.extend_from_slice(&l.to_be_bytes()); }
    for &l in &label_mfm_even { buf.extend_from_slice(&l.to_be_bytes()); }

    // 5. Header checksum: XOR of all MFM header longs (info + label)
    let mut hdr_cksum: u32 = 0;
    hdr_cksum ^= info_odd;
    hdr_cksum ^= info_even;
    for i in 0..4 {
        hdr_cksum ^= label_mfm_odd[i];
        hdr_cksum ^= label_mfm_even[i];
    }
    // The checksum itself is stored odd/even split + MFM
    let hdr_cksum_odd = mfm_encode_long(odd_bits(hdr_cksum));
    let hdr_cksum_even = mfm_encode_long(even_bits(hdr_cksum));
    buf.extend_from_slice(&hdr_cksum_odd.to_be_bytes());
    buf.extend_from_slice(&hdr_cksum_even.to_be_bytes());

    // 6-7. Data: 512 bytes as 128 longs, odd/even split + MFM
    let mut data_longs = [0u32; 128];
    for i in 0..128 {
        let offset = i * 4;
        data_longs[i] = u32::from_be_bytes([
            data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
        ]);
    }

    // Data checksum: XOR of all MFM data longs (computed from both halves)
    let mut data_cksum: u32 = 0;
    let mut data_mfm_odd = [0u32; 128];
    let mut data_mfm_even = [0u32; 128];
    for i in 0..128 {
        data_mfm_odd[i] = mfm_encode_long(odd_bits(data_longs[i]));
        data_mfm_even[i] = mfm_encode_long(even_bits(data_longs[i]));
        data_cksum ^= data_mfm_odd[i];
        data_cksum ^= data_mfm_even[i];
    }

    // Data checksum (odd/even + MFM)
    let data_cksum_odd = mfm_encode_long(odd_bits(data_cksum));
    let data_cksum_even = mfm_encode_long(even_bits(data_cksum));
    buf.extend_from_slice(&data_cksum_odd.to_be_bytes());
    buf.extend_from_slice(&data_cksum_even.to_be_bytes());

    // Data odd words first, then even words
    for &l in &data_mfm_odd { buf.extend_from_slice(&l.to_be_bytes()); }
    for &l in &data_mfm_even { buf.extend_from_slice(&l.to_be_bytes()); }
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

/// MFM-encode a 16-bit data value into a 32-bit MFM longword.
/// Each data bit is preceded by a clock bit. Clock is 1 only when
/// both the preceding data bit AND current data bit are 0.
fn mfm_encode_long(data: u32) -> u32 {
    let data = data & 0xFFFF; // only low 16 bits are data
    let mut mfm = 0u32;
    for i in (0..16).rev() {
        let data_bit = (data >> i) & 1;
        let bit_pos = (15 - i) * 2; // MSB-first positioning
        // Clock bit: set if both previous data and current data are 0
        let prev_data = if i < 15 { (data >> (i + 1)) & 1 } else {
            // For the very first bit, use 0 as previous (conservative)
            0
        };
        let clock = if prev_data == 0 && data_bit == 0 { 1 } else { 0 };
        mfm |= clock << (31 - bit_pos);
        mfm |= data_bit << (30 - bit_pos);
    }
    mfm
}

/// Decode a 32-bit MFM longword back to a 16-bit data value.
/// Extracts data bits (odd positions in the MFM stream).
#[cfg(test)]
fn mfm_decode_long(mfm: u32) -> u32 {
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
}
