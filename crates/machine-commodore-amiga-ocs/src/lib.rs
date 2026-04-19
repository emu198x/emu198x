//! Commodore Amiga (OCS chipset) machine — incremental restart.
//!
//! Built milestone-by-milestone per
//! `wiki/decisions/amiga-restart-plan.md`. Each milestone adds the
//! minimum hardware behaviour the running ROM demands; nothing more.
//!
//! Current milestone: **M1 — chip RAM + CPU bus integration.**

mod memory;

pub use memory::{Memory, CHIP_RAM_SIZE};

use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::State;
use motorola_68000::Cpu68000;

/// Amiga (OCS) machine.
pub struct AmigaOcs {
    cpu: Cpu68000,
    memory: Memory,
    cck_count: u64,
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
            cck_count: 0,
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

    /// Read a word at the given 24-bit address through the active
    /// memory map. Used by tests to verify the map.
    #[must_use]
    pub fn read_word(&self, addr: u32) -> u16 {
        self.memory.read_word(addr)
    }

    /// Read a chip-RAM byte directly, ignoring the OVL overlay.
    #[must_use]
    pub fn read_chip_ram_byte(&self, addr: u32) -> u8 {
        self.memory.read_chip_ram_byte(addr)
    }

    /// Tick one colour-clock period. The 68000 advances one CPU
    /// clock per CCK on the A500 (master/4 = 7.09 MHz both ways).
    pub fn tick_cck(&mut self) {
        self.service_cpu_bus();
        self.cpu.ipl = 0; // No Paula yet — no interrupts.
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
