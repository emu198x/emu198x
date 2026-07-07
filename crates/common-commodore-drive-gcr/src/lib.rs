//! Commodore GCR disk codec — the raw-GCR encode/decode primitives shared by
//! the 1541 and 1571 drive cores.
//!
//! Both drives lay the same 4-to-5 GCR bitstream on the surface: sector data is
//! grouped four bytes at a time into five GCR bytes ([`encode_4bytes_to_gcr`]),
//! wrapped in a header/data block layout ([`encode_sector_to_gcr`]); reading
//! reverses it, locating sync marks ([`gcr_find_sync`]) and decoding blocks
//! back to sectors ([`gcr_read_sector_from_raw_track`]). This crate is the first
//! extraction of the shared drive core (#764); the rotation/serialiser engine
//! and the track builders follow in later steps.
//!
//! The geometry constants ([`MAX_HEAD_POSITION`], [`TRACK_SLOT_COUNT`]) and
//! [`speed_zone_for_track`] encode the 1541/1571's identical physical geometry.
//! A later step parameterises geometry so other Commodore GCR drives (4040,
//! 8050, 1551) can reuse this codec with a different track layout.

use format_commodore_c64_d64::{D64ParseError, read_sector, sectors_in_track};
use format_commodore_c64_g64::G64Image;
use serde::{Deserialize, Serialize};

/// How a mounted disk's bytes are encoded, so the drive tracks the format
/// once (at mount) rather than re-sniffing it on every rebuild/flush. Shared by
/// the 1541 (D64/G64) and 1571 (D64/G64/D71).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriveImageFormat {
    /// A decoded sector image (the surface is GCR-encoded on mount).
    #[default]
    D64,
    /// A raw-GCR image (the surface is the file's bytes verbatim, preserving
    /// copy protection). Read-only in v1.
    G64,
    /// A decoded double-sided sector image (1571 only).
    D71,
}

/// GCR 4-to-5 encoding table: a 4-bit nibble maps to a 5-bit GCR code with no
/// more than two consecutive zero bits, so the bitstream is self-clocking.
pub const GCR_CONVERSION_TABLE: [u8; 16] = [
    0x0A, 0x0B, 0x12, 0x13, 0x0E, 0x0F, 0x16, 0x17, 0x09, 0x19, 0x1A, 0x1B, 0x0D, 0x1D, 0x1E, 0x15,
];

/// Inverse of [`GCR_CONVERSION_TABLE`]: maps a 5-bit GCR code back to its
/// 4-bit nibble (invalid codes map to 0).
const FROM_GCR_CONVERSION_TABLE: [u8; 32] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 0, 1, 0, 12, 4, 5, 0, 0, 2, 3, 0, 15, 6, 7, 0, 9, 10, 11, 0, 13,
    14, 0,
];

/// Length in bytes of a sync mark (`0xFF` run) that precedes a header/data block.
pub const SYNC_SIZE: usize = 5;
/// Gap in bytes between a sector's header block and its following data sync.
pub const HEADER_GAP_SIZE: usize = 9;
/// GCR-encoded size in bytes of one header + data block (excluding gaps/syncs).
pub const SECTOR_GCR_SIZE_WITH_HEADER: usize = 335;
/// The head-position count: 84 half-tracks (tracks 1–42 in half steps). Shared
/// 1541/1571 geometry.
pub const MAX_HEAD_POSITION: u8 = 84;
/// The number of addressable track slots (head positions 2..=`MAX_HEAD_POSITION`).
pub const TRACK_SLOT_COUNT: usize = (MAX_HEAD_POSITION as usize) - 1;
/// Raw GCR bytes per revolution for each of the four speed zones (1541/1571).
pub const RAW_TRACK_SIZE_BY_ZONE: [usize; 4] = [6_250, 6_666, 7_142, 7_692];
/// Inter-sector gap in bytes for each speed zone.
pub const GAP_SIZE_BY_ZONE: [usize; 4] = [9, 12, 17, 8];

/// The header fields written before a sector's data block: sector/track address
/// and the two-byte disk ID.
#[derive(Clone, Copy)]
pub struct GcrHeader {
    /// Physical sector number.
    pub sector: u8,
    /// Physical track number.
    pub track: u8,
    /// Second disk-ID byte.
    pub id2: u8,
    /// First disk-ID byte.
    pub id1: u8,
}

/// Encodes one 256-byte sector — header block, gap, and data block with
/// checksum — as raw GCR into `dest`. `dest` must be sized
/// `SECTOR_GCR_SIZE_WITH_HEADER + HEADER_GAP_SIZE + gap_size + SYNC_SIZE * 2`.
pub fn encode_sector_to_gcr(source: &[u8], dest: &mut [u8], header: GcrHeader, gap_size: usize) {
    debug_assert_eq!(source.len(), 256);
    debug_assert_eq!(
        dest.len(),
        SECTOR_GCR_SIZE_WITH_HEADER + HEADER_GAP_SIZE + gap_size + (SYNC_SIZE * 2)
    );

    dest.fill(0x55);
    let mut offset = 0usize;
    dest[offset..offset + SYNC_SIZE].fill(0xFF);
    offset += SYNC_SIZE;

    let mut block = [0u8; 4];
    block[0] = 0x08;
    block[1] = header.sector ^ header.track ^ header.id2 ^ header.id1;
    block[2] = header.sector;
    block[3] = header.track;
    encode_4bytes_to_gcr(block, &mut dest[offset..offset + 5]);
    offset += 5;

    block = [header.id2, header.id1, 0x0F, 0x0F];
    encode_4bytes_to_gcr(block, &mut dest[offset..offset + 5]);
    offset += 5;

    offset += HEADER_GAP_SIZE;
    dest[offset..offset + SYNC_SIZE].fill(0xFF);
    offset += SYNC_SIZE;

    let mut checksum = source[0] ^ source[1] ^ source[2];
    block = [0x07, source[0], source[1], source[2]];
    encode_4bytes_to_gcr(block, &mut dest[offset..offset + 5]);
    offset += 5;

    let mut index = 3usize;
    for _ in 0..63 {
        block.copy_from_slice(&source[index..index + 4]);
        checksum ^= block[0] ^ block[1] ^ block[2] ^ block[3];
        encode_4bytes_to_gcr(block, &mut dest[offset..offset + 5]);
        offset += 5;
        index += 4;
    }

    block = [source[255], checksum ^ source[255], 0, 0];
    encode_4bytes_to_gcr(block, &mut dest[offset..offset + 5]);
}

/// Encodes four data bytes into five GCR bytes (`dest` must be 5 bytes long).
pub fn encode_4bytes_to_gcr(source: [u8; 4], dest: &mut [u8]) {
    let mut encoded = 0u64;
    for byte in source {
        encoded = (encoded << 5) | u64::from(GCR_CONVERSION_TABLE[usize::from(byte >> 4)]);
        encoded = (encoded << 5) | u64::from(GCR_CONVERSION_TABLE[usize::from(byte & 0x0F)]);
    }

    dest.copy_from_slice(&encoded.to_be_bytes()[3..]);
}

/// The standard 1541/1571 speed zone (0–3) for a physical track number.
#[must_use]
pub const fn speed_zone_for_track(track: u8) -> u8 {
    (track < 31) as u8 + (track < 25) as u8 + (track < 18) as u8
}

/// The track-data slot index for a head position, or `None` when the head is
/// outside the addressable range (positions 2..=`MAX_HEAD_POSITION`).
#[must_use]
pub fn track_slot_index(head_position: u8) -> Option<usize> {
    if (2..TRACK_SLOT_COUNT as u8 + 2).contains(&head_position) {
        Some(usize::from(head_position - 2))
    } else {
        None
    }
}

/// Scans a raw GCR track for the next sync mark (ten or more `1` bits), starting
/// at `bit_offset` and looking at most `remaining_bits` ahead. Returns the bit
/// offset of the first non-`1` bit after the sync, or `None` if none is found.
#[must_use]
pub fn gcr_find_sync(
    raw: &[u8],
    mut bit_offset: usize,
    mut remaining_bits: usize,
) -> Option<usize> {
    if raw.is_empty() {
        return None;
    }

    let total_bits = raw.len() * 8;
    let mut window = 0u16;
    let mut byte = raw[bit_offset >> 3] << (bit_offset & 0x07);

    while remaining_bits > 0 {
        if byte & 0x80 != 0 {
            window = (window << 1) | 1;
        } else if window & 0x03FF != 0x03FF {
            window <<= 1;
        } else {
            return Some(bit_offset);
        }

        if (bit_offset & 0x07) != 0x07 {
            bit_offset += 1;
            byte <<= 1;
        } else {
            bit_offset += 1;
            if bit_offset >= total_bits {
                bit_offset = 0;
            }
            byte = raw[bit_offset >> 3];
        }

        remaining_bits -= 1;
    }

    None
}

/// Decodes five GCR bytes into four data bytes.
#[must_use]
pub fn gcr_decode_4bytes(source: &[u8]) -> [u8; 4] {
    let mut expanded = u32::from(source[0]) << 13;
    let mut dest = [0u8; 4];

    for (i, byte) in dest.iter_mut().enumerate() {
        expanded |= u32::from(source[i + 1]) << (5 + (i as u32 * 2));
        *byte = FROM_GCR_CONVERSION_TABLE[((expanded >> 16) & 0x1F) as usize] << 4;
        expanded <<= 5;
        *byte |= FROM_GCR_CONVERSION_TABLE[((expanded >> 16) & 0x1F) as usize];
        expanded <<= 5;
    }

    dest
}

/// Decodes `blocks` consecutive GCR groups (five bytes → four data bytes each)
/// starting at `bit_offset`, wrapping around the track as needed.
#[must_use]
pub fn gcr_decode_block(raw: &[u8], bit_offset: usize, blocks: usize) -> Vec<u8> {
    let shift = bit_offset & 0x07;
    let mut byte_offset = bit_offset >> 3;
    let mut carry = raw[byte_offset] << shift;
    let mut decoded = Vec::with_capacity(blocks * 4);

    for _ in 0..blocks {
        let mut gcr = [0u8; 5];
        for item in &mut gcr {
            byte_offset += 1;
            if byte_offset >= raw.len() {
                byte_offset = 0;
            }
            if shift == 0 {
                *item = carry;
                carry = raw[byte_offset];
            } else {
                *item = carry | (((u16::from(raw[byte_offset]) << shift) >> 8) as u8);
                carry = raw[byte_offset] << shift;
            }
        }
        decoded.extend_from_slice(&gcr_decode_4bytes(&gcr));
    }

    decoded
}

/// Reads one decoded 256-byte sector out of a raw GCR track by locating its
/// header (`0x08`, matching sector number) then its following data block
/// (`0x07`). Returns `None` if the sector or its data block can't be found.
#[must_use]
pub fn gcr_read_sector_from_raw_track(raw: &[u8], sector: u8) -> Option<[u8; 256]> {
    let total_bits = raw.len() * 8;
    let mut search = 0usize;
    let mut first_sync = None;

    loop {
        let sync = gcr_find_sync(raw, search, total_bits)?;
        if first_sync == Some(sync) {
            return None;
        }
        first_sync.get_or_insert(sync);

        let header = gcr_decode_block(raw, sync, 1);
        if header[0] == 0x08 && header[2] == sector {
            let data_sync = gcr_find_sync(raw, sync, 500 * 8)?;
            let decoded = gcr_decode_block(raw, data_sync, 65);
            if decoded[0] != 0x07 {
                return None;
            }

            let mut sector_data = [0u8; 256];
            sector_data.copy_from_slice(&decoded[1..257]);
            return Some(sector_data);
        }

        search = sync.wrapping_add(1) % total_bits;
    }
}

/// Builds the live GCR track surface (one physical side) from a decoded D64
/// image: tracks 1–35 are GCR-encoded sector-by-sector and rotated to a stable
/// start offset, landing in their half-track slots. Returns the per-slot raw
/// GCR (`TRACK_SLOT_COUNT` slots; unreachable slots stay empty).
///
/// # Errors
///
/// Propagates a [`D64ParseError`] if the BAM or any sector can't be read.
pub fn build_gcr_tracks_from_d64(bytes: &[u8]) -> Result<Vec<Vec<u8>>, D64ParseError> {
    let bam = read_sector(bytes, 18, 0)?;
    let id1 = bam[0xA2];
    let id2 = bam[0xA3];
    let mut tracks = vec![Vec::new(); TRACK_SLOT_COUNT];
    let mut track_offset = 0usize;

    for track in 1..=35u8 {
        let zone = speed_zone_for_track(track);
        let track_size = RAW_TRACK_SIZE_BY_ZONE[usize::from(zone)];
        let sectors = usize::from(sectors_in_track(track)?);
        let sector_size = SECTOR_GCR_SIZE_WITH_HEADER
            + HEADER_GAP_SIZE
            + GAP_SIZE_BY_ZONE[usize::from(zone)]
            + (SYNC_SIZE * 2);
        let mut temp = vec![0x55; track_size];
        let gap_size = GAP_SIZE_BY_ZONE[usize::from(zone)];

        for sector in 0..sectors {
            let offset = sector * sector_size;
            encode_sector_to_gcr(
                read_sector(bytes, track, sector as u8)?,
                &mut temp[offset..offset + sector_size],
                GcrHeader {
                    sector: sector as u8,
                    track,
                    id2,
                    id1,
                },
                gap_size,
            );
        }

        track_offset += (sectors * sector_size).saturating_sub(gap_size);
        track_offset += (track_size * 100) / 270;
        track_offset %= track_size;

        let mut raw = vec![0x55; track_size];
        raw[track_offset..].copy_from_slice(&temp[..track_size - track_offset]);
        raw[..track_offset].copy_from_slice(&temp[track_size - track_offset..]);

        let slot = usize::from((track * 2) - 2);
        tracks[slot] = raw;
    }

    Ok(tracks)
}

/// Builds the live GCR track surface (one physical side) from a parsed G64: each
/// half-track's raw GCR drops into the matching slot verbatim (the G64
/// half-track index is the drive's own slot index). Slots beyond the drive's
/// reachable range are dropped.
#[must_use]
pub fn build_gcr_tracks_from_g64(image: &G64Image) -> Vec<Vec<u8>> {
    let mut tracks = vec![Vec::new(); TRACK_SLOT_COUNT];
    for (slot, track) in image.half_tracks.iter().enumerate().take(TRACK_SLOT_COUNT) {
        if let Some(track) = track {
            tracks[slot] = track.gcr.clone();
        }
    }
    tracks
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A whole 256-byte sector round-trips through GCR encode → read-back.
    #[test]
    fn sector_encodes_and_reads_back() {
        let mut source = [0u8; 256];
        for (i, byte) in source.iter_mut().enumerate() {
            *byte = (i as u8).wrapping_mul(7).wrapping_add(3);
        }
        let gap_size = 8;
        let mut gcr =
            vec![0u8; SECTOR_GCR_SIZE_WITH_HEADER + HEADER_GAP_SIZE + gap_size + SYNC_SIZE * 2];
        encode_sector_to_gcr(
            &source,
            &mut gcr,
            GcrHeader {
                sector: 5,
                track: 18,
                id2: 0x41,
                id1: 0x42,
            },
            gap_size,
        );
        // Pad the track so the reader's wrap-around has room to find the syncs.
        gcr.extend(std::iter::repeat_n(0x55u8, 64));
        let read = gcr_read_sector_from_raw_track(&gcr, 5).expect("sector 5 reads back");
        assert_eq!(read, source);
    }

    #[test]
    fn four_bytes_round_trip() {
        let block = [0xDE, 0xAD, 0xBE, 0xEF];
        let mut gcr = [0u8; 5];
        encode_4bytes_to_gcr(block, &mut gcr);
        assert_eq!(gcr_decode_4bytes(&gcr), block);
    }

    #[test]
    fn speed_zones_follow_the_1541_track_boundaries() {
        assert_eq!(speed_zone_for_track(1), 3);
        assert_eq!(speed_zone_for_track(17), 3);
        assert_eq!(speed_zone_for_track(18), 2);
        assert_eq!(speed_zone_for_track(24), 2);
        assert_eq!(speed_zone_for_track(25), 1);
        assert_eq!(speed_zone_for_track(30), 1);
        assert_eq!(speed_zone_for_track(31), 0);
        assert_eq!(speed_zone_for_track(42), 0);
    }

    #[test]
    fn track_slot_index_maps_head_positions() {
        assert_eq!(track_slot_index(0), None);
        assert_eq!(track_slot_index(1), None);
        assert_eq!(track_slot_index(2), Some(0));
        assert_eq!(track_slot_index(3), Some(1));
        assert_eq!(track_slot_index(70), Some(68));
        assert_eq!(track_slot_index(MAX_HEAD_POSITION + 1), None);
    }
}
