//! MOS Technology 6560 (NTSC) / 6561 (PAL) — VIC chip ("VIC-I").
//!
//! The VIC chip handles both video AND audio for the VIC-20. Video output
//! is 22×23 characters in text mode (176×184 px active area surrounded by
//! a substantial border for a TV-visible 224×216 frame). The chip also
//! provides 3 tone generators and 1 noise generator.
//!
//! Extracted from `machine-commodore-vic-20::vic` as a stand-alone chip
//! crate so the wider workspace can reuse it.
//!
//! Video is text-mode. Audio is a full four-source model: three tone shift
//! registers (bass/alto/soprano, `$900A`-`$900C`) and a 16-bit-LFSR noise
//! source (`$900D`), each gated by its register bit 7 and clocked at the VIC
//! machine-cycle rate through a per-channel divider; the four outputs are mixed,
//! scaled by the master volume nibble (`$900E`), passed through the chip's
//! nonlinear output DAC and an RC high/low-pass network, and resampled to the
//! host rate. The model mirrors VICE 3.10 `vic20sound.c` (Rasanen & Heikkila).

/// Framebuffer dimensions (visible area for text mode).
pub const ACTIVE_WIDTH: u32 = 176;
pub const ACTIVE_HEIGHT: u32 = 184;

/// Border thickness around the active area. The VIC chip generates a
/// substantial border around the active 22 x 23 character display;
/// VIC-20 reference emulators (VICE) typically render ~30-40 px of
/// border each side. 24 px L/R + 16 px T/B is a clean approximation
/// that matches the period look on a typical PAL television set.
pub const BORDER_LEFT: u32 = 24;
pub const BORDER_RIGHT: u32 = 24;
pub const BORDER_TOP: u32 = 16;
pub const BORDER_BOTTOM: u32 = 16;

pub const FB_WIDTH: u32 = ACTIVE_WIDTH + BORDER_LEFT + BORDER_RIGHT;
pub const FB_HEIGHT: u32 = ACTIVE_HEIGHT + BORDER_TOP + BORDER_BOTTOM;

use serde::{Deserialize, Serialize};

/// VIC-20 machine-cycle clock — the rate the sound oscillators are clocked at
/// (the VIC generates the CPU phi2, so they share this clock). NTSC 6560 and
/// PAL 6561 differ, which shifts every pitch slightly, exactly as on hardware.
const CPU_CLOCK_NTSC: f32 = 1_022_727.0;
const CPU_CLOCK_PAL: f32 = 1_108_405.0;

/// Default host audio sample rate the chip resamples its output down to.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

/// Per-channel counter-reload shift: bass `<<4`, alto `<<3`, soprano `<<2`,
/// noise `<<1` — the divider chain that puts the three tones an octave apart
/// and the noise at the top (VICE `"\4\3\2\1"`).
const CH_SPEED: [u8; 4] = [4, 3, 2, 1];

/// One VIC sound source: an 8-bit shift register clocked by a reloadable
/// counter. The tone channels feed back the inverted top bit; the noise channel
/// is additionally gated by the 16-bit noise LFSR.
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
struct SoundChannel {
    /// Current output level (0 or 1).
    out: u8,
    /// 8-bit waveform shift register.
    shift: u8,
    /// Signed reload counter; a byte is clocked out when it reaches zero.
    ctr: i16,
}

/// VIC 6560/6561 chip.
#[derive(Clone, Serialize, Deserialize)]
pub struct Vic6560 {
    /// ARGB32 framebuffer.
    framebuffer: Vec<u32>,
    /// VIC registers ($9000-$900F). Sound reads `$A`-`$E` from here directly.
    regs: [u8; 16],
    /// Whether a frame has completed.
    frame_complete: bool,
    /// Current scanline.
    scanline: u32,
    /// Current pixel position within the scanline.
    pixel_x: u32,
    /// Total lines per frame (PAL: 312, NTSC: 261).
    lines_per_frame: u32,
    /// Cycles per line (PAL: 71, NTSC: 65).
    cycles_per_line: u32,

    // ---- sound ----
    /// The three tone channels (0-2) and the noise channel (3).
    sound_ch: [SoundChannel; 4],
    /// 16-bit noise LFSR and its previous LSB (for the noise clock's edge gate).
    noise_lfsr: u16,
    noise_lfsr0_old: u8,
    /// Output level summed over the current sample window, and the window's
    /// cycle count, for the `accum/accum_cycles` average the DAC is fed.
    accum: i32,
    accum_cycles: i32,
    /// Fractional machine-cycle position within the current output sample.
    cycle_in_sample: f32,
    /// Machine cycles per host output sample (`cpu_clock / sample_rate`).
    cycles_per_sample: f32,
    /// RC output-network filter state and coefficients (see `new`).
    lowpass: f32,
    highpass: f32,
    lowpass_beta: f32,
    highpass_beta: f32,
    /// Host sample rate the chip resamples to.
    sample_rate: u32,
    /// Produced host samples awaiting drain via [`Vic6560::take_audio`].
    audio_buffer: Vec<f32>,
}

impl Vic6560 {
    /// Create a new VIC chip.
    ///
    /// `pal`: true for PAL (6561), false for NTSC (6560).
    #[must_use]
    pub fn new(pal: bool) -> Self {
        let (lines, cycles) = if pal { (312, 71) } else { (261, 65) };
        let cpu_clock = if pal { CPU_CLOCK_PAL } else { CPU_CLOCK_NTSC };
        let sample_rate = DEFAULT_SAMPLE_RATE;
        // VICE's output stage: a low-pass (R = 1k, C = 100nF, ~1591 Hz) followed
        // by a DC-blocking high-pass (R = 1k, C = 1uF, ~159 Hz). One-pole betas
        // derived from the per-sample timestep.
        let dt = 1.0 / sample_rate as f32;
        Self {
            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
            regs: [0; 16],
            frame_complete: false,
            scanline: 0,
            pixel_x: 0,
            lines_per_frame: lines,
            cycles_per_line: cycles,
            sound_ch: [SoundChannel::default(); 4],
            noise_lfsr: 0,
            noise_lfsr0_old: 0,
            accum: 0,
            accum_cycles: 0,
            cycle_in_sample: 0.0,
            cycles_per_sample: cpu_clock / sample_rate as f32,
            lowpass: 0.0,
            highpass: 0.0,
            lowpass_beta: dt / (dt + 1e-4),
            highpass_beta: dt / (dt + 1e-3),
            sample_rate,
            audio_buffer: Vec::new(),
        }
    }

    /// Read a VIC register.
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        let reg = (addr & 0x0F) as usize;
        self.regs[reg]
    }

    /// Write a VIC register.
    pub fn write(&mut self, addr: u8, value: u8) {
        let reg = (addr & 0x0F) as usize;
        self.regs[reg] = value;
    }

    /// Tick one VIC cycle. Call with callbacks for reading screen RAM,
    /// colour RAM, and character ROM.
    pub fn tick(
        &mut self,
        read_screen: impl Fn(u16) -> u8,
        read_colour: impl Fn(u16) -> u8,
        read_char_rom: impl Fn(u16) -> u8,
    ) -> bool {
        // The VIC clocks its sound once per machine cycle, same clock as video.
        self.clock_sound();

        self.pixel_x += 1;

        if self.pixel_x >= self.cycles_per_line {
            self.pixel_x = 0;
            self.scanline += 1;

            if self.scanline >= self.lines_per_frame {
                self.scanline = 0;
                self.frame_complete = true;
                return true;
            }
        }

        // Render during visible area
        // Text display: 22 columns x 23 rows = 176x184 pixels
        let visible_y_start = 28u32;
        let visible_y_end = visible_y_start + 184;

        // At the start of each new frame, repaint the entire framebuffer
        // with the VIC border colour (register $F low nibble) so the
        // border around the 176 x 184 active region carries the right
        // colour. Mid-frame border-colour changes affect the next frame
        // — v1 simplification.
        if self.scanline == 0 && self.pixel_x == 0 {
            let border_colour = VIC_PALETTE[(self.regs[0x0F] as usize) & 0x0F];
            self.framebuffer.fill(border_colour);
        }

        if self.scanline >= visible_y_start && self.scanline < visible_y_end && self.pixel_x < 22 {
            let vis_y = self.scanline - visible_y_start;
            let char_row = vis_y / 8;
            let pixel_in_char_y = vis_y % 8;
            let char_col = self.pixel_x;

            // Screen memory base from registers
            let screen_base =
                (u16::from(self.regs[5]) & 0xF0) << 6 | (u16::from(self.regs[2]) & 0x80) << 2;

            let char_addr = screen_base.wrapping_add(char_row as u16 * 22 + char_col as u16);
            let char_code = read_screen(char_addr);
            let colour_nibble = read_colour(char_addr) & 0x0F;

            // Character ROM lookup
            let char_rom_base = (u16::from(self.regs[5]) & 0x0F) << 10;
            let char_rom_addr =
                char_rom_base.wrapping_add(u16::from(char_code) * 8 + pixel_in_char_y as u16);
            let char_data = read_char_rom(char_rom_addr);

            // Background colour from register $0F
            let bg_colour = VIC_PALETTE[(self.regs[0x0F] as usize >> 4) & 0x0F];
            let fg_colour = VIC_PALETTE[colour_nibble as usize];

            // Render 8 pixels for this character column, offset into
            // the active region of the framebuffer (skip the border).
            for px in 0..8 {
                let active_x = char_col * 8 + px;
                if active_x < ACTIVE_WIDTH {
                    let bit = (char_data >> (7 - px)) & 1;
                    let colour = if bit != 0 { fg_colour } else { bg_colour };
                    let fb_x = BORDER_LEFT + active_x;
                    let fb_y = BORDER_TOP + vis_y;
                    let idx = (fb_y * FB_WIDTH + fb_x) as usize;
                    if idx < self.framebuffer.len() {
                        self.framebuffer[idx] = colour;
                    }
                }
            }
        }

        false
    }

    /// Take the frame-complete flag.
    pub fn take_frame_complete(&mut self) -> bool {
        let result = self.frame_complete;
        self.frame_complete = false;
        result
    }

    /// Reference to the framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    /// Current registers (for observation).
    #[must_use]
    pub fn regs(&self) -> &[u8; 16] {
        &self.regs
    }

    /// The host sample rate the chip's audio is resampled to.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Drains the host-rate audio samples produced since the last call
    /// (mono, f32 in `[-1.0, 1.0]`). The runtime pumps these into the sink.
    #[must_use]
    pub fn take_audio(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.audio_buffer)
    }

    /// Advances the four sound sources by one machine cycle and, when a host
    /// sample period has elapsed, mixes and emits one output sample. Mirrors
    /// VICE 3.10 `vic20sound.c::vic_sound_clock` (per-cycle path).
    fn clock_sound(&mut self) {
        for (j, &chspeed) in CH_SPEED.iter().enumerate() {
            let reg = self.regs[0x0A + j];
            self.sound_ch[j].ctr -= 1;
            if self.sound_ch[j].ctr <= 0 {
                // Reload from the inverted low 7 bits (zero folds to 128),
                // shifted by the channel's divider.
                let inv = (!reg) & 0x7F;
                let reload = if inv != 0 { inv } else { 128 };
                self.sound_ch[j].ctr += i16::from(reload) << chspeed;

                let enabled = (reg & 0x80) >> 7; // 0 or 1
                // The noise channel only clocks its shift register on a rising
                // edge of the noise LFSR's LSB; the tone channels clock always.
                let noise_edge = (self.noise_lfsr & 1) != 0 && self.noise_lfsr0_old == 0;
                if j != 3 || noise_edge {
                    let shift = self.sound_ch[j].shift;
                    let feedback = (((shift & 0x80) >> 7) ^ 1) & enabled;
                    self.sound_ch[j].shift = (shift << 1) | feedback;
                }

                if j == 3 {
                    // 16-bit noise LFSR (VICE taps 3/12/14/15, gated by enable).
                    let l = self.noise_lfsr;
                    let gate1 = ((l >> 3) & 1) ^ ((l >> 12) & 1);
                    let gate2 = ((l >> 14) & 1) ^ ((l >> 15) & 1);
                    let gate3 = (gate1 ^ gate2) ^ 1;
                    let gate4 = (gate3 & u16::from(enabled)) ^ 1;
                    self.noise_lfsr0_old = (l & 1) as u8;
                    self.noise_lfsr = (l << 1) | gate4;
                }

                let mask = if j == 3 { enabled } else { 1 };
                self.sound_ch[j].out = self.sound_ch[j].shift & mask;
            }
            self.accum += i32::from(self.sound_ch[j].out);
        }
        self.accum_cycles += 1;

        self.cycle_in_sample += 1.0;
        if self.cycle_in_sample >= self.cycles_per_sample {
            self.cycle_in_sample -= self.cycles_per_sample;
            self.emit_sample();
        }
    }

    /// Mixes the accumulated output over the elapsed sample window through the
    /// master volume, the nonlinear DAC table, and the RC filter network, and
    /// pushes one host sample.
    fn emit_sample(&mut self) {
        let volume = i32::from(self.regs[0x0E] & 0x0F);
        // Average output level over the window, scaled to 0..=28 (four sources
        // times seven), as VICE feeds the DAC.
        let level = if self.accum_cycles > 0 {
            (self.accum * 7) / self.accum_cycles
        } else {
            0
        };
        let index = ((level + 1) * volume) as usize;
        let voltage = VOLTAGE_FUNCTION[index.min(VOLTAGE_FUNCTION.len() - 1)];

        // VICE order: read the filtered output, then advance both poles.
        let output = self.lowpass - self.highpass;
        self.highpass += self.highpass_beta * (self.lowpass - self.highpass);
        self.lowpass += self.lowpass_beta * (voltage - self.lowpass);

        self.audio_buffer.push((output / 32768.0).clamp(-1.0, 1.0));
        self.accum = 0;
        self.accum_cycles = 0;
    }
}

impl Default for Vic6560 {
    fn default() -> Self {
        Self::new(true)
    }
}

/// VIC-20 colour palette (ARGB32).
static VIC_PALETTE: [u32; 16] = [
    0xFF00_0000, // 0  Black
    0xFFFF_FFFF, // 1  White
    0xFF78_2922, // 2  Red
    0xFF87_D6DD, // 3  Cyan
    0xFFAA_5FB6, // 4  Purple
    0xFF55_A049, // 5  Green
    0xFF40_31A2, // 6  Blue
    0xFFBF_CE72, // 7  Yellow
    0xFFAA_7449, // 8  Orange
    0xFFEA_B489, // 9  Light Orange
    0xFFB8_6962, // 10 Light Red
    0xFFC7_FFFF, // 11 Light Cyan
    0xFFEA_9FF6, // 12 Light Purple
    0xFF94_E089, // 13 Light Green
    0xFF87_71F2, // 14 Light Blue
    0xFFFF_FFB2, // 15 Light Yellow
];

/// The VIC-I output stage's nonlinear DAC voltage table, ported verbatim
/// from VICE 3.10 `vic20sound.c` (`voltagefunction[]`, by Rasanen &
/// Heikkila, GPL-2.0-or-later). Indexed by `(mixed_level + 1) * volume`; it
/// models the RC/op-amp output network's non-linearity so loudness tracks the
/// hardware rather than rising linearly.
///
/// The literals are copied verbatim from VICE's `float` table; the extra digits
/// round to the same `f32`, so precision-trimming them would only risk drifting
/// from the reference — hence the `excessive_precision` allow.
#[rustfmt::skip]
#[allow(clippy::excessive_precision)]
static VOLTAGE_FUNCTION: [f32; 436] = [
    0.00, 148.28, 296.55, 735.97, 914.88, 1126.89, 1321.86, 1503.07, 1603.50, 1758.00, 1913.98, 2070.94,
    2220.36, 2342.91, 2488.07, 3188.98, 3285.76, 3382.53, 3479.31, 3576.08, 3672.86, 3769.63, 3866.41, 3963.18,
    4059.96, 4248.10, 4436.24, 4624.38, 4812.53, 5000.67, 5188.81, 5192.91, 5197.00, 5338.52, 5480.04, 5621.56,
    5763.07, 5904.59, 6046.11, 6187.62, 6329.14, 6609.31, 6889.47, 7169.64, 7449.80, 7729.97, 7809.36, 7888.75,
    7968.13, 8047.52, 8126.91, 8206.30, 8285.69, 8365.07, 8444.46, 8523.85, 8603.24, 8905.93, 9208.63, 9511.32,
    9814.02, 9832.86, 9851.70, 9870.54, 9889.38, 9908.22, 9927.07, 9945.91, 9964.75, 9983.59, 10002.43, 10021.27,
    10040.12, 10787.23, 11534.34, 12281.45, 12284.98, 12288.50, 12292.03, 12295.56, 12299.09, 12302.62, 12306.15, 12309.68,
    12313.21, 12316.74, 12320.26, 12323.79, 12327.32, 13113.05, 13898.78, 13910.58, 13922.39, 13934.19, 13945.99, 13957.80,
    13969.60, 13981.40, 13993.21, 14005.01, 14016.81, 14028.62, 14040.42, 14052.22, 14064.03, 16926.31, 16987.04, 17047.77,
    17108.50, 17169.23, 17229.96, 17290.69, 17351.42, 17412.15, 17472.88, 17533.61, 17594.34, 17655.07, 17715.80, 17776.53,
    17837.26, 18041.51, 18245.77, 18450.02, 18654.28, 18858.53, 19062.78, 19267.04, 19471.29, 19675.55, 19879.80, 20084.05,
    20288.31, 20417.74, 20547.17, 20676.61, 20774.26, 20871.91, 20969.55, 21067.20, 21164.85, 21262.50, 21360.15, 21457.80,
    21555.45, 21653.09, 21750.74, 21848.39, 21946.04, 22043.69, 22141.34, 22212.33, 22283.33, 22354.33, 22425.33, 22496.32,
    22567.32, 22638.32, 22709.32, 22780.31, 22851.31, 22922.31, 22993.31, 23064.30, 23135.30, 23206.30, 23255.45, 23304.60,
    23353.75, 23402.91, 23452.06, 23501.21, 23550.36, 23599.51, 23648.67, 23768.81, 23888.96, 24009.11, 24129.26, 24249.41,
    24369.56, 24451.92, 24534.28, 24616.63, 24698.99, 24781.35, 24863.70, 24946.06, 25028.42, 25110.77, 25193.13, 25275.49,
    25357.84, 25440.20, 25522.56, 25604.92, 25658.87, 25712.83, 25766.79, 25820.75, 25874.71, 25928.66, 25982.62, 26036.58,
    26090.54, 26144.49, 26198.45, 26252.41, 26306.37, 26360.33, 26414.28, 26501.23, 26588.17, 26675.12, 26762.06, 26849.01,
    26935.95, 27022.90, 27109.84, 27196.78, 27283.73, 27370.67, 27457.62, 27544.56, 27631.51, 27718.45, 27726.89, 27735.33,
    27743.78, 27752.22, 27760.66, 27769.10, 27777.54, 27785.98, 27794.43, 27802.87, 27811.31, 27819.75, 27828.19, 27836.63,
    27845.08, 27853.52, 27861.96, 27870.40, 27878.84, 27887.28, 27895.73, 27904.17, 27912.61, 27921.05, 27929.49, 27937.93,
    27946.38, 27954.82, 27963.26, 27971.70, 27980.14, 27988.58, 27997.03, 28005.47, 28013.91, 28022.35, 28030.79, 28039.23,
    28047.68, 28056.12, 28064.56, 28073.00, 28081.44, 28089.88, 28098.33, 28106.77, 28115.21, 28123.65, 28132.09, 28140.53,
    28148.98, 28157.42, 28165.86, 28174.30, 28182.74, 28191.18, 28199.63, 28208.07, 28216.51, 28224.95, 28233.39, 28241.83,
    28250.28, 28258.72, 28267.16, 28275.60, 28284.04, 28292.48, 28300.93, 28309.37, 28317.81, 28326.25, 28334.69, 28343.13,
    28351.58, 28360.02, 28368.46, 28376.90, 28385.34, 28393.78, 28402.23, 28410.67, 28419.11, 28427.55, 28435.99, 28444.43,
    28452.88, 28461.32, 28469.76, 28478.20, 28486.64, 28495.08, 28503.53, 28511.97, 28520.41, 28528.85, 28537.29, 28545.73,
    28554.18, 28562.62, 28571.06, 28579.50, 28587.94, 28596.38, 28604.83, 28613.27, 28621.71, 28630.15, 28638.59, 28647.03,
    28655.48, 28663.92, 28672.36, 28680.80, 28689.24, 28697.68, 28706.13, 28714.57, 28723.01, 28731.45, 28739.89, 28748.33,
    28756.78, 28765.22, 28773.66, 28782.10, 28790.54, 28798.98, 28807.43, 28815.87, 28824.31, 28832.75, 28841.19, 28849.63,
    28858.08, 28866.52, 28874.96, 28883.40, 28891.84, 28900.28, 28908.73, 28917.17, 28925.61, 28934.05, 28942.49, 28950.93,
    28959.38, 28967.82, 28976.26, 28984.70, 28993.14, 29001.58, 29010.03, 29018.47, 29026.91, 29035.35, 29043.79, 29052.23,
    29060.68, 29069.12, 29077.56, 29086.00, 29094.44, 29102.88, 29111.33, 29119.77, 29128.21, 29136.65, 29145.09, 29153.53,
    29161.98, 29170.42, 29178.86, 29187.30, 29195.74, 29204.18, 29212.63, 29221.07, 29229.51, 29237.95, 29246.39, 29254.83,
    29263.28, 29271.72, 29280.16, 29288.60, 29297.04, 29305.48, 29313.93, 29322.37, 29330.81, 29339.25, 29347.69, 29356.13,
    29364.58, 29373.02, 29381.46, 29389.90, 29398.34, 29406.78, 29415.23, 29423.67, 29432.11, 29440.55, 29448.99, 29457.43,
    29465.88, 29474.32, 29482.76, 29491.20,
];

#[cfg(test)]
mod sound_tests {
    use super::*;

    /// ~0.1 s of NTSC machine cycles — enough audio for a stable pitch measure.
    const TENTH_SEC: usize = 102_272;

    /// Clocks the chip `cycles` machine cycles (via `tick`, which clocks sound)
    /// with inert video callbacks, and drains the produced host samples.
    fn render_audio(vic: &mut Vic6560, cycles: usize) -> Vec<f32> {
        for _ in 0..cycles {
            vic.tick(|_| 0, |_| 0, |_| 0);
        }
        vic.take_audio()
    }

    fn peak_to_peak(samples: &[f32]) -> f32 {
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for &s in samples {
            lo = lo.min(s);
            hi = hi.max(s);
        }
        if samples.is_empty() { 0.0 } else { hi - lo }
    }

    fn zero_crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|w| (w[0] <= 0.0) != (w[1] <= 0.0))
            .count()
    }

    #[test]
    fn silent_at_power_on() {
        let mut vic = Vic6560::new(false);
        let out = render_audio(&mut vic, TENTH_SEC);
        assert!(
            !out.is_empty(),
            "the chip should still emit samples when silent"
        );
        assert!(
            peak_to_peak(&out) < 1e-3,
            "no enabled voice should be silent, got p2p {}",
            peak_to_peak(&out)
        );
    }

    #[test]
    fn volume_zero_mutes_an_enabled_tone() {
        let mut vic = Vic6560::new(false);
        vic.write(0x0A, 0xE0); // bass voice enabled, mid pitch
        vic.write(0x0E, 0x00); // master volume 0
        let out = render_audio(&mut vic, TENTH_SEC);
        assert!(
            peak_to_peak(&out) < 1e-3,
            "volume 0 must mute an enabled voice, got p2p {}",
            peak_to_peak(&out)
        );
    }

    #[test]
    fn enabled_tone_is_audible_and_oscillates() {
        let mut vic = Vic6560::new(false);
        vic.write(0x0A, 0xE0); // bass voice enabled
        vic.write(0x0E, 0x0F); // full volume
        let out = render_audio(&mut vic, TENTH_SEC);
        assert!(
            peak_to_peak(&out) > 0.01,
            "an enabled tone should be audible, got p2p {}",
            peak_to_peak(&out)
        );
        assert!(
            zero_crossings(&out) > 10,
            "a tone should oscillate, got {} zero-crossings",
            zero_crossings(&out)
        );
    }

    #[test]
    fn higher_frequency_register_raises_pitch() {
        // reload = (~reg) & 127, so a larger register value → smaller reload →
        // the voice clocks faster → higher pitch. Compare zero-crossing rates.
        let mut low = Vic6560::new(false);
        low.write(0x0C, 0x90);
        low.write(0x0E, 0x0F);
        let low_x = zero_crossings(&render_audio(&mut low, TENTH_SEC));

        let mut high = Vic6560::new(false);
        high.write(0x0C, 0xF0);
        high.write(0x0E, 0x0F);
        let high_x = zero_crossings(&render_audio(&mut high, TENTH_SEC));

        assert!(
            high_x > low_x,
            "raising the frequency register must raise pitch: low={low_x} high={high_x}"
        );
    }

    #[test]
    fn noise_voice_is_audible() {
        let mut vic = Vic6560::new(false);
        vic.write(0x0D, 0xF0); // noise voice enabled
        vic.write(0x0E, 0x0F);
        let out = render_audio(&mut vic, TENTH_SEC);
        assert!(
            peak_to_peak(&out) > 0.01,
            "the noise voice should be audible, got p2p {}",
            peak_to_peak(&out)
        );
    }
}
