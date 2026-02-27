//! C64 memory subsystem.
//!
//! The C64 has 64K RAM with overlaid ROMs and I/O controlled by the 6510's
//! internal I/O port at $00 (DDR) and $01 (data register).
//!
//! # Banking
//!
//! The CPU port at $01 bits 0-2 control which ROMs and I/O are visible:
//!
//! | HIRAM(2) | LORAM(1) | CHAREN(0) | $A000-$BFFF | $D000-$DFFF | $E000-$FFFF |
//! |----------|----------|-----------|-------------|-------------|-------------|
//! | 1        | 1        | 1         | BASIC       | I/O         | Kernal      |
//! | 1        | 1        | 0         | BASIC       | Char ROM    | Kernal      |
//! | 1        | 0        | x         | RAM         | I/O         | Kernal      |
//! | 0        | 1        | x         | RAM         | I/O         | RAM         |
//! | 0        | 0        | x         | RAM         | RAM         | RAM         |
//!
//! VIC-II sees a separate 16K bank selected by CIA2 port A, and reads
//! character ROM in banks 0 and 2 at offsets $1000-$1FFF.

#![allow(clippy::cast_possible_truncation)]

use crate::cia::Cia;
use mos_sid_6581::Sid6581;
use crate::vic::Vic;

/// C64 memory subsystem: 64K RAM + ROMs + colour RAM + 6510 port.
pub struct C64Memory {
    /// 64K RAM. Writes always go here.
    ram: Box<[u8; 0x10000]>,
    /// Kernal ROM (8K, mapped at $E000-$FFFF).
    kernal_rom: Vec<u8>,
    /// BASIC ROM (8K, mapped at $A000-$BFFF).
    basic_rom: Vec<u8>,
    /// Character ROM (4K, mapped at $D000-$DFFF for CPU, $1000-$1FFF for VIC).
    char_rom: Vec<u8>,
    /// Colour RAM (1K, 4-bit per nybble, at $D800-$DBFF).
    colour_ram: [u8; 1024],
    /// 6510 port: data direction register ($00).
    port_ddr: u8,
    /// 6510 port: data register ($01).
    port_data: u8,
}

impl C64Memory {
    /// Create a new C64 memory subsystem with the given ROMs.
    ///
    /// # Panics
    ///
    /// Panics if ROM sizes are incorrect.
    #[must_use]
    pub fn new(kernal_rom: &[u8], basic_rom: &[u8], char_rom: &[u8]) -> Self {
        assert!(kernal_rom.len() == 8192, "Kernal ROM must be 8192 bytes");
        assert!(basic_rom.len() == 8192, "BASIC ROM must be 8192 bytes");
        assert!(char_rom.len() == 4096, "Character ROM must be 4096 bytes");

        Self {
            ram: Box::new([0; 0x10000]),
            kernal_rom: kernal_rom.to_vec(),
            basic_rom: basic_rom.to_vec(),
            char_rom: char_rom.to_vec(),
            colour_ram: [0; 1024],
            port_ddr: 0x2F,  // Default: bits 0-3,5 output
            port_data: 0x37, // Default: all ROMs + I/O visible
        }
    }

    /// HIRAM bit (bit 2 of port $01): Kernal ROM visible when set.
    fn hiram(&self) -> bool {
        self.effective_port() & 0x04 != 0
    }

    /// LORAM bit (bit 1 of port $01): BASIC ROM visible when set.
    fn loram(&self) -> bool {
        self.effective_port() & 0x02 != 0
    }

    /// CHAREN bit (bit 0 of port $01): I/O visible when set, Char ROM when clear.
    fn charen(&self) -> bool {
        self.effective_port() & 0x01 != 0
    }

    /// Effective port value: (data & ddr) | (external_pullups & !ddr).
    /// Undriven inputs float high due to pull-up resistors.
    fn effective_port(&self) -> u8 {
        (self.port_data & self.port_ddr) | (0x37 & !self.port_ddr)
    }

    /// Is the I/O area ($D000-$DFFF) visible to the CPU?
    fn io_visible(&self) -> bool {
        // I/O is visible when CHAREN=1 AND (HIRAM=1 OR LORAM=1)
        self.charen() && (self.hiram() || self.loram())
    }

    /// Is char ROM visible at $D000-$DFFF to the CPU?
    fn char_rom_visible(&self) -> bool {
        // Char ROM visible when CHAREN=0 AND (HIRAM=1 AND LORAM=1)
        !self.charen() && self.hiram() && self.loram()
    }

    /// CPU read: applies banking rules, handles $00/$01 port.
    ///
    /// I/O reads ($D000-$DFFF when I/O visible) are routed through the
    /// bus layer which calls `io_read` instead, so this method only
    /// handles RAM/ROM reads.
    #[must_use]
    pub fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            // 6510 port
            0x0000 => self.port_ddr,
            0x0001 => {
                // Read: output bits from data reg, input bits from external
                // For now, external lines float high (pull-up).
                (self.port_data & self.port_ddr) | (0x37 & !self.port_ddr)
            }

            // BASIC ROM area
            0xA000..=0xBFFF => {
                if self.hiram() && self.loram() {
                    self.basic_rom[(addr - 0xA000) as usize]
                } else {
                    self.ram[addr as usize]
                }
            }

            // $D000-$DFFF: Char ROM, I/O, or RAM
            0xD000..=0xDFFF => {
                if self.io_visible() {
                    // I/O — handled by bus layer calling io_read() instead.
                    // This path shouldn't be hit for I/O; return RAM as fallback.
                    self.ram[addr as usize]
                } else if self.char_rom_visible() {
                    self.char_rom[(addr - 0xD000) as usize]
                } else {
                    self.ram[addr as usize]
                }
            }

            // Kernal ROM area
            0xE000..=0xFFFF => {
                if self.hiram() {
                    self.kernal_rom[(addr - 0xE000) as usize]
                } else {
                    self.ram[addr as usize]
                }
            }

            // All other addresses: plain RAM
            _ => self.ram[addr as usize],
        }
    }

    /// CPU write: always writes to RAM, handles $00/$01 port.
    ///
    /// I/O writes ($D000-$DFFF when I/O visible) are routed through the
    /// bus layer which calls `io_write` instead.
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000 => self.port_ddr = value,
            0x0001 => self.port_data = value,
            _ => self.ram[addr as usize] = value,
        }
    }

    /// VIC-II read: sees a 16K bank with character ROM at $1000-$1FFF
    /// in banks 0 and 2.
    ///
    /// `bank_addr` is the address within the VIC-II's 16K window (0x0000-0x3FFF).
    /// `vic_bank` is the 16K bank number (0-3) from CIA2 port A.
    #[must_use]
    pub fn vic_read(&self, vic_bank: u8, bank_addr: u16) -> u8 {
        let bank_offset = u32::from(vic_bank) * 0x4000;
        let full_addr = (bank_offset + u32::from(bank_addr)) as u16;

        // Character ROM is visible to VIC-II at $1000-$1FFF in banks 0 and 2
        if (vic_bank == 0 || vic_bank == 2) && (0x1000..0x2000).contains(&bank_addr) {
            self.char_rom[(bank_addr - 0x1000) as usize]
        } else {
            self.ram[full_addr as usize]
        }
    }

    /// Peek: read memory without side effects (for observation/debugging).
    /// Uses CPU banking rules.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.cpu_read(addr)
    }

    /// Direct RAM write (bypasses I/O routing, for PRG loading).
    pub fn ram_write(&mut self, addr: u16, value: u8) {
        self.ram[addr as usize] = value;
    }

    /// Direct RAM read (for debugging/observation).
    #[must_use]
    pub fn ram_read(&self, addr: u16) -> u8 {
        self.ram[addr as usize]
    }

    /// Read colour RAM at the given offset (0-1023).
    #[must_use]
    pub fn colour_ram_read(&self, offset: u16) -> u8 {
        if (offset as usize) < self.colour_ram.len() {
            self.colour_ram[offset as usize] & 0x0F
        } else {
            0
        }
    }

    /// Write colour RAM at the given offset (0-1023). Only low 4 bits stored.
    pub fn colour_ram_write(&mut self, offset: u16, value: u8) {
        if (offset as usize) < self.colour_ram.len() {
            self.colour_ram[offset as usize] = value & 0x0F;
        }
    }

    /// Read I/O register (called by bus when $D000-$DFFF and I/O visible).
    #[must_use]
    pub fn io_read(&self, addr: u16, vic: &mut Vic, sid: &Sid6581, cia1: &Cia, cia2: &Cia) -> u8 {
        match addr {
            0xD000..=0xD3FF => vic.read((addr & 0x3F) as u8),
            0xD400..=0xD7FF => sid.read((addr & 0x1F) as u8),
            0xD800..=0xDBFF => self.colour_ram_read(addr - 0xD800),
            0xDC00..=0xDCFF => cia1.read((addr & 0x0F) as u8),
            0xDD00..=0xDDFF => cia2.read((addr & 0x0F) as u8),
            0xDE00..=0xDFFF => 0xFF, // I/O expansion area (unmapped)
            _ => 0xFF,
        }
    }

    /// Write I/O register (called by bus when $D000-$DFFF and I/O visible).
    pub fn io_write(
        &mut self,
        addr: u16,
        value: u8,
        vic: &mut Vic,
        sid: &mut Sid6581,
        cia1: &mut Cia,
        cia2: &mut Cia,
    ) {
        match addr {
            0xD000..=0xD3FF => vic.write((addr & 0x3F) as u8, value),
            0xD400..=0xD7FF => sid.write((addr & 0x1F) as u8, value),
            0xD800..=0xDBFF => self.colour_ram_write(addr - 0xD800, value),
            0xDC00..=0xDCFF => cia1.write((addr & 0x0F) as u8, value),
            0xDD00..=0xDDFF => cia2.write((addr & 0x0F) as u8, value),
            0xDE00..=0xDFFF => {} // I/O expansion area (ignored)
            _ => {}
        }
    }

    /// Check if I/O is visible for the bus layer.
    #[must_use]
    pub fn is_io_visible(&self) -> bool {
        self.io_visible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_memory() -> C64Memory {
        let kernal = vec![0xEE; 8192];
        let basic = vec![0xBB; 8192];
        let chargen = vec![0xCC; 4096];
        C64Memory::new(&kernal, &basic, &chargen)
    }

    #[test]
    fn default_banking_sees_roms() {
        let mem = make_memory();
        // Default $01 = $37: HIRAM=1, LORAM=1, CHAREN=1
        assert_eq!(mem.cpu_read(0xA000), 0xBB); // BASIC ROM
        assert_eq!(mem.cpu_read(0xE000), 0xEE); // Kernal ROM
    }

    #[test]
    fn writes_go_to_ram() {
        let mut mem = make_memory();
        mem.cpu_write(0xA000, 0x42);
        // Read still sees BASIC ROM
        assert_eq!(mem.cpu_read(0xA000), 0xBB);
        // But RAM has the written value
        assert_eq!(mem.ram_read(0xA000), 0x42);
    }

    #[test]
    fn all_ram_banking() {
        let mut mem = make_memory();
        // Set port $01 to $00: all RAM
        mem.cpu_write(0x0001, 0x00);
        mem.ram[0xA000] = 0x42;
        mem.ram[0xD000] = 0x43;
        mem.ram[0xE000] = 0x44;
        assert_eq!(mem.cpu_read(0xA000), 0x42);
        assert_eq!(mem.cpu_read(0xD000), 0x43);
        assert_eq!(mem.cpu_read(0xE000), 0x44);
    }

    #[test]
    fn char_rom_visible_when_charen_clear() {
        let mut mem = make_memory();
        // $01 = $36: HIRAM=1, LORAM=1, CHAREN=0 → Char ROM at $D000
        mem.cpu_write(0x0001, 0x36);
        assert_eq!(mem.cpu_read(0xD000), 0xCC);
    }

    #[test]
    fn port_read_write() {
        let mut mem = make_memory();
        mem.cpu_write(0x0000, 0xFF); // DDR: all output
        mem.cpu_write(0x0001, 0x55);
        assert_eq!(mem.cpu_read(0x0000), 0xFF);
        assert_eq!(mem.cpu_read(0x0001), 0x55);
    }

    #[test]
    fn vic_sees_char_rom_in_bank_0() {
        let mem = make_memory();
        // VIC bank 0, addr $1000 should see char ROM
        assert_eq!(mem.vic_read(0, 0x1000), 0xCC);
        // VIC bank 0, addr $0000 should see RAM
        assert_eq!(mem.vic_read(0, 0x0000), 0x00);
    }

    #[test]
    fn vic_bank_1_sees_ram() {
        let mut mem = make_memory();
        mem.ram[0x5000] = 0xAA; // Bank 1, offset $1000
        assert_eq!(mem.vic_read(1, 0x1000), 0xAA);
    }

    #[test]
    fn colour_ram() {
        let mut mem = make_memory();
        mem.colour_ram_write(0, 0x0F);
        assert_eq!(mem.colour_ram_read(0), 0x0F);
        // High nybble is masked
        mem.colour_ram_write(1, 0xFF);
        assert_eq!(mem.colour_ram_read(1), 0x0F);
    }
}
