//! SNA snapshot loader for 48K and 128K Spectrum.
//!
//! **48K format** (49,179 bytes): 27-byte header + 49,152 bytes of RAM.
//! PC is stored on the stack (SP points to it in RAM), so after loading
//! we pop it.
//!
//! **128K format** (131,103 bytes): 27-byte header + 49,152 bytes of RAM
//! (banks 5, 2, and the currently paged bank in that order) + 4-byte
//! extension (PC, port $7FFD, TR-DOS flag) + 5 × 16,384 bytes of the
//! remaining RAM banks in ascending order.

#![allow(clippy::cast_possible_truncation)]

use emu_core::Cpu;

use crate::Spectrum;

/// Expected size of a 48K SNA snapshot file.
const SNA_48K_SIZE: usize = 49_179;

/// Expected size of a 128K SNA snapshot file.
const SNA_128K_SIZE: usize = 131_103;

/// Header size in bytes.
const HEADER_SIZE: usize = 27;

/// RAM dump size (48K from $4000-$FFFF).
const RAM_SIZE: usize = 49_152;

/// Load a SNA snapshot into the given Spectrum instance.
///
/// Accepts both 48K (49,179 bytes) and 128K (131,103 bytes) snapshots.
/// The Spectrum must already be created with the correct model — 48K
/// snapshots load into any model, 128K snapshots require a 128K model.
///
/// # Errors
///
/// Returns an error if the data is the wrong size, or (for 48K) the
/// stack pointer doesn't point into RAM.
pub fn load_sna(spectrum: &mut Spectrum, data: &[u8]) -> Result<(), String> {
    match data.len() {
        SNA_48K_SIZE => load_sna_48k(spectrum, data),
        SNA_128K_SIZE => load_sna_128k(spectrum, data),
        n => Err(format!(
            "SNA file must be {SNA_48K_SIZE} (48K) or {SNA_128K_SIZE} (128K) bytes, got {n}"
        )),
    }
}

/// Load the common 27-byte header into the CPU registers.
fn load_sna_header(spectrum: &mut Spectrum, data: &[u8]) -> u8 {
    spectrum.cpu_mut().reset();

    let cpu = spectrum.cpu_mut();
    let regs = &mut cpu.regs;

    regs.i = data[0];

    regs.l_alt = data[1];
    regs.h_alt = data[2];
    regs.e_alt = data[3];
    regs.d_alt = data[4];
    regs.c_alt = data[5];
    regs.b_alt = data[6];
    regs.f_alt = data[7];
    regs.a_alt = data[8];

    regs.l = data[9];
    regs.h = data[10];
    regs.e = data[11];
    regs.d = data[12];
    regs.c = data[13];
    regs.b = data[14];

    regs.iy = u16::from(data[15]) | (u16::from(data[16]) << 8);
    regs.ix = u16::from(data[17]) | (u16::from(data[18]) << 8);

    let iff2 = data[19] & 0x04 != 0;
    regs.iff1 = iff2;
    regs.iff2 = iff2;

    regs.r = data[20];
    regs.f = data[21];
    regs.a = data[22];
    regs.sp = u16::from(data[23]) | (u16::from(data[24]) << 8);
    regs.im = data[25];

    data[26] // border colour
}

/// Load a 48K SNA snapshot.
fn load_sna_48k(spectrum: &mut Spectrum, data: &[u8]) -> Result<(), String> {
    let border = load_sna_header(spectrum, data);

    // Load RAM ($4000-$FFFF) byte by byte through the memory trait.
    let ram_data = &data[HEADER_SIZE..HEADER_SIZE + RAM_SIZE];
    let bus = spectrum.bus_mut();
    for (i, &byte) in ram_data.iter().enumerate() {
        bus.memory.write(0x4000u16 + i as u16, byte);
    }
    bus.ula.set_border_colour(border & 0x07);

    // Pop PC from the stack.
    let sp = spectrum.cpu().regs.sp;
    if sp < 0x4000 {
        return Err(format!(
            "SNA stack pointer ${sp:04X} points into ROM — cannot pop PC"
        ));
    }
    let pc_lo = spectrum.bus().memory.read(sp);
    let pc_hi = spectrum.bus().memory.read(sp.wrapping_add(1));
    let pc = u16::from(pc_lo) | (u16::from(pc_hi) << 8);
    spectrum.cpu_mut().regs.sp = sp.wrapping_add(2);
    spectrum.cpu_mut().regs.pc = pc;

    Ok(())
}

/// Load a 128K SNA snapshot.
fn load_sna_128k(spectrum: &mut Spectrum, data: &[u8]) -> Result<(), String> {
    let border = load_sna_header(spectrum, data);

    // The first 48K of RAM in the file is banks 5, 2, and the currently paged bank.
    // We load this through the memory trait. First, set up the bank register so that
    // $C000 maps to the correct bank, then load the remaining 5 banks.

    // Read the 128K extension at offset 27 + 49152 = 49179.
    let ext_offset = HEADER_SIZE + RAM_SIZE;
    let pc = u16::from(data[ext_offset]) | (u16::from(data[ext_offset + 1]) << 8);
    let port_7ffd = data[ext_offset + 2];
    // data[ext_offset + 3] is the TR-DOS flag — we ignore it.

    let paged_bank = (port_7ffd & 0x07) as usize;

    // Load bank 5 ($4000-$7FFF in the file's first 16K)
    let bank5_data = &data[HEADER_SIZE..HEADER_SIZE + 0x4000];
    // Load bank 2 ($8000-$BFFF in the file's second 16K)
    let bank2_data = &data[HEADER_SIZE + 0x4000..HEADER_SIZE + 0x8000];
    // Load the paged bank ($C000-$FFFF in the file's third 16K)
    let paged_data = &data[HEADER_SIZE + 0x8000..HEADER_SIZE + 0xC000];

    // Set the bank register so $C000 maps to the paged bank during loading.
    let bus = spectrum.bus_mut();
    bus.memory.write_bank_register(port_7ffd);

    // Write banks 5, 2, paged through the normal address space.
    for (i, &byte) in bank5_data.iter().enumerate() {
        bus.memory.write(0x4000u16 + i as u16, byte);
    }
    for (i, &byte) in bank2_data.iter().enumerate() {
        bus.memory.write(0x8000u16 + i as u16, byte);
    }
    for (i, &byte) in paged_data.iter().enumerate() {
        bus.memory.write(0xC000u16 + i as u16, byte);
    }

    bus.ula.set_border_colour(border & 0x07);

    // Load the remaining 5 banks. The file stores them in ascending bank
    // order, skipping banks 5, 2, and the paged bank.
    let extra_offset = ext_offset + 4;
    let mut file_pos = extra_offset;
    for bank in 0u8..8 {
        if bank == 5 || bank == 2 || bank as usize == paged_bank {
            continue;
        }
        let bank_data = &data[file_pos..file_pos + 0x4000];
        file_pos += 0x4000;

        // Page this bank in, write the data, then restore.
        let bus = spectrum.bus_mut();
        let saved_reg = port_7ffd;
        bus.memory
            .write_bank_register((port_7ffd & 0xF8) | bank);
        for (i, &byte) in bank_data.iter().enumerate() {
            bus.memory.write(0xC000u16 + i as u16, byte);
        }
        bus.memory.write_bank_register(saved_reg);
    }

    spectrum.cpu_mut().regs.pc = pc;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SpectrumConfig, SpectrumModel};

    fn make_spectrum() -> Spectrum {
        let rom = vec![0u8; 0x4000];
        Spectrum::new(&SpectrumConfig {
            model: SpectrumModel::Spectrum48K,
            rom,
        })
    }

    fn make_sna(sp: u16, pc: u16) -> Vec<u8> {
        let mut data = vec![0u8; SNA_48K_SIZE];

        // Set some recognisable register values.
        data[0] = 0x3F; // I
        data[20] = 0x42; // R
        data[21] = 0xFF; // F
        data[22] = 0xAA; // A
        data[23] = sp as u8; // SP low
        data[24] = (sp >> 8) as u8; // SP high
        data[25] = 1; // IM 1
        data[26] = 2; // Border = red

        // Place PC on the stack in RAM.
        // SP is an address in $4000-$FFFF range, offset in RAM = SP - $4000.
        let sp_offset = (sp - 0x4000) as usize;
        data[HEADER_SIZE + sp_offset] = pc as u8;
        data[HEADER_SIZE + sp_offset + 1] = (pc >> 8) as u8;

        data
    }

    #[test]
    fn load_sna_sets_registers() {
        let mut spec = make_spectrum();
        let sna = make_sna(0x8000, 0x1234);

        load_sna(&mut spec, &sna).expect("load_sna should succeed");

        let regs = &spec.cpu().regs;
        assert_eq!(regs.i, 0x3F);
        assert_eq!(regs.r, 0x42);
        assert_eq!(regs.f, 0xFF);
        assert_eq!(regs.a, 0xAA);
        assert_eq!(regs.im, 1);
        assert_eq!(regs.pc, 0x1234);
        assert_eq!(regs.sp, 0x8002); // SP advanced by 2 after pop
    }

    #[test]
    fn load_sna_sets_border() {
        let mut spec = make_spectrum();
        let sna = make_sna(0x8000, 0x0000);

        load_sna(&mut spec, &sna).expect("load_sna should succeed");

        assert_eq!(spec.bus().ula.border_colour(), 2);
    }

    #[test]
    fn load_sna_wrong_size() {
        let mut spec = make_spectrum();
        let result = load_sna(&mut spec, &[0u8; 100]);
        assert!(result.is_err());
    }

    #[test]
    fn load_sna_sp_in_rom() {
        let mut spec = make_spectrum();
        let mut sna = vec![0u8; SNA_48K_SIZE];
        sna[23] = 0x00; // SP = 0x0000 (in ROM)
        sna[24] = 0x00;

        let result = load_sna(&mut spec, &sna);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("points into ROM"));
    }

    // --- 128K SNA tests ---

    fn make_128k_spectrum() -> Spectrum {
        let rom = vec![0u8; 0x8000]; // 32K ROM
        Spectrum::new(&SpectrumConfig {
            model: SpectrumModel::Spectrum128K,
            rom,
        })
    }

    fn make_128k_sna(port_7ffd: u8, pc: u16) -> Vec<u8> {
        let mut data = vec![0u8; SNA_128K_SIZE];

        // Header: recognisable values
        data[0] = 0x3F; // I
        data[20] = 0x42; // R
        data[21] = 0xFF; // F
        data[22] = 0xAA; // A
        data[23] = 0x00; // SP low (doesn't matter — PC is in extension)
        data[24] = 0x80; // SP = $8000
        data[25] = 1; // IM 1
        data[26] = 3; // Border = magenta

        // Write a marker byte into bank 5 (offset HEADER in file)
        data[HEADER_SIZE] = 0x55;
        // Write a marker byte into bank 2 (offset HEADER + 16K)
        data[HEADER_SIZE + 0x4000] = 0x22;

        // Extension at offset HEADER_SIZE + RAM_SIZE
        let ext = HEADER_SIZE + RAM_SIZE;
        data[ext] = pc as u8;
        data[ext + 1] = (pc >> 8) as u8;
        data[ext + 2] = port_7ffd;
        data[ext + 3] = 0; // TR-DOS flag

        data
    }

    #[test]
    fn load_sna_128k_sets_pc_from_extension() {
        let mut spec = make_128k_spectrum();
        let sna = make_128k_sna(0x00, 0xABCD);

        load_sna(&mut spec, &sna).expect("load_sna should succeed");

        assert_eq!(spec.cpu().regs.pc, 0xABCD);
        assert_eq!(spec.cpu().regs.a, 0xAA);
        assert_eq!(spec.cpu().regs.i, 0x3F);
    }

    #[test]
    fn load_sna_128k_loads_fixed_banks() {
        let mut spec = make_128k_spectrum();
        let sna = make_128k_sna(0x00, 0x0000);

        load_sna(&mut spec, &sna).expect("load_sna should succeed");

        // Bank 5 at $4000
        assert_eq!(spec.bus().memory.read(0x4000), 0x55);
        // Bank 2 at $8000
        assert_eq!(spec.bus().memory.read(0x8000), 0x22);
    }

    #[test]
    fn load_sna_128k_sets_border() {
        let mut spec = make_128k_spectrum();
        let sna = make_128k_sna(0x00, 0x0000);

        load_sna(&mut spec, &sna).expect("load_sna should succeed");

        assert_eq!(spec.bus().ula.border_colour(), 3);
    }
}
