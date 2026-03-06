//! SNA snapshot parser and loader for 48K and 128K ZX Spectrum.
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

/// Expected size of a 48K SNA snapshot file.
const SNA_48K_SIZE: usize = 49_179;

/// Expected size of a 128K SNA snapshot file.
const SNA_128K_SIZE: usize = 131_103;

/// Header size in bytes.
const HEADER_SIZE: usize = 27;

/// RAM dump size (48K from $4000-$FFFF).
const RAM_SIZE: usize = 49_152;

/// Z80 register state extracted from a snapshot.
#[derive(Debug, Clone, Default)]
pub struct Z80Registers {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub a_alt: u8,
    pub f_alt: u8,
    pub b_alt: u8,
    pub c_alt: u8,
    pub d_alt: u8,
    pub e_alt: u8,
    pub h_alt: u8,
    pub l_alt: u8,
    pub ix: u16,
    pub iy: u16,
    pub sp: u16,
    pub pc: u16,
    pub i: u8,
    pub r: u8,
    pub im: u8,
    pub iff1: bool,
    pub iff2: bool,
}

/// Target machine for loading snapshots.
///
/// This trait abstracts the Spectrum hardware so snapshot loaders can
/// operate without depending on any particular emulator implementation.
pub trait SnapshotTarget {
    /// Set all CPU registers from the snapshot.
    fn set_registers(&mut self, regs: &Z80Registers);

    /// Write a byte to the memory address space (respects banking).
    fn write_ram(&mut self, addr: u16, val: u8);

    /// Read a byte from the memory address space.
    fn read_ram(&self, addr: u16) -> u8;

    /// Set the border colour (0-7).
    fn set_border(&mut self, colour: u8);

    /// Write the 128K bank register (port $7FFD).
    /// No-op on 48K machines.
    fn write_bank_register(&mut self, val: u8);

    /// Write a value to the given AY register (0-15).
    /// No-op on machines without an AY chip.
    fn set_ay_register(&mut self, _reg: u8, _val: u8) {}

    /// Select the active AY register.
    /// No-op on machines without an AY chip.
    fn select_ay_register(&mut self, _reg: u8) {}
}

/// A parsed SNA snapshot.
pub struct SnaSnapshot {
    /// CPU registers from the header.
    pub registers: Z80Registers,
    /// Border colour (0-7).
    pub border: u8,
    /// Whether this is a 128K snapshot.
    pub is_128k: bool,
    /// Raw file data (kept for the apply step).
    data: Vec<u8>,
}

impl SnaSnapshot {
    /// Parse a SNA snapshot from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is the wrong size.
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        let is_128k = match data.len() {
            SNA_48K_SIZE => false,
            SNA_128K_SIZE => true,
            n => {
                return Err(format!(
                    "SNA file must be {SNA_48K_SIZE} (48K) or {SNA_128K_SIZE} (128K) bytes, got {n}"
                ));
            }
        };

        let mut regs = Z80Registers::default();
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

        let border = data[26];

        Ok(Self {
            registers: regs,
            border,
            is_128k,
            data: data.to_vec(),
        })
    }

    /// Apply the snapshot to a target machine.
    ///
    /// For 48K snapshots, pops PC from the stack via `read_ram()`.
    /// For 128K snapshots, reads PC from the extension and pages banks.
    ///
    /// # Errors
    ///
    /// Returns an error if (for 48K) the stack pointer points into ROM.
    pub fn apply(self, target: &mut impl SnapshotTarget) -> Result<(), String> {
        if self.is_128k {
            self.apply_128k(target)
        } else {
            self.apply_48k(target)
        }
    }

    fn apply_48k(self, target: &mut impl SnapshotTarget) -> Result<(), String> {
        target.set_registers(&self.registers);

        // Load RAM ($4000-$FFFF).
        let ram_data = &self.data[HEADER_SIZE..HEADER_SIZE + RAM_SIZE];
        for (i, &byte) in ram_data.iter().enumerate() {
            target.write_ram(0x4000u16 + i as u16, byte);
        }
        target.set_border(self.border & 0x07);

        // Pop PC from the stack.
        let sp = self.registers.sp;
        if sp < 0x4000 {
            return Err(format!(
                "SNA stack pointer ${sp:04X} points into ROM — cannot pop PC"
            ));
        }
        let pc_lo = target.read_ram(sp);
        let pc_hi = target.read_ram(sp.wrapping_add(1));
        let pc = u16::from(pc_lo) | (u16::from(pc_hi) << 8);

        let mut regs = self.registers;
        regs.sp = sp.wrapping_add(2);
        regs.pc = pc;
        target.set_registers(&regs);

        Ok(())
    }

    fn apply_128k(self, target: &mut impl SnapshotTarget) -> Result<(), String> {
        target.set_registers(&self.registers);

        // Read the 128K extension.
        let ext_offset = HEADER_SIZE + RAM_SIZE;
        let pc = u16::from(self.data[ext_offset]) | (u16::from(self.data[ext_offset + 1]) << 8);
        let port_7ffd = self.data[ext_offset + 2];

        let paged_bank = (port_7ffd & 0x07) as usize;
        if paged_bank == 2 || paged_bank == 5 {
            return Err(format!(
                "SNA 128K paged bank {paged_bank} duplicates a fixed window and is not representable"
            ));
        }

        let bank5_data = &self.data[HEADER_SIZE..HEADER_SIZE + 0x4000];
        let bank2_data = &self.data[HEADER_SIZE + 0x4000..HEADER_SIZE + 0x8000];
        let paged_data = &self.data[HEADER_SIZE + 0x8000..HEADER_SIZE + 0xC000];

        target.write_bank_register(port_7ffd);

        for (i, &byte) in bank5_data.iter().enumerate() {
            target.write_ram(0x4000u16 + i as u16, byte);
        }
        for (i, &byte) in bank2_data.iter().enumerate() {
            target.write_ram(0x8000u16 + i as u16, byte);
        }
        for (i, &byte) in paged_data.iter().enumerate() {
            target.write_ram(0xC000u16 + i as u16, byte);
        }

        target.set_border(self.border & 0x07);

        // Load remaining 5 banks.
        let extra_offset = ext_offset + 4;
        let mut file_pos = extra_offset;
        for bank in 0u8..8 {
            if bank == 5 || bank == 2 || bank as usize == paged_bank {
                continue;
            }
            let bank_data = &self.data[file_pos..file_pos + 0x4000];
            file_pos += 0x4000;

            let saved_reg = port_7ffd;
            target.write_bank_register((port_7ffd & 0xF8) | bank);
            for (i, &byte) in bank_data.iter().enumerate() {
                target.write_ram(0xC000u16 + i as u16, byte);
            }
            target.write_bank_register(saved_reg);
        }

        let mut regs = self.registers;
        regs.pc = pc;
        target.set_registers(&regs);
        Ok(())
    }
}

/// Convenience function: parse and apply in one step.
///
/// # Errors
///
/// Returns an error if the data is the wrong size, or (for 48K) the
/// stack pointer doesn't point into RAM.
pub fn load_sna(target: &mut impl SnapshotTarget, data: &[u8]) -> Result<(), String> {
    let snapshot = SnaSnapshot::parse(data)?;
    snapshot.apply(target)
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
    }

    impl TestTarget {
        fn new() -> Self {
            Self {
                ram: [0; 65536],
                border: 0,
                regs: Z80Registers::default(),
                bank_reg: 0,
                bank_history: Vec::new(),
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
    }

    fn make_sna(sp: u16, pc: u16) -> Vec<u8> {
        let mut data = vec![0u8; SNA_48K_SIZE];
        data[0] = 0x3F; // I
        data[20] = 0x42; // R
        data[21] = 0xFF; // F
        data[22] = 0xAA; // A
        data[23] = sp as u8;
        data[24] = (sp >> 8) as u8;
        data[25] = 1; // IM 1
        data[26] = 2; // Border red

        // Place PC on the stack.
        let sp_offset = (sp - 0x4000) as usize;
        data[HEADER_SIZE + sp_offset] = pc as u8;
        data[HEADER_SIZE + sp_offset + 1] = (pc >> 8) as u8;
        data
    }

    #[test]
    fn load_sna_sets_registers() {
        let mut target = TestTarget::new();
        let sna = make_sna(0x8000, 0x1234);
        load_sna(&mut target, &sna).expect("load should succeed");
        assert_eq!(target.regs.i, 0x3F);
        assert_eq!(target.regs.r, 0x42);
        assert_eq!(target.regs.f, 0xFF);
        assert_eq!(target.regs.a, 0xAA);
        assert_eq!(target.regs.im, 1);
        assert_eq!(target.regs.pc, 0x1234);
        assert_eq!(target.regs.sp, 0x8002);
    }

    #[test]
    fn load_sna_sets_border() {
        let mut target = TestTarget::new();
        let sna = make_sna(0x8000, 0x0000);
        load_sna(&mut target, &sna).expect("load should succeed");
        assert_eq!(target.border, 2);
    }

    #[test]
    fn load_sna_wrong_size() {
        let mut target = TestTarget::new();
        assert!(load_sna(&mut target, &[0u8; 100]).is_err());
    }

    #[test]
    fn parse_sna_extracts_header_fields_and_mode() {
        let mut data = vec![0u8; SNA_128K_SIZE];
        data[0] = 0x3F;
        data[1] = 0x10;
        data[2] = 0x20;
        data[15] = 0x34;
        data[16] = 0x12;
        data[19] = 0x04;
        data[20] = 0x56;
        data[21] = 0x78;
        data[22] = 0x9A;
        data[23] = 0xBC;
        data[24] = 0xDE;
        data[25] = 0x02;
        data[26] = 0x0F;

        let snapshot = SnaSnapshot::parse(&data).expect("parse should succeed");

        assert!(snapshot.is_128k);
        assert_eq!(snapshot.registers.i, 0x3F);
        assert_eq!(snapshot.registers.l_alt, 0x10);
        assert_eq!(snapshot.registers.h_alt, 0x20);
        assert_eq!(snapshot.registers.iy, 0x1234);
        assert!(snapshot.registers.iff1);
        assert!(snapshot.registers.iff2);
        assert_eq!(snapshot.registers.r, 0x56);
        assert_eq!(snapshot.registers.f, 0x78);
        assert_eq!(snapshot.registers.a, 0x9A);
        assert_eq!(snapshot.registers.sp, 0xDEBC);
        assert_eq!(snapshot.registers.im, 0x02);
        assert_eq!(snapshot.border, 0x0F);
    }

    #[test]
    fn load_sna_sp_in_rom() {
        let mut target = TestTarget::new();
        let mut sna = vec![0u8; SNA_48K_SIZE];
        sna[23] = 0x00; // SP = 0x0000
        sna[24] = 0x00;
        let result = load_sna(&mut target, &sna);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("points into ROM"));
    }

    #[test]
    fn load_sna_128k_sets_pc() {
        let mut target = TestTarget::new();
        let mut sna = vec![0u8; SNA_128K_SIZE];
        sna[0] = 0x3F; // I
        sna[22] = 0xAA; // A
        let ext = HEADER_SIZE + RAM_SIZE;
        sna[ext] = 0xCD; // PC low
        sna[ext + 1] = 0xAB; // PC high
        sna[ext + 2] = 0x00; // port 7FFD

        load_sna(&mut target, &sna).expect("load should succeed");
        assert_eq!(target.regs.pc, 0xABCD);
        assert_eq!(target.regs.a, 0xAA);
    }

    #[test]
    fn load_sna_128k_masks_border_and_restores_bank_register() {
        let mut target = TestTarget::new();
        let mut sna = vec![0u8; SNA_128K_SIZE];
        sna[26] = 0xFF; // border should be masked to 7

        // Bank 5 and bank 2 fixed windows.
        sna[HEADER_SIZE] = 0x11;
        sna[HEADER_SIZE + 0x4000] = 0x22;

        let ext = HEADER_SIZE + RAM_SIZE;
        sna[ext] = 0x34;
        sna[ext + 1] = 0x12; // PC = 0x1234
        sna[ext + 2] = 0x13; // port 7FFD, paged bank 3

        load_sna(&mut target, &sna).expect("load should succeed");

        assert_eq!(target.border, 7);
        assert_eq!(target.regs.pc, 0x1234);
        assert_eq!(target.bank_reg, 0x13);
        assert_eq!(target.ram[0x4000], 0x11);
        assert_eq!(target.ram[0x8000], 0x22);
        assert!(
            target.bank_history.contains(&0x10),
            "128K load should temporarily page bank 0 into $C000"
        );
        assert!(
            target.bank_history.contains(&0x17),
            "128K load should temporarily page bank 7 into $C000"
        );
        assert_eq!(target.bank_history.last(), Some(&0x13));
    }

    #[test]
    fn load_sna_128k_rejects_fixed_window_paged_bank() {
        let mut target = TestTarget::new();
        let mut sna = vec![0u8; SNA_128K_SIZE];
        let ext = HEADER_SIZE + RAM_SIZE;
        sna[ext + 2] = 0x05; // bank 5 duplicated into $C000, not representable in fixed layout

        let result = load_sna(&mut target, &sna);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not representable"));
    }
}
