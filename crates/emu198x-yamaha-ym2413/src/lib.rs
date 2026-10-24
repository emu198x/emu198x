//! The Yamaha YM2413 (OPLL) FM synthesis chip and Konami's VRC7 sound core.
//!
//! Nine channels of two-operator FM, or six channels and five rhythm voices,
//! with 15 instruments in ROM and one the program defines. The sound of the
//! Japanese Master System's FM unit, MSX-MUSIC and the FM-PAC cartridge. The
//! VRC7 (the DS1001 core on Konami's Famicom mapper) is the same design with
//! six channels and its own instrument ROM.
//!
//! # How it is modelled
//!
//! The chip divides its input clock by four. Each internal clock works on one
//! of 18 operator slots, and the slots run as a pipeline: on any one clock
//! the phase generator, the envelope generator and the operator are each
//! working on a different slot. A sample takes 18 internal clocks, 72 input
//! clocks in all. This crate steps that pipeline one internal clock at a
//! time, so register writes land on the slot the silicon would land them on
//! and the two output pins change when the silicon's do.
//!
//! The pipeline, its stage offsets and its arithmetic follow Nuked-OPLL
//! 1.0.2 by Nuke.YKT, commit `1269cf5a`, which was reverse-engineered from
//! the YM2413 and VRC7 die shots by digshadow and John McMaster (Silicon
//! Pr0n). This crate is a Rust re-expression of that model, not a binding to
//! it; Nuked-OPLL is GPL-2.0-or-later, the same licence as this crate. Its
//! tests compare the output pins with Nuked-OPLL's, clock by clock.
//!
//! # Using it
//!
//! Drive [`Ym2413::tick`] from the chip's input clock (3.58 MHz on the
//! Master System and the MSX). Write with [`Ym2413::write`]: port 0 takes a
//! register address, port 1 the data. The chip has no readable registers.
//!
//! After each internal clock the `mo` and `ro` fields hold the melody and
//! rhythm output pins. [`Ym2413::take_samples`] returns one `[mo, ro]` pair
//! per 18 internal clocks, each the sum of its pin across them. On a real
//! board the two pins are mixed with resistors that differ by board; the
//! YM2413 application note weights the rhythm pin about twice the melody,
//! and the host decides.
//!
//! The chip exposes its state as public fields and is stepped by the
//! machine, so it owns no bus and makes no callbacks, the same as every
//! Emu198x chip.

mod tables;

use serde::{Deserialize, Serialize};

use tables::{CH_OFFSET, EG_KSL, EG_STEP_HI, EXP, LOGSIN, PATCHES_VRC7, PATCHES_YM2413, PG_MULTI};

/// Input clocks per internal clock.
pub const PRESCALER: u8 = 4;

/// Internal clocks per output sample: one per operator slot.
pub const SLOTS: u32 = 18;

const EG_ATTACK: u8 = 0;
const EG_DECAY: u8 = 1;
const EG_SUSTAIN: u8 = 2;
const EG_RELEASE: u8 = 3;

/// Rhythm voices, as the slot schedule visits them. `RM_NONE` is any slot
/// that is not a rhythm voice's.
const RM_BD0: u8 = 0;
const RM_HH: u8 = 1;
const RM_TOM: u8 = 2;
const RM_BD1: u8 = 3;
const RM_SD: u8 = 4;
const RM_TC: u8 = 5;
const RM_NONE: u8 = RM_TC + 1;

/// Which chip: they share the synthesis pipeline and differ in their
/// instrument ROM, their rhythm section and their output stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Variant {
    /// The YM2413 itself: nine channels or six plus rhythm, a melody pin
    /// and a rhythm pin.
    Ym2413,
    /// Konami's VRC7 (DS1001). Rhythm mode is wired on inside, which leaves
    /// six melodic channels; its instrument ROM is its own and it drives
    /// only the melody pin.
    Vrc7,
}

/// One instrument: the settings the user instrument's eight registers hold,
/// and the shape of each ROM instrument. Two-element fields are
/// `[modulator, carrier]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patch {
    /// The modulator's total level.
    pub tl: u8,
    /// The carrier's half-wave rectification.
    pub dc: u8,
    /// The modulator's half-wave rectification.
    pub dm: u8,
    /// The modulator's self-feedback.
    pub fb: u8,
    pub am: [u8; 2],
    pub vib: [u8; 2],
    /// Envelope type: 1 holds the sustain level while the key is down.
    pub et: [u8; 2],
    pub ksr: [u8; 2],
    pub multi: [u8; 2],
    pub ksl: [u8; 2],
    pub ar: [u8; 2],
    pub dr: [u8; 2],
    pub sl: [u8; 2],
    pub rr: [u8; 2],
}

/// One YM2413 or VRC7.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ym2413 {
    pub variant: Variant,
    /// Input clocks since the last internal clock.
    pub prescaler: u8,
    /// The slot the pipeline is on, 0-17.
    pub cycles: u32,
    /// The melody output pin for this internal clock.
    pub mo: i16,
    /// The rhythm output pin for this internal clock.
    pub ro: i16,

    // Bus interface.
    write_data: u8,
    write_a: u8,
    write_d: u8,
    write_a_en: bool,
    write_d_en: bool,
    write_fm_address: bool,
    write_fm_data: bool,
    write_mode_address: u8,
    address: u8,
    data: u8,

    // Envelope generator.
    eg_counter_state: u8,
    eg_counter_state_prev: u8,
    eg_timer: u32,
    eg_timer_low_lock: u8,
    eg_timer_carry: u8,
    eg_timer_shift: u8,
    eg_timer_shift_lock: u8,
    eg_timer_shift_stop: u8,
    eg_state: [u8; 18],
    eg_level: [u8; 18],
    eg_kon: u8,
    eg_dokon: u32,
    eg_off: u8,
    eg_rate: u8,
    eg_maxrate: bool,
    eg_zerorate: bool,
    eg_inc_lo: u8,
    eg_inc_hi: u8,
    eg_rate_hi: u8,
    eg_sl: u8,
    eg_ksltl: u16,
    eg_out: u8,
    eg_silent: bool,

    // Phase generator.
    pg_out: u16,
    pg_inc: u32,
    pg_phase: [u32; 18],
    pg_phase_next: u32,

    // Operator.
    op_fb1: [i16; 9],
    op_fb2: [i16; 9],
    op_fbsum: i16,
    op_mod: i16,
    op_neg: u8,
    op_logsin: u16,
    op_exp_m: u16,
    op_exp_s: u16,

    // Channel.
    ch_out: i16,
    ch_out_hh: i16,
    ch_out_tm: i16,
    ch_out_bd: i16,
    ch_out_sd: i16,
    ch_out_tc: i16,

    // LFO.
    lfo_counter: u16,
    lfo_vib_counter: u8,
    lfo_am_counter: u16,
    lfo_am_step: u8,
    lfo_am_dir: u8,
    lfo_am_car: u8,
    lfo_am_out: u8,

    // Registers.
    fnum: [u16; 9],
    block: [u8; 9],
    kon: [u8; 9],
    son: [u8; 9],
    vol: [u8; 9],
    inst: [u8; 9],
    rhythm: u8,
    testmode: u8,
    patch: Patch,

    // The slot being prepared.
    c_tl: u8,
    c_dc: u8,
    c_dm: u8,
    c_fb: u8,
    c_am: u8,
    c_vib: u8,
    c_et: u8,
    c_ksr: u8,
    c_ksr_freq: u8,
    c_ksl_freq: u8,
    c_ksl_block: u8,
    c_multi: u8,
    c_ksl: u8,
    c_adrr: [u8; 3],
    c_sl: u8,
    c_fnum: u16,
    c_block: u16,

    // Rhythm.
    /// A delay line: bit 7 is rhythm mode as written, and each clock
    /// shifts it right with the sign bit held, so bit 6 is the same a clock
    /// later.
    rm_enable: i8,
    rm_noise: u32,
    rm_select: u8,
    rm_hh_bit2: u8,
    rm_hh_bit3: u8,
    rm_hh_bit7: u8,
    rm_hh_bit8: u8,
    rm_tc_bit3: u8,
    rm_tc_bit5: u8,

    // Output: what the channel stage computed, which reaches the pins on the
    // next clock.
    output_m: i16,
    output_r: i16,
    slot_in_sample: u32,
    sample_mo: i32,
    sample_ro: i32,
    samples: Vec<[i32; 2]>,
}

impl Ym2413 {
    /// A chip just out of reset.
    #[must_use]
    pub fn new(variant: Variant) -> Self {
        let vrc7 = variant == Variant::Vrc7;
        Self {
            variant,
            prescaler: 0,
            cycles: 0,
            mo: 0,
            ro: 0,
            write_data: 0,
            write_a: 0,
            write_d: 0,
            write_a_en: false,
            write_d_en: false,
            write_fm_address: false,
            write_fm_data: false,
            write_mode_address: 0,
            address: 0,
            data: 0,
            eg_counter_state: 0,
            eg_counter_state_prev: 0,
            eg_timer: 0,
            eg_timer_low_lock: 0,
            eg_timer_carry: 0,
            eg_timer_shift: 0,
            eg_timer_shift_lock: 0,
            eg_timer_shift_stop: 0,
            eg_state: [EG_RELEASE; 18],
            eg_level: [0x7f; 18],
            eg_kon: 0,
            eg_dokon: 0,
            eg_off: 0,
            eg_rate: 0,
            eg_maxrate: false,
            eg_zerorate: false,
            eg_inc_lo: 0,
            eg_inc_hi: 0,
            eg_rate_hi: 0,
            eg_sl: 0,
            eg_ksltl: 0,
            eg_out: 0x7f,
            eg_silent: false,
            pg_out: 0,
            pg_inc: 0,
            pg_phase: [0; 18],
            pg_phase_next: 0,
            op_fb1: [0; 9],
            op_fb2: [0; 9],
            op_fbsum: 0,
            op_mod: 0,
            op_neg: 0,
            op_logsin: 0,
            op_exp_m: 0,
            op_exp_s: 0,
            ch_out: 0,
            ch_out_hh: 0,
            ch_out_tm: 0,
            ch_out_bd: 0,
            ch_out_sd: 0,
            ch_out_tc: 0,
            lfo_counter: 0,
            lfo_vib_counter: 0,
            lfo_am_counter: 0,
            lfo_am_step: 0,
            lfo_am_dir: 0,
            lfo_am_car: 0,
            lfo_am_out: 0,
            fnum: [0; 9],
            block: [0; 9],
            kon: [0; 9],
            son: [0; 9],
            vol: [0; 9],
            inst: [0; 9],
            // The VRC7 has rhythm mode wired on.
            rhythm: if vrc7 { 0x20 } else { 0 },
            testmode: 0,
            patch: Patch::default(),
            c_tl: 0,
            c_dc: 0,
            c_dm: 0,
            c_fb: 0,
            c_am: 0,
            c_vib: 0,
            c_et: 0,
            c_ksr: 0,
            c_ksr_freq: 0,
            c_ksl_freq: 0,
            c_ksl_block: 0,
            c_multi: 0,
            c_ksl: 0,
            c_adrr: [0; 3],
            c_sl: 0,
            c_fnum: 0,
            c_block: 0,
            rm_enable: if vrc7 { i8::MIN } else { 0 },
            rm_noise: 0,
            rm_select: RM_NONE,
            rm_hh_bit2: 0,
            rm_hh_bit3: 0,
            rm_hh_bit7: 0,
            rm_hh_bit8: 0,
            rm_tc_bit3: 0,
            rm_tc_bit5: 0,
            output_m: 0,
            output_r: 0,
            slot_in_sample: 0,
            sample_mo: 0,
            sample_ro: 0,
            samples: Vec::new(),
        }
    }

    /// Pull the reset line: everything returns to its power-on state except
    /// the variant.
    pub fn reset(&mut self) {
        *self = Self::new(self.variant);
    }

    /// One input clock. Every fourth runs an internal clock.
    pub fn tick(&mut self) {
        self.prescaler += 1;
        if self.prescaler == PRESCALER {
            self.prescaler = 0;
            self.clock();
        }
    }

    /// Write `data` to port `port` (A0): 0 takes a register address, 1 the
    /// data. The chip samples the write on its next internal clock, and a
    /// data write reaches its register when the pipeline next passes the
    /// register's channel. Software must leave 12 input clocks after an
    /// address write and 84 after a data write; this model, like the chip,
    /// does nothing to protect a write made sooner.
    pub fn write(&mut self, port: u8, data: u8) {
        self.write_data = data;
        if port & 1 != 0 {
            self.write_d |= 1;
        } else {
            self.write_a |= 1;
        }
    }

    /// The samples completed since the last call, one per 18 internal
    /// clocks: the melody and rhythm pins, each summed across those clocks.
    pub fn take_samples(&mut self) -> Vec<[i32; 2]> {
        std::mem::take(&mut self.samples)
    }

    /// The user instrument (registers `$00`-`$07`) as last written.
    #[must_use]
    pub fn user_patch(&self) -> Patch {
        self.patch
    }

    fn patch_rom(&self) -> &'static [Patch; 21] {
        match self.variant {
            Variant::Ym2413 => &PATCHES_YM2413,
            Variant::Vrc7 => &PATCHES_VRC7,
        }
    }

    /// One internal clock: the whole pipeline advances one slot.
    pub fn clock(&mut self) {
        // The pins are what the previous clock produced.
        self.mo = self.output_m;
        self.ro = self.output_r;
        self.accumulate();

        if self.cycles == 0 {
            self.lfo_am_out = ((self.lfo_am_counter >> 3) & 0x0f) as u8;
        }
        self.rm_enable >>= 1;
        self.do_mode_write();
        self.rm_select += 1;
        if self.rm_select > RM_TC {
            self.rm_select = RM_NONE;
        }
        if self.cycles == 11 && self.rhythm_now() {
            self.rm_select = RM_BD0;
        }
        self.prepare_patch1();
        self.channel();
        self.phase_generate();
        self.operator();
        self.phase_calc_increment();
        self.envelope_output();
        self.envelope_ksl_tl();
        self.envelope_generate();
        self.do_lfo();
        self.do_rhythm();
        self.prepare_patch2();
        self.do_reg_write();
        self.do_io();
        self.cycles = (self.cycles + 1) % SLOTS;
    }

    fn accumulate(&mut self) {
        self.sample_mo += i32::from(self.mo);
        self.sample_ro += i32::from(self.ro);
        self.slot_in_sample += 1;
        if self.slot_in_sample == SLOTS {
            self.samples.push([self.sample_mo, self.sample_ro]);
            self.slot_in_sample = 0;
            self.sample_mo = 0;
            self.sample_ro = 0;
        }
    }

    /// Rhythm mode as the current slot sees it.
    fn rhythm_now(&self) -> bool {
        self.rm_enable as u8 & 0x80 != 0
    }

    /// Rhythm mode one clock later in the pipeline.
    fn rhythm_late(&self) -> bool {
        self.rm_enable as u8 & 0x40 != 0
    }

    fn do_io(&mut self) {
        self.write_a_en = self.write_a & 3 == 1;
        self.write_d_en = self.write_d & 3 == 1;
        self.write_a <<= 1;
        self.write_d <<= 1;
    }

    fn do_mode_write(&mut self) {
        if self.write_mode_address & 0x10 == 0 || !self.write_d_en {
            return;
        }
        let slot = usize::from(self.write_mode_address & 1);
        let data = self.write_data;
        match self.write_mode_address & 0x0f {
            0x00 | 0x01 => {
                self.patch.multi[slot] = data & 0x0f;
                self.patch.ksr[slot] = (data >> 4) & 1;
                self.patch.et[slot] = (data >> 5) & 1;
                self.patch.vib[slot] = (data >> 6) & 1;
                self.patch.am[slot] = (data >> 7) & 1;
            }
            0x02 => {
                self.patch.ksl[0] = (data >> 6) & 3;
                self.patch.tl = data & 0x3f;
            }
            0x03 => {
                self.patch.ksl[1] = (data >> 6) & 3;
                self.patch.dc = (data >> 4) & 1;
                self.patch.dm = (data >> 3) & 1;
                self.patch.fb = data & 7;
            }
            0x04 | 0x05 => {
                self.patch.dr[slot] = data & 0x0f;
                self.patch.ar[slot] = (data >> 4) & 0x0f;
            }
            0x06 | 0x07 => {
                self.patch.rr[slot] = data & 0x0f;
                self.patch.sl[slot] = (data >> 4) & 0x0f;
            }
            0x0e => {
                self.rhythm = data & 0x3f;
                if self.variant == Variant::Vrc7 {
                    self.rhythm |= 0x20;
                }
                self.rm_enable =
                    ((self.rm_enable as u8 & 0x7f) | ((self.rhythm << 2) & 0x80)) as i8;
            }
            0x0f => self.testmode = data & 0x0f,
            _ => {}
        }
    }

    fn do_reg_write(&mut self) {
        if self.write_a_en {
            if self.write_data & 0xc0 == 0 {
                self.write_fm_address = true;
                self.address = self.write_data;
            } else {
                self.write_fm_address = false;
            }
        }
        if self.write_fm_address && self.write_d_en {
            self.data = self.write_data;
        }

        // A data write reaches a channel register when the pipeline passes
        // the slot whose number matches the register's low nibble.
        if self.write_fm_data
            && !self.write_a_en
            && u32::from(self.address & 0x0f) == self.cycles
            && self.cycles < 16
        {
            let channel = (self.cycles % 9) as usize;
            let data = self.data;
            match self.address & 0xf0 {
                0x10 => self.fnum[channel] = (self.fnum[channel] & 0x100) | u16::from(data),
                0x20 => {
                    self.fnum[channel] = (self.fnum[channel] & 0xff) | (u16::from(data & 1) << 8);
                    self.block[channel] = (data >> 1) & 7;
                    self.kon[channel] = (data >> 4) & 1;
                    self.son[channel] = (data >> 5) & 1;
                }
                0x30 => {
                    self.vol[channel] = data & 0x0f;
                    self.inst[channel] = (data >> 4) & 0x0f;
                }
                _ => {}
            }
        }

        if self.write_a_en {
            self.write_fm_data = false;
        }
        if self.write_fm_address && self.write_d_en {
            self.write_fm_data = true;
        }
        if self.write_a_en {
            self.write_mode_address = if self.write_data & 0xf0 == 0 {
                0x10 | (self.write_data & 0x0f)
            } else {
                0
            };
        }
    }

    /// Which instrument the slot being prepared plays: a rhythm patch, a ROM
    /// instrument or the user's.
    fn slot_patch(&self) -> Patch {
        let instr = self.inst[CH_OFFSET[self.cycles as usize]];
        if self.rm_select <= RM_TC {
            self.patch_rom()[15 + usize::from(self.rm_select)]
        } else if instr > 0 {
            self.patch_rom()[usize::from(instr) - 1]
        } else {
            self.patch
        }
    }

    fn mcsel(&self) -> usize {
        (((self.cycles + 1) / 3) & 1) as usize
    }

    fn prepare_patch1(&mut self) {
        let mcsel = self.mcsel();
        let ch = CH_OFFSET[self.cycles as usize];
        let patch = self.slot_patch();
        self.c_tl = if self.rm_select == RM_HH || self.rm_select == RM_TOM {
            self.inst[ch] << 2
        } else if mcsel == 1 {
            self.vol[ch] << 2
        } else {
            patch.tl
        };
        self.c_adrr = [patch.ar[mcsel], patch.dr[mcsel], patch.rr[mcsel]];
        self.c_et = patch.et[mcsel];
        self.c_ksr = patch.ksr[mcsel];
        self.c_ksl = patch.ksl[mcsel];
        self.c_ksr_freq = (self.block[ch] << 1) | (self.fnum[ch] >> 8) as u8;
        self.c_ksl_freq = (self.fnum[ch] >> 5) as u8;
        self.c_ksl_block = self.block[ch];
    }

    fn prepare_patch2(&mut self) {
        let mcsel = self.mcsel();
        let ch = CH_OFFSET[self.cycles as usize];
        let patch = self.slot_patch();
        self.c_fnum = self.fnum[ch];
        self.c_block = u16::from(self.block[ch]);
        self.c_multi = patch.multi[mcsel];
        self.c_sl = patch.sl[mcsel];
        self.c_fb = patch.fb;
        self.c_vib = patch.vib[mcsel];
        self.c_am = patch.am[mcsel];
        self.c_dc = (self.c_dc << 1) | patch.dc;
        self.c_dm = (self.c_dm << 1) | patch.dm;
    }

    fn phase_generate(&mut self) {
        let cycles = self.cycles as usize;
        self.pg_phase[(cycles + 17) % 18] = self.pg_phase_next.wrapping_add(self.pg_inc);

        let ismod = if self.rhythm_late() && (cycles == 13 || cycles == 14) {
            false
        } else {
            ((cycles + 3) / 3) & 1 != 0
        };
        let phase = self.pg_phase[cycles];
        // A key-on restarts the phase.
        self.pg_phase_next = if self.testmode & 4 != 0
            || (ismod && self.eg_dokon & 0x8000 != 0)
            || (!ismod && self.eg_dokon & 1 != 0)
        {
            0
        } else {
            phase
        };

        let bit = |n: u32| ((phase >> (n + 9)) & 1) as u8;
        if cycles == 13 {
            self.rm_hh_bit2 = bit(2);
            self.rm_hh_bit3 = bit(3);
            self.rm_hh_bit7 = bit(7);
            self.rm_hh_bit8 = bit(8);
        } else if cycles == 17 && self.rhythm_now() {
            self.rm_tc_bit3 = bit(3);
            self.rm_tc_bit5 = bit(5);
        }

        self.pg_out = if self.rhythm_now() {
            let rm_bit = (self.rm_hh_bit2 ^ self.rm_hh_bit7)
                | (self.rm_hh_bit3 ^ self.rm_tc_bit5)
                | (self.rm_tc_bit3 ^ self.rm_tc_bit5);
            let noise = (self.rm_noise & 1) as u8;
            match cycles {
                // High hat.
                13 => {
                    let base = u16::from(rm_bit) << 9;
                    if rm_bit ^ noise != 0 {
                        base | 0xd0
                    } else {
                        base | 0x34
                    }
                }
                // Snare drum.
                16 => (u16::from(self.rm_hh_bit8) << 9) | (u16::from(self.rm_hh_bit8 ^ noise) << 8),
                // Top cymbal.
                17 => (u16::from(rm_bit) << 9) | 0x100,
                _ => (phase >> 9) as u16,
            }
        } else {
            (phase >> 9) as u16
        };
    }

    fn phase_calc_increment(&mut self) {
        let mut freq = u32::from(self.c_fnum) << 1;
        if self.c_vib != 0 {
            match self.lfo_vib_counter {
                1 | 3 => freq += freq >> 8,
                2 => freq += freq >> 7,
                5 | 7 => freq -= freq >> 8,
                6 => freq -= freq >> 7,
                _ => {}
            }
        }
        freq = (freq << self.c_block) >> 1;
        self.pg_inc = (freq * PG_MULTI[usize::from(self.c_multi)]) >> 1;
    }

    fn envelope_ksl_tl(&mut self) {
        let mut ksl =
            EG_KSL[usize::from(self.c_ksl_freq)] - ((8 - i32::from(self.c_ksl_block)) << 3);
        if ksl < 0 {
            ksl = 0;
        }
        ksl <<= 1;
        ksl = if self.c_ksl != 0 {
            ksl >> (3 - self.c_ksl)
        } else {
            0
        };
        self.eg_ksltl = (ksl + (i32::from(self.c_tl) << 1)) as u16;
    }

    fn envelope_output(&mut self) {
        let mut level =
            i32::from(self.eg_level[(self.cycles as usize + 17) % 18]) + i32::from(self.eg_ksltl);
        if self.c_am != 0 {
            level += i32::from(self.lfo_am_out);
        }
        if level >= 128 {
            level = 127;
        }
        if self.testmode & 1 != 0 {
            level = 0;
        }
        self.eg_out = level as u8;
    }

    /// Whether a decay, sustain or release step applies on this clock.
    fn eg_decay_step(&self) -> i32 {
        let hi = self.eg_rate_hi;
        let inc_hi = self.eg_inc_hi != 0;
        let prev = self.eg_counter_state_prev;
        let i0 = hi == 15 || (hi == 14 && inc_hi);
        let i1 = (hi == 14 && !inc_hi)
            || (hi == 13 && inc_hi)
            || (hi == 13 && !inc_hi && prev & 1 != 0)
            || (hi == 12 && inc_hi && prev & 1 != 0)
            || (hi == 12 && !inc_hi && prev & 3 == 3)
            || (self.eg_inc_lo != 0 && prev & 3 == 3);
        (i32::from(i0) << 1) | i32::from(i1)
    }

    fn envelope_generate(&mut self) {
        let cycles = self.cycles as usize;
        let mcsel = self.mcsel() == 1;
        let ch = CH_OFFSET[cycles];

        // The envelope timer.
        let timer_inc = if self.eg_counter_state & 3 != 3 {
            0
        } else if cycles == 0 {
            1
        } else {
            self.eg_timer_carry
        };
        let timer_low = (self.eg_timer & 3) as u8;
        let timer_bit = (self.eg_timer & 1) as u8 + timer_inc;
        self.eg_timer_carry = timer_bit >> 1;
        self.eg_timer = (u32::from(timer_bit & 1) << 17) | (self.eg_timer >> 1);
        if self.testmode & 8 != 0 {
            self.eg_timer &= 0x2ffff;
            self.eg_timer |= (u32::from(self.write_data) << 14) & 0x10000;
        }
        if self.eg_timer_shift_stop == 0 && (self.eg_timer >> 16) & 1 != 0 {
            self.eg_timer_shift = self.cycles as u8;
        }
        if cycles == 0 && self.eg_counter_state_prev & 1 == 1 {
            self.eg_timer_low_lock = timer_low;
            self.eg_timer_shift_lock = self.eg_timer_shift;
            if self.eg_timer_shift_lock > 13 {
                self.eg_timer_shift_lock = 0;
            }
            self.eg_timer_shift = 0;
        }
        self.eg_timer_shift_stop |= ((self.eg_timer >> 16) & 1) as u8;
        if cycles == 0 {
            self.eg_timer_shift_stop = 0;
        }
        self.eg_counter_state_prev = self.eg_counter_state;
        if cycles == 17 {
            self.eg_counter_state = self.eg_counter_state.wrapping_add(1);
        }

        // The level two slots back.
        let back = (cycles + 16) % 18;
        let level = i32::from(self.eg_level[back]);
        let mut next_level = level;
        let zero = level == 0;
        self.eg_silent = level == 0x7f;

        if self.eg_state[back] != EG_ATTACK && self.eg_off & 2 != 0 && self.eg_dokon & 2 == 0 {
            next_level = 0x7f;
        }
        if self.eg_maxrate && self.eg_dokon & 2 != 0 {
            next_level = 0;
        }

        let state = self.eg_state[back];
        let sl = i32::from(self.eg_sl);
        let quiet = self.eg_off & 2 == 0 && self.eg_dokon & 2 == 0;
        let (mut next_state, step) = match state {
            EG_ATTACK => {
                let mut step = 0;
                if !self.eg_maxrate && self.eg_kon & 2 != 0 && !zero {
                    let shift = if self.eg_rate_hi < 12 {
                        i32::from(self.eg_inc_lo)
                    } else {
                        i32::from(self.eg_rate_hi) - 11 + i32::from(self.eg_inc_hi)
                    };
                    if shift > 0 {
                        step = !level >> (5 - shift);
                    }
                }
                (if zero { EG_DECAY } else { EG_ATTACK }, step)
            }
            EG_DECAY => {
                let step = if quiet && (level >> 3) != sl {
                    self.eg_decay_step()
                } else {
                    0
                };
                (
                    if (level >> 3) == sl {
                        EG_SUSTAIN
                    } else {
                        EG_DECAY
                    },
                    step,
                )
            }
            _ => (state, if quiet { self.eg_decay_step() } else { 0 }),
        };
        if self.eg_kon & 2 == 0 {
            next_state = EG_RELEASE;
        }
        if self.eg_dokon & 2 != 0 {
            next_state = EG_ATTACK;
        }
        self.eg_level[back] = (next_level + step) as u8;
        self.eg_state[back] = next_state;

        let rate_hi = self.eg_rate >> 2;
        let rate_lo = self.eg_rate & 3;
        self.eg_inc_hi = EG_STEP_HI[usize::from(rate_lo)][usize::from(self.eg_timer_low_lock)];
        let sum = (self.eg_timer_shift_lock + rate_hi) & 0x0f;
        self.eg_inc_lo = 0;
        if rate_hi < 12 && !self.eg_zerorate {
            self.eg_inc_lo = match sum {
                12 => 1,
                13 => (rate_lo >> 1) & 1,
                14 => rate_lo & 1,
                _ => 0,
            };
        }
        self.eg_maxrate = rate_hi == 0x0f;
        self.eg_rate_hi = rate_hi;

        self.eg_kon = (self.eg_kon << 1) | self.kon[ch];
        self.eg_off = (self.eg_off << 1) | u8::from((self.eg_level[cycles] >> 2) == 0x1f);
        let r = self.rhythm;
        self.eg_kon |= match self.rm_select {
            RM_BD0 | RM_BD1 => (r >> 4) & 1,
            RM_SD => (r >> 3) & 1,
            RM_TOM => (r >> 2) & 1,
            RM_TC => (r >> 1) & 1,
            RM_HH => r & 1,
            _ => 0,
        };

        // The rate for the slot entering the generator.
        self.eg_dokon <<= 1;
        let mut state_rate = self.eg_state[cycles];
        if state_rate == EG_RELEASE && self.eg_kon & 1 != 0 && self.eg_off & 1 != 0 {
            state_rate = EG_ATTACK;
            self.eg_dokon |= 1;
        }
        let son = self.son[ch] != 0;
        let mut rate = match state_rate {
            EG_ATTACK => self.c_adrr[0],
            EG_DECAY => self.c_adrr[1],
            EG_SUSTAIN => {
                if self.c_et == 0 {
                    self.c_adrr[2]
                } else {
                    0
                }
            }
            _ => {
                if son {
                    5
                } else {
                    self.c_adrr[2]
                }
            }
        };
        let kon = self.eg_kon & 1 != 0;
        if !kon && !mcsel && self.rm_select != RM_TOM && self.rm_select != RM_HH {
            rate = 0;
        }
        if kon && self.eg_state[cycles] == EG_RELEASE && self.eg_off & 1 == 0 {
            rate = 12;
        }
        if !kon && !son && mcsel && self.c_et == 0 {
            rate = 7;
        }
        self.eg_zerorate = rate == 0;
        let mut ksr = self.c_ksr_freq;
        if self.c_ksr == 0 {
            ksr >>= 2;
        }
        self.eg_rate = (rate << 2) + ksr;
        if self.eg_rate & 0x40 != 0 {
            self.eg_rate = 0x3c | (ksr & 3);
        }
        self.eg_sl = self.c_sl;
    }

    fn channel(&mut self) {
        let cycles = self.cycles;
        let ch_out = self.ch_out;
        let ismod = (cycles / 3) & 1 != 0;
        let mute_m = ismod || (self.rhythm_late() && (cycles + 15) % 18 >= 12);
        if self.variant == Variant::Vrc7 {
            let mut out = ch_out;
            if out >= 0 {
                out += 1;
            }
            self.output_m = if mute_m { 0 } else { out };
            self.output_r = 0;
            return;
        }
        // Which slots carry a rhythm voice to the rhythm pin is Nuked-OPLL's
        // best reading of the die; it marks this as possibly incorrect.
        let mute_r = !(self.rhythm_late() && matches!(cycles, 16 | 17 | 0..=5 | 9 | 10));
        let mut out = ch_out;
        let mut sign = ch_out >> 8;
        if out >= 0 {
            out += 1;
            sign += 1;
        }
        self.output_m = if mute_m { sign } else { out };
        self.output_r = if mute_r { sign } else { out };
    }

    fn operator(&mut self) {
        let cycles = self.cycles as usize;
        let ismod1 = if self.rhythm_now() && (cycles == 15 || cycles == 16) {
            false
        } else {
            ((cycles + 1) / 3) & 1 != 0
        };
        let ismod2 = if self.rhythm_late() && (cycles == 13 || cycles == 14) {
            false
        } else {
            ((cycles + 3) / 3) & 1 != 0
        };
        let ismod3 = if self.rhythm_late() && (cycles == 16 || cycles == 17) {
            false
        } else {
            (cycles / 3) & 1 != 0
        };

        let mut op_mod: u32 = 0;
        if ismod3 {
            op_mod |= (i32::from(self.op_mod) << 1) as u32;
        }
        if ismod2 && self.c_fb != 0 {
            op_mod |= (i32::from(self.op_fbsum) >> (7 - self.c_fb)) as u32;
        }

        let mut exp_shift = self.op_exp_s;
        let rectified = if ismod1 {
            self.c_dm & 4 != 0
        } else {
            self.c_dc & 4 != 0
        };
        if self.eg_silent || (self.op_neg & 2 != 0 && rectified) {
            exp_shift |= 12;
        }
        let mut output = (u32::from(self.op_exp_m) >> exp_shift) as i16;
        if !self.eg_silent && self.op_neg & 2 != 0 {
            output = !output;
        }

        let level = (u32::from(self.op_logsin) + (u32::from(self.eg_out) << 4)).min(4095);
        self.op_exp_m = EXP[(level & 0xff) as usize];
        self.op_exp_s = (level >> 8) as u16;

        let mut phase = op_mod.wrapping_add(u32::from(self.pg_out)) & 0x3ff;
        if phase & 0x100 != 0 {
            phase ^= 0xff;
        }
        self.op_logsin = LOGSIN[(phase & 0xff) as usize];
        self.op_neg = (self.op_neg << 1) | (phase >> 9) as u8;
        self.op_fbsum = ((i32::from(self.op_fb1[(cycles + 3) % 9])
            + i32::from(self.op_fb2[(cycles + 3) % 9]))
            >> 1) as i16;

        if ismod1 {
            self.op_fb2[cycles % 9] = self.op_fb1[cycles % 9];
            self.op_fb1[cycles % 9] = output;
        }
        self.op_mod = output & 0x1ff;

        let mut routput: i16 = 0;
        if self.variant != Variant::Vrc7 {
            routput = match cycles {
                2 => self.ch_out_hh,
                3 => self.ch_out_tm,
                4 => self.ch_out_bd,
                8 => self.ch_out_sd,
                9 => self.ch_out_tc,
                _ => 0,
            };
            match cycles {
                15 => self.ch_out_hh = output >> 3,
                16 => self.ch_out_tm = output >> 3,
                17 => self.ch_out_bd = output >> 3,
                0 => self.ch_out_sd = output >> 3,
                1 => self.ch_out_tc = output >> 3,
                _ => {}
            }
        }
        if !self.rhythm_now() {
            routput = 0;
        }
        self.ch_out = if ismod1 { routput } else { output >> 3 };
    }

    fn do_rhythm(&mut self) {
        let mut nbit = (self.rm_noise ^ (self.rm_noise >> 14)) & 1;
        nbit |= u32::from(self.rm_noise == 0) | u32::from((self.testmode >> 1) & 1);
        self.rm_noise = (nbit << 22) | (self.rm_noise >> 1);
    }

    fn do_lfo(&mut self) {
        let cycles = self.cycles;
        if cycles == 17 {
            let mut vib_step = (((self.lfo_counter & 0x3ff) + 1) >> 10) as u8;
            self.lfo_am_step = (((self.lfo_counter & 0x3f) + 1) >> 6) as u8;
            vib_step |= (self.testmode >> 3) & 1;
            self.lfo_vib_counter = (self.lfo_vib_counter + vib_step) & 7;
            self.lfo_counter = self.lfo_counter.wrapping_add(1);
        }

        let mut am_inc = 0;
        if (self.lfo_am_step != 0 || self.testmode & 8 != 0) && cycles < 9 {
            am_inc = self.lfo_am_dir | u8::from(cycles == 0);
        }
        if cycles >= 9 {
            self.lfo_am_car = 0;
        }
        if cycles == 0 {
            if self.lfo_am_dir != 0 && self.lfo_am_counter & 0x7f == 0 {
                self.lfo_am_dir = 0;
            } else if self.lfo_am_dir == 0 && self.lfo_am_counter & 0x69 == 0x69 {
                self.lfo_am_dir = 1;
            }
        }
        let mut am_bit = (self.lfo_am_counter & 1) as u8;
        am_bit += am_inc + self.lfo_am_car;
        self.lfo_am_car = am_bit >> 1;
        am_bit &= 1;
        self.lfo_am_counter = (u16::from(am_bit) << 8) | (self.lfo_am_counter >> 1);

        if self.testmode & 2 != 0 {
            self.lfo_vib_counter = 0;
            self.lfo_counter = 0;
            self.lfo_am_dir = 0;
            self.lfo_am_counter &= 0xff;
        }
    }
}

#[cfg(test)]
mod tests;
