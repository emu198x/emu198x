//! The Sony S-DSP, the sound generator in the Super Nintendo's sound module.
//!
//! Eight voices play BRR-compressed samples from the module's 64 KB of RAM,
//! each with Gaussian interpolation, an ADSR or GAIN envelope, optional
//! noise and pitch modulation, and stereo volume. An echo unit with an
//! eight-tap FIR filter keeps its delay line in the same RAM. The output is
//! 32 kHz stereo.
//!
//! # The clock
//!
//! The DSP does one sample's work in 32 steps, and one step takes as long as
//! one SPC700 cycle (24 cycles of the module's 24.576 MHz crystal), so the
//! host calls [`SDsp::tick`] once per SPC700 cycle. Which voice reads which
//! register on which step is fixed (the schedule is in [`SDsp::tick`]), so a
//! register written between steps lands exactly when hardware would see it.
//!
//! # The RAM
//!
//! The DSP owns the 64 KB audio RAM. On the board the DSP sits between the
//! RAM and the SPC700 and interleaves the CPU's accesses with its own, so the
//! RAM belongs to the DSP's side. The host routes the SPC700's bus cycles to
//! [`SDsp::ram`] and [`SDsp::ram_mut`] between ticks, which keeps the CPU's
//! bus pin-level and gives the DSP the same view of RAM the CPU's last write
//! left. The CPU-side traps (the IPL ROM, `$F0`–`$FF`) are the host's.
//!
//! # Registers
//!
//! The SPC700 reaches the 128 registers through `$F2` (address) and `$F3`
//! (data). The host forwards them to [`SDsp::read`] and [`SDsp::write`];
//! on hardware a write to an address with bit 7 set does nothing, and the
//! host is the one that sees that bit.
//!
//! # Provenance
//!
//! The schedule and every step's arithmetic follow ares's DSP (ISC licence,
//! © 2004-2025 ares team, Near et al), re-expressed in Rust, integer widths
//! included. A differential harness in `tests/oracle/` runs ares's own source
//! against scripted cases and the golden files pin this crate to it.

mod gaussian;

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use gaussian::GAUSSIAN;

/// Steps per output sample.
pub const STEPS_PER_SAMPLE: u8 = 32;

/// The output sample rate, in Hz.
pub const SAMPLE_RATE: u32 = 32_000;

const RAM_SIZE: usize = 0x1_0000;

/// Envelope phases.
const RELEASE: u8 = 0;
const ATTACK: u8 = 1;
const DECAY: u8 = 2;
const SUSTAIN: u8 = 3;

/// Samples per envelope or noise event at each rate, against a counter that
/// runs down from 30720.
const COUNTER_RATE: [u16; 32] = [
    0, 2048, 1536, 1280, 1024, 768, 640, 512, 384, 320, 256, 192, 160, 128, 96, 80, 64, 48, 40, 32,
    24, 20, 16, 12, 10, 8, 6, 5, 4, 3, 2, 1,
];

/// The counters are not all aligned at zero.
const COUNTER_OFFSET: [u16; 32] = [
    0, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040,
    536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 0, 0,
];

fn sclamp16(x: i32) -> i32 {
    x.clamp(-0x8000, 0x7FFF)
}

/// Store into a 17-bit signed latch.
fn i17(x: i32) -> i32 {
    (x << 15) >> 15
}

fn i16t(x: i32) -> i32 {
    i32::from(x as i16)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Voice {
    /// Register base: `$x0` for voice x.
    index: u8,
    volume: [i8; 2],
    /// 14 bits.
    pitch: u16,
    source: u8,
    adsr0: u8,
    adsr1: u8,
    gain: u8,
    envx: u8,
    keyoff: bool,
    modulate: bool,
    noise: bool,
    echo: bool,
    /// Decoded samples, a ring of twelve.
    buffer: [i16; 12],
    buffer_offset: u8,
    /// Position in the sample, 0x1000 per decoded sample.
    gaussian_offset: u16,
    brr_address: u16,
    /// Byte within the BRR block, 1 to 9.
    brr_offset: u8,
    keyon_delay: u8,
    envelope_mode: u8,
    /// 0 to 2047.
    envelope: u16,
    /// The unclamped envelope, which GAIN mode 7 reads.
    envelope_raw: i32,
    keylatch: bool,
    keyon_latched: bool,
    keyoff_latched: bool,
    modulate_latched: bool,
    noise_latched: bool,
    echo_latched: bool,
    end: bool,
    looped: bool,
}

impl Voice {
    fn new(n: u8) -> Self {
        Self {
            index: n << 4,
            volume: [0; 2],
            pitch: 0,
            source: 0,
            adsr0: 0,
            adsr1: 0,
            gain: 0,
            envx: 0,
            keyoff: false,
            modulate: false,
            noise: false,
            echo: false,
            buffer: [0; 12],
            buffer_offset: 0,
            gaussian_offset: 0,
            brr_address: 0,
            brr_offset: 1,
            keyon_delay: 0,
            envelope_mode: RELEASE,
            envelope: 0,
            envelope_raw: 0,
            keylatch: false,
            keyon_latched: false,
            keyoff_latched: false,
            modulate_latched: false,
            noise_latched: false,
            echo_latched: false,
            end: false,
            looped: false,
        }
    }
}

/// The S-DSP and its 64 KB of audio RAM.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SDsp {
    ram: Vec<u8>,
    #[serde(with = "BigArray")]
    registers: [u8; 128],
    /// Step within the sample, 0 to 31.
    step: u8,

    // Global counter, 15 bits.
    counter: u16,
    /// Toggles each sample; KON and KOFF are sampled every other one.
    every_other: bool,

    // Main volume.
    soft_reset: bool,
    mute: bool,
    main_volume: [i8; 2],
    main_output: [i32; 2],

    // Echo.
    echo_feedback: i8,
    echo_volume: [i8; 2],
    fir: [i8; 8],
    echo_history: [[i16; 8]; 2],
    echo_page: u8,
    echo_delay: u8,
    echo_readonly: bool,
    echo_input: [i32; 2],
    echo_output: [i32; 2],
    echo_page_latched: u8,
    echo_readonly_latched: bool,
    echo_address: u16,
    echo_offset: u16,
    echo_length: u16,
    echo_history_offset: u8,

    // Noise.
    noise_frequency: u8,
    noise_lfsr: u16,

    // BRR.
    brr_bank: u8,
    brr_bank_latched: u8,
    brr_source: u8,
    brr_address: u16,
    brr_next_address: u16,
    brr_header: u8,
    brr_byte: u8,

    // Latches shared by the voices as the schedule passes through them.
    latch_adsr0: u8,
    latch_envx: u8,
    latch_outx: u8,
    latch_pitch: u16,
    latch_output: i16,

    voices: [Voice; 8],

    samples: Vec<(i16, i16)>,
}

impl Default for SDsp {
    fn default() -> Self {
        Self::new()
    }
}

impl SDsp {
    /// A DSP at power-on, with its RAM and registers cleared.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ram: vec![0; RAM_SIZE],
            registers: [0; 128],
            step: 0,
            counter: 0,
            every_other: true,
            soft_reset: true,
            mute: true,
            main_volume: [0; 2],
            main_output: [0; 2],
            echo_feedback: 0,
            echo_volume: [0; 2],
            fir: [0; 8],
            echo_history: [[0; 8]; 2],
            echo_page: 0,
            echo_delay: 0,
            echo_readonly: true,
            echo_input: [0; 2],
            echo_output: [0; 2],
            echo_page_latched: 0,
            echo_readonly_latched: false,
            echo_address: 0,
            echo_offset: 0,
            echo_length: 0,
            echo_history_offset: 0,
            noise_frequency: 0,
            noise_lfsr: 0x4000,
            brr_bank: 0,
            brr_bank_latched: 0,
            brr_source: 0,
            brr_address: 0,
            brr_next_address: 0,
            brr_header: 0,
            brr_byte: 0,
            latch_adsr0: 0,
            latch_envx: 0,
            latch_outx: 0,
            latch_pitch: 0,
            latch_output: 0,
            voices: std::array::from_fn(|n| Voice::new(n as u8)),
            samples: Vec::new(),
        }
    }

    /// The audio RAM.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// The audio RAM, for the host to route SPC700 writes into.
    pub fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }

    /// The step the next tick runs, 0 to 31.
    #[must_use]
    pub fn step(&self) -> u8 {
        self.step
    }

    /// Read a register, as `$F3` does.
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        self.registers[usize::from(addr & 0x7F)]
    }

    /// Write a register, as `$F3` does.
    pub fn write(&mut self, addr: u8, data: u8) {
        let addr = addr & 0x7F;
        self.registers[usize::from(addr)] = data;
        let bits = |n: usize| data >> n & 1 != 0;
        match addr {
            0x0C => self.main_volume[0] = data as i8,
            0x1C => self.main_volume[1] = data as i8,
            0x2C => self.echo_volume[0] = data as i8,
            0x3C => self.echo_volume[1] = data as i8,
            0x4C => {
                for (n, v) in self.voices.iter_mut().enumerate() {
                    v.keylatch = bits(n);
                }
            }
            0x5C => {
                for (n, v) in self.voices.iter_mut().enumerate() {
                    v.keyoff = bits(n);
                }
            }
            0x6C => {
                self.noise_frequency = data & 0x1F;
                self.echo_readonly = bits(5);
                self.mute = bits(6);
                self.soft_reset = bits(7);
            }
            0x7C => {
                for v in &mut self.voices {
                    v.end = false;
                }
                // Writing ENDX clears it, whatever was written.
                self.registers[0x7C] = 0;
            }
            0x0D => self.echo_feedback = data as i8,
            0x2D => {
                for (n, v) in self.voices.iter_mut().enumerate() {
                    // Voice 0 has no previous voice to be modulated by.
                    v.modulate = n != 0 && bits(n);
                }
            }
            0x3D => {
                for (n, v) in self.voices.iter_mut().enumerate() {
                    v.noise = bits(n);
                }
            }
            0x4D => {
                for (n, v) in self.voices.iter_mut().enumerate() {
                    v.echo = bits(n);
                }
            }
            0x5D => self.brr_bank = data,
            0x6D => self.echo_page = data,
            0x7D => self.echo_delay = data & 0x0F,
            _ => {}
        }
        let v = &mut self.voices[usize::from(addr >> 4)];
        match addr & 0x0F {
            0x00 => v.volume[0] = data as i8,
            0x01 => v.volume[1] = data as i8,
            0x02 => v.pitch = v.pitch & 0x3F00 | u16::from(data),
            0x03 => v.pitch = v.pitch & 0x00FF | u16::from(data & 0x3F) << 8,
            0x04 => v.source = data,
            0x05 => v.adsr0 = data,
            0x06 => v.adsr1 = data,
            0x07 => v.gain = data,
            0x08 => self.latch_envx = data,
            0x09 => self.latch_outx = data,
            0x0F => self.fir[usize::from(addr >> 4)] = data as i8,
            _ => {}
        }
    }

    /// Samples produced since the last call, left and right.
    pub fn take_samples(&mut self) -> Vec<(i16, i16)> {
        std::mem::take(&mut self.samples)
    }

    /// Run one step: one SPC700 cycle's worth of DSP work.
    pub fn tick(&mut self) {
        match self.step {
            0 => {
                self.voice5(0);
                self.voice2(1);
            }
            1 => {
                self.voice6(0);
                self.voice3(1);
            }
            2 => {
                self.voice7(0);
                self.voice4(1);
                self.voice1(3);
            }
            3 => {
                self.voice8(0);
                self.voice5(1);
                self.voice2(2);
            }
            4 => {
                self.voice9(0);
                self.voice6(1);
                self.voice3(2);
            }
            5 => {
                self.voice7(1);
                self.voice4(2);
                self.voice1(4);
            }
            6 => {
                self.voice8(1);
                self.voice5(2);
                self.voice2(3);
            }
            7 => {
                self.voice9(1);
                self.voice6(2);
                self.voice3(3);
            }
            8 => {
                self.voice7(2);
                self.voice4(3);
                self.voice1(5);
            }
            9 => {
                self.voice8(2);
                self.voice5(3);
                self.voice2(4);
            }
            10 => {
                self.voice9(2);
                self.voice6(3);
                self.voice3(4);
            }
            11 => {
                self.voice7(3);
                self.voice4(4);
                self.voice1(6);
            }
            12 => {
                self.voice8(3);
                self.voice5(4);
                self.voice2(5);
            }
            13 => {
                self.voice9(3);
                self.voice6(4);
                self.voice3(5);
            }
            14 => {
                self.voice7(4);
                self.voice4(5);
                self.voice1(7);
            }
            15 => {
                self.voice8(4);
                self.voice5(5);
                self.voice2(6);
            }
            16 => {
                self.voice9(4);
                self.voice6(5);
                self.voice3(6);
            }
            17 => {
                self.voice1(0);
                self.voice7(5);
                self.voice4(6);
            }
            18 => {
                self.voice8(5);
                self.voice5(6);
                self.voice2(7);
            }
            19 => {
                self.voice9(5);
                self.voice6(6);
                self.voice3(7);
            }
            20 => {
                self.voice1(1);
                self.voice7(6);
                self.voice4(7);
            }
            21 => {
                self.voice8(6);
                self.voice5(7);
                self.voice2(0);
            }
            22 => {
                self.voice3a(0);
                self.voice9(6);
                self.voice6(7);
                self.echo22();
            }
            23 => {
                self.voice7(7);
                self.echo23();
            }
            24 => {
                self.voice8(7);
                self.echo24();
            }
            25 => {
                self.voice3b(0);
                self.voice9(7);
                self.echo25();
            }
            26 => self.echo26(),
            27 => {
                self.misc27();
                self.echo27();
            }
            28 => {
                self.misc28();
                self.echo28();
            }
            29 => {
                self.misc29();
                self.echo29();
            }
            30 => {
                self.misc30();
                self.voice3c(0);
                self.echo30();
            }
            _ => {
                self.voice4(0);
                self.voice1(2);
            }
        }
        self.step = (self.step + 1) % STEPS_PER_SAMPLE;
    }

    fn ram_at(&self, addr: u16) -> u8 {
        self.ram[usize::from(addr)]
    }

    // ── Counter ────────────────────────────────────────────────────────

    fn counter_tick(&mut self) {
        if self.counter == 0 {
            self.counter = 2048 * 5 * 3;
        }
        self.counter -= 1;
    }

    fn counter_poll(&self, rate: usize) -> bool {
        if rate == 0 {
            return false;
        }
        (self.counter + COUNTER_OFFSET[rate]).is_multiple_of(COUNTER_RATE[rate])
    }

    // ── Gaussian interpolation ─────────────────────────────────────────

    fn interpolate(&self, n: usize) -> i32 {
        let v = &self.voices[n];
        let offset = usize::from((v.gaussian_offset >> 4) as u8);
        let forward = 255 - offset;
        let reverse = offset;
        let mut at = (usize::from(v.buffer_offset) + usize::from(v.gaussian_offset >> 12)) % 12;
        let next = |at: &mut usize| {
            let s = i32::from(v.buffer[*at]);
            *at = (*at + 1) % 12;
            s
        };
        let g = |i: usize| i32::from(GAUSSIAN[i]);
        let mut out = (g(forward) * next(&mut at)) >> 11;
        out += (g(forward + 256) * next(&mut at)) >> 11;
        out += (g(reverse + 256) * next(&mut at)) >> 11;
        out = i16t(out);
        out += (g(reverse) * next(&mut at)) >> 11;
        sclamp16(out) & !1
    }

    // ── Envelope ───────────────────────────────────────────────────────

    fn envelope_run(&mut self, n: usize) {
        let adsr0 = self.latch_adsr0;
        let v = &self.voices[n];
        let mut envelope = i32::from(v.envelope);
        if v.envelope_mode == RELEASE {
            envelope = (envelope - 8).max(0);
            self.voices[n].envelope = envelope as u16;
            return;
        }

        let rate;
        let mut data = i32::from(v.adsr1);
        if adsr0 & 0x80 != 0 {
            if v.envelope_mode >= DECAY {
                envelope -= 1;
                envelope -= envelope >> 8;
                rate = if v.envelope_mode == DECAY {
                    i32::from(adsr0 >> 4 & 7) * 2 + 16
                } else {
                    data & 0x1F
                };
            } else {
                rate = i32::from(adsr0 & 0x0F) * 2 + 1;
                envelope += if rate < 31 { 0x20 } else { 0x400 };
            }
        } else {
            data = i32::from(v.gain);
            let mode = data >> 5;
            if mode < 4 {
                envelope = data << 4;
                rate = 31;
            } else {
                rate = data & 0x1F;
                if mode == 4 {
                    envelope -= 0x20;
                } else if mode < 6 {
                    envelope -= 1;
                    envelope -= envelope >> 8;
                } else {
                    envelope += 0x20;
                    if mode > 6 && v.envelope_raw as u32 >= 0x600 {
                        envelope += 0x8 - 0x20;
                    }
                }
            }
        }

        let v = &mut self.voices[n];
        if envelope >> 8 == data >> 5 && v.envelope_mode == DECAY {
            v.envelope_mode = SUSTAIN;
        }
        v.envelope_raw = envelope;
        // Linear decrease below zero also lands here, through the cast.
        if envelope as u32 > 0x7FF {
            envelope = if envelope < 0 { 0 } else { 0x7FF };
            if v.envelope_mode == ATTACK {
                v.envelope_mode = DECAY;
            }
        }
        if self.counter_poll(rate as usize) {
            self.voices[n].envelope = (envelope & 0x7FF) as u16;
        }
    }

    // ── BRR ────────────────────────────────────────────────────────────

    fn brr_decode(&mut self, n: usize) {
        let addr = self.voices[n]
            .brr_address
            .wrapping_add(u16::from(self.voices[n].brr_offset) + 1);
        let mut nybbles = i32::from(self.brr_byte) << 8 | i32::from(self.ram_at(addr));
        let filter = self.brr_header >> 2 & 3;
        let scale = i32::from(self.brr_header >> 4);
        let v = &mut self.voices[n];
        for _ in 0..4 {
            let mut s = i32::from(nybbles as i16 >> 12);
            nybbles <<= 4;
            if scale <= 12 {
                s <<= scale;
                s >>= 1;
            } else {
                s &= !0x7FF;
            }
            let back = |k: usize| (usize::from(v.buffer_offset) + 12 - k) % 12;
            let p1 = i32::from(v.buffer[back(1)]);
            let p2 = i32::from(v.buffer[back(2)]) >> 1;
            match filter {
                1 => {
                    s += p1 >> 1;
                    s += (-p1) >> 5;
                }
                2 => {
                    s += p1;
                    s -= p2;
                    s += p2 >> 4;
                    s += (p1 * -3) >> 6;
                }
                3 => {
                    s += p1;
                    s -= p2;
                    s += (p1 * -13) >> 7;
                    s += (p2 * 3) >> 4;
                }
                _ => {}
            }
            s = sclamp16(s);
            s = i16t(s << 1);
            v.buffer[usize::from(v.buffer_offset)] = s as i16;
            v.buffer_offset = (v.buffer_offset + 1) % 12;
        }
    }

    // ── Voice steps ────────────────────────────────────────────────────

    fn voice_output(&mut self, n: usize, channel: usize) {
        let v = &self.voices[n];
        let amp = (i32::from(self.latch_output) * i32::from(v.volume[channel])) >> 7;
        self.main_output[channel] = sclamp16(i17(self.main_output[channel] + amp));
        if v.echo_latched {
            self.echo_output[channel] = sclamp16(i17(self.echo_output[channel] + amp));
        }
    }

    fn voice1(&mut self, n: usize) {
        self.brr_address =
            (u16::from(self.brr_bank_latched) << 8).wrapping_add(u16::from(self.brr_source) << 2);
        self.brr_source = self.voices[n].source;
    }

    fn voice2(&mut self, n: usize) {
        let mut addr = self.brr_address;
        if self.voices[n].keyon_delay == 0 {
            addr = addr.wrapping_add(2);
        }
        let lo = self.ram_at(addr);
        let hi = self.ram_at(addr.wrapping_add(1));
        self.brr_next_address = u16::from(hi) << 8 | u16::from(lo);
        self.latch_adsr0 = self.voices[n].adsr0;
        self.latch_pitch = self.voices[n].pitch & 0xFF;
    }

    fn voice3(&mut self, n: usize) {
        self.voice3a(n);
        self.voice3b(n);
        self.voice3c(n);
    }

    fn voice3a(&mut self, n: usize) {
        self.latch_pitch |= self.voices[n].pitch & !0xFF;
    }

    fn voice3b(&mut self, n: usize) {
        let v = &self.voices[n];
        self.brr_byte = self.ram_at(v.brr_address.wrapping_add(u16::from(v.brr_offset)));
        self.brr_header = self.ram_at(v.brr_address);
    }

    fn voice3c(&mut self, n: usize) {
        if self.voices[n].modulate_latched {
            let pitch = i32::from(self.latch_pitch);
            let delta = ((i32::from(self.latch_output) >> 5) * pitch) >> 10;
            self.latch_pitch = ((pitch + delta) & 0x7FFF) as u16;
        }

        if self.voices[n].keyon_delay != 0 {
            let next = self.brr_next_address;
            let v = &mut self.voices[n];
            if v.keyon_delay == 5 {
                v.brr_address = next;
                v.brr_offset = 1;
                v.buffer_offset = 0;
                // The header is ignored on this sample.
                self.brr_header = 0;
            }
            // The envelope never runs during KON.
            v.envelope = 0;
            v.envelope_raw = 0;
            // BRR decoding waits for the last three samples.
            v.gaussian_offset = 0;
            v.keyon_delay -= 1;
            if v.keyon_delay & 3 != 0 {
                v.gaussian_offset = 0x4000;
            }
            // Nor is pitch added during KON.
            self.latch_pitch = 0;
        }

        let mut output = self.interpolate(n);
        if self.voices[n].noise_latched {
            output = i16t(i32::from(self.noise_lfsr) << 1);
        }

        let envelope = i32::from(self.voices[n].envelope);
        self.latch_output = (((output * envelope) >> 11) & !1) as i16;
        self.voices[n].envx = (envelope >> 4) as u8;

        // Immediate silence at the end of a sample, or on a soft reset.
        if self.soft_reset || self.brr_header & 3 == 1 {
            let v = &mut self.voices[n];
            v.envelope_mode = RELEASE;
            v.envelope = 0;
        }

        if self.every_other {
            let v = &mut self.voices[n];
            if v.keyoff_latched {
                v.envelope_mode = RELEASE;
            }
            if v.keyon_latched {
                v.keyon_delay = 5;
                v.envelope_mode = ATTACK;
            }
        }

        if self.voices[n].keyon_delay == 0 {
            self.envelope_run(n);
        }
    }

    fn voice4(&mut self, n: usize) {
        self.voices[n].looped = false;
        if self.voices[n].gaussian_offset >= 0x4000 {
            self.brr_decode(n);
            let header = self.brr_header;
            let next = self.brr_next_address;
            let v = &mut self.voices[n];
            v.brr_offset += 2;
            if v.brr_offset >= 9 {
                v.brr_address = v.brr_address.wrapping_add(9);
                if header & 1 != 0 {
                    v.brr_address = next;
                    v.looped = true;
                }
                v.brr_offset = 1;
            }
        }
        let pitch = self.latch_pitch;
        let v = &mut self.voices[n];
        // Pitch modulation can push the position too far ahead; it stops at
        // 0x7FFF.
        v.gaussian_offset = ((v.gaussian_offset & 0x3FFF) + pitch).min(0x7FFF);
        self.voice_output(n, 0);
    }

    fn voice5(&mut self, n: usize) {
        self.voice_output(n, 1);
        let v = &mut self.voices[n];
        v.end |= v.looped;
        if v.keyon_delay == 5 {
            v.end = false;
        }
    }

    fn voice6(&mut self, _n: usize) {
        self.latch_outx = (self.latch_output >> 8) as u8;
    }

    fn voice7(&mut self, n: usize) {
        let endx = self
            .voices
            .iter()
            .enumerate()
            .fold(0u8, |acc, (i, v)| acc | u8::from(v.end) << i);
        self.registers[0x7C] = endx;
        self.latch_envx = self.voices[n].envx;
    }

    fn voice8(&mut self, n: usize) {
        self.registers[usize::from(self.voices[n].index | 0x09)] = self.latch_outx;
    }

    fn voice9(&mut self, n: usize) {
        self.registers[usize::from(self.voices[n].index | 0x08)] = self.latch_envx;
    }

    // ── Misc ───────────────────────────────────────────────────────────

    fn misc27(&mut self) {
        for v in &mut self.voices {
            v.modulate_latched = v.modulate;
        }
    }

    fn misc28(&mut self) {
        for v in &mut self.voices {
            v.noise_latched = v.noise;
            v.echo_latched = v.echo;
        }
        self.brr_bank_latched = self.brr_bank;
    }

    fn misc29(&mut self) {
        self.every_other = !self.every_other;
        if self.every_other {
            // KON clears 63 clocks after it was last read.
            for v in &mut self.voices {
                v.keylatch &= !v.keyon_latched;
            }
        }
    }

    fn misc30(&mut self) {
        if self.every_other {
            for v in &mut self.voices {
                v.keyon_latched = v.keylatch;
                v.keyoff_latched = v.keyoff;
            }
        }
        self.counter_tick();
        if self.counter_poll(usize::from(self.noise_frequency)) {
            let lfsr = i32::from(self.noise_lfsr);
            let feedback = lfsr << 13 ^ lfsr << 14;
            self.noise_lfsr = ((feedback & 0x4000) | lfsr >> 1) as u16 & 0x7FFF;
        }
    }

    // ── Echo ───────────────────────────────────────────────────────────

    fn fir_tap(&self, channel: usize, index: usize) -> i32 {
        let at = (usize::from(self.echo_history_offset) + index + 1) & 7;
        (i32::from(self.echo_history[channel][at]) * i32::from(self.fir[index])) >> 6
    }

    fn echo_mix(&self, channel: usize) -> i32 {
        let main = i16t((self.main_output[channel] * i32::from(self.main_volume[channel])) >> 7);
        let echo = i16t((self.echo_input[channel] * i32::from(self.echo_volume[channel])) >> 7);
        sclamp16(main + echo)
    }

    fn echo_read(&mut self, channel: usize) {
        let addr = self.echo_address.wrapping_add(channel as u16 * 2);
        let lo = self.ram_at(addr);
        let hi = self.ram_at(addr.wrapping_add(1));
        let s = i32::from(i16::from_le_bytes([lo, hi]));
        self.echo_history[channel][usize::from(self.echo_history_offset)] = (s >> 1) as i16;
    }

    fn echo_write(&mut self, channel: usize) {
        if !self.echo_readonly_latched {
            let addr = self.echo_address.wrapping_add(channel as u16 * 2);
            let bytes = self.echo_output[channel].to_le_bytes();
            self.ram[usize::from(addr)] = bytes[0];
            self.ram[usize::from(addr.wrapping_add(1))] = bytes[1];
        }
        self.echo_output[channel] = 0;
    }

    fn echo22(&mut self) {
        self.echo_history_offset = (self.echo_history_offset + 1) & 7;
        self.echo_address = (u16::from(self.echo_page_latched) << 8).wrapping_add(self.echo_offset);
        self.echo_read(0);
        self.echo_input = [i17(self.fir_tap(0, 0)), i17(self.fir_tap(1, 0))];
    }

    fn echo23(&mut self) {
        for ch in 0..2 {
            let sum = self.fir_tap(ch, 1) + self.fir_tap(ch, 2);
            self.echo_input[ch] = i17(self.echo_input[ch] + sum);
        }
        self.echo_read(1);
    }

    fn echo24(&mut self) {
        for ch in 0..2 {
            let sum = self.fir_tap(ch, 3) + self.fir_tap(ch, 4) + self.fir_tap(ch, 5);
            self.echo_input[ch] = i17(self.echo_input[ch] + sum);
        }
    }

    fn echo25(&mut self) {
        for ch in 0..2 {
            let mut s = self.echo_input[ch] + self.fir_tap(ch, 6);
            s = i16t(s);
            s += i16t(self.fir_tap(ch, 7));
            self.echo_input[ch] = sclamp16(s) & !1;
        }
    }

    fn echo26(&mut self) {
        // The left output is computed now and sent with the right one next
        // step.
        self.main_output[0] = self.echo_mix(0);
        for ch in 0..2 {
            let feedback = i16t((self.echo_input[ch] * i32::from(self.echo_feedback)) >> 7);
            self.echo_output[ch] = sclamp16(self.echo_output[ch] + feedback) & !1;
        }
    }

    fn echo27(&mut self) {
        let mut left = self.main_output[0];
        let mut right = self.echo_mix(1);
        self.main_output = [0, 0];
        if self.mute {
            left = 0;
            right = 0;
        }
        self.samples.push((left as i16, right as i16));
    }

    fn echo28(&mut self) {
        self.echo_readonly_latched = self.echo_readonly;
    }

    fn echo29(&mut self) {
        self.echo_page_latched = self.echo_page;
        if self.echo_offset == 0 {
            self.echo_length = u16::from(self.echo_delay) << 11;
        }
        self.echo_offset = self.echo_offset.wrapping_add(4);
        if self.echo_offset >= self.echo_length {
            self.echo_offset = 0;
        }
        self.echo_write(0);
        self.echo_readonly_latched = self.echo_readonly;
    }

    fn echo30(&mut self) {
        self.echo_write(1);
    }
}

#[cfg(test)]
mod tests;
