//! C64 bus: memory and I/O routing.
//!
//! Implements `emu_core::Bus` for the C64. Routes CPU addresses through
//! the memory banking logic and I/O chip mapping.
//!
//! The C64 is fully memory-mapped — there is no separate I/O address space.
//! The 6502 `io_read`/`io_write` methods are unused (return $FF / 0).

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Bus, ReadResult};

use crate::cia::Cia;
use crate::keyboard::KeyboardMatrix;
use crate::memory::C64Memory;
use crate::sid::Sid;
use crate::vic::Vic;

/// The C64 bus, implementing `emu_core::Bus`.
///
/// Owns all subsystems. The CPU accesses everything through the `Bus` trait.
pub struct C64Bus {
    pub memory: C64Memory,
    pub vic: Vic,
    pub sid: Sid,
    pub cia1: Cia,
    pub cia2: Cia,
    pub keyboard: KeyboardMatrix,
}

impl C64Bus {
    #[must_use]
    pub fn new(memory: C64Memory) -> Self {
        Self {
            memory,
            vic: Vic::new(),
            sid: Sid::new(),
            cia1: Cia::new(),
            cia2: Cia::new(),
            keyboard: KeyboardMatrix::new(),
        }
    }

    /// Update the VIC-II bank from CIA2 port A.
    pub fn update_vic_bank(&mut self) {
        // CIA2 port A bits 0-1, inverted, select the VIC-II bank
        let pa = self.cia2.port_a_output();
        let bank = (!pa) & 0x03;
        self.vic.set_bank(bank);
    }
}

impl Bus for C64Bus {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr16 = addr as u16;

        // Check for I/O area reads ($D000-$DFFF when I/O visible)
        if (0xD000..=0xDFFF).contains(&addr16) && self.memory.is_io_visible() {
            let data = match addr16 {
                0xD000..=0xD3FF => self.vic.read((addr16 & 0x3F) as u8),
                0xD400..=0xD7FF => self.sid.read((addr16 & 0x1F) as u8),
                0xD800..=0xDBFF => self.memory.colour_ram_read(addr16 - 0xD800),
                0xDC00..=0xDCFF => {
                    let reg = (addr16 & 0x0F) as u8;
                    if reg == 0x0D {
                        self.cia1.read_icr_and_clear()
                    } else {
                        self.cia1.read_with_keyboard(reg, &self.keyboard)
                    }
                }
                0xDD00..=0xDDFF => {
                    let reg = (addr16 & 0x0F) as u8;
                    if reg == 0x0D {
                        self.cia2.read_icr_and_clear()
                    } else {
                        self.cia2.read(reg)
                    }
                }
                0xDE00..=0xDFFF => 0xFF, // I/O expansion
                _ => 0xFF,
            };
            return ReadResult::new(data);
        }

        ReadResult::new(self.memory.cpu_read(addr16))
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr16 = addr as u16;

        // Always write to RAM (except $00/$01 port)
        self.memory.cpu_write(addr16, value);

        // Also route I/O writes when I/O visible
        if (0xD000..=0xDFFF).contains(&addr16) && self.memory.is_io_visible() {
            match addr16 {
                0xD000..=0xD3FF => self.vic.write((addr16 & 0x3F) as u8, value),
                0xD400..=0xD7FF => self.sid.write((addr16 & 0x1F) as u8, value),
                0xD800..=0xDBFF => self.memory.colour_ram_write(addr16 - 0xD800, value),
                0xDC00..=0xDCFF => self.cia1.write((addr16 & 0x0F) as u8, value),
                0xDD00..=0xDDFF => {
                    self.cia2.write((addr16 & 0x0F) as u8, value);
                    // Update VIC bank when CIA2 port A changes
                    if (addr16 & 0x0F) == 0x00 || (addr16 & 0x0F) == 0x02 {
                        self.update_vic_bank();
                    }
                }
                _ => {}
            }
        }

        0 // No wait states
    }

    // C64 doesn't use separate I/O space — everything is memory-mapped.
    fn io_read(&mut self, _addr: u32) -> ReadResult {
        ReadResult::new(0xFF)
    }

    fn io_write(&mut self, _addr: u32, _value: u8) -> u8 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bus() -> C64Bus {
        let kernal = vec![0xEE; 8192];
        let basic = vec![0xBB; 8192];
        let chargen = vec![0xCC; 4096];
        let memory = C64Memory::new(&kernal, &basic, &chargen);
        C64Bus::new(memory)
    }

    #[test]
    fn ram_read_write() {
        let mut bus = make_bus();
        bus.write(0x8000, 0xAB);
        assert_eq!(bus.read(0x8000).data, 0xAB);
    }

    #[test]
    fn basic_rom_visible() {
        let bus = make_bus();
        assert_eq!(bus.memory.cpu_read(0xA000), 0xBB);
    }

    #[test]
    fn kernal_rom_visible() {
        let bus = make_bus();
        assert_eq!(bus.memory.cpu_read(0xE000), 0xEE);
    }

    #[test]
    fn vic_register_access() {
        let mut bus = make_bus();
        // Write border colour
        bus.write(0xD020, 0x06);
        assert_eq!(bus.read(0xD020).data, 0x06);
    }

    #[test]
    fn colour_ram_access() {
        let mut bus = make_bus();
        bus.write(0xD800, 0x05);
        assert_eq!(bus.read(0xD800).data, 0x05);
    }

    #[test]
    fn cia1_register_access() {
        let mut bus = make_bus();
        // Write CIA1 DDR A
        bus.write(0xDC02, 0xFF);
        assert_eq!(bus.read(0xDC02).data, 0xFF);
    }

    #[test]
    fn cia2_bank_updates_vic() {
        let mut bus = make_bus();
        // CIA2 port A DDR = output
        bus.write(0xDD02, 0x03);
        // Set VIC bank to 2 (write %01 to CIA2 PA, inverted = bank 2)
        bus.write(0xDD00, 0x01);
        assert_eq!(bus.vic.bank(), 2);
    }

    #[test]
    fn io_expansion_returns_ff() {
        let bus = make_bus();
        // $DE00-$DFFF returns $FF
        let val = bus
            .memory
            .io_read(0xDE00, &Vic::new(), &Sid::new(), &Cia::new(), &Cia::new());
        assert_eq!(val, 0xFF);
    }
}
