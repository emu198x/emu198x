//! The Yamaha YM2612 (OPN2) FM synthesis chip and its CMOS twin, the YM3438.
//!
//! Six channels of four-operator FM, a DAC that can stand in for channel 6,
//! a global LFO, two timers, CSM key-on and channel 3's per-operator
//! frequencies. The Sega Mega Drive's sound chip.
//!
//! # How it is modelled
//!
//! The chip divides its input clock by six. Each internal clock works on one
//! of 24 operator slots, and the slots run as a pipeline: on any one clock
//! the phase generator, the envelope generator, the operator and the channel
//! accumulator are each working on a different slot. A sample takes 24
//! internal clocks, 144 input clocks in all. This crate steps that pipeline
//! one internal clock at a time, so register writes land on the slot the
//! silicon would land them on and the output pins change when the silicon's
//! do.
//!
//! The pipeline, its stage offsets and its arithmetic follow Nuked-OPN2
//! 1.0.12 by Alexey Khokholov (Nuke.YKT), commit `335747d7`, which was
//! reverse-engineered from the YM3438 die shot by digshadow (Silicon Pr0n).
//! This crate is a Rust re-expression of that model, not a binding to it:
//! Nuked-OPN2 is LGPL-2.1-or-later, which permits that under this crate's
//! GPL-2.0-or-later licence. ymfm by Aaron Giles is the second reference;
//! the two agree on everything this crate depends on. The crate's tests
//! compare its output pins with Nuked-OPN2's, clock by clock.
//!
//! # Using it
//!
//! Drive [`Ym2612::tick`] from the chip's input clock (the Mega Drive's
//! master clock divided by 7). Write with [`Ym2612::write`] and read the
//! status with [`Ym2612::read`]. After each tick the `mol` and `mor` fields
//! hold the analogue output pins for that internal clock, and
//! [`Ym2612::take_samples`] returns one stereo sample per 24 internal clocks,
//! the sum of the pins across them.
//!
//! The chip exposes its state as public fields and is stepped by the
//! machine, so it owns no bus and makes no callbacks, the same as every
//! Emu198x chip.

mod tables;

use serde::{Deserialize, Serialize};

use tables::{
    CH_OFFSET, EG_AM_SHIFT, EG_STEP_HI, EXP, FM_ALGORITHM, FN_NOTE, LFO_CYCLES, LOGSIN, OP_OFFSET,
    PG_DETUNE, PG_LFO_SH1, PG_LFO_SH2,
};

/// Input clocks per internal clock.
pub const PRESCALER: u8 = 6;

/// Internal clocks per output sample: one per operator slot.
pub const SLOTS: usize = 24;

/// How many internal clocks the status byte stays on the data bus after a
/// read on the NMOS YM2612. After that the bus has decayed to zero.
const STATUS_HOLD_YM2612: u32 = 300_000;
/// The same for the CMOS YM3438, whose bus holds its charge far longer.
const STATUS_HOLD_YM3438: u32 = 40_000_000;

const EG_ATTACK: u8 = 0;
const EG_DECAY: u8 = 1;
const EG_SUSTAIN: u8 = 2;
const EG_RELEASE: u8 = 3;

/// Which chip: they share every register and the whole synthesis pipeline,
/// and differ only in how the output reaches the pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Variant {
    /// The NMOS original. Its 9-bit DAC outputs each channel in turn and
    /// pulls the pins to the sign level between channels, which gives the
    /// "ladder effect": a step near zero that makes quiet sounds louder and
    /// grittier.
    Ym2612,
    /// The CMOS reissue (YM3438, sold as the OPN2C), used by later Mega
    /// Drive boards. Its output carries no ladder step.
    Ym3438,
}

/// Sign-extend the value whose sign bit is `bit`.
fn sign_extend(bit: u32, value: i32) -> i32 {
    (value & ((1 << bit) - 1)) - (value & (1 << bit))
}

/// One YM2612 or YM3438.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ym2612 {
    pub variant: Variant,
    /// Whether the status can be read at every port address, rather than
    /// only at port 0. True on the YM3438, on Mega Drive boards with the
    /// chip inside an ASIC, and on the TeraDrive; false on the first boards
    /// with a discrete YM2612.
    pub status_on_all_ports: bool,
    /// Input clocks since the last internal clock.
    pub prescaler: u8,
    /// The slot the pipeline is on, 0-23.
    pub cycles: u32,
    /// The channel the pipeline is on: `cycles % 6`.
    pub channel: u32,
    /// The MOL output pin for this internal clock.
    pub mol: i16,
    /// The MOR output pin for this internal clock.
    pub mor: i16,

    // Bus interface.
    write_data: u16,
    write_a: u8,
    write_d: u8,
    write_a_en: bool,
    write_d_en: bool,
    write_busy: bool,
    write_busy_cnt: u8,
    write_fm_address: bool,
    write_fm_data: bool,
    write_fm_mode_a: u16,
    address: u16,
    data: u8,
    pin_test_in: u8,
    busy: bool,

    // LFO.
    lfo_en: u8,
    lfo_freq: u8,
    lfo_pm: u8,
    lfo_am: u8,
    lfo_cnt: u8,
    lfo_inc: u8,
    lfo_quotient: u8,

    // Phase generator.
    pg_fnum: u16,
    pg_block: u8,
    pg_kcode: u8,
    pg_inc: [u32; SLOTS],
    pg_phase: [u32; SLOTS],
    pg_reset: [bool; SLOTS],
    pg_read: u32,

    // Envelope generator.
    eg_cycle: u8,
    eg_cycle_stop: u8,
    eg_shift: u8,
    eg_shift_lock: u8,
    eg_timer_low_lock: u8,
    eg_timer: u16,
    eg_timer_inc: u16,
    eg_quotient: u16,
    eg_custom_timer: u8,
    eg_rate: u8,
    eg_ksv: u8,
    eg_inc: u8,
    eg_ratemax: bool,
    eg_sl: [u8; 2],
    eg_lfo_am: u8,
    eg_tl: [u8; 2],
    eg_state: [u8; SLOTS],
    eg_level: [u16; SLOTS],
    eg_out: [u16; SLOTS],
    eg_kon: [bool; SLOTS],
    eg_kon_csm: [bool; SLOTS],
    eg_kon_latch: [bool; SLOTS],
    eg_ssg_enable: [bool; SLOTS],
    eg_ssg_pgrst_latch: [bool; SLOTS],
    eg_ssg_repeat_latch: [bool; SLOTS],
    eg_ssg_hold_up_latch: [bool; SLOTS],
    eg_ssg_dir: [bool; SLOTS],
    eg_ssg_inv: [bool; SLOTS],
    eg_read: [u32; 2],
    eg_read_inc: u32,

    // Operators.
    fm_op1: [[i16; 2]; 6],
    fm_op2: [i16; 6],
    fm_out: [i16; SLOTS],
    fm_mod: [u16; SLOTS],

    // Channel accumulators and output.
    ch_acc: [i16; 6],
    ch_out: [i16; 6],
    ch_lock: i16,
    ch_lock_l: bool,
    ch_lock_r: bool,
    ch_read: i16,

    // Timers.
    timer_a_cnt: u16,
    timer_a_reg: u16,
    timer_a_load_lock: u8,
    timer_a_load: u8,
    timer_a_enable: u8,
    timer_a_reset: u8,
    timer_a_load_latch: u8,
    timer_a_overflow_flag: u8,
    timer_a_overflow: u8,
    timer_b_cnt: u16,
    timer_b_subcnt: u8,
    timer_b_reg: u16,
    timer_b_load_lock: u8,
    timer_b_load: u8,
    timer_b_enable: u8,
    timer_b_reset: u8,
    timer_b_load_latch: u8,
    timer_b_overflow_flag: u8,
    timer_b_overflow: u8,

    // Mode registers.
    mode_test_21: [u8; 8],
    mode_test_2c: [u8; 8],
    mode_ch3: u8,
    mode_kon_channel: u32,
    mode_kon_operator: [bool; 4],
    mode_kon: [bool; SLOTS],
    mode_csm: bool,
    mode_kon_csm: bool,
    dacen: bool,
    dacdata: i16,

    // Operator registers, by slot.
    ks: [u8; SLOTS],
    ar: [u8; SLOTS],
    sr: [u8; SLOTS],
    dt: [u8; SLOTS],
    multi: [u8; SLOTS],
    sl: [u8; SLOTS],
    rr: [u8; SLOTS],
    dr: [u8; SLOTS],
    am: [bool; SLOTS],
    tl: [u8; SLOTS],
    ssg_eg: [u8; SLOTS],

    // Channel registers.
    fnum: [u16; 6],
    block: [u8; 6],
    kcode: [u8; 6],
    fnum_3ch: [u16; 6],
    block_3ch: [u8; 6],
    kcode_3ch: [u8; 6],
    reg_a4: u8,
    reg_ac: u8,
    connect: [u8; 6],
    fb: [u8; 6],
    pan_l: [bool; 6],
    pan_r: [bool; 6],
    ams: [u8; 6],
    pms: [u8; 6],

    // The data bus as last driven by a status read, and how many internal
    // clocks it has left before it decays.
    status: u8,
    status_time: u32,

    // Output summed across the 24 clocks of the current sample.
    sample_l: i32,
    sample_r: i32,
    #[serde(skip)]
    samples: Vec<[i32; 2]>,
}

impl Ym2612 {
    /// A chip just out of reset.
    #[must_use]
    pub fn new(variant: Variant) -> Self {
        Self {
            variant,
            status_on_all_ports: variant == Variant::Ym3438,
            prescaler: 0,
            cycles: 0,
            channel: 0,
            mol: 0,
            mor: 0,
            write_data: 0,
            write_a: 0,
            write_d: 0,
            write_a_en: false,
            write_d_en: false,
            write_busy: false,
            write_busy_cnt: 0,
            write_fm_address: false,
            write_fm_data: false,
            write_fm_mode_a: 0,
            address: 0,
            data: 0,
            pin_test_in: 0,
            busy: false,
            lfo_en: 0,
            lfo_freq: 0,
            lfo_pm: 0,
            lfo_am: 0,
            lfo_cnt: 0,
            lfo_inc: 0,
            lfo_quotient: 0,
            pg_fnum: 0,
            pg_block: 0,
            pg_kcode: 0,
            pg_inc: [0; SLOTS],
            pg_phase: [0; SLOTS],
            pg_reset: [false; SLOTS],
            pg_read: 0,
            eg_cycle: 0,
            eg_cycle_stop: 0,
            eg_shift: 0,
            eg_shift_lock: 0,
            eg_timer_low_lock: 0,
            eg_timer: 0,
            eg_timer_inc: 0,
            eg_quotient: 0,
            eg_custom_timer: 0,
            eg_rate: 0,
            eg_ksv: 0,
            eg_inc: 0,
            eg_ratemax: false,
            eg_sl: [0; 2],
            eg_lfo_am: 0,
            eg_tl: [0; 2],
            eg_state: [EG_RELEASE; SLOTS],
            eg_level: [0x3ff; SLOTS],
            eg_out: [0x3ff; SLOTS],
            eg_kon: [false; SLOTS],
            eg_kon_csm: [false; SLOTS],
            eg_kon_latch: [false; SLOTS],
            eg_ssg_enable: [false; SLOTS],
            eg_ssg_pgrst_latch: [false; SLOTS],
            eg_ssg_repeat_latch: [false; SLOTS],
            eg_ssg_hold_up_latch: [false; SLOTS],
            eg_ssg_dir: [false; SLOTS],
            eg_ssg_inv: [false; SLOTS],
            eg_read: [0; 2],
            eg_read_inc: 0,
            fm_op1: [[0; 2]; 6],
            fm_op2: [0; 6],
            fm_out: [0; SLOTS],
            fm_mod: [0; SLOTS],
            ch_acc: [0; 6],
            ch_out: [0; 6],
            ch_lock: 0,
            ch_lock_l: false,
            ch_lock_r: false,
            ch_read: 0,
            timer_a_cnt: 0,
            timer_a_reg: 0,
            timer_a_load_lock: 0,
            timer_a_load: 0,
            timer_a_enable: 0,
            timer_a_reset: 0,
            timer_a_load_latch: 0,
            timer_a_overflow_flag: 0,
            timer_a_overflow: 0,
            timer_b_cnt: 0,
            timer_b_subcnt: 0,
            timer_b_reg: 0,
            timer_b_load_lock: 0,
            timer_b_load: 0,
            timer_b_enable: 0,
            timer_b_reset: 0,
            timer_b_load_latch: 0,
            timer_b_overflow_flag: 0,
            timer_b_overflow: 0,
            mode_test_21: [0; 8],
            mode_test_2c: [0; 8],
            mode_ch3: 0,
            mode_kon_channel: 0,
            mode_kon_operator: [false; 4],
            mode_kon: [false; SLOTS],
            mode_csm: false,
            mode_kon_csm: false,
            dacen: false,
            dacdata: 0,
            ks: [0; SLOTS],
            ar: [0; SLOTS],
            sr: [0; SLOTS],
            dt: [0; SLOTS],
            multi: [1; SLOTS],
            sl: [0; SLOTS],
            rr: [0; SLOTS],
            dr: [0; SLOTS],
            am: [false; SLOTS],
            tl: [0; SLOTS],
            ssg_eg: [0; SLOTS],
            fnum: [0; 6],
            block: [0; 6],
            kcode: [0; 6],
            fnum_3ch: [0; 6],
            block_3ch: [0; 6],
            kcode_3ch: [0; 6],
            reg_a4: 0,
            reg_ac: 0,
            connect: [0; 6],
            fb: [0; 6],
            pan_l: [true; 6],
            pan_r: [true; 6],
            ams: [0; 6],
            pms: [0; 6],
            status: 0,
            status_time: 0,
            sample_l: 0,
            sample_r: 0,
            samples: Vec::new(),
        }
    }

    /// Pull the reset line: everything returns to its power-on state except
    /// the variant and how the status is read.
    pub fn reset(&mut self) {
        let status_on_all_ports = self.status_on_all_ports;
        *self = Self::new(self.variant);
        self.status_on_all_ports = status_on_all_ports;
    }

    /// One input clock. Every sixth runs an internal clock.
    pub fn tick(&mut self) {
        self.prescaler += 1;
        if self.prescaler == PRESCALER {
            self.prescaler = 0;
            self.clock();
        }
    }

    /// Write `data` to port `port` (A1 and A0). Ports 0 and 2 take a
    /// register address for part I and part II; ports 1 and 3 take the data.
    /// The chip samples the write on its next internal clock.
    pub fn write(&mut self, port: u8, data: u8) {
        let port = u16::from(port & 3);
        self.write_data = ((port << 7) & 0x100) | u16::from(data);
        if port & 1 != 0 {
            self.write_d |= 1;
        } else {
            self.write_a |= 1;
        }
    }

    /// Read port `port`: the status byte (busy in bit 7, the timer B and A
    /// overflow flags in bits 1 and 0), or with the test register's read
    /// mode set, internal data. A port that does not drive the status
    /// returns whatever the bus still holds from the last status read, which
    /// decays to zero.
    pub fn read(&mut self, port: u8) -> u8 {
        if port & 3 == 0 || self.status_on_all_ports {
            if self.mode_test_21[6] != 0 {
                let slot = (self.cycles as usize + 18) % SLOTS;
                let mut testdata: u16 = (((self.pg_read & 1) << 15)
                    | ((self.eg_read[usize::from(self.mode_test_21[0])] & 1) << 14))
                    as u16;
                if self.mode_test_2c[4] != 0 {
                    testdata |= (self.ch_read as u16) & 0x1ff;
                } else {
                    testdata |= (self.fm_out[slot] as u16) & 0x3fff;
                }
                self.status = if self.mode_test_21[7] != 0 {
                    (testdata & 0xff) as u8
                } else {
                    (testdata >> 8) as u8
                };
            } else {
                self.status = (u8::from(self.busy) << 7)
                    | (self.timer_b_overflow_flag << 1)
                    | self.timer_a_overflow_flag;
            }
            self.status_time = match self.variant {
                Variant::Ym2612 => STATUS_HOLD_YM2612,
                Variant::Ym3438 => STATUS_HOLD_YM3438,
            };
        }
        if self.status_time != 0 {
            self.status
        } else {
            0
        }
    }

    /// The IRQ output: a timer has overflowed with its flag enabled.
    #[must_use]
    pub fn irq(&self) -> bool {
        self.timer_a_overflow_flag | self.timer_b_overflow_flag != 0
    }

    /// Drive the TEST pin, an input the test registers can route to the
    /// envelope generator's clock.
    pub fn set_test_pin(&mut self, value: bool) {
        self.pin_test_in = u8::from(value);
    }

    /// The TEST pin as an output: high on the last slot of each sample while
    /// test register `$2C` bit 7 is set.
    #[must_use]
    pub fn test_pin(&self) -> bool {
        self.mode_test_2c[7] != 0 && self.cycles == 23
    }

    /// The samples completed since the last call, one per 24 internal
    /// clocks: the sum of the left and right pins across those clocks.
    pub fn take_samples(&mut self) -> Vec<[i32; 2]> {
        std::mem::take(&mut self.samples)
    }

    /// One internal clock: the whole pipeline advances one slot.
    pub fn clock(&mut self) {
        let slot = self.cycles;
        self.lfo_inc = self.mode_test_21[1];
        self.pg_read >>= 1;
        self.eg_read[1] >>= 1;
        self.eg_cycle = self.eg_cycle.wrapping_add(1);
        // Lock the envelope timer for this sample.
        if self.cycles == 1 && self.eg_quotient == 2 {
            self.eg_shift_lock = if self.eg_cycle_stop != 0 {
                0
            } else {
                self.eg_shift + 1
            };
            self.eg_timer_low_lock = (self.eg_timer & 3) as u8;
        }
        match self.cycles {
            0 => {
                self.lfo_pm = self.lfo_cnt >> 2;
                self.lfo_am = if self.lfo_cnt & 0x40 != 0 {
                    self.lfo_cnt & 0x3f
                } else {
                    self.lfo_cnt ^ 0x3f
                };
                self.lfo_am <<= 1;
            }
            1 => {
                self.eg_quotient = (self.eg_quotient + 1) % 3;
                self.eg_cycle = 0;
                self.eg_cycle_stop = 1;
                self.eg_shift = 0;
                self.eg_timer_inc |= self.eg_quotient >> 1;
                self.eg_timer += self.eg_timer_inc;
                self.eg_timer_inc = self.eg_timer >> 12;
                self.eg_timer &= 0xfff;
            }
            2 => {
                self.pg_read = self.pg_phase[21] & 0x3ff;
                self.eg_read[1] = u32::from(self.eg_out[0]);
            }
            13 => {
                self.eg_cycle = 0;
                self.eg_cycle_stop = 1;
                self.eg_shift = 0;
                self.eg_timer += self.eg_timer_inc;
                self.eg_timer_inc = self.eg_timer >> 12;
                self.eg_timer &= 0xfff;
            }
            23 => self.lfo_inc |= 1,
            _ => {}
        }
        self.eg_timer &= !((u32::from(self.mode_test_21[5]) << self.eg_cycle) as u16);
        if (((u32::from(self.eg_timer) >> self.eg_cycle) as u8)
            | (self.pin_test_in & self.eg_custom_timer))
            & self.eg_cycle_stop
            != 0
        {
            self.eg_shift = self.eg_cycle;
            self.eg_cycle_stop = 0;
        }

        self.do_io();
        self.do_timer_a();
        self.do_timer_b();
        self.key_on();
        self.ch_output();
        self.ch_generate();
        self.fm_prepare();
        self.fm_generate();
        self.phase_generate();
        self.phase_calc_increment();
        self.envelope_adsr();
        self.envelope_generate();
        self.envelope_ssg_eg();
        self.envelope_prepare();

        // Fetch the F-number and block for the next slot.
        let next = ((self.channel + 1) % 6) as usize;
        let (fnum, block, kcode) = if self.mode_ch3 != 0 {
            match slot {
                1 => (self.fnum_3ch[1], self.block_3ch[1], self.kcode_3ch[1]),
                7 => (self.fnum_3ch[0], self.block_3ch[0], self.kcode_3ch[0]),
                13 => (self.fnum_3ch[2], self.block_3ch[2], self.kcode_3ch[2]),
                _ => (self.fnum[next], self.block[next], self.kcode[next]),
            }
        } else {
            (self.fnum[next], self.block[next], self.kcode[next])
        };
        self.pg_fnum = fnum;
        self.pg_block = block;
        self.pg_kcode = kcode;

        self.update_lfo();
        self.do_reg_write();
        self.cycles = (self.cycles + 1) % SLOTS as u32;
        self.channel = self.cycles % 6;

        self.sample_l += i32::from(self.mol);
        self.sample_r += i32::from(self.mor);
        if self.cycles == 0 {
            self.samples.push([self.sample_l, self.sample_r]);
            self.sample_l = 0;
            self.sample_r = 0;
        }

        if self.status_time != 0 {
            self.status_time -= 1;
        }
    }

    /// Sample the write strobes and run the busy counter: a data write keeps
    /// the chip busy for 32 internal clocks.
    fn do_io(&mut self) {
        self.write_a_en = self.write_a & 3 == 1;
        self.write_d_en = self.write_d & 3 == 1;
        self.write_a <<= 1;
        self.write_d <<= 1;
        self.busy = self.write_busy;
        self.write_busy_cnt += u8::from(self.write_busy);
        self.write_busy = (self.write_busy && self.write_busy_cnt >> 5 == 0) || self.write_d_en;
        self.write_busy_cnt &= 0x1f;
    }

    /// Commit a latched register write as the pipeline reaches the slot or
    /// channel it addresses, and decode new address and data writes.
    fn do_reg_write(&mut self) {
        let mut slot = (self.cycles % 12) as usize;
        let channel = self.channel as usize;
        if self.write_fm_data {
            if OP_OFFSET[slot] == self.address & 0x107 {
                if self.address & 0x08 != 0 {
                    // Operators 2 and 4.
                    slot += 12;
                }
                let data = self.data;
                match self.address & 0xf0 {
                    0x30 => {
                        let multi = data & 0x0f;
                        self.multi[slot] = if multi == 0 { 1 } else { multi << 1 };
                        self.dt[slot] = (data >> 4) & 0x07;
                    }
                    0x40 => self.tl[slot] = data & 0x7f,
                    0x50 => {
                        self.ar[slot] = data & 0x1f;
                        self.ks[slot] = (data >> 6) & 0x03;
                    }
                    0x60 => {
                        self.dr[slot] = data & 0x1f;
                        self.am[slot] = data & 0x80 != 0;
                    }
                    0x70 => self.sr[slot] = data & 0x1f,
                    0x80 => {
                        self.rr[slot] = data & 0x0f;
                        let sl = (data >> 4) & 0x0f;
                        // Sustain level 15 means 31: fully attenuated.
                        self.sl[slot] = sl | ((sl + 1) & 0x10);
                    }
                    0x90 => self.ssg_eg[slot] = data & 0x0f,
                    _ => {}
                }
            }
            if CH_OFFSET[channel] == self.address & 0x103 {
                let data = self.data;
                match self.address & 0xfc {
                    0xa0 => {
                        self.fnum[channel] = u16::from(data) | (u16::from(self.reg_a4 & 7) << 8);
                        self.block[channel] = (self.reg_a4 >> 3) & 7;
                        self.kcode[channel] = (self.block[channel] << 2)
                            | FN_NOTE[usize::from(self.fnum[channel] >> 7)];
                    }
                    0xa4 => self.reg_a4 = data,
                    0xa8 => {
                        self.fnum_3ch[channel] =
                            u16::from(data) | (u16::from(self.reg_ac & 7) << 8);
                        self.block_3ch[channel] = (self.reg_ac >> 3) & 7;
                        self.kcode_3ch[channel] = (self.block_3ch[channel] << 2)
                            | FN_NOTE[usize::from(self.fnum_3ch[channel] >> 7)];
                    }
                    0xac => self.reg_ac = data,
                    0xb0 => {
                        self.connect[channel] = data & 7;
                        self.fb[channel] = (data >> 3) & 7;
                    }
                    0xb4 => {
                        self.pms[channel] = data & 7;
                        self.ams[channel] = (data >> 4) & 3;
                        self.pan_l[channel] = data & 0x80 != 0;
                        self.pan_r[channel] = data & 0x40 != 0;
                    }
                    _ => {}
                }
            }
        }

        if self.write_a_en || self.write_d_en {
            if self.write_a_en {
                self.write_fm_data = false;
            }
            if self.write_fm_address && self.write_d_en {
                self.write_fm_data = true;
            }
            if self.write_a_en {
                // Addresses below $10 belong to the SSG the OPN2 lacks.
                if self.write_data & 0xf0 != 0 {
                    self.address = self.write_data;
                    self.write_fm_address = true;
                } else {
                    self.write_fm_address = false;
                }
            }
            // The mode registers, which only part I has.
            if self.write_d_en && self.write_data & 0x100 == 0 {
                let data = self.write_data as u8;
                match self.write_fm_mode_a {
                    0x21 => {
                        for (i, bit) in self.mode_test_21.iter_mut().enumerate() {
                            *bit = (data >> i) & 1;
                        }
                    }
                    0x22 => {
                        self.lfo_en = if data & 0x08 != 0 { 0x7f } else { 0 };
                        self.lfo_freq = data & 7;
                    }
                    0x24 => {
                        self.timer_a_reg &= 0x03;
                        self.timer_a_reg |= u16::from(data) << 2;
                    }
                    0x25 => {
                        self.timer_a_reg &= 0x3fc;
                        self.timer_a_reg |= u16::from(data & 3);
                    }
                    0x26 => self.timer_b_reg = u16::from(data),
                    0x27 => {
                        self.mode_ch3 = (data & 0xc0) >> 6;
                        self.mode_csm = self.mode_ch3 == 2;
                        self.timer_a_load = data & 1;
                        self.timer_a_enable = (data >> 2) & 1;
                        self.timer_a_reset = (data >> 4) & 1;
                        self.timer_b_load = (data >> 1) & 1;
                        self.timer_b_enable = (data >> 3) & 1;
                        self.timer_b_reset = (data >> 5) & 1;
                    }
                    0x28 => {
                        for (i, op) in self.mode_kon_operator.iter_mut().enumerate() {
                            *op = (data >> (4 + i)) & 1 != 0;
                        }
                        self.mode_kon_channel = if data & 3 == 3 {
                            // Not a channel: matches no slot.
                            0xff
                        } else {
                            u32::from(data & 3) + u32::from((data >> 2) & 1) * 3
                        };
                    }
                    0x2a => {
                        self.dacdata &= 1;
                        self.dacdata |= i16::from(data ^ 0x80) << 1;
                    }
                    0x2b => self.dacen = data & 0x80 != 0,
                    0x2c => {
                        for (i, bit) in self.mode_test_2c.iter_mut().enumerate() {
                            *bit = (data >> i) & 1;
                        }
                        self.dacdata &= 0x1fe;
                        self.dacdata |= i16::from(self.mode_test_2c[3]);
                        self.eg_custom_timer =
                            u8::from(self.mode_test_2c[7] == 0 && self.mode_test_2c[6] != 0);
                    }
                    _ => {}
                }
            }
            if self.write_a_en {
                self.write_fm_mode_a = self.write_data & 0x1ff;
            }
        }

        if self.write_fm_data {
            self.data = self.write_data as u8;
        }
    }

    /// The phase increment for this slot: F-number with the phase LFO
    /// applied, shifted by the block, detuned, and multiplied.
    fn phase_calc_increment(&mut self) {
        let chan = self.channel as usize;
        let slot = self.cycles as usize;
        let mut fnum = u32::from(self.pg_fnum);
        let fnum_h = fnum >> 4;
        let lfo = self.lfo_pm;
        let mut lfo_l = usize::from(lfo & 0x0f);
        let pms = usize::from(self.pms[chan]);
        let dt = self.dt[slot];
        let dt_l = dt & 3;
        let mut kcode = self.pg_kcode;

        fnum <<= 1;
        if lfo_l & 0x08 != 0 {
            lfo_l ^= 0x0f;
        }
        let mut fm = (fnum_h >> PG_LFO_SH1[pms][lfo_l]) + (fnum_h >> PG_LFO_SH2[pms][lfo_l]);
        if pms > 5 {
            fm <<= pms - 5;
        }
        fm >>= 2;
        fnum = if lfo & 0x10 != 0 {
            fnum.wrapping_sub(fm)
        } else {
            fnum.wrapping_add(fm)
        };
        fnum &= 0xfff;

        let mut basefreq = (fnum << self.pg_block) >> 2;

        let mut detune = 0;
        if dt_l != 0 {
            kcode = kcode.min(0x1c);
            let block = kcode >> 2;
            let note = kcode & 3;
            // 0, 0, 2 or 3 for detune 0-3: a bitwise OR, not a truth value.
            let sum = block + 9 + (u8::from(dt_l == 3) | (dt_l & 2));
            let sum_h = sum >> 1;
            let sum_l = sum & 1;
            detune = PG_DETUNE[usize::from((sum_l << 2) | note)] >> (9 - sum_h);
        }
        basefreq = if dt & 4 != 0 {
            basefreq.wrapping_sub(detune)
        } else {
            basefreq.wrapping_add(detune)
        };
        basefreq &= 0x1ffff;
        self.pg_inc[slot] = ((basefreq * u32::from(self.multi[slot])) >> 1) & 0xfffff;
    }

    fn phase_generate(&mut self) {
        let slot = (self.cycles as usize + 20) % SLOTS;
        if self.pg_reset[slot] {
            self.pg_inc[slot] = 0;
        }
        let slot = (self.cycles as usize + 19) % SLOTS;
        if self.pg_reset[slot] || self.mode_test_21[3] != 0 {
            self.pg_phase[slot] = 0;
        }
        self.pg_phase[slot] = (self.pg_phase[slot] + self.pg_inc[slot]) & 0xfffff;
    }

    /// SSG-EG: the repeat, hold and invert latches for this slot.
    fn envelope_ssg_eg(&mut self) {
        let slot = self.cycles as usize;
        let ssg = self.ssg_eg[slot];
        let mut direction = false;
        self.eg_ssg_pgrst_latch[slot] = false;
        self.eg_ssg_repeat_latch[slot] = false;
        self.eg_ssg_hold_up_latch[slot] = false;
        if ssg & 0x08 != 0 {
            direction = self.eg_ssg_dir[slot];
            if self.eg_level[slot] & 0x200 != 0 {
                if ssg & 3 == 0 {
                    self.eg_ssg_pgrst_latch[slot] = true;
                }
                if ssg & 1 == 0 {
                    self.eg_ssg_repeat_latch[slot] = true;
                }
                if ssg & 3 == 2 {
                    direction = !direction;
                }
                if ssg & 3 == 3 {
                    direction = true;
                }
            }
            if self.eg_kon_latch[slot] && (ssg & 7 == 5 || ssg & 7 == 3) {
                self.eg_ssg_hold_up_latch[slot] = true;
            }
            direction &= self.eg_kon[slot];
        }
        self.eg_ssg_dir[slot] = direction;
        self.eg_ssg_enable[slot] = ssg & 0x08 != 0;
        self.eg_ssg_inv[slot] =
            (self.eg_ssg_dir[slot] ^ (ssg & 0x04 != 0 && ssg & 0x08 != 0)) && self.eg_kon[slot];
    }

    /// The envelope state machine: key on and off, attack, decay, sustain
    /// and release, for the slot two behind the pipeline head.
    fn envelope_adsr(&mut self) {
        let slot = (self.cycles as usize + 22) % SLOTS;
        let nkon = self.eg_kon_latch[slot];
        let okon = self.eg_kon[slot];
        let mut nextstate = self.eg_state[slot];
        let mut inc: i32 = 0;
        self.eg_read[0] = self.eg_read_inc;
        self.eg_read_inc = u32::from(self.eg_inc > 0);

        self.pg_reset[slot] = (nkon && !okon) || self.eg_ssg_pgrst_latch[slot];

        let kon_event = (nkon && !okon) || (okon && self.eg_ssg_repeat_latch[slot]);
        let koff_event = okon && !nkon;

        let mut level = i32::from(self.eg_level[slot]);
        let mut ssg_level = level;
        if self.eg_ssg_inv[slot] {
            ssg_level = (512 - level) & 0x3ff;
        }
        if koff_event {
            level = ssg_level;
        }
        let eg_off = if self.eg_ssg_enable[slot] {
            level >> 9 != 0
        } else {
            level & 0x3f0 == 0x3f0
        };
        let mut nextlevel = level;
        if kon_event {
            nextstate = EG_ATTACK;
            if self.eg_ratemax {
                // Instant attack.
                nextlevel = 0;
            } else if self.eg_state[slot] == EG_ATTACK && level != 0 && self.eg_inc != 0 && nkon {
                inc = (!level << self.eg_inc) >> 5;
            }
        } else {
            match self.eg_state[slot] {
                EG_ATTACK => {
                    if level == 0 {
                        nextstate = EG_DECAY;
                    } else if self.eg_inc != 0 && !self.eg_ratemax && nkon {
                        inc = (!level << self.eg_inc) >> 5;
                    }
                }
                EG_DECAY if level >> 4 == i32::from(self.eg_sl[1]) << 1 => {
                    nextstate = EG_SUSTAIN;
                }
                // Decay, sustain and release all step the attenuation up;
                // SSG-EG steps four times as fast.
                EG_DECAY | EG_SUSTAIN | EG_RELEASE if !eg_off && self.eg_inc != 0 => {
                    inc = 1 << (self.eg_inc - 1);
                    if self.eg_ssg_enable[slot] {
                        inc <<= 2;
                    }
                }
                _ => {}
            }
            if !nkon {
                nextstate = EG_RELEASE;
            }
        }
        if self.eg_kon_csm[slot] {
            nextlevel |= i32::from(self.eg_tl[1]) << 3;
        }
        if !kon_event
            && !self.eg_ssg_hold_up_latch[slot]
            && self.eg_state[slot] != EG_ATTACK
            && eg_off
        {
            nextstate = EG_RELEASE;
            nextlevel = 0x3ff;
        }
        nextlevel += inc;

        self.eg_kon[slot] = self.eg_kon_latch[slot];
        self.eg_level[slot] = (nextlevel & 0x3ff) as u16;
        self.eg_state[slot] = nextstate;
    }

    /// Choose this slot's rate and work out the envelope step it earns on
    /// this sample.
    fn envelope_prepare(&mut self) {
        let slot = self.cycles as usize;
        let rate = ((self.eg_rate << 1) + self.eg_ksv).min(0x3f);
        let sum = ((rate >> 2) + self.eg_shift_lock) & 0x0f;
        let mut inc = 0;
        if self.eg_rate != 0 && self.eg_quotient == 2 {
            if rate < 48 {
                inc = match sum {
                    12 => 1,
                    13 => (rate >> 1) & 1,
                    14 => rate & 1,
                    _ => 0,
                };
            } else {
                inc = (EG_STEP_HI[usize::from(rate & 3)][usize::from(self.eg_timer_low_lock)]
                    + (rate >> 2)
                    - 11)
                    .min(4);
            }
        }
        self.eg_inc = inc;
        self.eg_ratemax = rate >> 1 == 0x1f;

        let mut rate_sel = self.eg_state[slot];
        if (self.eg_kon[slot] && self.eg_ssg_repeat_latch[slot])
            || (!self.eg_kon[slot] && self.eg_kon_latch[slot])
        {
            rate_sel = EG_ATTACK;
        }
        match rate_sel {
            EG_ATTACK => self.eg_rate = self.ar[slot],
            EG_DECAY => self.eg_rate = self.dr[slot],
            EG_SUSTAIN => self.eg_rate = self.sr[slot],
            EG_RELEASE => self.eg_rate = (self.rr[slot] << 1) | 1,
            _ => {}
        }
        self.eg_ksv = self.pg_kcode >> (self.ks[slot] ^ 3);
        self.eg_lfo_am = if self.am[slot] {
            self.lfo_am >> EG_AM_SHIFT[usize::from(self.ams[self.channel as usize])]
        } else {
            0
        };
        // Total and sustain level reach the generator a slot late.
        self.eg_tl[1] = self.eg_tl[0];
        self.eg_tl[0] = self.tl[slot];
        self.eg_sl[1] = self.eg_sl[0];
        self.eg_sl[0] = self.sl[slot];
    }

    /// The attenuation the operator sees: envelope, AM LFO and total level.
    fn envelope_generate(&mut self) {
        let slot = (self.cycles as usize + 23) % SLOTS;
        let mut level = self.eg_level[slot];
        if self.eg_ssg_inv[slot] {
            level = 512u16.wrapping_sub(level);
        }
        if self.mode_test_21[5] != 0 {
            level = 0;
        }
        level &= 0x3ff;
        level += u16::from(self.eg_lfo_am);
        // In CSM mode channel 3's total level is applied by the key-on
        // instead. The channel counter runs one ahead of this stage.
        if !(self.mode_csm && self.channel == 3) {
            level += u16::from(self.eg_tl[0]) << 3;
        }
        self.eg_out[slot] = level.min(0x3ff);
    }

    fn update_lfo(&mut self) {
        let period = LFO_CYCLES[usize::from(self.lfo_freq)];
        if self.lfo_quotient & period == period {
            self.lfo_quotient = 0;
            self.lfo_cnt = self.lfo_cnt.wrapping_add(1);
        } else {
            self.lfo_quotient = self.lfo_quotient.wrapping_add(self.lfo_inc);
        }
        self.lfo_cnt &= self.lfo_en;
    }

    /// The phase modulation the next operator receives, by algorithm, and
    /// the operator outputs the feedback and modulation paths keep.
    fn fm_prepare(&mut self) {
        let slot = (self.cycles as usize + 6) % SLOTS;
        let channel = self.channel as usize;
        let op = slot / 6;
        let connect = usize::from(self.connect[channel]);
        let prevslot = (self.cycles as usize + 18) % SLOTS;
        let route = &FM_ALGORITHM[op];

        // Two inputs onto one wire: where both drive it, the bits OR.
        let mut mod1: i16 = 0;
        let mut mod2: i16 = 0;
        if route[0][connect] {
            mod2 |= self.fm_op1[channel][0];
        }
        if route[1][connect] {
            mod1 |= self.fm_op1[channel][1];
        }
        if route[2][connect] {
            mod1 |= self.fm_op2[channel];
        }
        if route[3][connect] {
            mod2 |= self.fm_out[prevslot];
        }
        if route[4][connect] {
            mod1 |= self.fm_out[prevslot];
        }
        let mut modulation = i32::from(mod1) + i32::from(mod2);
        if op == 0 {
            let fb = self.fb[channel];
            modulation = if fb == 0 { 0 } else { modulation >> (10 - fb) };
        } else {
            modulation >>= 1;
        }
        self.fm_mod[slot] = modulation as u16;

        let slot = (self.cycles as usize + 18) % SLOTS;
        if slot / 6 == 0 {
            self.fm_op1[channel][1] = self.fm_op1[channel][0];
            self.fm_op1[channel][0] = self.fm_out[slot];
        }
        if slot / 6 == 2 {
            self.fm_op2[channel] = self.fm_out[slot];
        }
    }

    /// One operator: phase plus modulation, through the log-sine ROM, plus
    /// attenuation, through the exponent ROM.
    fn fm_generate(&mut self) {
        let slot = (self.cycles as usize + 19) % SLOTS;
        let phase = (self.fm_mod[slot].wrapping_add((self.pg_phase[slot] >> 10) as u16)) & 0x3ff;
        let quarter = if phase & 0x100 != 0 {
            (phase ^ 0xff) & 0xff
        } else {
            phase & 0xff
        };
        let level = (LOGSIN[usize::from(quarter)] + (self.eg_out[slot] << 2)).min(0x1fff);
        let mut output =
            (i32::from(EXP[usize::from((level & 0xff) ^ 0xff)] | 0x400) << 2) >> (level >> 8);
        let test = i32::from(self.mode_test_21[4]) << 13;
        output = if phase & 0x200 != 0 {
            ((!output) ^ test) + 1
        } else {
            output ^ test
        };
        self.fm_out[slot] = sign_extend(13, output) as i16;
    }

    /// Add this slot's output to its channel if the algorithm makes it a
    /// carrier, and hand the finished channel on when its first operator
    /// comes round again.
    fn ch_generate(&mut self) {
        let slot = (self.cycles as usize + 18) % SLOTS;
        let channel = self.channel as usize;
        let op = slot / 6;
        let test_dac = self.mode_test_2c[5] != 0;
        let mut acc = self.ch_acc[channel];
        let mut add = i16::from(test_dac);
        if op == 0 && !test_dac {
            acc = 0;
        }
        if FM_ALGORITHM[op][5][usize::from(self.connect[channel])] && !test_dac {
            add += self.fm_out[slot] >> 5;
        }
        let sum = (acc + add).clamp(-256, 255);
        if op == 0 || test_dac {
            self.ch_out[channel] = self.ch_acc[channel];
        }
        self.ch_acc[channel] = sum;
    }

    /// Drive the output pins. Each channel gets a quarter of the sample in
    /// turn; on the YM2612 the DAC shows it for one clock in four and the
    /// sign level for the rest.
    fn ch_output(&mut self) {
        let cycles = self.cycles;
        let mut channel = self.channel as usize;
        let test_dac = self.mode_test_2c[5] != 0;
        self.ch_read = self.ch_lock;
        if cycles < 12 {
            // Channels 4-6.
            channel += 1;
        }
        if cycles & 3 == 0 {
            if !test_dac {
                self.ch_lock = self.ch_out[channel];
            }
            self.ch_lock_l = self.pan_l[channel];
            self.ch_lock_r = self.pan_r[channel];
        }
        let mut out = if (cycles >> 2 == 1 && self.dacen) || test_dac {
            sign_extend(8, i32::from(self.dacdata)) as i16
        } else {
            self.ch_lock
        };
        self.mol = 0;
        self.mor = 0;
        match self.variant {
            Variant::Ym2612 => {
                let out_en = cycles & 3 == 3 || test_dac;
                let mut sign = out >> 8;
                if out >= 0 {
                    out += 1;
                    sign += 1;
                }
                self.mol = if self.ch_lock_l && out_en { out } else { sign };
                self.mor = if self.ch_lock_r && out_en { out } else { sign };
                self.mol *= 3;
                self.mor *= 3;
            }
            Variant::Ym3438 => {
                let out_en = cycles & 3 != 0 || test_dac;
                if self.ch_lock_l && out_en {
                    self.mol = out;
                }
                if self.ch_lock_r && out_en {
                    self.mor = out;
                }
            }
        }
    }

    fn do_timer_a(&mut self) {
        let mut load = self.timer_a_overflow;
        if self.cycles == 2 {
            load |= u8::from(self.timer_a_load_lock == 0 && self.timer_a_load != 0);
            self.timer_a_load_lock = self.timer_a_load;
            self.mode_kon_csm = self.mode_csm && load != 0;
        }
        let mut time = if self.timer_a_load_latch != 0 {
            self.timer_a_reg
        } else {
            self.timer_a_cnt
        };
        self.timer_a_load_latch = load;
        if (self.cycles == 1 && self.timer_a_load_lock != 0) || self.mode_test_21[2] != 0 {
            time += 1;
        }
        if self.timer_a_reset != 0 {
            self.timer_a_reset = 0;
            self.timer_a_overflow_flag = 0;
        } else {
            self.timer_a_overflow_flag |= self.timer_a_overflow & self.timer_a_enable;
        }
        self.timer_a_overflow = (time >> 10) as u8;
        self.timer_a_cnt = time & 0x3ff;
    }

    fn do_timer_b(&mut self) {
        let mut load = self.timer_b_overflow;
        if self.cycles == 2 {
            load |= u8::from(self.timer_b_load_lock == 0 && self.timer_b_load != 0);
            self.timer_b_load_lock = self.timer_b_load;
        }
        let mut time = if self.timer_b_load_latch != 0 {
            self.timer_b_reg
        } else {
            self.timer_b_cnt
        };
        self.timer_b_load_latch = load;
        if self.cycles == 1 {
            self.timer_b_subcnt += 1;
        }
        if (self.timer_b_subcnt == 0x10 && self.timer_b_load_lock != 0) || self.mode_test_21[2] != 0
        {
            time += 1;
        }
        self.timer_b_subcnt &= 0x0f;
        if self.timer_b_reset != 0 {
            self.timer_b_reset = 0;
            self.timer_b_overflow_flag = 0;
        } else {
            self.timer_b_overflow_flag |= self.timer_b_overflow & self.timer_b_enable;
        }
        self.timer_b_overflow = (time >> 8) as u8;
        self.timer_b_cnt = time & 0xff;
    }

    /// Latch this slot's key state, including CSM's key-on of channel 3,
    /// and take a `$28` write when the pipeline reaches its channel.
    fn key_on(&mut self) {
        let slot = self.cycles as usize;
        let chan = self.channel as usize;
        self.eg_kon_latch[slot] = self.mode_kon[slot];
        self.eg_kon_csm[slot] = false;
        if self.channel == 2 && self.mode_kon_csm {
            self.eg_kon_latch[slot] = true;
            self.eg_kon_csm[slot] = true;
        }
        if self.cycles == self.mode_kon_channel {
            self.mode_kon[chan] = self.mode_kon_operator[0];
            self.mode_kon[chan + 12] = self.mode_kon_operator[1];
            self.mode_kon[chan + 6] = self.mode_kon_operator[2];
            self.mode_kon[chan + 18] = self.mode_kon_operator[3];
        }
    }
}

#[cfg(test)]
mod tests;
