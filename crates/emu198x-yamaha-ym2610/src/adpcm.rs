//! The YM2610's two ADPCM units.
//!
//! ADPCM-A plays six fixed-rate channels of 4-bit Yamaha ADPCM; ADPCM-B
//! plays one channel of 4-bit delta-modulated ADPCM at a programmable rate.
//! Both read their samples from external ROM, which on the Neo Geo is the
//! cartridge's V-ROM.
//!
//! Both engines are re-expressed from ymfm by Aaron Giles (BSD-3-Clause),
//! `src/ymfm_adpcm.cpp` at commit `81aec25c`. The arithmetic is ymfm's to the
//! bit, which `tests/ymfm_oracle.rs` checks against ymfm itself. This file
//! keeps only what the YM2610 wires up: both units address ROM in 256-byte
//! units, and ADPCM-B always plays from external memory and never records.
//!
//! The units read ROM through a borrowed slice passed to each clock rather
//! than raising a request on a pin. The V-ROM bus belongs to the YM2610
//! alone: no other chip drives it and nothing can stall it, so a request
//! would always be answered at once with the same byte the slice gives.

use serde::{Deserialize, Serialize};

/// Both units address ROM in 256-byte units on the YM2610: a start or end
/// register value is shifted left by this many bits.
pub const ADDRESS_SHIFT: u32 = 8;

/// The ADPCM-A step sizes, indexed by the step index.
const A_STEPS: [i32; 49] = [
    16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107, 118, 130,
    143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796,
    876, 963, 1060, 1166, 1282, 1411, 1552,
];

/// How each nibble's magnitude moves the ADPCM-A step index.
const A_STEP_INC: [i32; 8] = [-1, -1, -1, -1, 2, 5, 7, 9];

/// How each nibble's magnitude scales the ADPCM-B step, in 64ths.
const B_STEP_SCALE: [i32; 8] = [57, 57, 57, 57, 77, 102, 128, 153];
const B_STEP_MIN: i32 = 127;
const B_STEP_MAX: i32 = 24576;

/// ADPCM-B status: the sample reached its end address.
pub const B_STATUS_EOS: u8 = 0x01;
/// ADPCM-B status: the unit is ready for CPU data. Always set on the YM2610,
/// which has no CPU data path.
pub const B_STATUS_BRDY: u8 = 0x02;
/// ADPCM-B status: a sample is playing.
pub const B_STATUS_PLAYING: u8 = 0x04;

/// Read one byte of sample ROM. Addresses past the slice read as zero; a
/// machine that mirrors its ROM passes a slice that already does.
fn rom_byte(rom: &[u8], address: u32) -> u8 {
    rom.get(address as usize).copied().unwrap_or(0)
}

/// One ADPCM-A channel's decoder.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct AChannel {
    playing: bool,
    /// Which nibble of `byte` comes next: false for the high one.
    low_nibble: bool,
    byte: u8,
    address: u32,
    /// The 12-bit decoder output, held unsigned; bit 11 is the sign.
    accumulator: i32,
    step_index: i32,
}

/// The six-channel ADPCM-A unit: registers `$100`–`$12F`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdpcmA {
    /// Registers `$00`–`$2F` in rows of 16; serde handles no longer arrays.
    regs: [[u8; 0x10]; 3],
    channels: [AChannel; 6],
}

impl Default for AdpcmA {
    fn default() -> Self {
        Self::new()
    }
}

impl AdpcmA {
    /// The unit after reset: every channel stopped, panned to both sides at
    /// full instrument level.
    #[must_use]
    pub fn new() -> Self {
        let mut regs = [[0; 0x10]; 3];
        // ymfm sets these at reset; some Neo Geo homebrew relies on it.
        regs[0][0x08..0x0e].fill(0xdf);
        Self {
            regs,
            channels: Default::default(),
        }
    }

    /// Write `data` to register `reg` (`$00`–`$2F`, the low six bits of the
    /// chip address). Register `$00` keys channels on, or off with bit 7 set.
    pub fn write(&mut self, reg: u8, data: u8) {
        let reg = usize::from(reg & 0x3f);
        if reg >= 0x30 {
            return;
        }
        self.regs[reg >> 4][reg & 0x0f] = data;
        if reg == 0 {
            for ch in 0..6 {
                if data & (1 << ch) != 0 {
                    self.key(ch, data & 0x80 == 0);
                }
            }
        }
    }

    /// The registers as last written.
    #[must_use]
    pub fn registers(&self) -> [u8; 0x30] {
        std::array::from_fn(|index| self.reg(index))
    }

    fn reg(&self, index: usize) -> u8 {
        self.regs[index >> 4][index & 0x0f]
    }

    fn start(&self, ch: usize) -> u32 {
        u32::from(self.reg(0x10 + ch)) | (u32::from(self.reg(0x18 + ch)) << 8)
    }

    fn end(&self, ch: usize) -> u32 {
        u32::from(self.reg(0x20 + ch)) | (u32::from(self.reg(0x28 + ch)) << 8)
    }

    fn key(&mut self, ch: usize, on: bool) {
        let start = self.start(ch) << ADDRESS_SHIFT;
        let channel = &mut self.channels[ch];
        channel.playing = on;
        if on {
            *channel = AChannel {
                playing: true,
                address: start,
                ..AChannel::default()
            };
        }
    }

    /// Whether channel `ch` is playing.
    #[must_use]
    pub fn playing(&self, ch: usize) -> bool {
        self.channels.get(ch).is_some_and(|c| c.playing)
    }

    /// Decode one nibble on every playing channel, reading `rom` as each
    /// needs a new byte. Returns a bit per channel that reached its end
    /// address on this clock.
    pub fn clock(&mut self, rom: &[u8]) -> u8 {
        let mut ended = 0;
        for ch in 0..6 {
            if self.clock_channel(ch, rom) {
                ended |= 1 << ch;
            }
        }
        ended
    }

    fn clock_channel(&mut self, ch: usize, rom: &[u8]) -> bool {
        let end = (self.end(ch) + 1) << ADDRESS_SHIFT;
        let channel = &mut self.channels[ch];
        if !channel.playing {
            channel.accumulator = 0;
            return false;
        }
        let data = if channel.low_nibble {
            channel.low_nibble = false;
            channel.byte & 0x0f
        } else {
            // The end address is inclusive and only its low 20 bits are
            // compared, so a sample can run past a 1 MiB boundary.
            if (channel.address ^ end) & 0xf_ffff == 0 {
                channel.playing = false;
                channel.accumulator = 0;
                return true;
            }
            channel.byte = rom_byte(rom, channel.address);
            channel.address = channel.address.wrapping_add(1);
            channel.low_nibble = true;
            channel.byte >> 4
        };
        let magnitude = usize::from(data & 7);
        let mut delta = (2 * magnitude as i32 + 1) * A_STEPS[channel.step_index as usize] / 8;
        if data & 8 != 0 {
            delta = -delta;
        }
        channel.accumulator = (channel.accumulator + delta) & 0xfff;
        channel.step_index = (channel.step_index + A_STEP_INC[magnitude]).clamp(0, 48);
        false
    }

    /// The six channels mixed after their instrument and total levels, as
    /// `[left, right]`.
    #[must_use]
    pub fn output(&self) -> [i32; 2] {
        let total = i32::from(self.reg(0x01) & 0x3f);
        let mut out = [0; 2];
        for (ch, channel) in self.channels.iter().enumerate() {
            let control = self.reg(0x08 + ch);
            let volume = (i32::from(control & 0x1f) ^ 0x1f) + (total ^ 0x3f);
            if volume >= 63 {
                continue;
            }
            let mul = 15 - (volume & 7);
            let shift = 5 + (volume >> 3);
            // The 12-bit accumulator moved to the top of a 16-bit word, so
            // its bit 11 becomes the sign.
            let sample = i32::from((channel.accumulator << 4) as u16 as i16);
            let value = ((sample * mul) >> shift) & !3;
            if control & 0x80 != 0 {
                out[0] += value;
            }
            if control & 0x40 != 0 {
                out[1] += value;
            }
        }
        out
    }
}

/// The single-channel ADPCM-B unit: registers `$10`–`$1B`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdpcmB {
    regs: [u8; 0x10],
    status: u8,
    /// Which nibble of `byte` comes next: false for the high one.
    low_nibble: bool,
    byte: u8,
    /// The 16-bit phase between nibbles; delta-N is added each clock.
    position: u16,
    address: u32,
    accumulator: i32,
    previous: i32,
    step: i32,
}

impl Default for AdpcmB {
    fn default() -> Self {
        Self::new()
    }
}

impl AdpcmB {
    /// The unit after reset: stopped and ready.
    #[must_use]
    pub fn new() -> Self {
        let mut regs = [0; 0x10];
        // The limit register, which the YM2610 cannot write.
        regs[0x0c] = 0xff;
        regs[0x0d] = 0xff;
        Self {
            regs,
            status: B_STATUS_BRDY,
            low_nibble: false,
            byte: 0,
            position: 0,
            address: 0,
            accumulator: 0,
            previous: 0,
            step: B_STEP_MIN,
        }
    }

    /// Write `data` to register `reg` (`$0`–`$F`, the low four bits of the
    /// chip address). On the YM2610 the control register's external-memory
    /// bit is wired high and its record bit low, so the unit only plays ROM.
    pub fn write(&mut self, reg: u8, data: u8) {
        let reg = usize::from(reg & 0x0f);
        let data = if reg == 0 {
            (data | 0x20) & !0x40
        } else {
            data
        };
        self.regs[reg] = data;
        if reg == 0 {
            if self.execute() {
                self.load_start();
            } else {
                self.status &= !B_STATUS_EOS;
            }
            if data & 0x01 != 0 {
                self.reset_channel();
            }
        }
    }

    /// The registers as last written.
    #[must_use]
    pub fn registers(&self) -> &[u8; 0x10] {
        &self.regs
    }

    /// The status bits: [`B_STATUS_EOS`], [`B_STATUS_BRDY`] and
    /// [`B_STATUS_PLAYING`].
    #[must_use]
    pub fn status(&self) -> u8 {
        self.status
    }

    fn execute(&self) -> bool {
        self.regs[0] & 0x80 != 0
    }

    fn repeat(&self) -> bool {
        self.regs[0] & 0x10 != 0
    }

    fn reg16(&self, lo: usize) -> u32 {
        u32::from(self.regs[lo]) | (u32::from(self.regs[lo + 1]) << 8)
    }

    /// The last byte of the 256-byte unit the register at `lo` names.
    fn last_byte(&self, lo: usize) -> u32 {
        ((self.reg16(lo) + 1) << ADDRESS_SHIFT).wrapping_sub(1)
    }

    fn reset_channel(&mut self) {
        let regs = self.regs;
        *self = Self::new();
        self.regs = regs;
    }

    fn load_start(&mut self) {
        self.status = (self.status & !B_STATUS_EOS) | B_STATUS_PLAYING;
        self.address = self.reg16(0x02) << ADDRESS_SHIFT;
        self.low_nibble = false;
        self.byte = 0;
        self.position = 0;
        self.accumulator = 0;
        self.previous = 0;
        self.step = B_STEP_MIN;
    }

    /// One clock: advance the phase by delta-N, and decode a nibble each
    /// time it overflows, reading `rom` as a new byte is needed.
    pub fn clock(&mut self, rom: &[u8]) {
        if !self.execute() || self.status & B_STATUS_PLAYING == 0 {
            self.status &= !B_STATUS_PLAYING;
            return;
        }
        let position = u32::from(self.position) + self.reg16(0x09);
        self.position = position as u16;
        if position < 0x1_0000 {
            return;
        }
        if !self.low_nibble {
            self.byte = rom_byte(rom, self.address);
        }
        let data = if self.low_nibble {
            self.byte & 0x0f
        } else {
            self.byte >> 4
        };
        self.low_nibble = !self.low_nibble;
        if !self.low_nibble {
            // Both nibbles of the byte are done: move on, or stop at the
            // inclusive end address.
            if self.address == self.last_byte(0x04) {
                if self.repeat() {
                    self.load_start();
                } else {
                    self.accumulator = 0;
                    self.previous = 0;
                    self.status = (self.status & !B_STATUS_PLAYING) | B_STATUS_EOS;
                    return;
                }
            } else if self.address == self.last_byte(0x0c) {
                self.address = 0;
            } else {
                self.address = (self.address + 1) & 0xff_ffff;
            }
        }
        self.previous = self.accumulator;
        let magnitude = usize::from(data & 7);
        let mut delta = (2 * magnitude as i32 + 1) * self.step / 8;
        if data & 8 != 0 {
            delta = -delta;
        }
        self.accumulator = (self.accumulator + delta).clamp(-32768, 32767);
        self.step = (self.step * B_STEP_SCALE[magnitude] / 64).clamp(B_STEP_MIN, B_STEP_MAX);
    }

    /// The channel after its level, interpolated between the last two
    /// decoded values by the phase, as `[left, right]`.
    #[must_use]
    pub fn output(&self) -> [i32; 2] {
        let position = i64::from(self.position);
        let blended = (i64::from(self.previous) * (0x1_0000 - position)
            + i64::from(self.accumulator) * position)
            >> 16;
        // The YM2610 drops one more bit than the YM2608.
        let value = ((blended * i64::from(self.regs[0x0b])) >> 9) as i32;
        let pan = self.regs[0x01];
        [
            if pan & 0x80 != 0 { value } else { 0 },
            if pan & 0x40 != 0 { value } else { 0 },
        ]
    }
}
