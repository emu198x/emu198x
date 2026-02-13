//! Amiga system bus implementing M68kBus (word-level, function codes, wait states).
//!
//! This is the key architectural difference from emu-amiga: the bus speaks
//! the 68000's native word-level protocol and returns wait cycles for chip
//! RAM contention, rather than gating the CPU at system level.
//!
//! Memory map (24-bit address bus):
//! - $000000-$1FFFFF: Chip RAM / ROM overlay
//! - $BFD000-$BFEFFF: CIA-A (odd bytes) / CIA-B (even bytes)
//! - $DFF000-$DFF1FF: Custom chip registers
//! - $C00000-$D7FFFF: Slow (Ranger) RAM
//! - $F80000-$FFFFFF: Kickstart ROM/WCS

#![allow(clippy::cast_possible_truncation)]

use emu_m68k::bus::{BusResult, FunctionCode, M68kBus};

use crate::agnus::Agnus;
use crate::agnus::dma;
use crate::blitter::Blitter;
use crate::cia::Cia;
use crate::copper::Copper;
use crate::custom_regs;
use crate::denise::Denise;
use crate::keyboard::Keyboard;
use crate::memory::Memory;
use crate::paula::Paula;

/// Amiga system bus.
pub struct AmigaBus {
    pub memory: Memory,
    pub agnus: Agnus,
    pub denise: Denise,
    pub paula: Paula,
    pub copper: Copper,
    pub blitter: Blitter,
    pub cia_a: Cia,
    pub cia_b: Cia,
    pub keyboard: Keyboard,
    /// Debug: trace counter for custom register writes.
    #[cfg(debug_assertions)]
    trace_count: u32,
}

impl AmigaBus {
    /// Create a new Amiga bus from the given components.
    pub fn new(
        memory: Memory,
        agnus: Agnus,
        denise: Denise,
    ) -> Self {
        Self {
            memory,
            agnus,
            denise,
            paula: Paula::new(),
            copper: Copper::new(),
            blitter: Blitter::new(),
            cia_a: Cia::new(),
            cia_b: Cia::new(),
            keyboard: Keyboard::new(),
            #[cfg(debug_assertions)]
            trace_count: 0,
        }
    }

    /// Read a word from chip RAM (for DMA).
    #[must_use]
    pub fn read_chip_word(&self, addr: u32) -> u16 {
        self.memory.read_chip_word(addr)
    }

    /// Queue a raw keyboard keycode.
    pub fn queue_keyboard_raw(&mut self, code: u8, pressed: bool) {
        self.keyboard.queue_raw(code, pressed);
    }

    /// Pump keyboard: try to inject next byte into Paula serial buffer.
    pub fn pump_keyboard(&mut self) {
        if self.paula.serial_rx_empty() {
            if let Some(byte) = self.keyboard.try_send() {
                self.paula.queue_serial_rx(byte);
            }
        }
    }

    /// Dispatch a custom register word write.
    pub fn write_custom_reg(&mut self, offset: u16, value: u16) {
        #[cfg(debug_assertions)]
        {
            if self.trace_count < 200 {
                match offset {
                    0x0096 | 0x009A | 0x009C | 0x0100 | 0x0180 => {
                        let name = match offset {
                            0x0096 => "DMACON",
                            0x009A => "INTENA",
                            0x009C => "INTREQ",
                            0x0100 => "BPLCON0",
                            0x0180 => "COLOR00",
                            _ => "?",
                        };
                        eprintln!("  CUSTOM: {name}=${value:04X} (write #{n})", n = self.trace_count);
                        self.trace_count += 1;
                    }
                    _ => {}
                }
            }
        }
        match offset {
            // Copper
            custom_regs::COPCON => {
                self.copper.danger = value & 0x02 != 0;
            }
            custom_regs::COP1LCH => {
                self.copper.cop1lc =
                    (self.copper.cop1lc & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::COP1LCL => {
                self.copper.cop1lc =
                    (self.copper.cop1lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
            }
            custom_regs::COP2LCH => {
                self.copper.cop2lc =
                    (self.copper.cop2lc & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::COP2LCL => {
                self.copper.cop2lc =
                    (self.copper.cop2lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
            }
            custom_regs::COPJMP1 => self.copper.restart_cop1(),
            custom_regs::COPJMP2 => self.copper.restart_cop2(),

            // Blitter (stub)
            custom_regs::BLTCON0 => self.blitter.bltcon0 = value,
            custom_regs::BLTCON1 => self.blitter.bltcon1 = value,
            custom_regs::BLTAFWM => self.blitter.bltafwm = value,
            custom_regs::BLTALWM => self.blitter.bltalwm = value,
            custom_regs::BLTCPTH => {
                self.blitter.bltcpt =
                    (self.blitter.bltcpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::BLTCPTL => {
                self.blitter.bltcpt =
                    (self.blitter.bltcpt & 0xFFFF_0000) | u32::from(value);
            }
            custom_regs::BLTBPTH => {
                self.blitter.bltbpt =
                    (self.blitter.bltbpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::BLTBPTL => {
                self.blitter.bltbpt =
                    (self.blitter.bltbpt & 0xFFFF_0000) | u32::from(value);
            }
            custom_regs::BLTAPTH => {
                self.blitter.bltapt =
                    (self.blitter.bltapt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::BLTAPTL => {
                self.blitter.bltapt =
                    (self.blitter.bltapt & 0xFFFF_0000) | u32::from(value);
            }
            custom_regs::BLTDPTH => {
                self.blitter.bltdpt =
                    (self.blitter.bltdpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::BLTDPTL => {
                self.blitter.bltdpt =
                    (self.blitter.bltdpt & 0xFFFF_0000) | u32::from(value);
            }
            custom_regs::BLTSIZE => self.blitter.bltsize = value,
            custom_regs::BLTCMOD => self.blitter.bltcmod = value,
            custom_regs::BLTBMOD => self.blitter.bltbmod = value,
            custom_regs::BLTAMOD => self.blitter.bltamod = value,
            custom_regs::BLTDMOD => self.blitter.bltdmod = value,
            custom_regs::BLTCDAT => self.blitter.bltcdat = value,
            custom_regs::BLTBDAT => self.blitter.bltbdat = value,
            custom_regs::BLTADAT => self.blitter.bltadat = value,

            // Display window
            custom_regs::DIWSTRT => self.agnus.diwstrt = value,
            custom_regs::DIWSTOP => self.agnus.diwstop = value,
            custom_regs::DDFSTRT => self.agnus.ddfstrt = value,
            custom_regs::DDFSTOP => self.agnus.ddfstop = value,

            // DMA control
            custom_regs::DMACON => self.agnus.write_dmacon(value),

            // Interrupt control
            custom_regs::INTENA => self.paula.write_intena(value),
            custom_regs::INTREQ => self.paula.write_intreq(value),

            // Serial port (stub)
            custom_regs::SERDAT => {
                self.paula.request_interrupt(0); // TBE
            }
            custom_regs::SERPER => {}

            // Bitplane pointers
            custom_regs::BPL1PTH => self.set_bpl_pth(0, value),
            custom_regs::BPL1PTL => self.set_bpl_ptl(0, value),
            custom_regs::BPL2PTH => self.set_bpl_pth(1, value),
            custom_regs::BPL2PTL => self.set_bpl_ptl(1, value),
            custom_regs::BPL3PTH => self.set_bpl_pth(2, value),
            custom_regs::BPL3PTL => self.set_bpl_ptl(2, value),
            custom_regs::BPL4PTH => self.set_bpl_pth(3, value),
            custom_regs::BPL4PTL => self.set_bpl_ptl(3, value),
            custom_regs::BPL5PTH => self.set_bpl_pth(4, value),
            custom_regs::BPL5PTL => self.set_bpl_ptl(4, value),
            custom_regs::BPL6PTH => self.set_bpl_pth(5, value),
            custom_regs::BPL6PTL => self.set_bpl_ptl(5, value),

            // Bitplane control
            custom_regs::BPLCON0 => {
                self.denise.bplcon0 = value;
                let num_bpl = self.denise.num_bitplanes();
                self.agnus.set_num_bitplanes(num_bpl);
            }
            custom_regs::BPLCON1 => self.denise.bplcon1 = value,
            custom_regs::BPLCON2 => self.denise.bplcon2 = value,

            // Bitplane modulo
            custom_regs::BPL1MOD => self.agnus.bpl1mod = value,
            custom_regs::BPL2MOD => self.agnus.bpl2mod = value,

            // Bitplane data latches
            custom_regs::BPL1DAT => self.denise.load_bitplane(0, value),
            custom_regs::BPL2DAT => self.denise.load_bitplane(1, value),
            custom_regs::BPL3DAT => self.denise.load_bitplane(2, value),
            custom_regs::BPL4DAT => self.denise.load_bitplane(3, value),
            custom_regs::BPL5DAT => self.denise.load_bitplane(4, value),
            custom_regs::BPL6DAT => self.denise.load_bitplane(5, value),

            // Colour palette: $180-$1BE (32 colours)
            off @ 0x180..=0x1BE => {
                let colour_idx = ((off - 0x180) / 2) as usize;
                if colour_idx < 32 {
                    self.denise.palette[colour_idx] = value & 0x0FFF;
                }
            }

            _ => {}
        }
    }

    /// Read a custom register word.
    #[allow(clippy::match_same_arms)]
    fn read_custom_reg(&mut self, offset: u16) -> u16 {
        match offset {
            custom_regs::DMACONR => self.agnus.dmacon & 0x03FF,
            custom_regs::VPOSR => self.agnus.read_vposr(),
            custom_regs::VHPOSR => self.agnus.read_vhposr(),
            custom_regs::JOY0DAT | custom_regs::JOY1DAT | custom_regs::ADKCONR => 0x0000,
            custom_regs::POTGOR => 0xFF00,
            custom_regs::SERDATR => self.paula.read_serdatr(),
            custom_regs::INTENAR => self.paula.intena,
            custom_regs::INTREQR => self.paula.intreq,
            _ => 0x0000,
        }
    }

    fn set_bpl_pth(&mut self, plane: usize, value: u16) {
        self.agnus.bpl_pt[plane] =
            (self.agnus.bpl_pt[plane] & 0x0000_FFFF) | (u32::from(value) << 16);
    }

    fn set_bpl_ptl(&mut self, plane: usize, value: u16) {
        self.agnus.bpl_pt[plane] =
            (self.agnus.bpl_pt[plane] & 0xFFFF_0000) | u32::from(value & 0xFFFE);
    }

    fn read_cia_a(&mut self, addr: u32) -> u8 {
        let reg = ((addr >> 8) & 0x0F) as u8;
        if reg == 0x0D {
            self.cia_a.read_icr_and_clear()
        } else {
            self.cia_a.read(reg)
        }
    }

    fn read_cia_b(&mut self, addr: u32) -> u8 {
        let reg = ((addr >> 8) & 0x0F) as u8;
        if reg == 0x0D {
            self.cia_b.read_icr_and_clear()
        } else {
            self.cia_b.read(reg)
        }
    }

    fn write_cia_a(&mut self, addr: u32, value: u8) {
        let reg = ((addr >> 8) & 0x0F) as u8;
        self.cia_a.write(reg, value);

        // CIA-A PRA controls overlay and keyboard handshake
        if reg == 0x00 || reg == 0x02 {
            let output = self.cia_a.port_a_output();

            // Overlay control: OVL (PRA bit 0) directly maps ROM to $0.
            // OVL = 1 → overlay ACTIVE (ROM at $0)
            // OVL = 0 → overlay INACTIVE (chip RAM at $0)
            // At power-on, pull-up holds OVL=1 (overlay active for reset vectors).
            // KS clears bit 0 to expose chip RAM.
            if output & 0x01 != 0 {
                self.memory.set_overlay();
            } else {
                self.memory.clear_overlay();
            }

            // Keyboard handshake
            if let Some(byte) = self.keyboard.cia_pra_written(output) {
                self.paula.queue_serial_rx(byte);
            }
        }
    }

    fn write_cia_b(&mut self, addr: u32, value: u8) {
        let reg = ((addr >> 8) & 0x0F) as u8;
        self.cia_b.write(reg, value);
    }

    fn is_cia_region(addr: u32) -> bool {
        let masked = addr & 0x00FF_F000;
        masked == 0x00BF_D000 || masked == 0x00BF_E000
    }

    fn is_custom_region(addr: u32) -> bool {
        (addr & 0x00FF_F000) == 0x00DF_F000
    }

    /// Peek at chip RAM without side effects.
    #[must_use]
    pub fn peek_chip_ram(&self, addr: u32) -> u8 {
        self.memory.peek_chip_ram(addr)
    }

    /// Compute chip RAM contention wait cycles for a CPU access.
    fn chip_ram_wait(&self) -> u8 {
        dma::chip_ram_contention(&self.agnus)
    }
}

impl M68kBus for AmigaBus {
    fn read_word(&mut self, addr: u32, _fc: FunctionCode) -> BusResult {
        let addr = addr & 0x00FF_FFFE; // 24-bit, word-aligned

        #[cfg(debug_assertions)]
        if addr < 8 && !self.memory.overlay {
            let offset = (addr & self.memory.chip_ram_mask) as usize;
            let hi = self.memory.chip_ram[offset];
            let lo = self.memory.chip_ram[offset + 1];
            let val = u16::from(hi) << 8 | u16::from(lo);
            eprintln!("  BUS READ_WORD ${addr:06X} → chip_ram[{offset:06X}] = ${val:04X}");
        }

        if Self::is_cia_region(addr) {
            // CIA: read both odd (CIA-A) and even (CIA-B) bytes
            let hi = self.read_cia_b(addr);
            let lo = self.read_cia_a(addr | 1);
            return BusResult::new(u16::from(hi) << 8 | u16::from(lo));
        }

        if Self::is_custom_region(addr) {
            let offset = (addr & 0x01FE) as u16;
            let word = self.read_custom_reg(offset);
            return BusResult::new(word);
        }

        // Chip RAM: add contention wait cycles
        if addr < 0x20_0000 {
            let wait = self.chip_ram_wait();
            let hi = self.memory.read(addr);
            let lo = self.memory.read(addr | 1);
            return BusResult::with_wait(u16::from(hi) << 8 | u16::from(lo), wait);
        }

        // All other memory (slow RAM, ROM)
        let hi = self.memory.read(addr);
        let lo = self.memory.read(addr | 1);
        BusResult::new(u16::from(hi) << 8 | u16::from(lo))
    }

    fn write_word(&mut self, addr: u32, value: u16, _fc: FunctionCode) -> BusResult {
        let addr = addr & 0x00FF_FFFE;

        #[cfg(debug_assertions)]
        if addr < 8 {
            eprintln!("  BUS WRITE_WORD ${addr:06X} = ${value:04X} (overlay={})", self.memory.overlay);
        }

        if Self::is_cia_region(addr) {
            self.write_cia_b(addr, (value >> 8) as u8);
            self.write_cia_a(addr | 1, value as u8);
            return BusResult::write_ok();
        }

        if Self::is_custom_region(addr) {
            let offset = (addr & 0x01FE) as u16;
            self.write_custom_reg(offset, value);
            return BusResult::write_ok();
        }

        if addr < 0x20_0000 {
            let wait = self.chip_ram_wait();
            self.memory.write(addr, (value >> 8) as u8);
            self.memory.write(addr | 1, value as u8);
            return BusResult::write_wait(wait);
        }

        self.memory.write(addr, (value >> 8) as u8);
        self.memory.write(addr | 1, value as u8);
        BusResult::write_ok()
    }

    fn read_byte(&mut self, addr: u32, _fc: FunctionCode) -> BusResult {
        let addr = addr & 0x00FF_FFFF;

        if Self::is_cia_region(addr) {
            let data = if addr & 1 != 0 {
                self.read_cia_a(addr)
            } else {
                self.read_cia_b(addr)
            };
            return BusResult::new(u16::from(data));
        }

        if Self::is_custom_region(addr) {
            let offset = (addr & 0x01FE) as u16;
            let word = self.read_custom_reg(offset);
            let data = if addr & 1 == 0 {
                (word >> 8) as u8
            } else {
                word as u8
            };
            return BusResult::new(u16::from(data));
        }

        if addr < 0x20_0000 {
            let wait = self.chip_ram_wait();
            return BusResult::with_wait(u16::from(self.memory.read(addr)), wait);
        }

        BusResult::new(u16::from(self.memory.read(addr)))
    }

    fn write_byte(&mut self, addr: u32, value: u8, _fc: FunctionCode) -> BusResult {
        let addr = addr & 0x00FF_FFFF;

        if Self::is_cia_region(addr) {
            if addr & 1 != 0 {
                self.write_cia_a(addr, value);
            } else {
                self.write_cia_b(addr, value);
            }
            return BusResult::write_ok();
        }

        if Self::is_custom_region(addr) {
            // Byte writes to custom registers: the 68000 does a word write
            // for byte operations, but only the addressed byte carries data.
            // We handle this by routing through write_custom_reg with the byte
            // in the appropriate position.
            let offset = (addr & 0x01FE) as u16;
            let word = if addr & 1 == 0 {
                u16::from(value) << 8
            } else {
                u16::from(value)
            };
            self.write_custom_reg(offset, word);
            return BusResult::write_ok();
        }

        if addr < 0x20_0000 {
            let wait = self.chip_ram_wait();
            self.memory.write(addr, value);
            return BusResult::write_wait(wait);
        }

        self.memory.write(addr, value);
        BusResult::write_ok()
    }

    fn reset(&mut self) {
        #[cfg(debug_assertions)]
        {
            // Trace: what's at $0004 (ExecBase) in chip RAM?
            let eb0 = self.memory.chip_ram[4];
            let eb1 = self.memory.chip_ram[5];
            let eb2 = self.memory.chip_ram[6];
            let eb3 = self.memory.chip_ram[7];
            let exec_base = u32::from(eb0) << 24 | u32::from(eb1) << 16
                | u32::from(eb2) << 8 | u32::from(eb3);
            eprintln!("  RESET: ExecBase@$0004={exec_base:08X} overlay={}", self.memory.overlay);
        }
        self.cia_a.reset();
        self.cia_b.reset();
        self.agnus = Agnus::new(self.agnus.variant(), self.agnus.region());
        self.denise = Denise::new(crate::config::DeniseVariant::Denise8362);
        self.paula = Paula::new();
        self.copper = Copper::new();
        self.blitter = Blitter::new();
        self.keyboard.reset();
        // NOTE: Overlay is NOT re-asserted here. On a real A500/A2000+, the
        // /OVL line has a pull-up that keeps overlay OFF after CIA reset
        // (DDR→0 = input, pull-up = high = overlay off). Overlay is only
        // forced ON at cold boot (power-on), handled in Amiga::new().
    }

    fn interrupt_ack(&mut self, level: u8) -> u8 {
        // Amiga uses autovectors
        24 + level
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AmigaConfig, AmigaModel, AgnusVariant, Chipset, CpuVariant,
        DeniseVariant, KickstartSource, Region,
    };

    fn make_bus() -> AmigaBus {
        let config = AmigaConfig {
            model: AmigaModel::A1000,
            chipset: Chipset::Ocs,
            agnus: AgnusVariant::Agnus8361,
            denise: DeniseVariant::Denise8362,
            cpu: CpuVariant::M68000,
            region: Region::Pal,
            chip_ram_size: 256 * 1024,
            slow_ram_size: 0,
            fast_ram_size: 0,
            kickstart: KickstartSource::Wcs(vec![0u8; 256 * 1024]),
        };
        let memory = Memory::new(&config).expect("valid");
        let agnus = Agnus::new(config.agnus, config.region);
        let denise = Denise::new(config.denise);
        AmigaBus::new(memory, agnus, denise)
    }

    #[test]
    fn chip_ram_read_write_word() {
        let mut bus = make_bus();
        bus.memory.clear_overlay();
        let fc = FunctionCode::SupervisorData;
        bus.write_word(0x100, 0xABCD, fc);
        let result = bus.read_word(0x100, fc);
        assert_eq!(result.data, 0xABCD);
    }

    #[test]
    fn custom_reg_word_write() {
        let mut bus = make_bus();
        let fc = FunctionCode::SupervisorData;
        // Write COLOR00 = $0F00
        bus.write_word(0xDFF180, 0x0F00, fc);
        assert_eq!(bus.denise.palette[0], 0x0F00);
    }

    #[test]
    fn custom_reg_word_read() {
        let mut bus = make_bus();
        bus.agnus.vpos = 0x2C;
        bus.agnus.hpos = 0x40;
        let fc = FunctionCode::SupervisorData;
        let result = bus.read_word(0xDFF006, fc);
        assert_eq!(result.data, 0x2C40);
    }

    #[test]
    fn cia_a_overlay_control() {
        let mut bus = make_bus();
        assert!(bus.memory.overlay);
        let fc = FunctionCode::SupervisorData;
        // Set DDR for bits 0-1 output
        bus.write_byte(0xBFE201, 0x03, fc);
        // Write PRA bit 0 = 1 → OVL HIGH → overlay ACTIVE (ROM at $0)
        bus.write_byte(0xBFE001, 0x01, fc);
        assert!(bus.memory.overlay);
        // Write PRA bit 0 = 0 → OVL LOW → overlay INACTIVE (chip RAM at $0)
        bus.write_byte(0xBFE001, 0x00, fc);
        assert!(!bus.memory.overlay);
    }
}
