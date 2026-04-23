/// Spectrum snapshot format parsers (.z80 and .sna).
///
/// Both formats produce the same `Z80Snapshot` struct, which the machine
/// crate uses to restore state.
pub mod sna;

/// .z80 snapshot format parser.
///
/// Supports v1, v2, and v3 formats. Decompresses ED ED-compressed data
/// and maps hardware types to Spectrum models.
///
/// Format reference: https://worldofspectrum.org/faq/reference/z80format.htm

/// Parsed .z80 snapshot — machine-agnostic representation.
#[derive(Clone, Debug)]
pub struct Z80Snapshot {
    // Z80 registers
    pub af: u16,
    pub bc: u16,
    pub de: u16,
    pub hl: u16,
    pub af_alt: u16,
    pub bc_alt: u16,
    pub de_alt: u16,
    pub hl_alt: u16,
    pub ix: u16,
    pub iy: u16,
    pub sp: u16,
    pub pc: u16,
    pub i: u8,
    pub r: u8,
    pub im: u8,
    pub iff1: bool,
    pub iff2: bool,

    /// Border colour (0-7).
    pub border: u8,

    /// Hardware model.
    pub model: SnapshotModel,

    /// Port $7FFD value (128K paging state). 0 for 48K snapshots.
    pub port_7ffd: u8,
    /// Port $1FFD value (+2A/+3 paging). 0 if not applicable.
    pub port_1ffd: u8,
    /// Port $FFFD value (AY register select). 0 if not applicable.
    pub ay_register: u8,
    /// AY register contents (16 bytes).
    pub ay_regs: [u8; 16],

    /// Memory pages: (page_number, 16384 bytes).
    /// Page numbering: 0-7 = RAM banks, 8 = ROM 0, etc.
    /// For 48K v1 snapshots: pages 5, 2, 0 (= $4000, $8000, $C000).
    pub pages: Vec<(u8, Vec<u8>)>,
}

/// Which Spectrum model the snapshot targets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapshotModel {
    Spectrum48K,
    Spectrum128K,
    SpectrumPlus2,
    SpectrumPlus2A,
    SpectrumPlus3,
    Pentagon128,
    Scorpion256,
}

/// Parse a .z80 file.
pub fn parse_z80(data: &[u8]) -> Result<Z80Snapshot, String> {
    if data.len() < 30 {
        return Err("File too short for .z80 header".into());
    }

    let af = u16::from_be_bytes([data[0], data[1]]); // A then F (big-endian!)
    let bc = read_u16(data, 2);
    let hl = read_u16(data, 4);
    let pc_v1 = read_u16(data, 6);
    let sp = read_u16(data, 8);
    let i = data[10];
    let mut r = data[11] & 0x7F;
    let byte12 = if data[12] == 0xFF { 1 } else { data[12] };
    r |= (byte12 & 0x01) << 7; // Bit 7 of R from byte 12
    let border = (byte12 >> 1) & 0x07;
    let compressed_v1 = byte12 & 0x20 != 0;
    let de = read_u16(data, 13);
    let bc_alt = read_u16(data, 15);
    let de_alt = read_u16(data, 17);
    let hl_alt = read_u16(data, 19);
    let af_alt = u16::from_be_bytes([data[21], data[22]]); // A' then F'
    let iy = read_u16(data, 23);
    let ix = read_u16(data, 25);
    let iff1 = data[27] != 0;
    let iff2 = data[28] != 0;
    let im = data[29] & 0x03;

    if pc_v1 != 0 {
        // Version 1: 48K only, data follows the 30-byte header
        let raw = &data[30..];
        let mem = if compressed_v1 {
            decompress_v1(raw)?
        } else {
            if raw.len() < 49152 {
                return Err(format!("v1 uncompressed data too short: {}", raw.len()));
            }
            raw[..49152].to_vec()
        };

        // Split into v2/v3-compatible page numbers:
        // page 8 = $4000-$7FFF, page 4 = $8000-$BFFF, page 5 = $C000-$FFFF
        let pages = vec![
            (8, mem[..16384].to_vec()),
            (4, mem[16384..32768].to_vec()),
            (5, mem[32768..49152].to_vec()),
        ];

        return Ok(Z80Snapshot {
            af,
            bc,
            de,
            hl,
            af_alt,
            bc_alt,
            de_alt,
            hl_alt,
            ix,
            iy,
            sp,
            pc: pc_v1,
            i,
            r,
            im,
            iff1,
            iff2,
            border,
            model: SnapshotModel::Spectrum48K,
            port_7ffd: 0,
            port_1ffd: 0,
            ay_register: 0,
            ay_regs: [0; 16],
            pages,
        });
    }

    // Version 2 or 3: extended header
    if data.len() < 32 {
        return Err("File too short for v2/v3 header".into());
    }
    let ext_len = read_u16(data, 30) as usize;
    let pc = read_u16(data, 32);

    if data.len() < 32 + ext_len {
        return Err("File too short for extended header".into());
    }

    let hw_mode = data[34];
    let port_7ffd = data[35];
    let ay_register = if ext_len >= 25 { data[38] } else { 0 };
    let mut ay_regs = [0u8; 16];
    if ext_len >= 41 {
        ay_regs.copy_from_slice(&data[39..55]);
    }

    let port_1ffd = if ext_len >= 54 { data[86] } else { 0 };

    // Map hardware mode to model
    let model = match (ext_len, hw_mode) {
        (23, 0) | (54.., 0) => SnapshotModel::Spectrum48K,
        (23, 1) | (54.., 1) => SnapshotModel::Spectrum48K, // 48K + IF1
        (23, 2) => SnapshotModel::Spectrum48K,             // SamRam (treat as 48K)
        (23, 3) | (54.., 3) => SnapshotModel::Spectrum128K,
        (23, 4) | (54.., 4) => SnapshotModel::Spectrum128K, // 128K + IF1
        (54.., 5) => SnapshotModel::Spectrum128K,           // +2
        (54.., 6) => SnapshotModel::SpectrumPlus2A,
        (54.., 7) => SnapshotModel::SpectrumPlus2A, // +2A
        (54.., 9) => SnapshotModel::Pentagon128,
        (54.., 10) => SnapshotModel::Scorpion256,
        (54.., 12) => SnapshotModel::SpectrumPlus2,  // +2
        (54.., 13) => SnapshotModel::SpectrumPlus2A, // +2A
        _ => SnapshotModel::Spectrum48K,             // Default fallback
    };

    // Parse memory pages
    let mut offset = 32 + ext_len;
    let mut pages = Vec::new();

    while offset + 3 <= data.len() {
        let block_len = read_u16(data, offset) as usize;
        let page_num = data[offset + 2];
        offset += 3;

        if block_len == 0xFFFF {
            // Uncompressed: 16384 bytes follow
            if offset + 16384 > data.len() {
                break;
            }
            pages.push((page_num, data[offset..offset + 16384].to_vec()));
            offset += 16384;
        } else {
            if offset + block_len > data.len() {
                break;
            }
            let decompressed = decompress_page(&data[offset..offset + block_len])?;
            pages.push((page_num, decompressed));
            offset += block_len;
        }
    }

    Ok(Z80Snapshot {
        af,
        bc,
        de,
        hl,
        af_alt,
        bc_alt,
        de_alt,
        hl_alt,
        ix,
        iy,
        sp,
        pc,
        i,
        r,
        im,
        iff1,
        iff2,
        border,
        model,
        port_7ffd,
        port_1ffd,
        ay_register,
        ay_regs,
        pages,
    })
}

/// Decompress v1 format (whole 48K block, terminated by 00 ED ED 00).
fn decompress_v1(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(49152);
    let mut i = 0;

    while i < data.len() && out.len() < 49152 {
        if i + 3 < data.len()
            && data[i] == 0x00
            && data[i + 1] == 0xED
            && data[i + 2] == 0xED
            && data[i + 3] == 0x00
        {
            break; // End marker
        }

        if i + 3 < data.len() && data[i] == 0xED && data[i + 1] == 0xED {
            let count = data[i + 2] as usize;
            let val = data[i + 3];
            for _ in 0..count {
                out.push(val);
            }
            i += 4;
        } else {
            out.push(data[i]);
            i += 1;
        }
    }

    // Pad to 49152 if needed
    out.resize(49152, 0);
    Ok(out)
}

/// Decompress a v2/v3 page block (ED ED compressed, fixed output size 16384).
fn decompress_page(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(16384);
    let mut i = 0;

    while i < data.len() && out.len() < 16384 {
        if i + 3 < data.len() && data[i] == 0xED && data[i + 1] == 0xED {
            let count = data[i + 2] as usize;
            let val = data[i + 3];
            for _ in 0..count {
                if out.len() < 16384 {
                    out.push(val);
                }
            }
            i += 4;
        } else {
            out.push(data[i]);
            i += 1;
        }
    }

    out.resize(16384, 0);
    Ok(out)
}

fn read_u16(data: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes([data[pos], data[pos + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompress_simple() {
        // ED ED 03 42 = repeat 0x42 three times
        let data = vec![0xED, 0xED, 0x03, 0x42];
        let out = decompress_page(&data).unwrap();
        assert_eq!(out[0], 0x42);
        assert_eq!(out[1], 0x42);
        assert_eq!(out[2], 0x42);
        assert_eq!(out[3], 0x00); // Padding
    }

    #[test]
    fn v1_header_parse() {
        // Minimal v1 header with PC != 0
        let mut data = vec![0u8; 30 + 49152];
        data[0] = 0x3E; // A
        data[1] = 0x00; // F
        data[6] = 0x00; // PC low
        data[7] = 0x80; // PC high = $8000 (non-zero = v1)
        data[12] = 0x00; // Byte 12: not compressed

        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.pc, 0x8000);
        assert_eq!(snap.model, SnapshotModel::Spectrum48K);
        assert_eq!(snap.pages.len(), 3);
    }
}
