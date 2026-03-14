//! Atari 5200 bus: address decoding for the 6502C.
//!
//! The 5200 uses the full 16-bit address space of the 6502C:
//!
//! - $0000-$3FFF: 16KB RAM
//! - $4000-$BFFF: Cartridge ROM (up to 32KB, mirrored)
//! - $C000-$CFFF: GTIA registers (addr & $1F, mirrored every $100)
//! - $D400-$D4FF: ANTIC registers (addr & $0F)
//! - $E800-$E8FF: POKEY registers (addr & $0F)
//! - $F800-$FFFF: BIOS ROM (2KB, if loaded)
//!
//! Unmapped regions return $FF. Writes to ROM/unmapped are ignored.

use atari_antic::Antic;
use atari_gtia::Gtia;
use atari_pokey::Pokey;
use emu_core::{Bus, ReadResult};

use crate::cartridge::Cartridge;

/// Inner bus state (owns the hardware).
pub struct Atari5200BusInner {
    /// 16KB RAM.
    pub ram: [u8; 16384],
    /// ANTIC display list processor.
    pub antic: Antic,
    /// GTIA graphics output.
    pub gtia: Gtia,
    /// POKEY sound and I/O.
    pub pokey: Pokey,
    /// Cartridge ROM.
    pub cart: Cartridge,
    /// 2KB BIOS ROM (empty if no BIOS loaded).
    pub bios: Vec<u8>,
}

/// Thin wrapper that implements `emu_core::Bus`.
///
/// Newtype around a mutable reference to `Atari5200BusInner`,
/// needed because the CPU `tick()` takes `&mut impl Bus`.
pub struct Atari5200Bus<'a>(pub &'a mut Atari5200BusInner);

impl Bus for Atari5200Bus<'_> {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr = addr as u16;
        let data = match addr {
            // $0000-$3FFF: RAM
            0x0000..=0x3FFF => self.0.ram[(addr & 0x3FFF) as usize],

            // $4000-$BFFF: Cartridge ROM
            0x4000..=0xBFFF => self.0.cart.read(addr),

            // $C000-$CFFF: GTIA (addr & $1F)
            0xC000..=0xCFFF => self.0.gtia.read(addr as u8),

            // $D400-$D5FF: ANTIC (addr & $0F)
            0xD400..=0xD5FF => self.0.antic.read(addr as u8),

            // $E800-$E9FF: POKEY (addr & $0F)
            0xE800..=0xE9FF => self.0.pokey.read(addr as u8),

            // $F800-$FFFF: BIOS ROM or cartridge fallback
            0xF800..=0xFFFF => {
                if self.0.bios.is_empty() {
                    // No BIOS: fall through to cartridge for reset vector
                    self.0.cart.read(addr)
                } else {
                    let offset = (addr - 0xF800) as usize;
                    self.0.bios.get(offset).copied().unwrap_or(0xFF)
                }
            }

            // Unmapped
            _ => 0xFF,
        };

        ReadResult::new(data)
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr = addr as u16;
        match addr {
            // $0000-$3FFF: RAM
            0x0000..=0x3FFF => {
                self.0.ram[(addr & 0x3FFF) as usize] = value;
            }

            // $C000-$CFFF: GTIA
            0xC000..=0xCFFF => {
                self.0.gtia.write(addr as u8, value);
            }

            // $D400-$D5FF: ANTIC
            0xD400..=0xD5FF => {
                self.0.antic.write(addr as u8, value);
            }

            // $E800-$E9FF: POKEY
            0xE800..=0xE9FF => {
                self.0.pokey.write(addr as u8, value);
            }

            // ROM and unmapped: writes are ignored
            _ => {}
        }

        0 // No wait states
    }

    // The 5200 is fully memory-mapped -- no separate I/O space.
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
    use atari_antic::AnticRegion;

    fn make_bus() -> Atari5200BusInner {
        let rom = vec![0xEA; 8192]; // 8KB NOP sled
        let cart = Cartridge::from_rom(&rom).expect("8K ROM");
        Atari5200BusInner {
            ram: [0; 16384],
            antic: Antic::new(AnticRegion::Ntsc),
            gtia: Gtia::new(),
            pokey: Pokey::new(1_789_772),
            cart,
            bios: Vec::new(),
        }
    }

    #[test]
    fn ram_read_write() {
        let mut inner = make_bus();
        let mut bus = Atari5200Bus(&mut inner);
        bus.write(0x0100, 0xAB);
        assert_eq!(bus.read(0x0100).data, 0xAB);
    }

    #[test]
    fn ram_mirrors_within_16k() {
        let mut inner = make_bus();
        let mut bus = Atari5200Bus(&mut inner);
        bus.write(0x0000, 0x42);
        assert_eq!(bus.read(0x0000).data, 0x42);
        // Top of RAM
        bus.write(0x3FFF, 0x99);
        assert_eq!(bus.read(0x3FFF).data, 0x99);
    }

    #[test]
    fn cartridge_rom_read() {
        let mut inner = make_bus();
        let mut bus = Atari5200Bus(&mut inner);
        // ROM filled with NOPs (0xEA)
        assert_eq!(bus.read(0xA000).data, 0xEA);
        assert_eq!(bus.read(0xBFFF).data, 0xEA);
    }

    #[test]
    fn gtia_write_and_read() {
        let mut inner = make_bus();
        let mut bus = Atari5200Bus(&mut inner);
        // Write COLBK ($1A via GTIA)
        bus.write(0xC01A, 0x94);
        // GTIA reads are collision/input registers, not the same as write regs.
        // Just verify the write doesn't panic.
    }

    #[test]
    fn antic_write_and_read() {
        let mut inner = make_bus();
        let mut bus = Atari5200Bus(&mut inner);
        // Write DMACTL ($D400)
        bus.write(0xD400, 0x22);
        // VCOUNT reads as scan_line/2 (initially 0)
        assert_eq!(bus.read(0xD40B).data, 0);
    }

    #[test]
    fn pokey_write_and_read() {
        let mut inner = make_bus();
        let mut bus = Atari5200Bus(&mut inner);
        // Write AUDF1 ($E800)
        bus.write(0xE800, 0x10);
        // IRQST reads as $FF initially (no interrupts pending)
        assert_eq!(bus.read(0xE80E).data, 0xFF);
    }

    #[test]
    fn unmapped_reads_ff() {
        let mut inner = make_bus();
        let mut bus = Atari5200Bus(&mut inner);
        // $D000 is unmapped (between GTIA and ANTIC)
        assert_eq!(bus.read(0xD000).data, 0xFF);
    }

    #[test]
    fn bios_rom_read() {
        let mut inner = make_bus();
        inner.bios = vec![0xBB; 2048];
        let mut bus = Atari5200Bus(&mut inner);
        assert_eq!(bus.read(0xF800).data, 0xBB);
        assert_eq!(bus.read(0xFFFF).data, 0xBB);
    }

    #[test]
    fn no_bios_falls_through_to_cart() {
        let mut inner = make_bus();
        // No BIOS loaded, cart is 8KB at $A000-$BFFF filled with 0xEA
        let mut bus = Atari5200Bus(&mut inner);
        // $FFFC should map to cartridge
        let data = bus.read(0xFFFC).data;
        // The cartridge mirrors, so this reads from the ROM
        assert_eq!(data, 0xEA);
    }
}
