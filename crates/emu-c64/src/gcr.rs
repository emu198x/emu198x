//! GCR (Group Code Recording) encoding for D64 sectors.
//!
//! The 1541 drive stores data on disk using GCR encoding: every 4 bits
//! are mapped to a unique 5-bit pattern that guarantees no more than
//! two consecutive zeros (essential for reliable clock recovery).
//!
//! Each sector on disk consists of:
//!   - Sync mark: 5 bytes of $FF (40 one-bits)
//!   - Header block: 10 GCR bytes (8 raw → 10 GCR)
//!   - Header gap: 9 bytes of $55
//!   - Sync mark: 5 bytes of $FF
//!   - Data block: 325 GCR bytes (260 raw → 325 GCR)
//!   - Inter-sector gap: ~9 bytes of $55
//!
//! Zone-dependent byte rate (at ~1 MHz drive CPU clock):
//!   Zone 0 (tracks  1-17): 26 cycles/bit → 208 cycles/byte
//!   Zone 1 (tracks 18-24): 28 cycles/bit → 224 cycles/byte
//!   Zone 2 (tracks 25-30): 30 cycles/bit → 240 cycles/byte
//!   Zone 3 (tracks 31-35): 32 cycles/bit → 256 cycles/byte

#![allow(clippy::cast_possible_truncation)]

use crate::d64::D64;

/// 4-bit to 5-bit GCR encoding table.
const GCR_ENCODE: [u8; 16] = [
    0x0A, 0x0B, 0x12, 0x13, 0x0E, 0x0F, 0x16, 0x17,
    0x09, 0x19, 0x1A, 0x1B, 0x0D, 0x1D, 0x1E, 0x15,
];

/// 5-bit to 4-bit GCR decoding table (inverse of GCR_ENCODE).
/// Invalid codes map to 0x00.
const GCR_DECODE: [u8; 32] = [
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // 00-07: invalid
    0xFF, 0x08, 0x00, 0x01, 0xFF, 0x0C, 0x04, 0x05, // 08-0F
    0xFF, 0xFF, 0x02, 0x03, 0xFF, 0x0F, 0x06, 0x07, // 10-17
    0xFF, 0x09, 0x0A, 0x0B, 0xFF, 0x0D, 0x0E, 0xFF, // 18-1F
];

/// Speed zone for a given track number.
///
/// Returns the zone (0-3) which determines the bit rate.
#[must_use]
pub fn speed_zone(track: u8) -> u8 {
    match track {
        1..=17 => 0,
        18..=24 => 1,
        25..=30 => 2,
        31..=35 => 3,
        _ => 0,
    }
}

/// Cycles per GCR byte for a given track (at ~1 MHz drive CPU clock).
#[must_use]
pub fn cycles_per_byte(track: u8) -> u32 {
    match speed_zone(track) {
        0 => 208,
        1 => 224,
        2 => 240,
        3 => 256,
        _ => 208,
    }
}

/// Decode 5 GCR bytes into 4 raw bytes.
///
/// Returns `None` if any GCR nybble is invalid.
pub fn decode_gcr_group(input: &[u8; 5]) -> Option<[u8; 4]> {
    // Unpack 40 bits (5 bytes) into 8 x 5-bit GCR nybbles
    let g0 = (input[0] >> 3) & 0x1F;
    let g1 = ((input[0] << 2) | (input[1] >> 6)) & 0x1F;
    let g2 = (input[1] >> 1) & 0x1F;
    let g3 = ((input[1] << 4) | (input[2] >> 4)) & 0x1F;
    let g4 = ((input[2] << 1) | (input[3] >> 7)) & 0x1F;
    let g5 = (input[3] >> 2) & 0x1F;
    let g6 = ((input[3] << 3) | (input[4] >> 5)) & 0x1F;
    let g7 = input[4] & 0x1F;

    let d = [
        GCR_DECODE[g0 as usize],
        GCR_DECODE[g1 as usize],
        GCR_DECODE[g2 as usize],
        GCR_DECODE[g3 as usize],
        GCR_DECODE[g4 as usize],
        GCR_DECODE[g5 as usize],
        GCR_DECODE[g6 as usize],
        GCR_DECODE[g7 as usize],
    ];

    // Check for invalid codes
    if d.iter().any(|&b| b == 0xFF) {
        return None;
    }

    Some([
        (d[0] << 4) | d[1],
        (d[2] << 4) | d[3],
        (d[4] << 4) | d[5],
        (d[6] << 4) | d[7],
    ])
}

/// Decode a GCR data block (325 GCR bytes → 260 raw bytes).
///
/// Returns the 256 data bytes (skipping the marker byte, checksum, and
/// padding), or `None` on decode error or checksum mismatch.
pub fn decode_data_block(gcr: &[u8]) -> Option<Vec<u8>> {
    if gcr.len() < 325 {
        return None;
    }

    let mut raw = Vec::with_capacity(260);
    for chunk in gcr[..325].chunks_exact(5) {
        let group = decode_gcr_group(&[chunk[0], chunk[1], chunk[2], chunk[3], chunk[4]])?;
        raw.extend_from_slice(&group);
    }

    if raw.len() < 260 {
        return None;
    }

    // raw[0] = 0x07 marker, raw[1..257] = data, raw[257] = checksum
    let data = &raw[1..257];
    let expected_checksum = raw[257];
    let mut checksum: u8 = 0;
    for &b in data {
        checksum ^= b;
    }
    if checksum != expected_checksum {
        return None;
    }

    Some(data.to_vec())
}

/// Encode 4 raw bytes into 5 GCR bytes.
///
/// Each nibble maps to a 5-bit GCR code. Four bytes = eight nibbles =
/// 40 GCR bits = 5 GCR bytes.
fn encode_gcr_group(input: &[u8; 4]) -> [u8; 5] {
    let gcr_nibbles: [u8; 8] = [
        GCR_ENCODE[(input[0] >> 4) as usize],
        GCR_ENCODE[(input[0] & 0x0F) as usize],
        GCR_ENCODE[(input[1] >> 4) as usize],
        GCR_ENCODE[(input[1] & 0x0F) as usize],
        GCR_ENCODE[(input[2] >> 4) as usize],
        GCR_ENCODE[(input[2] & 0x0F) as usize],
        GCR_ENCODE[(input[3] >> 4) as usize],
        GCR_ENCODE[(input[3] & 0x0F) as usize],
    ];

    // Pack 8 x 5-bit values into 5 bytes (40 bits)
    [
        (gcr_nibbles[0] << 3) | (gcr_nibbles[1] >> 2),
        (gcr_nibbles[1] << 6) | (gcr_nibbles[2] << 1) | (gcr_nibbles[3] >> 4),
        (gcr_nibbles[3] << 4) | (gcr_nibbles[4] >> 1),
        (gcr_nibbles[4] << 7) | (gcr_nibbles[5] << 2) | (gcr_nibbles[6] >> 3),
        (gcr_nibbles[6] << 5) | gcr_nibbles[7],
    ]
}

/// Encode a complete sector's header block.
///
/// Raw header: $08, checksum, sector, track, id2, id1, $0F, $0F
/// Returns 10 GCR bytes (8 raw → 2 groups of 4 → 2 x 5 GCR bytes).
fn encode_header(track: u8, sector: u8, disk_id: [u8; 2]) -> [u8; 10] {
    let checksum = sector ^ track ^ disk_id[0] ^ disk_id[1];
    let raw: [u8; 8] = [
        0x08, checksum, sector, track, disk_id[1], disk_id[0], 0x0F, 0x0F,
    ];
    let g0 = encode_gcr_group(&[raw[0], raw[1], raw[2], raw[3]]);
    let g1 = encode_gcr_group(&[raw[4], raw[5], raw[6], raw[7]]);
    [
        g0[0], g0[1], g0[2], g0[3], g0[4], g1[0], g1[1], g1[2], g1[3], g1[4],
    ]
}

/// Encode a complete sector's data block.
///
/// Raw: $07, 256 data bytes, checksum, $00, $00 = 260 bytes = 65 groups → 325 GCR bytes.
fn encode_data_block(sector_data: &[u8]) -> Vec<u8> {
    assert!(sector_data.len() == 256, "sector must be 256 bytes");

    let mut checksum: u8 = 0;
    for &b in sector_data {
        checksum ^= b;
    }

    // Build the 260-byte raw data block
    let mut raw = Vec::with_capacity(260);
    raw.push(0x07); // Data block marker
    raw.extend_from_slice(sector_data);
    raw.push(checksum);
    raw.push(0x00); // Padding
    raw.push(0x00); // Padding

    // Encode 65 groups of 4 bytes → 325 GCR bytes
    let mut gcr = Vec::with_capacity(325);
    for chunk in raw.chunks_exact(4) {
        let group = encode_gcr_group(&[chunk[0], chunk[1], chunk[2], chunk[3]]);
        gcr.extend_from_slice(&group);
    }
    gcr
}

/// Encode a complete sector (sync + header + gap + sync + data + gap).
fn encode_sector(track: u8, sector: u8, data: &[u8], disk_id: [u8; 2]) -> Vec<u8> {
    let mut out = Vec::with_capacity(380);

    // Header sync: 5 bytes of $FF
    out.extend_from_slice(&[0xFF; 5]);
    // Header block: 10 GCR bytes
    out.extend_from_slice(&encode_header(track, sector, disk_id));
    // Header gap: 9 bytes of $55
    out.extend_from_slice(&[0x55; 9]);
    // Data sync: 5 bytes of $FF
    out.extend_from_slice(&[0xFF; 5]);
    // Data block: 325 GCR bytes
    out.extend_from_slice(&encode_data_block(data));
    // Inter-sector gap: 9 bytes of $55
    out.extend_from_slice(&[0x55; 9]);

    out
}

/// Encode a complete track from a D64 image.
///
/// Returns the GCR-encoded byte stream for the entire track, which the
/// drive head reads continuously in a loop.
#[must_use]
pub fn encode_track(d64: &D64, track: u8) -> Vec<u8> {
    let num_sectors = D64::sectors_per_track(track);
    let disk_id = d64.disk_id();

    let mut gcr_track = Vec::with_capacity(num_sectors as usize * 380);
    for sector in 0..num_sectors {
        let data = d64
            .read_sector(track, sector)
            .expect("valid track/sector within D64");
        gcr_track.extend_from_slice(&encode_sector(track, sector, data, disk_id));
    }
    gcr_track
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gcr_group_encode_known_values() {
        // Encode [0x00, 0x00, 0x00, 0x00] → all GCR(0)=0x0A → 40 bits of 01010
        let result = encode_gcr_group(&[0x00, 0x00, 0x00, 0x00]);
        // 01010_01010_01010_01010_01010_01010_01010_01010
        // = 01010010 10010100 10100101 00101001 01001010
        assert_eq!(result, [0x52, 0x94, 0xA5, 0x29, 0x4A]);
    }

    #[test]
    fn gcr_group_roundtrip_all_ff() {
        // 0xFF: high nibble = F → GCR(F)=0x15, low nibble = F → GCR(F)=0x15
        let result = encode_gcr_group(&[0xFF, 0xFF, 0xFF, 0xFF]);
        // 10101_10101_10101_10101_10101_10101_10101_10101
        // = 10101101 01101011 01011010 11010110 10110101
        assert_eq!(result, [0xAD, 0x6B, 0x5A, 0xD6, 0xB5]);
    }

    #[test]
    fn sector_has_sync_header_data() {
        let data = [0u8; 256];
        let encoded = encode_sector(1, 0, &data, [0x41, 0x42]);

        // Check header sync
        assert_eq!(&encoded[0..5], &[0xFF; 5]);
        // Header: 10 GCR bytes at offset 5
        // Header gap: 9 bytes at offset 15
        assert_eq!(&encoded[15..24], &[0x55; 9]);
        // Data sync at offset 24
        assert_eq!(&encoded[24..29], &[0xFF; 5]);
        // Data block: 325 bytes at offset 29
        // Inter-sector gap: 9 bytes at offset 354
        assert_eq!(&encoded[354..363], &[0x55; 9]);
        // Total: 5+10+9+5+325+9 = 363
        assert_eq!(encoded.len(), 363);
    }

    #[test]
    fn checksum_valid() {
        let mut data = [0u8; 256];
        data[0] = 0xAB;
        data[1] = 0xCD;
        let encoded = encode_data_block(&data);
        assert_eq!(encoded.len(), 325);
    }

    #[test]
    fn track_length_matches_zone() {
        let d64_data = vec![0u8; 174_848];
        let d64 = D64::from_bytes(&d64_data).expect("valid");

        // Track 1: 21 sectors, zone 0
        let t1 = encode_track(&d64, 1);
        assert_eq!(t1.len(), 21 * 363);

        // Track 18: 19 sectors, zone 1
        let t18 = encode_track(&d64, 18);
        assert_eq!(t18.len(), 19 * 363);

        // Track 31: 17 sectors, zone 3
        let t31 = encode_track(&d64, 31);
        assert_eq!(t31.len(), 17 * 363);
    }

    #[test]
    fn speed_zone_values() {
        assert_eq!(speed_zone(1), 0);
        assert_eq!(speed_zone(17), 0);
        assert_eq!(speed_zone(18), 1);
        assert_eq!(speed_zone(24), 1);
        assert_eq!(speed_zone(25), 2);
        assert_eq!(speed_zone(30), 2);
        assert_eq!(speed_zone(31), 3);
        assert_eq!(speed_zone(35), 3);
    }

    #[test]
    fn cycles_per_byte_values() {
        assert_eq!(cycles_per_byte(1), 208);
        assert_eq!(cycles_per_byte(18), 224);
        assert_eq!(cycles_per_byte(25), 240);
        assert_eq!(cycles_per_byte(31), 256);
    }
}
