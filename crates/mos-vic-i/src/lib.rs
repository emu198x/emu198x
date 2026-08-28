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

/// Pixel clock of the 6560 (NTSC): **four** pixels per machine cycle.
///
/// This said eight, reasoning that the VIC fetches one character of eight
/// pixels per cycle, and checked itself against the line time — which the
/// eight cannot fail, because doubling the pixel count and the clock together
/// leaves the microseconds unchanged. VICE settles it in one comment:
///
/// ```text
/// #define VIC_NTSC_SCREEN_WIDTH  260   /* 65 cycles * 4 pixels */
/// #define VIC_PAL_SCREEN_WIDTH   284   /* 71 cycles * 4 pixels */
/// ```
///
/// A character therefore spans two machine cycles, not one, and 22 columns
/// occupy 44 of the 65 or 71 — which is the VIC-20's familiar wide border,
/// rather than the display filling barely a third of the line that the eight
/// implied.
///
/// The second check is the published pixel aspect. `vic_get_pixel_aspect` in
/// VICE's `vic20/vic.c` gives PAL 1.66574035 and NTSC 1.50411479, citing
/// codebase64 (it halves both for its own doubled rendering, which this
/// framebuffer does not do). Against
/// `knowledge/decisions/pixel-aspect-comes-from-the-raster.md` these clocks
/// give 1.6656 and 1.5000; the old ones gave exactly half each.
///
/// The claim that the VIC-20 and the C64 share a pixel shape on NTSC went with
/// it. A VIC-20 pixel is twice as wide.
pub const NTSC_PIXEL_CLOCK_HZ: f64 = 4_090_908.0;

/// Pixel clock of the 6561 (PAL) — the 4.4336 MHz colour subcarrier, which is
/// the master oscillator divided by four. See [`NTSC_PIXEL_CLOCK_HZ`].
pub const PAL_PIXEL_CLOCK_HZ: f64 = 4_433_620.0;

/// The active display: 22 x 23 characters of 8 x 8 pixels.
/// The display a stock VIC-20 puts up: 22 columns by 23 rows of 8 x 8.
///
/// The *default*, not the model. Registers 2 and 3 set the column and row
/// counts and register 3's bit 0 doubles the character height, so a program
/// can make the display any size it likes up to the raster. These two survive
/// because the framebuffer is still defined as the stock display plus a border
/// either side; nothing is clipped to them. See [`Vic6560::tick`].
pub const ACTIVE_WIDTH: u32 = 176;
pub const ACTIVE_HEIGHT: u32 = 184;

/// Pixels the VIC emits per machine cycle.
///
/// The VIC generates the CPU's phi2, so a cycle is a unit of the raster as
/// well as of time: 71 of them make a PAL line and 65 an NTSC one, four pixels
/// each. Register 0's horizontal origin counts in cycles for the same reason —
/// MAME's `mos6560.cpp` has `XPOS (((int)m_reg[0] & 0x7f) * 4)`.
pub const PIXELS_PER_CYCLE: u32 = 4;

/// Scan lines a set displays, which is the framebuffer's height.
///
/// Per `knowledge/decisions/the-framebuffer-is-the-sets-window.md`. This used
/// to be a fixed 16 lines of border either side of the active 184, giving 216
/// — 75% of a PAL field, and the comment said where it came from: "VICE
/// typically render ~30-40 px of border each side... a clean approximation
/// that matches the period look".
#[must_use]
pub const fn framebuffer_height(pal: bool) -> u32 {
    if pal { 288 } else { 240 }
}

/// Pixels a set displays along a line, which is the framebuffer's width.
///
/// A PAL line is 71 cycles of 4 pixels — 284 in 64.06 µs — and a set shows
/// about 52 µs of it, so 230. NTSC is 65 cycles, 260 pixels in 63.55 µs, so
/// 213. Rounded to leave a whole border either side of the active 176.
#[must_use]
pub const fn framebuffer_width(pal: bool) -> u32 {
    ACTIVE_WIDTH + 2 * border_left(pal)
}

/// What the window has left over either side of a *stock* display.
///
/// A position the KERNAL's defaults happen to produce, not a boundary. The
/// display's real position comes from registers 0 and 1, so a program that
/// moves the screen moves it; this is kept because the window's width is
/// defined as the stock display plus a border either side.
#[must_use]
pub const fn border_left(pal: bool) -> u32 {
    if pal { 27 } else { 19 }
}

/// The same above and below, and the same caveat.
#[must_use]
pub const fn border_top(pal: bool) -> u32 {
    (framebuffer_height(pal) - ACTIVE_HEIGHT) / 2
}

/// The raster pixel the framebuffer's first pixel sits on.
///
/// The two parts number their lines from different points relative to sync, so
/// this is not derivable from the line length the way the vertical one is.
/// MAME's `mos6560.h` states both: the 6560's buffer is `(4+201)` with "4 left
/// not visible" and the 6561's `(20+229)` with "20 left not visible".
///
/// The check is that the KERNAL's own origin lands the display where a set
/// centres it. Register 0 defaults to 12 on PAL, so the display starts at
/// pixel 48 — 28 into a 230-pixel window, leaving 26 the other side. NTSC
/// defaults to 5, so pixel 20, 16 into a 214-pixel window.
#[must_use]
pub const fn window_first_pixel(pal: bool) -> u32 {
    if pal { 20 } else { 4 }
}

/// The scan line the framebuffer's first line sits on.
///
/// Unlike the horizontal case this needs no outside figure: the lines a set
/// hides are the ones the frame has over the field, and on this chip they fall
/// at the top, before the picture. 312 less 288 is 24 on PAL and 261 less 240
/// is 21 on NTSC.
///
/// The same check applies and passes. Register 1 defaults to 38, two scan
/// lines to the unit, so a PAL display starts on line 76 — 52 into the window,
/// which is exactly [`border_top`]. NTSC defaults to 25, line 50, 29 into the
/// window against a centred 28.
#[must_use]
pub const fn window_first_line(pal: bool) -> u32 {
    lines_per_frame(pal) - framebuffer_height(pal)
}

/// Scan lines in a whole frame, picture and blanking together.
#[must_use]
pub const fn lines_per_frame(pal: bool) -> u32 {
    if pal { 312 } else { 261 }
}

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
    /// Width of that framebuffer, which the region decides.
    fb_width: u32,
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

/// What the registers say about the scan line being drawn.
///
/// The colours come in fours because a multicolour cell draws from four, and
/// three of them are shared by the whole line — only the ink is per cell.
struct LineState {
    origin_x: u32,
    columns: u32,
    char_height: u32,
    /// The line's offset into the display, or `None` when the beam is above or
    /// below it.
    row: Option<u32>,
    screen_base: u16,
    char_rom_base: u16,
    /// Register 15 bits 2-0. Also multicolour's `01`.
    border: u32,
    /// Register 15 bits 7-4.
    background: u32,
    /// Register 14 bits 7-4, and only multicolour uses it.
    auxiliary: u32,
    /// Register 15 bit 3 **clear**. The VIC-20 powers up with it set.
    reverse: bool,
}

impl LineState {
    /// `normal` under reverse video, `reversed` otherwise.
    ///
    /// Reverse swaps ink and paper, and in a multicolour cell it swaps the
    /// same pair — `00` and `10` trade places while the border and auxiliary
    /// colours stay put. MAME's `m_multiinverted` differs from `m_multi` in
    /// exactly those two entries.
    const fn reversible(&self, normal: u32, reversed: u32) -> u32 {
        if self.reverse { normal } else { reversed }
    }
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
            framebuffer: vec![
                0xFF00_0000;
                (framebuffer_width(pal) * framebuffer_height(pal)) as usize
            ],
            fb_width: framebuffer_width(pal),
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
        match reg {
            // The raster counter is split across these two registers. VICE's
            // `vic_read` and the MiSTer m6561 core both expose bit 0 in
            // register 3 bit 7 and the remaining bits in register 4.
            0x03 => ((self.scanline as u8 & 1) << 7) | (self.regs[reg] & 0x7F),
            0x04 => (self.scanline >> 1) as u8,
            _ => self.regs[reg],
        }
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

        self.render_cycle(&read_screen, &read_colour, &read_char_rom);

        false
    }

    /// Paint the four pixels this machine cycle covers.
    ///
    /// Every pixel of the window is written every line — border outside the
    /// display, character pixels inside — rather than the display alone over a
    /// border painted once a frame. Two reasons. The display's position is the
    /// registers' to decide, so there is no fixed rectangle to paint around;
    /// and a border colour written partway down a frame has to land there,
    /// which is the ordinary way a VIC-20 program draws a raster bar.
    ///
    /// Register decode follows MAME's `mos6560.cpp`: `XPOS` is register 0's
    /// low seven bits times four pixels, `YPOS` register 1 times two scan
    /// lines, `CHARS_X` register 2's low seven bits, `CHARS_Y` register 3 bits
    /// 1-6, and register 3 bit 0 doubles the character height. `FRAMECOLOR` is
    /// register 15's low **three** bits — bit 3 is the reverse-video flag, and
    /// reading four bits made a program that toggled reverse change the border
    /// colour with it.
    fn render_cycle(
        &mut self,
        read_screen: &impl Fn(u16) -> u8,
        read_colour: &impl Fn(u16) -> u8,
        read_char_rom: &impl Fn(u16) -> u8,
    ) {
        let pal = self.lines_per_frame == lines_per_frame(true);
        let fb_width = self.fb_width;

        let Some(fb_y) = self.scanline.checked_sub(window_first_line(pal)) else {
            return;
        };
        if fb_y >= framebuffer_height(pal) {
            return;
        }

        let line = self.line_state();
        let row_base = fb_y * fb_width;
        for offset in 0..PIXELS_PER_CYCLE {
            let raster_x = self.pixel_x * PIXELS_PER_CYCLE + offset;
            let Some(fb_x) = raster_x.checked_sub(window_first_pixel(pal)) else {
                continue;
            };
            if fb_x >= fb_width {
                continue;
            }

            let colour = self
                .display_pixel(raster_x, &line, read_screen, read_colour, read_char_rom)
                .unwrap_or(line.border);
            self.framebuffer[(row_base + fb_x) as usize] = colour;
        }
    }

    /// Everything the registers say about the scan line being drawn.
    ///
    /// Read once per cycle rather than once per pixel, and gathered rather
    /// than passed around: the pixel path needs eleven of these and a
    /// parameter list that long stops being readable.
    fn line_state(&self) -> LineState {
        let colours = self.regs[0x0F];
        let origin_y = u32::from(self.regs[1]) * 2;
        let rows = u32::from((self.regs[3] & 0x7E) >> 1);
        let char_height = if self.regs[3] & 0x01 == 0 { 8 } else { 16 };

        LineState {
            origin_x: u32::from(self.regs[0] & 0x7F) * PIXELS_PER_CYCLE,
            columns: u32::from(self.regs[2] & 0x7F),
            char_height,
            row: self
                .scanline
                .checked_sub(origin_y)
                .filter(|y| *y < rows * char_height),
            screen_base: (u16::from(self.regs[5]) & 0xF0) << 6
                | (u16::from(self.regs[2]) & 0x80) << 2,
            char_rom_base: (u16::from(self.regs[5]) & 0x0F) << 10,
            border: VIC_PALETTE[(colours & 0x07) as usize],
            background: VIC_PALETTE[(colours >> 4) as usize],
            auxiliary: VIC_PALETTE[(self.regs[0x0E] >> 4) as usize],
            reverse: colours & 0x08 == 0,
        }
    }

    /// The character pixel at raster position `raster_x`, or `None` when the
    /// beam is outside the display and the border shows instead.
    fn display_pixel(
        &self,
        raster_x: u32,
        line: &LineState,
        read_screen: &impl Fn(u16) -> u8,
        read_colour: &impl Fn(u16) -> u8,
        read_char_rom: &impl Fn(u16) -> u8,
    ) -> Option<u32> {
        let row = line.row?;
        let x = raster_x.checked_sub(line.origin_x)?;
        if x >= line.columns * 8 {
            return None;
        }

        let char_row = row / line.char_height;
        let cell = line
            .screen_base
            .wrapping_add((char_row * line.columns + x / 8) as u16);
        let attribute = read_colour(cell);
        let ink = VIC_PALETTE[(attribute & 0x07) as usize];

        let glyph = line.char_rom_base.wrapping_add(
            u16::from(read_screen(cell)) * line.char_height as u16
                + (row % line.char_height) as u16,
        );
        let bits = read_char_rom(glyph);
        let dot = x % 8;

        // Colour RAM bit 3 puts the cell in multicolour: two bits a pixel,
        // four pixels a byte, each two dots wide, drawing from four colours
        // instead of two. Reading the bit as part of the colour index — which
        // is what this did until #1091 — rendered every such cell as a solid
        // hi-res glyph somewhere in the bright half of the palette.
        if attribute & 0x08 != 0 {
            let pair = (bits >> (6 - (dot / 2) * 2)) & 0x03;
            return Some(match pair {
                0 => line.reversible(ink, line.background),
                1 => line.border,
                2 => line.reversible(line.background, ink),
                _ => line.auxiliary,
            });
        }

        Some(if bits >> (7 - dot) & 1 != 0 {
            line.reversible(line.background, ink)
        } else {
            line.reversible(ink, line.background)
        })
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

    /// Framebuffer width — the window this chip's region displays.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.fb_width
    }

    /// Framebuffer height, read off the buffer so it cannot disagree with it.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.framebuffer.len() as u32 / self.fb_width
    }

    /// Border either side of the active area.
    #[must_use]
    pub fn border_left(&self) -> u32 {
        (self.fb_width - ACTIVE_WIDTH) / 2
    }

    /// Border above the active area.
    #[must_use]
    pub fn border_top(&self) -> u32 {
        (self.framebuffer_height() - ACTIVE_HEIGHT) / 2
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const PAL: bool = true;

    /// The KERNAL's own register values: origin 12/38, 22 columns of 23 rows,
    /// screen and character generator at zero, background 1, border 3.
    fn stock(vic: &mut Vic6560) {
        vic.write(0x00, 12);
        vic.write(0x01, 38);
        vic.write(0x02, 22);
        vic.write(0x03, 23 << 1);
        vic.write(0x05, 0x00);
        vic.write(0x0F, 0x1B);
    }

    /// Every cell holds character 1 in colour 2, and character 1's glyph is
    /// solid at both character heights — so a display pixel is colour 2, paper
    /// is colour 1 and border is colour 3, all distinguishable.
    fn run_frame(vic: &mut Vic6560) {
        let cycles = 71 * lines_per_frame(PAL);
        for _ in 0..cycles {
            vic.tick(
                |_| 1,
                |_| 2,
                |addr| if (8..32).contains(&addr) { 0xFF } else { 0x00 },
            );
        }
    }

    /// The same, with the colour RAM and character generator supplied — the
    /// two the multicolour tests need to vary.
    fn run_frame_with(
        vic: &mut Vic6560,
        colour: impl Fn(u16) -> u8 + Copy,
        char_rom: impl Fn(u16) -> u8 + Copy,
    ) {
        for _ in 0..71 * lines_per_frame(PAL) {
            vic.tick(|_| 1, colour, char_rom);
        }
    }

    fn pixel(vic: &Vic6560, x: u32, y: u32) -> u32 {
        vic.framebuffer()[(y * vic.framebuffer_width() + x) as usize]
    }

    /// Where [`stock`] puts the display's top-left pixel, derived from the
    /// registers it writes rather than from the border constants.
    ///
    /// It is 28 across and 52 down on PAL. The vertical figure is exactly
    /// [`border_top`]; the horizontal one is a pixel right of [`border_left`],
    /// because the KERNAL's origin is where it is and the window's is where
    /// MAME says. A pixel is not worth fitting a constant to hide.
    fn stock_origin() -> (u32, u32) {
        (
            12 * PIXELS_PER_CYCLE - window_first_pixel(PAL),
            38 * 2 - window_first_line(PAL),
        )
    }

    /// #361: registers 3 and 4 expose the live raster counter, not the bytes
    /// last written to them. Register 4 advances once for every two lines.
    #[test]
    fn raster_registers_follow_the_live_scanline() {
        let mut vic = Vic6560::new(PAL);
        vic.write(0x03, 0x2F);
        vic.write(0x04, 0xFF);

        assert_eq!(vic.read(0x03), 0x2F, "line 0 is even");
        assert_eq!(vic.read(0x04), 0, "the stored register 4 byte is ignored");

        for _ in 0..vic.cycles_per_line {
            vic.tick(|_| 0, |_| 0, |_| 0);
        }
        assert_eq!(vic.read(0x03), 0xAF, "line 1 sets the raster low bit");
        assert_eq!(vic.read(0x04), 0, "the divided raster has not advanced");

        for _ in 0..vic.cycles_per_line {
            vic.tick(|_| 0, |_| 0, |_| 0);
        }
        assert_eq!(vic.read(0x03), 0x2F, "line 2 clears the raster low bit");
        assert_eq!(vic.read(0x04), 1, "the divided raster advances on line 2");
    }

    /// #1087: the display was drawn at a fixed border offset and the origin
    /// registers were never read, so a program could not move the screen.
    #[test]
    fn the_origin_registers_move_the_display() {
        let mut vic = Vic6560::new(PAL);
        stock(&mut vic);
        vic.write(0x00, 12 + 4); // four cycles right — sixteen pixels
        vic.write(0x01, 38 + 5); // five units down — ten scan lines
        run_frame(&mut vic);

        let (x0, y0) = stock_origin();
        let (x, y) = (x0 + 16, y0 + 10);
        assert_eq!(
            pixel(&vic, x, y),
            VIC_PALETTE[2],
            "the display moved with the registers"
        );
        assert_eq!(
            pixel(&vic, x - 1, y),
            VIC_PALETTE[3],
            "and the border followed it across"
        );
        assert_eq!(pixel(&vic, x, y - 1), VIC_PALETTE[3], "and down");
    }

    /// The check that the window's own origin is right: with the KERNAL's
    /// values the picture has to land where a set centres it, which is
    /// `border_left` and `border_top` by construction.
    #[test]
    fn the_stock_registers_land_the_display_on_the_stock_border() {
        let mut vic = Vic6560::new(PAL);
        stock(&mut vic);
        run_frame(&mut vic);

        let (x, y) = stock_origin();
        assert_eq!(pixel(&vic, x, y), VIC_PALETTE[2], "first display pixel");
        assert_eq!(pixel(&vic, x - 1, y), VIC_PALETTE[3], "border left of it");
        assert_eq!(
            pixel(&vic, x + ACTIVE_WIDTH - 1, y + ACTIVE_HEIGHT - 1),
            VIC_PALETTE[2],
            "last display pixel"
        );
        assert_eq!(
            pixel(&vic, x + ACTIVE_WIDTH, y + ACTIVE_HEIGHT - 1),
            VIC_PALETTE[3],
            "border right of it"
        );
    }

    #[test]
    fn registers_2_and_3_set_how_many_columns_and_rows_there_are() {
        let mut vic = Vic6560::new(PAL);
        stock(&mut vic);
        vic.write(0x02, 10); // ten columns — eighty pixels
        vic.write(0x03, 5 << 1); // five rows — forty scan lines
        run_frame(&mut vic);

        let (x, y) = stock_origin();
        assert_eq!(pixel(&vic, x + 79, y), VIC_PALETTE[2], "last column shows");
        assert_eq!(pixel(&vic, x + 80, y), VIC_PALETTE[3], "and stops there");
        assert_eq!(pixel(&vic, x, y + 39), VIC_PALETTE[2], "last row shows");
        assert_eq!(pixel(&vic, x, y + 40), VIC_PALETTE[3], "and stops there");
    }

    #[test]
    fn register_3_bit_0_doubles_the_character_height() {
        // Character 1's glyph is solid over both an 8-byte and a 16-byte
        // stride, so only the row count separates the two: five rows of 16 is
        // eighty scan lines where five rows of 8 is forty.
        let mut vic = Vic6560::new(PAL);
        stock(&mut vic);
        vic.write(0x03, (5 << 1) | 1);
        run_frame(&mut vic);

        let (x, y) = stock_origin();
        assert_eq!(pixel(&vic, x, y + 79), VIC_PALETTE[2], "eighty lines tall");
        assert_eq!(pixel(&vic, x, y + 80), VIC_PALETTE[3], "and no taller");
    }

    /// Register 15 bit 3 is the reverse-video flag. Reading four bits for the
    /// border colour made toggling reverse change the border with it — MAME's
    /// `FRAMECOLOR` is `m_reg[0x0f] & 0x07`.
    #[test]
    fn reverse_video_does_not_change_the_border_colour() {
        let mut plain = Vic6560::new(PAL);
        stock(&mut plain);
        run_frame(&mut plain);

        let mut reversed = Vic6560::new(PAL);
        stock(&mut reversed);
        reversed.write(0x0F, 0x13); // same colours, bit 3 clear
        run_frame(&mut reversed);

        assert_eq!(
            pixel(&plain, 0, border_top(PAL)),
            pixel(&reversed, 0, border_top(PAL)),
            "the border is the low three bits, and reverse is not one of them"
        );
        let (x, y) = stock_origin();
        assert_eq!(
            pixel(&reversed, x, y),
            VIC_PALETTE[1],
            "a lit pixel takes the paper colour under reverse video"
        );
    }

    /// The border is composited with the picture rather than painted once a
    /// frame, so a colour written partway down lands on that frame — which is
    /// how a VIC-20 raster bar works.
    #[test]
    fn a_mid_frame_border_write_splits_the_border() {
        let mut vic = Vic6560::new(PAL);
        stock(&mut vic);
        let half = 71 * lines_per_frame(PAL) / 2;
        for _ in 0..half {
            vic.tick(|_| 1, |_| 2, |_| 0x00);
        }
        vic.write(0x0F, 0x14); // border 4 from here down
        for _ in 0..half {
            vic.tick(|_| 1, |_| 2, |_| 0x00);
        }

        assert_eq!(pixel(&vic, 0, 10), VIC_PALETTE[3], "above the write");
        assert_eq!(
            pixel(&vic, 0, framebuffer_height(PAL) - 10),
            VIC_PALETTE[4],
            "below the write"
        );
    }

    /// #1091: colour RAM bit 3 puts a cell in multicolour. Reading it as part
    /// of the colour index drew the cell as a solid hi-res glyph somewhere in
    /// the bright half of the palette instead.
    #[test]
    fn a_multicolour_cell_draws_from_four_colours() {
        // 0b00_01_10_11 walks the four bit-pairs across one byte, so the four
        // pixels of the cell take one colour each.
        let mut vic = Vic6560::new(PAL);
        stock(&mut vic);
        vic.write(0x0E, 0x50); // auxiliary colour 5
        run_frame_with(&mut vic, |_| 0x08 | 2, |_| 0b00_01_10_11);

        let (x, y) = stock_origin();
        for (dot, expected) in [(0, 1), (2, 3), (4, 2), (6, 5)] {
            assert_eq!(
                pixel(&vic, x + dot, y),
                VIC_PALETTE[expected],
                "pair {} should take colour {expected}",
                dot / 2
            );
        }
    }

    #[test]
    fn a_multicolour_pixel_is_two_dots_wide() {
        let mut vic = Vic6560::new(PAL);
        stock(&mut vic);
        vic.write(0x0E, 0x50);
        run_frame_with(&mut vic, |_| 0x08 | 2, |_| 0b00_01_10_11);

        let (x, y) = stock_origin();
        for dot in [0, 2, 4, 6] {
            assert_eq!(
                pixel(&vic, x + dot, y),
                pixel(&vic, x + dot + 1, y),
                "dots {dot} and {} belong to one pixel",
                dot + 1
            );
        }
    }

    /// Reverse video swaps ink and paper in a multicolour cell too, and only
    /// those two: MAME's `m_multiinverted` differs from `m_multi` in entries 0
    /// and 2 and leaves the border and auxiliary colours where they are.
    #[test]
    fn reverse_video_swaps_only_the_ink_and_paper_of_a_multicolour_cell() {
        let mut vic = Vic6560::new(PAL);
        stock(&mut vic);
        vic.write(0x0E, 0x50);
        vic.write(0x0F, 0x13); // same colours as stock, bit 3 clear
        run_frame_with(&mut vic, |_| 0x08 | 2, |_| 0b00_01_10_11);

        let (x, y) = stock_origin();
        for (dot, expected) in [(0, 2), (2, 3), (4, 1), (6, 5)] {
            assert_eq!(
                pixel(&vic, x + dot, y),
                VIC_PALETTE[expected],
                "pair {} should take colour {expected} under reverse video",
                dot / 2
            );
        }
    }

    /// The shape of the old bug, stated directly: a cell with bit 3 set and
    /// ink 2 used to index the palette with 10.
    #[test]
    fn colour_ram_bit_3_never_reaches_the_palette() {
        let mut vic = Vic6560::new(PAL);
        stock(&mut vic);
        vic.write(0x0E, 0x50);
        run_frame_with(&mut vic, |_| 0x08 | 2, |_| 0xFF);

        assert!(
            !vic.framebuffer().contains(&VIC_PALETTE[0x0A]),
            "an attribute of $0A is multicolour with ink 2, not colour 10"
        );
    }

    #[test]
    fn the_window_holds_the_field_and_opens_where_blanking_ends() {
        for pal in [true, false] {
            assert_eq!(
                window_first_line(pal) + framebuffer_height(pal),
                lines_per_frame(pal),
                "the window and the blanking have to account for the whole frame"
            );
            // The KERNAL's vertical origin lands within a line of centred,
            // which is the check on `window_first_line`.
            let origin = if pal { 38 } else { 25 } * 2;
            let placed = origin - window_first_line(pal);
            assert!(
                placed.abs_diff(border_top(pal)) <= 1,
                "{pal} places the stock display at {placed}, not near {}",
                border_top(pal)
            );

            // Horizontally the check is weaker, because `window_first_pixel`
            // comes from MAME rather than from the frame's own arithmetic: the
            // stock display has to sit wholly inside the window with border
            // both sides. PAL lands a pixel right of centre, NTSC three left.
            let origin_x = if pal { 12 } else { 5 } * PIXELS_PER_CYCLE;
            let placed_x = origin_x - window_first_pixel(pal);
            assert!(
                placed_x > 0 && placed_x + ACTIVE_WIDTH < framebuffer_width(pal),
                "{pal} places the stock display at {placed_x}, not inside its window"
            );
        }
    }
}
