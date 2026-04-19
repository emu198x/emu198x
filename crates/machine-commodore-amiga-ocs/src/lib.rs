//! Commodore Amiga (OCS chipset) machine — incremental restart.
//!
//! Built milestone-by-milestone per
//! `wiki/decisions/amiga-restart-plan.md`. Each milestone adds the
//! minimum hardware behaviour the running ROM demands; nothing more.
//!
//! Current milestone: **M0 — CPU + ROM + OVL mapping.**
//! No chip RAM, no chipset, no CIAs.

mod memory;

pub use memory::Memory;

use motorola_68000::Cpu68000;

/// Amiga (OCS) machine.
///
/// At M0 this is a 68000 CPU paired with a Kickstart ROM. The ROM is
/// readable at its anchor `$F80000-$FFFFFF` (mirrored to fill the 512K
/// ROM region) AND through the OVL=1 reset overlay at `$0-$3FFFF`.
///
/// All other addresses read as floating bus (`$FF`) and silently
/// drop writes.
pub struct AmigaOcs {
    cpu: Cpu68000,
    memory: Memory,
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

        // Real reset behaviour: CPU autonomously reads SSP from $0
        // and PC from $4 via two longword bus cycles. With OVL=1
        // those addresses map to ROM offsets 0 and 4, so we read
        // straight from the ROM image.
        let ssp = memory.read_long(0x000000);
        let pc = memory.read_long(0x000004);
        cpu.reset_to(ssp, pc);

        Self { cpu, memory }
    }

    /// Direct CPU access (read-only — mutating the CPU outside the
    /// tick loop breaks invariants).
    #[must_use]
    pub fn cpu(&self) -> &Cpu68000 {
        &self.cpu
    }

    /// Convenience: read a word at the given 24-bit address through
    /// the active memory map. Used by tests to verify the map.
    #[must_use]
    pub fn read_word(&self, addr: u32) -> u16 {
        self.memory.read_word(addr)
    }
}
