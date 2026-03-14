//! Atari 7800 bus: address decoding for the 6502C "SALLY".
//!
//! The 7800 memory map:
//!
//! - $0000-$001F: TIA registers (audio only in 7800 mode)
//! - $0020-$003F: MARIA registers
//! - $0040-$00FF: Zero-page RAM (192 bytes)
//! - $0100-$011F: TIA mirror
//! - $0120-$013F: MARIA mirror
//! - $0140-$01FF: Stack RAM (192 bytes)
//! - $0200-$027F: TIA/MARIA mirrors
//! - $0280-$02FF: RIOT I/O registers
//! - $0300-$03FF: TIA/MARIA mirrors
//! - $0400-$047F: TIA/MARIA mirrors
//! - $0480-$04FF: RIOT mirror
//! - $1800-$27FF: Main RAM (4KB)
//! - $2800-$3FFF: Main RAM mirror
//! - $4000-$FFFF: Cartridge ROM

use atari_maria::Maria;
use emu_core::{Bus, ReadResult};
use mos_riot_6532::Riot6532;

use crate::cartridge::Cartridge;
use crate::tia_audio::TiaAudio;

/// Inner bus state (owns the hardware).
pub struct Atari7800BusInner {
    /// MARIA display processor.
    pub maria: Maria,
    /// TIA audio registers (video is handled by MARIA).
    pub tia_audio: TiaAudio,
    /// RIOT 6532 (I/O and timer).
    pub riot: Riot6532,
    /// Cartridge ROM.
    pub cart: Cartridge,
    /// Zero-page RAM ($0040-$00FF, 192 bytes).
    pub ram_zp: [u8; 192],
    /// Stack RAM ($0140-$01FF, 192 bytes).
    pub ram_stack: [u8; 192],
    /// Main RAM ($1800-$27FF, 4KB, mirrored to $3FFF).
    pub ram_main: [u8; 4096],
}

/// Thin wrapper that implements `emu_core::Bus`.
pub struct Atari7800Bus<'a>(pub &'a mut Atari7800BusInner);

impl Bus for Atari7800Bus<'_> {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr = addr as u16;
        let data = match addr {
            // $0000-$001F: TIA
            0x0000..=0x001F => self.0.tia_audio.read(addr as u8),

            // $0020-$003F: MARIA
            0x0020..=0x003F => self.0.maria.read(addr as u8 - 0x20),

            // $0040-$00FF: Zero-page RAM
            0x0040..=0x00FF => self.0.ram_zp[(addr - 0x40) as usize],

            // $0100-$011F: TIA mirror
            0x0100..=0x011F => self.0.tia_audio.read((addr & 0x1F) as u8),

            // $0120-$013F: MARIA mirror
            0x0120..=0x013F => self.0.maria.read((addr & 0x1F) as u8),

            // $0140-$01FF: Stack RAM
            0x0140..=0x01FF => self.0.ram_stack[(addr - 0x140) as usize],

            // $0200-$027F: TIA/MARIA mirrors
            0x0200..=0x027F => {
                if addr & 0x20 != 0 {
                    self.0.maria.read((addr & 0x1F) as u8)
                } else {
                    self.0.tia_audio.read((addr & 0x1F) as u8)
                }
            }

            // $0280-$02FF: RIOT
            0x0280..=0x02FF => self.0.riot.read(addr),

            // $0300-$03FF: TIA/MARIA mirrors + RIOT mirrors
            0x0300..=0x03FF => {
                if addr & 0x80 != 0 {
                    self.0.riot.read(addr)
                } else if addr & 0x20 != 0 {
                    self.0.maria.read((addr & 0x1F) as u8)
                } else {
                    self.0.tia_audio.read((addr & 0x1F) as u8)
                }
            }

            // $0400-$047F: TIA/MARIA mirrors
            0x0400..=0x047F => {
                if addr & 0x20 != 0 {
                    self.0.maria.read((addr & 0x1F) as u8)
                } else {
                    self.0.tia_audio.read((addr & 0x1F) as u8)
                }
            }

            // $0480-$04FF: RIOT mirror
            0x0480..=0x04FF => self.0.riot.read(addr),

            // $0500-$17FF: unmapped
            0x0500..=0x17FF => 0xFF,

            // $1800-$3FFF: Main RAM (4KB, mirrored)
            0x1800..=0x3FFF => self.0.ram_main[((addr - 0x1800) & 0x0FFF) as usize],

            // $4000-$FFFF: Cartridge ROM
            0x4000..=0xFFFF => self.0.cart.read(addr),
        };

        ReadResult::new(data)
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr = addr as u16;
        match addr {
            // $0000-$001F: TIA
            0x0000..=0x001F => self.0.tia_audio.write(addr as u8, value),

            // $0020-$003F: MARIA
            0x0020..=0x003F => self.0.maria.write(addr as u8 - 0x20, value),

            // $0040-$00FF: Zero-page RAM
            0x0040..=0x00FF => self.0.ram_zp[(addr - 0x40) as usize] = value,

            // $0100-$011F: TIA mirror
            0x0100..=0x011F => self.0.tia_audio.write((addr & 0x1F) as u8, value),

            // $0120-$013F: MARIA mirror
            0x0120..=0x013F => self.0.maria.write((addr & 0x1F) as u8, value),

            // $0140-$01FF: Stack RAM
            0x0140..=0x01FF => self.0.ram_stack[(addr - 0x140) as usize] = value,

            // $0200-$027F: TIA/MARIA mirrors
            0x0200..=0x027F => {
                if addr & 0x20 != 0 {
                    self.0.maria.write((addr & 0x1F) as u8, value);
                } else {
                    self.0.tia_audio.write((addr & 0x1F) as u8, value);
                }
            }

            // $0280-$02FF: RIOT
            0x0280..=0x02FF => self.0.riot.write(addr, value),

            // $0300-$03FF: TIA/MARIA mirrors + RIOT mirrors
            0x0300..=0x03FF => {
                if addr & 0x80 != 0 {
                    self.0.riot.write(addr, value);
                } else if addr & 0x20 != 0 {
                    self.0.maria.write((addr & 0x1F) as u8, value);
                } else {
                    self.0.tia_audio.write((addr & 0x1F) as u8, value);
                }
            }

            // $0400-$047F: TIA/MARIA mirrors
            0x0400..=0x047F => {
                if addr & 0x20 != 0 {
                    self.0.maria.write((addr & 0x1F) as u8, value);
                } else {
                    self.0.tia_audio.write((addr & 0x1F) as u8, value);
                }
            }

            // $0480-$04FF: RIOT mirror
            0x0480..=0x04FF => self.0.riot.write(addr, value),

            // $0500-$17FF: unmapped
            0x0500..=0x17FF => {}

            // $1800-$3FFF: Main RAM (4KB, mirrored)
            0x1800..=0x3FFF => self.0.ram_main[((addr - 0x1800) & 0x0FFF) as usize] = value,

            // $4000-$FFFF: Cartridge (writes trigger bank switching)
            0x4000..=0xFFFF => self.0.cart.write(addr, value),
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
    use atari_maria::MariaRegion;

    fn make_bus() -> Atari7800BusInner {
        let rom = vec![0xEA; 32768]; // 32KB NOP sled
        let cart = Cartridge::from_rom(&rom).expect("32K ROM");
        Atari7800BusInner {
            maria: Maria::new(MariaRegion::Ntsc),
            tia_audio: TiaAudio::new(),
            riot: Riot6532::new(),
            cart,
            ram_zp: [0; 192],
            ram_stack: [0; 192],
            ram_main: [0; 4096],
        }
    }

    #[test]
    fn zp_ram_read_write() {
        let mut inner = make_bus();
        let mut bus = Atari7800Bus(&mut inner);
        bus.write(0x0040, 0xAB);
        assert_eq!(bus.read(0x0040).data, 0xAB);
        bus.write(0x00FF, 0xCD);
        assert_eq!(bus.read(0x00FF).data, 0xCD);
    }

    #[test]
    fn stack_ram_read_write() {
        let mut inner = make_bus();
        let mut bus = Atari7800Bus(&mut inner);
        bus.write(0x0140, 0x42);
        assert_eq!(bus.read(0x0140).data, 0x42);
        bus.write(0x01FF, 0x99);
        assert_eq!(bus.read(0x01FF).data, 0x99);
    }

    #[test]
    fn main_ram_read_write() {
        let mut inner = make_bus();
        let mut bus = Atari7800Bus(&mut inner);
        bus.write(0x1800, 0x11);
        assert_eq!(bus.read(0x1800).data, 0x11);
        // Mirror: $2800 should alias to $1800.
        assert_eq!(bus.read(0x2800).data, 0x11);
    }

    #[test]
    fn cartridge_rom_read() {
        let mut inner = make_bus();
        let mut bus = Atari7800Bus(&mut inner);
        assert_eq!(bus.read(0x8000).data, 0xEA);
        assert_eq!(bus.read(0xFFFF).data, 0xEA);
    }

    #[test]
    fn riot_timer_accessible() {
        let mut inner = make_bus();
        let mut bus = Atari7800Bus(&mut inner);
        // Write TIM1T.
        bus.write(0x0294, 10);
        let val = bus.read(0x0284).data;
        assert_eq!(val, 10);
    }

    #[test]
    fn maria_register_write() {
        let mut inner = make_bus();
        let mut bus = Atari7800Bus(&mut inner);
        // Write MARIA BACKGRND at $0020.
        bus.write(0x0020, 0x94);
        // BACKGRND is write-only; read returns 0 for non-MSTAT.
        // Just verify no panic.
    }

    #[test]
    fn tia_audio_write_through_bus() {
        let mut inner = make_bus();
        let mut bus = Atari7800Bus(&mut inner);
        // Write AUDC0 at $0015.
        bus.write(0x0015, 0x0A);
        assert_eq!(inner.tia_audio.audc0, 0x0A);
    }

    #[test]
    fn unmapped_reads_ff() {
        let mut inner = make_bus();
        let mut bus = Atari7800Bus(&mut inner);
        assert_eq!(bus.read(0x0500).data, 0xFF);
        assert_eq!(bus.read(0x1000).data, 0xFF);
    }
}
