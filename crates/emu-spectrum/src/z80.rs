//! .Z80 snapshot loader for ZX Spectrum (v1, v2, v3 formats).
//!
//! Reference: docs/Reference/docs/formats/spectrum.md lines 355–519.
//!
//! **Version 1** (offset 6–7 PC ≠ 0): 30-byte header + one memory block.
//! 48K only.
//!
//! **Version 2/3** (offset 6–7 PC = 0): 30-byte base header + extended
//! header + page-based memory blocks. Supports 48K and 128K.

#![allow(clippy::cast_possible_truncation)]

use emu_core::Cpu;

use crate::Spectrum;

/// Minimum size for a v1 header.
const V1_HEADER_SIZE: usize = 30;

/// Load a .Z80 snapshot into the given Spectrum instance.
///
/// Detects the format version automatically. The Spectrum must already be
/// created with the correct model — 128K snapshots require a 128K model.
///
/// # Errors
///
/// Returns an error if the data is too short, the format is unrecognised,
/// or decompression fails.
pub fn load_z80(spectrum: &mut Spectrum, data: &[u8]) -> Result<(), String> {
    if data.len() < V1_HEADER_SIZE {
        return Err(format!(
            "Z80 file too short: need at least {V1_HEADER_SIZE} bytes, got {}",
            data.len()
        ));
    }

    let version = detect_version(data);
    match version {
        1 => load_v1(spectrum, data),
        _ => load_v2v3(spectrum, data, version),
    }
}

/// Detect the .Z80 format version.
fn detect_version(data: &[u8]) -> u8 {
    let pc = u16::from(data[6]) | (u16::from(data[7]) << 8);
    if pc != 0 {
        return 1;
    }

    if data.len() < 32 {
        return 2; // Fallback: treat as v2
    }

    let ext_len = u16::from(data[30]) | (u16::from(data[31]) << 8);
    match ext_len {
        23 => 2,
        54 | 55 => 3,
        _ => 3, // Unknown extension length — assume v3
    }
}

/// Load the base 30-byte header into CPU registers.
///
/// Returns the flags byte 1 (offset 12) for the caller to extract
/// compression and border info.
fn load_base_header(spectrum: &mut Spectrum, data: &[u8]) -> u8 {
    spectrum.cpu_mut().reset();

    let cpu = spectrum.cpu_mut();
    let regs = &mut cpu.regs;

    regs.a = data[0];
    regs.f = data[1];
    regs.c = data[2];
    regs.b = data[3];
    regs.l = data[4];
    regs.h = data[5];
    // PC at offsets 6–7 handled by caller (v1 reads it here, v2/v3 from ext header)
    regs.sp = u16::from(data[8]) | (u16::from(data[9]) << 8);
    regs.i = data[10];

    // R register: low 7 bits from offset 11, bit 7 from flags byte 1 bit 0.
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

    flags1
}

/// Load a version 1 .Z80 snapshot (48K only).
fn load_v1(spectrum: &mut Spectrum, data: &[u8]) -> Result<(), String> {
    let flags1 = load_base_header(spectrum, data);

    let pc = u16::from(data[6]) | (u16::from(data[7]) << 8);
    spectrum.cpu_mut().regs.pc = pc;

    // Border colour from flags byte 1 bits 1–3.
    let border = (flags1 >> 1) & 0x07;
    spectrum.bus_mut().ula.set_border_colour(border);

    let compressed = flags1 & 0x20 != 0;
    let mem_data = &data[V1_HEADER_SIZE..];

    let mut ram = vec![0u8; 0xC000]; // 48K: $4000-$FFFF

    if compressed {
        decompress_z80(mem_data, &mut ram)?;
    } else {
        let len = mem_data.len().min(ram.len());
        ram[..len].copy_from_slice(&mem_data[..len]);
    }

    // Write RAM through the memory interface.
    let bus = spectrum.bus_mut();
    for (i, &byte) in ram.iter().enumerate() {
        bus.memory.write(0x4000u16 + i as u16, byte);
    }

    Ok(())
}

/// Load a version 2 or 3 .Z80 snapshot.
fn load_v2v3(spectrum: &mut Spectrum, data: &[u8], _version: u8) -> Result<(), String> {
    let flags1 = load_base_header(spectrum, data);

    if data.len() < 32 {
        return Err("Z80 v2/v3 file too short for extended header".to_string());
    }

    let ext_len = u16::from(data[30]) | (u16::from(data[31]) << 8);
    let ext_header_start = 32;
    let ext_header_end = ext_header_start + ext_len as usize;

    if data.len() < ext_header_end {
        return Err(format!(
            "Z80 file too short: extended header needs {} bytes",
            ext_header_end
        ));
    }

    // PC from extended header.
    let pc = u16::from(data[32]) | (u16::from(data[33]) << 8);
    spectrum.cpu_mut().regs.pc = pc;

    // Hardware mode at offset 34.
    let hw_mode = data[34];

    // Port $7FFD at offset 35 (128K bank register).
    let port_7ffd = data[35];

    // AY register at offset 38 (selected register).
    // AY register contents at offsets 39–54 (16 bytes).
    let has_ay_data = ext_header_end >= 55;
    if has_ay_data {
        let ay_selected = data[38];
        if let Some(ay) = &mut spectrum.bus_mut().ay {
            // Restore all 16 AY registers.
            for reg in 0..16u8 {
                ay.select_register(reg);
                ay.write_data(data[39 + reg as usize]);
            }
            // Restore the selected register.
            ay.select_register(ay_selected);
        }
    }

    // Border colour from flags byte 1 bits 1–3.
    let border = (flags1 >> 1) & 0x07;
    spectrum.bus_mut().ula.set_border_colour(border);

    // Determine if this is a 128K snapshot based on hardware mode.
    let is_128k = is_128k_hardware(hw_mode, ext_len);

    if is_128k {
        // Set the bank register before loading pages.
        spectrum.bus_mut().memory.write_bank_register(port_7ffd);
    }

    // Read memory blocks.
    let mut pos = ext_header_end;
    while pos + 3 <= data.len() {
        let block_len = u16::from(data[pos]) | (u16::from(data[pos + 1]) << 8);
        let page = data[pos + 2];
        pos += 3;

        let (block_data, compressed) = if block_len == 0xFFFF {
            // Uncompressed: 16K raw
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
            decompress_z80(block_data, &mut page_ram)?;
        } else {
            let len = block_data.len().min(0x4000);
            page_ram[..len].copy_from_slice(&block_data[..len]);
        }

        // Map page number to address or bank.
        if is_128k {
            load_128k_page(spectrum, page, &page_ram, port_7ffd)?;
        } else {
            load_48k_page(spectrum, page, &page_ram)?;
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
        // Version 2
        matches!(hw_mode, 3 | 4)
    } else {
        // Version 3
        matches!(hw_mode, 4 | 5 | 6 | 7 | 12 | 13)
    }
}

/// Load a 48K page into the Spectrum memory.
///
/// Page mapping for 48K snapshots:
///   4 → $8000–$BFFF
///   5 → $C000–$FFFF
///   8 → $4000–$7FFF
fn load_48k_page(spectrum: &mut Spectrum, page: u8, ram: &[u8]) -> Result<(), String> {
    let base_addr: u16 = match page {
        4 => 0x8000,
        5 => 0xC000,
        8 => 0x4000,
        _ => return Ok(()), // Skip unknown pages (ROM pages, etc.)
    };

    let bus = spectrum.bus_mut();
    for (i, &byte) in ram.iter().enumerate() {
        bus.memory.write(base_addr + i as u16, byte);
    }
    Ok(())
}

/// Load a 128K page into the correct bank.
///
/// Page mapping for 128K snapshots:
///   3 → bank 0, 4 → bank 1, 5 → bank 2, 6 → bank 3,
///   7 → bank 4, 8 → bank 5, 9 → bank 6, 10 → bank 7
fn load_128k_page(
    spectrum: &mut Spectrum,
    page: u8,
    ram: &[u8],
    port_7ffd: u8,
) -> Result<(), String> {
    let bank = match page {
        3 => 0,
        4 => 1,
        5 => 2,
        6 => 3,
        7 => 4,
        8 => 5,
        9 => 6,
        10 => 7,
        _ => return Ok(()), // Skip ROM pages
    };

    // Bank 5 is always at $4000, bank 2 at $8000. Other banks need to be
    // paged in at $C000.
    let bus = spectrum.bus_mut();
    match bank {
        5 => {
            for (i, &byte) in ram.iter().enumerate() {
                bus.memory.write(0x4000u16 + i as u16, byte);
            }
        }
        2 => {
            for (i, &byte) in ram.iter().enumerate() {
                bus.memory.write(0x8000u16 + i as u16, byte);
            }
        }
        _ => {
            // Page this bank in at $C000, write, then restore.
            bus.memory
                .write_bank_register((port_7ffd & 0xF8) | bank);
            for (i, &byte) in ram.iter().enumerate() {
                bus.memory.write(0xC000u16 + i as u16, byte);
            }
            bus.memory.write_bank_register(port_7ffd);
        }
    }

    Ok(())
}

/// Decompress Z80-format RLE data.
///
/// Escape sequence: `ED ED xx yy` = repeat byte `yy` × `xx` times.
fn decompress_z80(src: &[u8], dst: &mut [u8]) -> Result<(), String> {
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SpectrumConfig, SpectrumModel};

    fn make_48k_spectrum() -> Spectrum {
        let rom = vec![0u8; 0x4000];
        Spectrum::new(&SpectrumConfig {
            model: SpectrumModel::Spectrum48K,
            rom,
        })
    }

    fn make_128k_spectrum() -> Spectrum {
        let rom = vec![0u8; 0x8000];
        Spectrum::new(&SpectrumConfig {
            model: SpectrumModel::Spectrum128K,
            rom,
        })
    }

    /// Build a minimal v1 uncompressed .Z80 snapshot.
    fn make_v1_uncompressed(pc: u16) -> Vec<u8> {
        let mut data = vec![0u8; V1_HEADER_SIZE + 0xC000]; // 30 header + 48K RAM

        // A=0xAA, F=0xFF
        data[0] = 0xAA;
        data[1] = 0xFF;
        // BC=0x1234
        data[2] = 0x34; // C
        data[3] = 0x12; // B
        // HL=0x5678
        data[4] = 0x78; // L
        data[5] = 0x56; // H
        // PC
        data[6] = pc as u8;
        data[7] = (pc >> 8) as u8;
        // SP=0x8000
        data[8] = 0x00;
        data[9] = 0x80;
        // I=0x3F
        data[10] = 0x3F;
        // R=0x42
        data[11] = 0x42;
        // Flags byte 1: bit 0 = R bit 7, bits 1-3 = border 2 (red),
        // bit 5 = 0 (uncompressed)
        data[12] = 0x04; // border=2 (010 << 1), R bit 7=0

        // IFF1=1, IFF2=1
        data[27] = 1;
        data[28] = 1;
        // Flags byte 2: IM=1
        data[29] = 1;

        // Write a recognisable pattern in RAM.
        // Page at $4000 (file offset 30+0)
        data[V1_HEADER_SIZE] = 0x55;
        // Page at $8000 (file offset 30+0x4000)
        data[V1_HEADER_SIZE + 0x4000] = 0x88;
        // Page at $C000 (file offset 30+0x8000)
        data[V1_HEADER_SIZE + 0x8000] = 0xCC;

        data
    }

    #[test]
    fn v1_uncompressed_sets_registers() {
        let mut spec = make_48k_spectrum();
        let z80_data = make_v1_uncompressed(0xABCD);

        load_z80(&mut spec, &z80_data).expect("load_z80 should succeed");

        assert_eq!(spec.cpu().regs.a, 0xAA);
        assert_eq!(spec.cpu().regs.f, 0xFF);
        assert_eq!(spec.cpu().regs.b, 0x12);
        assert_eq!(spec.cpu().regs.c, 0x34);
        assert_eq!(spec.cpu().regs.h, 0x56);
        assert_eq!(spec.cpu().regs.l, 0x78);
        assert_eq!(spec.cpu().regs.pc, 0xABCD);
        assert_eq!(spec.cpu().regs.sp, 0x8000);
        assert_eq!(spec.cpu().regs.i, 0x3F);
        assert_eq!(spec.cpu().regs.im, 1);
        assert!(spec.cpu().regs.iff1);
    }

    #[test]
    fn v1_uncompressed_loads_memory() {
        let mut spec = make_48k_spectrum();
        // PC must be non-zero for v1 detection (PC=0 triggers v2/v3).
        let z80_data = make_v1_uncompressed(0x0100);

        load_z80(&mut spec, &z80_data).expect("load_z80 should succeed");

        assert_eq!(spec.bus().memory.peek(0x4000), 0x55);
        assert_eq!(spec.bus().memory.peek(0x8000), 0x88);
        assert_eq!(spec.bus().memory.peek(0xC000), 0xCC);
    }

    #[test]
    fn v1_uncompressed_sets_border() {
        let mut spec = make_48k_spectrum();
        let z80_data = make_v1_uncompressed(0x0100);

        load_z80(&mut spec, &z80_data).expect("load_z80 should succeed");

        assert_eq!(spec.bus().ula.border_colour(), 2);
    }

    #[test]
    fn v1_compressed_loads_correctly() {
        let mut spec = make_48k_spectrum();

        // Build a v1 compressed snapshot.
        let mut header = vec![0u8; V1_HEADER_SIZE];
        header[6] = 0x00;
        header[7] = 0x01; // PC=0x0100 (non-zero → v1)
        header[8] = 0x00;
        header[9] = 0x80; // SP=0x8000
        // Flags byte 1: compressed (bit 5), border=3 (011 << 1)
        header[12] = 0x26; // 0b0010_0110: bit5=1 (compressed), bits1-3=011 (border 3)

        // Build compressed body: fill $4000 with 0xAA (256 times)
        // ED ED count value
        let mut body: Vec<u8> = Vec::new();
        // 256 × 0xAA at start of RAM ($4000)
        body.extend_from_slice(&[0xED, 0xED, 0x00, 0xAA]); // count=0 → 256 repetitions? No, count=0 means 0.
        // Actually: count is the byte value. ED ED 05 AA = repeat AA 5 times.
        // Let's do: 10 × 0xAA
        body.extend_from_slice(&[0xED, 0xED, 10, 0xAA]);
        // Then some literal bytes
        body.push(0x55);
        body.push(0x66);
        // Then pad the rest with zeros (uncompressed literal zeros).
        // Actually, rest of 48K RAM is just zeros. We can end the body.
        // The decompressor stops when dst is full or src is exhausted.

        let mut data = header;
        data.extend_from_slice(&body);

        load_z80(&mut spec, &data).expect("load_z80 should succeed");

        // First 10 bytes should be 0xAA.
        for i in 0..10 {
            assert_eq!(
                spec.bus().memory.peek(0x4000 + i),
                0xAA,
                "Byte at $400{i:X} should be 0xAA"
            );
        }
        // Next bytes should be 0x55, 0x66.
        assert_eq!(spec.bus().memory.peek(0x400A), 0x55);
        assert_eq!(spec.bus().memory.peek(0x400B), 0x66);

        assert_eq!(spec.bus().ula.border_colour(), 3);
    }

    /// Build a minimal v2 128K .Z80 snapshot.
    fn make_v2_128k(pc: u16, port_7ffd: u8) -> Vec<u8> {
        let mut data = Vec::new();

        // Base header (30 bytes)
        let mut header = vec![0u8; 30];
        header[0] = 0xBB; // A
        header[1] = 0xCC; // F
        // PC=0 → triggers v2/v3 detection
        header[6] = 0;
        header[7] = 0;
        header[8] = 0x00;
        header[9] = 0x80; // SP=$8000
        header[10] = 0x3F; // I
        header[12] = 0x04; // Flags1: border=2
        header[27] = 1; // IFF1
        header[28] = 1; // IFF2
        header[29] = 1; // IM=1
        data.extend_from_slice(&header);

        // Extended header length = 23 (v2)
        data.push(23);
        data.push(0);

        // Extended header (23 bytes)
        let mut ext = vec![0u8; 23];
        ext[0] = pc as u8; // PC low
        ext[1] = (pc >> 8) as u8; // PC high
        ext[2] = 3; // Hardware mode: 128K (v2 value)
        ext[3] = port_7ffd; // Port $7FFD
        data.extend_from_slice(&ext);

        // Memory blocks: write bank 5 (page 8) with recognisable data.
        // Compressed block: 3-byte header + compressed data.
        let mut page8_data = vec![0u8; 0x4000];
        page8_data[0] = 0x55; // First byte of bank 5
        let compressed = compress_for_test(&page8_data);
        data.push((compressed.len() & 0xFF) as u8);
        data.push(((compressed.len() >> 8) & 0xFF) as u8);
        data.push(8); // Page 8 = bank 5
        data.extend_from_slice(&compressed);

        // Bank 2 (page 5) with recognisable data.
        let mut page5_data = vec![0u8; 0x4000];
        page5_data[0] = 0x22;
        let compressed = compress_for_test(&page5_data);
        data.push((compressed.len() & 0xFF) as u8);
        data.push(((compressed.len() >> 8) & 0xFF) as u8);
        data.push(5); // Page 5 = bank 2
        data.extend_from_slice(&compressed);

        // Bank 0 (page 3) with data.
        let mut page3_data = vec![0u8; 0x4000];
        page3_data[0] = 0x00; // marker byte for bank 0
        page3_data[1] = 0xBB;
        let compressed = compress_for_test(&page3_data);
        data.push((compressed.len() & 0xFF) as u8);
        data.push(((compressed.len() >> 8) & 0xFF) as u8);
        data.push(3); // Page 3 = bank 0
        data.extend_from_slice(&compressed);

        data
    }

    /// Trivial "compression" for tests: just return the data as-is (uncompressed).
    /// Use 0xFFFF marker length for true uncompressed, but that's harder.
    /// Instead, we just avoid the ED ED pattern and return raw data.
    /// For test data that's mostly zeros, no ED ED sequences appear.
    fn compress_for_test(data: &[u8]) -> Vec<u8> {
        // Simple: just return data unchanged. The decompressor handles literal
        // bytes fine (it only triggers on ED ED xx yy sequences).
        data.to_vec()
    }

    #[test]
    fn v2_128k_sets_pc_from_ext_header() {
        let mut spec = make_128k_spectrum();
        let z80_data = make_v2_128k(0xABCD, 0x00);

        load_z80(&mut spec, &z80_data).expect("load_z80 should succeed");

        assert_eq!(spec.cpu().regs.pc, 0xABCD);
        assert_eq!(spec.cpu().regs.a, 0xBB);
    }

    #[test]
    fn v2_128k_loads_bank_5_and_2() {
        let mut spec = make_128k_spectrum();
        let z80_data = make_v2_128k(0x0000, 0x00);

        load_z80(&mut spec, &z80_data).expect("load_z80 should succeed");

        // Bank 5 at $4000
        assert_eq!(spec.bus().memory.peek(0x4000), 0x55);
        // Bank 2 at $8000
        assert_eq!(spec.bus().memory.peek(0x8000), 0x22);
    }

    #[test]
    fn v2_128k_loads_bank_0() {
        let mut spec = make_128k_spectrum();
        // Page bank 0 at $C000 via port_7ffd=0
        let z80_data = make_v2_128k(0x0000, 0x00);

        load_z80(&mut spec, &z80_data).expect("load_z80 should succeed");

        // Bank 0 is paged at $C000 (port_7ffd=0 means bank 0)
        assert_eq!(spec.bus().memory.peek(0xC001), 0xBB);
    }

    #[test]
    fn truncated_data_returns_error() {
        let mut spec = make_48k_spectrum();
        let result = load_z80(&mut spec, &[0u8; 10]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too short"));
    }

    #[test]
    fn decompress_z80_rle() {
        let src = [0xED, 0xED, 5, 0xAA, 0x11, 0x22];
        let mut dst = [0u8; 8];
        decompress_z80(&src, &mut dst).unwrap();
        assert_eq!(&dst[..7], &[0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0x11, 0x22]);
    }

    #[test]
    fn decompress_z80_literal_ed() {
        // A single ED byte followed by a non-ED byte should pass through literally.
        let src = [0xED, 0x55, 0x66];
        let mut dst = [0u8; 3];
        decompress_z80(&src, &mut dst).unwrap();
        assert_eq!(dst, [0xED, 0x55, 0x66]);
    }
}
