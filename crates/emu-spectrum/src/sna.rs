//! SNA snapshot loader for 48K Spectrum.
//!
//! The 48K SNA format is 49,179 bytes: 27-byte header + 49,152 bytes of RAM.
//! The header contains the Z80 register state. PC is stored on the stack
//! (SP points to it in RAM), so after loading we pop it.

#![allow(clippy::cast_possible_truncation)]

use emu_core::Cpu;

use crate::Spectrum;

/// Expected size of a 48K SNA snapshot file.
const SNA_48K_SIZE: usize = 49_179;

/// Header size in bytes.
const HEADER_SIZE: usize = 27;

/// Load a 48K SNA snapshot into the given Spectrum instance.
///
/// Sets all Z80 registers, loads RAM ($4000-$FFFF), sets the border colour,
/// and pops PC from the stack.
///
/// # Errors
///
/// Returns an error if the data is not exactly 49,179 bytes, or if the
/// stack pointer doesn't point into RAM ($4000-$FFFF).
pub fn load_sna(spectrum: &mut Spectrum, data: &[u8]) -> Result<(), String> {
    if data.len() != SNA_48K_SIZE {
        return Err(format!(
            "SNA file must be exactly {SNA_48K_SIZE} bytes, got {}",
            data.len()
        ));
    }

    // Reset the CPU to clear the micro-op pipeline, then set registers.
    spectrum.cpu_mut().reset();

    let cpu = spectrum.cpu_mut();
    let regs = &mut cpu.regs;

    // Parse header — all 16-bit values are little-endian.
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

    // Byte 19: IFF2 is bit 2
    let iff2 = data[19] & 0x04 != 0;
    regs.iff1 = iff2;
    regs.iff2 = iff2;

    regs.r = data[20];

    regs.f = data[21];
    regs.a = data[22];

    regs.sp = u16::from(data[23]) | (u16::from(data[24]) << 8);

    regs.im = data[25];

    let border_colour = data[26];

    // Load RAM ($4000-$FFFF): 49,152 bytes starting at offset 27.
    let ram_data = &data[HEADER_SIZE..];

    // We need to downcast the memory to Memory48K to use load_ram.
    // The bus memory is a Box<dyn SpectrumMemory>, so we write byte by byte.
    let bus = spectrum.bus_mut();
    for (i, &byte) in ram_data.iter().enumerate() {
        let addr = 0x4000u16 + i as u16;
        bus.memory.write(addr, byte);
    }

    // Set border colour via the video chip.
    bus.video.set_border_colour(border_colour & 0x07);

    // Pop PC from the stack: read 2 bytes at SP from RAM, increment SP.
    let sp = spectrum.cpu().regs.sp;
    if sp < 0x4000 {
        return Err(format!(
            "SNA stack pointer ${sp:04X} points into ROM — cannot pop PC"
        ));
    }

    let pc_lo = spectrum.bus().memory.read(sp);
    let pc_hi = spectrum.bus().memory.read(sp.wrapping_add(1));
    let pc = u16::from(pc_lo) | (u16::from(pc_hi) << 8);

    // Clear the two stack bytes (they were the saved PC, not real stack data)
    // and advance SP.
    spectrum.cpu_mut().regs.sp = sp.wrapping_add(2);
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

        assert_eq!(spec.bus().video.border_colour(), 2);
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
}
