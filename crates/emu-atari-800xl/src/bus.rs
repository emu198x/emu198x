//! Atari 800XL bus: address decoding for the 6502C.
//!
//! The 800XL has 64KB RAM with ROMs overlaid on top, controlled by
//! PIA PORTB. The hardware I/O area $D000-$D7FF is always registers.
//!
//! # Memory map
//!
//! | Address       | Read                                         | Write      |
//! |---------------|----------------------------------------------|------------|
//! | $0000-$3FFF   | RAM                                          | RAM        |
//! | $4000-$4FFF   | RAM                                          | RAM        |
//! | $5000-$57FF   | RAM or Self-test ROM (PORTB bit 7 = 0)       | RAM        |
//! | $5800-$7FFF   | RAM                                          | RAM        |
//! | $8000-$9FFF   | RAM or Cartridge (16KB cart)                  | RAM        |
//! | $A000-$BFFF   | RAM, BASIC ROM, or Cartridge                 | RAM        |
//! | $C000-$CFFF   | RAM or OS ROM (PORTB bit 0)                  | RAM        |
//! | $D000-$D0FF   | GTIA registers                               | GTIA       |
//! | $D100-$D1FF   | Unmapped ($FF)                               | Ignored    |
//! | $D200-$D2FF   | POKEY registers                              | POKEY      |
//! | $D300-$D3FF   | PIA registers                                | PIA        |
//! | $D400-$D4FF   | ANTIC registers                              | ANTIC      |
//! | $D500-$D7FF   | Unmapped ($FF)                               | Ignored    |
//! | $D800-$FFFF   | RAM or OS ROM (PORTB bit 0)                  | RAM        |

use atari_antic::Antic;
use atari_gtia::Gtia;
use atari_pokey::Pokey;
use emu_core::{Bus, ReadResult};
use mos_pia_6520::Pia6520;

use crate::cartridge::Cartridge;

/// Inner bus state (owns the hardware).
pub struct Atari800xlBusInner {
    /// 64KB RAM.
    pub ram: [u8; 65536],
    /// ANTIC display list processor.
    pub antic: Antic,
    /// GTIA graphics output.
    pub gtia: Gtia,
    /// POKEY sound and I/O.
    pub pokey: Pokey,
    /// PIA for joystick input and memory banking.
    pub pia: Pia6520,
    /// Cartridge ROM (optional).
    pub cart: Option<Cartridge>,
    /// OS ROM (up to 16KB covering $C000-$FFFF, with $D000-$D7FF gap).
    pub os_rom: Option<Vec<u8>>,
    /// BASIC ROM (8KB at $A000-$BFFF).
    pub basic_rom: Option<Vec<u8>>,
}

impl Atari800xlBusInner {
    /// Compute the effective PORTB value for banking decisions.
    ///
    /// Bits configured as output use the output register value.
    /// Bits configured as input float high (pull-ups), so they read as 1.
    fn effective_portb(&self) -> u8 {
        self.pia.port_b_output() | !self.pia.ddr_b()
    }
}

/// Thin wrapper that implements `emu_core::Bus`.
pub struct Atari800xlBus<'a>(pub &'a mut Atari800xlBusInner);

impl Bus for Atari800xlBus<'_> {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr = addr as u16;
        let portb = self.0.effective_portb();
        let os_rom_enabled = portb & 0x01 != 0;
        let basic_enabled = portb & 0x02 == 0; // Bit 1 = 0 means BASIC on
        let self_test = portb & 0x80 == 0; // Bit 7 = 0 means self-test on

        let data = match addr {
            // $0000-$4FFF: always RAM
            0x0000..=0x4FFF => self.0.ram[addr as usize],

            // $5000-$57FF: self-test ROM or RAM
            0x5000..=0x57FF => {
                if self_test && os_rom_enabled {
                    if let Some(ref os) = self.0.os_rom {
                        // Self-test ROM is at $D000-$D7FF in the OS ROM file,
                        // mapped to $5000-$57FF. OS ROM file offset for $D000
                        // is $D000 - $C000 = $1000.
                        let offset = (addr - 0x5000 + 0x1000) as usize;
                        os.get(offset).copied().unwrap_or(0xFF)
                    } else {
                        self.0.ram[addr as usize]
                    }
                } else {
                    self.0.ram[addr as usize]
                }
            }

            // $5800-$7FFF: always RAM
            0x5800..=0x7FFF => self.0.ram[addr as usize],

            // $8000-$9FFF: cartridge (16KB) or RAM
            0x8000..=0x9FFF => {
                if let Some(ref cart) = self.0.cart
                    && cart.covers(addr)
                {
                    return ReadResult::new(cart.read(addr));
                }
                self.0.ram[addr as usize]
            }

            // $A000-$BFFF: cartridge, BASIC ROM, or RAM
            0xA000..=0xBFFF => {
                if let Some(ref cart) = self.0.cart
                    && cart.covers(addr)
                {
                    return ReadResult::new(cart.read(addr));
                }
                if basic_enabled
                    && let Some(ref basic) = self.0.basic_rom
                {
                    let offset = (addr - 0xA000) as usize;
                    return ReadResult::new(
                        basic.get(offset).copied().unwrap_or(0xFF),
                    );
                }
                self.0.ram[addr as usize]
            }

            // $C000-$CFFF: OS ROM or RAM
            0xC000..=0xCFFF => {
                if os_rom_enabled
                    && let Some(ref os) = self.0.os_rom
                {
                    let offset = (addr - 0xC000) as usize;
                    return ReadResult::new(
                        os.get(offset).copied().unwrap_or(0xFF),
                    );
                }
                self.0.ram[addr as usize]
            }

            // $D000-$D0FF: GTIA
            0xD000..=0xD0FF => self.0.gtia.read(addr as u8),

            // $D100-$D1FF: unmapped
            0xD100..=0xD1FF => 0xFF,

            // $D200-$D2FF: POKEY
            0xD200..=0xD2FF => self.0.pokey.read(addr as u8),

            // $D300-$D3FF: PIA
            0xD300..=0xD3FF => self.0.pia.read((addr & 0x03) as u8),

            // $D400-$D4FF: ANTIC
            0xD400..=0xD4FF => self.0.antic.read(addr as u8),

            // $D500-$D7FF: unmapped
            0xD500..=0xD7FF => 0xFF,

            // $D800-$FFFF: OS ROM or RAM
            0xD800..=0xFFFF => {
                if os_rom_enabled
                    && let Some(ref os) = self.0.os_rom
                {
                    // OS ROM file maps $C000-$FFFF continuously.
                    let offset = (addr - 0xC000) as usize;
                    return ReadResult::new(
                        os.get(offset).copied().unwrap_or(0xFF),
                    );
                }
                self.0.ram[addr as usize]
            }
        };

        ReadResult::new(data)
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr = addr as u16;
        match addr {
            // $0000-$CFFF: always goes to RAM
            0x0000..=0xCFFF => {
                self.0.ram[addr as usize] = value;
            }

            // $D000-$D0FF: GTIA
            0xD000..=0xD0FF => {
                self.0.gtia.write(addr as u8, value);
            }

            // $D100-$D1FF: unmapped
            0xD100..=0xD1FF => {}

            // $D200-$D2FF: POKEY
            0xD200..=0xD2FF => {
                self.0.pokey.write(addr as u8, value);
            }

            // $D300-$D3FF: PIA
            0xD300..=0xD3FF => {
                self.0.pia.write((addr & 0x03) as u8, value);
            }

            // $D400-$D4FF: ANTIC
            0xD400..=0xD4FF => {
                self.0.antic.write(addr as u8, value);
            }

            // $D500-$D7FF: unmapped
            0xD500..=0xD7FF => {}

            // $D800-$FFFF: write-through to RAM under ROM
            0xD800..=0xFFFF => {
                self.0.ram[addr as usize] = value;
            }
        }

        0 // No wait states
    }

    // The 800XL is fully memory-mapped -- no separate I/O space.
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

    fn make_bus() -> Atari800xlBusInner {
        Atari800xlBusInner {
            ram: [0; 65536],
            antic: Antic::new(AnticRegion::Ntsc),
            gtia: Gtia::new(),
            pokey: Pokey::new(1_789_772),
            pia: Pia6520::new(),
            cart: None,
            os_rom: None,
            basic_rom: None,
        }
    }

    #[test]
    fn ram_read_write() {
        let mut inner = make_bus();
        let mut bus = Atari800xlBus(&mut inner);
        bus.write(0x0100, 0xAB);
        assert_eq!(bus.read(0x0100).data, 0xAB);
    }

    #[test]
    fn ram_full_range() {
        let mut inner = make_bus();
        let mut bus = Atari800xlBus(&mut inner);
        // Low RAM
        bus.write(0x0000, 0x11);
        assert_eq!(bus.read(0x0000).data, 0x11);
        // Mid RAM
        bus.write(0x4000, 0x22);
        assert_eq!(bus.read(0x4000).data, 0x22);
        // High RAM (under OS ROM area, no ROM loaded)
        bus.write(0xE000, 0x33);
        assert_eq!(bus.read(0xE000).data, 0x33);
    }

    #[test]
    fn os_rom_overlay() {
        let mut inner = make_bus();
        // Load a 16KB OS ROM filled with $BB
        inner.os_rom = Some(vec![0xBB; 16384]);
        // Write to RAM underneath
        inner.ram[0xC000] = 0x11;
        inner.ram[0xFFFC] = 0x22;

        // PIA PORTB defaults: DDR=0 (all input), output=0, so
        // effective = 0x00 | !0x00 = 0xFF. Bit 0 = 1 -> OS ROM on.
        let mut bus = Atari800xlBus(&mut inner);

        // Read should see OS ROM
        assert_eq!(bus.read(0xC000).data, 0xBB);
        assert_eq!(bus.read(0xFFFC).data, 0xBB);

        // Write goes to RAM, not ROM
        bus.write(0xC000, 0xCC);
        assert_eq!(inner.ram[0xC000], 0xCC);
    }

    #[test]
    fn os_rom_disabled_shows_ram() {
        let mut inner = make_bus();
        inner.os_rom = Some(vec![0xBB; 16384]);
        inner.ram[0xC000] = 0x11;

        // Set PIA PORTB to disable OS ROM: bit 0 = 0.
        // Need DDR configured as output so we can drive the pin.
        // Write DDR_B = $FF (CRB bit 2 = 0 selects DDR)
        inner.pia.write(0x02, 0xFF);
        // Set CRB bit 2 = 1 to select data register
        inner.pia.write(0x03, 0x04);
        // Write PORTB = $FE (bit 0 = 0, OS ROM off)
        inner.pia.write(0x02, 0xFE);

        let mut bus = Atari800xlBus(&mut inner);
        assert_eq!(bus.read(0xC000).data, 0x11);
    }

    #[test]
    fn basic_rom_overlay() {
        let mut inner = make_bus();
        inner.basic_rom = Some(vec![0xAA; 8192]);
        inner.ram[0xA000] = 0x11;

        // Set PIA PORTB bit 1 = 0 to enable BASIC.
        inner.pia.write(0x02, 0xFF); // DDR_B = all output
        inner.pia.write(0x03, 0x04); // CRB bit 2 = 1
        inner.pia.write(0x02, 0xFD); // PORTB = $FD (bit 1 = 0)

        let mut bus = Atari800xlBus(&mut inner);
        assert_eq!(bus.read(0xA000).data, 0xAA);
    }

    #[test]
    fn cartridge_overrides_basic() {
        let mut inner = make_bus();
        inner.basic_rom = Some(vec![0xAA; 8192]);

        let mut cart_data = vec![0xCC; 8192];
        cart_data[0x1FFC] = 0x00; // Reset vector low
        cart_data[0x1FFD] = 0xA0; // Reset vector high
        inner.cart = Some(Cartridge::from_rom(&cart_data).expect("8K cart"));

        // Even with BASIC enabled, cartridge takes priority
        inner.pia.write(0x02, 0xFF);
        inner.pia.write(0x03, 0x04);
        inner.pia.write(0x02, 0xFD); // BASIC enabled

        let mut bus = Atari800xlBus(&mut inner);
        assert_eq!(bus.read(0xA000).data, 0xCC);
    }

    #[test]
    fn hardware_io_always_accessible() {
        let mut inner = make_bus();
        let mut bus = Atari800xlBus(&mut inner);

        // Write to GTIA COLBK
        bus.write(0xD01A, 0x94);
        // Write to ANTIC DMACTL
        bus.write(0xD400, 0x22);
        // POKEY IRQST
        assert_eq!(bus.read(0xD20E).data, 0xFF);
        // ANTIC VCOUNT
        assert_eq!(bus.read(0xD40B).data, 0);
    }

    #[test]
    fn unmapped_reads_ff() {
        let mut inner = make_bus();
        let mut bus = Atari800xlBus(&mut inner);
        assert_eq!(bus.read(0xD100).data, 0xFF);
        assert_eq!(bus.read(0xD500).data, 0xFF);
    }

    #[test]
    fn self_test_rom_mapping() {
        let mut inner = make_bus();
        let mut os = vec![0; 16384];
        // Self-test area in OS ROM file: offset $1000-$17FF
        for byte in &mut os[0x1000..0x1800] {
            *byte = 0xDD;
        }
        inner.os_rom = Some(os);
        inner.ram[0x5000] = 0x11;

        // Enable self-test: PORTB bit 7 = 0
        inner.pia.write(0x02, 0xFF); // DDR_B = all output
        inner.pia.write(0x03, 0x04); // CRB bit 2 = 1
        inner.pia.write(0x02, 0x7F); // PORTB = $7F (bit 7 = 0, bit 0 = 1)

        let mut bus = Atari800xlBus(&mut inner);
        assert_eq!(bus.read(0x5000).data, 0xDD);
    }

    #[test]
    fn write_under_rom_goes_to_ram() {
        let mut inner = make_bus();
        inner.os_rom = Some(vec![0xBB; 16384]);

        let mut bus = Atari800xlBus(&mut inner);
        bus.write(0xFFFC, 0x42);
        assert_eq!(inner.ram[0xFFFC], 0x42);
        // But read still returns ROM
        let mut bus = Atari800xlBus(&mut inner);
        assert_eq!(bus.read(0xFFFC).data, 0xBB);
    }
}
