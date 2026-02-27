//! 1541 drive bus: address decoding for the drive's internal 6502.
//!
//! Address map:
//!   $0000-$07FF: 2KB RAM (mirrored through $1FFF via 11-bit decode)
//!   $1800-$180F: VIA1 — IEC serial bus interface (mirrored in $1800-$1BFF)
//!   $1C00-$1C0F: VIA2 — Disk controller (mirrored in $1C00-$1FFF)
//!   $C000-$FFFF: 16KB ROM
//!
//! Note: RAM and VIA regions overlap due to partial address decoding.
//! Addresses $0800-$17FF mirror RAM. VIA decoding wins at $1800-$1FFF.

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Bus, ReadResult};
use mos_via_6522::Via6522;

/// 1541 drive bus.
pub struct Drive1541Bus {
    /// 2KB drive RAM.
    ram: [u8; 2048],
    /// 16KB drive ROM ($C000-$FFFF).
    rom: Vec<u8>,
    /// VIA1: IEC serial bus interface.
    pub via1: Via6522,
    /// VIA2: Disk controller.
    pub via2: Via6522,
}

impl Drive1541Bus {
    /// Create a new drive bus with the given ROM.
    ///
    /// ROM must be 16,384 bytes.
    pub fn new(rom: Vec<u8>) -> Self {
        assert!(rom.len() == 16384, "1541 ROM must be 16384 bytes");
        Self {
            ram: [0; 2048],
            rom,
            via1: Via6522::new(),
            via2: Via6522::new(),
        }
    }

    /// Borrow the ROM data.
    #[must_use]
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }
}

impl Bus for Drive1541Bus {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr16 = addr as u16;
        match addr16 {
            // VIA1: $1800-$1BFF (mirrored, register select = addr & 0x0F)
            0x1800..=0x1BFF => ReadResult::new(self.via1.read((addr16 & 0x0F) as u8)),
            // VIA2: $1C00-$1FFF (mirrored, register select = addr & 0x0F)
            0x1C00..=0x1FFF => ReadResult::new(self.via2.read((addr16 & 0x0F) as u8)),
            // ROM: $C000-$FFFF
            0xC000..=0xFFFF => ReadResult::new(self.rom[(addr16 - 0xC000) as usize]),
            // RAM: $0000-$07FF (mirrored through $17FF, but VIA wins above)
            _ => ReadResult::new(self.ram[(addr16 & 0x07FF) as usize]),
        }
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr16 = addr as u16;
        match addr16 {
            0x1800..=0x1BFF => self.via1.write((addr16 & 0x0F) as u8, value),
            0x1C00..=0x1FFF => self.via2.write((addr16 & 0x0F) as u8, value),
            0xC000..=0xFFFF => {} // ROM — writes ignored
            _ => self.ram[(addr16 & 0x07FF) as usize] = value,
        }
        0 // No wait states
    }

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

    fn make_bus() -> Drive1541Bus {
        Drive1541Bus::new(vec![0xEA; 16384]) // NOP sled for ROM
    }

    #[test]
    fn ram_read_write() {
        let mut bus = make_bus();
        bus.write(0x0000, 0xAB);
        assert_eq!(bus.read(0x0000).data, 0xAB);
    }

    #[test]
    fn ram_mirrors() {
        let mut bus = make_bus();
        bus.write(0x0100, 0xCD);
        assert_eq!(bus.read(0x0900).data, 0xCD); // $0100 mirrored at $0900
    }

    #[test]
    fn rom_read() {
        let mut rom = vec![0; 16384];
        rom[0] = 0x42; // $C000
        rom[16383] = 0xFF; // $FFFF
        let mut bus = Drive1541Bus::new(rom);
        assert_eq!(bus.read(0xC000).data, 0x42);
        assert_eq!(bus.read(0xFFFF).data, 0xFF);
    }

    #[test]
    fn rom_write_ignored() {
        let mut bus = make_bus();
        bus.write(0xC000, 0x00);
        assert_eq!(bus.read(0xC000).data, 0xEA); // Unchanged
    }

    #[test]
    fn via1_access() {
        let mut bus = make_bus();
        // Write VIA1 DDR A
        bus.write(0x1803, 0xFF);
        assert_eq!(bus.read(0x1803).data, 0xFF);
    }

    #[test]
    fn via2_access() {
        let mut bus = make_bus();
        bus.write(0x1C03, 0xFF);
        assert_eq!(bus.read(0x1C03).data, 0xFF);
    }

    #[test]
    fn via1_mirror() {
        let mut bus = make_bus();
        bus.write(0x1803, 0xAA);
        // VIA1 mirrors in $1800-$1BFF range
        assert_eq!(bus.read(0x1813).data, 0xAA); // Same register
    }
}
