//! The replay engine an instruction runs on.
//!
//! An [`Exec`] counts the bus cycles an instruction asks for. Cycles before
//! the target have already happened: reads get the byte recorded for them,
//! writes and idles pass. The target cycle is captured and the instruction
//! stops with [`Yield`]. Registers live in a copy, so a stopped instruction
//! leaves nothing behind.

use serde::{Deserialize, Serialize};

use crate::Registers;

/// One bus cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BusCycle {
    Read(u16),
    Write(u16, u8),
    Idle,
}

/// The instruction reached the cycle the pins are waiting for.
#[derive(Debug)]
pub(crate) struct Yield;

pub(crate) type Step<T = ()> = Result<T, Yield>;

pub(crate) struct Exec<'a> {
    pub regs: Registers,
    reads: &'a [u8],
    cycle: usize,
    target: usize,
    emitted: Option<BusCycle>,
}

impl<'a> Exec<'a> {
    pub fn new(regs: Registers, reads: &'a [u8], target: usize) -> Self {
        Self {
            regs,
            reads,
            cycle: 0,
            target,
            emitted: None,
        }
    }

    /// The cycle captured by the last [`Yield`].
    pub fn emitted(&self) -> BusCycle {
        self.emitted.unwrap_or(BusCycle::Idle)
    }

    fn bus(&mut self, cycle: BusCycle) -> Step<u8> {
        let index = self.cycle;
        self.cycle += 1;
        if index == self.target {
            self.emitted = Some(cycle);
            return Err(Yield);
        }
        Ok(self.reads.get(index).copied().unwrap_or(0))
    }

    pub fn read(&mut self, addr: u16) -> Step<u8> {
        self.bus(BusCycle::Read(addr))
    }

    pub fn write(&mut self, addr: u16, data: u8) -> Step {
        self.bus(BusCycle::Write(addr, data)).map(|_| ())
    }

    pub fn idle(&mut self) -> Step {
        self.bus(BusCycle::Idle).map(|_| ())
    }

    /// The byte at PC, advancing PC.
    pub fn fetch(&mut self) -> Step<u8> {
        let pc = self.regs.pc;
        self.regs.pc = pc.wrapping_add(1);
        self.read(pc)
    }

    /// A fetch of a 16-bit operand, low byte first.
    pub fn fetch_word(&mut self) -> Step<u16> {
        let lo = self.fetch()?;
        let hi = self.fetch()?;
        Ok(u16::from(hi) << 8 | u16::from(lo))
    }

    fn direct(&self, addr: u8) -> u16 {
        u16::from(self.regs.p.p) << 8 | u16::from(addr)
    }

    /// A read from the direct page.
    pub fn load(&mut self, addr: u8) -> Step<u8> {
        let full = self.direct(addr);
        self.read(full)
    }

    /// A write to the direct page.
    pub fn store(&mut self, addr: u8, data: u8) -> Step {
        let full = self.direct(addr);
        self.write(full, data)
    }

    pub fn push(&mut self, data: u8) -> Step {
        let sp = self.regs.sp;
        self.regs.sp = sp.wrapping_sub(1);
        self.write(0x0100 | u16::from(sp), data)
    }

    pub fn pull(&mut self) -> Step<u8> {
        self.regs.sp = self.regs.sp.wrapping_add(1);
        self.read(0x0100 | u16::from(self.regs.sp))
    }

    /// A dummy read of the byte after the opcode, as single-byte
    /// instructions do.
    pub fn read_pc(&mut self) -> Step {
        let pc = self.regs.pc;
        self.read(pc).map(|_| ())
    }
}
