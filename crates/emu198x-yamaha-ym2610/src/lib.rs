//! The Yamaha YM2610 (OPNB): the SNK Neo Geo's sound chip.
//!
//! Four channels of four-operator FM, an SSG with three square-wave
//! channels, noise and an envelope, six channels of fixed-rate ADPCM-A and
//! one channel of variable-rate ADPCM-B, plus the OPN family's two timers.
//!
//! # How it is modelled
//!
//! The YM2610 is an OPN-family chip. Its FM block is the OPN2's six-channel
//! pipeline with channels 1 and 4 left unconnected, and the same register
//! map. So this crate does not carry a second FM core: it composes
//! [`emu198x_yamaha_ym2612::Ym2612`] in its CMOS [`Variant::Ym3438`] form,
//! whose output carries no DAC ladder, as the YM2610's digital output does
//! not. The YM2610 differs from the OPN2 at the edges, and the edges are
//! here: it has no DAC register and no channels 1 and 4, so writes that would
//! key those channels on or enable the DAC never reach the FM core. Keeping
//! one FM core means one oracle (Nuked-OPN2, run by the YM2612 crate) covers
//! both chips, and a fix to the pipeline reaches both at once.
//!
//! The SSG is the AY-3-8910's sound section, so it is
//! [`emu198x_gi_ay_3_8910::Ay3_8910`], clocked at a quarter of the input
//! clock. The YM2610 has no I/O ports; registers `$0E`–`$0F` read `$FF`.
//!
//! The two ADPCM units are in [`adpcm`], re-expressed from ymfm by Aaron
//! Giles (BSD-3-Clause). They run once per FM sample: ADPCM-B on every
//! sample and ADPCM-A on every third, the 18.5 kHz rate the Neo Geo's
//! 8 MHz clock gives.
//!
//! Register routing, the status ports and the end-of-sample flags follow
//! ymfm's `ym2610` class (`src/ymfm_opn.cpp`, commit `81aec25c`).
//!
//! # Using it
//!
//! Drive [`Ym2610::tick`] from the chip's input clock (8 MHz on the Neo
//! Geo, the 24 MHz master clock divided by 3), passing the ADPCM-A and
//! ADPCM-B sample ROMs. The sample ROMs are borrowed per tick: the V-ROM bus
//! belongs to the YM2610 alone, so a borrowed slice gives the same byte a
//! pin-level request would, at once and without a round trip through the
//! machine. Write with [`Ym2610::write`], read with [`Ym2610::read`], and
//! collect a [`Sample`] per 144 input clocks with [`Ym2610::take_samples`].
//!
//! The chip exposes its state as public fields and is stepped by the
//! machine, so it owns no bus and makes no callbacks, the same as every
//! Emu198x chip.

pub mod adpcm;

use emu198x_gi_ay_3_8910::Ay3_8910;
pub use emu198x_yamaha_ym2612::Variant;
use emu198x_yamaha_ym2612::Ym2612;
use serde::{Deserialize, Serialize};

use adpcm::{AdpcmA, AdpcmB, B_STATUS_EOS};

/// Input clocks per FM sample: six per internal clock, 24 internal clocks.
pub const CLOCKS_PER_SAMPLE: u32 = 144;

/// Input clocks per SSG clock.
const SSG_DIVIDER: u8 = 4;

/// The end-of-sample flags software can see and clear: ADPCM-A channels in
/// bits 0-5 and ADPCM-B in bit 7. Bit 6 tracks ADPCM-B internally.
const EOS_FLAGS_MASK: u8 = 0xbf;

/// One output sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// FM, ADPCM-A and ADPCM-B mixed and clamped to 16 bits, as `[left,
    /// right]`: the digital stream the YM2610 sends to its YM3016 DAC.
    pub fm: [i16; 2],
    /// The SSG's analogue output, 0.0 to 1.0, which leaves the chip on its
    /// own pin and is mixed on the board.
    pub ssg: f32,
}

/// One YM2610.
#[derive(Serialize, Deserialize)]
pub struct Ym2610 {
    /// The FM block: the OPN2 pipeline with channels 1 and 4 unconnected.
    pub fm: Ym2612,
    /// The SSG.
    pub ssg: Ay3_8910,
    /// The six ADPCM-A channels.
    pub adpcm_a: AdpcmA,
    /// The ADPCM-B channel.
    pub adpcm_b: AdpcmB,
    /// The latched register address; bit 8 is set when it came through the
    /// upper address port.
    pub address: u16,
    /// End-of-sample flags: ADPCM-A channels in bits 0-5, ADPCM-B in bit 7,
    /// and in bit 6 the ADPCM-B flag as last seen.
    pub eos_status: u8,
    /// Which end-of-sample flags register `$1C` leaves visible.
    pub flag_mask: u8,
    /// Input clocks since the SSG last ran.
    ssg_divider: u8,
    /// FM samples since ADPCM-A last ran.
    adpcm_a_phase: u8,
    #[serde(skip)]
    samples: Vec<Sample>,
}

impl Default for Ym2610 {
    fn default() -> Self {
        Self::new()
    }
}

impl Ym2610 {
    /// A chip just out of reset.
    #[must_use]
    pub fn new() -> Self {
        Self {
            fm: Ym2612::new(Variant::Ym3438),
            // The SSG yields one sample per 36 of its clocks, which is one
            // per FM sample: 144 input clocks over a divider of 4.
            ssg: Ay3_8910::new(CLOCKS_PER_SAMPLE / u32::from(SSG_DIVIDER), 1, 1),
            adpcm_a: AdpcmA::new(),
            adpcm_b: AdpcmB::new(),
            address: 0,
            eos_status: 0,
            flag_mask: EOS_FLAGS_MASK,
            ssg_divider: 0,
            adpcm_a_phase: 0,
            samples: Vec::new(),
        }
    }

    /// Pull the reset line.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// One input clock. `rom_a` and `rom_b` are the ADPCM-A and ADPCM-B
    /// sample ROMs; on boards where both read the same ROM, pass it twice.
    pub fn tick(&mut self, rom_a: &[u8], rom_b: &[u8]) {
        self.fm.tick();
        self.ssg_divider += 1;
        if self.ssg_divider == SSG_DIVIDER {
            self.ssg_divider = 0;
            self.ssg.tick();
        }
        if self.fm.prescaler == 0 && self.fm.cycles == 0 {
            self.finish_sample(rom_a, rom_b);
        }
    }

    /// The FM pipeline has just completed a sample: run the ADPCM units and
    /// mix.
    fn finish_sample(&mut self, rom_a: &[u8], rom_b: &[u8]) {
        let fm = self.fm.take_samples().last().copied().unwrap_or([0, 0]);

        self.adpcm_a_phase += 1;
        if self.adpcm_a_phase == 3 {
            self.adpcm_a_phase = 0;
            self.eos_status |= self.adpcm_a.clock(rom_a);
        }
        self.adpcm_b.clock(rom_b);

        // Bit 6 follows ADPCM-B's end flag; bit 7, the one software sees,
        // copies it whenever it changes.
        let live = if self.adpcm_b.status() & B_STATUS_EOS != 0 {
            0x40
        } else {
            0
        };
        if (live ^ self.eos_status) & 0x40 != 0 {
            self.eos_status = (self.eos_status & !0xc0) | live | (live << 1);
        }

        let a = self.adpcm_a.output();
        let b = self.adpcm_b.output();
        let mut mixed = [0i16; 2];
        for side in 0..2 {
            // The YM3438 pins carry each 9-bit channel for 3 of every 4
            // internal clocks, so a sample's sum is three times the channel
            // total. Scaling by 16/3 puts it at the OPNB's 13-bit FM level.
            let fm_level = fm[side] * 16 / 3;
            let sum = fm_level + a[side] + b[side];
            mixed[side] = sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        }

        let mut ssg = [0.0];
        self.ssg.end_frame(&mut ssg);
        self.samples.push(Sample {
            fm: mixed,
            ssg: ssg[0],
        });
    }

    /// The samples completed since the last call.
    pub fn take_samples(&mut self) -> Vec<Sample> {
        std::mem::take(&mut self.samples)
    }

    /// Write `data` to port `port` (A1 and A0). Ports 0 and 2 latch a
    /// register address for the lower and upper banks; ports 1 and 3 write
    /// data to it. A data write through the port that does not match the
    /// latched bank is ignored.
    pub fn write(&mut self, port: u8, data: u8) {
        match port & 3 {
            0 => {
                self.address = u16::from(data);
                self.fm.write(0, data);
            }
            2 => {
                self.address = 0x100 | u16::from(data);
                self.fm.write(2, data);
            }
            1 => {
                if self.address & 0x100 != 0 {
                    return;
                }
                let address = self.address as u8;
                match address {
                    0x00..=0x0d => {
                        self.ssg.select_register(address);
                        self.ssg.write_data(data);
                    }
                    0x0e..=0x0f => {}
                    0x10..=0x1b => self.adpcm_b.write(address & 0x0f, data),
                    0x1c => {
                        self.flag_mask = !data & EOS_FLAGS_MASK;
                        self.eos_status &= !(data & EOS_FLAGS_MASK);
                    }
                    // Key-on for the unconnected channels 1 and 4.
                    0x28 if data & 3 == 0 => return,
                    // No DAC.
                    0x2a | 0x2b => return,
                    _ => {}
                }
                // The FM core decodes only its own registers, but every data
                // write starts its busy flag.
                self.fm.write(1, data);
            }
            _ => {
                if self.address & 0x100 == 0 {
                    return;
                }
                if self.address < 0x130 {
                    self.adpcm_a.write(self.address as u8 & 0x3f, data);
                }
                self.fm.write(3, data);
            }
        }
    }

    /// Read port `port`. Port 0 is the status (busy in bit 7, the timer
    /// flags in bits 1 and 0); port 1 reads back the SSG register latched by
    /// port 0; port 2 is the end-of-sample flags; port 3 reads zero.
    pub fn read(&mut self, port: u8) -> u8 {
        match port & 3 {
            0 => self.fm.read(0) & 0x83,
            1 => match self.address {
                0x00..=0x0d => {
                    self.ssg.select_register(self.address as u8);
                    self.ssg.read_data()
                }
                0x0e..=0x0f => 0xff,
                // The chip identifies itself here.
                0xff => 0x01,
                _ => 0,
            },
            2 => self.eos_status & self.flag_mask,
            _ => 0,
        }
    }

    /// The IRQ output: a timer has overflowed with its flag enabled.
    #[must_use]
    pub fn irq(&self) -> bool {
        self.fm.irq()
    }
}

#[cfg(test)]
mod tests;
