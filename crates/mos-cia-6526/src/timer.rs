//! 6526 interval-timer state machine (issue #17).
//!
//! Ported from VICE's `ciatimer.{h,c}` (Andre Fachat) as vendored in VICE
//! 3.10 (`emulators/c64/vice-3.10/src/core/`). VICE drives the same state
//! bits through a precomputed 16K transition table plus alarm-based
//! warp-ahead; our machine ticks every φ2 cycle, so only the per-cycle
//! transition (`ciat_update`'s "inc" branch) is ported and the table becomes
//! a pure function.
//!
//! The state bits model the 6526's internal pipeline — the source of every
//! cycle-observable timer quirk:
//!
//! - Writing CR with START set raises `COUNT2` on the next transition and
//!   `COUNT3` the one after; the counter only decrements while `COUNT3` is
//!   up, so counting begins **two cycles** after the write lands.
//! - The force-load strobe (CR bit 4) travels `FLOAD → LOAD1 → LOAD`: the
//!   latch is copied one cycle after the write, and a decrement already in
//!   flight for that cycle is suppressed (`LOAD` clears `COUNT3`).
//! - One-shot (CR bit 3) travels `CR_ONESHOT → ONESHOT0 → ONESHOT`, so
//!   clearing one-shot right at an underflow still stops the timer.
//! - `OUT` is high for exactly the underflow cycle — it is what the ICR
//!   samples, what PB6/PB7 pulse mode outputs, and what cascaded Timer B
//!   counts (via `STEP`, consumed by the following transition).
//!
//! `PHI2IN` is *set* for the φ2-counting input mode; `set_ctrl` XORs the CR
//! byte's bit 5 so CR INMODE = 0 (count φ2) maps to `PHI2IN` = 1.

use serde::{Deserialize, Serialize};

pub const CR_START: u16 = 0x001;
pub const COUNT2: u16 = 0x002;
pub const STEP: u16 = 0x004;
pub const CR_ONESHOT: u16 = 0x008;
pub const CR_FLOAD: u16 = 0x010;
pub const PHI2IN: u16 = 0x020;
pub const COUNT3: u16 = 0x040;
pub const LOAD1: u16 = 0x080;
pub const ONESHOT0: u16 = 0x100;
pub const LOAD: u16 = 0x200;
pub const OUT: u16 = 0x400;
pub const COUNT: u16 = 0x800;
pub const ONESHOT: u16 = 0x1000;

/// Control-register bits that map straight into the state word
/// (START, ONESHOT, FLOAD, INMODE).
pub const CR_MASK: u16 = CR_START | CR_ONESHOT | CR_FLOAD | PHI2IN;

/// One cycle of the 6526 timer pipeline (VICE `ciat_init_table` entry).
#[must_use]
fn transition(t: u16) -> u16 {
    let mut next = t & (CR_START | CR_ONESHOT | PHI2IN);

    if (t & CR_START != 0) && (t & PHI2IN != 0) {
        next |= COUNT2;
    }
    if (t & COUNT2 != 0) || ((t & STEP != 0) && (t & CR_START != 0)) {
        next |= COUNT3;
    }
    if t & COUNT3 != 0 {
        next |= COUNT;
    }

    if t & CR_FLOAD != 0 {
        next |= LOAD1;
    }
    if t & LOAD1 != 0 {
        next |= LOAD;
    }

    if t & CR_ONESHOT != 0 {
        next |= ONESHOT0;
    }
    if t & ONESHOT0 != 0 {
        next |= ONESHOT;
    }

    next
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CiaTimer {
    state: u16,
    cnt: u16,
    latch: u16,
}

impl CiaTimer {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: 0,
            cnt: 0xFFFF,
            latch: 0xFFFF,
        }
    }

    /// Advance one φ2 cycle; returns `true` on underflow (the `OUT` cycle).
    ///
    /// Port of the per-cycle ("inc") branch of VICE `ciat_update` plus its
    /// shared underflow/load/one-shot epilogue.
    pub fn tick(&mut self) -> bool {
        let t = self.state;
        if self.cnt != 0 && (t & COUNT3 != 0) {
            self.cnt -= 1;
        }
        let mut t = transition(t);

        let mut underflow = false;
        if self.cnt == 0 && (t & COUNT3 != 0) {
            t |= LOAD | OUT;
            underflow = true;
        }
        if t & LOAD != 0 {
            self.cnt = self.latch;
            t &= !COUNT3;
        }
        if (t & OUT != 0) && (t & (ONESHOT | ONESHOT0) != 0) {
            t &= !(CR_START | COUNT2);
        }

        self.state = t;
        underflow
    }

    /// Cascade input: count one Timer A underflow (VICE `ciat_single_step`).
    /// The `STEP` bit is consumed by the *next* transition, so the cascaded
    /// count lands with the hardware's one-cycle offset.
    pub fn single_step(&mut self) {
        if self.state & CR_START != 0 {
            self.state |= STEP;
        }
    }

    /// Control-register write (VICE `ciat_set_ctrl`). Bit 5 is XORed so that
    /// INMODE = φ2 sets `PHI2IN`; Timer B callers must pre-map CRB's two
    /// INMODE bits onto bit 5 (VICE ORs 0x20 in for the count-TA modes).
    pub fn set_ctrl(&mut self, byte: u8) {
        self.state = (self.state & !CR_MASK) | ((u16::from(byte) & CR_MASK) ^ PHI2IN);
    }

    /// Latch low-byte write (VICE `ciat_set_latchlo`): a load in progress
    /// this cycle picks the new low byte up immediately.
    pub fn set_latch_lo(&mut self, byte: u8) {
        self.latch = (self.latch & 0xFF00) | u16::from(byte);
        if self.state & LOAD != 0 {
            self.cnt = (self.cnt & 0xFF00) | u16::from(byte);
        }
    }

    /// Latch high-byte write (VICE `ciat_set_latchhi`): also reloads the
    /// counter while the timer is stopped or loading.
    pub fn set_latch_hi(&mut self, byte: u8) {
        self.latch = (self.latch & 0x00FF) | (u16::from(byte) << 8);
        if (self.state & LOAD != 0) || (self.state & CR_START == 0) {
            self.cnt = self.latch;
        }
    }

    #[must_use]
    pub const fn counter(&self) -> u16 {
        self.cnt
    }

    #[must_use]
    pub const fn latch(&self) -> u16 {
        self.latch
    }

    /// Live START bit (CR reads reflect the pipeline, not the stored byte).
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.state & CR_START != 0
    }

    /// High for exactly the underflow cycle (PB6/PB7 pulse output).
    #[must_use]
    pub const fn is_underflow_cycle(&self) -> bool {
        self.state & OUT != 0
    }
}

impl Default for CiaTimer {
    fn default() -> Self {
        Self::new()
    }
}
