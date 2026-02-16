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

use crate::agnus::Agnus;
use crate::bus::{M68kBus, FunctionCode, BusStatus};

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
    pub agnus: Agnus,
    // We'll add CPU, Denise, etc. as we go
}

impl Amiga {
    pub fn new() -> Self {
        Self {
            master_clock: 0,
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
        if self.master_clock % TICKS_PER_CPU == 0 {
            // self.cpu.tick(&mut self.bus_wrapper);
        }
    }
}

pub struct AmigaBusWrapper<'a> {
    pub agnus: &'a mut Agnus,
    // pub memory: &'a mut Memory,
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
                crate::agnus::SlotOwner::Cpu => {
                    // CPU has the bus!
                    // BusStatus::Ready(data)
                    BusStatus::Ready(0) // Stub
                }
                _ => {
                    // DMA is using the bus, CPU must wait.
                    BusStatus::Wait
                }
            }
        } else {
            // ROM or Fast RAM: no contention
            BusStatus::Ready(0) // Stub
        }
    }
}
