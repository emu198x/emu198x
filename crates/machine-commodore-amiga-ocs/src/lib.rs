//! Commodore Amiga (OCS chipset) machine — incremental restart.
//!
//! Built milestone-by-milestone per
//! `wiki/decisions/amiga-restart-plan.md`. Each milestone adds the
//! minimum hardware behaviour the running ROM demands; nothing more.
//!
//! Current milestone: **M6 — beam counter + VBL interrupt.**

mod agnus;
mod chipset;
mod cia;
mod copper;
mod denise;
mod memory;

pub use agnus::{Agnus, PAL_FRAME_LINES, PAL_LINE_CCKS};
pub use chipset::Chipset;
pub use cia::Cia;
pub use copper::Copper;
pub use denise::{Denise, FB_HEIGHT, FB_WIDTH};
pub use memory::{Memory, CHIP_RAM_SIZE};

use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::State;
use motorola_68000::Cpu68000;

const CUSTOM_BASE: u32 = 0x00DF_0000;
const CUSTOM_TOP: u32 = 0x00E0_0000;

/// CIA E clock divider — CIAs tick once per 10 CCKs.
const CIA_E_CLOCK_DIVISOR: u64 = 10;

/// Amiga (OCS) machine.
pub struct AmigaOcs {
    cpu: Cpu68000,
    memory: Memory,
    chipset: Chipset,
    cia_a: Cia,
    cia_b: Cia,
    agnus: Agnus,
    copper: Copper,
    denise: Denise,
    cck_count: u64,
    e_clock_phase: u64,
    /// Diagnostic: count of unique custom-register read offsets seen
    /// since reset, indexed by offset / 2.
    pub debug_reg_read_counts: std::collections::HashMap<u16, u64>,
    /// Diagnostic: peak INTENA value seen during boot. Bit 14 set
    /// here would prove the boot has reached the master-enable code
    /// path even if INTENA is later cleared.
    pub debug_peak_intena: u16,
    /// Diagnostic: cumulative count of CPU writes to INTENA ($DFF09A).
    pub debug_intena_writes: u64,
    /// Diagnostic: per-write log of every INTENA store, captured to
    /// help trace the master-enable lifecycle. Each entry is
    /// `(cck, pc, written_word, intena_before, intena_after)`. Only
    /// writes that actually change INTENA are kept (purely-no-op
    /// writes still count toward `debug_intena_writes`).
    pub debug_intena_log: Vec<(u64, u32, u16, u16, u16)>,
}

impl AmigaOcs {
    /// Build a new Amiga (OCS) with the given Kickstart ROM image
    /// and chip RAM only (no expansion).
    #[must_use]
    pub fn new(kickstart: Vec<u8>) -> Self {
        Self::with_slow_ram(kickstart, 0)
    }

    /// Build a new Amiga (OCS) with the given Kickstart ROM image
    /// plus a trapdoor slow-RAM expansion at `$C00000` (common A500
    /// config: 512 KiB).
    #[must_use]
    pub fn with_slow_ram(kickstart: Vec<u8>, slow_ram_bytes: usize) -> Self {
        let memory = Memory::new_with_slow_ram(kickstart, slow_ram_bytes);
        let mut cpu = Cpu68000::new();
        let ssp = memory.read_long(0x000000);
        let pc = memory.read_long(0x000004);
        cpu.reset_to(ssp, pc);
        Self {
            cpu,
            memory,
            chipset: Chipset::new(),
            cia_a: Cia::new(),
            cia_b: Cia::new(),
            agnus: Agnus::new(),
            copper: Copper::new(),
            denise: Denise::new(),
            cck_count: 0,
            e_clock_phase: 0,
            debug_reg_read_counts: std::collections::HashMap::new(),
            debug_peak_intena: 0,
            debug_intena_writes: 0,
            debug_intena_log: Vec::new(),
        }
    }

    /// Read-only Agnus access.
    #[must_use]
    pub fn agnus(&self) -> &Agnus {
        &self.agnus
    }

    /// Read-only Copper access.
    #[must_use]
    pub fn copper(&self) -> &Copper {
        &self.copper
    }

    /// Read-only Denise access.
    #[must_use]
    pub fn denise(&self) -> &Denise {
        &self.denise
    }

    /// Read-only chipset access.
    #[must_use]
    pub fn chipset(&self) -> &Chipset {
        &self.chipset
    }

    /// Read-only memory access (for tests inspecting OVL state etc.).
    #[must_use]
    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    /// Read-only CIA-A access.
    #[must_use]
    pub fn cia_a(&self) -> &Cia {
        &self.cia_a
    }

    /// Read-only CIA-B access.
    #[must_use]
    pub fn cia_b(&self) -> &Cia {
        &self.cia_b
    }

    /// Convenience: CIA-A PRA byte.
    #[must_use]
    pub fn cia_a_pra(&self) -> u8 {
        self.cia_a.pra
    }

    /// Convenience: CIA-A DDRA byte.
    #[must_use]
    pub fn cia_a_ddra(&self) -> u8 {
        self.cia_a.ddra
    }

    /// Convenience: current INTENA value.
    #[must_use]
    pub fn intena(&self) -> u16 {
        self.chipset.intena
    }

    /// Convenience: current INTREQ value.
    #[must_use]
    pub fn intreq(&self) -> u16 {
        self.chipset.intreq
    }

    /// Convenience: current DMACON value.
    #[must_use]
    pub fn dmacon(&self) -> u16 {
        self.chipset.dmacon
    }

    /// Convenience: current BPLCON0 value.
    #[must_use]
    pub fn bplcon0(&self) -> u16 {
        self.chipset.bplcon0
    }

    /// Convenience: a colour table entry.
    #[must_use]
    pub fn color(&self, idx: usize) -> u16 {
        self.chipset.color[idx]
    }

    /// Backdoor for tests: write a word as if the CPU did it.
    pub fn poke_word(&mut self, addr: u32, val: u16) {
        if (CUSTOM_BASE..CUSTOM_TOP).contains(&addr) {
            let offset = (addr - CUSTOM_BASE) as u16 & 0x1FE;
            self.dispatch_custom_write(offset, val);
        } else {
            self.memory.write_word(addr, val);
        }
    }

    /// Dispatch a custom-register word write to the right submodule.
    /// Shared between `poke_word` and the CPU bus servicer.
    fn dispatch_custom_write(&mut self, offset: u16, val: u16) {
        let intena_before = self.chipset.intena;
        match offset {
            0x080 => {
                self.copper.cop1lc =
                    (self.copper.cop1lc & 0x0000_FFFF) | (u32::from(val) << 16);
            }
            0x082 => {
                self.copper.cop1lc =
                    (self.copper.cop1lc & 0xFFFF_0000) | u32::from(val & 0xFFFE);
            }
            0x084 => {
                self.copper.cop2lc =
                    (self.copper.cop2lc & 0x0000_FFFF) | (u32::from(val) << 16);
            }
            0x086 => {
                self.copper.cop2lc =
                    (self.copper.cop2lc & 0xFFFF_0000) | u32::from(val & 0xFFFE);
            }
            0x088 => self.copper.jump1(),
            0x08A => self.copper.jump2(),
            _ => self.chipset.write_word(offset, val),
        }
        if offset == 0x09A {
            self.debug_intena_writes += 1;
            let intena_after = self.chipset.intena;
            if intena_after > self.debug_peak_intena {
                self.debug_peak_intena = intena_after;
            }
            if intena_after != intena_before {
                self.debug_intena_log.push((
                    self.cck_count,
                    self.cpu.regs.pc,
                    val,
                    intena_before,
                    intena_after,
                ));
            }
        }
    }

    /// Backdoor for tests: write a byte as if the CPU did it.
    pub fn poke_byte(&mut self, addr: u32, val: u8) {
        if let Some(reg) = cia::decode_cia_a(addr) {
            self.cia_a.write_register(reg, val);
            self.memory.set_overlay(self.cia_a.ovl());
        } else if let Some(reg) = cia::decode_cia_b(addr) {
            self.cia_b.write_register(reg, val);
        } else if (CUSTOM_BASE..CUSTOM_TOP).contains(&addr) {
            // Custom registers are word-only; byte writes pad with
            // the same byte in both halves on real hardware. For our
            // purposes a byte write just writes the byte value.
            let offset = (addr - CUSTOM_BASE) as u16 & 0x1FE;
            self.chipset.write_word(offset, u16::from(val) << 8 | u16::from(val));
        } else {
            self.memory.write_byte(addr, val);
        }
    }

    /// CPU access (read-only — mutating outside the tick loop breaks
    /// invariants).
    #[must_use]
    pub fn cpu(&self) -> &Cpu68000 {
        &self.cpu
    }

    /// Total CCKs (colour clocks) elapsed since construction.
    #[must_use]
    pub fn cck_count(&self) -> u64 {
        self.cck_count
    }

    /// Read a word at the given 24-bit address — peeks state without
    /// side effects (does NOT clear ICR etc). For inspecting state
    /// during tests; not equivalent to a CPU bus cycle.
    #[must_use]
    pub fn read_word(&self, addr: u32) -> u16 {
        self.bus_read_word(addr & 0xFF_FFFF)
    }

    /// Read a word as if the CPU did the bus cycle. Side-effecting:
    /// CIA-A ICR reads clear ICR; future read-side-effect registers
    /// behave like the CPU sees them.
    pub fn cpu_read_word(&mut self, addr: u32) -> u16 {
        let addr24 = addr & 0xFF_FFFF;
        if let Some(reg) = cia::decode_cia_a(addr24) {
            return u16::from(self.cia_a.read_register(reg));
        }
        if let Some(reg) = cia::decode_cia_b(addr24) {
            return u16::from(self.cia_b.read_register(reg));
        }
        self.bus_read_word(addr24)
    }

    /// Read a longword (big-endian) at the given 24-bit address.
    #[must_use]
    pub fn read_long(&self, addr: u32) -> u32 {
        let hi = self.bus_read_word(addr & 0xFF_FFFF);
        let lo = self.bus_read_word(addr.wrapping_add(2) & 0xFF_FFFF);
        (u32::from(hi) << 16) | u32::from(lo)
    }

    fn bus_read_word(&self, addr24: u32) -> u16 {
        if let Some(reg) = cia::decode_cia_a(addr24) {
            return u16::from(self.cia_a.peek_register(reg));
        }
        if let Some(reg) = cia::decode_cia_b(addr24) {
            return u16::from(self.cia_b.peek_register(reg));
        }
        if (CUSTOM_BASE..CUSTOM_TOP).contains(&addr24) {
            let offset = (addr24 - CUSTOM_BASE) as u16 & 0x1FE;
            return match offset {
                0x004 => self.agnus.vposr(),
                0x006 => self.agnus.vhposr(),
                _ => self.chipset.read_word(offset),
            };
        }
        self.memory.read_word(addr24)
    }

    /// Read a chip-RAM byte directly, ignoring the OVL overlay.
    #[must_use]
    pub fn read_chip_ram_byte(&self, addr: u32) -> u8 {
        self.memory.read_chip_ram_byte(addr)
    }

    /// Tick one colour-clock period. The 68000 advances one CPU
    /// clock per CCK on the A500 (master/4 = 7.09 MHz both ways).
    pub fn tick_cck(&mut self) {
        // Advance the beam first; if VBL fires, request the VERTB
        // interrupt before the CPU's interrupt sample.
        if self.agnus.tick_cck() {
            self.chipset.write_word(0x09C, 0x8020);
            // VBL also restarts the copper from COP1LC.
            self.copper.jump1();
        }

        // Copper runs when DMACON.COPEN (bit 7) AND DMAEN (bit 9) are
        // both set.
        if self.chipset.dmacon & 0x0280 == 0x0280 {
            self.copper.tick_cck(
                &self.memory,
                &mut self.chipset,
                self.agnus.vpos,
                self.agnus.hpos,
            );
        }

        // Denise renders one CCK's worth of pixels (scanline-renderer
        // at M11.1 — full line at hpos=0 of each visible line).
        self.denise.tick_cck(
            self.agnus.vpos,
            self.agnus.hpos,
            &mut self.chipset,
            &self.memory,
        );

        // Tick both CIAs every CIA_E_CLOCK_DIVISOR CCKs (E clock = master/10).
        self.e_clock_phase += 1;
        if self.e_clock_phase >= CIA_E_CLOCK_DIVISOR {
            self.e_clock_phase = 0;
            self.cia_a.tick_e_clock();
            self.cia_b.tick_e_clock();
            // CIA-A /IRQ → Paula INTREQ.PORTS (bit 3, level 2).
            if self.cia_a.irq_pending {
                self.chipset.write_word(0x09C, 0x8008);
            }
            // CIA-B /IRQ → Paula INTREQ.EXTER (bit 13, level 6).
            if self.cia_b.irq_pending {
                self.chipset.write_word(0x09C, 0xA000);
            }
        }

        self.service_cpu_bus();
        self.cpu.ipl = self.chipset.compute_ipl();
        self.cpu.tick();
        self.cck_count += 1;
    }

    fn service_cpu_bus(&mut self) {
        // Snapshot the bus-cycle parameters out of the CPU state so we
        // can mutate self.memory without borrow conflicts.
        let bus_info = match &self.cpu.state {
            State::BusCycle {
                addr,
                fc,
                is_read,
                is_word,
                data,
                cycle_count,
                ..
            } => Some((*addr, *fc, *is_read, *is_word, *data, *cycle_count)),
            _ => None,
        };

        let Some((addr, fc, is_read, is_word, data, cycle_count)) = bus_info else {
            return;
        };

        // 68000 bus cycle is 4 CCKs (S0-S7). DTACK is sampled at S4
        // = cycle 2. We complete the bus cycle on the first poll at
        // or after cycle 2 and then hold the result steady.
        if cycle_count < 2 {
            self.cpu.bus_status = BusStatus::Wait;
            return;
        }
        if matches!(self.cpu.bus_status, BusStatus::Ready(_) | BusStatus::Error) {
            return;
        }

        // M1 has no interrupt controller — InterruptAck cycles return
        // a default vector (uninitialised IRQ).
        if fc == FunctionCode::InterruptAck {
            self.cpu.bus_status = BusStatus::Ready(0x0018);
            return;
        }

        let addr24 = addr & 0xFF_FFFF;

        // CIA-A address space (odd bytes in $BFE000-$BFEFFF).
        if let Some(reg) = cia::decode_cia_a(addr24) {
            if is_read {
                let val = u16::from(self.cia_a.read_register(reg));
                self.cpu.bus_status = BusStatus::Ready(val);
            } else {
                let val = data.unwrap_or(0);
                self.cia_a.write_register(reg, val as u8);
                self.memory.set_overlay(self.cia_a.ovl());
                self.cpu.bus_status = BusStatus::Ready(0);
            }
            return;
        }

        // CIA-B address space (even bytes in $BFD000-$BFDFFF).
        if let Some(reg) = cia::decode_cia_b(addr24) {
            if is_read {
                // CIA-B is on the high data byte; word reads put the
                // CIA value in the high byte. We expose the byte
                // value in the low byte for convenience to the bus.
                let val = u16::from(self.cia_b.read_register(reg));
                self.cpu.bus_status = BusStatus::Ready(val);
            } else {
                let val = data.unwrap_or(0);
                // Word writes to CIA-B target the high byte; we take
                // the high byte if it's a word write, low byte if byte.
                let byte = if is_word { (val >> 8) as u8 } else { val as u8 };
                self.cia_b.write_register(reg, byte);
                self.cpu.bus_status = BusStatus::Ready(0);
            }
            return;
        }

        // Custom-register space dispatches to the chipset module.
        // Agnus owns the beam-position read-side registers; everything
        // else routes to Chipset.
        if (CUSTOM_BASE..CUSTOM_TOP).contains(&addr24) {
            let offset = (addr24 - CUSTOM_BASE) as u16 & 0x1FE;
            if is_read {
                *self.debug_reg_read_counts.entry(offset).or_insert(0) += 1;
                let val = match offset {
                    0x004 => self.agnus.vposr(),
                    0x006 => self.agnus.vhposr(),
                    _ => self.chipset.read_word(offset),
                };
                self.cpu.bus_status = BusStatus::Ready(if is_word { val } else { val & 0xFF });
            } else {
                let val = data.unwrap_or(0);
                self.dispatch_custom_write(offset, val);
                self.cpu.bus_status = BusStatus::Ready(0);
            }
            return;
        }

        if is_read {
            let val = if is_word {
                self.memory.read_word(addr24)
            } else {
                u16::from(self.memory.read_byte(addr24))
            };
            self.cpu.bus_status = BusStatus::Ready(val);
        } else {
            let val = data.unwrap_or(0);
            if is_word {
                self.memory.write_word(addr24, val);
            } else {
                self.memory.write_byte(addr24, val as u8);
            }
            self.cpu.bus_status = BusStatus::Ready(0);
        }
    }
}
