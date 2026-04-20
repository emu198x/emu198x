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
mod memory;

pub use agnus::{Agnus, PAL_FRAME_LINES, PAL_LINE_CCKS};
pub use chipset::Chipset;
pub use cia::CiaA;
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
    cia_a: CiaA,
    agnus: Agnus,
    cck_count: u64,
    e_clock_phase: u64,
    /// Diagnostic: count of unique custom-register read offsets seen
    /// since reset, indexed by offset / 2.
    pub debug_reg_read_counts: std::collections::HashMap<u16, u64>,
}

impl AmigaOcs {
    /// Build a new Amiga (OCS) with the given Kickstart ROM image.
    ///
    /// The CPU is reset using the SSP/PC longwords at ROM offsets 0/4,
    /// matching what real-hardware reset would fetch from `$00000000`
    /// and `$00000004` (mapped to ROM via OVL=1).
    #[must_use]
    pub fn new(kickstart: Vec<u8>) -> Self {
        let memory = Memory::new(kickstart);
        let mut cpu = Cpu68000::new();
        let ssp = memory.read_long(0x000000);
        let pc = memory.read_long(0x000004);
        cpu.reset_to(ssp, pc);
        Self {
            cpu,
            memory,
            chipset: Chipset::new(),
            cia_a: CiaA::new(),
            agnus: Agnus::new(),
            cck_count: 0,
            e_clock_phase: 0,
            debug_reg_read_counts: std::collections::HashMap::new(),
        }
    }

    /// Read-only Agnus access.
    #[must_use]
    pub fn agnus(&self) -> &Agnus {
        &self.agnus
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
    pub fn cia_a(&self) -> &CiaA {
        &self.cia_a
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
            self.chipset.write_word(offset, val);
        } else {
            self.memory.write_word(addr, val);
        }
    }

    /// Backdoor for tests: write a byte as if the CPU did it.
    pub fn poke_byte(&mut self, addr: u32, val: u8) {
        if let Some(reg) = cia::decode_cia_a(addr) {
            self.cia_a.write_register(reg, val);
            self.memory.set_overlay(self.cia_a.ovl());
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
            // Side-effect-free peek for the public API; the CPU's
            // bus path uses read_register() directly so reading-clears
            // ICR gets the proper effect.
            return u16::from(self.cia_a.peek_register(reg));
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
        }

        // Tick CIA-A every CIA_E_CLOCK_DIVISOR CCKs (E clock = master/10).
        self.e_clock_phase += 1;
        if self.e_clock_phase >= CIA_E_CLOCK_DIVISOR {
            self.e_clock_phase = 0;
            self.cia_a.tick_e_clock();
            // CIA-A /IRQ → Paula INTREQ.PORTS (bit 3). Latch on edge.
            if self.cia_a.irq_pending {
                self.chipset.write_word(0x09C, 0x8008);
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
                self.chipset.write_word(offset, val);
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
