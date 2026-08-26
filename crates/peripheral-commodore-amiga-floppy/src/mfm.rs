//! MFM track encoding for Amiga raw disk format.
//!
//! Direct port of vAmiga's `AmigaEncoder::encodeSector` +
//! `MFM::addClockBits` / `MFM::encodeOddEven`, cross-referenced with
//! WinUAE `disk.cpp:mfmcode` / `disk.cpp:2188+`. The algorithm is:
//!
//! 1. Reserve 1,088 bytes per sector pre-filled with `$AA`.
//! 2. Write the pre-SYNC gap and two `$4489` sync words (bytes 0..7).
//! 3. Spread the header info (4 bytes) into an 8-byte odd/even pair at
//!    offset 8, and leave the 32-byte unused area at 16..47 filled with
//!    `$AA`.
//! 4. Spread the 512 sector data bytes into a 1024-byte odd/even pair
//!    at offset 64.
//! 5. Compute the block checksum (byte-wise XOR of bytes 8..47, taken
//!    4 at a time) and spread it at offset 48.
//! 6. Compute the data checksum (byte-wise XOR of bytes 64..1087, taken
//!    4 at a time) and spread it at offset 56.
//! 7. Run `addClockBits` byte-by-byte from offset 8 to end of sector so
//!    every `$aa`/`$55`/mixed byte ends up with its clock bits set
//!    according to the MFM rule.
//!
//! The odd/even interleave places one data byte into two output bytes:
//! the odd-indexed bits land at positions 0,2,4,6 of the first output
//! byte; the even-indexed bits land at positions 0,2,4,6 of the second
//! output byte. Clock bits occupy positions 1,3,5,7 and are filled in
//! afterwards.

/// Size of one MFM-encoded track in bytes. 11 sectors × 1088 bytes =
/// 11,968 bytes; the remaining padding up to 12,668 bytes is the
/// track-level gap the drive rotates through between passes.
pub const MFM_TRACK_BYTES: usize = 12_668;

/// MFM bytes per sector (header + data).
pub const SECTOR_MFM_BYTES: usize = 1088;

/// Encode a full track of sector data into Amiga raw MFM format.
///
/// `track_sectors` must be exactly `sectors_per_track * 512` bytes.
/// `track_num` is `cyl * 2 + head`.
///
/// After each sector is independently encoded (see
/// `encode_sector_into`), the leading clock bit of each sector (the
/// MSB of its first $AA gap byte) is re-computed against the last
/// data bit of the previous byte on the track. Each sector is
/// encoded as if starting from scratch — fine in isolation, but
/// when sectors sit next to each other on the track the MFM
/// invariant "clock bit = 1 iff both adjacent data bits = 0"
/// breaks at the sector boundary when the previous sector's last
/// data bit happens to be 1. Real trackdisk readers rely on that
/// invariant; KS 1.3 trackdisk will decode sector 0 correctly but
/// mangle later sectors if the boundary clock bits are wrong. This
/// step matches vAmiga's `rectifyClockBit` fix-up pass.
pub fn encode_mfm_track(track_sectors: &[u8], track_num: u8, sectors_per_track: u32) -> Vec<u8> {
    let mut buf = vec![0xAAu8; MFM_TRACK_BYTES];

    for s in 0..sectors_per_track {
        let sector_data = &track_sectors[s as usize * 512..(s as usize + 1) * 512];
        let offset = s as usize * SECTOR_MFM_BYTES;
        encode_sector_into(
            &mut buf[offset..offset + SECTOR_MFM_BYTES],
            track_num,
            s as u8,
            sectors_per_track as u8,
            sector_data,
        );
    }

    // Rectify clock bits at sector boundaries. Each boundary is the
    // MSB of byte `n * SECTOR_MFM_BYTES` for n in 0..=sectors_per_track.
    // The MSB is a clock bit; it should be 0 when either neighbouring
    // data bit is 1 (the data bit *in* this byte — bit 6 — and the
    // data bit *before* — bit 0 of the previous byte on the track,
    // wrapping at the track end like a real disk).
    let sector_count = sectors_per_track as usize;
    let track_len = buf.len();
    for n in 0..=sector_count {
        let byte_idx = (n * SECTOR_MFM_BYTES) % track_len;
        let prev_idx = (byte_idx + track_len - 1) % track_len;
        let next_data_in_same_byte = (buf[byte_idx] >> 6) & 1;
        let prev_data = buf[prev_idx] & 1;
        let clock_bit = if prev_data == 0 && next_data_in_same_byte == 0 {
            1
        } else {
            0
        };
        buf[byte_idx] = (buf[byte_idx] & 0x7F) | (clock_bit << 7);
    }

    buf
}

fn encode_sector_into(
    sector: &mut [u8],
    track: u8,
    sector_num: u8,
    sectors_per_track: u8,
    data: &[u8],
) {
    debug_assert_eq!(sector.len(), SECTOR_MFM_BYTES);
    debug_assert_eq!(data.len(), 512);

    // 1. Pre-sync gap (offsets 0..3) and sync words (offsets 4..7).
    sector[0] = 0xAA;
    sector[1] = 0xAA;
    sector[2] = 0xAA;
    sector[3] = 0xAA;
    sector[4] = 0x44;
    sector[5] = 0x89;
    sector[6] = 0x44;
    sector[7] = 0x89;

    // 2. Track + sector info (raw 4 bytes, odd/even encoded into 8).
    // Sectors-until-gap counts this sector and the ones after it up to
    // the track gap: range is 11 (for sector 0) down to 1 (for sector
    // sectors_per_track - 1). vAmiga and WinUAE both use this
    // convention.
    let info = [0xFFu8, track, sector_num, sectors_per_track - sector_num];
    encode_odd_even(&mut sector[8..16], &info);

    // 3. Unused area: 32 bytes of $AA at offset 16..47.
    for b in &mut sector[16..48] {
        *b = 0xAA;
    }

    // 4. Sector data: 512 raw bytes, odd/even encoded into 1024 bytes
    // at offset 64..1087.
    encode_odd_even(&mut sector[64..1088], data);

    // 5. Block (header) checksum: byte-wise XOR of bytes 8..47 taken
    // 4 at a time, then odd/even encoded into bytes 48..55.
    let mut bcheck = [0u8; 4];
    let mut i = 8;
    while i < 48 {
        bcheck[0] ^= sector[i];
        bcheck[1] ^= sector[i + 1];
        bcheck[2] ^= sector[i + 2];
        bcheck[3] ^= sector[i + 3];
        i += 4;
    }
    encode_odd_even(&mut sector[48..56], &bcheck);

    // 6. Data checksum: byte-wise XOR of bytes 64..1087 taken 4 at a
    // time, then odd/even encoded into bytes 56..63.
    let mut dcheck = [0u8; 4];
    let mut i = 64;
    while i < SECTOR_MFM_BYTES {
        dcheck[0] ^= sector[i];
        dcheck[1] ^= sector[i + 1];
        dcheck[2] ^= sector[i + 2];
        dcheck[3] ^= sector[i + 3];
        i += 4;
    }
    encode_odd_even(&mut sector[56..64], &dcheck);

    // 7. Add clock bits to the MFM region (byte 8 onwards). The first
    // byte's previous context comes from sector[7] (the last sync byte,
    // $89, whose LSB is 1 — this forces the leading clock bit to 0).
    for i in 8..SECTOR_MFM_BYTES {
        sector[i] = add_clock_bits_byte(sector[i], sector[i - 1]);
    }
}

/// vAmiga's odd/even split. For each source byte, write the odd bits
/// (positions 1,3,5,7 of the source) into the low `count` bytes of dst
/// at even positions, and the even bits (positions 0,2,4,6) into the
/// high `count` bytes at even positions.
fn encode_odd_even(dst: &mut [u8], src: &[u8]) {
    let count = src.len();
    debug_assert_eq!(dst.len(), 2 * count);
    for i in 0..count {
        dst[i] = (src[i] >> 1) & 0x55;
        dst[i + count] = src[i] & 0x55;
    }
}

/// Add MFM clock bits to a byte that already carries data bits at
/// positions 0,2,4,6. `previous` is the byte immediately before this
/// one in the MFM stream — its LSB is the last data bit that was
/// transmitted and feeds into the clock computation for this byte's
/// leading clock bit (bit 7).
///
/// The MFM rule: a clock bit is 1 only when both adjacent data bits
/// are 0.
fn add_clock_bits_byte(value: u8, previous: u8) -> u8 {
    // Strip any stale clock bits, keeping only the data-bit positions.
    let value = value & 0x55;

    // `lShifted` carries each data bit up by one — these are the clock
    // positions whose *right* neighbour is a 1.
    // `rShifted` carries each data bit down by one, with the previous
    // byte's LSB tucked into bit 7 — these are the clock positions
    // whose *left* neighbour is a 1.
    let l_shifted = value << 1;
    let r_shifted = (value >> 1) | (previous << 7);
    let c_bits_inv = l_shifted | r_shifted;

    // Invert so we have 1 where both neighbours are 0 (i.e., where the
    // clock bit must be 1). The XOR with 0xAA restricts the result to
    // clock positions only.
    let c_bits = c_bits_inv ^ 0xAA;

    value | c_bits
}

/// A decoded sector from an MFM track.
pub struct DecodedSector {
    pub track: u8,
    pub sector: u8,
    pub data: [u8; 512],
}

/// Decode an MFM word stream (as captured by DMA) into sector data.
///
/// Scans for `$4489` sync pairs, then decodes the Amiga sector
/// structure: header info, label, checksums, and 512-byte data block.
/// Returns only sectors with valid data checksums.
pub fn decode_mfm_track(mfm_words: &[u16]) -> Vec<DecodedSector> {
    let mut sectors = Vec::new();
    let mut i = 0;

    while i + 1 < mfm_words.len() {
        if mfm_words[i] != 0x4489 {
            i += 1;
            continue;
        }
        while i < mfm_words.len() && mfm_words[i] == 0x4489 {
            i += 1;
        }

        // Need 2 info + 8 label + 2 hdr_cksum + 2 data_cksum + 256
        // data-odd + 256 data-even = 534 words.
        if i + 534 > mfm_words.len() {
            break;
        }

        let read_mfm_long = |pos: usize| -> u32 {
            (u32::from(mfm_words[pos]) << 16) | u32::from(mfm_words[pos + 1])
        };
        let decode_long =
            |odd: u32, even: u32| -> u32 { ((odd & 0x5555_5555) << 1) | (even & 0x5555_5555) };

        // Info: 2 MFM longs (odd + even), reconstructed into one 32-bit.
        let info_odd = read_mfm_long(i);
        let info_even = read_mfm_long(i + 2);
        let info = decode_long(info_odd, info_even);
        let info_bytes = info.to_be_bytes();
        let track = info_bytes[1];
        let sector_num = info_bytes[2];
        i += 4;

        // Label (16 bytes = 4 longs odd + 4 longs even) — skip.
        i += 16;

        // Header checksum — skip.
        i += 4;

        // Data checksum — read (odd + even) then reconstruct.
        let stored_dcheck_odd = read_mfm_long(i);
        let stored_dcheck_even = read_mfm_long(i + 2);
        let stored_dcheck = decode_long(stored_dcheck_odd, stored_dcheck_even);
        i += 4;

        // Data: 256 odd words + 256 even words = 512 words of data
        // (128 longs).
        let mut data_odd_longs = [0u32; 128];
        let mut data_even_longs = [0u32; 128];
        for (j, val) in data_odd_longs.iter_mut().enumerate() {
            *val = read_mfm_long(i + j * 2);
        }
        i += 256;
        for (j, val) in data_even_longs.iter_mut().enumerate() {
            *val = read_mfm_long(i + j * 2);
        }
        i += 256;

        // Verify the data checksum: XOR of all data MFM longs (both
        // halves), masked to data-bit positions only.
        let mut computed_dcheck: u32 = 0;
        for (odd, even) in data_odd_longs.iter().zip(data_even_longs.iter()) {
            computed_dcheck ^= odd;
            computed_dcheck ^= even;
        }
        computed_dcheck &= 0x5555_5555;
        if computed_dcheck != stored_dcheck {
            continue; // Skip bad sector.
        }

        // Decode data longs.
        let mut data = [0u8; 512];
        for j in 0..128 {
            let long = decode_long(data_odd_longs[j], data_even_longs[j]);
            let bytes = long.to_be_bytes();
            data[j * 4] = bytes[0];
            data[j * 4 + 1] = bytes[1];
            data[j * 4 + 2] = bytes[2];
            data[j * 4 + 3] = bytes[3];
        }

        sectors.push(DecodedSector {
            track,
            sector: sector_num,
            data,
        });
    }

    sectors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_odd_even_round_trip() {
        let src: Vec<u8> = (0..16).map(|i| i as u8 * 17).collect();
        let mut tmp = vec![0u8; 32];
        encode_odd_even(&mut tmp, &src);
        // Decode by recombining: byte i = (odd[i] << 1) | even[i].
        let mut decoded = vec![0u8; 16];
        for i in 0..16 {
            decoded[i] = ((tmp[i] & 0x55) << 1) | (tmp[i + 16] & 0x55);
        }
        assert_eq!(decoded, src);
    }

    #[test]
    fn clock_bits_for_all_zero_data_are_all_set() {
        // A byte with no data bits should produce $AA after clock
        // insertion (assuming previous byte's LSB is 0): clock bits at
        // positions 1,3,5,7 all set because both neighbours are 0.
        assert_eq!(add_clock_bits_byte(0x00, 0x00), 0xAA);
    }

    #[test]
    fn clock_bits_for_all_one_data_are_all_clear() {
        // A byte with data bits at every position should have no
        // clock bits (both neighbours are 1).
        assert_eq!(add_clock_bits_byte(0x55, 0xFF), 0x55);
    }

    #[test]
    fn encode_track_then_decode_round_trips() {
        // Build one track of well-distributed data.
        let mut track_data = vec![0u8; 11 * 512];
        for (i, b) in track_data.iter_mut().enumerate() {
            *b = ((i * 31 + 7) & 0xFF) as u8;
        }
        let mfm = encode_mfm_track(&track_data, 0, 11);
        assert_eq!(mfm.len(), MFM_TRACK_BYTES);

        let words: Vec<u16> = mfm
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_be_bytes(c))
            .collect();
        let sectors = decode_mfm_track(&words);
        assert_eq!(sectors.len(), 11);
        for s in &sectors {
            assert_eq!(s.track, 0);
            let expected = &track_data[s.sector as usize * 512..(s.sector as usize + 1) * 512];
            assert_eq!(&s.data[..], expected, "sector {} mismatch", s.sector);
        }
    }

    #[test]
    fn sync_words_are_preserved_in_the_stream() {
        let track_data = vec![0u8; 11 * 512];
        let mfm = encode_mfm_track(&track_data, 0, 11);
        // Each sector should contain exactly the bytes $44 $89 $44 $89
        // at offsets 4..7.
        for s in 0..11 {
            let off = s * SECTOR_MFM_BYTES + 4;
            assert_eq!(
                &mfm[off..off + 4],
                &[0x44, 0x89, 0x44, 0x89],
                "sync missing for sector {s}"
            );
        }
    }

    #[test]
    fn header_info_bytes_decode_to_expected_values() {
        let track_data = vec![0u8; 11 * 512];
        let mfm = encode_mfm_track(&track_data, 3, 11); // track 3
        // Info is at offset 8 (odd half) + 12 (even half) of sector 0.
        // Decode the info longword.
        let odd = u32::from_be_bytes([mfm[8], mfm[9], mfm[10], mfm[11]]);
        let even = u32::from_be_bytes([mfm[12], mfm[13], mfm[14], mfm[15]]);
        let info = ((odd & 0x5555_5555) << 1) | (even & 0x5555_5555);
        let bytes = info.to_be_bytes();
        assert_eq!(bytes[0], 0xFF, "format");
        assert_eq!(bytes[1], 3, "track");
        assert_eq!(bytes[2], 0, "sector");
        assert_eq!(bytes[3], 11, "sectors until gap");
    }
}
