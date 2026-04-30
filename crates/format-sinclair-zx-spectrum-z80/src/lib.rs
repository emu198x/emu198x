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

    // Version 2 or 3: extended header. The two read_u16 calls below
    // touch bytes at offsets 30, 31, 32, and 33 — so we need at least
    // 34 bytes, not 32. Without this larger guard a 32- or 33-byte
    // file panics in `read_u16(data, 32)`.
    if data.len() < 34 {
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

    // Byte 86 of the file = byte 54 of the extension (0-indexed). It
    // is only present when the extension is at least 55 bytes long
    // (the +3 disk-system port_1ffd byte). The original `>= 54` guard
    // accessed data[86] for a 54-byte extension, which only stretches
    // through data[85] — a one-byte over-read.
    let port_1ffd = if ext_len >= 55 { data[86] } else { 0 };

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

    /// Build a minimal valid v1 header (30 bytes) with default fields.
    fn v1_header() -> Vec<u8> {
        let mut h = vec![0u8; 30];
        h[6] = 0x00; // PC low
        h[7] = 0x80; // PC high = $8000 (non-zero = v1)
        h
    }

    /// Build a minimal v2/v3 header. `ext_len` is 23 (v2) or 55 (v3).
    /// The buffer is sized to `32 + ext_len`, which is what the parser
    /// validates against. Note: the parser only reads `data[86]`
    /// (the `+3` `port_1ffd` byte) when `ext_len >= 55`, so an
    /// `ext_len = 54` buffer of 86 bytes is also safe.
    fn v2_header(ext_len: u16, hw_mode: u8) -> Vec<u8> {
        let mut h = vec![0u8; 32 + ext_len as usize];
        // PC at offset 6 = 0 marks v2/v3
        h[6] = 0;
        h[7] = 0;
        // ext_len at offset 30 (LE)
        h[30] = (ext_len & 0xFF) as u8;
        h[31] = (ext_len >> 8) as u8;
        // Hardware mode at offset 34
        h[34] = hw_mode;
        h
    }

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

    // ---------------- header v1 error/edge ----------------

    #[test]
    fn parse_z80_rejects_short_buffer() {
        let data = vec![0u8; 29];
        let err = parse_z80(&data).unwrap_err();
        assert!(err.contains("too short"));
    }

    #[test]
    fn v1_header_decodes_all_register_fields() {
        let mut data = vec![0u8; 30 + 49152];
        // A=0x12, F=0x34 (big-endian in file)
        data[0] = 0x12;
        data[1] = 0x34;
        // BC=0x5678 (LE)
        data[2] = 0x78;
        data[3] = 0x56;
        // HL=0x9ABC
        data[4] = 0xBC;
        data[5] = 0x9A;
        // PC=0x1000 (non-zero → v1)
        data[6] = 0x00;
        data[7] = 0x10;
        // SP=0x2000
        data[8] = 0x00;
        data[9] = 0x20;
        // I=0x3F
        data[10] = 0x3F;
        // R low 7 bits = 0x55
        data[11] = 0x55;
        // Byte 12: bit0 → R bit7, bits1-3 → border, bit5 → compressed
        // border=4 (bits1-3 = 100), R bit7 = 1, compressed=0
        data[12] = 0b0000_1001;
        // DE=0xDEAD
        data[13] = 0xAD;
        data[14] = 0xDE;
        // BC'=0xBEEF, DE'=0xCAFE, HL'=0x1234
        data[15] = 0xEF;
        data[16] = 0xBE;
        data[17] = 0xFE;
        data[18] = 0xCA;
        data[19] = 0x34;
        data[20] = 0x12;
        // A'=0x77, F'=0x88 (big-endian)
        data[21] = 0x77;
        data[22] = 0x88;
        // IY=0x4444, IX=0x5555
        data[23] = 0x44;
        data[24] = 0x44;
        data[25] = 0x55;
        data[26] = 0x55;
        // IFF1=1, IFF2=0
        data[27] = 0x01;
        data[28] = 0x00;
        // IM=2
        data[29] = 0x02;

        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.af, 0x1234);
        assert_eq!(snap.bc, 0x5678);
        assert_eq!(snap.hl, 0x9ABC);
        assert_eq!(snap.pc, 0x1000);
        assert_eq!(snap.sp, 0x2000);
        assert_eq!(snap.i, 0x3F);
        assert_eq!(snap.r, 0x55 | 0x80);
        assert_eq!(snap.border, 4);
        assert_eq!(snap.de, 0xDEAD);
        assert_eq!(snap.bc_alt, 0xBEEF);
        assert_eq!(snap.de_alt, 0xCAFE);
        assert_eq!(snap.hl_alt, 0x1234);
        assert_eq!(snap.af_alt, 0x7788);
        assert_eq!(snap.iy, 0x4444);
        assert_eq!(snap.ix, 0x5555);
        assert!(snap.iff1);
        assert!(!snap.iff2);
        assert_eq!(snap.im, 2);
        assert_eq!(snap.model, SnapshotModel::Spectrum48K);
        // pages 8/4/5 in that order
        assert_eq!(snap.pages[0].0, 8);
        assert_eq!(snap.pages[1].0, 4);
        assert_eq!(snap.pages[2].0, 5);
    }

    #[test]
    fn v1_byte12_0xff_is_remapped_to_one() {
        // Per format spec: byte 12 == 0xFF must be treated as 1.
        let mut data = v1_header();
        data[12] = 0xFF;
        data.extend(vec![0u8; 49152]);
        let snap = parse_z80(&data).unwrap();
        // byte=1: bit0 -> R bit7 = 1; bits1-3 -> border = 0; bit5 -> compressed = 0
        assert_eq!(snap.r, 0x80);
        assert_eq!(snap.border, 0);
    }

    #[test]
    fn v1_uncompressed_too_short_returns_error() {
        let mut data = v1_header();
        // Only 100 bytes of body — well short of 49152.
        data.extend(vec![0u8; 100]);
        let err = parse_z80(&data).unwrap_err();
        assert!(err.contains("too short"));
    }

    #[test]
    fn v1_compressed_block_decompresses() {
        // Build a v1 with the "compressed" flag set, body = a single ED ED RLE
        // that runs the buffer to 49152 bytes via padding.
        let mut data = v1_header();
        data[12] = 0x20; // compressed flag (bit5)
        // body: ED ED 04 AA  then end marker 00 ED ED 00
        data.extend(vec![0xED, 0xED, 0x04, 0xAA, 0x00, 0xED, 0xED, 0x00]);
        let snap = parse_z80(&data).unwrap();
        // First four bytes of $4000-$7FFF (page 8) come from the RLE.
        let page8 = &snap.pages[0].1;
        assert_eq!(&page8[..4], &[0xAA, 0xAA, 0xAA, 0xAA]);
        // Padding fills the rest.
        assert_eq!(page8[5], 0x00);
    }

    #[test]
    fn v1_compressed_literal_byte_passes_through() {
        let mut data = v1_header();
        data[12] = 0x20;
        // Two literal bytes then end marker.
        data.extend(vec![0x12, 0x34, 0x00, 0xED, 0xED, 0x00]);
        let snap = parse_z80(&data).unwrap();
        let page8 = &snap.pages[0].1;
        assert_eq!(page8[0], 0x12);
        assert_eq!(page8[1], 0x34);
    }

    // ---------------- header v2/v3 ----------------

    #[test]
    fn v2_header_too_short_for_extension_returns_error() {
        // PC=0 marks v2/v3, but buffer is only 31 bytes → can't read ext_len.
        let mut data = vec![0u8; 31];
        data[6] = 0;
        data[7] = 0;
        let err = parse_z80(&data).unwrap_err();
        assert!(err.contains("v2/v3 header"));
    }

    #[test]
    fn v2_header_too_short_for_full_extension_returns_error() {
        // ext_len=23 demands at least 55 bytes; 34 is enough to read PC
        // but short of the extension itself.
        let mut data = vec![0u8; 34];
        data[30] = 23;
        let err = parse_z80(&data).unwrap_err();
        assert!(err.contains("extended header"));
    }

    /// Regression for the `data.len() < 32` over-read fixed alongside
    /// this test: a 32-byte buffer marked as v2/v3 (PC=0) used to
    /// panic in `read_u16(data, 32)` because the early-exit guard
    /// permitted any buffer ≥ 32 bytes through, while the read needs
    /// 34 bytes. After the fix, the function returns a clean
    /// "v2/v3 header" error.
    #[test]
    fn v2_header_32_byte_buffer_returns_error_not_panic() {
        let mut data = vec![0u8; 32];
        data[6] = 0;
        data[7] = 0;
        let err = parse_z80(&data).unwrap_err();
        assert!(err.contains("v2/v3 header"), "got {err:?}");
    }

    /// Regression for the `data[86]` over-read fixed alongside this
    /// test: a v3 file declaring `ext_len = 54` (the "v3 without
    /// port_1ffd" variant) used to crash because the parser
    /// unconditionally accessed `data[86]` whenever `ext_len >= 54`,
    /// but a 54-byte extension only spans bytes 32..=85 (file size
    /// 86). The fix gates the access on `ext_len >= 55`, leaving
    /// `port_1ffd` defaulted to 0 for the 54-byte form.
    #[test]
    fn v3_header_54_byte_extension_does_not_read_past_buffer() {
        let data = v2_header(54, 0);
        assert_eq!(data.len(), 86, "test fixture sanity");
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.port_1ffd, 0);
    }

    #[test]
    fn v2_header_23_byte_extension_parses_48k_default() {
        // ext_len = 23, hw_mode = 0 → 48K
        let data = v2_header(23, 0);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::Spectrum48K);
        // ay_register stays 0 (ext_len < 25), ay_regs all zeros.
        assert_eq!(snap.ay_register, 0);
        assert_eq!(snap.ay_regs, [0; 16]);
        assert_eq!(snap.port_1ffd, 0);
        assert_eq!(snap.pages.len(), 0); // No memory pages appended
    }

    #[test]
    fn v2_hw_mode_1_is_48k_with_if1() {
        let data = v2_header(23, 1);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::Spectrum48K);
    }

    #[test]
    fn v2_hw_mode_2_samram_treated_as_48k() {
        let data = v2_header(23, 2);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::Spectrum48K);
    }

    #[test]
    fn v2_hw_mode_3_is_128k() {
        let data = v2_header(23, 3);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::Spectrum128K);
    }

    #[test]
    fn v2_hw_mode_4_is_128k_with_if1() {
        let data = v2_header(23, 4);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::Spectrum128K);
    }

    #[test]
    fn v2_hw_mode_unknown_falls_back_to_48k() {
        let data = v2_header(23, 99);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::Spectrum48K);
    }

    #[test]
    fn v3_hw_mode_5_is_plus2_returning_128k_alias() {
        let data = v2_header(55, 5);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::Spectrum128K);
    }

    #[test]
    fn v3_hw_mode_6_is_plus2a() {
        let data = v2_header(55, 6);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::SpectrumPlus2A);
    }

    #[test]
    fn v3_hw_mode_7_is_plus2a() {
        let data = v2_header(55, 7);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::SpectrumPlus2A);
    }

    #[test]
    fn v3_hw_mode_9_is_pentagon() {
        let data = v2_header(55, 9);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::Pentagon128);
    }

    #[test]
    fn v3_hw_mode_10_is_scorpion() {
        let data = v2_header(55, 10);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::Scorpion256);
    }

    #[test]
    fn v3_hw_mode_12_is_plus2() {
        let data = v2_header(55, 12);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::SpectrumPlus2);
    }

    #[test]
    fn v3_hw_mode_13_is_plus2a() {
        let data = v2_header(55, 13);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.model, SnapshotModel::SpectrumPlus2A);
    }

    #[test]
    fn v2_extension_25_bytes_reads_ay_register() {
        // ext_len=25 → ay_register at offset 38 is decoded; ay_regs not.
        let mut data = v2_header(25, 0);
        data[38] = 0x07;
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.ay_register, 0x07);
        assert_eq!(snap.ay_regs, [0; 16]);
    }

    #[test]
    fn v3_extension_55_bytes_reads_ay_regs_and_1ffd() {
        // ext_len = 55 (real-world v3 extension size). Both AY regs and
        // port_1ffd are decoded; pc and 7ffd live at their fixed offsets.
        let mut data = v2_header(55, 3);
        // PC at offset 32
        data[32] = 0x34;
        data[33] = 0x12;
        // port_7ffd at offset 35
        data[35] = 0x10;
        // ay_register
        data[38] = 0x05;
        // 16 ay_regs at 39..55
        for i in 0..16 {
            data[39 + i] = (i as u8) ^ 0xA5;
        }
        // port_1ffd at offset 86
        data[86] = 0xC3;

        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.pc, 0x1234);
        assert_eq!(snap.port_7ffd, 0x10);
        assert_eq!(snap.ay_register, 0x05);
        for i in 0..16 {
            assert_eq!(snap.ay_regs[i], (i as u8) ^ 0xA5);
        }
        assert_eq!(snap.port_1ffd, 0xC3);
    }

    // ---------------- v2/v3 page parsing ----------------

    #[test]
    fn v2_uncompressed_page_block_loads_full_16k() {
        // ext_len=23, then one block: len=0xFFFF (uncompressed flag), page=8
        let mut data = v2_header(23, 0);
        data.push(0xFF); // block_len lo
        data.push(0xFF); // block_len hi (== 0xFFFF)
        data.push(8); // page_num
        let mut payload = vec![0u8; 16384];
        payload[0] = 0x42;
        payload[16383] = 0xC3;
        data.extend(&payload);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.pages.len(), 1);
        assert_eq!(snap.pages[0].0, 8);
        assert_eq!(snap.pages[0].1[0], 0x42);
        assert_eq!(snap.pages[0].1[16383], 0xC3);
    }

    #[test]
    fn v2_uncompressed_page_truncated_breaks_out_cleanly() {
        // Block claims uncompressed (0xFFFF) but body is short — loop must
        // break, leaving zero pages, and parse must still succeed.
        let mut data = v2_header(23, 0);
        data.push(0xFF);
        data.push(0xFF);
        data.push(8);
        // Only 100 bytes of payload, not 16384.
        data.extend(vec![0u8; 100]);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.pages.len(), 0);
    }

    #[test]
    fn v2_compressed_page_block_decompresses() {
        // ext_len=23, then a compressed block.
        let mut data = v2_header(23, 0);
        // RLE: ED ED 03 0xAA — repeat 0xAA three times
        let body = vec![0xED, 0xED, 0x03, 0xAA];
        data.push(body.len() as u8);
        data.push(0);
        data.push(8);
        data.extend(&body);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.pages.len(), 1);
        let page = &snap.pages[0].1;
        assert_eq!(page[0], 0xAA);
        assert_eq!(page[1], 0xAA);
        assert_eq!(page[2], 0xAA);
        assert_eq!(page[3], 0x00); // padding
    }

    #[test]
    fn v2_compressed_page_truncated_breaks_out_cleanly() {
        // Block_len says 200 but file ends after only 50 bytes of payload.
        let mut data = v2_header(23, 0);
        data.push(200);
        data.push(0);
        data.push(8);
        data.extend(vec![0u8; 50]);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.pages.len(), 0);
    }

    #[test]
    fn v2_two_compressed_pages_load_in_order() {
        let mut data = v2_header(23, 0);
        // First block: page 8 — RLE 0x11 four times
        let b1 = vec![0xED, 0xED, 0x04, 0x11];
        data.push(b1.len() as u8);
        data.push(0);
        data.push(8);
        data.extend(&b1);
        // Second block: page 5 — single literal then RLE
        let b2 = vec![0x99, 0xED, 0xED, 0x02, 0x88];
        data.push(b2.len() as u8);
        data.push(0);
        data.push(5);
        data.extend(&b2);
        let snap = parse_z80(&data).unwrap();
        assert_eq!(snap.pages.len(), 2);
        assert_eq!(snap.pages[0].0, 8);
        assert_eq!(snap.pages[0].1[0], 0x11);
        assert_eq!(snap.pages[0].1[3], 0x11);
        assert_eq!(snap.pages[1].0, 5);
        assert_eq!(snap.pages[1].1[0], 0x99);
        assert_eq!(snap.pages[1].1[1], 0x88);
        assert_eq!(snap.pages[1].1[2], 0x88);
    }

    // ---------------- decompression edges ----------------

    #[test]
    fn decompress_page_literal_only_pads_to_16k() {
        let data = vec![0x12, 0x34, 0x56];
        let out = decompress_page(&data).unwrap();
        assert_eq!(out.len(), 16384);
        assert_eq!(out[0], 0x12);
        assert_eq!(out[1], 0x34);
        assert_eq!(out[2], 0x56);
        assert_eq!(out[3], 0x00);
    }

    #[test]
    fn decompress_page_caps_output_at_16k() {
        // Repeated maximal RLE blocks would produce more than 16384 bytes
        // if not capped. The function must clamp the inner push loop and
        // the output length both at 16384.
        let mut data = Vec::new();
        // 100 RLE blocks × 255 bytes = 25500 bytes' worth of writes.
        for _ in 0..100 {
            data.extend_from_slice(&[0xED, 0xED, 0xFF, 0x42]);
        }
        let out = decompress_page(&data).unwrap();
        assert_eq!(out.len(), 16384);
        assert_eq!(out[0], 0x42);
        assert_eq!(out[16383], 0x42);
    }

    #[test]
    fn decompress_page_marker_at_eof_passes_through_as_literal() {
        // ED ED at the very end without count/value bytes — the i+3<len
        // guard fails, so each byte is taken literally.
        let data = vec![0xED, 0xED];
        let out = decompress_page(&data).unwrap();
        assert_eq!(out[0], 0xED);
        assert_eq!(out[1], 0xED);
    }

    #[test]
    fn decompress_v1_recognises_end_marker_and_pads() {
        // 00 ED ED 00 = end marker; rest of buffer fills with zeros.
        let data = vec![0x12, 0x34, 0x00, 0xED, 0xED, 0x00, 0x99, 0x99];
        let out = decompress_v1(&data).unwrap();
        assert_eq!(out.len(), 49152);
        assert_eq!(out[0], 0x12);
        assert_eq!(out[1], 0x34);
        // After end marker the rest is zero-padded.
        assert_eq!(out[2], 0x00);
        assert_eq!(out[3], 0x00);
    }

    #[test]
    fn decompress_v1_rle_block_then_literal() {
        let data = vec![
            0xED, 0xED, 0x05, 0x77, // RLE x5
            0x88, // literal
            0x00, 0xED, 0xED, 0x00, // end marker
        ];
        let out = decompress_v1(&data).unwrap();
        assert_eq!(&out[..6], &[0x77, 0x77, 0x77, 0x77, 0x77, 0x88]);
    }
}
