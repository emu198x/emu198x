use crate::mcycle::{self, MStep};
use crate::registers::Registers;
use crate::walker::Walker;

/// DDCB/FDCB fetch sequence: fetch displacement, then read sub-opcode.
/// FetchDisp reads the displacement byte and increments PC.
/// ReadAddr reads the sub-opcode from the current PC (staged by Execute)
/// WITHOUT incrementing PC — the real Z80 doesn't advance PC for the
/// DDCB sub-opcode read (it's a memory read, not an M1 fetch).
/// After this completes, walker.staged.disp has the displacement and
/// walker.staged.data_lo has the sub-opcode.
pub(crate) static DDCB_FETCH: [MStep; 2] = [MStep::FetchDisp, MStep::FetchByte];

/// Z80 CPU — half-cycle signal-level state machine.
///
/// The Z80 is a chip with pins. It exposes output signals (address bus,
/// data bus, control signals) and accepts input signals (data bus for
/// reads, WAIT, INT, NMI). Each `tick()` call advances by one half-cycle
/// of the master clock.
///
/// # Usage
///
/// The machine loop drives the Z80:
///
/// ```ignore
/// for _hc in 0..frame_halfcycles {
///     chipset.tick();                    // ULA/VDP/PPU ticks every half-cycle
///     if chipset.cpu_clock_active() {    // Chipset gates the CPU clock
///         z80.tick();
///         // Inspect z80 output signals, perform bus transactions
///         if z80.mreq && z80.rd {
///             z80.data_in = memory[z80.addr as usize];
///         }
///         // ...
///     }
///     z80.irq = chipset.interrupt_active();
/// }
/// ```
///
/// The Z80 never "calls" bus methods. The machine inspects its signals
/// and performs transactions. This matches real hardware where the CPU
/// is just another chip on the bus.
///
/// # Save states
///
/// Save states should be taken at instruction boundaries (when
/// `instruction_complete()` is true). Mid-instruction state includes
/// the walker's current `&'static [MStep]` sequence which cannot be
/// serialised; `#[serde(skip)]` restores it to the idle NOP sequence
/// on deserialisation, and the next `tick()` will fetch a fresh
/// instruction cleanly.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Z80 {
    /// Register file (public for machine inspection and test setup).
    pub regs: Registers,

    // === Output signals (Z80 → machine) ===
    /// Address bus A0-A15. Actively driven during all bus cycles.
    pub addr: u16,
    /// Data bus D0-D7. Driven by Z80 during write cycles.
    pub data: u8,
    /// Memory request. Active during memory read/write cycles.
    pub mreq: bool,
    /// I/O request. Active during I/O read/write and interrupt acknowledge.
    pub iorq: bool,
    /// Read. Active during read cycles (memory or I/O).
    pub rd: bool,
    /// Write. Active during write cycles (memory or I/O).
    pub wr: bool,
    /// Machine cycle 1. Active during opcode fetch. Also active during
    /// interrupt acknowledge (with IORQ) to distinguish IntAck from I/O.
    pub m1: bool,
    /// Refresh. Active during T3-T4 of M1 cycle. Address bus holds IR.
    pub rfsh: bool,
    /// Halt. CPU is executing phantom NOP fetches waiting for interrupt.
    pub halt: bool,

    // === Input signals (machine → Z80) ===
    /// Data bus for reads. Machine must set this before the next tick
    /// when the Z80 is performing a read (mreq && rd, or iorq && rd).
    pub data_in: u8,
    /// Wait. When asserted, the Z80 inserts wait states at specific
    /// half-cycles (T2 rise of memory accesses). Used by +2A/+3 gate
    /// array for WAIT-based contention. The Ferranti ULA uses clock
    /// gating instead (doesn't call tick at all).
    pub wait: bool,
    /// Maskable interrupt request. Level-triggered. Checked at the end
    /// of each instruction when IFF1 is set.
    pub irq: bool,
    /// Non-maskable interrupt. Edge-triggered (detected on rising edge).
    pub nmi: bool,

    // === Internal state ===
    /// Current half-cycle state in the state machine.
    ///
    /// Public so the FUSE-corpus integration test
    /// (`tests/z80_fuse.rs`) can align bus events with T-states.
    /// Production consumers should never read this — use the pin
    /// signals (`mreq`, `rd`, etc.) instead.
    pub phase: Phase,
    /// MStep sequence walker — tracks instruction progress and staged data.
    ///
    /// Public for the FUSE-corpus integration test (see note on `phase`).
    pub walker: Walker,
    /// EI was just executed — defer interrupt check by one instruction.
    pub(crate) ei_pending: bool,
    /// Previous NMI state for edge detection.
    nmi_prev: bool,

    /// Edge-detection state for `bus_request()`. The bus signals
    /// (`mreq`, `iorq`, `rd`, `wr`) are level-driven and held high for
    /// multiple half-cycles per M-cycle, so a `tick()` loop that polls
    /// `(mreq && rd)` directly would re-fire each transaction once per
    /// half-cycle. These shadow flags record the previous observation
    /// of (mreq && rd), (mreq && wr), and iorq so `bus_request()` can
    /// return `Some(BusOp)` exactly once per M-cycle (on the rising
    /// edge) and `None` thereafter until the strobe falls.
    #[serde(default)]
    prev_mr: bool,
    #[serde(default)]
    prev_mw: bool,
    #[serde(default)]
    prev_iorq: bool,
}

/// One bus transaction the Z80 is asking the host to perform. Returned
/// from [`Z80::bus_request`] exactly once per M-cycle's rising edge of
/// the corresponding strobe(s) — the host inspects this to decide what
/// to drive on the bus, and is guaranteed not to see the same M-cycle
/// twice. The host is still responsible for setting `data_in` (read
/// cycles) before the Z80 latches it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusOp {
    /// Memory read (M1 opcode fetch *or* operand read). Host drives
    /// `data_in` from `addr`.
    MemRead,
    /// Memory write. Host writes `data` to `addr`.
    MemWrite,
    /// I/O read (`IN` instruction). Host drives `data_in` from the
    /// peripheral selected by `addr`.
    IoRead,
    /// I/O write (`OUT` instruction). Host writes `data` to the
    /// peripheral selected by `addr`.
    IoWrite,
    /// Interrupt acknowledge. Host drives the IM2 vector (or 0xFF for
    /// IM1 / floating bus). M1 is held active alongside IORQ.
    IntAck,
}

/// Internal half-cycle phase of the Z80 state machine.
///
/// Each M-cycle type has a sequence of phases. The Z80 advances through
/// these phases one half-cycle at a time. Bus signals are set at specific
/// phases to match the real Z80's pin timing.
///
/// Public for the FUSE-corpus integration test; treat as crate-internal.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Phase {
    /// M1 opcode fetch: T1 through T4, rise and fall.
    /// 8 half-cycles total.
    M1(M1Phase),

    /// Memory read: T1 through T3.
    /// 6 half-cycles total.
    MemRead(MemPhase),

    /// Memory write: T1 through T3.
    /// 6 half-cycles total.
    MemWrite(MemPhase),

    /// Contended memory cycle without a read or write strobe.
    /// 6 half-cycles total.
    Contend(MemPhase),

    /// I/O read: T1 through T4 (I/O is always 4 T-states on Z80).
    /// 8 half-cycles total.
    IoRead(IoPhase),

    /// I/O write: T1 through T4.
    /// 8 half-cycles total.
    IoWrite(IoPhase),

    /// Internal operation: no bus activity, just burns time.
    /// 2 half-cycles per T-state, repeated N times.
    Internal(InternalPhase),

    /// Interrupt acknowledge: special M1-like cycle, 7+ T-states.
    IntAck(IntAckPhase),

    /// Transitional: the current M-step just completed, advance to next.
    /// This is processed immediately (0 half-cycles).
    NextStep,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum M1Phase {
    T1Rise,
    T1Fall,
    T2Rise,
    T2Fall,
    T3Rise,
    T3Fall,
    T4Rise,
    T4Fall,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MemPhase {
    T1Rise,
    T1Fall,
    T2Rise,
    T2Fall,
    T3Rise,
    T3Fall,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IoPhase {
    T1Rise,
    T1Fall,
    T2Rise,
    T2Fall,
    T3Rise,
    T3Fall,
    T4Rise,
    T4Fall,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InternalPhase {
    /// Half-cycles remaining (counts down).
    pub remaining: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IntAckPhase {
    /// IntAck is 7 T-states (14 half-cycles): 5 internal + 2 for data read.
    /// Phases T1-T5 are internal (IORQ + M1 asserted at T4).
    T1Rise,
    T1Fall,
    T2Rise,
    T2Fall,
    T3Rise,
    T3Fall,
    T4Rise,
    T4Fall,
    T5Rise,
    T5Fall,
    T6Rise,
    T6Fall,
    T7Rise,
    T7Fall,
}

impl Default for Z80 {
    fn default() -> Self {
        Self {
            regs: Registers::default(),
            addr: 0,
            data: 0,
            mreq: false,
            iorq: false,
            rd: false,
            wr: false,
            m1: false,
            rfsh: false,
            halt: false,
            data_in: 0,
            wait: false,
            irq: false,
            nmi: false,
            phase: Phase::M1(M1Phase::T1Rise),
            walker: Walker::default(),
            ei_pending: false,
            nmi_prev: false,
            prev_mr: false,
            prev_mw: false,
            prev_iorq: false,
        }
    }
}

impl Z80 {
    /// Create a new Z80 in reset state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Is the current instruction complete? True at instruction boundaries.
    pub fn instruction_complete(&self) -> bool {
        self.walker.instruction_complete
    }

    /// Re-derives `walker.sequence` from the preserved `(prefix, opcode)`
    /// after a snapshot restore.
    ///
    /// `walker.sequence` is `#[serde(skip)]` because it's a `&'static`
    /// reference to one of the per-opcode MStep tables; serde defaults
    /// it to `SEQ_NOP` on deserialise. For snapshots taken at instruction
    /// boundaries that's fine — the next `tick()` fetches a fresh
    /// instruction and overwrites the field. For snapshots taken
    /// mid-instruction the restored Z80 would walk the wrong sequence
    /// (or fall off `SEQ_NOP`'s single Execute step) and diverge from
    /// the original.
    ///
    /// Call once after `restore`, before the next `tick()`. Idempotent;
    /// safe to call when the walker is already at an instruction
    /// boundary (the lookup returns the same sequence the next fetch
    /// would have picked).
    pub fn rehydrate_walker_sequence(&mut self) {
        use crate::walker::Prefix;
        self.walker.sequence = match self.walker.prefix {
            Prefix::None => self.decode_opcode(self.walker.opcode),
            Prefix::CB => self.decode_cb(self.walker.opcode),
            Prefix::ED => self.decode_ed(self.walker.opcode),
            Prefix::DD | Prefix::FD => self.decode_dd_fd(self.walker.opcode),
            Prefix::DDCB | Prefix::FDCB => {
                if self.walker.ddcb_fetch_phase {
                    &DDCB_FETCH
                } else if (self.walker.opcode >> 6) == 1 {
                    mcycle::SEQ_DDCB_BIT
                } else {
                    mcycle::SEQ_DDCB_HL
                }
            }
        };
    }

    /// Edge-detected bus transaction request, if one fired this tick.
    ///
    /// Real Z80 bus strobes are level-driven and held active across
    /// multiple half-cycles per M-cycle (e.g. IORQ+RD is high for three
    /// consecutive phases of an `IN` instruction). A naïve dispatcher
    /// that re-reads from the peripheral every tick where
    /// `iorq && rd` would advance any FDC / AY / state-bearing port
    /// machine multiple times per instruction — which actually broke
    /// the +3 BIOS disk Loader, where the µPD765A's result FIFO was
    /// drained three times per `IN` and the BIOS only saw the first
    /// byte of each multi-byte status word.
    ///
    /// `bus_request` collapses the held strobes into one transaction
    /// per M-cycle by detecting the rising edge of (mreq && rd),
    /// (mreq && wr), and iorq, and returning `Some(BusOp)` only at
    /// that instant. Every subsequent tick within the same M-cycle
    /// returns `None`. The method is `&mut self` because it advances
    /// the shadow flags; call it once per `tick()`.
    ///
    /// Hosts that already drive the bus from their own latch (e.g.
    /// the FUSE-corpus integration test, which records every signal
    /// transition) can keep using the raw `mreq` / `iorq` / `rd` /
    /// `wr` pins; they remain part of the public surface and are
    /// unchanged. `bus_request` is purely an ergonomic dispatcher
    /// helper for "ordinary" machines.
    #[must_use]
    pub fn bus_request(&mut self) -> Option<BusOp> {
        let mr = self.mreq && self.rd;
        let mw = self.mreq && self.wr;
        let iorq = self.iorq;

        let mr_rising = mr && !self.prev_mr;
        let mw_rising = mw && !self.prev_mw;
        let iorq_rising = iorq && !self.prev_iorq;

        self.prev_mr = mr;
        self.prev_mw = mw;
        self.prev_iorq = iorq;

        // IntAck (iorq && m1) takes priority over plain io read,
        // because m1 is asserted during the entire fetch+intack cycle
        // and `iorq && m1` is the documented interrupt acknowledge
        // contract on real hardware.
        if iorq_rising && self.m1 {
            Some(BusOp::IntAck)
        } else if iorq_rising && self.rd {
            Some(BusOp::IoRead)
        } else if iorq_rising && self.wr {
            Some(BusOp::IoWrite)
        } else if mr_rising {
            Some(BusOp::MemRead)
        } else if mw_rising {
            Some(BusOp::MemWrite)
        } else {
            None
        }
    }

    /// Advance one half-cycle of the master clock.
    ///
    /// After calling, inspect output signals and perform bus transactions.
    /// Prefer [`Self::bus_request`] for ordinary host dispatchers — it
    /// collapses the held bus strobes into exactly one transaction per
    /// M-cycle. The raw level-driven pins below are still exposed for
    /// signal-trace tests and unusual peripherals:
    /// - `mreq && rd`: memory read — set `data_in = memory[addr]`
    /// - `mreq && wr`: memory write — `memory[addr] = data`
    /// - `iorq && rd && !m1`: I/O read — set `data_in = io_read(addr)`
    /// - `iorq && wr`: I/O write — `io_write(addr, data)`
    /// - `iorq && m1`: interrupt acknowledge — set `data_in = vector_byte`
    pub fn tick(&mut self) {
        // Dispatch to the appropriate phase handler
        match self.phase {
            Phase::M1(m1) => self.tick_m1(m1),
            Phase::MemRead(mr) => self.tick_mem_read(mr),
            Phase::MemWrite(mw) => self.tick_mem_write(mw),
            Phase::Contend(mc) => self.tick_contend(mc),
            Phase::IoRead(io) => self.tick_io_read(io),
            Phase::IoWrite(io) => self.tick_io_write(io),
            Phase::Internal(int) => self.tick_internal(int),
            Phase::IntAck(ia) => self.tick_int_ack(ia),
            Phase::NextStep => self.advance_to_next_step(),
        }
    }

    // === M1 Opcode Fetch ===

    fn tick_m1(&mut self, phase: M1Phase) {
        match phase {
            M1Phase::T1Rise => {
                // Address bus = PC, assert M1
                self.addr = self.regs.pc;
                self.m1 = true;
                self.mreq = false;
                self.rd = false;
                self.phase = Phase::M1(M1Phase::T1Fall);
            }
            M1Phase::T1Fall => {
                // MREQ and RD fall (active)
                self.mreq = true;
                self.rd = true;
                self.phase = Phase::M1(M1Phase::T2Rise);
            }
            M1Phase::T2Rise => {
                // Data available on bus — latch it
                // (Machine should have set data_in by now)
                // Check WAIT — if asserted, stay in this state
                if self.wait {
                    return; // Insert wait state — don't advance
                }
                self.phase = Phase::M1(M1Phase::T2Fall);
            }
            M1Phase::T2Fall => {
                // End of read: deassert MREQ, RD
                // Latch the opcode byte
                let opcode = self.data_in;
                self.mreq = false;
                self.rd = false;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                // Store opcode for decode at T4
                self.data = opcode; // Reuse data bus as temp storage
                self.phase = Phase::M1(M1Phase::T3Rise);
            }
            M1Phase::T3Rise => {
                // Refresh: IR on address bus, RFSH active.
                // MREQ goes active on T3Fall (not T3Rise) — this half-cycle
                // with addr=IR and mreq=false allows the ULA to apply
                // contention if IR is in the contended range.
                self.addr = self.regs.ir();
                self.rfsh = true;
                self.m1 = false;
                self.phase = Phase::M1(M1Phase::T3Fall);
            }
            M1Phase::T3Fall => {
                self.mreq = true; // MREQ for refresh, one half-cycle after IR on bus
                self.phase = Phase::M1(M1Phase::T4Rise);
            }
            M1Phase::T4Rise => {
                // End refresh
                self.mreq = false;
                self.rfsh = false;
                self.regs.inc_r();
                self.phase = Phase::M1(M1Phase::T4Fall);
            }
            M1Phase::T4Fall => {
                let opcode = self.data;

                match self.walker.prefix {
                    crate::walker::Prefix::None => {
                        match opcode {
                            // Prefix bytes: start another M1 fetch
                            0xCB => {
                                self.walker.prefix = crate::walker::Prefix::CB;
                                self.phase = Phase::M1(M1Phase::T1Rise);
                            }
                            0xED => {
                                self.walker.prefix = crate::walker::Prefix::ED;
                                self.phase = Phase::M1(M1Phase::T1Rise);
                            }
                            0xDD => {
                                self.walker.prefix = crate::walker::Prefix::DD;
                                self.phase = Phase::M1(M1Phase::T1Rise);
                            }
                            0xFD => {
                                self.walker.prefix = crate::walker::Prefix::FD;
                                self.phase = Phase::M1(M1Phase::T1Rise);
                            }
                            _ => {
                                self.walker.opcode = opcode;
                                self.begin_new_instruction();
                                self.walker.sequence = self.decode_opcode(opcode);
                                self.try_advance_walker();
                            }
                        }
                    }
                    crate::walker::Prefix::CB => {
                        // CB sub-opcode: decode CB instruction
                        self.walker.opcode = opcode;
                        // Keep prefix = CB so execute() dispatches correctly
                        self.begin_new_instruction();
                        self.walker.sequence = self.decode_cb(opcode);
                        self.try_advance_walker();
                    }
                    crate::walker::Prefix::ED => {
                        // ED sub-opcode: decode ED instruction
                        self.walker.opcode = opcode;
                        // Keep prefix = ED so execute() dispatches correctly
                        self.begin_new_instruction();
                        self.walker.sequence = self.decode_ed(opcode);
                        self.try_advance_walker();
                    }
                    crate::walker::Prefix::DD | crate::walker::Prefix::FD => {
                        match opcode {
                            // DD DD, DD FD, FD DD, FD FD: restart prefix
                            0xDD => {
                                self.walker.prefix = crate::walker::Prefix::DD;
                                self.phase = Phase::M1(M1Phase::T1Rise);
                            }
                            0xFD => {
                                self.walker.prefix = crate::walker::Prefix::FD;
                                self.phase = Phase::M1(M1Phase::T1Rise);
                            }
                            // DD CB / FD CB: indexed bit ops
                            // DDCB/FDCB is special: displacement byte comes BEFORE the sub-opcode
                            // Sequence: DD CB <disp> <sub-opcode>
                            // We fetch the disp+opcode via FetchDisp+FetchByte, then decode
                            0xCB => {
                                let new_prefix = if self.walker.prefix == crate::walker::Prefix::DD
                                {
                                    crate::walker::Prefix::DDCB
                                } else {
                                    crate::walker::Prefix::FDCB
                                };
                                self.walker.prefix = new_prefix;
                                self.walker.opcode = 0;
                                self.begin_new_instruction();
                                // Fetch displacement, then sub-opcode
                                self.walker.sequence = &DDCB_FETCH;
                                self.walker.ddcb_fetch_phase = true;
                                self.try_advance_walker();
                            }
                            // DD ED / FD ED: ED takes priority
                            0xED => {
                                self.walker.prefix = crate::walker::Prefix::ED;
                                self.phase = Phase::M1(M1Phase::T1Rise);
                            }
                            _ => {
                                // Regular instruction with IX/IY prefix
                                self.walker.opcode = opcode;
                                // Keep DD/FD prefix for execute dispatch
                                self.begin_new_instruction();
                                self.walker.sequence = self.decode_dd_fd(opcode);
                                self.try_advance_walker();
                            }
                        }
                    }
                    _ => {
                        // DDCB/FDCB are dispatched separately at the top of
                        // this match — this arm is unreachable.
                        self.walker.prefix = crate::walker::Prefix::None;
                        self.phase = Phase::M1(M1Phase::T1Rise);
                    }
                }
            }
        }
    }

    // === Memory Read ===

    fn tick_mem_read(&mut self, phase: MemPhase) {
        match phase {
            MemPhase::T1Rise => {
                // Address on bus (set by caller before entering this M-cycle)
                self.mreq = false;
                self.rd = false;
                self.phase = Phase::MemRead(MemPhase::T1Fall);
            }
            MemPhase::T1Fall => {
                self.mreq = true;
                self.rd = true;
                self.phase = Phase::MemRead(MemPhase::T2Rise);
            }
            MemPhase::T2Rise => {
                if self.wait {
                    return; // Wait state
                }
                self.phase = Phase::MemRead(MemPhase::T2Fall);
            }
            MemPhase::T2Fall => {
                // Latch data
                // data_in has been set by the machine
                self.mreq = false;
                self.rd = false;
                self.phase = Phase::MemRead(MemPhase::T3Rise);
            }
            MemPhase::T3Rise => {
                self.phase = Phase::MemRead(MemPhase::T3Fall);
            }
            MemPhase::T3Fall => {
                // M-cycle complete — advance walker
                self.advance_to_next_step();
            }
        }
    }

    // === Memory Write ===

    fn tick_mem_write(&mut self, phase: MemPhase) {
        match phase {
            MemPhase::T1Rise => {
                // Address on bus (set by caller)
                self.mreq = false;
                self.wr = false;
                self.phase = Phase::MemWrite(MemPhase::T1Fall);
            }
            MemPhase::T1Fall => {
                // MREQ active, data on bus
                self.mreq = true;
                // self.data already set by the walker
                self.phase = Phase::MemWrite(MemPhase::T2Rise);
            }
            MemPhase::T2Rise => {
                if self.wait {
                    return;
                }
                self.wr = true;
                self.phase = Phase::MemWrite(MemPhase::T2Fall);
            }
            MemPhase::T2Fall => {
                self.phase = Phase::MemWrite(MemPhase::T3Rise);
            }
            MemPhase::T3Rise => {
                self.mreq = false;
                self.wr = false;
                self.phase = Phase::MemWrite(MemPhase::T3Fall);
            }
            MemPhase::T3Fall => {
                // M-cycle complete — advance walker (no data to latch for writes)
                self.walker.advance();
                self.try_advance_walker();
            }
        }
    }

    // === Contended Memory Cycle (no RD/WR strobe) ===

    fn tick_contend(&mut self, phase: MemPhase) {
        match phase {
            MemPhase::T1Rise => {
                self.mreq = false;
                self.rd = false;
                self.wr = false;
                self.phase = Phase::Contend(MemPhase::T1Fall);
            }
            MemPhase::T1Fall => {
                self.mreq = true;
                self.phase = Phase::Contend(MemPhase::T2Rise);
            }
            MemPhase::T2Rise => {
                if self.wait {
                    return;
                }
                self.phase = Phase::Contend(MemPhase::T2Fall);
            }
            MemPhase::T2Fall => {
                self.mreq = false;
                self.phase = Phase::Contend(MemPhase::T3Rise);
            }
            MemPhase::T3Rise => {
                self.phase = Phase::Contend(MemPhase::T3Fall);
            }
            MemPhase::T3Fall => {
                self.walker.advance();
                self.try_advance_walker();
            }
        }
    }

    // === I/O Read ===

    fn tick_io_read(&mut self, phase: IoPhase) {
        match phase {
            IoPhase::T1Rise => {
                // Port address on bus
                self.phase = Phase::IoRead(IoPhase::T1Fall);
            }
            IoPhase::T1Fall => {
                self.phase = Phase::IoRead(IoPhase::T2Rise);
            }
            IoPhase::T2Rise => {
                // IORQ and RD active
                self.iorq = true;
                self.rd = true;
                self.phase = Phase::IoRead(IoPhase::T2Fall);
            }
            IoPhase::T2Fall => {
                if self.wait {
                    return; // I/O wait state
                }
                self.phase = Phase::IoRead(IoPhase::T3Rise);
            }
            IoPhase::T3Rise => {
                // Data available
                self.phase = Phase::IoRead(IoPhase::T3Fall);
            }
            IoPhase::T3Fall => {
                self.iorq = false;
                self.rd = false;
                self.phase = Phase::IoRead(IoPhase::T4Rise);
            }
            IoPhase::T4Rise => {
                self.phase = Phase::IoRead(IoPhase::T4Fall);
            }
            IoPhase::T4Fall => {
                self.advance_to_next_step();
            }
        }
    }

    // === I/O Write ===

    fn tick_io_write(&mut self, phase: IoPhase) {
        match phase {
            IoPhase::T1Rise => {
                // Port address on bus
                self.phase = Phase::IoWrite(IoPhase::T1Fall);
            }
            IoPhase::T1Fall => {
                // Data on bus
                self.phase = Phase::IoWrite(IoPhase::T2Rise);
            }
            IoPhase::T2Rise => {
                self.iorq = true;
                self.wr = true;
                self.phase = Phase::IoWrite(IoPhase::T2Fall);
            }
            IoPhase::T2Fall => {
                if self.wait {
                    return;
                }
                self.phase = Phase::IoWrite(IoPhase::T3Rise);
            }
            IoPhase::T3Rise => {
                self.phase = Phase::IoWrite(IoPhase::T3Fall);
            }
            IoPhase::T3Fall => {
                self.iorq = false;
                self.wr = false;
                self.phase = Phase::IoWrite(IoPhase::T4Rise);
            }
            IoPhase::T4Rise => {
                self.phase = Phase::IoWrite(IoPhase::T4Fall);
            }
            IoPhase::T4Fall => {
                self.walker.advance();
                self.try_advance_walker();
            }
        }
    }

    // === Internal (no bus activity) ===

    fn tick_internal(&mut self, phase: InternalPhase) {
        if phase.remaining <= 1 {
            self.walker.advance();
            self.try_advance_walker();
        } else {
            self.phase = Phase::Internal(InternalPhase {
                remaining: phase.remaining - 1,
            });
        }
    }

    // === Interrupt Acknowledge ===

    fn tick_int_ack(&mut self, phase: IntAckPhase) {
        match phase {
            IntAckPhase::T1Rise => {
                self.m1 = true;
                self.phase = Phase::IntAck(IntAckPhase::T1Fall);
            }
            IntAckPhase::T1Fall => {
                self.phase = Phase::IntAck(IntAckPhase::T2Rise);
            }
            IntAckPhase::T2Rise => {
                self.phase = Phase::IntAck(IntAckPhase::T2Fall);
            }
            IntAckPhase::T2Fall => {
                self.phase = Phase::IntAck(IntAckPhase::T3Rise);
            }
            IntAckPhase::T3Rise => {
                self.phase = Phase::IntAck(IntAckPhase::T3Fall);
            }
            IntAckPhase::T3Fall => {
                self.phase = Phase::IntAck(IntAckPhase::T4Rise);
            }
            IntAckPhase::T4Rise => {
                // IORQ asserted (with M1 already active = IntAck)
                self.iorq = true;
                self.phase = Phase::IntAck(IntAckPhase::T4Fall);
            }
            IntAckPhase::T4Fall => {
                if self.wait {
                    return;
                }
                self.phase = Phase::IntAck(IntAckPhase::T5Rise);
            }
            IntAckPhase::T5Rise => {
                self.phase = Phase::IntAck(IntAckPhase::T5Fall);
            }
            IntAckPhase::T5Fall => {
                // Latch interrupt data from bus
                self.phase = Phase::IntAck(IntAckPhase::T6Rise);
            }
            IntAckPhase::T6Rise => {
                self.iorq = false;
                self.m1 = false;
                self.phase = Phase::IntAck(IntAckPhase::T6Fall);
            }
            IntAckPhase::T6Fall => {
                self.phase = Phase::IntAck(IntAckPhase::T7Rise);
            }
            IntAckPhase::T7Rise => {
                self.phase = Phase::IntAck(IntAckPhase::T7Fall);
            }
            IntAckPhase::T7Fall => {
                // IntAck complete — advance walker
                self.advance_to_next_step();
            }
        }
    }

    // === Step Advancement ===

    /// Called when an M-cycle phase sequence completes (Phase::NextStep).
    /// Latches data from the completed step, advances the walker, and
    /// enters the next step's phase — or starts a new M1 fetch.
    fn advance_to_next_step(&mut self) {
        // Latch data from the just-completed step
        let data_in = self.data_in;
        self.walker.latch_read(data_in, &mut self.regs);

        // Advance to next step
        let done = self.walker.advance();
        if done {
            self.begin_next_instruction();
            return;
        }

        self.try_advance_walker();
    }

    /// Process the walker's current step. If it's Execute (0 HC), process
    /// it immediately and keep advancing. Otherwise, enter the step's phase.
    fn try_advance_walker(&mut self) {
        loop {
            match self.walker.begin_current_step() {
                Some(phase) => {
                    // Set up signals for this step
                    self.walker
                        .setup_signals(&mut self.addr, &mut self.data, &mut self.regs);
                    self.phase = phase;
                    return;
                }
                None => {
                    // Execute step (0 HC) or sequence complete
                    if let Some(mcycle::MStep::Execute) = self.walker.current_step() {
                        self.execute_operation();
                        let done = self.walker.advance();
                        if done {
                            self.begin_next_instruction();
                            return;
                        }
                        // Loop to process next step (might be another Execute)
                    } else {
                        // Sequence complete
                        self.begin_next_instruction();
                        return;
                    }
                }
            }
        }
    }

    /// Start the next instruction's M1 fetch.
    fn begin_next_instruction(&mut self) {
        // Check if we just completed the DDCB/FDCB fetch phase.
        // The fetch phase uses DDCB_FETCH sequence. If we're in DDCB/FDCB prefix and
        // the current sequence is the fetch sequence, transition to execution phase.
        if self.walker.ddcb_fetch_phase {
            self.walker.ddcb_fetch_phase = false;
            // The fetch phase put disp in staged.disp and sub-opcode in staged.data_lo
            let sub_opcode = self.walker.staged.data_lo;
            self.walker.opcode = sub_opcode;
            let op_type = sub_opcode >> 6;

            // Reset step index for the execution phase
            self.walker.step_idx = 0;
            self.walker.sequence = if op_type == 1 {
                mcycle::SEQ_DDCB_BIT // BIT: read-only
            } else {
                mcycle::SEQ_DDCB_HL // rotate/shift/SET/RES: read-modify-write
            };
            self.try_advance_walker();
            return;
        }

        self.walker.instruction_complete = true;
        self.walker.prefix = crate::walker::Prefix::None;

        // Q register: instructions that modify flags set Q = F (in execute).
        // Instructions that don't modify flags: Q was already set to 0 at
        // instruction start (see begin_instruction below). So Q naturally
        // reflects whether this instruction modified flags.

        // Check for interrupts

        // NMI is edge-triggered: detect rising edge
        let nmi_edge = self.nmi && !self.nmi_prev;
        self.nmi_prev = self.nmi;

        if nmi_edge {
            self.halt = false;
            self.regs.iff1 = false; // NMI disables IFF1 (but not IFF2)
            self.begin_new_instruction();
            self.walker.sequence = mcycle::SEQ_NMI;
            self.walker.opcode = 0; // not a real opcode
            self.try_advance_walker();
            return;
        }

        // IRQ is level-triggered, checked if IFF1 is set
        if self.irq && self.regs.iff1 && !self.ei_pending {
            self.halt = false;
            self.regs.iff1 = false;
            self.regs.iff2 = false;
            self.begin_new_instruction();
            self.walker.sequence = match self.regs.im {
                0 | 1 => mcycle::SEQ_INT_IM1,
                2 => mcycle::SEQ_INT_IM2,
                _ => mcycle::SEQ_INT_IM1,
            };
            self.walker.opcode = 0;
            self.try_advance_walker();
            return;
        }

        // Clear EI pending flag (EI defers interrupts by one instruction)
        self.ei_pending = false;

        // HALT: re-execute the HALT opcode by rewinding PC one byte. The
        // real Z80 stays at the same PC and runs phantom 4 T-state M1
        // fetches forever until IRQ/NMI clears `halt`; equivalently we
        // back PC up to the HALT byte each instruction boundary so the
        // next M1 fetch reads HALT again. PC oscillates between the
        // HALT byte and the byte after across each phantom cycle (T2Fall
        // advances it during fetch). When IRQ accept fires in the
        // branches above, the M1 fetch that latched HALT has already
        // completed — so PC at that moment is the byte after HALT,
        // which is the address pushed to the stack. RETI / RET from the
        // ISR therefore returns past HALT, matching real-hardware
        // behaviour.
        if self.halt {
            self.regs.pc = self.regs.pc.wrapping_sub(1);
        }

        // Normal: start next M1 fetch
        self.phase = Phase::M1(M1Phase::T1Rise);
    }

    /// Execute the current instruction's operation using staged data.
    fn execute_operation(&mut self) {
        crate::execute::execute(self);
    }

    /// Begin a new instruction: reset walker and Q register.
    fn begin_new_instruction(&mut self) {
        self.walker.begin_instruction();
        // Q register: save the previous instruction's Q value for SCF/CCF,
        // then reset Q to 0. Flag-modifying instructions will set Q = F
        // via set_f_q() during their Execute step.
        self.regs.prev_q = self.regs.q;
        self.regs.q = 0;
    }

    /// Decode an unprefixed opcode into its MStep sequence.
    fn decode_opcode(&self, opcode: u8) -> &'static [mcycle::MStep] {
        // This will be a 256-entry lookup table.
        // For now, handle the most common instructions.
        match opcode {
            0x00 => mcycle::SEQ_NOP,

            // LD r, r' — register to register (0x40-0x7F excluding 0x76 HALT and (HL) ops)
            0x40..=0x7F if opcode != 0x76 && (opcode & 0x07) != 0x06 && (opcode & 0x38) != 0x30 => {
                mcycle::SEQ_LD_R_R
            }
            // LD r, (HL) — 0x46, 0x4E, 0x56, 0x5E, 0x66, 0x6E, 0x7E
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => mcycle::SEQ_LD_R_HL,
            // LD (HL), r — 0x70-0x77 excluding 0x76 (HALT)
            0x70..=0x75 | 0x77 => mcycle::SEQ_LD_HL_R,

            // HALT
            0x76 => mcycle::SEQ_HALT,

            // LD r, n — 0x06, 0x0E, 0x16, 0x1E, 0x26, 0x2E, 0x3E
            0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x3E => mcycle::SEQ_LD_R_N,
            // LD (HL), n
            0x36 => mcycle::SEQ_LD_HL_N,

            // LD A, (BC)
            0x0A => mcycle::SEQ_LD_A_IND,
            // LD A, (DE)
            0x1A => mcycle::SEQ_LD_A_IND,
            // LD (BC), A
            0x02 => mcycle::SEQ_LD_IND_A,
            // LD (DE), A
            0x12 => mcycle::SEQ_LD_IND_A,

            // LD A, (nn)
            0x3A => mcycle::SEQ_LD_A_NN,
            // LD (nn), A
            0x32 => mcycle::SEQ_LD_NN_A,

            // LD rr, nn — 0x01, 0x11, 0x21, 0x31
            0x01 | 0x11 | 0x21 | 0x31 => mcycle::SEQ_LD_RR_NN,

            // LD SP, HL
            0xF9 => mcycle::SEQ_LD_SP_HL,

            // LD (nn), HL
            0x22 => mcycle::SEQ_LD_NN_RR,
            // LD HL, (nn)
            0x2A => mcycle::SEQ_LD_RR_NN_IND,

            // PUSH rr — 0xC5, 0xD5, 0xE5, 0xF5
            0xC5 | 0xD5 | 0xE5 | 0xF5 => mcycle::SEQ_PUSH,
            // POP rr — 0xC1, 0xD1, 0xE1, 0xF1
            0xC1 | 0xD1 | 0xE1 | 0xF1 => mcycle::SEQ_POP,

            // ALU A, r — 0x80-0xBF (lower 3 bits = source reg, excluding (HL))
            0x80..=0xBF if (opcode & 0x07) != 0x06 => mcycle::SEQ_ALU_R,
            // ALU A, (HL) — 0x86, 0x8E, 0x96, 0x9E, 0xA6, 0xAE, 0xB6, 0xBE
            0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => mcycle::SEQ_ALU_HL,
            // ALU A, n — 0xC6, 0xCE, 0xD6, 0xDE, 0xE6, 0xEE, 0xF6, 0xFE
            0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => mcycle::SEQ_ALU_N,

            // INC r — 0x04, 0x0C, 0x14, 0x1C, 0x24, 0x2C, 0x3C
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x3C => mcycle::SEQ_ALU_R,
            // DEC r — 0x05, 0x0D, 0x15, 0x1D, 0x25, 0x2D, 0x3D
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x3D => mcycle::SEQ_ALU_R,
            // INC (HL)
            0x34 => mcycle::SEQ_INC_DEC_HL,
            // DEC (HL)
            0x35 => mcycle::SEQ_INC_DEC_HL,

            // INC rr — 0x03, 0x13, 0x23, 0x33
            0x03 | 0x13 | 0x23 | 0x33 => mcycle::SEQ_INC_DEC_RR,
            // DEC rr — 0x0B, 0x1B, 0x2B, 0x3B
            0x0B | 0x1B | 0x2B | 0x3B => mcycle::SEQ_INC_DEC_RR,

            // ADD HL, rr — 0x09, 0x19, 0x29, 0x39
            0x09 | 0x19 | 0x29 | 0x39 => mcycle::SEQ_ADD_HL_RR,

            // JP nn
            0xC3 => mcycle::SEQ_JP_NN,
            // JP cc, nn — 0xC2, 0xCA, 0xD2, 0xDA, 0xE2, 0xEA, 0xF2, 0xFA
            0xC2 | 0xCA | 0xD2 | 0xDA | 0xE2 | 0xEA | 0xF2 | 0xFA => mcycle::SEQ_JP_CC_NN,
            // JP (HL)
            0xE9 => mcycle::SEQ_JP_HL,

            // JR e
            0x18 => mcycle::SEQ_JR_E,
            // JR cc, e
            0x20 | 0x28 | 0x30 | 0x38 => {
                let cc = (opcode >> 3) & 0x03;
                if crate::alu::condition(&self.regs, cc) {
                    mcycle::SEQ_JR_CC_TAKEN
                } else {
                    mcycle::SEQ_JR_CC_NOT_TAKEN
                }
            }

            // DJNZ e
            0x10 => {
                if self.regs.b().wrapping_sub(1) != 0 {
                    mcycle::SEQ_DJNZ_TAKEN
                } else {
                    mcycle::SEQ_DJNZ_NOT_TAKEN
                }
            }

            // CALL nn
            0xCD => mcycle::SEQ_CALL_NN,
            // CALL cc, nn
            0xC4 | 0xCC | 0xD4 | 0xDC | 0xE4 | 0xEC | 0xF4 | 0xFC => mcycle::SEQ_CALL_CC,

            // RET
            0xC9 => mcycle::SEQ_RET,
            // RET cc — Execute checks condition before PopLo/PopHi
            0xC0 | 0xC8 | 0xD0 | 0xD8 | 0xE0 | 0xE8 | 0xF0 | 0xF8 => mcycle::SEQ_RET_CC,

            // RST — 0xC7, 0xCF, 0xD7, 0xDF, 0xE7, 0xEF, 0xF7, 0xFF
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => mcycle::SEQ_RST,

            // Rotates on A
            0x07 => mcycle::SEQ_RLCA,
            0x0F => mcycle::SEQ_RRCA,
            0x17 => mcycle::SEQ_RLA,
            0x1F => mcycle::SEQ_RRA,

            // Misc single-byte
            0x27 => mcycle::SEQ_DAA,
            0x2F => mcycle::SEQ_CPL,
            0x37 => mcycle::SEQ_SCF,
            0x3F => mcycle::SEQ_CCF,
            0x08 => mcycle::SEQ_EX_AF,
            0xD9 => mcycle::SEQ_EXX,
            0xEB => mcycle::SEQ_EX_DE_HL,
            0xE3 => mcycle::SEQ_EX_SP_HL,
            0xF3 => mcycle::SEQ_DI,
            0xFB => mcycle::SEQ_EI,

            // IN A, (n)
            0xDB => mcycle::SEQ_IN_A_N,
            // OUT (n), A
            0xD3 => mcycle::SEQ_OUT_N_A,

            // Prefixes are handled in M1 T4Fall, not here.
            // If we get here something is wrong — treat as NOP.
            0xCB | 0xED | 0xDD | 0xFD => mcycle::SEQ_NOP,

            // Catch-all: treat unknown as NOP (will be filled in)
            _ => mcycle::SEQ_NOP,
        }
    }

    /// Decode a CB-prefix sub-opcode.
    fn decode_cb(&self, opcode: u8) -> &'static [mcycle::MStep] {
        let r = opcode & 0x07;
        let op_type = opcode >> 6; // 0=rotate/shift, 1=BIT, 2=RES, 3=SET

        if r == 6 {
            // (HL) operand
            if op_type == 1 {
                mcycle::SEQ_CB_BIT_HL // BIT b, (HL): read-only
            } else {
                mcycle::SEQ_CB_HL // RLC/SET/RES (HL): read-modify-write
            }
        } else {
            mcycle::SEQ_CB_R // Register operand
        }
    }

    /// Decode an ED-prefix sub-opcode.
    fn decode_ed(&self, opcode: u8) -> &'static [mcycle::MStep] {
        match opcode {
            // LD I, A / LD R, A / LD A, I / LD A, R
            0x47 | 0x4F | 0x57 | 0x5F => mcycle::SEQ_LD_IR,

            // NEG (and undocumented mirrors)
            0x44 | 0x4C | 0x54 | 0x5C | 0x64 | 0x6C | 0x74 | 0x7C => mcycle::SEQ_NEG,

            // RETI / RETN (and mirrors)
            0x45 | 0x4D | 0x55 | 0x5D | 0x65 | 0x6D | 0x75 | 0x7D => mcycle::SEQ_RETI,

            // IM 0 / IM 1 / IM 2 (and mirrors)
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x76 | 0x7E => mcycle::SEQ_IM,

            // IN r, (C) — 0x40, 0x48, 0x50, 0x58, 0x60, 0x68, 0x70, 0x78
            0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x70 | 0x78 => mcycle::SEQ_IN_R_C,

            // OUT (C), r — 0x41, 0x49, 0x51, 0x59, 0x61, 0x69, 0x71, 0x79
            0x41 | 0x49 | 0x51 | 0x59 | 0x61 | 0x69 | 0x71 | 0x79 => mcycle::SEQ_OUT_C_R,

            // SBC HL, rr — 0x42, 0x52, 0x62, 0x72
            0x42 | 0x52 | 0x62 | 0x72 => mcycle::SEQ_ADD_HL_RR, // same timing as ADD HL,rr

            // ADC HL, rr — 0x4A, 0x5A, 0x6A, 0x7A
            0x4A | 0x5A | 0x6A | 0x7A => mcycle::SEQ_ADD_HL_RR,

            // LD (nn), rr — 0x43, 0x53, 0x63, 0x73
            0x43 | 0x53 | 0x63 | 0x73 => mcycle::SEQ_LD_NN_RR,

            // LD rr, (nn) — 0x4B, 0x5B, 0x6B, 0x7B
            0x4B | 0x5B | 0x6B | 0x7B => mcycle::SEQ_LD_RR_NN_IND,

            // RLD / RRD
            0x67 | 0x6F => mcycle::SEQ_RLD_RRD,

            // LDI / LDD
            0xA0 | 0xA8 => mcycle::SEQ_LDI,

            // LDIR / LDDR (handled with repeat check in execute)
            0xB0 | 0xB8 => mcycle::SEQ_LDIR_REPEAT, // Execute will set done if BC=0

            // CPI / CPD
            0xA1 | 0xA9 => mcycle::SEQ_CPI,

            // CPIR / CPDR
            0xB1 | 0xB9 => mcycle::SEQ_CPIR_REPEAT,

            // INI / IND
            0xA2 | 0xAA => mcycle::SEQ_INI,

            // INIR / INDR
            0xB2 | 0xBA => mcycle::SEQ_INIR_REPEAT,

            // OUTI / OUTD
            0xA3 | 0xAB => mcycle::SEQ_OUTI,

            // OTIR / OTDR
            0xB3 | 0xBB => mcycle::SEQ_OTIR_REPEAT,

            // Undocumented: ED + anything else = NOP (8 T-states: two M1 fetches)
            _ => mcycle::SEQ_NOP,
        }
    }

    /// Decode a DD/FD-prefixed opcode.
    /// If the opcode uses (HL), substitute with indexed (IX+d)/(IY+d) sequences.
    /// If it uses H or L, the execute dispatch handles IXH/IXL substitution.
    /// If it doesn't use HL/H/L at all, use the unprefixed sequence.
    fn decode_dd_fd(&self, opcode: u8) -> &'static [mcycle::MStep] {
        match opcode {
            // Instructions that use (HL) → (IX+d)/(IY+d)
            // LD r, (HL) → LD r, (IX+d)
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => mcycle::SEQ_LD_R_IXD,
            // LD (HL), r → LD (IX+d), r
            0x70..=0x75 | 0x77 => mcycle::SEQ_LD_IXD_R,
            // LD (HL), n → LD (IX+d), n
            0x36 => mcycle::SEQ_LD_IXD_N,
            // ALU A, (HL) → ALU A, (IX+d)
            0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => mcycle::SEQ_ALU_IXD,
            // INC (HL) → INC (IX+d)
            0x34 => mcycle::SEQ_INC_DEC_IXD,
            // DEC (HL) → DEC (IX+d)
            0x35 => mcycle::SEQ_INC_DEC_IXD,

            // Instructions that use HL as 16-bit but NOT (HL) — same timing, execute substitutes IX/IY
            // ADD IX, rr (same timing as ADD HL, rr)
            0x09 | 0x19 | 0x29 | 0x39 => mcycle::SEQ_ADD_HL_RR,
            // LD IX, nn
            0x21 => mcycle::SEQ_LD_RR_NN,
            // LD (nn), IX
            0x22 => mcycle::SEQ_LD_NN_RR,
            // LD IX, (nn)
            0x2A => mcycle::SEQ_LD_RR_NN_IND,
            // INC IX / DEC IX
            0x23 | 0x2B => mcycle::SEQ_INC_DEC_RR,
            // LD SP, IX
            0xF9 => mcycle::SEQ_LD_SP_HL,
            // PUSH IX
            0xE5 => mcycle::SEQ_PUSH,
            // POP IX
            0xE1 => mcycle::SEQ_POP,
            // EX (SP), IX
            0xE3 => mcycle::SEQ_EX_SP_HL,
            // JP (IX)
            0xE9 => mcycle::SEQ_JP_HL,

            // Instructions that use H/L → IXH/IXL (undocumented, same timing)
            // LD r, IXH/IXL or LD IXH/IXL, r etc.
            // These use the same sequences as unprefixed — the execute dispatch
            // handles the register substitution.

            // All other instructions: DD/FD is ignored (pass through to unprefixed)
            _ => self.decode_opcode(opcode),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state() {
        let z80 = Z80::new();
        assert_eq!(z80.regs.pc, 0);
        assert_eq!(z80.regs.sp, 0xFFFF);
        assert!(!z80.mreq);
        assert!(!z80.iorq);
        assert!(!z80.rd);
        assert!(!z80.wr);
        assert!(!z80.halt);
        assert_eq!(z80.phase, Phase::M1(M1Phase::T1Rise));
    }

    #[test]
    fn m1_fetch_signals() {
        let mut z80 = Z80::new();
        let mut mem = [0u8; 65536];
        mem[0] = 0x00; // NOP

        // T1 rise: address on bus, M1 asserted
        z80.tick();
        assert_eq!(z80.addr, 0x0000);
        assert!(z80.m1);
        assert!(!z80.mreq);
        assert!(!z80.rd);

        // T1 fall: MREQ and RD active
        z80.tick();
        assert!(z80.mreq);
        assert!(z80.rd);

        // Machine responds: put data on bus
        z80.data_in = mem[z80.addr as usize];

        // T2 rise: data latched
        z80.tick();

        // T2 fall: end read, PC incremented
        z80.tick();
        assert!(!z80.mreq);
        assert!(!z80.rd);
        assert_eq!(z80.regs.pc, 1);

        // T3 rise: refresh — IR on bus, RFSH active, MREQ not yet active
        // (MREQ goes active at T3 fall, one half-cycle later — this window
        // allows the ULA to apply contention if IR is in contended memory)
        z80.tick();
        assert!(z80.rfsh);
        assert!(!z80.mreq); // MREQ not yet active at T3 rise
        assert!(!z80.m1);

        // T3 fall: MREQ goes active for refresh
        z80.tick();
        assert!(z80.mreq);

        // T4 rise: end refresh, R incremented
        z80.tick();
        assert!(!z80.rfsh);
        assert!(!z80.mreq);
        assert_eq!(z80.regs.r, 1);

        // T4 fall: decode
        z80.tick();
        // After 8 half-cycles, we should be back at M1 T1 rise (NOP loops)
    }

    #[test]
    fn new_matches_default_and_reports_instruction_boundary() {
        // `new()` is documented as the reset constructor — it must be
        // identical to `default()` and start at an instruction boundary
        // so a fresh CPU is ready to fetch.
        let from_new = Z80::new();
        let from_default = Z80::default();
        assert_eq!(from_new.regs.pc, from_default.regs.pc);
        assert_eq!(from_new.regs.sp, from_default.regs.sp);
        assert_eq!(from_new.phase, from_default.phase);
        assert!(from_new.instruction_complete());
    }

    #[test]
    fn nop_run_returns_to_instruction_boundary() {
        // After a complete NOP fetch (8 half-cycles), the CPU must be
        // back at an instruction boundary with PC advanced by one.
        let mut z80 = Z80::new();
        let mut mem = [0u8; 65_536];
        mem[0] = 0x00; // NOP

        for _ in 0..8 {
            z80.tick();
            if z80.mreq && z80.rd {
                z80.data_in = mem[z80.addr as usize];
            }
        }

        assert!(z80.instruction_complete());
        assert_eq!(z80.regs.pc, 1);
        assert_eq!(z80.phase, Phase::M1(M1Phase::T1Rise));
    }

    #[test]
    fn rehydrate_mid_instruction_walker_matches_forward_execution() {
        // LD A, n (opcode 0x3E) takes two M-cycles: M1 fetch (4 T-states)
        // then a memory read for the immediate (3 T-states). Snapshotting
        // mid-instruction via serde would default `walker.sequence` to
        // SEQ_NOP. Without rehydration the restored Z80 walks the wrong
        // sequence and diverges from the original.
        let mut original = Z80::new();
        let mut mem = [0u8; 65_536];
        mem[0] = 0x3E; // LD A, n
        mem[1] = 0x42; // n = 0x42
        mem[2] = 0x00; // NOP follow-on

        // Tick through M1 fetch (8 half-cycles) so the walker's sequence
        // is now the LD A, n sequence and step_idx is mid-instruction.
        for _ in 0..8 {
            original.tick();
            if original.mreq && original.rd {
                original.data_in = mem[original.addr as usize];
            }
        }

        assert!(
            !original.instruction_complete(),
            "expected mid-instruction state after LD A, n M1 fetch"
        );
        assert_eq!(original.walker.opcode, 0x3E);

        // Round-trip via serde — same code path the Spectrum snapshot uses.
        // serde_json is the in-tree dev-dep; serde format is irrelevant to the
        // bug (the `#[serde(skip)]` fallback fires regardless of format).
        let serialized = serde_json::to_string(&original).expect("encode");
        let mut restored: Z80 = serde_json::from_str(&serialized).expect("decode");

        // Without rehydration, walker.sequence has fallen back to SEQ_NOP.
        // Rehydrate from (prefix, opcode) to restore the real sequence.
        restored.rehydrate_walker_sequence();

        // Run both forward through the rest of the instruction and one
        // follow-on NOP. Bus transactions must be byte-identical.
        let mut original_bus_trace: Vec<(u16, u8)> = Vec::new();
        let mut restored_bus_trace: Vec<(u16, u8)> = Vec::new();

        for _ in 0..16 {
            original.tick();
            if original.mreq && original.rd {
                original.data_in = mem[original.addr as usize];
                original_bus_trace.push((original.addr, original.data_in));
            }

            restored.tick();
            if restored.mreq && restored.rd {
                restored.data_in = mem[restored.addr as usize];
                restored_bus_trace.push((restored.addr, restored.data_in));
            }
        }

        assert_eq!(
            original_bus_trace, restored_bus_trace,
            "rehydrated Z80 must produce identical bus reads to the original"
        );
        assert_eq!(original.regs.a(), 0x42);
        assert_eq!(restored.regs.a(), 0x42);
        assert_eq!(original.regs.pc, restored.regs.pc);
    }

    #[test]
    fn halt_blocks_until_irq_then_irq_returns_past_halt() {
        // HALT must run phantom 4-T-state cycles forever (PC stuck at the
        // byte after HALT) until an IRQ fires, then push that post-HALT
        // address to the stack so RETI returns past HALT, not to HALT.
        //
        // Layout: PC=$0000 starts with HALT (0x76). Memory after HALT is
        // 0x18 0xFE (JR -2, a tight loop) so any mistaken fetch past
        // HALT would spin without crashing — diagnostic, not the test.
        let mut z80 = Z80::new();
        let mut mem = [0u8; 65_536];
        mem[0x0000] = 0x76; // HALT
        mem[0x0001] = 0x18; // JR
        mem[0x0002] = 0xFE; // -2

        z80.regs.iff1 = true;
        z80.regs.im = 1;
        z80.regs.sp = 0xFF00;

        // Tick the CPU until halt latches, then 200 more ticks (~25 phantom
        // M1 cycles of 8 half-cycles each). Observe that PC settles to 1
        // (post-HALT) and R has incremented many times — proving the
        // phantom-fetch loop is doing work.
        let mut halt_seen = false;
        for _ in 0..16 {
            z80.tick();
            if z80.mreq && z80.rd {
                z80.data_in = mem[z80.addr as usize];
            }
            if z80.halt {
                halt_seen = true;
                break;
            }
        }
        assert!(halt_seen, "HALT flag should set during the first instruction");
        let initial_r = z80.regs.r;

        // Run 200 half-cycles with no IRQ. Halt must persist, PC must
        // not run past the HALT byte, and R must keep incrementing
        // (proving real M1 cycles are happening).
        for _ in 0..200 {
            z80.tick();
            if z80.mreq && z80.rd {
                z80.data_in = mem[z80.addr as usize];
            }
            assert!(z80.halt, "halt must persist while IRQ is low");
            assert!(
                z80.regs.pc <= 0x0001,
                "PC must oscillate between HALT byte and the one after, never escape (got {:#06x})",
                z80.regs.pc,
            );
        }
        assert!(
            z80.regs.r.wrapping_sub(initial_r) >= 10,
            "R should have incremented across many phantom M1 cycles ({} → {})",
            initial_r,
            z80.regs.r,
        );

        // Raise IRQ. Within ~16 half-cycles the CPU should accept it,
        // clear halt, and start dispatching the IM1 service routine.
        z80.irq = true;
        let mut accepted = false;
        for _ in 0..32 {
            z80.tick();
            if z80.mreq && z80.rd {
                z80.data_in = mem[z80.addr as usize];
            }
            if !z80.halt {
                accepted = true;
                break;
            }
        }
        assert!(accepted, "IRQ must clear halt and break the phantom loop");

        // Run the IM1 service long enough for both stack pushes to land,
        // then read SP-1 / SP-2 from memory: after IM1 dispatch SP=0xFEFE
        // and (0xFEFE / 0xFEFF) hold the pushed PC's low / high bytes.
        // The pushed value must be 0x0001 (the byte after HALT), so RETI
        // returns past HALT rather than back to it.
        for _ in 0..64 {
            z80.tick();
            if z80.mreq && z80.rd {
                z80.data_in = mem[z80.addr as usize];
            }
            if z80.mreq && z80.wr {
                mem[z80.addr as usize] = z80.data;
            }
        }
        let saved_pc = u16::from(mem[0xFEFE]) | (u16::from(mem[0xFEFF]) << 8);
        assert_eq!(
            saved_pc, 0x0001,
            "IRQ must push the post-HALT address (0x0001), not the HALT address (0x0000); pushed {saved_pc:#06x}",
        );
    }
}
