//! ZX80 video: a split bus and a refresh trick, not a ULA.
//!
//! The ZX80 has no custom video chip. It has a 74LS373 character latch, a
//! pair of 74LS157 multiplexers, and some open-collector inverters, and the
//! CPU does the timing itself. This models that rather than borrowing the
//! ZX81's ULA, which is what
//! `reference/by-system/sinclair-zx80/zx80-video-hardware-reference.md` §5
//! says to do — in terms: *"Do not model a ULA. There isn't one."*
//!
//! ## How a picture happens
//!
//! 1. The ROM points the program counter at the display file with bit 15
//!    set. When the Z80 fetches an opcode from there, the inverters force
//!    `$00` — a `NOP` — onto the **CPU side** of the data bus, while the
//!    **memory side** still carries the real byte. A single read serves two
//!    masters: the CPU marches on, and the byte is latched as a character
//!    code.
//! 2. The ZX80 uses static RAM, so the Z80's refresh cycles are spare. When
//!    `/REFRESH` goes low the multiplexers take the ROM address lines away
//!    from the CPU and compose one instead:
//!
//!    | ROM address bits | Source |
//!    | --- | --- |
//!    | `A0`-`A2` | 3-bit line counter — which row of the character |
//!    | `A3`-`A8` | 6-bit character code, from the latch |
//!    | `A9`-`A12` | 4 bits of the refresh address, pointing at the bitmaps |
//!
//! 3. The byte fetched is shifted out over the next 8 pixel clocks — which
//!    is exactly as long as the forced `NOP` takes to execute. That timing
//!    is the design, not a coincidence.
//!
//! A byte with bit 6 set is *not* forced: `$76` is `HALT`, and executing it
//! is how a line ends. That is the whole line-termination mechanism.
//!
//! ## What follows from this
//!
//! **Video exists only while the CPU is executing forced NOPs.** When it
//! runs real code the picture goes, which is why a ZX80 blanks during
//! input, calculation and tape. That is not an artefact to tolerate; it is
//! what using one was like, and a renderer that draws from the display file
//! once a frame cannot reproduce it.
//!
//! Vertical sync is software too: `IN` with A0 low starts it, `OUT` stops
//! it. There is no timing chip to ask.

/// Framebuffer geometry, matching what the runtime and UI already expect.
pub const FB_WIDTH: u32 = 320;
pub const FB_HEIGHT: u32 = 240;

const TSTATES_PER_LINE: u32 = 207;
const LINES_PER_FRAME: u32 = 312;

/// Two pixels per T-state: the 6.5 MHz pixel clock against a 3.25 MHz CPU.
/// A character is 8 pixels, so it occupies 4 T-states — the length of the
/// forced `NOP` that fetched it.
const PIXELS_PER_TSTATE: u32 = 2;

const PAPER: u32 = 0xFFFF_FFFF;
const INK: u32 = 0xFF00_0000;

/// The ZX80's video hardware: a latch, a line counter, and a beam.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Zx80Video {
    framebuffer: Vec<u32>,
    /// T-states since the current display line began. The horizontal
    /// position; the vertical one is counted, not clocked.
    tstate: u32,
    /// Which display line is being drawn, counted by line syncs rather than
    /// derived from a T-state schedule.
    ///
    /// This is the difference between a ZX80 and a machine with a video
    /// chip. There is no beam running to a timetable: the CPU emits one
    /// line, ends it with a `HALT`, and the interrupt that releases the
    /// `HALT` is the line sync. Counting T-states instead puts the picture
    /// wherever the ROM's housekeeping happens to leave the clock — 232
    /// lines down the frame, in the case that led to this.
    display_line: u32,
    /// 74LS373. Holds the character code the last display fetch put on the
    /// memory side of the bus.
    char_latch: u8,
    /// The ULA's job on a ZX81; here a 3-bit counter advanced by each line
    /// sync, selecting which of the eight rows of the character is shown.
    line_counter: u8,
    /// Whether the software vertical sync is currently asserted.
    vsync: bool,
    frame_complete: bool,
    /// Pixels painted this frame. A machine running real code paints none,
    /// which is the property that distinguishes a ZX80 from a ZX81.
    painted: u32,
    /// Set by a forced `NOP`, consumed by the refresh cycle that follows it.
    ///
    /// Every M1 has a refresh cycle, but only a display fetch loads the
    /// latch. Without this the latch's stale contents would be shifted out
    /// during ordinary code, smearing the last character across the screen.
    pending: bool,
    pub dbg_forced: u32,
    pub dbg_refresh: u32,
    pub dbg_paint_calls: u32,
    pub dbg_min_line: u32,
    pub dbg_max_line: u32,
    pub dbg_min_x: u32,
    pub dbg_max_x: u32,
    pub dbg_vsync_start: u32,
    pub dbg_vsync_stop: u32,
    pub dbg_overflow: u32,
    pub dbg_events: Vec<(char, u32)>,
}

impl Zx80Video {
    #[must_use]
    pub fn new() -> Self {
        Self {
            framebuffer: vec![PAPER; (FB_WIDTH * FB_HEIGHT) as usize],
            tstate: 0,
            display_line: 0,
            char_latch: 0,
            line_counter: 0,
            vsync: false,
            frame_complete: false,
            painted: 0,
            pending: false,
            dbg_forced: 0,
            dbg_refresh: 0,
            dbg_paint_calls: 0,
            dbg_min_line: u32::MAX,
            dbg_max_line: 0,
            dbg_min_x: u32::MAX,
            dbg_max_x: 0,
            dbg_vsync_start: 0,
            dbg_vsync_stop: 0,
            dbg_overflow: 0,
            dbg_events: Vec::new(),
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    #[must_use]
    pub fn painted_pixels(&self) -> u32 {
        self.painted
    }

    /// Advance one T-state of the current display line.
    pub fn tick(&mut self) {
        self.tstate += 1;
    }

    /// A display line has ended — the `HALT` that terminates it has been
    /// released by the interrupt wired to A6.
    ///
    /// This is the line sync: it advances the vertical position and the
    /// 3-bit counter selecting which row of the character set is shown, and
    /// restarts the horizontal position.
    pub fn line_sync(&mut self) {
        self.display_line += 1;
        self.log('H', 0);
        self.line_counter = (self.line_counter + 1) & 0x07;
        self.tstate = 0;
        if self.display_line >= LINES_PER_FRAME {
            self.dbg_overflow += 1;
            self.end_frame();
        }
    }

    /// An `IN` with A0 low: start the vertical sync, and with it the frame.
    pub fn vsync_start(&mut self) {
        self.dbg_vsync_start += 1;
        self.log('I', self.tstate);
        self.vsync = true;
    }

    /// An `OUT`: stop the vertical sync. The frame ends here on a real
    /// machine, which is why the rate is a software constant.
    pub fn vsync_stop(&mut self) {
        self.dbg_vsync_stop += 1;
        self.log('O', self.tstate);
        if self.vsync {
            self.vsync = false;
            self.end_frame();
        }
        self.line_counter = 0;
    }

    fn log(&mut self, kind: char, _tstate: u32) {
        if self.dbg_events.len() < 400 {
            self.dbg_events.push((kind, self.display_line));
        }
    }

    fn end_frame(&mut self) {
        self.tstate = 0;
        self.display_line = 0;
        self.frame_complete = true;
    }

    pub fn take_frame_complete(&mut self) -> bool {
        std::mem::replace(&mut self.frame_complete, false)
    }

    /// Begin a new frame's picture. Called when the frame is consumed, so a
    /// frame in which the CPU never entered the display routine comes out
    /// blank rather than holding the previous picture.
    pub fn clear(&mut self) {
        self.framebuffer.fill(PAPER);
        self.painted = 0;
        self.pending = false;
    }

    /// An opcode fetch. Returns `Some(0x00)` when the inverters force a
    /// `NOP`, having latched the real byte as a character code.
    ///
    /// The condition is the hardware's: the address has A15 set, and the
    /// byte has bit 6 clear. `$76` (`HALT`) has bit 6 set and executes
    /// normally, which is how a display line ends.
    pub fn opcode_fetch(&mut self, addr: u16, byte: u8) -> Option<u8> {
        if addr & 0x8000 == 0 || byte & 0x40 != 0 {
            return None;
        }
        self.char_latch = byte;
        self.pending = true;
        self.dbg_forced += 1;
        Some(0x00)
    }

    /// The refresh cycle that follows a forced `NOP`. `refresh_addr` is the
    /// address the Z80 puts out during `/REFRESH` — `I` in the high byte —
    /// from which the multiplexers take `A9`-`A12`.
    ///
    /// `read_rom` fetches the composed address. The eight pixels are painted
    /// at the beam, which is where the shift register would have clocked
    /// them out.
    pub fn refresh(&mut self, refresh_addr: u16, read_rom: impl Fn(u16) -> u8) {
        self.dbg_refresh += 1;
        if !std::mem::take(&mut self.pending) {
            return;
        }
        self.dbg_paint_calls += 1;
        self.log('P', self.tstate);
        let code = u16::from(self.char_latch);
        let inverse = self.char_latch & 0x80 != 0;
        let addr =
            (refresh_addr & 0x1E00) | ((code & 0x3F) << 3) | u16::from(self.line_counter & 0x07);
        let mut pattern = read_rom(addr);
        if inverse {
            pattern = !pattern;
        }
        self.paint(pattern);
    }

    fn paint(&mut self, pattern: u8) {
        let line = self.display_line;
        let line_tstate = self.tstate;
        self.dbg_min_line = self.dbg_min_line.min(line);
        self.dbg_max_line = self.dbg_max_line.max(line);
        self.dbg_min_x = self.dbg_min_x.min(line_tstate * PIXELS_PER_TSTATE);
        self.dbg_max_x = self.dbg_max_x.max(line_tstate * PIXELS_PER_TSTATE);
        let Some(y) = line.checked_sub(FIRST_VISIBLE_LINE) else {
            return;
        };
        if y >= FB_HEIGHT {
            return;
        }
        let x0 = line_tstate * PIXELS_PER_TSTATE;
        let Some(x0) = x0.checked_sub(FIRST_VISIBLE_PIXEL) else {
            return;
        };
        for bit in 0..8u32 {
            let x = x0 + bit;
            if x >= FB_WIDTH {
                break;
            }
            let lit = pattern & (0x80 >> bit) != 0;
            let index = (y * FB_WIDTH + x) as usize;
            self.framebuffer[index] = if lit { INK } else { PAPER };
            self.painted += 1;
        }
    }
}

/// Where the visible window starts. Calibrated against the real ROM rather
/// than derived: the boot screen's cursor must land where a ZX80's does.
const FIRST_VISIBLE_LINE: u32 = 0;
const FIRST_VISIBLE_PIXEL: u32 = 0;

impl Default for Zx80Video {
    fn default() -> Self {
        Self::new()
    }
}
