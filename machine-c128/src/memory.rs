//! C128 memory model with 128K RAM and MMU banking.
//!
//! The C128 memory map is significantly more complex than the C64,
//! controlled by the MMU at $D500-$D50B:
//!
//! - 128K RAM in two 64K banks
//! - Multiple ROM areas (BASIC low/high, KERNAL, editor)
//! - Zero page and stack relocation
//! - Common RAM areas shared between banks
//! - VIC and VDC video output

use emu_core::{Bus, IoBus};
use machine_c64::{Mmu, Reu, Vdc, VdcRevision};

/// C128 CIA (Complex Interface Adapter) chip state.
/// Identical to C64 CIA but exposed separately.
#[derive(Default, Clone)]
pub struct Cia {
    pub pra: u8,
    pub prb: u8,
    pub ddra: u8,
    pub ddrb: u8,
    pub ta_latch_lo: u8,
    pub ta_latch_hi: u8,
    pub ta_lo: u8,
    pub ta_hi: u8,
    pub tb_latch_lo: u8,
    pub tb_latch_hi: u8,
    pub tb_lo: u8,
    pub tb_hi: u8,
    pub cra: u8,
    pub crb: u8,
    pub icr: u8,
    pub icr_mask: u8,
    pub tod_10ths: u8,
    pub tod_sec: u8,
    pub tod_min: u8,
    pub tod_hr: u8,
    pub alarm_10ths: u8,
    pub alarm_sec: u8,
    pub alarm_min: u8,
    pub alarm_hr: u8,
    pub tod_latch_10ths: u8,
    pub tod_latch_sec: u8,
    pub tod_latch_min: u8,
    pub tod_latch_hr: u8,
    pub tod_latched: bool,
    pub tod_running: bool,
}

/// C128 memory subsystem.
pub struct C128Memory {
    /// RAM Bank 0 (64KB)
    pub ram0: [u8; 65536],
    /// RAM Bank 1 (64KB)
    pub ram1: [u8; 65536],

    /// C128 BASIC ROM low (16KB at $4000)
    basic_lo: [u8; 16384],
    /// C128 BASIC ROM high (16KB at $8000)
    basic_hi: [u8; 16384],
    /// C128 KERNAL ROM (16KB at $E000, shared with screen editor)
    kernal: [u8; 16384],
    /// Screen Editor ROM (4KB at $C000)
    editor: [u8; 4096],
    /// Character ROM (8KB - two 4K character sets)
    chargen: [u8; 8192],

    /// C64 BASIC ROM (8KB) for C64 mode
    c64_basic: [u8; 8192],
    /// C64 KERNAL ROM (8KB) for C64 mode
    c64_kernal: [u8; 8192],

    /// MMU (Memory Management Unit)
    pub mmu: Mmu,

    /// VIC-II registers
    pub vic_registers: [u8; 64],
    /// VDC (80-column chip)
    pub vdc: Vdc,
    /// SID registers
    pub sid_registers: [u8; 32],
    /// CIA1
    pub cia1: Cia,
    /// CIA2
    pub cia2: Cia,
    /// Color RAM (1KB, 4-bit values)
    pub color_ram: [u8; 1024],

    /// Keyboard matrix
    pub keyboard_matrix: [u8; 11], // C128 has 11 rows (3 extra for C128 keys)

    /// Processor port DDR ($00)
    pub port_ddr: u8,
    /// Processor port data ($01)
    pub port_data: u8,

    /// Cycle counter
    pub cycles: u32,
    /// Current raster line
    pub current_raster_line: u16,
    /// Tape signal
    pub tape_signal: bool,

    /// REU (optional)
    pub reu: Reu,

    /// Pending SID writes
    pub(crate) sid_writes: Vec<(u8, u8)>,

    /// C64 mode flag
    c64_mode: bool,
}

impl C128Memory {
    /// Create a new C128 memory.
    pub fn new() -> Self {
        Self {
            ram0: [0; 65536],
            ram1: [0; 65536],
            basic_lo: [0; 16384],
            basic_hi: [0; 16384],
            kernal: [0; 16384],
            editor: [0; 4096],
            chargen: [0; 8192],
            c64_basic: [0; 8192],
            c64_kernal: [0; 8192],
            mmu: Mmu::new(),
            vic_registers: [0; 64],
            vdc: Vdc::new(VdcRevision::Vdc8568),
            sid_registers: [0; 32],
            cia1: Cia::default(),
            cia2: Cia::default(),
            color_ram: [0; 1024],
            keyboard_matrix: [0xFF; 11],
            port_ddr: 0x2F,
            port_data: 0x37,
            cycles: 0,
            current_raster_line: 0,
            tape_signal: false,
            reu: Reu::default(),
            sid_writes: Vec::new(),
            c64_mode: false,
        }
    }

    /// Load C128 BASIC ROM (low part).
    pub fn load_basic_lo(&mut self, data: &[u8]) {
        let len = data.len().min(16384);
        self.basic_lo[..len].copy_from_slice(&data[..len]);
    }

    /// Load C128 BASIC ROM (high part).
    pub fn load_basic_hi(&mut self, data: &[u8]) {
        let len = data.len().min(16384);
        self.basic_hi[..len].copy_from_slice(&data[..len]);
    }

    /// Load C128 KERNAL ROM.
    pub fn load_kernal(&mut self, data: &[u8]) {
        let len = data.len().min(16384);
        self.kernal[..len].copy_from_slice(&data[..len]);
    }

    /// Load Screen Editor ROM.
    pub fn load_editor(&mut self, data: &[u8]) {
        let len = data.len().min(4096);
        self.editor[..len].copy_from_slice(&data[..len]);
    }

    /// Load Character ROM.
    pub fn load_chargen(&mut self, data: &[u8]) {
        let len = data.len().min(8192);
        self.chargen[..len].copy_from_slice(&data[..len]);
    }

    /// Load C64 BASIC ROM (for C64 mode).
    pub fn load_c64_basic(&mut self, data: &[u8]) {
        let len = data.len().min(8192);
        self.c64_basic[..len].copy_from_slice(&data[..len]);
    }

    /// Load C64 KERNAL ROM (for C64 mode).
    pub fn load_c64_kernal(&mut self, data: &[u8]) {
        let len = data.len().min(8192);
        self.c64_kernal[..len].copy_from_slice(&data[..len]);
    }

    /// Check if in C64 mode.
    pub fn is_c64_mode(&self) -> bool {
        self.c64_mode
    }

    /// Enter C64 mode.
    pub fn enter_c64_mode(&mut self) {
        self.c64_mode = true;
        // Configure MMU for C64 compatibility
        self.mmu.cr = 0x3F;
    }

    /// Exit C64 mode.
    pub fn exit_c64_mode(&mut self) {
        self.c64_mode = false;
        self.mmu.reset();
    }

    /// Check if Z80 CPU is active.
    pub fn is_z80_mode(&self) -> bool {
        self.mmu.is_z80_mode()
    }

    /// Get current RAM bank.
    fn current_bank(&self) -> u8 {
        self.mmu.ram_bank()
    }

    /// Read from current RAM bank.
    fn read_ram(&self, addr: u16) -> u8 {
        if self.current_bank() == 0 {
            self.ram0[addr as usize]
        } else {
            self.ram1[addr as usize]
        }
    }

    /// Write to current RAM bank.
    fn write_ram(&mut self, addr: u16, value: u8) {
        if self.current_bank() == 0 {
            self.ram0[addr as usize] = value;
        } else {
            self.ram1[addr as usize] = value;
        }
    }

    /// Get the VIC bank base address.
    pub fn vic_bank(&self) -> u16 {
        // CIA2 PA bits 0-1 (active low) select VIC bank
        let bank = (!self.cia2.pra) & 0x03;
        (bank as u16) * 0x4000
    }

    /// Read I/O area.
    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            // VIC-II ($D000-$D3FF, mirrored)
            0xD000..=0xD3FF => {
                let reg = (addr & 0x3F) as usize;
                match reg {
                    // Raster counter low byte
                    0x12 => (self.current_raster_line & 0xFF) as u8,
                    // Control register 1 (includes raster bit 8)
                    0x11 => {
                        let raster_msb = if self.current_raster_line > 255 { 0x80 } else { 0 };
                        (self.vic_registers[reg] & 0x7F) | raster_msb
                    }
                    _ => self.vic_registers[reg],
                }
            }

            // SID ($D400-$D4FF)
            0xD400..=0xD4FF => {
                let reg = (addr & 0x1F) as usize;
                self.sid_registers[reg]
            }

            // MMU ($D500-$D50B)
            0xD500..=0xD50B => self.mmu.read(addr),

            // VDC ($D600-$D6FF)
            0xD600..=0xD6FF => match addr & 0x01 {
                0 => self.vdc.read_status(),
                _ => {
                    // Note: VDC data register read requires mutable access
                    // Return 0xFF here; actual reads go through Bus trait
                    0xFF
                }
            },

            // Color RAM ($D800-$DBFF)
            0xD800..=0xDBFF => self.color_ram[(addr - 0xD800) as usize] | 0xF0,

            // CIA1 ($DC00-$DCFF)
            0xDC00..=0xDCFF => self.read_cia1(addr),

            // CIA2 ($DD00-$DDFF)
            0xDD00..=0xDDFF => self.read_cia2(addr),

            // Expansion port / unmapped
            _ => 0xFF,
        }
    }

    /// Write I/O area.
    fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            // VIC-II ($D000-$D3FF)
            0xD000..=0xD3FF => {
                let reg = (addr & 0x3F) as usize;
                self.vic_registers[reg] = value;
            }

            // SID ($D400-$D4FF)
            0xD400..=0xD4FF => {
                let reg = (addr & 0x1F) as usize;
                self.sid_registers[reg] = value;
                self.sid_writes.push((reg as u8, value));
            }

            // MMU ($D500-$D50B)
            0xD500..=0xD50B => self.mmu.write(addr, value),

            // VDC ($D600-$D6FF)
            0xD600..=0xD6FF => match addr & 0x01 {
                0 => self.vdc.write_address(value),
                _ => self.vdc.write_data(value),
            },

            // Color RAM ($D800-$DBFF)
            0xD800..=0xDBFF => {
                self.color_ram[(addr - 0xD800) as usize] = value & 0x0F;
            }

            // CIA1 ($DC00-$DCFF)
            0xDC00..=0xDCFF => self.write_cia1(addr, value),

            // CIA2 ($DD00-$DDFF)
            0xDD00..=0xDDFF => self.write_cia2(addr, value),

            _ => {}
        }
    }

    /// Read from CIA1.
    fn read_cia1(&self, addr: u16) -> u8 {
        let reg = addr & 0x0F;
        match reg {
            0 => {
                // Port A: keyboard column select and joystick 2
                self.cia1.pra
            }
            1 => {
                // Port B: keyboard row read and joystick 1
                let col_select = self.cia1.pra;
                let mut row_data = 0xFF;
                for col in 0..8 {
                    if col_select & (1 << col) == 0 {
                        row_data &= self.keyboard_matrix[col];
                    }
                }
                row_data
            }
            2 => self.cia1.ddra,
            3 => self.cia1.ddrb,
            4 => self.cia1.ta_lo,
            5 => self.cia1.ta_hi,
            6 => self.cia1.tb_lo,
            7 => self.cia1.tb_hi,
            13 => {
                // ICR: read clears
                let icr = self.cia1.icr;
                icr
            }
            14 => self.cia1.cra,
            15 => self.cia1.crb,
            _ => 0xFF,
        }
    }

    /// Write to CIA1.
    fn write_cia1(&mut self, addr: u16, value: u8) {
        let reg = addr & 0x0F;
        match reg {
            0 => self.cia1.pra = value,
            1 => self.cia1.prb = value,
            2 => self.cia1.ddra = value,
            3 => self.cia1.ddrb = value,
            4 => self.cia1.ta_latch_lo = value,
            5 => self.cia1.ta_latch_hi = value,
            6 => self.cia1.tb_latch_lo = value,
            7 => self.cia1.tb_latch_hi = value,
            13 => {
                if value & 0x80 != 0 {
                    self.cia1.icr_mask |= value & 0x1F;
                } else {
                    self.cia1.icr_mask &= !(value & 0x1F);
                }
            }
            14 => {
                if value & 0x10 != 0 {
                    self.cia1.ta_lo = self.cia1.ta_latch_lo;
                    self.cia1.ta_hi = self.cia1.ta_latch_hi;
                }
                self.cia1.cra = value & 0xEF;
            }
            15 => {
                if value & 0x10 != 0 {
                    self.cia1.tb_lo = self.cia1.tb_latch_lo;
                    self.cia1.tb_hi = self.cia1.tb_latch_hi;
                }
                self.cia1.crb = value & 0xEF;
            }
            _ => {}
        }
    }

    /// Read from CIA2.
    fn read_cia2(&self, addr: u16) -> u8 {
        let reg = addr & 0x0F;
        match reg {
            0 => self.cia2.pra,
            1 => self.cia2.prb,
            2 => self.cia2.ddra,
            3 => self.cia2.ddrb,
            4 => self.cia2.ta_lo,
            5 => self.cia2.ta_hi,
            6 => self.cia2.tb_lo,
            7 => self.cia2.tb_hi,
            13 => self.cia2.icr,
            14 => self.cia2.cra,
            15 => self.cia2.crb,
            _ => 0xFF,
        }
    }

    /// Write to CIA2.
    fn write_cia2(&mut self, addr: u16, value: u8) {
        let reg = addr & 0x0F;
        match reg {
            0 => self.cia2.pra = value,
            1 => self.cia2.prb = value,
            2 => self.cia2.ddra = value,
            3 => self.cia2.ddrb = value,
            4 => self.cia2.ta_latch_lo = value,
            5 => self.cia2.ta_latch_hi = value,
            6 => self.cia2.tb_latch_lo = value,
            7 => self.cia2.tb_latch_hi = value,
            13 => {
                if value & 0x80 != 0 {
                    self.cia2.icr_mask |= value & 0x1F;
                } else {
                    self.cia2.icr_mask &= !(value & 0x1F);
                }
            }
            14 => {
                if value & 0x10 != 0 {
                    self.cia2.ta_lo = self.cia2.ta_latch_lo;
                    self.cia2.ta_hi = self.cia2.ta_latch_hi;
                }
                self.cia2.cra = value & 0xEF;
            }
            15 => {
                if value & 0x10 != 0 {
                    self.cia2.tb_lo = self.cia2.tb_latch_lo;
                    self.cia2.tb_hi = self.cia2.tb_latch_hi;
                }
                self.cia2.crb = value & 0xEF;
            }
            _ => {}
        }
    }

    /// Tick CIA1 timers. Returns true if IRQ should fire.
    pub fn tick_cia1(&mut self, cycles: u32) -> bool {
        let mut irq = false;

        if self.cia1.cra & 0x01 != 0 {
            let timer = u16::from_le_bytes([self.cia1.ta_lo, self.cia1.ta_hi]);
            if let Some(new_timer) = timer.checked_sub(cycles as u16) {
                self.cia1.ta_lo = (new_timer & 0xFF) as u8;
                self.cia1.ta_hi = (new_timer >> 8) as u8;
            } else {
                self.cia1.icr |= 0x01;
                if self.cia1.icr_mask & 0x01 != 0 {
                    self.cia1.icr |= 0x80;
                    irq = true;
                }
                if self.cia1.cra & 0x08 != 0 {
                    self.cia1.cra &= !0x01;
                }
                self.cia1.ta_lo = self.cia1.ta_latch_lo;
                self.cia1.ta_hi = self.cia1.ta_latch_hi;
            }
        }

        irq
    }

    /// Tick CIA2 timers. Returns true if NMI should fire.
    pub fn tick_cia2(&mut self, cycles: u32) -> bool {
        let mut nmi = false;

        if self.cia2.cra & 0x01 != 0 {
            let timer = u16::from_le_bytes([self.cia2.ta_lo, self.cia2.ta_hi]);
            if let Some(new_timer) = timer.checked_sub(cycles as u16) {
                self.cia2.ta_lo = (new_timer & 0xFF) as u8;
                self.cia2.ta_hi = (new_timer >> 8) as u8;
            } else {
                self.cia2.icr |= 0x01;
                if self.cia2.icr_mask & 0x01 != 0 {
                    self.cia2.icr |= 0x80;
                    nmi = true;
                }
                if self.cia2.cra & 0x08 != 0 {
                    self.cia2.cra &= !0x01;
                }
                self.cia2.ta_lo = self.cia2.ta_latch_lo;
                self.cia2.ta_hi = self.cia2.ta_latch_hi;
            }
        }

        nmi
    }

    /// Clear pending SID writes.
    pub fn take_sid_writes(&mut self) -> Vec<(u8, u8)> {
        std::mem::take(&mut self.sid_writes)
    }

    /// Read a byte for VIC-II rendering.
    pub fn vic_read(&self, addr: u16) -> u8 {
        let bank = self.vic_bank();
        let physical = bank.wrapping_add(addr);

        // Check for Character ROM at $1000-$1FFF or $9000-$9FFF in banks 0 or 2
        if (bank == 0x0000 || bank == 0x8000) && (addr & 0x3000 == 0x1000) {
            return self.chargen[(addr & 0x0FFF) as usize];
        }

        self.ram0[physical as usize]
    }
}

impl Default for C128Memory {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus for C128Memory {
    fn read(&mut self, address: u32) -> u8 {
        let addr = (address & 0xFFFF) as u16;
        self.cycles += 1;

        // If in C64 mode, use C64-compatible banking
        if self.c64_mode {
            return self.read_c64_mode(addr);
        }

        // Use MMU for address translation
        let (phys, is_ram, is_io) = self.mmu.translate(addr);

        if is_io {
            return self.read_io(addr);
        }

        if is_ram {
            let bank = (phys >> 16) as u8;
            let ram_addr = (phys & 0xFFFF) as usize;
            return if bank == 0 {
                self.ram0[ram_addr]
            } else {
                self.ram1[ram_addr]
            };
        }

        // ROM access
        match addr {
            0x0000 => self.port_ddr,
            0x0001 => {
                let tape_bit = if self.tape_signal { 0 } else { 0x10 };
                (self.port_data & self.port_ddr) | (!self.port_ddr & 0xC0) | tape_bit
            }
            0x4000..=0x7FFF => self.basic_lo[(addr - 0x4000) as usize],
            0x8000..=0xBFFF => self.basic_hi[(addr - 0x8000) as usize],
            0xC000..=0xCFFF => self.editor[(addr - 0xC000) as usize],
            0xE000..=0xFFFF => self.kernal[(addr - 0xE000) as usize],
            _ => self.read_ram(addr),
        }
    }

    fn write(&mut self, address: u32, value: u8) {
        let addr = (address & 0xFFFF) as u16;
        self.cycles += 1;

        // If in C64 mode, use C64-compatible banking
        if self.c64_mode {
            self.write_c64_mode(addr, value);
            return;
        }

        match addr {
            0x0000 => self.port_ddr = value,
            0x0001 => self.port_data = value,
            0xD000..=0xDFFF if self.mmu.io_visible() => self.write_io(addr, value),
            _ => self.write_ram(addr, value),
        }
    }

    fn tick(&mut self, cycles: u32) {
        self.cycles += cycles;
        self.vdc.tick(cycles);
    }
}

impl IoBus for C128Memory {
    fn read_io(&mut self, port: u16) -> u8 {
        self.cycles += 4;

        let port_lo = port as u8;

        match port_lo {
            0xD5 => self.mmu.cr,
            0xFE => {
                let row_select = !(port >> 8) as u8;
                let mut result = 0xFF;
                for row in 0..8 {
                    if row_select & (1 << row) != 0 {
                        result &= self.keyboard_matrix[row];
                    }
                }
                result
            }
            _ => 0xFF,
        }
    }

    fn write_io(&mut self, port: u16, value: u8) {
        self.cycles += 4;

        let port_lo = port as u8;

        match port_lo {
            0xD5 => self.mmu.cr = value,
            0xD6 => self.mmu.mcr = value,
            0xFE => self.vic_registers[0x20] = value & 0x07,
            _ => {}
        }
    }
}

impl C128Memory {
    /// C64 mode read.
    fn read_c64_mode(&mut self, addr: u16) -> u8 {
        let config = (self.port_data | !self.port_ddr) & 0x07;
        let io_visible = (config & 0x04 != 0) && (config & 0x03 != 0);
        let basic_visible = config & 0x03 == 0x03;
        let kernal_visible = config & 0x02 != 0;
        let char_visible = (config & 0x04 == 0) && (config & 0x03 != 0);

        match addr {
            0x0000 => self.port_ddr,
            0x0001 => {
                let tape_bit = if self.tape_signal { 0 } else { 0x10 };
                (self.port_data & self.port_ddr) | (!self.port_ddr & 0xC0) | tape_bit
            }
            0xA000..=0xBFFF if basic_visible => self.c64_basic[(addr - 0xA000) as usize],
            0xD000..=0xDFFF if io_visible => self.read_io(addr),
            0xD000..=0xDFFF if char_visible => self.chargen[(addr - 0xD000) as usize],
            0xE000..=0xFFFF if kernal_visible => self.c64_kernal[(addr - 0xE000) as usize],
            _ => self.ram0[addr as usize],
        }
    }

    /// C64 mode write.
    fn write_c64_mode(&mut self, addr: u16, value: u8) {
        let config = (self.port_data | !self.port_ddr) & 0x07;
        let io_visible = (config & 0x04 != 0) && (config & 0x03 != 0);

        match addr {
            0x0000 => self.port_ddr = value,
            0x0001 => self.port_data = value,
            0xD000..=0xDFFF if io_visible => self.write_io(addr, value),
            _ => self.ram0[addr as usize] = value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_c128_memory() {
        let mem = C128Memory::new();
        assert!(!mem.is_c64_mode());
        assert!(!mem.is_z80_mode());
    }

    #[test]
    fn test_ram_banks() {
        let mut mem = C128Memory::new();

        // Write to bank 0
        mem.ram0[0x1000] = 0x42;

        // Write to bank 1
        mem.ram1[0x1000] = 0x99;

        // Verify
        assert_eq!(mem.ram0[0x1000], 0x42);
        assert_eq!(mem.ram1[0x1000], 0x99);
    }

    #[test]
    fn test_c64_mode() {
        let mut mem = C128Memory::new();

        mem.enter_c64_mode();
        assert!(mem.is_c64_mode());

        mem.exit_c64_mode();
        assert!(!mem.is_c64_mode());
    }
}
