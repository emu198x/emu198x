//! The "Rock" - A Cycle-Strict Amiga Emulator.
//!
//! Foundation: Crystal-accuracy.
//! Bus Model: Reactive (Request/Acknowledge), not Predictive.
//! CPU Model: Ticks every 4 crystal cycles, polls bus until DTACK.

pub mod config;
pub mod bus;
pub mod agnus;
pub mod denise;
pub mod paula;
pub mod memory;

use crate::agnus::{Agnus, SlotOwner};
use cpu_m68k_rock::cpu::Cpu68000;
use cpu_m68k_rock::bus::{M68kBus, FunctionCode, BusStatus};

/// Standard Amiga PAL Master Crystal Frequency (Hz)
pub const PAL_CRYSTAL_HZ: u64 = 28_375_160;
/// Standard Amiga NTSC Master Crystal Frequency (Hz)
pub const NTSC_CRYSTAL_HZ: u64 = 28_636_360;

/// Number of crystal ticks per Colour Clock (CCK)
pub const TICKS_PER_CCK: u64 = 8;
/// Number of crystal ticks per CPU Cycle
pub const TICKS_PER_CPU: u64 = 4;

pub struct Amiga {
    pub master_clock: u64,
    pub cpu: Cpu68000,
    pub agnus: Agnus,
    // pub denise: Denise,
    // pub paula: Paula,
}

impl Amiga {
    pub fn new() -> Self {
        Self {
            master_clock: 0,
            cpu: Cpu68000::new(),
            agnus: Agnus::new(),
        }
    }

    pub fn tick(&mut self) {
        self.master_clock += 1;

        // 1. Tick Agnus/DMA (Every 8 ticks)
        if self.master_clock % TICKS_PER_CCK == 0 {
            self.agnus.tick_cck();
        }

        // 2. Tick CPU (Every 4 ticks)
        // Note: The CPU crate's tick() already handles the divisor internally if we pass master_clock,
        // but doing it here is clearer.
        let mut bus = AmigaBusWrapper {
            agnus: &mut self.agnus,
        };
        self.cpu.tick(&mut bus, self.master_clock);
    }
}

pub struct AmigaBusWrapper<'a> {
    pub agnus: &'a mut Agnus,
}

impl<'a> M68kBus for AmigaBusWrapper<'a> {
    fn poll_cycle(
        &mut self,
        addr: u32,
        _fc: FunctionCode,
        _is_read: bool,
        _is_word: bool,
        _data: Option<u16>,
    ) -> BusStatus {
        // Here is the Rock:
        // If it's a Chip RAM address, we MUST check Agnus.
        if addr < 0x200000 {
            match self.agnus.current_slot() {
                SlotOwner::Cpu => {
                    // CPU has the bus!
                    // In a real implementation, we would now access memory.
                    BusStatus::Ready(0) // Stub
                }
                _ => {
                    // DMA is using the bus (Refresh, Disk, etc.), CPU must wait.
                    BusStatus::Wait
                }
            }
        } else {
            // ROM or Fast RAM: no contention in this model.
            BusStatus::Ready(0) // Stub
        }
    }

    fn poll_interrupt_ack(&mut self, _level: u8) -> BusStatus {
        BusStatus::Ready(24 + _level as u16) // Autovector stub
    }

    fn reset(&mut self) {
        // Handle system reset
    }
}
