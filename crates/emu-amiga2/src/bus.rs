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

use cpu_m68k::{BusResult, FunctionCode, M68kBus};

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
    /// Diagnostic: log of key custom register writes (offset, value, name, source, cpu_pc).
    pub reg_log: Vec<(u16, u16, &'static str, &'static str, u32)>,
    /// CPU PC for diagnostic logging (updated by tick loop).
    pub diag_cpu_pc: u32,
    /// Copper PC for diagnostic logging (updated before Copper tick).
    pub diag_copper_pc: u32,
    /// Disk DMA: DSKLEN register (bit 15 = enable DMA).
    dsklen: u16,
    /// Disk DMA: previous DSKLEN write had bit 15 set (double-write detection).
    dsklen_prev_set: bool,
    /// Disk DMA: pointer register.
    dskpt: u32,
    /// Last observed CIA-B PRB output (for edge detection).
    cia_b_prb_last: u8,
    /// Stub floppy track position (0-79).
    floppy_track: u8,
    /// Stub floppy medium state (false = no disk inserted).
    floppy_inserted: bool,
    /// Diagnostic: COPJMP2 dump counter.
    copjmp2_dump_count: u32,
    /// Diagnostic: CIA-B trace counter.
    #[cfg(debug_assertions)]
    cia_b_trace_count: u32,
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
            cia_a: {
                let mut cia = Cia::new();
                // Floppy input defaults (stub): no disk inserted, at track 0.
                cia.external_a = 0xEB;
                cia
            },
            cia_b: Cia::new(),
            keyboard: Keyboard::new(),
            #[cfg(debug_assertions)]
            trace_count: 0,
            reg_log: Vec::new(),
            diag_cpu_pc: 0,
            diag_copper_pc: 0,
            dsklen: 0,
            dsklen_prev_set: false,
            dskpt: 0,
            cia_b_prb_last: 0xFF,
            floppy_track: 0,
            floppy_inserted: false,
            copjmp2_dump_count: 0,
            #[cfg(debug_assertions)]
            cia_b_trace_count: 0,
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

    /// Pump keyboard: tick the power-up delay and inject bytes into CIA-A SDR.
    ///
    /// On real hardware, the keyboard controller sends data through CIA-A's
    /// serial shift register (SDR), which triggers CIA-A ICR bit 3 (SP).
    /// This propagates to Paula INTREQ bit 3 (PORTS, IPL level 2).
    pub fn pump_keyboard(&mut self) {
        if let Some(byte) = self.keyboard.pump() {
            self.cia_a.queue_serial_byte(byte);
        }
    }

    /// Dispatch a custom register word write (CPU source).
    pub fn write_custom_reg(&mut self, offset: u16, value: u16) {
        self.write_custom_reg_from(offset, value, "cpu");
    }

    /// Dispatch a custom register word write (with source for diagnostics).
    pub fn write_custom_reg_from(&mut self, offset: u16, value: u16, source: &'static str) {
        #[cfg(debug_assertions)]
        {
            if self.trace_count < 200 {
                match offset {
                    0x002E | 0x0096 | 0x009A | 0x009C | 0x0100 | 0x0180 => {
                        let name = match offset {
                            0x002E => "COPCON",
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
        // Log key register writes (limited to avoid memory explosion)
        if self.reg_log.len() < 2000 {
            match offset {
                custom_regs::COP1LCH | custom_regs::COP1LCL
                | custom_regs::COP2LCH | custom_regs::COP2LCL
                | custom_regs::COPJMP1 | custom_regs::COPJMP2
                | custom_regs::COPCON
                | custom_regs::DMACON | custom_regs::BPLCON0 => {
                    let name = match offset {
                        custom_regs::COPCON => "COPCON",
                        custom_regs::COP1LCH => "COP1LCH",
                        custom_regs::COP1LCL => "COP1LCL",
                        custom_regs::COP2LCH => "COP2LCH",
                        custom_regs::COP2LCL => "COP2LCL",
                        custom_regs::COPJMP1 => "COPJMP1",
                        custom_regs::COPJMP2 => "COPJMP2",
                        custom_regs::DMACON => "DMACON",
                        custom_regs::BPLCON0 => "BPLCON0",
                        _ => "?",
                    };
                    let pc = if source == "cop" { self.diag_copper_pc } else { self.diag_cpu_pc };
                    self.reg_log.push((offset, value, name, source, pc));
                }
                _ => {}
            }
        }

        match offset {
            // Beam position write registers
            custom_regs::VPOSW => self.agnus.write_vposw(value),
            custom_regs::VHPOSW => self.agnus.write_vhposw(value),

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
                let new_cop2lc = (self.copper.cop2lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                // Defensive guard: ROM sometimes transiently derives COP2LC from ExecBase
                // when View cprlist pointers are temporarily null. Keep the last valid
                // display list instead of switching to non-Copper data.
                let exec_base = self.diag_read_long(0x0004) & 0x00FF_FFFE;
                if new_cop2lc == exec_base && self.copper.cop2lc >= 0x00002000 {
                    eprintln!(
                        "=== COP2LC guard: ignoring ExecBase pointer ${new_cop2lc:08X} at PC=${:08X} (keeping ${:08X}) ===",
                        self.diag_cpu_pc,
                        self.copper.cop2lc
                    );
                    return;
                }
                // Diagnostic: dump full chain when COP2LC changes to a non-zero value
                if new_cop2lc != 0 && new_cop2lc != self.copper.cop2lc && self.copjmp2_dump_count < 10 {
                    self.copjmp2_dump_count += 1;
                    let pc = self.diag_cpu_pc;
                    eprintln!("=== COP2LC → ${new_cop2lc:08X} (was ${:08X}) from CPU PC=${pc:08X} ===",
                        self.copper.cop2lc);
                    self.dump_cop2lc_chain(new_cop2lc);
                }
                self.copper.cop2lc = new_cop2lc;
            }
            custom_regs::COPJMP1 => self.copper.restart_cop1(),
            custom_regs::COPJMP2 => {
                self.copper.restart_cop2();
            }

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
            custom_regs::BLTSIZE => {
                eprintln!(
                    "  BLTSIZE=${value:04X} PC=${:08X} CON0=${:04X} CON1=${:04X} APT=${:08X} BPT=${:08X} CPT=${:08X} DPT=${:08X} AMOD=${:04X} BMOD=${:04X} CMOD=${:04X} DMOD=${:04X}",
                    self.diag_cpu_pc,
                    self.blitter.bltcon0,
                    self.blitter.bltcon1,
                    self.blitter.bltapt,
                    self.blitter.bltbpt,
                    self.blitter.bltcpt,
                    self.blitter.bltdpt,
                    self.blitter.bltamod,
                    self.blitter.bltbmod,
                    self.blitter.bltcmod,
                    self.blitter.bltdmod,
                );
                self.blitter.bltsize = value;
                self.blitter.do_blit(&mut self.memory);
            }
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
            custom_regs::INTENA => {
                let old = self.paula.intena;
                self.paula.write_intena(value);
                let new_val = self.paula.intena;
                // Diagnostic: catch when SOFT (bit 2) or PORTS (bit 3) gets cleared
                let lost = old & !new_val;
                if lost & 0x000C != 0 {
                    let pc = if source == "cop" { self.diag_copper_pc } else { self.diag_cpu_pc };
                    eprintln!("=== INTENA LOST SOFT/PORTS: ${old:04X} → ${new_val:04X} (write ${value:04X} from {source} PC=${pc:08X}) ===");
                }
            }
            custom_regs::INTREQ => self.paula.write_intreq(value),

            // Disk DMA
            custom_regs::DSKPTH => {
                self.dskpt = (self.dskpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom_regs::DSKPTL => {
                self.dskpt = (self.dskpt & 0xFFFF_0000) | u32::from(value);
            }
            custom_regs::DSKLEN => {
                let enable = value & 0x8000 != 0;
                if enable && self.dsklen_prev_set {
                    // Double-write with bit 15 set — start disk DMA.
                    // No disk present: ignore. No DMA, no interrupt.
                    #[cfg(debug_assertions)]
                    eprintln!("  DISK DMA START: DSKLEN=${value:04X} DSKPT=${:08X} — no disk, ignoring",
                        self.dskpt);
                }
                self.dsklen_prev_set = enable;
                self.dsklen = value;
            }

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
            custom_regs::DMACONR => {
                let busy = if self.blitter.is_busy() { 0x4000 } else { 0 };
                (self.agnus.dmacon & 0x03FF) | busy
            }
            custom_regs::VPOSR => self.agnus.read_vposr(),
            custom_regs::VHPOSR => self.agnus.read_vhposr(),
            custom_regs::JOY0DAT | custom_regs::JOY1DAT | custom_regs::ADKCONR => 0x0000,
            custom_regs::DSKBYTR => 0x0000, // No disk DMA running
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
            let value = self.cia_b.read_icr_and_clear();
            #[cfg(debug_assertions)]
            if self.cia_b_trace_count < 300 {
                eprintln!(
                    "  CIA-B READ ICR -> ${value:02X} PC=${:08X}",
                    self.diag_cpu_pc
                );
                self.cia_b_trace_count += 1;
            }
            value
        } else {
            self.cia_b.read(reg)
        }
    }

    fn write_cia_a(&mut self, addr: u32, value: u8) {
        let reg = ((addr >> 8) & 0x0F) as u8;
        self.cia_a.write(reg, value);

        // CIA-A PRA controls overlay
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
        }

        // CIA-A CRA controls keyboard handshake via serial port direction (bit 6).
        // ROM sets CRA bit 6 = 1 (output mode, pulls KDAT low) then clears it
        // back to 0 (input mode, releases KDAT). The falling edge is the handshake.
        if reg == 0x0E {
            self.keyboard.cia_cra_written(value);
        }
    }

    fn write_cia_b(&mut self, addr: u32, value: u8) {
        let reg = ((addr >> 8) & 0x0F) as u8;
        #[cfg(debug_assertions)]
        let old_status = self.cia_b.icr_status();
        #[cfg(debug_assertions)]
        let old_mask = self.cia_b.icr_mask();
        self.cia_b.write(reg, value);

        #[cfg(debug_assertions)]
        if self.cia_b_trace_count < 300 && (0x08..=0x0F).contains(&reg) {
            eprintln!(
                "  CIA-B WRITE reg=${reg:02X} val=${value:02X} PC=${:08X} ICR ${old_status:02X}/{old_mask:02X} -> ${:02X}/${:02X} TOD=${:06X} ALARM=${:06X} CRA=${:02X} CRB=${:02X}",
                self.diag_cpu_pc,
                self.cia_b.icr_status(),
                self.cia_b.icr_mask(),
                self.cia_b.tod_counter(),
                self.cia_b.tod_alarm(),
                self.cia_b.cra(),
                self.cia_b.crb(),
            );
            self.cia_b_trace_count += 1;
        }

        if reg == 0x01 || reg == 0x03 {
            self.update_floppy_inputs_from_cia_b();
        }
    }

    fn update_floppy_inputs_from_cia_b(&mut self) {
        let prb = self.cia_b.port_b_output();

        // CIA-B PRB disk control lines (active low on real hardware):
        // bit 0 STEP, bit 1 DIR, bit 3 SEL0 (DF0), bit 7 MTR.
        let df0_selected = prb & 0x08 == 0;
        let motor_on = prb & 0x80 == 0;

        // Step pulse on falling edge of STEP while DF0 selected.
        let step_falling = (self.cia_b_prb_last & 0x01) != 0 && (prb & 0x01) == 0;
        if df0_selected && step_falling {
            let dir_outward = prb & 0x02 != 0;
            if dir_outward {
                self.floppy_track = self.floppy_track.saturating_sub(1);
            } else {
                self.floppy_track = self.floppy_track.saturating_add(1).min(79);
            }
        }
        self.cia_b_prb_last = prb;

        // CIA-A PRA input bits:
        // bit 2 /CHNG: 0 when no disk inserted or disk changed
        // bit 3 /WPRO: 1 (not write-protected)
        // bit 4 /TK0:  0 at track 0, 1 otherwise
        // bit 5 /RDY:  0 only when a disk is present, selected, and motor is on
        let mut external = self.cia_a.external_a | 0x3C;
        if self.floppy_inserted {
            external |= 0x04; // /CHNG high after media became current
        } else {
            external &= !0x04; // /CHNG low: no disk inserted
        }
        external |= 0x08; // /WPRO high (no write-protect notch modeled)
        if self.floppy_track == 0 {
            external &= !0x10; // /TK0 low
        } else {
            external |= 0x10; // /TK0 high
        }
        if self.floppy_inserted && df0_selected && motor_on {
            external &= !0x20; // /RDY low (ready)
        } else {
            external |= 0x20; // /RDY high (not ready/no media)
        }
        self.cia_a.external_a = external;
    }

    fn is_cia_a_region(addr: u32) -> bool {
        (addr & 0x00FF_F000) == 0x00BF_E000
    }

    fn is_cia_b_region(addr: u32) -> bool {
        (addr & 0x00FF_F000) == 0x00BF_D000
    }

    fn is_cia_region(addr: u32) -> bool {
        Self::is_cia_a_region(addr) || Self::is_cia_b_region(addr)
    }

    fn is_custom_region(addr: u32) -> bool {
        (addr & 0x00FF_F000) == 0x00DF_F000
    }

    /// Peek at chip RAM without side effects.
    #[must_use]
    pub fn peek_chip_ram(&self, addr: u32) -> u8 {
        self.memory.peek_chip_ram(addr)
    }

    /// Read a 32-bit big-endian value from memory (diagnostic helper).
    fn diag_read_long(&self, addr: u32) -> u32 {
        u32::from(self.memory.read(addr)) << 24
            | u32::from(self.memory.read(addr + 1)) << 16
            | u32::from(self.memory.read(addr + 2)) << 8
            | u32::from(self.memory.read(addr + 3))
    }

    /// Dump the full View→cprlist→start chain for COP2LC diagnostics.
    fn dump_cop2lc_chain(&self, cop2lc_val: u32) {
        // ExecBase from $0004
        let exec_base = self.diag_read_long(0x0004);

        // Find GfxBase by scanning ExecBase library list at offset $17A
        let mut gfx_base = 0u32;
        if exec_base != 0 && exec_base < 0x8_0000 {
            let lib_list = self.diag_read_long(exec_base + 0x17A);
            let mut node = lib_list;
            for _ in 0..30 {
                if node == 0 || node >= 0x8_0000 { break; }
                let name_ptr = self.diag_read_long(node + 10);
                if name_ptr != 0 && name_ptr < 0xFF_FFFF {
                    let c0 = self.memory.read(name_ptr);
                    let c1 = self.memory.read(name_ptr + 1);
                    if c0 == b'g' && c1 == b'r' {
                        gfx_base = node;
                        break;
                    }
                }
                node = self.diag_read_long(node);
            }
        }
        eprintln!("  ExecBase=${exec_base:08X} GfxBase=${gfx_base:08X}");

        if gfx_base == 0 { return; }

        // GfxBase layout (from NDK):
        //   +$22: ActiView (4)   +$26: copinit (4)
        //   +$2A: cia (4)        +$2E: blitter (4)
        //   +$32: LOFlist (4)    +$36: SHFlist (4)
        let actiview = self.diag_read_long(gfx_base + 0x22);
        let copinit = self.diag_read_long(gfx_base + 0x26);
        let loflist = self.diag_read_long(gfx_base + 0x32);
        let shflist = self.diag_read_long(gfx_base + 0x36);
        eprintln!("  GfxBase: ActiView=${actiview:08X} copinit=${copinit:08X} LOFlist=${loflist:08X} SHFlist=${shflist:08X}");

        // Dump View struct
        if actiview != 0 && actiview < 0x8_0000 {
            let vp = self.diag_read_long(actiview);
            let lof_cprlist = self.diag_read_long(actiview + 4);
            let shf_cprlist = self.diag_read_long(actiview + 8);
            let dy = u16::from(self.memory.read(actiview + 12)) << 8
                | u16::from(self.memory.read(actiview + 13));
            let dx = u16::from(self.memory.read(actiview + 14)) << 8
                | u16::from(self.memory.read(actiview + 15));
            let modes = u16::from(self.memory.read(actiview + 16)) << 8
                | u16::from(self.memory.read(actiview + 17));
            eprintln!("  View@${actiview:08X}: VP=${vp:08X} LOFCprList=${lof_cprlist:08X} SHFCprList=${shf_cprlist:08X} Dy={dy} Dx={dx} Modes=${modes:04X}");

            // Dump cprlist struct (struct cprlist { Next(4), start(4), MaxCount(2) })
            for &(label, cpr) in &[("LOFCprList", lof_cprlist), ("SHFCprList", shf_cprlist)] {
                if cpr != 0 && cpr < 0x8_0000 {
                    let next = self.diag_read_long(cpr);
                    let start = self.diag_read_long(cpr + 4);
                    let maxcount = u16::from(self.memory.read(cpr + 8)) << 8
                        | u16::from(self.memory.read(cpr + 9));
                    eprintln!("  {label}@${cpr:08X}: Next=${next:08X} start=${start:08X} MaxCount={maxcount}");

                    // Dump copper instructions at start
                    if start != 0 && start < 0x8_0000 {
                        eprintln!("    Copper instructions at ${start:08X}:");
                        for j in 0..10u32 {
                            let addr = start + j * 4;
                            let w1 = self.memory.read_chip_word(addr);
                            let w2 = self.memory.read_chip_word(addr + 2);
                            let kind = if w1 & 1 == 0 {
                                format!("MOVE ${w2:04X}→reg${:04X}", w1 & 0x01FE)
                            } else if w1 == 0xFFFF && w2 == 0xFFFE {
                                "END".to_string()
                            } else if w2 & 1 == 0 {
                                format!("WAIT v={:02X} h={:02X}", (w1 >> 8) & 0xFF, (w1 >> 1) & 0x7F)
                            } else {
                                "SKIP".to_string()
                            };
                            eprintln!("      ${addr:06X}: {w1:04X} {w2:04X}  {kind}");
                            if w1 == 0xFFFF && w2 == 0xFFFE { break; }
                        }
                    }
                }
            }
        }

        // Dump memory at $23C8 (potential correct copper list from watchpoint)
        eprintln!("  Memory at $23C8:");
        for j in 0..10u32 {
            let addr = 0x23C8 + j * 4;
            let w1 = self.memory.read_chip_word(addr);
            let w2 = self.memory.read_chip_word(addr + 2);
            eprintln!("    ${addr:06X}: {w1:04X} {w2:04X}");
            if w1 == 0xFFFF && w2 == 0xFFFE { break; }
        }

        // Dump memory at COP2LC target
        if cop2lc_val != 0 && cop2lc_val < 0x8_0000 {
            eprintln!("  Memory at COP2LC=${cop2lc_val:08X}:");
            for j in 0..10u32 {
                let addr = cop2lc_val + j * 4;
                let w1 = self.memory.read_chip_word(addr);
                let w2 = self.memory.read_chip_word(addr + 2);
                eprintln!("    ${addr:06X}: {w1:04X} {w2:04X}");
                if w1 == 0xFFFF && w2 == 0xFFFE { break; }
            }
        }

        // Raw hex dump of $2400-$2420 region
        eprintln!("  Raw $2400-$241F:");
        for j in 0..16u32 {
            let addr = 0x2400 + j * 2;
            let w = self.memory.read_chip_word(addr);
            if j % 8 == 0 { eprint!("    ${addr:06X}:"); }
            eprint!(" {w:04X}");
            if j % 8 == 7 { eprintln!(); }
        }
    }

    /// Compute chip RAM contention wait cycles for a CPU access.
    fn chip_ram_wait(&self) -> u8 {
        dma::chip_ram_contention(&self.agnus)
    }

    /// Check if a 24-bit address is unmapped (would cause BERR on real hardware).
    ///
    /// On the Amiga, the Gary chip generates DTACK for mapped regions and
    /// times out (BERR) for unmapped ones. Mapped regions:
    /// - $000000-$1FFFFF: Chip RAM (Agnus generates DTACK)
    /// - $BFD000-$BFEFFF: CIA registers
    /// - $C00000-$C00000+slow_ram_size: Slow (Ranger) RAM
    /// - $DFF000-$DFF1FF: Custom chip registers
    /// - $F80000-$FFFFFF: Kickstart ROM/WCS
    fn is_unmapped(&self, addr: u32) -> bool {
        let addr = addr & 0x00FF_FFFF;
        match addr {
            0x00_0000..=0x1F_FFFF => false, // Chip RAM (Agnus always responds)
            0xBF_D000..=0xBF_EFFF => false, // CIA registers
            0xC0_0000..=0xD7_FFFF => {
                // Slow RAM: only mapped within actual size
                if self.memory.slow_ram.is_empty() {
                    true
                } else {
                    let offset = (addr - 0xC0_0000) as usize;
                    offset >= self.memory.slow_ram.len()
                }
            }
            0xDF_F000..=0xDF_F1FF => false, // Custom registers
            0xF8_0000..=0xFF_FFFF => false, // Kickstart ROM/WCS
            _ => true,                       // Everything else: unmapped → BERR
        }
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

        if Self::is_cia_a_region(addr) {
            // CIA-A is on the odd byte lane in the BFE region.
            let lo = self.read_cia_a(addr | 1);
            return BusResult::new(0xFF00 | u16::from(lo));
        }
        if Self::is_cia_b_region(addr) {
            // CIA-B is on the even byte lane in the BFD region.
            let hi = self.read_cia_b(addr);
            return BusResult::new((u16::from(hi) << 8) | 0x00FF);
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
            eprintln!(
                "  BUS WRITE_WORD ${addr:06X} = ${value:04X} (overlay={} PC=${:08X})",
                self.memory.overlay,
                self.diag_cpu_pc
            );
        }

        if Self::is_cia_a_region(addr) {
            // Only the odd byte lane is connected for CIA-A.
            self.write_cia_a(addr | 1, value as u8);
            return BusResult::write_ok();
        }
        if Self::is_cia_b_region(addr) {
            // Only the even byte lane is connected for CIA-B.
            self.write_cia_b(addr, (value >> 8) as u8);
            return BusResult::write_ok();
        }

        if Self::is_custom_region(addr) {
            let offset = (addr & 0x01FE) as u16;
            self.write_custom_reg(offset, value);
            return BusResult::write_ok();
        }

        if addr < 0x20_0000 {
            let wait = self.chip_ram_wait();
            if let Some(wa) = self.memory.watch_addr {
                if addr >= wa && addr < wa + 32 {
                    eprintln!("  WATCH-BUS: write_word ${addr:06X} = ${value:04X} PC=${:08X}", self.diag_cpu_pc);
                }
            }
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

        if Self::is_cia_a_region(addr) {
            if addr & 1 != 0 {
                return BusResult::new(u16::from(self.read_cia_a(addr)));
            }
            return BusResult::new(0x00FF);
        }
        if Self::is_cia_b_region(addr) {
            if addr & 1 == 0 {
                return BusResult::new(u16::from(self.read_cia_b(addr)));
            }
            return BusResult::new(0x00FF);
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

        if Self::is_cia_a_region(addr) {
            if addr & 1 != 0 {
                self.write_cia_a(addr, value);
            }
            return BusResult::write_ok();
        }
        if Self::is_cia_b_region(addr) {
            if addr & 1 == 0 {
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
            eprintln!(
                "  CUSTOM BYTE WRITE addr=${addr:06X} off=${offset:04X} val=${value:02X} word=${word:04X} PC=${:08X}",
                self.diag_cpu_pc
            );
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
        self.cia_a.external_a = 0xEB;
        self.cia_b.reset();
        self.cia_b_prb_last = 0xFF;
        self.floppy_track = 0;
        self.floppy_inserted = false;
        #[cfg(debug_assertions)]
        {
            self.cia_b_trace_count = 0;
        }
        self.agnus = Agnus::new(self.agnus.variant(), self.agnus.region());
        self.denise = Denise::new(crate::config::DeniseVariant::Denise8362);
        self.paula = Paula::new();
        self.copper = Copper::new();
        self.blitter = Blitter::new();
        self.keyboard.reset();
        // RESET re-asserts the startup mapping path in ROM boot code.
        // If overlay stays off here, ROM's RESET vector trampoline jumps into
        // ExecBase data at $000004 instead of the Kickstart restart entry.
        self.memory.set_overlay();
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
