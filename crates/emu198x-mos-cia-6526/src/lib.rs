//! MOS 6526 CIA (Complex Interface Adapter).
//!
//! Timers and the interrupt flag register are cycle-accurate ports of VICE's
//! CIA core (`emulators/c64/vice-3.10/src/core/ciacore.c` + `ciatimer.{h,c}`,
//! issue #17): the interval timers run the 6526's internal pipeline (see
//! [`timer`]), and IFR/IRQ effects that the silicon delays are carried in a
//! shift-register delay line (`ifr_delay`, VICE's "software mercury delay
//! line"):
//!
//! - An interrupt source sets its ICR flag bit immediately, but on the old
//!   6526 the IR bit (ICR bit 7) and the /IRQ line follow **one cycle
//!   later** (`D7SET`/`RAISE` stages). The 6526A ("new CIA") asserts in the
//!   same cycle unless the ICR was read in the previous cycle.
//! - Reading the ICR drops the line at once, but the flag-clearing ACK runs
//!   down its own pipeline stage.
//! - The old 6526 has the **timer B bug**: reading the ICR exactly one cycle
//!   before a Timer B underflow eats the TB flag (it is set, then consumed
//!   by the in-flight acknowledge) — the read that observes it never
//!   reports it and no interrupt fires.
//! - Writing the ICR mask so that a pending flag becomes enabled raises the
//!   interrupt through the same delayed pipeline (two cycles on the old
//!   6526).
//!
//! PB6/PB7 can carry the timer outputs (CRA/CRB bit 1), in pulse mode (high
//! for the underflow cycle) or toggle mode (flips each underflow, set high
//! on START).
//!
//! The serial port and the CNT pin (MOS 6526 datasheet, "Serial Port (SDR)"
//! and "Input Modes"; `reference/by-topic/cia-6526/mos-6526-cia-recreated`):
//!
//! - **Output** (CRA bit 6 set) is VICE's `sdr_delay` pipeline
//!   (`ciacore_intsdr`): Timer A underflows toggle CNT about 1.5 cycles
//!   later, SP changes on each falling CNT edge, MSB first, and the SDR
//!   interrupt follows the eighth bit by two cycles. A byte written to the
//!   SDR mid-transfer is chained on.
//! - **Input** (CRA bit 6 clear) shifts SP in on each rising CNT edge and
//!   loads the SDR, with the SDR interrupt, after eight (VICE
//!   `ciacore_set_cnt`).
//! - **CNT as a timer clock**: Timer A (CRA bit 5) and Timer B (CRB bits
//!   6:5 = 01) count rising CNT edges, and Timer B mode 11 counts Timer A
//!   underflows only while CNT is high. VICE leaves this as a TODO; the
//!   edge-to-count latency follows the C64 MiSTer core's `mos6526.v`
//!   (`countA0`/`countA1` ahead of the φ2 pipeline).
//!
//! CNT and SP are open drain, so the level the chip sees is its own drive
//! ANDed with whatever the board drives in [`Cia6526::cnt_in`] and
//! [`Cia6526::sp_in`]. /PC goes low for the one cycle after a port B access.

#![allow(clippy::cast_possible_truncation)]

mod timer;

use serde::{Deserialize, Serialize};

pub use timer::CiaTimer;

/// Which CIA silicon revision to model. The C64 breadbin ships the old
/// 6526; the C64C board carries the 8521/6526A with the faster interrupt
/// path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CiaModel {
    #[default]
    Mos6526,
    Mos6526A,
}

// ICR flag bits (shared between status and mask).
const IM_TA: u16 = 0x01;
const IM_TB: u16 = 0x02;
const IM_TOD: u16 = 0x04;
const IM_SDR: u16 = 0x08;
const IM_FLAG: u16 = 0x10;
/// ICR bit 7 (IR) — mirrored into the flags word like VICE's `CIA_IM_SET`.
const IM_SET: u16 = 0x80;
/// Timer-B-bug marker (VICE `CIA_IM_TBB`): TB underflowed the cycle after an
/// ICR read; the flag is eaten by the in-flight acknowledge.
const IM_TBB: u16 = 0x100;

// ifr_delay stages (VICE ciacore.c). The word shifts LEFT each cycle; a
// `…1` bit becomes the acting `…0` bit on the following cycle.
const IRQ_ACK1: u32 = 0x0001;
const IRQ_ACK0: u32 = 0x0002;
const IRQ_ACK_1: u32 = 0x0004;
const IRQ_ACK_2: u32 = 0x0008;
const IRQ_D7SET1: u32 = 0x0010;
const IRQ_D7SET0: u32 = 0x0020;
const IRQ_D7SET_1: u32 = 0x0040;
const IRQ_RAISE1: u32 = 0x0100;
const IRQ_RAISE0: u32 = 0x0200;
const IRQ_RAISE_1: u32 = 0x0400;
const IRQ_READ0: u32 = 0x1000;
const IRQ_READ1: u32 = 0x2000;
const IRQ_READ2: u32 = 0x4000;
/// Bits that fall off the end of their group after the shift.
const IRQ_CLEAR: u32 = IRQ_ACK_2 | IRQ_D7SET_1 | IRQ_RAISE_1 | IRQ_READ2;

/// "Never" sentinel for the last-ICR-read clock.
const NEVER: u64 = u64::MAX;

/// CRA bit 6: serial port output (CNT sources the shift clock).
const CRA_SPMODE_OUT: u8 = 0x40;

// sdr_delay stages (VICE ciacore.c). Like ifr_delay the word shifts LEFT
// each cycle, so within a group the stage with the higher suffix acts later.
const SDR_TOGGLE_CNT2: u32 = 0x0001;
const SDR_TOGGLE_CNT1: u32 = 0x0002;
const SDR_TOGGLE_CNT0: u32 = 0x0004;
const SDR_TOGGLE_CNT_1: u32 = 0x0008;
const SDR_NOGGLE_CNT2: u32 = 0x0010;
const SDR_NOGGLE_CNT1: u32 = 0x0020;
const SDR_NOGGLE_CNT0: u32 = 0x0040;
const SDR_NOGGLE_CNT_1: u32 = 0x0080;
const SDR_SET_SDR_IRQ2: u32 = 0x0200;
const SDR_SET_SDR_IRQ0: u32 = 0x0800;
/// CNT output history, one bit per cycle (CNT0 newest).
const SDR_CNT0: u32 = 0x1000;
const SDR_CNT1: u32 = 0x2000;
const SDR_CNT2: u32 = 0x4000;
const SDR_SET1: u32 = 0x0004_0000;
const SDR_SET0: u32 = 0x0008_0000;
/// Bits that fall off the end of their group after the shift.
const SDR_CLEAR: u32 = SDR_NOGGLE_CNT2 | 0x0100 | SDR_CNT0 | 0x0001_0000 | 0x0010_0000;
const SDR_ALL_TOGGLE_CNT: u32 =
    SDR_TOGGLE_CNT2 | SDR_TOGGLE_CNT1 | SDR_TOGGLE_CNT0 | SDR_TOGGLE_CNT_1;
const SDR_ALL_NOGGLE_CNT: u32 =
    SDR_NOGGLE_CNT2 | SDR_NOGGLE_CNT1 | SDR_NOGGLE_CNT0 | SDR_NOGGLE_CNT_1;

/// Cycles from a rising CNT edge being seen to the timer's count input
/// (MiSTer `mos6526.v`: `cnt_in_r` → `countA0` → `countA1` → `countA2`).
const CNT_COUNT_STAGE: u8 = 0x08;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cia6526 {
    /// /IRQ pin state (already through the silicon's delay pipeline).
    pub irq: bool,
    pub pa: u8,
    pub pb: u8,
    pub pa_in: u8,
    pub pb_in: u8,
    pub flag: bool,
    /// Level the board drives onto CNT; `true` when released (the pin's
    /// pull-up). The chip sees this ANDed with its own open-drain output.
    pub cnt_in: bool,
    /// Level the board drives onto SP; `true` when released.
    pub sp_in: bool,
    /// /PC output: low for the one cycle after a port B read or write.
    pub pc: bool,
    model: CiaModel,
    /// φ2 cycles ticked; timebase for the read-race bookkeeping.
    clock: u64,
    port_a: u8,
    port_b: u8,
    ddr_a: u8,
    ddr_b: u8,
    timer_a: CiaTimer,
    timer_b: CiaTimer,
    /// PB6 toggle flip-flop (set on Timer A START, flips each underflow).
    ta_toggle: bool,
    /// PB7 toggle flip-flop.
    tb_toggle: bool,
    /// ICR flags: bits 0-4 sources, bit 7 IR, bit 8 the TB-bug marker.
    irqflags: u16,
    /// Flags set this cycle, not yet run through the IFR pipeline.
    new_irqflags: u16,
    /// Flags scheduled to clear when the ACK stage lands.
    ack_irqflags: u16,
    /// The IFR delay line (see module docs).
    ifr_delay: u32,
    /// Clock of the last ICR read (`NEVER` if none) — the `rdi` bookkeeping
    /// behind the timer B bug and the 6526A read race.
    icr_read_clock: u64,
    icr_mask: u8,
    cra: u8,
    crb: u8,
    tod: [u8; 4],
    tod_alarm: [u8; 4],
    tod_latch: [u8; 4],
    tod_latched: bool,
    tod_halted: bool,
    /// Phi2 cycles per 10 Hz tick at 50 Hz input (CRA7 = 1).
    tod_divider_50hz: u32,
    /// Phi2 cycles per 10 Hz tick at 60 Hz input (CRA7 = 0).
    tod_divider_60hz: u32,
    tod_counter: u32,
    /// Mains-tick ring counter for the /5 (50 Hz) or /6 (60 Hz)
    /// pre-divider from the TOD input down to the 10 Hz tenths counter
    /// (CRA bit 7 selects which). `#[serde(default)]` so pre-prescaler
    /// snapshots still load (starts a fresh ring, worst case one tenth
    /// of jitter on the first tick after restore).
    #[serde(default)]
    tod_tick_counter: u8,
    prev_flag: bool,
    /// The Serial Data Register as the CPU sees it: the byte last written
    /// (output) or last received (input).
    sdr: u8,
    /// The shift register itself (VICE `shifter`); the byte being moved
    /// sits in bits 8..1 while shifting out.
    shifter: u16,
    /// CNT half-periods left in the byte (VICE `sr_bits`).
    sr_bits: u32,
    /// A byte written to the SDR is waiting behind the one shifting out.
    sdr_valid: bool,
    /// Switching to input mid-byte still owes the SDR interrupt when output
    /// is selected again (VICE `sdr_force_finish`).
    sdr_force_finish: bool,
    /// The serial-port delay line (see the `SDR_*` stages).
    sdr_delay: u32,
    /// The chip's own CNT drive; `true` when released.
    cnt_out: bool,
    /// The chip's own SP drive, the last bit shifted out.
    sp_out: bool,
    /// Timer A underflowed on the previous tick. The serial-port stage for
    /// a cycle runs after that cycle's register access, so it lands at the
    /// start of the next tick.
    ta_underflowed_last: bool,
    /// CNT level seen on the previous tick, for edge detection.
    cnt_prev: bool,
    /// Rising CNT edges on their way to the timer count inputs, one bit
    /// per cycle (`CNT_COUNT_STAGE` is the one that counts).
    cnt_rises: u8,
    /// A port B access this cycle pulses /PC on the next.
    pc_pending: bool,
}

impl Cia6526 {
    #[must_use]
    pub fn new() -> Self {
        // PAL defaults: 985 248 Hz phi2 → 50 Hz / 60 Hz pre-dividers.
        Self::new_with_tod_dividers(19_705, 16_421)
    }

    #[must_use]
    pub fn new_with_tod(tod_divider: u32) -> Self {
        // Legacy constructor — uses the same divider for both 50 and 60 Hz
        // modes. Prefer new_with_tod_dividers(pal, ntsc).
        Self::new_with_tod_dividers(tod_divider, tod_divider)
    }

    #[must_use]
    pub fn new_with_tod_dividers(tod_divider_50hz: u32, tod_divider_60hz: u32) -> Self {
        let mut cia = Self {
            irq: false,
            pa: 0xFF,
            pb: 0xFF,
            pa_in: 0xFF,
            pb_in: 0xFF,
            flag: true,
            cnt_in: true,
            sp_in: true,
            pc: true,
            model: CiaModel::Mos6526,
            clock: 0,
            port_a: 0xFF,
            port_b: 0xFF,
            ddr_a: 0,
            ddr_b: 0,
            timer_a: CiaTimer::new(),
            timer_b: CiaTimer::new(),
            ta_toggle: false,
            tb_toggle: false,
            irqflags: 0,
            new_irqflags: 0,
            ack_irqflags: 0,
            ifr_delay: 0,
            icr_read_clock: NEVER,
            icr_mask: 0,
            cra: 0,
            crb: 0,
            tod: [0; 4],
            tod_alarm: [0; 4],
            tod_latch: [0; 4],
            tod_latched: false,
            tod_halted: true,
            tod_divider_50hz,
            tod_divider_60hz,
            tod_counter: 0,
            tod_tick_counter: 0,
            prev_flag: true,
            sdr: 0,
            shifter: 0,
            sr_bits: 0,
            sdr_valid: false,
            sdr_force_finish: false,
            sdr_delay: SDR_CNT0 | SDR_CNT1 | SDR_CNT2 | 0x8000,
            cnt_out: true,
            sp_out: true,
            ta_underflowed_last: false,
            cnt_prev: true,
            cnt_rises: 0,
            pc_pending: false,
        };
        cia.update_pins();
        cia
    }

    /// Select the modelled silicon revision (default: old 6526).
    pub fn set_model(&mut self, model: CiaModel) {
        self.model = model;
    }

    #[must_use]
    pub const fn model(&self) -> CiaModel {
        self.model
    }

    pub fn tick(&mut self) {
        self.clock += 1;
        self.poll_flag();
        self.tick_tod();

        // /PC: low for exactly the cycle after a port B access.
        self.pc = !self.pc_pending;
        self.pc_pending = false;

        let cnt = self.cnt_line();
        let cnt_changed = cnt != self.cnt_prev;
        self.cnt_prev = cnt;
        if cnt_changed && self.cra & CRA_SPMODE_OUT == 0 {
            self.shift_in_on_cnt_edge(cnt);
        }
        self.cnt_rises = ((self.cnt_rises << 1) | u8::from(cnt_changed && cnt)) & 0x0F;
        let cnt_count = self.cnt_rises & CNT_COUNT_STAGE != 0;

        let ta_underflowed = self.timer_a.tick();
        let tb_underflowed = self.timer_b.tick();

        // CRA INMODE (bit 5): Timer A counts rising CNT edges. The STEP bit
        // is consumed by the timer's next transition, as for the cascade.
        if self.cra & 0x20 != 0 && cnt_count {
            self.timer_a.single_step();
        }

        if ta_underflowed {
            self.ta_toggle = !self.ta_toggle;
            self.set_irq_flag(IM_TA);
        }

        // CRB INMODE (bits 6:5): φ2, rising CNT edges, Timer A underflows,
        // or Timer A underflows while CNT is high.
        let tb_step = match self.crb & 0x60 {
            0x20 => cnt_count,
            0x40 => ta_underflowed,
            0x60 => ta_underflowed && cnt,
            _ => false,
        };
        if tb_step {
            self.timer_b.single_step();
        }

        if tb_underflowed {
            self.tb_toggle = !self.tb_toggle;
            self.set_irq_flag(IM_TB);
            // Timer B bug (old 6526): an ICR read in the previous cycle has
            // an acknowledge in flight that eats this flag (VICE
            // cia_do_update_tb).
            if self.model == CiaModel::Mos6526 && self.icr_read_clock == self.clock.wrapping_sub(1)
            {
                self.irqflags |= IM_TBB;
            } else {
                self.irqflags &= !IM_TBB;
            }
        }

        self.run_sdr_cycle();
        self.ta_underflowed_last = ta_underflowed;

        self.run_ifr_cycle();
        self.update_pins();
    }

    /// One cycle of the serial-port output pipeline (VICE `ciacore_intta`'s
    /// shift-register branch followed by `ciacore_intsdr`), for the cycle
    /// before this tick: VICE runs both as alarms after that cycle's
    /// register access, so a write to SDR or CRA in that cycle is already
    /// visible here.
    fn run_sdr_cycle(&mut self) {
        // ~1.5 cycles from a Timer A underflow to the CNT toggle. Two
        // underflows too close together cannot both be seen.
        if self.ta_underflowed_last
            && self.cra & CRA_SPMODE_OUT != 0
            && (self.sr_bits != 0 || self.sdr_valid)
        {
            self.sdr_delay |= if self.sdr_delay & (SDR_TOGGLE_CNT0 | SDR_NOGGLE_CNT0) != 0 {
                SDR_NOGGLE_CNT1
            } else {
                SDR_TOGGLE_CNT1
            };
        }

        let mut feed = 0;

        // A byte written to SDR reaches the shift register two cycles on.
        if self.sdr_delay & SDR_SET0 != 0 {
            match self.sr_bits {
                0 => {
                    self.sr_bits = 16;
                    self.shifter = u16::from(self.sdr) << 1;
                }
                1 => {
                    self.shifter |= u16::from(self.sdr);
                    self.sr_bits = 17;
                }
                _ => self.sdr_valid = true,
            }
        }

        if self.sdr_delay & SDR_TOGGLE_CNT0 != 0 {
            let falling = if self.sr_bits == 0 {
                false
            } else {
                self.sr_bits -= 1;
                self.sr_bits & 1 != 0
            };
            if falling {
                // SP takes the next bit (the one just left of the byte) as
                // CNT falls; it stays valid until the next falling edge.
                self.sp_out = (self.shifter >> 8) & 1 != 0;
                self.cnt_out = false;
                if self.sr_bits == 1 {
                    // Eighth bit out: the interrupt follows two cycles on.
                    feed |= SDR_SET_SDR_IRQ2;
                    if self.sdr_valid {
                        self.shifter |= u16::from(self.sdr);
                        self.sdr_valid = false;
                        self.sr_bits = 17;
                    }
                }
            } else {
                self.shifter <<= 1;
                self.cnt_out = true;
            }
        }

        if self.sdr_delay & SDR_SET_SDR_IRQ0 != 0 {
            self.set_irq_flag(IM_SDR);
        }

        self.sdr_delay |= feed;
        self.sdr_delay <<= 1;
        self.sdr_delay &= !SDR_CLEAR;
        if self.cnt_out {
            self.sdr_delay |= SDR_CNT0;
        }
    }

    /// An edge on CNT while the serial port is an input (VICE
    /// `ciacore_set_cnt`): a falling edge opens a byte, each rising edge
    /// shifts SP in, and the eighth loads the SDR and raises its interrupt.
    ///
    /// VICE decrements its bit counter unconditionally, so a rising edge
    /// outside a byte (CNT low across a switch to input) wraps it and no
    /// byte ever completes. Here that edge still shifts its bit in but does
    /// not count, so the next falling edge starts a clean byte.
    fn shift_in_on_cnt_edge(&mut self, rising: bool) {
        if !rising && self.sr_bits == 0 {
            self.sr_bits = 16;
        }
        if rising {
            self.shifter = (self.shifter << 1) | u16::from(self.sp_line());
        }
        if self.sr_bits == 0 {
            return;
        }
        self.sr_bits -= 1;
        if rising && self.sr_bits == 0 {
            self.sdr = self.shifter as u8;
            self.set_irq_flag(IM_SDR);
        }
    }

    /// CRA write that flips the serial-port direction (VICE
    /// `strange_extra_sdr_flags` plus the reset around it).
    fn switch_sp_direction(&mut self, cra: u8) {
        if cra & CRA_SPMODE_OUT == 0 {
            // To input: a byte caught mid-shift still owes its interrupt.
            let cnt_wanted = match self.model {
                CiaModel::Mos6526 => SDR_CNT0 | SDR_CNT1 | SDR_CNT2,
                CiaModel::Mos6526A => SDR_CNT1 | SDR_CNT2,
            };
            let mut force = self.sdr_delay & cnt_wanted != cnt_wanted;
            if !force
                && self.sr_bits != 2
                && self.sdr_delay & (SDR_TOGGLE_CNT2 | SDR_TOGGLE_CNT1) == 0
                && self.sdr_delay & SDR_TOGGLE_CNT0 != 0
            {
                force = true;
            }
            self.sdr_force_finish = force;
        } else {
            if !self.cnt_out && self.sr_bits != 0 {
                self.shifter <<= 1;
            }
            if self.sdr_force_finish {
                self.sdr_delay |= SDR_SET_SDR_IRQ2;
                self.sdr_force_finish = false;
            }
        }
        self.sr_bits = 0;
        self.sdr_valid = false;
        self.sdr_delay &= !(SDR_ALL_TOGGLE_CNT | SDR_ALL_NOGGLE_CNT);
        self.cnt_out = true;
    }

    /// The level on the CNT pin: the chip's open-drain drive ANDed with the
    /// board's.
    #[must_use]
    pub const fn cnt_line(&self) -> bool {
        self.cnt_in && self.cnt_out
    }

    /// The level on the SP pin. The chip drives SP only as a serial output.
    #[must_use]
    pub const fn sp_line(&self) -> bool {
        self.sp_in && (self.cra & CRA_SPMODE_OUT == 0 || self.sp_out)
    }

    /// One cycle of the IFR delay pipeline (VICE `cia_run_ifr_cycle`).
    fn run_ifr_cycle(&mut self) {
        let mut delay = self.ifr_delay;

        if delay & IRQ_ACK0 != 0 {
            self.irqflags &= !self.ack_irqflags;
            if self.model == CiaModel::Mos6526 {
                self.irqflags &= !IM_SET;
            }
            self.ack_irqflags = 0;
        }

        if self.new_irqflags & u16::from(self.icr_mask) & 0x1F != 0 {
            match self.model {
                CiaModel::Mos6526 => {
                    // Old CIA: IR + the line follow one cycle behind the flag.
                    delay |= IRQ_RAISE1 | IRQ_D7SET1;
                }
                CiaModel::Mos6526A => {
                    if self.icr_read_clock.wrapping_add(1) == self.clock {
                        // Read race: ICR read in the previous cycle delays
                        // the raise by a cycle (VICE / TLR's cia-int tests).
                        delay |= IRQ_RAISE1 | IRQ_D7SET1;
                    } else {
                        delay |= IRQ_RAISE0 | IRQ_D7SET0;
                    }
                }
            }
        }

        if delay & IRQ_D7SET0 != 0 {
            self.irqflags |= IM_SET;
        }
        if delay & IRQ_RAISE0 != 0 {
            self.irq = true;
        }

        self.new_irqflags = 0;
        self.ifr_delay = (delay << 1) & !IRQ_CLEAR;
    }

    /// Flag an interrupt source (VICE `cia_set_irq_flag`): the ICR bit is
    /// visible immediately; IR/line follow through the pipeline.
    fn set_irq_flag(&mut self, bits: u16) {
        self.irqflags |= bits;
        self.new_irqflags |= bits;
        self.ack_irqflags &= !bits;
    }

    pub fn read(&mut self, reg: u8) -> u8 {
        match reg & 0x0F {
            0x00 => (self.port_a & self.ddr_a) | (self.pa_in & !self.ddr_a),
            0x01 => {
                self.pc_pending = true;
                let byte = (self.port_b & self.ddr_b) | (self.pb_in & !self.ddr_b);
                self.pb67_override(byte)
            }
            0x02 => self.ddr_a,
            0x03 => self.ddr_b,
            0x04 => self.timer_a.counter() as u8,
            0x05 => (self.timer_a.counter() >> 8) as u8,
            0x06 => self.timer_b.counter() as u8,
            0x07 => (self.timer_b.counter() >> 8) as u8,
            0x08 => {
                let value = if self.tod_latched {
                    self.tod_latch[0]
                } else {
                    self.tod[0]
                };
                self.tod_latched = false;
                value
            }
            0x09 => {
                if self.tod_latched {
                    self.tod_latch[1]
                } else {
                    self.tod[1]
                }
            }
            0x0A => {
                if self.tod_latched {
                    self.tod_latch[2]
                } else {
                    self.tod[2]
                }
            }
            0x0B => {
                if !self.tod_latched {
                    self.tod_latch = self.tod;
                    self.tod_latched = true;
                }
                self.tod_latch[3]
            }
            0x0C => self.sdr,
            0x0D => self.read_icr(),
            0x0E => (self.cra & !0x01) | u8::from(self.timer_a.is_running()),
            0x0F => (self.crb & !0x01) | u8::from(self.timer_b.is_running()),
            _ => 0xFF,
        }
    }

    /// ICR read (VICE `ciacore_read` case `CIA_ICR`): returns the flags plus
    /// IR, drops the line at once, and schedules the flag clear through the
    /// ACK stage. On the old 6526 the in-flight TB-bug flag is eaten before
    /// it is ever visible.
    fn read_icr(&mut self) -> u8 {
        if self.irqflags & IM_TBB != 0 {
            // Timer B bug: the flag set last cycle is consumed unseen.
            self.irqflags &= !(IM_TBB | IM_TB);
        }

        let result;
        match self.model {
            CiaModel::Mos6526 => {
                self.ifr_delay |= IRQ_ACK1;
                self.ifr_delay &= !IRQ_RAISE0;
                result = (self.irqflags & 0xFF) as u8;
                self.irqflags &= IM_SET;
                self.new_irqflags = 0;
                // ack_irqflags is effectively unused on old CIAs.
            }
            CiaModel::Mos6526A => {
                if self.ifr_delay & IRQ_RAISE0 != 0 && self.irqflags & 0x1F != 0 {
                    self.irqflags |= IM_SET;
                }
                if self.irqflags & 0x9F != 0 {
                    self.ack_irqflags |= (self.irqflags & 0x9F) | IM_SET;
                }
                self.ifr_delay |= IRQ_ACK1;
                self.ifr_delay &= !(IRQ_RAISE0 | IRQ_D7SET0);
                result = (self.irqflags & 0xFF) as u8;
            }
        }

        self.ifr_delay |= IRQ_READ0;
        self.irq = false;
        self.icr_read_clock = self.clock;
        result
    }

    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 0x0F {
            0x00 => self.port_a = value,
            0x01 => {
                self.port_b = value;
                self.pc_pending = true;
            }
            0x02 => self.ddr_a = value,
            0x03 => self.ddr_b = value,
            0x04 => self.timer_a.set_latch_lo(value),
            0x05 => self.timer_a.set_latch_hi(value),
            0x06 => self.timer_b.set_latch_lo(value),
            0x07 => self.timer_b.set_latch_hi(value),
            // TOD / alarm writes — CRB bit 7 selects alarm vs clock.
            // Clock writes: $B halts, $8 restarts. Alarm writes never
            // affect the TOD halt state (datasheet confirmed).
            0x08 => {
                if self.crb & 0x80 != 0 {
                    self.tod_alarm[0] = value & 0x0F;
                } else {
                    self.tod[0] = value & 0x0F;
                    self.tod_halted = false;
                    self.tod_counter = 0;
                    self.tod_tick_counter = 0;
                }
            }
            0x09 => {
                if self.crb & 0x80 != 0 {
                    self.tod_alarm[1] = value & 0x7F;
                } else {
                    self.tod[1] = value & 0x7F;
                }
            }
            0x0A => {
                if self.crb & 0x80 != 0 {
                    self.tod_alarm[2] = value & 0x7F;
                } else {
                    self.tod[2] = value & 0x7F;
                }
            }
            0x0B => {
                if self.crb & 0x80 != 0 {
                    self.tod_alarm[3] = value & 0x9F;
                } else {
                    self.tod[3] = value & 0x9F;
                    self.tod_halted = true;
                }
            }
            0x0C => {
                // In output mode the byte reaches the shift register two
                // cycles later (VICE schedules `SDR_SET1`).
                if self.cra & CRA_SPMODE_OUT != 0 {
                    self.sdr_delay |= SDR_SET1;
                }
                self.sdr = value;
            }
            0x0D => self.write_icr_mask(value),
            0x0E => {
                // Rising START edge presets the PB6 toggle high.
                if value & 0x01 != 0 && self.cra & 0x01 == 0 {
                    self.ta_toggle = true;
                }
                if (value ^ self.cra) & CRA_SPMODE_OUT != 0 {
                    self.switch_sp_direction(value);
                }
                self.timer_a.set_ctrl(value);
                self.cra = value & 0xEF; // force-load strobe is not stored
            }
            0x0F => {
                if value & 0x01 != 0 && self.crb & 0x01 == 0 {
                    self.tb_toggle = true;
                }
                // CRB has two INMODE bits; the count-TA modes (bit 6) map to
                // the timer's single-step input, so feed the state machine
                // "not φ2" (VICE ORs 0x20 in for these modes).
                if value & 0x40 != 0 {
                    self.timer_b.set_ctrl(value | 0x20);
                } else {
                    self.timer_b.set_ctrl(value);
                }
                self.crb = value & 0xEF;
            }
            _ => {}
        }
        self.update_pins();
    }

    /// ICR mask write (VICE `ciacore_store_internal` case `CIA_ICR`):
    /// enabling a mask bit for an already-pending flag raises the interrupt
    /// through the delayed pipeline.
    fn write_icr_mask(&mut self, value: u8) {
        if value & 0x80 != 0 {
            self.icr_mask |= value & 0x7F;
        } else {
            self.icr_mask &= !(value & 0x7F);
        }

        if self.irqflags & u16::from(self.icr_mask) & 0x7F != 0 {
            if !self.irq {
                match self.model {
                    CiaModel::Mos6526 => {
                        self.ifr_delay |= IRQ_RAISE1 | IRQ_D7SET1;
                    }
                    CiaModel::Mos6526A => {
                        if self.ifr_delay & IRQ_READ1 == 0 {
                            self.ifr_delay |= IRQ_RAISE0 | IRQ_D7SET0;
                        }
                    }
                }
            }
        } else if self.model == CiaModel::Mos6526 && self.ifr_delay & IRQ_ACK_1 != 0 {
            // Disabling the source while a raise is in flight cancels it
            // (needs an ICR read two cycles ago — VICE NOTE_1).
            self.ifr_delay &= !(IRQ_RAISE0 | IRQ_D7SET0);
        }
    }

    /// PB6/PB7 timer outputs override the port lines when CRA/CRB bit 1 is
    /// set: toggle mode (bit 2) outputs the flip-flop, pulse mode outputs
    /// the underflow cycle (VICE `ciacore_update_pb67`).
    ///
    /// Public so board code that models port B externally (e.g. the C64's
    /// keyboard-matrix wired-AND) can apply the timer outputs on top.
    #[must_use]
    pub fn timer_port_b_override(&self, byte: u8) -> u8 {
        self.pb67_override(byte)
    }

    fn pb67_override(&self, mut byte: u8) -> u8 {
        if self.cra & 0x02 != 0 {
            byte &= 0xBF;
            let high = if self.cra & 0x04 != 0 {
                self.ta_toggle
            } else {
                self.timer_a.is_underflow_cycle()
            };
            if high {
                byte |= 0x40;
            }
        }
        if self.crb & 0x02 != 0 {
            byte &= 0x7F;
            let high = if self.crb & 0x04 != 0 {
                self.tb_toggle
            } else {
                self.timer_b.is_underflow_cycle()
            };
            if high {
                byte |= 0x80;
            }
        }
        byte
    }

    #[must_use]
    pub fn irq_active(&self) -> bool {
        self.irq
    }

    #[must_use]
    pub fn timer_a(&self) -> u16 {
        self.timer_a.counter()
    }

    #[must_use]
    pub fn timer_a_latch(&self) -> u16 {
        self.timer_a.latch()
    }

    #[must_use]
    pub fn timer_b(&self) -> u16 {
        self.timer_b.counter()
    }

    #[must_use]
    pub fn timer_b_latch(&self) -> u16 {
        self.timer_b.latch()
    }

    /// ICR status as the CPU would read it (without the read's side
    /// effects): source flags plus IR.
    #[must_use]
    pub fn icr_status(&self) -> u8 {
        (self.irqflags & 0xFF) as u8
    }

    #[must_use]
    pub fn icr_mask(&self) -> u8 {
        self.icr_mask
    }

    #[must_use]
    pub fn cra(&self) -> u8 {
        (self.cra & !0x01) | u8::from(self.timer_a.is_running())
    }

    #[must_use]
    pub fn crb(&self) -> u8 {
        (self.crb & !0x01) | u8::from(self.timer_b.is_running())
    }

    #[must_use]
    pub fn port_a_latch(&self) -> u8 {
        self.port_a
    }

    #[must_use]
    pub fn port_b_latch(&self) -> u8 {
        self.port_b
    }

    #[must_use]
    pub fn ddr_a(&self) -> u8 {
        self.ddr_a
    }

    #[must_use]
    pub fn ddr_b(&self) -> u8 {
        self.ddr_b
    }

    #[must_use]
    pub fn port_a_drive_state(&self) -> u8 {
        (self.port_a & self.ddr_a) | !self.ddr_a
    }

    #[must_use]
    pub fn port_b_drive_state(&self) -> u8 {
        (self.port_b & self.ddr_b) | !self.ddr_b
    }

    fn update_pins(&mut self) {
        self.pa = (self.port_a & self.ddr_a) | (self.pa_in & !self.ddr_a);
        let pb = (self.port_b & self.ddr_b) | (self.pb_in & !self.ddr_b);
        self.pb = self.pb67_override(pb);
        // The /IRQ pin is owned by the IFR pipeline (run_ifr_cycle /
        // read_icr), not recomputed from the flags.
    }

    fn poll_flag(&mut self) {
        if self.prev_flag && !self.flag {
            self.set_irq_flag(IM_FLAG);
        }
        self.prev_flag = self.flag;
    }

    fn tick_tod(&mut self) {
        if self.tod_halted {
            return;
        }
        // CRA bit 7 (TODIN): 1 = 50 Hz input, 0 = 60 Hz input. Select
        // the matching pre-divider each tick so runtime flips take
        // effect as soon as software writes CRA.
        let divider = if self.cra & 0x80 != 0 {
            self.tod_divider_50hz
        } else {
            self.tod_divider_60hz
        };
        self.tod_counter += 1;
        if self.tod_counter < divider {
            return;
        }
        self.tod_counter = 0;

        // CRA bit 7 (TODIN) selects the CIA's internal /5 (50 Hz) or
        // /6 (60 Hz) pre-divider from the mains input down to the 10 Hz
        // tenths counter. Advance the clock only once every 5 or 6 mains
        // ticks — without this stage the TOD ran at the 50/60 Hz mains
        // rate, i.e. 5-6x too fast.
        let prescale: u8 = if self.cra & 0x80 != 0 { 5 } else { 6 };
        self.tod_tick_counter += 1;
        if self.tod_tick_counter < prescale {
            return;
        }
        self.tod_tick_counter = 0;

        self.tod[0] = (self.tod[0] + 1) & 0x0F;
        if self.tod[0] < 10 {
            self.check_alarm();
            return;
        }
        self.tod[0] = 0;

        self.tod[1] = bcd_increment(self.tod[1]);
        if self.tod[1] < 0x60 {
            self.check_alarm();
            return;
        }
        self.tod[1] = 0;

        self.tod[2] = bcd_increment(self.tod[2]);
        if self.tod[2] < 0x60 {
            self.check_alarm();
            return;
        }
        self.tod[2] = 0;

        let pm = self.tod[3] & 0x80;
        let hours = self.tod[3] & 0x1F;
        let next = bcd_increment(hours);
        if next == 0x12 {
            self.tod[3] = 0x12 | (pm ^ 0x80);
        } else if next == 0x13 {
            self.tod[3] = 0x01 | pm;
        } else {
            self.tod[3] = next | pm;
        }

        self.check_alarm();
    }

    /// Alarm match: when all four TOD bytes equal the alarm, raise
    /// ICR bit 2 (ALARM). Checked after any TOD increment.
    fn check_alarm(&mut self) {
        if self.tod == self.tod_alarm {
            self.set_irq_flag(IM_TOD);
        }
    }
}

impl Default for Cia6526 {
    fn default() -> Self {
        Self::new()
    }
}

fn bcd_increment(value: u8) -> u8 {
    let lo = value & 0x0F;
    let hi = value & 0xF0;
    if lo < 9 {
        hi | (lo + 1)
    } else if hi < 0x90 {
        (hi + 0x10) & 0xF0
    } else {
        0x00
    }
}

#[cfg(test)]
mod tests;
