//! IPF (Interchangeable Preservation Format) disk image parser.
//!
//! IPF files store pre-encoded MFM data per track, preserving copy protection
//! timing and non-standard sector layouts. This parser handles the container
//! format: record headers, CAPS/INFO/IMGE/DATA records.
//!
//! IPF images are read-only — the format preserves magnetic flux transitions
//! and cannot meaningfully accept sector writes.

use drive_amiga_floppy::DiskImage;

/// Maximum number of tracks: 84 cylinders x 2 heads.
const MAX_TRACKS: usize = 168;

/// IPF record type identifiers.
const RECORD_CAPS: u32 = 0x4341_5053; // "CAPS"
const RECORD_INFO: u32 = 0x494E_464F; // "INFO"
const RECORD_IMGE: u32 = 0x494D_4745; // "IMGE"
const RECORD_DATA: u32 = 0x4441_5441; // "DATA"

/// IPF file magic bytes ("CAPS").
const IPF_MAGIC: &[u8; 4] = b"CAPS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpfDataType {
    /// Standard Amiga track data.
    Standard,
    /// Copy-protected track with non-standard timing.
    CopyProtect,
    /// Empty / unformatted track.
    Empty,
}

#[derive(Debug, Clone)]
struct IpfTrack {
    mfm_data: Vec<u8>,
    data_type: IpfDataType,
}

/// Parsed IPF disk image.
#[derive(Debug)]
pub struct IpfImage {
    tracks: Vec<Option<IpfTrack>>,
    sectors_per_track: u32,
}

/// Errors returned by the IPF parser.
#[derive(Debug, Clone)]
pub enum IpfError {
    /// File is too short to contain a valid header.
    TooShort,
    /// Missing "CAPS" magic at the start.
    BadMagic,
    /// Record header is truncated or malformed.
    TruncatedRecord { offset: usize },
    /// Record CRC does not match.
    BadRecordCrc { record_type: u32, offset: usize },
    /// A DATA record references a region beyond the file.
    DataOutOfBounds {
        track: usize,
        offset: usize,
        len: usize,
    },
    /// No INFO record found.
    MissingInfo,
}

impl std::fmt::Display for IpfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "IPF file too short"),
            Self::BadMagic => write!(f, "missing CAPS magic"),
            Self::TruncatedRecord { offset } => {
                write!(f, "truncated record at offset {offset}")
            }
            Self::BadRecordCrc {
                record_type,
                offset,
            } => {
                write!(
                    f,
                    "bad CRC in record 0x{record_type:08X} at offset {offset}"
                )
            }
            Self::DataOutOfBounds { track, offset, len } => {
                write!(
                    f,
                    "DATA out of bounds: track {track}, offset {offset}, len {len}"
                )
            }
            Self::MissingInfo => write!(f, "no INFO record found"),
        }
    }
}

impl std::error::Error for IpfError {}

/// Read a big-endian u32 from a byte slice.
fn read_be_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// CRC-32 used by IPF record headers (standard CRC-32/ISO-HDLC).
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// IPF record header layout (12-byte prefix):
///   0-3: record type (4-byte ASCII identifier)
///   4-7: record length (total, including these 12 bytes)
///   8-11: CRC-32 of the record (with CRC field zeroed)
/// Remaining bytes up to `length` are record-specific payload.
struct RecordHeader {
    record_type: u32,
    length: u32,
    crc: u32,
}

/// Minimum record size: type(4) + length(4) + CRC(4).
const RECORD_PREFIX_SIZE: usize = 12;

fn parse_record_header(data: &[u8], offset: usize) -> Result<RecordHeader, IpfError> {
    if offset + RECORD_PREFIX_SIZE > data.len() {
        return Err(IpfError::TruncatedRecord { offset });
    }
    Ok(RecordHeader {
        record_type: read_be_u32(data, offset),
        length: read_be_u32(data, offset + 4),
        crc: read_be_u32(data, offset + 8),
    })
}

fn verify_record_crc(data: &[u8], offset: usize, header: &RecordHeader) -> Result<(), IpfError> {
    let len = header.length as usize;
    if offset + len > data.len() {
        return Err(IpfError::TruncatedRecord { offset });
    }
    // CRC is computed over the record with the CRC field (bytes 8-11) zeroed.
    let mut buf = data[offset..offset + len].to_vec();
    buf[8] = 0;
    buf[9] = 0;
    buf[10] = 0;
    buf[11] = 0;
    let computed = crc32(&buf);
    if computed != header.crc {
        return Err(IpfError::BadRecordCrc {
            record_type: header.record_type,
            offset,
        });
    }
    Ok(())
}

impl IpfImage {
    /// Parse an IPF file from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns `IpfError` if the file is too short, has bad magic, contains
    /// truncated or corrupt records, or references out-of-bounds data.
    #[allow(clippy::too_many_lines)]
    pub fn from_bytes(data: &[u8]) -> Result<Self, IpfError> {
        if data.len() < 4 {
            return Err(IpfError::TooShort);
        }

        // Validate magic.
        if &data[0..4] != IPF_MAGIC {
            return Err(IpfError::BadMagic);
        }

        let mut tracks: Vec<Option<IpfTrack>> = (0..MAX_TRACKS).map(|_| None).collect();
        let mut imge_records: Vec<ImgeRecord> = Vec::new();
        let mut data_records: Vec<DataRecord> = Vec::new();
        let mut found_info = false;
        let mut sectors_per_track = 11u32; // default DD

        let mut offset = 0;
        while offset + RECORD_PREFIX_SIZE <= data.len() {
            let header = parse_record_header(data, offset)?;
            let len = header.length as usize;
            if len < RECORD_PREFIX_SIZE || offset + len > data.len() {
                break;
            }

            verify_record_crc(data, offset, &header)?;

            match header.record_type {
                RECORD_CAPS => {
                    // Already validated magic above; nothing extra needed.
                }
                RECORD_INFO => {
                    found_info = true;
                    // INFO payload: media_type(4), encoder_type(4), ...
                    if len >= RECORD_PREFIX_SIZE + 4 {
                        let media_type = read_be_u32(data, offset + RECORD_PREFIX_SIZE);
                        // Media type 1 = floppy DD (11 SPT), 2 = floppy HD (22 SPT).
                        sectors_per_track = if media_type == 2 { 22 } else { 11 };
                    }
                }
                RECORD_IMGE => {
                    // IMGE payload (after 12-byte prefix): track(4), side(4),
                    // density(4), signal(4), data_key(4), ... at least 20 bytes.
                    if len >= RECORD_PREFIX_SIZE + 20 {
                        let p = offset + RECORD_PREFIX_SIZE;
                        let track = read_be_u32(data, p) as usize;
                        let side = read_be_u32(data, p + 4) as usize;
                        let density_type = read_be_u32(data, p + 8);
                        let signal_type = read_be_u32(data, p + 12);
                        let data_key = read_be_u32(data, p + 16);
                        imge_records.push(ImgeRecord {
                            track,
                            side,
                            density_type,
                            signal_type,
                            data_key,
                        });
                    }
                }
                RECORD_DATA => {
                    // DATA payload: data_length(4), data_bits(4), data_key(4),
                    // then raw MFM bytes.
                    if len >= RECORD_PREFIX_SIZE + 12 {
                        let p = offset + RECORD_PREFIX_SIZE;
                        let data_length = read_be_u32(data, p) as usize;
                        let data_bits = read_be_u32(data, p + 4) as usize;
                        let data_key = read_be_u32(data, p + 8);
                        let mfm_offset = p + 12;
                        let mfm_len = if data_length > 0 {
                            data_length
                        } else {
                            data_bits.div_ceil(8)
                        };
                        data_records.push(DataRecord {
                            data_key,
                            mfm_offset,
                            mfm_len,
                        });
                    }
                }
                _ => {
                    // Unknown record type — skip.
                }
            }

            offset += len;
        }

        if !found_info {
            return Err(IpfError::MissingInfo);
        }

        // Match IMGE records to DATA records by data_key and populate tracks.
        for imge in &imge_records {
            let track_idx = imge.track * 2 + imge.side;
            if track_idx >= MAX_TRACKS {
                continue;
            }

            // Find matching DATA record.
            let data_rec = data_records.iter().find(|d| d.data_key == imge.data_key);
            let mfm_data = if let Some(dr) = data_rec {
                if dr.mfm_offset + dr.mfm_len > data.len() {
                    return Err(IpfError::DataOutOfBounds {
                        track: track_idx,
                        offset: dr.mfm_offset,
                        len: dr.mfm_len,
                    });
                }
                data[dr.mfm_offset..dr.mfm_offset + dr.mfm_len].to_vec()
            } else {
                Vec::new()
            };

            let data_type = match imge.density_type {
                0 => IpfDataType::Empty,
                1 => IpfDataType::Standard,
                _ => IpfDataType::CopyProtect,
            };

            tracks[track_idx] = Some(IpfTrack {
                mfm_data,
                data_type,
            });
        }

        Ok(Self {
            tracks,
            sectors_per_track,
        })
    }

    /// Check whether the first 4 bytes of `data` are the "CAPS" magic.
    #[must_use]
    pub fn is_ipf(data: &[u8]) -> bool {
        data.len() >= 4 && &data[0..4] == IPF_MAGIC
    }

    /// Number of populated tracks.
    #[must_use]
    pub fn track_count(&self) -> usize {
        self.tracks.iter().filter(|t| t.is_some()).count()
    }

    /// Access the raw MFM data for a specific track.
    #[must_use]
    pub fn track_mfm(&self, cyl: u32, head: u32) -> Option<&[u8]> {
        let idx = (cyl as usize) * 2 + (head as usize);
        self.tracks
            .get(idx)?
            .as_ref()
            .map(|t| t.mfm_data.as_slice())
    }

    /// Track data type.
    #[must_use]
    pub fn track_data_type(&self, cyl: u32, head: u32) -> Option<IpfDataType> {
        let idx = (cyl as usize) * 2 + (head as usize);
        self.tracks.get(idx)?.as_ref().map(|t| t.data_type)
    }
}

impl DiskImage for IpfImage {
    fn encode_mfm_track(&self, cyl: u32, head: u32) -> Option<Vec<u8>> {
        self.track_mfm(cyl, head).map(<[u8]>::to_vec)
    }

    fn sectors_per_track(&self) -> u32 {
        self.sectors_per_track
    }

    fn is_writable(&self) -> bool {
        false
    }

    fn write_sector(&mut self, _cyl: u32, _head: u32, _sector: u32, _data: &[u8]) {
        // IPF is read-only — writes are silently ignored.
    }

    fn save_data(&self) -> Option<Vec<u8>> {
        None // IPF cannot be saved back.
    }
}

// Internal parsing helpers.

struct ImgeRecord {
    track: usize,
    side: usize,
    density_type: u32,
    #[allow(dead_code)]
    signal_type: u32,
    data_key: u32,
}

struct DataRecord {
    data_key: u32,
    mfm_offset: usize,
    mfm_len: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid IPF file with one track of MFM data.
    fn build_test_ipf(track: u32, head: u32, mfm: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();

        // CAPS record: empty payload (just the 12-byte prefix).
        buf.extend_from_slice(&build_record(RECORD_CAPS, &[]));

        // INFO record: media_type(4) = 1 (DD floppy).
        let mut info_payload = [0u8; 4];
        info_payload[0..4].copy_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(&build_record(RECORD_INFO, &info_payload));

        // IMGE record: track(4), side(4), density(4), signal(4), data_key(4).
        let mut imge_payload = [0u8; 20];
        imge_payload[0..4].copy_from_slice(&track.to_be_bytes());
        imge_payload[4..8].copy_from_slice(&head.to_be_bytes());
        imge_payload[8..12].copy_from_slice(&1u32.to_be_bytes()); // Standard
        imge_payload[12..16].copy_from_slice(&0u32.to_be_bytes()); // signal
        imge_payload[16..20].copy_from_slice(&42u32.to_be_bytes()); // data_key
        buf.extend_from_slice(&build_record(RECORD_IMGE, &imge_payload));

        // DATA record: data_length(4), data_bits(4), data_key(4), then MFM.
        let mfm_len = mfm.len() as u32;
        let mut data_payload = Vec::with_capacity(12 + mfm.len());
        data_payload.extend_from_slice(&mfm_len.to_be_bytes());
        data_payload.extend_from_slice(&0u32.to_be_bytes()); // data_bits
        data_payload.extend_from_slice(&42u32.to_be_bytes()); // data_key
        data_payload.extend_from_slice(mfm);
        buf.extend_from_slice(&build_record(RECORD_DATA, &data_payload));

        buf
    }

    /// Build a single IPF record with correct CRC.
    fn build_record(record_type: u32, payload: &[u8]) -> Vec<u8> {
        let total_len = (RECORD_PREFIX_SIZE + payload.len()) as u32;
        let mut rec = Vec::with_capacity(total_len as usize);
        rec.extend_from_slice(&record_type.to_be_bytes()); // 0-3: type
        rec.extend_from_slice(&total_len.to_be_bytes()); // 4-7: length
        rec.extend_from_slice(&[0u8; 4]); // 8-11: CRC placeholder
        rec.extend_from_slice(payload); // 12+: record-specific data

        // Compute CRC over the record with CRC field zeroed (already zero).
        let computed = crc32(&rec);
        rec[8..12].copy_from_slice(&computed.to_be_bytes());
        rec
    }

    #[test]
    fn is_ipf_detects_magic() {
        assert!(IpfImage::is_ipf(b"CAPS\x00\x00"));
        assert!(!IpfImage::is_ipf(b"ADF\x00\x00\x00"));
        assert!(!IpfImage::is_ipf(b"CAP"));
    }

    #[test]
    fn parse_minimal_ipf() {
        let mfm = vec![0xAAu8; 128];
        let ipf_data = build_test_ipf(0, 0, &mfm);
        let image = IpfImage::from_bytes(&ipf_data).expect("should parse");

        assert_eq!(image.track_count(), 1);
        assert_eq!(image.sectors_per_track(), 11);
        assert_eq!(image.track_mfm(0, 0).expect("track 0/0"), &mfm[..]);
        assert_eq!(image.track_data_type(0, 0), Some(IpfDataType::Standard));
    }

    #[test]
    fn parse_ipf_two_sided_track() {
        let mfm_h0 = vec![0x44u8; 64];
        let mfm_h1 = vec![0x89u8; 96];

        let mut buf = Vec::new();
        buf.extend_from_slice(&build_record(RECORD_CAPS, &[]));

        let mut info_payload = [0u8; 4];
        info_payload[0..4].copy_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(&build_record(RECORD_INFO, &info_payload));

        // IMGE for cyl 5, head 0, key=10.
        let mut imge0 = [0u8; 20];
        imge0[0..4].copy_from_slice(&5u32.to_be_bytes());
        imge0[4..8].copy_from_slice(&0u32.to_be_bytes());
        imge0[8..12].copy_from_slice(&1u32.to_be_bytes());
        imge0[16..20].copy_from_slice(&10u32.to_be_bytes());
        buf.extend_from_slice(&build_record(RECORD_IMGE, &imge0));

        // IMGE for cyl 5, head 1, key=11.
        let mut imge1 = [0u8; 20];
        imge1[0..4].copy_from_slice(&5u32.to_be_bytes());
        imge1[4..8].copy_from_slice(&1u32.to_be_bytes());
        imge1[8..12].copy_from_slice(&1u32.to_be_bytes());
        imge1[16..20].copy_from_slice(&11u32.to_be_bytes());
        buf.extend_from_slice(&build_record(RECORD_IMGE, &imge1));

        // DATA key=10: data_length(4), data_bits(4), data_key(4), mfm.
        let mut dp0 = Vec::new();
        dp0.extend_from_slice(&(mfm_h0.len() as u32).to_be_bytes());
        dp0.extend_from_slice(&0u32.to_be_bytes());
        dp0.extend_from_slice(&10u32.to_be_bytes());
        dp0.extend_from_slice(&mfm_h0);
        buf.extend_from_slice(&build_record(RECORD_DATA, &dp0));

        // DATA key=11.
        let mut dp1 = Vec::new();
        dp1.extend_from_slice(&(mfm_h1.len() as u32).to_be_bytes());
        dp1.extend_from_slice(&0u32.to_be_bytes());
        dp1.extend_from_slice(&11u32.to_be_bytes());
        dp1.extend_from_slice(&mfm_h1);
        buf.extend_from_slice(&build_record(RECORD_DATA, &dp1));

        let image = IpfImage::from_bytes(&buf).expect("should parse");
        assert_eq!(image.track_count(), 2);
        assert_eq!(image.track_mfm(5, 0).expect("cyl 5 head 0"), &mfm_h0[..]);
        assert_eq!(image.track_mfm(5, 1).expect("cyl 5 head 1"), &mfm_h1[..]);
    }

    #[test]
    fn missing_magic_returns_error() {
        let result = IpfImage::from_bytes(b"NOPE_not_ipf_data");
        assert!(result.is_err());
    }

    #[test]
    fn ipf_is_not_writable() {
        let mfm = vec![0xAAu8; 64];
        let ipf_data = build_test_ipf(0, 0, &mfm);
        let image = IpfImage::from_bytes(&ipf_data).expect("should parse");
        assert!(!image.is_writable());
        assert!(image.save_data().is_none());
    }

    #[test]
    fn disk_image_trait_encode_mfm_track() {
        let mfm = vec![0x44u8; 200];
        let ipf_data = build_test_ipf(3, 1, &mfm);
        let image = IpfImage::from_bytes(&ipf_data).expect("should parse");

        // Use through the DiskImage trait.
        let encoded = DiskImage::encode_mfm_track(&image, 3, 1);
        assert_eq!(encoded.as_deref(), Some(mfm.as_slice()));

        // Missing track returns None.
        assert!(DiskImage::encode_mfm_track(&image, 0, 0).is_none());
    }

    #[test]
    fn bad_crc_detected() {
        let mfm = vec![0xAAu8; 64];
        let mut ipf_data = build_test_ipf(0, 0, &mfm);
        // Corrupt the CRC of the first record (CAPS, bytes 8-11).
        ipf_data[8] ^= 0xFF;
        let result = IpfImage::from_bytes(&ipf_data);
        assert!(result.is_err());
    }

    #[test]
    fn hd_media_type_gives_22_spt() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_record(RECORD_CAPS, &[]));

        let mut info_payload = [0u8; 4];
        info_payload[0..4].copy_from_slice(&2u32.to_be_bytes()); // HD
        buf.extend_from_slice(&build_record(RECORD_INFO, &info_payload));

        let image = IpfImage::from_bytes(&buf).expect("should parse");
        assert_eq!(image.sectors_per_track(), 22);
    }
}
