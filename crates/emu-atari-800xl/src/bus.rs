//! Atari 8-bit bus: address decoding for the 6502C.
//!
//! Supports all models from the 400 to the 130XE. The memory map varies
//! by model:
//!
//! - **400/800**: No XL-style PORTB banking. RAM size 16KB/48KB.
//! - **600XL/800XL/65XE**: 64KB RAM with PORTB-controlled ROM overlay.
//! - **130XE**: 64KB base + 64KB extended RAM in 4 × 16KB banks at
//!   $4000-$7FFF, selected by PORTB bits 2-3. Bits 4-5 control whether
//!   CPU and/or ANTIC see the extended bank.
//!
//! The hardware I/O area $D000-$D7FF is always registers on all models.

use atari_antic::Antic;
use atari_gtia::Gtia;
use atari_pokey::Pokey;
use emu_core::{Bus, ReadResult};
use mos_pia_6520::Pia6520;

use crate::cartridge::Cartridge;
use crate::config::Atari8bitModel;

/// Inner bus state (owns the hardware).
pub struct Atari800xlBusInner {
    /// Base RAM (up to 64KB, sized to model).
    pub ram: [u8; 65536],
    /// Usable RAM size (model-dependent: 16KB, 48KB, or 64KB).
    pub ram_size: usize,
    /// Extended RAM banks for 130XE (4 × 16KB = 64KB). Empty on other models.
    pub extended_ram: Vec<u8>,
    /// Computer model.
    pub model: Atari8bitModel,
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

    /// Read from the 130XE extended bank at offset within $4000-$7FFF.
    ///
    /// PORTB bits 2-3 select the bank (0-3). Bit 4 controls CPU access.
    fn read_extended(&self, offset: u16, portb: u8) -> Option<u8> {
        if !self.model.has_extended_banking() {
            return None;
        }
        // Bit 4 = 0 means CPU sees extended bank
        if portb & 0x10 != 0 {
            return None; // CPU sees main RAM
        }
        let bank = ((portb >> 2) & 0x03) as usize;
        let idx = bank * 16384 + offset as usize;
        self.extended_ram.get(idx).copied()
    }

    /// Write to the 130XE extended bank at offset within $4000-$7FFF.
    fn write_extended(&mut self, offset: u16, value: u8, portb: u8) -> bool {
        if !self.model.has_extended_banking() {
            return false;
        }
        if portb & 0x10 != 0 {
            return false; // CPU sees main RAM
        }
        let bank = ((portb >> 2) & 0x03) as usize;
        let idx = bank * 16384 + offset as usize;
        if idx < self.extended_ram.len() {
            self.extended_ram[idx] = value;
            return true;
        }
        false
    }

    /// Read from the 130XE extended bank for ANTIC DMA.
    ///
    /// PORTB bit 5 = 0 means ANTIC sees extended bank.
    pub fn antic_read_extended(&self, addr: u16) -> Option<u8> {
        if !self.model.has_extended_banking() {
            return None;
        }
        if !(0x4000..=0x7FFF).contains(&addr) {
            return None;
        }
        let portb = self.effective_portb();
        // Bit 5 = 0 means ANTIC sees extended bank
        if portb & 0x20 != 0 {
            return None; // ANTIC sees main RAM
        }
        let bank = ((portb >> 2) & 0x03) as usize;
        let offset = (addr - 0x4000) as usize;
        let idx = bank * 16384 + offset;
        self.extended_ram.get(idx).copied()
    }
}

/// Thin wrapper that implements `emu_core::Bus`.
pub struct Atari800xlBus<'a>(pub &'a mut Atari800xlBusInner);

impl Bus for Atari800xlBus<'_> {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr = addr as u16;
        let portb = self.0.effective_portb();
        let has_xl = self.0.model.has_xl_banking();
        let os_rom_enabled = !has_xl || portb & 0x01 != 0;
        let basic_enabled = has_xl && portb & 0x02 == 0; // Bit 1 = 0 means BASIC on
        let self_test = has_xl && portb & 0x80 == 0; // Bit 7 = 0 means self-test on

        let data = match addr {
            // $0000-$3FFF: always base RAM (within ram_size)
            0x0000..=0x3FFF => {
                if (addr as usize) < self.0.ram_size {
                    self.0.ram[addr as usize]
                } else {
                    0xFF
                }
            }

            // $4000-$7FFF: base RAM, extended bank (130XE), or unmapped
            0x4000..=0x4FFF => {
                if let Some(val) = self.0.read_extended(addr - 0x4000, portb) {
                    val
                } else if (addr as usize) < self.0.ram_size {
                    self.0.ram[addr as usize]
                } else {
                    0xFF
                }
            }

            // $5000-$57FF: self-test ROM (XL+), extended bank, or RAM
            0x5000..=0x57FF => {
                if self_test && os_rom_enabled {
                    if let Some(ref os) = self.0.os_rom {
                        let offset = (addr - 0x5000 + 0x1000) as usize;
                        os.get(offset).copied().unwrap_or(0xFF)
                    } else {
                        self.0.ram[addr as usize]
                    }
                } else if let Some(val) = self.0.read_extended(addr - 0x4000, portb) {
                    val
                } else if (addr as usize) < self.0.ram_size {
                    self.0.ram[addr as usize]
                } else {
                    0xFF
                }
            }

            // $5800-$7FFF: extended bank (130XE) or base RAM
            0x5800..=0x7FFF => {
                if let Some(val) = self.0.read_extended(addr - 0x4000, portb) {
                    val
                } else if (addr as usize) < self.0.ram_size {
                    self.0.ram[addr as usize]
                } else {
                    0xFF
                }
            }

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
            // $0000-$3FFF: base RAM
            0x0000..=0x3FFF => {
                if (addr as usize) < self.0.ram_size {
                    self.0.ram[addr as usize] = value;
                }
            }

            // $4000-$7FFF: extended bank (130XE) or base RAM
            0x4000..=0x7FFF => {
                let portb = self.0.effective_portb();
                if !self.0.write_extended(addr - 0x4000, value, portb)
                    && (addr as usize) < self.0.ram_size
                {
                    self.0.ram[addr as usize] = value;
                }
            }

            // $8000-$CFFF: base RAM
            0x8000..=0xCFFF => {
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
        make_bus_model(Atari8bitModel::A800XL)
    }

    fn make_bus_model(model: Atari8bitModel) -> Atari800xlBusInner {
        Atari800xlBusInner {
            ram: [0; 65536],
            ram_size: model.base_ram(),
            extended_ram: vec![0; model.extended_ram()],
            model,
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

    // --- Model-specific tests ---

    #[test]
    fn atari_400_has_16kb_ram() {
        let mut inner = make_bus_model(Atari8bitModel::A400);
        let mut bus = Atari800xlBus(&mut inner);

        // $0000-$3FFF is valid (16KB)
        bus.write(0x3FFF, 0xAB);
        assert_eq!(bus.read(0x3FFF).data, 0xAB);

        // $4000+ is beyond RAM — reads as $FF, writes ignored
        bus.write(0x4000, 0xCD);
        assert_eq!(bus.read(0x4000).data, 0xFF);
    }

    #[test]
    fn atari_800_has_48kb_ram() {
        let mut inner = make_bus_model(Atari8bitModel::A800);
        let mut bus = Atari800xlBus(&mut inner);

        // 48KB: $0000-$BFFF
        bus.write(0xBFFF, 0xAB);
        assert_eq!(bus.read(0xBFFF).data, 0xAB);

        // $C000+ is OS ROM territory (no XL banking — always ROM if loaded)
    }

    #[test]
    fn atari_400_no_xl_banking() {
        let mut inner = make_bus_model(Atari8bitModel::A400);
        inner.os_rom = Some(vec![0xBB; 16384]);

        // On the 400, OS ROM is always visible — no PORTB banking
        let mut bus = Atari800xlBus(&mut inner);
        assert_eq!(bus.read(0xE000).data, 0xBB);

        // Even if we set PORTB bit 0 = 0, OS ROM stays visible
        // (400 has no XL banking)
        inner.pia.write(0x02, 0xFF);
        inner.pia.write(0x03, 0x04);
        inner.pia.write(0x02, 0xFE); // PORTB bit 0 = 0
        let mut bus = Atari800xlBus(&mut inner);
        assert_eq!(bus.read(0xE000).data, 0xBB);
    }

    #[test]
    fn atari_130xe_extended_ram_cpu_bank() {
        let mut inner = make_bus_model(Atari8bitModel::A130XE);
        assert_eq!(inner.extended_ram.len(), 65536); // 4 × 16KB

        // Write to extended bank 0 via PORTB: bits 2-3 = 00, bit 4 = 0 (CPU sees extended)
        inner.pia.write(0x02, 0xFF); // DDR_B all output
        inner.pia.write(0x03, 0x04); // CRB bit 2 = 1
        // PORTB: bit 4 = 0 (CPU extended), bits 2-3 = 00 (bank 0), bit 0 = 1 (OS), bit 7 = 1
        inner.pia.write(0x02, 0xE1); // 1110_0001

        // Write to $4000 (offset 0 in extended bank 0)
        let mut bus = Atari800xlBus(&mut inner);
        bus.write(0x4000, 0xAA);

        // Verify it went to extended RAM, not base RAM
        assert_eq!(inner.extended_ram[0], 0xAA);
        assert_eq!(inner.ram[0x4000], 0x00); // Base RAM untouched

        // Read back
        let mut bus = Atari800xlBus(&mut inner);
        assert_eq!(bus.read(0x4000).data, 0xAA);
    }

    #[test]
    fn atari_130xe_bank_selection() {
        let mut inner = make_bus_model(Atari8bitModel::A130XE);

        // Pre-fill each extended bank with a distinct value
        for bank in 0..4 {
            let start = bank * 16384;
            inner.extended_ram[start] = (bank + 1) as u8;
        }

        inner.pia.write(0x02, 0xFF); // DDR_B all output
        inner.pia.write(0x03, 0x04); // CRB bit 2 = 1

        for bank in 0u8..4 {
            // PORTB: bit 4 = 0 (CPU extended), bits 2-3 = bank, bit 0 = 1
            let portb = 0xE1 | (bank << 2);
            inner.pia.write(0x02, portb);

            let mut bus = Atari800xlBus(&mut inner);
            assert_eq!(
                bus.read(0x4000).data,
                bank + 1,
                "bank {bank} should read {}",
                bank + 1
            );
        }
    }

    #[test]
    fn atari_130xe_cpu_disabled_sees_base_ram() {
        let mut inner = make_bus_model(Atari8bitModel::A130XE);
        inner.extended_ram[0] = 0xEE;
        inner.ram[0x4000] = 0x42;

        // PORTB: bit 4 = 1 (CPU does NOT see extended), bits 2-3 = 00
        inner.pia.write(0x02, 0xFF);
        inner.pia.write(0x03, 0x04);
        inner.pia.write(0x02, 0xF1); // 1111_0001 — bit 4 set

        let mut bus = Atari800xlBus(&mut inner);
        assert_eq!(bus.read(0x4000).data, 0x42); // Base RAM, not extended
    }
}
