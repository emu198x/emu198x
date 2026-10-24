//! The Sony SPC700, the S-SMP CPU in the Super Nintendo's sound module.
//!
//! # The bus
//!
//! The CPU is pin-level, like every CPU in Emu198x: one [`Spc700::tick`] is
//! one CPU cycle, and each cycle the CPU drives its pins and the host acts on
//! them before the next tick.
//!
//! - `rw` high and `idle` low: a read of `addr`. The host puts the byte on
//!   `data_in` before the next tick.
//! - `rw` low: a write of `data` to `addr`.
//! - `idle` high: an internal cycle with no access. The SPC700 spends many of
//!   these, in multiplies, divides, branches and stack work.
//! - `sync` marks the opcode fetch, the first cycle of every instruction.
//!
//! There is no bus trait and no callback. The IPL ROM, the three timers and
//! the `$F0`–`$FF` I/O registers are the sound module's business: the host
//! decodes `addr` and answers.
//!
//! # How an instruction runs
//!
//! Each instruction is written as an ordinary function over its bus cycles,
//! in the order the chip performs them. A tick replays the current
//! instruction from its start, feeding it the bytes read on earlier cycles,
//! and stops at the first cycle not yet performed: that cycle's access goes
//! out on the pins. When an instruction runs to its end, its register
//! changes are committed and the next instruction's opcode fetch is the
//! cycle driven. The replay is an implementation detail; what the host sees
//! is one bus cycle per tick, in hardware order.
//!
//! # Reset
//!
//! On reset the S-SMP loads PC straight from the last two bytes of its IPL
//! ROM, with no bus cycles. The host does that with [`Spc700::reset_to`].
//!
//! # Provenance
//!
//! The instruction bodies, cycle order included, follow ares's SPC700
//! (ISC licence, © 2004-2025 ares team, Near et al), re-expressed in Rust.
//! They are checked cycle by cycle against SingleStepTests/spc700.

mod alu;
mod exec;
mod ops;

use serde::{Deserialize, Serialize};

pub use exec::BusCycle;

/// The processor status word.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flags {
    /// Carry.
    pub c: bool,
    /// Zero.
    pub z: bool,
    /// Interrupt enable. Nothing on the SNES raises an interrupt.
    pub i: bool,
    /// Half carry.
    pub h: bool,
    /// Break.
    pub b: bool,
    /// Direct page: when set, direct-page addresses are in `$01xx`.
    pub p: bool,
    /// Overflow.
    pub v: bool,
    /// Negative.
    pub n: bool,
}

impl Flags {
    /// The flags packed as the PSW byte, carry in bit 0.
    #[must_use]
    pub fn bits(self) -> u8 {
        u8::from(self.c)
            | u8::from(self.z) << 1
            | u8::from(self.i) << 2
            | u8::from(self.h) << 3
            | u8::from(self.b) << 4
            | u8::from(self.p) << 5
            | u8::from(self.v) << 6
            | u8::from(self.n) << 7
    }

    /// Flags unpacked from a PSW byte.
    #[must_use]
    pub fn from_bits(bits: u8) -> Self {
        Self {
            c: bits & 0x01 != 0,
            z: bits & 0x02 != 0,
            i: bits & 0x04 != 0,
            h: bits & 0x08 != 0,
            b: bits & 0x10 != 0,
            p: bits & 0x20 != 0,
            v: bits & 0x40 != 0,
            n: bits & 0x80 != 0,
        }
    }
}

/// Why the CPU has stopped executing instructions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Halt {
    /// Running.
    #[default]
    None,
    /// `SLEEP` (`$EF`): waiting for an interrupt, which never comes on a SNES.
    Sleep,
    /// `STOP` (`$FF`): stopped until reset.
    Stop,
}

/// The programmer-visible registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registers {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    /// Stack pointer, an offset into page `$01`.
    pub sp: u8,
    pub pc: u16,
    pub p: Flags,
    /// Whether the CPU is halted, and by which instruction.
    pub halt: Halt,
}

impl Default for Registers {
    /// The state at power-on, as ares sets it.
    fn default() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xEF,
            pc: 0,
            p: Flags::from_bits(0x02),
            halt: Halt::None,
        }
    }
}

impl Registers {
    /// `YA`, the 16-bit pair the word instructions use.
    #[must_use]
    pub fn ya(&self) -> u16 {
        u16::from(self.y) << 8 | u16::from(self.a)
    }

    pub(crate) fn set_ya(&mut self, value: u16) {
        self.a = value as u8;
        self.y = (value >> 8) as u8;
    }
}

/// The longest instruction: `DIV` takes twelve cycles.
const MAX_CYCLES: usize = 16;

/// The SPC700 and its pins.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spc700 {
    pub regs: Registers,
    pub total_cycles: u64,

    // Outputs, driven by tick().
    /// Address bus.
    pub addr: u16,
    /// Data bus, driven during writes.
    pub data: u8,
    /// High for a read, low for a write.
    pub rw: bool,
    /// High for an internal cycle with no access.
    pub idle: bool,
    /// High on an opcode fetch.
    pub sync: bool,

    // Input, filled by the host.
    /// Data bus during reads.
    pub data_in: u8,

    /// Bytes read on each cycle of the current instruction so far.
    reads: [u8; MAX_CYCLES],
    /// Cycles of the current instruction already driven on the pins.
    step: usize,
    /// Whether the last cycle driven was a read, so `data_in` holds its byte.
    last_was_read: bool,
    /// While halted, the cycles alternate between a read of PC and an idle.
    halt_phase: bool,
}

impl Default for Spc700 {
    fn default() -> Self {
        Self::new()
    }
}

impl Spc700 {
    /// A CPU at power-on, with PC at zero until [`Self::reset_to`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_registers(Registers::default())
    }

    /// A CPU about to start an instruction with these registers.
    #[must_use]
    pub fn with_registers(regs: Registers) -> Self {
        Self {
            regs,
            total_cycles: 0,
            addr: regs.pc,
            data: 0,
            rw: true,
            idle: false,
            sync: false,
            data_in: 0,
            reads: [0; MAX_CYCLES],
            step: 0,
            last_was_read: false,
            halt_phase: false,
        }
    }

    /// Reset: power-on registers, with PC loaded from the IPL ROM's reset
    /// vector by the host.
    pub fn reset_to(&mut self, pc: u16) {
        *self = Self::with_registers(Registers {
            pc,
            ..Registers::default()
        });
    }

    /// Whether the next tick starts a new instruction.
    #[must_use]
    pub fn at_instruction_boundary(&self) -> bool {
        self.step == 0 && self.regs.halt == Halt::None
    }

    /// Advance one CPU cycle and drive its bus access on the pins.
    pub fn tick(&mut self) {
        self.total_cycles += 1;
        if self.last_was_read && self.step > 0 {
            self.reads[self.step - 1] = self.data_in;
        }
        loop {
            if self.regs.halt != Halt::None {
                self.drive_halted();
                return;
            }
            let mut exec = exec::Exec::new(self.regs, &self.reads, self.step);
            match ops::run(&mut exec) {
                Err(exec::Yield) => {
                    let cycle = exec.emitted();
                    self.drive(cycle, self.step == 0);
                    self.step += 1;
                    return;
                }
                Ok(()) => {
                    self.regs = exec.regs;
                    self.step = 0;
                    self.halt_phase = false;
                }
            }
        }
    }

    /// A halted CPU keeps the bus busy: a read of PC, then an idle cycle,
    /// over and over.
    fn drive_halted(&mut self) {
        let cycle = if self.halt_phase {
            BusCycle::Idle
        } else {
            BusCycle::Read(self.regs.pc)
        };
        self.halt_phase = !self.halt_phase;
        self.drive(cycle, false);
    }

    fn drive(&mut self, cycle: BusCycle, sync: bool) {
        self.sync = sync;
        match cycle {
            BusCycle::Read(addr) => {
                self.addr = addr;
                self.rw = true;
                self.idle = false;
                self.last_was_read = true;
            }
            BusCycle::Write(addr, data) => {
                self.addr = addr;
                self.data = data;
                self.rw = false;
                self.idle = false;
                self.last_was_read = false;
            }
            BusCycle::Idle => {
                self.rw = true;
                self.idle = true;
                self.last_was_read = false;
            }
        }
    }

    /// The access the pins describe this cycle.
    #[must_use]
    pub fn bus_cycle(&self) -> BusCycle {
        if self.idle {
            BusCycle::Idle
        } else if self.rw {
            BusCycle::Read(self.addr)
        } else {
            BusCycle::Write(self.addr, self.data)
        }
    }
}

#[cfg(test)]
mod tests;
