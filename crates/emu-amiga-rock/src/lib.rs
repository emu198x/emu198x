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
use crate::memory::Memory;
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
    pub memory: Memory,
    // pub denise: Denise,
    // pub paula: Paula,
}

impl Amiga {
    pub fn new(kickstart: Vec<u8>) -> Self {
        let mut cpu = Cpu68000::new();
        let memory = Memory::new(512 * 1024, kickstart);
        
        // Initial reset vectors are read from memory via overlay
        let ssp = (u32::from(memory.read_byte(0)) << 24) |
                  (u32::from(memory.read_byte(1)) << 16) |
                  (u32::from(memory.read_byte(2)) << 8)  |
                   u32::from(memory.read_byte(3));
        let pc  = (u32::from(memory.read_byte(4)) << 24) |
                  (u32::from(memory.read_byte(5)) << 16) |
                  (u32::from(memory.read_byte(6)) << 8)  |
                   u32::from(memory.read_byte(7));

        cpu.reset_to(ssp, pc);

        Self {
            master_clock: 0,
            cpu,
            agnus: Agnus::new(),
            memory,
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
            let mut bus = AmigaBusWrapper {
                agnus: &mut self.agnus,
                memory: &mut self.memory,
            };
            self.cpu.tick(&mut bus, self.master_clock);
        }
    }
}

pub struct AmigaBusWrapper<'a> {
    pub agnus: &'a mut Agnus,
    pub memory: &'a mut Memory,
}

impl<'a> M68kBus for AmigaBusWrapper<'a> {
    fn poll_cycle(
        &mut self,
        addr: u32,
        _fc: FunctionCode,
        is_read: bool,
        is_word: bool,
        data: Option<u16>,
    ) -> BusStatus {
        // println!("Bus Poll: addr=${:06X} read={} word={}", addr, is_read, is_word);
        // Chip RAM contention check
        if addr < 0x200000 {
            match self.agnus.current_slot() {
                SlotOwner::Cpu => {
                    // CPU has the bus!
                    if is_read {
                        let val = if is_word {
                            let hi = self.memory.read_byte(addr);
                            let lo = self.memory.read_byte(addr | 1);
                            (u16::from(hi) << 8) | u16::from(lo)
                        } else {
                            u16::from(self.memory.read_byte(addr))
                        };
                        // println!("Bus Ready: data=${:04X}", val);
                        BusStatus::Ready(val)
                    } else {
                        let val = data.unwrap_or(0);
                        if is_word {
                            self.memory.write_byte(addr, (val >> 8) as u8);
                            self.memory.write_byte(addr | 1, val as u8);
                        } else {
                            self.memory.write_byte(addr, val as u8);
                        }
                        BusStatus::Ready(0)
                    }
                }
                _ => BusStatus::Wait,
            }
        } else {
            // ROM or other: no contention
            if is_read {
                let val = if is_word {
                    let hi = self.memory.read_byte(addr);
                    let lo = self.memory.read_byte(addr | 1);
                    (u16::from(hi) << 8) | u16::from(lo)
                } else {
                    u16::from(self.memory.read_byte(addr))
                };
                // println!("Bus Ready (ROM): data=${:04X}", val);
                BusStatus::Ready(val)
            } else {
                // Ignore writes to ROM
                BusStatus::Ready(0)
            }
        }
    }

    fn poll_interrupt_ack(&mut self, _level: u8) -> BusStatus {
        BusStatus::Ready(24 + _level as u16) // Autovector stub
    }

    fn reset(&mut self) {
        // Handle system reset
    }
}
