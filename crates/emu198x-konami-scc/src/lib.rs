//! Konami SCC and SCC-I wavetable sound chips.
//!
//! The SCC (Sound Creative Chip) sits in Konami's MSX MegaROM cartridges
//! beside their bank-switching logic. It has five channels. Each plays a
//! 32-byte waveform of signed samples, stepping one sample every
//! `period + 1` input clocks, so a channel sounds at
//! `clock / (32 × (period + 1))`: on the MSX's 3.58 MHz clock, a period of
//! 253 gives 440 Hz. A 4-bit volume scales each sample and five enable bits
//! gate the channels.
//!
//! # Registers
//!
//! The chip decodes eight address bits; a cartridge maps them into a window
//! of its own (the MegaROM's `$9800-$98FF`, mirrored to `$9FFF`).
//!
//! | Range (SCC) | Range (SCC-I, SCC+ mode) | Holds                          |
//! |-------------|--------------------------|--------------------------------|
//! | `$00-$7F`   | `$00-$9F`                | waveforms, 32 bytes a channel  |
//! | `$80-$9F`   | `$A0-$BF`                | frequency, volume and enables  |
//! | `$E0-$FF`   | `$C0-$DF`                | the deformation register       |
//!
//! In the frequency block, which appears twice, `$x0-$x9` are the five
//! 12-bit periods (low byte, then the high nibble), `$xA-$xE` the volumes
//! and `$xF` the enable bits. The plain SCC has only four waveforms:
//! channels 4 and 5 play the same one, written at `$60-$7F`. The SCC-I gives
//! channel 5 its own and moves the blocks up to make room, but only in its
//! SCC+ mode; in its compatible mode it keeps the SCC's map, except that the
//! deformation register moves to `$C0-$DF` and channel 5's waveform can be
//! read at `$A0-$BF`. The frequency block and the deformation register are
//! write-only and read `$FF`.
//!
//! # Output
//!
//! Each channel outputs `(sample × volume) >> 4`, its low four bits dropped
//! before the five are summed: the measurement Jon De Schrijder made on the
//! chip's 11-bit output. A channel takes a new volume when it next steps to
//! a sample, not at once; a frequency write restarts the current sample and
//! shows it straight away. A period of 8 or less stops the channel where it
//! is. A disabled channel keeps stepping silently and is silent until its
//! next step after it is enabled again.
//!
//! # The deformation register
//!
//! | Bit | Effect                                                         |
//! |-----|----------------------------------------------------------------|
//! | 0   | periods are 4 bits: the high nibble alone is used              |
//! | 1   | periods are 8 bits: the high nibble is ignored (wins over bit 0) |
//! | 5   | a frequency write restarts the waveform from its first sample  |
//! | 6   | all waveforms rotate and become read-only                      |
//! | 7   | (SCC only) channels 4 and 5 rotate and become read-only        |
//!
//! With bits 6 and 7 both set, channels 1-3 rotate and every waveform is
//! read-only. A rotating waveform shifts by one sample every `period + 1`
//! clocks, which the CPU sees when it reads it back; channels 4 and 5 rotate
//! at channel 5's rate. The periods' width applies when a period is written.
//! Reading the deformation register's addresses writes `$FF` to it.
//!
//! # Provenance
//!
//! The register map, the output arithmetic, the power-on state and the
//! deformation register are facts read from openMSX (GPL-2.0, read for facts
//! only, not code), at commit `38af174e2785b490de07bb0ff891e950e64a3bcd`,
//! which records the measurements of Jon De Schrijder, Manuel Pazos and
//! NYYRIKKI.

use serde::{Deserialize, Serialize};

/// Channels.
pub const CHANNELS: usize = 5;

/// Samples in a waveform.
pub const WAVE_LENGTH: usize = 32;

/// Which chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Variant {
    /// The SCC of the MegaROM cartridges.
    Scc,
    /// The SCC-I of the Sound Cartridge, which starts in its compatible
    /// mode.
    SccI,
}

/// How the chip lays out its registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// The SCC's own map.
    Scc,
    /// The SCC-I mimicking the SCC.
    Compatible,
    /// The SCC-I with a waveform for channel 5.
    Plus,
}

/// The SCC or SCC-I.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scc {
    mode: Mode,
    waves: [[i8; WAVE_LENGTH]; CHANNELS],
    volume: [u8; CHANNELS],
    /// The periods as written.
    written_period: [u16; CHANNELS],
    /// The periods the channels run at, after the deformation register's
    /// width.
    period: [u16; CHANNELS],
    /// Clocks into the current sample.
    counter: [u16; CHANNELS],
    position: [u8; CHANNELS],
    /// Each channel's output.
    out: [i16; CHANNELS],
    enable: u8,
    deform: u8,
    rotate: [bool; CHANNELS],
    read_only: [bool; CHANNELS],
    /// Clocks since the deformation register last changed, which sets how
    /// far a rotating waveform has turned.
    deform_clocks: u64,
}

impl Scc {
    /// A chip as it powers up: waveforms of `$FF`, full volume, every
    /// channel disabled and stopped, the deformation register clear.
    #[must_use]
    pub fn new(variant: Variant) -> Self {
        Self {
            mode: match variant {
                Variant::Scc => Mode::Scc,
                Variant::SccI => Mode::Compatible,
            },
            waves: [[-1; WAVE_LENGTH]; CHANNELS],
            volume: [15; CHANNELS],
            written_period: [0; CHANNELS],
            period: [0; CHANNELS],
            counter: [0; CHANNELS],
            position: [0; CHANNELS],
            out: [0; CHANNELS],
            enable: 0,
            deform: 0,
            rotate: [false; CHANNELS],
            read_only: [false; CHANNELS],
            deform_clocks: 0,
        }
    }

    /// The reset line: the enable bits and the deformation register clear,
    /// and an SCC-I returns to its compatible mode. Waveforms, periods and
    /// volumes keep their values.
    pub fn reset(&mut self) {
        if self.mode != Mode::Scc {
            self.mode = Mode::Compatible;
        }
        self.set_deform(0);
        self.enable = 0;
    }

    /// The register layout in force.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Switch an SCC-I between its compatible and SCC+ modes. The SCC has
    /// one mode and ignores this.
    pub fn set_plus_mode(&mut self, plus: bool) {
        if self.mode != Mode::Scc {
            self.mode = if plus { Mode::Plus } else { Mode::Compatible };
        }
    }

    /// A CPU read of register `address`. Reading the deformation register's
    /// addresses writes `$FF` to it.
    pub fn read(&mut self, address: u8) -> u8 {
        if self.is_deform_address(address) {
            self.write_deform(0xFF);
        }
        self.peek(address)
    }

    /// What a CPU read of `address` returns, without its side effect.
    #[must_use]
    pub fn peek(&self, address: u8) -> u8 {
        match (self.mode, address) {
            (Mode::Scc | Mode::Compatible, 0x00..=0x7F) | (Mode::Plus, 0x00..=0x9F) => {
                self.read_wave(usize::from(address >> 5), address)
            }
            (Mode::Compatible, 0xA0..=0xBF) => self.read_wave(4, address),
            _ => 0xFF,
        }
    }

    /// A CPU write of `value` to register `address`.
    pub fn write(&mut self, address: u8, value: u8) {
        match (self.mode, address) {
            (Mode::Scc | Mode::Compatible, 0x00..=0x7F) | (Mode::Plus, 0x00..=0x9F) => {
                self.write_wave(usize::from(address >> 5), address, value);
            }
            (Mode::Scc | Mode::Compatible, 0x80..=0x9F) | (Mode::Plus, 0xA0..=0xBF) => {
                self.write_frequency_block(address, value);
            }
            (Mode::Scc, 0xE0..=0xFF) | (Mode::Compatible | Mode::Plus, 0xC0..=0xDF) => {
                self.write_deform(value);
            }
            _ => {}
        }
    }

    /// One input clock.
    pub fn tick(&mut self) {
        self.deform_clocks = self.deform_clocks.wrapping_add(1);
        for channel in 0..CHANNELS {
            let enabled = self.enable & (1 << channel) != 0;
            if !enabled {
                // Silent until the step after it is enabled again.
                self.out[channel] = 0;
            }
            let period = self.period[channel];
            if period <= 8 {
                continue;
            }
            self.counter[channel] += 1;
            if self.counter[channel] > period {
                self.counter[channel] = 0;
                let position = (self.position[channel] + 1) % WAVE_LENGTH as u8;
                self.position[channel] = position;
                if enabled {
                    self.out[channel] = self.sample(channel, position);
                }
            }
        }
    }

    /// One channel's output: its sample scaled by its volume, or 0 while it
    /// is disabled.
    #[must_use]
    pub fn channel_output(&self, channel: usize) -> i16 {
        if self.enable & (1 << channel) == 0 {
            0
        } else {
            self.out[channel]
        }
    }

    /// The five channels summed, -600 to 595.
    #[must_use]
    pub fn output(&self) -> i32 {
        (0..CHANNELS)
            .map(|channel| i32::from(self.channel_output(channel)))
            .sum()
    }

    /// A channel's period as it runs, after the deformation register's
    /// width.
    #[must_use]
    pub fn period(&self, channel: usize) -> u16 {
        self.period[channel]
    }

    /// The frequency a channel's waveform plays at, on input clock
    /// `clock_hz`; 0 when the channel is stopped.
    #[must_use]
    pub fn frequency_hz(&self, channel: usize, clock_hz: f64) -> f64 {
        let period = self.period[channel];
        if period <= 8 {
            0.0
        } else {
            clock_hz / (WAVE_LENGTH as f64 * (f64::from(period) + 1.0))
        }
    }

    /// A channel's volume, 0-15.
    #[must_use]
    pub fn volume(&self, channel: usize) -> u8 {
        self.volume[channel]
    }

    /// The enable bits.
    #[must_use]
    pub fn enable_bits(&self) -> u8 {
        self.enable
    }

    /// The deformation register as last written.
    #[must_use]
    pub fn deform(&self) -> u8 {
        self.deform
    }

    fn is_deform_address(&self, address: u8) -> bool {
        match self.mode {
            Mode::Scc => address >= 0xE0,
            Mode::Compatible | Mode::Plus => (0xC0..0xE0).contains(&address),
        }
    }

    fn sample(&self, channel: usize, position: u8) -> i16 {
        (i16::from(self.waves[channel][usize::from(position)]) * i16::from(self.volume[channel]))
            >> 4
    }

    fn read_wave(&self, channel: usize, address: u8) -> u8 {
        let mut index = usize::from(address) & (WAVE_LENGTH - 1);
        if self.rotate[channel] {
            // Channels 4 and 5 turn at channel 5's rate when only they rotate.
            let rate_channel =
                if channel == 3 && self.mode != Mode::Plus && self.deform & 0xC0 == 0x40 {
                    4
                } else {
                    channel
                };
            let step = u64::from(self.period[rate_channel]) + 1;
            index = (index + (self.deform_clocks / step) as usize) & (WAVE_LENGTH - 1);
        }
        self.waves[channel][index] as u8
    }

    fn write_wave(&mut self, channel: usize, address: u8, value: u8) {
        if self.read_only[channel] {
            return;
        }
        let index = usize::from(address) & (WAVE_LENGTH - 1);
        self.waves[channel][index] = value as i8;
        if self.mode != Mode::Plus && channel == 3 {
            // Without a waveform of its own, channel 5 plays channel 4's.
            self.waves[4][index] = value as i8;
        }
    }

    fn write_frequency_block(&mut self, address: u8, value: u8) {
        let register = usize::from(address & 0x0F);
        match register {
            0x0..=0x9 => {
                let channel = register / 2;
                let written = self.written_period[channel];
                let written = if register & 1 == 0 {
                    (written & 0xF00) | u16::from(value)
                } else {
                    (u16::from(value & 0x0F) << 8) | (written & 0xFF)
                };
                self.written_period[channel] = written;
                self.period[channel] = if self.deform & 0x02 != 0 {
                    written & 0xFF
                } else if self.deform & 0x01 != 0 {
                    written >> 8
                } else {
                    written
                };
                self.counter[channel] = 0;
                if self.deform & 0x20 != 0 {
                    self.position[channel] = 0;
                    self.deform_clocks = 0;
                }
                self.out[channel] = self.sample(channel, self.position[channel]);
            }
            0xA..=0xE => self.volume[register - 0xA] = value & 0x0F,
            _ => self.enable = value,
        }
    }

    fn write_deform(&mut self, value: u8) {
        if value != self.deform {
            self.deform_clocks = 0;
            self.set_deform(value);
        }
    }

    fn set_deform(&mut self, value: u8) {
        self.deform = value;
        // Only the SCC has the channel-4 rotation bit.
        let bits = if self.mode == Mode::Scc {
            value
        } else {
            value & !0x80
        };
        let (low, high) = match bits & 0xC0 {
            0x00 => ((false, false), (false, false)),
            0x40 => ((true, true), (true, true)),
            0x80 => ((false, false), (true, true)),
            _ => ((true, true), (false, true)),
        };
        for channel in 0..CHANNELS {
            let (rotate, read_only) = if channel < 3 { low } else { high };
            self.rotate[channel] = rotate;
            self.read_only[channel] = read_only;
        }
    }
}

#[cfg(test)]
mod tests;
