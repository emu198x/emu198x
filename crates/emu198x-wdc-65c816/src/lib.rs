//! WDC 65C816: the 16-bit 6502 behind the Super Nintendo's 5A22 and the
//! Apple IIGS.
//!
//! # The bus is pins
//!
//! The core owns no memory. Its bus is a set of public fields the machine
//! reads and writes between calls to [`Wdc65c816::tick`], and one `tick` is
//! one CPU cycle:
//!
//! - Before `tick`, the pins describe the cycle about to happen:
//!   [`addr`](Wdc65c816::addr) (24 bits, bank in the top byte),
//!   [`rw`](Wdc65c816::rw) (`true` for a read), and for a write the byte in
//!   [`data`](Wdc65c816::data).
//! - For a read, the machine puts the byte on
//!   [`data_in`](Wdc65c816::data_in); for a write it stores `data`. Then it
//!   calls `tick`, which completes the cycle and sets the pins up for the
//!   next one.
//!
//! The status outputs say what kind of cycle it is, which is what a 65C816
//! system decodes to time its bus:
//!
//! | VDA | VPA | Cycle |
//! |---|---|---|
//! | 1 | 1 | opcode fetch |
//! | 0 | 1 | operand (program) fetch |
//! | 1 | 0 | data access |
//! | 0 | 0 | internal operation: the address is not valid for memory |
//!
//! [`vpb`](Wdc65c816::vpb) marks the two vector reads and
//! [`mlb`](Wdc65c816::mlb) the cycles of a read-modify-write. The Super
//! Nintendo's 5A22 stretches an internal cycle to 6 master clocks and a bus
//! cycle to 6, 8 or 12 by address, so the machine needs VDA and VPA to know
//! which it has.
//!
//! The inputs are [`rdy`](Wdc65c816::rdy), [`irq`](Wdc65c816::irq),
//! [`nmi`](Wdc65c816::nmi), [`abort`](Wdc65c816::abort) and
//! [`res`](Wdc65c816::res), all `true` when asserted. NMI is taken on its
//! rising edge, IRQ while it is held and the I flag is clear.
//!
//! # Timing
//!
//! Every cycle the datasheet counts is a `tick`, internal ones included:
//! the extra cycle when the direct page register's low byte is not zero, the
//! index page-crossing cycle, a taken branch's cycles, the emulation-mode
//! branch page cross, and the long forms' extra fetches.
//!
//! # Provenance
//!
//! The instruction sequences and the ALU are re-expressed from ares's
//! `wdc65816` component (ISC licence, © the ares team, Near et al.), turned
//! from callback calls into a per-cycle state machine. The address an
//! internal cycle presents, and the bus status of every cycle, follow the
//! SingleStepTests 65816 corpus (MIT), which this crate is tested against
//! cycle by cycle.

pub mod registers;

mod alu;
mod exec;

use registers::{FLAG_D, FLAG_I, Registers};
use serde::{Deserialize, Serialize};

/// The pseudo-opcode for "the opcode fetch is on the bus".
const OP_FETCH: u16 = 0x100;
/// The hardware interrupt sequence (IRQ, NMI or ABORT).
const OP_INTERRUPT: u16 = 0x101;
/// The reset sequence.
const OP_RESET: u16 = 0x102;

/// Which interrupt the sequence in progress is taking.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Interrupt {
    #[default]
    Irq,
    Nmi,
    Abort,
}

/// The WDC 65C816.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wdc65c816 {
    pub regs: Registers,
    /// Cycles run since construction or reset.
    pub total_cycles: u64,

    // ── Outputs ──
    /// Address bus: bank in bits 16-23.
    pub addr: u32,
    /// Data the CPU drives on a write cycle.
    pub data: u8,
    /// `true` for a read cycle, `false` for a write.
    pub rw: bool,
    /// Valid data address.
    pub vda: bool,
    /// Valid program address.
    pub vpa: bool,
    /// Vector pull: the two cycles reading an interrupt or reset vector.
    pub vpb: bool,
    /// Memory lock: the cycles of a read-modify-write.
    pub mlb: bool,

    // ── Inputs ──
    /// Data the machine supplies for a read cycle.
    pub data_in: u8,
    /// Ready. Holding it low freezes the CPU in its current cycle.
    pub rdy: bool,
    /// Interrupt request, level-sensitive.
    pub irq: bool,
    /// Non-maskable interrupt, taken on the rising edge.
    pub nmi: bool,
    /// Abort: the current instruction completes without changing the
    /// registers, then the abort vector is taken.
    pub abort: bool,
    /// Reset. While held, the CPU does nothing; on release it runs the reset
    /// sequence.
    pub res: bool,

    // ── Instruction state ──
    /// The executing opcode, or one of the `OP_*` sequences.
    op: u16,
    /// The cycle within it. Step 0 is the opcode fetch.
    step: u8,
    /// Operand widths captured at decode, as the chip does.
    m8: bool,
    x8: bool,
    /// Working registers.
    u: u32,
    v: u32,
    w: u32,
    /// The byte the last read cycle returned.
    last: u8,
    /// The previous cycle was an opcode fetch, which moves an internal
    /// cycle's address on to the next byte.
    after_opcode: bool,
    /// Registers at the start of the instruction, for ABORT.
    saved: Option<Registers>,

    // ── Interrupt state ──
    nmi_prev: bool,
    nmi_latch: bool,
    abort_latch: bool,
    res_prev: bool,
    /// Sampled on each instruction's last cycle.
    int_pending: bool,
    interrupt: Interrupt,
    waiting: bool,
    stopped: bool,
}

impl Wdc65c816 {
    /// A powered-on CPU, about to run its reset sequence.
    #[must_use]
    pub fn new() -> Self {
        let mut cpu = Self {
            regs: Registers::new(),
            total_cycles: 0,
            addr: 0,
            data: 0,
            rw: true,
            vda: false,
            vpa: false,
            vpb: false,
            mlb: false,
            data_in: 0,
            rdy: true,
            irq: false,
            nmi: false,
            abort: false,
            res: false,
            op: OP_RESET,
            step: 0,
            m8: true,
            x8: true,
            u: 0,
            v: 0,
            w: 0,
            last: 0,
            after_opcode: false,
            saved: None,
            nmi_prev: false,
            nmi_latch: false,
            abort_latch: false,
            res_prev: false,
            int_pending: false,
            interrupt: Interrupt::Irq,
            waiting: false,
            stopped: false,
        };
        cpu.reset();
        cpu
    }

    /// Start the reset sequence: emulation mode, 8-bit registers, the direct
    /// page and banks at zero, IRQs disabled, then the vector at `$FFFC`.
    /// A, X and Y keep their low bytes.
    pub fn reset(&mut self) {
        self.total_cycles = 0;
        self.waiting = false;
        self.stopped = false;
        self.nmi_latch = false;
        self.abort_latch = false;
        self.int_pending = false;
        self.saved = None;
        self.op = OP_RESET;
        self.step = 0;
        self.exec();
    }

    /// Point the CPU at an opcode fetch from `pbr:pc` with the registers as
    /// they stand, skipping the reset sequence. For test harnesses and
    /// snapshot loaders.
    pub fn begin_at(&mut self, pbr: u8, pc: u16) {
        self.regs.pbr = pbr;
        self.regs.pc = pc;
        self.regs.normalise();
        self.waiting = false;
        self.stopped = false;
        self.int_pending = false;
        self.saved = None;
        self.fetch_opcode();
    }

    /// Emulation mode, as the E output reports it.
    #[must_use]
    pub fn e(&self) -> bool {
        self.regs.e
    }

    /// The accumulator width output: `true` while 8-bit.
    #[must_use]
    pub fn m(&self) -> bool {
        self.regs.m8()
    }

    /// The index width output: `true` while 8-bit.
    #[must_use]
    pub fn x(&self) -> bool {
        self.regs.x8()
    }

    /// Whether the CPU is between instructions: the next opcode fetch, or
    /// the first cycle of an interrupt sequence, is on the bus.
    #[must_use]
    pub fn instruction_complete(&self) -> bool {
        self.step == 0 && (self.op == OP_FETCH || self.op == OP_INTERRUPT)
    }

    /// Whether `WAI` has stopped the CPU until an interrupt.
    #[must_use]
    pub fn waiting(&self) -> bool {
        self.waiting
    }

    /// Whether `STP` has stopped the CPU until reset.
    #[must_use]
    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// Whether the current cycle drives the bus at all. `WAI` and `STP`
    /// leave it idle with no valid address.
    #[must_use]
    pub fn bus_active(&self) -> bool {
        !(self.waiting || self.stopped)
    }

    /// Run one CPU cycle: complete the one on the pins and set up the next.
    pub fn tick(&mut self) {
        self.total_cycles += 1;

        let res_released = self.res_prev && !self.res;
        self.res_prev = self.res;
        if self.res {
            return;
        }
        if res_released {
            self.reset();
            return;
        }

        if self.nmi && !self.nmi_prev {
            self.nmi_latch = true;
        }
        self.nmi_prev = self.nmi;
        if self.abort {
            self.abort_latch = true;
        }

        if self.stopped {
            return;
        }
        if self.waiting {
            // WAI ends on any interrupt input, masked IRQ included.
            if self.nmi_latch || self.irq || self.abort_latch {
                self.waiting = false;
                self.int_pending =
                    self.nmi_latch || self.abort_latch || (self.irq && !self.regs.flag(FLAG_I));
                self.finish();
            }
            return;
        }
        if !self.rdy {
            return;
        }

        if self.rw {
            self.last = self.data_in;
        }
        self.step += 1;
        self.exec();
    }

    // ── Bus cycle emitters ──

    fn bus(&mut self, addr: u32, read: bool, vda: bool, vpa: bool) {
        self.addr = addr & 0xFF_FFFF;
        self.rw = read;
        self.vda = vda;
        self.vpa = vpa;
        self.vpb = false;
        self.mlb = false;
        self.after_opcode = false;
    }

    fn pc_address(&self) -> u32 {
        u32::from(self.regs.pbr) << 16 | u32::from(self.regs.pc)
    }

    /// Put the next opcode fetch on the bus. The program counter moves past
    /// it when the opcode is taken in, so between instructions it names the
    /// next one.
    fn fetch_opcode(&mut self) {
        self.op = OP_FETCH;
        self.step = 0;
        self.bus(self.pc_address(), true, true, true);
        self.after_opcode = true;
    }

    /// An operand fetch from the program counter.
    fn fetch(&mut self) {
        self.bus(self.pc_address(), true, false, true);
        self.regs.pc = self.regs.pc.wrapping_add(1);
    }

    fn read(&mut self, addr: u32) {
        self.bus(addr, true, true, false);
    }

    fn write(&mut self, addr: u32, value: u8) {
        self.data = value;
        self.bus(addr, false, true, false);
    }

    fn read_vector(&mut self, addr: u16) {
        self.bus(u32::from(addr), true, true, false);
        self.vpb = true;
    }

    /// An internal cycle. The address bus holds the last address, except
    /// straight after an opcode fetch, when it shows the next byte.
    fn idle(&mut self) {
        let addr = if self.after_opcode {
            self.pc_address()
        } else {
            self.addr
        };
        self.bus(addr, true, false, false);
    }

    fn idle_at(&mut self, addr: u32) {
        self.bus(addr, true, false, false);
    }

    /// Mark the cycle just set up as part of a read-modify-write.
    fn lock(&mut self) {
        self.mlb = true;
    }

    // ── Stack ──

    fn push(&mut self, value: u8) {
        self.write(u32::from(self.regs.s), value);
        self.regs.s = if self.regs.e {
            (self.regs.s & 0xFF00) | (self.regs.s.wrapping_sub(1) & 0xFF)
        } else {
            self.regs.s.wrapping_sub(1)
        };
    }

    fn pull(&mut self) {
        self.regs.s = if self.regs.e {
            (self.regs.s & 0xFF00) | (self.regs.s.wrapping_add(1) & 0xFF)
        } else {
            self.regs.s.wrapping_add(1)
        };
        self.read(u32::from(self.regs.s));
    }

    /// A push by one of the 65C816's own instructions, which run the stack
    /// pointer through all 16 bits even in emulation mode and repair the
    /// high byte afterwards.
    fn push_n(&mut self, value: u8) {
        self.write(u32::from(self.regs.s), value);
        self.regs.s = self.regs.s.wrapping_sub(1);
    }

    fn pull_n(&mut self) {
        self.regs.s = self.regs.s.wrapping_add(1);
        self.read(u32::from(self.regs.s));
    }

    // ── Instruction boundaries ──

    /// The last cycle of an instruction is about to go on the bus: sample
    /// the interrupt inputs, as the chip does.
    fn last_cycle(&mut self) {
        self.int_pending = self.nmi_latch || (self.irq && !self.regs.flag(FLAG_I));
    }

    /// The instruction is done: take an interrupt or fetch the next opcode.
    ///
    /// ABORT, asserted at any point in the instruction, undoes its register
    /// changes and is taken first. NMI and IRQ are taken when the last-cycle
    /// sample saw them.
    fn finish(&mut self) {
        let interrupt = if self.abort_latch {
            self.abort_latch = false;
            self.int_pending = false;
            if let Some(saved) = self.saved {
                self.regs = saved;
            }
            Some(Interrupt::Abort)
        } else if self.int_pending {
            self.int_pending = false;
            if self.nmi_latch {
                self.nmi_latch = false;
                Some(Interrupt::Nmi)
            } else {
                Some(Interrupt::Irq)
            }
        } else {
            None
        };
        self.saved = None;
        match interrupt {
            Some(kind) => {
                self.interrupt = kind;
                self.op = OP_INTERRUPT;
                self.step = 0;
                self.exec();
            }
            None => self.fetch_opcode(),
        }
    }

    /// The vector for the interrupt being taken.
    fn interrupt_vector(&self) -> u16 {
        match (self.interrupt, self.regs.e) {
            (Interrupt::Irq, false) => 0xFFEE,
            (Interrupt::Irq, true) => 0xFFFE,
            (Interrupt::Nmi, false) => 0xFFEA,
            (Interrupt::Nmi, true) => 0xFFFA,
            (Interrupt::Abort, false) => 0xFFE8,
            (Interrupt::Abort, true) => 0xFFF8,
        }
    }

    /// Set I and clear D, as every interrupt and BRK/COP does.
    fn enter_interrupt(&mut self) {
        self.regs.p = (self.regs.p | FLAG_I) & !FLAG_D;
    }
}

impl Default for Wdc65c816 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
