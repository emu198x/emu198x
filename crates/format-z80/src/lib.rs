//! .Z80 snapshot loader for ZX Spectrum (v1, v2, v3 formats).
//!
//! Reference: <https://worldofspectrum.net/documentation/z80format.htm>
//!
//! **Version 1** (offset 6–7 PC ≠ 0): 30-byte header + one memory block.
//! 48K only.
//!
//! **Version 2/3** (offset 6–7 PC = 0): 30-byte base header + extended
//! header + page-based memory blocks. Supports 48K and 128K.

#![allow(clippy::cast_possible_truncation)]

pub use format_sna::{SnapshotTarget, Z80Registers};

/// Minimum size for a v1 header.
const V1_HEADER_SIZE: usize = 30;

/// Snapshot model detected from the hardware mode byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Z80SnapshotModel {
    Spectrum48K,
    Spectrum128K,
}

/// Load a .Z80 snapshot into the given target.
///
/// Detects the format version automatically.
///
/// # Errors
///
/// Returns an error if the data is too short, the format is unrecognised,
/// or decompression fails.
pub fn load_z80(target: &mut impl SnapshotTarget, data: &[u8]) -> Result<(), String> {
    if data.len() < V1_HEADER_SIZE {
        return Err(format!(
            "Z80 file too short: need at least {V1_HEADER_SIZE} bytes, got {}",
            data.len()
        ));
    }

    let version = detect_version(data);
    match version {
        1 => {
            load_v1(target, data);
            Ok(())
        }
        _ => load_v2v3(target, data, version),
    }
}

/// Detect the .Z80 format version.
fn detect_version(data: &[u8]) -> u8 {
    let pc = u16::from(data[6]) | (u16::from(data[7]) << 8);
    if pc != 0 {
        return 1;
    }

    if data.len() < 32 {
        return 2;
    }

    let ext_len = u16::from(data[30]) | (u16::from(data[31]) << 8);
    match ext_len {
        23 => 2,
        54 | 55 => 3,
        _ => 3,
    }
}

/// Load the base 30-byte header into a `Z80Registers` struct.
///
/// Returns `(registers, flags_byte_1)`.
fn load_base_header(data: &[u8]) -> (Z80Registers, u8) {
    let mut regs = Z80Registers::default();

    regs.a = data[0];
    regs.f = data[1];
    regs.c = data[2];
    regs.b = data[3];
    regs.l = data[4];
    regs.h = data[5];
    regs.sp = u16::from(data[8]) | (u16::from(data[9]) << 8);
    regs.i = data[10];

    let flags1 = if data[12] == 255 { 1 } else { data[12] };
    regs.r = (data[11] & 0x7F) | ((flags1 & 0x01) << 7);

    regs.e = data[13];
    regs.d = data[14];
    regs.c_alt = data[15];
    regs.b_alt = data[16];
    regs.e_alt = data[17];
    regs.d_alt = data[18];
    regs.l_alt = data[19];
    regs.h_alt = data[20];
    regs.a_alt = data[21];
    regs.f_alt = data[22];
    regs.iy = u16::from(data[23]) | (u16::from(data[24]) << 8);
    regs.ix = u16::from(data[25]) | (u16::from(data[26]) << 8);

    regs.iff1 = data[27] != 0;
    regs.iff2 = data[28] != 0;

    let flags2 = data[29];
    regs.im = flags2 & 0x03;

    (regs, flags1)
}

/// Load a version 1 .Z80 snapshot (48K only).
fn load_v1(target: &mut impl SnapshotTarget, data: &[u8]) {
    let (mut regs, flags1) = load_base_header(data);

    regs.pc = u16::from(data[6]) | (u16::from(data[7]) << 8);
    target.set_registers(&regs);

    let border = (flags1 >> 1) & 0x07;
    target.set_border(border);

    let compressed = flags1 & 0x20 != 0;
    let mem_data = &data[V1_HEADER_SIZE..];

    let mut ram = vec![0u8; 0xC000];

    if compressed {
        decompress_z80(mem_data, &mut ram);
    } else {
        let len = mem_data.len().min(ram.len());
        ram[..len].copy_from_slice(&mem_data[..len]);
    }

    for (i, &byte) in ram.iter().enumerate() {
        target.write_ram(0x4000u16 + i as u16, byte);
    }
}

/// Load a version 2 or 3 .Z80 snapshot.
fn load_v2v3(target: &mut impl SnapshotTarget, data: &[u8], _version: u8) -> Result<(), String> {
    let (mut regs, flags1) = load_base_header(data);

    if data.len() < 32 {
        return Err("Z80 v2/v3 file too short for extended header".to_string());
    }

    let ext_len = u16::from(data[30]) | (u16::from(data[31]) << 8);
    let ext_header_start = 32;
    let ext_header_end = ext_header_start + ext_len as usize;

    if data.len() < ext_header_end {
        return Err(format!(
            "Z80 file too short: extended header needs {ext_header_end} bytes"
        ));
    }

    let pc = u16::from(data[32]) | (u16::from(data[33]) << 8);
    regs.pc = pc;
    target.set_registers(&regs);

    let hw_mode = data[34];
    let port_7ffd = data[35];

    // AY register restore.
    let has_ay_data = ext_header_end >= 55;
    if has_ay_data {
        let ay_selected = data[38];
        for reg in 0..16u8 {
            target.set_ay_register(reg, data[39 + reg as usize]);
        }
        target.select_ay_register(ay_selected);
    }

    let border = (flags1 >> 1) & 0x07;
    target.set_border(border);

    let is_128k = is_128k_hardware(hw_mode, ext_len);

    if is_128k {
        target.write_bank_register(port_7ffd);
    }

    let mut pos = ext_header_end;
    while pos + 3 <= data.len() {
        let block_len = u16::from(data[pos]) | (u16::from(data[pos + 1]) << 8);
        let page = data[pos + 2];
        pos += 3;

        let (block_data, compressed) = if block_len == 0xFFFF {
            if pos + 0x4000 > data.len() {
                return Err(format!("Z80 uncompressed block at page {page} truncated"));
            }
            (&data[pos..pos + 0x4000], false)
        } else {
            let bl = block_len as usize;
            if pos + bl > data.len() {
                return Err(format!("Z80 compressed block at page {page} truncated"));
            }
            (&data[pos..pos + bl], true)
        };

        let mut page_ram = vec![0u8; 0x4000];
        if compressed {
            decompress_z80(block_data, &mut page_ram);
        } else {
            let len = block_data.len().min(0x4000);
            page_ram[..len].copy_from_slice(&block_data[..len]);
        }

        if is_128k {
            load_128k_page(target, page, &page_ram, port_7ffd);
        } else {
            load_48k_page(target, page, &page_ram);
        }

        if block_len == 0xFFFF {
            pos += 0x4000;
        } else {
            pos += block_len as usize;
        }
    }

    Ok(())
}

/// Determine if the hardware mode indicates a 128K machine.
fn is_128k_hardware(hw_mode: u8, ext_len: u16) -> bool {
    if ext_len == 23 {
        matches!(hw_mode, 3 | 4)
    } else {
        matches!(hw_mode, 4 | 5 | 6 | 7 | 12 | 13)
    }
}

/// Load a 48K page.
fn load_48k_page(target: &mut impl SnapshotTarget, page: u8, ram: &[u8]) {
    let base_addr: u16 = match page {
        4 => 0x8000,
        5 => 0xC000,
        8 => 0x4000,
        _ => return,
    };

    for (i, &byte) in ram.iter().enumerate() {
        target.write_ram(base_addr + i as u16, byte);
    }
}

/// Load a 128K page into the correct bank.
fn load_128k_page(target: &mut impl SnapshotTarget, page: u8, ram: &[u8], port_7ffd: u8) {
    let bank = match page {
        3 => 0,
        4 => 1,
        5 => 2,
        6 => 3,
        7 => 4,
        8 => 5,
        9 => 6,
        10 => 7,
        _ => return,
    };

    match bank {
        5 => {
            for (i, &byte) in ram.iter().enumerate() {
                target.write_ram(0x4000u16 + i as u16, byte);
            }
        }
        2 => {
            for (i, &byte) in ram.iter().enumerate() {
                target.write_ram(0x8000u16 + i as u16, byte);
            }
        }
        _ => {
            target.write_bank_register((port_7ffd & 0xF8) | bank);
            for (i, &byte) in ram.iter().enumerate() {
                target.write_ram(0xC000u16 + i as u16, byte);
            }
            target.write_bank_register(port_7ffd);
        }
    }
}

/// Decompress Z80-format RLE data.
///
/// Escape sequence: `ED ED xx yy` = repeat byte `yy` × `xx` times.
fn decompress_z80(src: &[u8], dst: &mut [u8]) {
    let mut si = 0;
    let mut di = 0;

    while si < src.len() && di < dst.len() {
        if si + 3 < src.len() && src[si] == 0xED && src[si + 1] == 0xED {
            let count = src[si + 2] as usize;
            let value = src[si + 3];
            for _ in 0..count {
                if di < dst.len() {
                    dst[di] = value;
                    di += 1;
                }
            }
            si += 4;
        } else {
            dst[di] = src[si];
            di += 1;
            si += 1;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Minimal snapshot target for testing.
    struct TestTarget {
        ram: [u8; 65536],
        border: u8,
        regs: Z80Registers,
        bank_reg: u8,
        bank_history: Vec<u8>,
        ay_regs: [u8; 16],
        ay_selected: u8,
    }

    impl TestTarget {
        fn new() -> Self {
            Self {
                ram: [0; 65536],
                border: 0,
                regs: Z80Registers::default(),
                bank_reg: 0,
                bank_history: Vec::new(),
                ay_regs: [0; 16],
                ay_selected: 0,
            }
        }
    }

    impl SnapshotTarget for TestTarget {
        fn set_registers(&mut self, regs: &Z80Registers) {
            self.regs = regs.clone();
        }
        fn write_ram(&mut self, addr: u16, val: u8) {
            self.ram[addr as usize] = val;
        }
        fn read_ram(&self, addr: u16) -> u8 {
            self.ram[addr as usize]
        }
        fn set_border(&mut self, colour: u8) {
            self.border = colour;
        }
        fn write_bank_register(&mut self, val: u8) {
            self.bank_reg = val;
            self.bank_history.push(val);
        }
        fn set_ay_register(&mut self, reg: u8, val: u8) {
            self.ay_regs[reg as usize] = val;
        }
        fn select_ay_register(&mut self, reg: u8) {
            self.ay_selected = reg;
        }
    }

    fn make_v1_uncompressed(pc: u16) -> Vec<u8> {
        let mut data = vec![0u8; V1_HEADER_SIZE + 0xC000];
        data[0] = 0xAA; // A
        data[1] = 0xFF; // F
        data[2] = 0x34; // C
        data[3] = 0x12; // B
        data[4] = 0x78; // L
        data[5] = 0x56; // H
        data[6] = pc as u8;
        data[7] = (pc >> 8) as u8;
        data[8] = 0x00; // SP low
        data[9] = 0x80; // SP high
        data[10] = 0x3F; // I
        data[11] = 0x42; // R
        data[12] = 0x04; // Flags1: border=2, uncompressed
        data[27] = 1; // IFF1
        data[28] = 1; // IFF2
        data[29] = 1; // IM=1

        data[V1_HEADER_SIZE] = 0x55;
        data[V1_HEADER_SIZE + 0x4000] = 0x88;
        data[V1_HEADER_SIZE + 0x8000] = 0xCC;

        data
    }

    #[test]
    fn v1_uncompressed_sets_registers() {
        let mut target = TestTarget::new();
        let z80_data = make_v1_uncompressed(0xABCD);
        load_z80(&mut target, &z80_data).expect("load should succeed");
        assert_eq!(target.regs.a, 0xAA);
        assert_eq!(target.regs.f, 0xFF);
        assert_eq!(target.regs.b, 0x12);
        assert_eq!(target.regs.c, 0x34);
        assert_eq!(target.regs.h, 0x56);
        assert_eq!(target.regs.l, 0x78);
        assert_eq!(target.regs.pc, 0xABCD);
        assert_eq!(target.regs.sp, 0x8000);
        assert_eq!(target.regs.i, 0x3F);
        assert_eq!(target.regs.im, 1);
        assert!(target.regs.iff1);
    }

    #[test]
    fn v1_uncompressed_loads_memory() {
        let mut target = TestTarget::new();
        let z80_data = make_v1_uncompressed(0x0100);
        load_z80(&mut target, &z80_data).expect("load should succeed");
        assert_eq!(target.ram[0x4000], 0x55);
        assert_eq!(target.ram[0x8000], 0x88);
        assert_eq!(target.ram[0xC000], 0xCC);
    }

    #[test]
    fn v1_uncompressed_sets_border() {
        let mut target = TestTarget::new();
        let z80_data = make_v1_uncompressed(0x0100);
        load_z80(&mut target, &z80_data).expect("load should succeed");
        assert_eq!(target.border, 2);
    }

    #[test]
    fn v1_compressed_loads_correctly() {
        let mut target = TestTarget::new();

        let mut header = vec![0u8; V1_HEADER_SIZE];
        header[6] = 0x00;
        header[7] = 0x01; // PC=0x0100
        header[8] = 0x00;
        header[9] = 0x80; // SP=0x8000
        header[12] = 0x26; // compressed, border=3

        let mut body: Vec<u8> = Vec::new();
        body.extend_from_slice(&[0xED, 0xED, 10, 0xAA]);
        body.push(0x55);
        body.push(0x66);

        let mut data = header;
        data.extend_from_slice(&body);

        load_z80(&mut target, &data).expect("load should succeed");

        for i in 0..10 {
            assert_eq!(target.ram[0x4000 + i], 0xAA);
        }
        assert_eq!(target.ram[0x400A], 0x55);
        assert_eq!(target.ram[0x400B], 0x66);
        assert_eq!(target.border, 3);
    }

    #[test]
    fn truncated_data_returns_error() {
        let mut target = TestTarget::new();
        let result = load_z80(&mut target, &[0u8; 10]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too short"));
    }

    #[test]
    fn decompress_z80_rle() {
        let src = [0xED, 0xED, 5, 0xAA, 0x11, 0x22];
        let mut dst = [0u8; 8];
        decompress_z80(&src, &mut dst);
        assert_eq!(&dst[..7], &[0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0x11, 0x22]);
    }

    #[test]
    fn decompress_z80_literal_ed() {
        let src = [0xED, 0x55, 0x66];
        let mut dst = [0u8; 3];
        decompress_z80(&src, &mut dst);
        assert_eq!(dst, [0xED, 0x55, 0x66]);
    }

    #[test]
    fn detect_version_distinguishes_v1_v2_and_v3_headers() {
        let mut v1 = vec![0u8; V1_HEADER_SIZE];
        v1[6] = 0x34;
        v1[7] = 0x12;
        assert_eq!(detect_version(&v1), 1);

        let mut v2 = vec![0u8; 32];
        v2[30] = 23;
        assert_eq!(detect_version(&v2), 2);

        let mut v3 = vec![0u8; 32];
        v3[30] = 54;
        assert_eq!(detect_version(&v3), 3);
    }

    #[test]
    fn load_base_header_maps_r_high_bit_and_interrupt_flags() {
        let mut data = vec![0u8; V1_HEADER_SIZE];
        data[0] = 0xAA;
        data[1] = 0x55;
        data[8] = 0x34;
        data[9] = 0x12;
        data[10] = 0x77;
        data[11] = 0x42;
        data[12] = 0xFF; // treated as 1, restoring R bit 7
        data[27] = 1;
        data[28] = 0;
        data[29] = 0x02;

        let (regs, flags1) = load_base_header(&data);

        assert_eq!(flags1, 1);
        assert_eq!(regs.a, 0xAA);
        assert_eq!(regs.f, 0x55);
        assert_eq!(regs.sp, 0x1234);
        assert_eq!(regs.i, 0x77);
        assert_eq!(regs.r, 0xC2);
        assert!(regs.iff1);
        assert!(!regs.iff2);
        assert_eq!(regs.im, 0x02);
    }

    #[test]
    fn v2_128k_load_restores_bank_and_ay_state() {
        let mut target = TestTarget::new();
        let mut data = vec![0u8; 55];

        // Base header: PC=0 forces v2/v3 path.
        data[12] = 0x0E; // border = 7
        data[30] = 23;
        data[31] = 0;
        data[32] = 0x34;
        data[33] = 0x12; // PC = 0x1234
        data[34] = 3; // v2 128K hardware
        data[35] = 0x13; // port 7FFD, bank 3 paged
        data[38] = 7; // selected AY register
        for reg in 0..16u8 {
            data[39 + reg as usize] = reg.wrapping_mul(3);
        }

        // Uncompressed page 8 -> bank 5 -> $4000
        data.extend_from_slice(&[0xFF, 0xFF, 8]);
        let mut page8 = vec![0u8; 0x4000];
        page8[0] = 0x11;
        data.extend_from_slice(&page8);

        // Uncompressed page 5 -> bank 2 -> $8000
        data.extend_from_slice(&[0xFF, 0xFF, 5]);
        let mut page5 = vec![0u8; 0x4000];
        page5[0] = 0x22;
        data.extend_from_slice(&page5);

        // Uncompressed page 7 -> bank 4 -> temporary page at $C000, then restore
        data.extend_from_slice(&[0xFF, 0xFF, 7]);
        let mut page7 = vec![0u8; 0x4000];
        page7[0] = 0x33;
        data.extend_from_slice(&page7);

        load_z80(&mut target, &data).expect("load should succeed");

        assert_eq!(target.regs.pc, 0x1234);
        assert_eq!(target.border, 7);
        assert_eq!(target.ram[0x4000], 0x11);
        assert_eq!(target.ram[0x8000], 0x22);
        assert_eq!(target.ram[0xC000], 0x33);
        assert_eq!(target.bank_reg, 0x13);
        assert!(target.bank_history.contains(&0x14));
        assert_eq!(target.bank_history.last(), Some(&0x13));
        assert_eq!(target.ay_selected, 7);
        assert_eq!(target.ay_regs[5], 15);
        assert_eq!(target.ay_regs[15], 45);
    }

    #[test]
    fn v2_48k_load_does_not_restore_bank_or_ay_state() {
        let mut target = TestTarget::new();
        let mut data = vec![0u8; 55];

        data[12] = 0x04; // border = 2
        data[30] = 23;
        data[31] = 0;
        data[32] = 0x78;
        data[33] = 0x56;
        data[34] = 0; // 48K hardware
        data[35] = 0x17; // should be ignored for 48K

        data.extend_from_slice(&[0xFF, 0xFF, 8]);
        let mut page8 = vec![0u8; 0x4000];
        page8[0] = 0x11;
        data.extend_from_slice(&page8);

        data.extend_from_slice(&[0xFF, 0xFF, 4]);
        let mut page4 = vec![0u8; 0x4000];
        page4[0] = 0x22;
        data.extend_from_slice(&page4);

        data.extend_from_slice(&[0xFF, 0xFF, 5]);
        let mut page5 = vec![0u8; 0x4000];
        page5[0] = 0x33;
        data.extend_from_slice(&page5);

        load_z80(&mut target, &data).expect("load should succeed");

        assert_eq!(target.regs.pc, 0x5678);
        assert_eq!(target.border, 2);
        assert_eq!(target.ram[0x4000], 0x11);
        assert_eq!(target.ram[0x8000], 0x22);
        assert_eq!(target.ram[0xC000], 0x33);
        assert_eq!(target.bank_reg, 0);
        assert!(target.bank_history.is_empty());
        assert_eq!(target.ay_selected, 0);
        assert_eq!(target.ay_regs, [0; 16]);
    }

    #[test]
    fn v3_128k_mode_uses_v3_hardware_mapping() {
        let mut target = TestTarget::new();
        let mut data = vec![0u8; 86];

        data[12] = 0x06; // border = 3
        data[30] = 54;
        data[31] = 0;
        data[32] = 0xEF;
        data[33] = 0xBE;
        data[34] = 12; // v3 128K-compatible hardware
        data[35] = 0x16; // bank 6 paged
        data[38] = 5;
        for reg in 0..16u8 {
            data[39 + reg as usize] = reg.wrapping_add(1);
        }

        data.extend_from_slice(&[0xFF, 0xFF, 10]);
        let mut page10 = vec![0u8; 0x4000];
        page10[0] = 0x44;
        data.extend_from_slice(&page10);

        load_z80(&mut target, &data).expect("load should succeed");

        assert_eq!(target.regs.pc, 0xBEEF);
        assert_eq!(target.border, 3);
        assert_eq!(target.ram[0xC000], 0x44);
        assert_eq!(target.bank_reg, 0x16);
        assert!(target.bank_history.contains(&0x17));
        assert_eq!(target.bank_history.last(), Some(&0x16));
        assert_eq!(target.ay_selected, 5);
        assert_eq!(target.ay_regs[0], 1);
        assert_eq!(target.ay_regs[15], 16);
    }

    #[test]
    fn v2_extended_header_truncation_returns_error() {
        let mut target = TestTarget::new();
        let mut data = vec![0u8; 40];
        data[30] = 23;
        data[31] = 0;

        let result = load_z80(&mut target, &data);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("extended header"));
    }

    #[test]
    fn v2_compressed_block_truncation_returns_error() {
        let mut target = TestTarget::new();
        let mut data = vec![0u8; 55];
        data[30] = 23;
        data[31] = 0;
        data[34] = 0; // 48K hardware, so page 8 is valid
        data.extend_from_slice(&[0x05, 0x00, 8, 0xAA, 0xBB]);

        let result = load_z80(&mut target, &data);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("compressed block at page 8 truncated")
        );
    }

    #[test]
    fn v2_uncompressed_block_truncation_returns_error() {
        let mut target = TestTarget::new();
        let mut data = vec![0u8; 55];
        data[30] = 23;
        data[31] = 0;
        data[34] = 0; // 48K hardware, so page 8 is valid
        data.extend_from_slice(&[0xFF, 0xFF, 8]);
        data.extend_from_slice(&[0xAA, 0xBB]);

        let result = load_z80(&mut target, &data);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("uncompressed block at page 8 truncated")
        );
    }
}
