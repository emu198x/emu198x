//! Atari 2600 bus: address decoding for 6507.
//!
//! The 6507 has only 13 address lines (A0-A12), so the effective address
//! space is 8KB ($0000-$1FFF), mirrored across the full 64KB.
//!
//! Address decoding:
//! - A12=1: Cartridge ROM ($1000-$1FFF)
//! - A12=0, A7=0: TIA registers ($0000-$007F)
//! - A12=0, A7=1, A9=0: RIOT RAM ($0080-$00FF)
//! - A12=0, A7=1, A9=1: RIOT I/O + timer ($0280-$029F)
//!
//! The 6507 has no IRQ or NMI pins, so those signals are never asserted.

use emu_core::{Bus, ReadResult};

use crate::cartridge::Cartridge;
use atari_tia::Tia;
use mos_riot_6532::Riot6532;

/// Inner bus state (owns the hardware).
pub struct Atari2600BusInner {
    pub tia: Tia,
    pub riot: Riot6532,
    pub cart: Cartridge,
}

/// Thin wrapper that implements `emu_core::Bus`.
///
/// This is a newtype around a mutable reference to `Atari2600BusInner`,
/// needed because the CPU `tick()` takes `&mut impl Bus`.
pub struct Atari2600Bus<'a>(pub &'a mut Atari2600BusInner);

impl Bus for Atari2600Bus<'_> {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr = (addr as u16) & 0x1FFF; // 13-bit address space

        let data = if addr & 0x1000 != 0 {
            // A12=1: Cartridge ROM
            self.0.cart.read(addr)
        } else if addr & 0x0080 == 0 {
            // A12=0, A7=0: TIA read registers
            self.0.tia.read(addr as u8)
        } else if addr & 0x0200 == 0 {
            // A12=0, A7=1, A9=0: RIOT RAM
            self.0.riot.read(addr)
        } else {
            // A12=0, A7=1, A9=1: RIOT I/O + timer
            self.0.riot.read(addr)
        };

        ReadResult::new(data)
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr = (addr as u16) & 0x1FFF;

        if addr & 0x1000 != 0 {
            // A12=1: Cartridge (hotspot detection only)
            self.0.cart.write(addr, value);
        } else if addr & 0x0080 == 0 {
            // A12=0, A7=0: TIA write registers
            self.0.tia.write(addr as u8, value);
        } else if addr & 0x0200 == 0 {
            // A12=0, A7=1, A9=0: RIOT RAM
            self.0.riot.write(addr, value);
        } else {
            // A12=0, A7=1, A9=1: RIOT I/O + timer
            self.0.riot.write(addr, value);
        }

        0 // No wait states
    }

    // The 2600 is fully memory-mapped — no separate I/O space.
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
    use atari_tia::TiaRegion;

    fn make_bus() -> Atari2600BusInner {
        let rom = vec![0xEA; 4096]; // 4K NOP sled
        let cart = Cartridge::from_rom(&rom).expect("4K ROM");
        Atari2600BusInner {
            tia: Tia::new(TiaRegion::Ntsc),
            riot: Riot6532::new(),
            cart,
        }
    }

    #[test]
    fn riot_ram_read_write() {
        let mut inner = make_bus();
        let mut bus = Atari2600Bus(&mut inner);
        bus.write(0x0080, 0xAB);
        assert_eq!(bus.read(0x0080).data, 0xAB);
    }

    #[test]
    fn cartridge_rom_read() {
        let mut inner = make_bus();
        let mut bus = Atari2600Bus(&mut inner);
        // ROM filled with NOPs (0xEA)
        assert_eq!(bus.read(0x1000).data, 0xEA);
        assert_eq!(bus.read(0x1FFF).data, 0xEA);
    }

    #[test]
    fn tia_write_and_read() {
        let mut inner = make_bus();
        let mut bus = Atari2600Bus(&mut inner);
        // Write COLUBK ($09)
        bus.write(0x0009, 0x9A);
        // TIA write registers don't read back the same way —
        // TIA reads are collision/input registers at different addresses.
        // Just verify the write doesn't panic.
    }

    #[test]
    fn address_mirroring_13_bit() {
        let mut inner = make_bus();
        let mut bus = Atari2600Bus(&mut inner);
        // Write to $0080 (RIOT RAM)
        bus.write(0x0080, 0x42);
        // Read via mirrored address $2080 (A12=1 is masked, A7=1, A9=0)
        // Actually $2080 & $1FFF = $0080
        assert_eq!(bus.read(0x2080).data, 0x42);
    }
}
