//! Motorola 68000 CPU core with Reactive Bus State Machine.

use crate::bus::{M68kBus, BusStatus, FunctionCode};

pub enum State {
    /// CPU is between instructions or in a purely internal state.
    Idle,
    /// CPU is performing an internal operation (e.g. ALU) for N cycles.
    Internal { cycles: u8 },
    /// CPU is performing a bus cycle and waiting for Ready/Error.
    BusCycle {
        addr: u32,
        fc: FunctionCode,
        is_read: bool,
        is_word: bool,
        data: Option<u16>,
        cycle_count: u8,
    },
    /// CPU is halted (double bus fault).
    Halted,
    /// CPU is stopped (waiting for interrupt).
    Stopped,
}

pub struct Cpu68000 {
    pub state: State,
    // ... registers, prefetch, etc. will be ported here
}

impl Cpu68000 {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
        }
    }

    /// Advance the CPU by one crystal clock (NOT one CPU clock).
    /// This allows us to handle phase offsets and bus contention precisely.
    pub fn tick<B: M68kBus>(&mut self, bus: &mut B, crystal_clock: u64) {
        // Amiga: CPU ticks every 4 crystal clocks.
        if crystal_clock % 4 != 0 {
            return;
        }

        match &mut self.state {
            State::Idle => {
                // Start next instruction or micro-op
            }
            State::Internal { cycles } => {
                if *cycles > 1 {
                    *cycles -= 1;
                } else {
                    self.state = State::Idle;
                }
            }
            State::BusCycle { addr, fc, is_read, is_word, data, cycle_count } => {
                *cycle_count += 1;
                
                // 68000 bus cycles take at least 4 clocks.
                // We only start polling for /DTACK at the end of S4 (cycle 4).
                if *cycle_count >= 4 {
                    match bus.poll_cycle(*addr, *fc, *is_read, *is_word, *data) {
                        BusStatus::Ready(read_data) => {
                            // Latch data and finish
                            if *is_read {
                                // self.latch(read_data);
                            }
                            self.state = State::Idle;
                        }
                        BusStatus::Wait => {
                            // Continue waiting...
                        }
                        BusStatus::Error => {
                            // self.trigger_exception(2);
                            self.state = State::Halted;
                        }
                    }
                }
            }
            State::Halted | State::Stopped => {}
        }
    }
}
