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
//!
//! ## The line ends before the `HALT` does
//!
//! Reaching the `NEWLINE` ends the line, but not the `HALT` it enters. The
//! Z80 goes on issuing M1 cycles until the interrupt arrives, with the
//! address bus holding the byte *after* the `HALT` — which is in the
//! display file, has A15 set, and is as likely as not a character code. So
//! the character generator has to be inhibited while `/HALT` is low, or
//! that one byte is latched and shifted out across the rest of the line.
//! Ungated, the boot screen's cursor came out repeated 33 times.
//!
//! The reference gives the forcing condition as A15 and D6 and does not
//! mention `/HALT`; this gate is what the real ROM's output requires.

/// Framebuffer width: 320 pixels, the 256-pixel display with 32 of border
/// either side.
///
/// **A little narrower than a set's window**, which at 6.5 MHz over 52.0 µs is
/// 338 — the #1054 audit reads it as 95%. Unlike the height, this cannot be
/// derived: `FIRST_CHAR_TSTATE` is a constant fitted to place the picture
/// inside a window that had already been chosen, so deriving a width from it
/// would be circular. Closing the gap needs a measurement against a reference
/// rather than arithmetic — MAME 0.289 puts its window 24 T-states earlier
/// than this, which is a starting point and not an answer.
///
/// See `knowledge/decisions/the-framebuffer-is-the-sets-window.md`, which
/// records the same open question for the height's sibling axis.
pub const FB_WIDTH: u32 = 320;
/// A PAL set displays 288 lines, and that is the whole of this figure: it is
/// the receiver's window, not a remainder of the ZX80's frame.
///
/// It used to be justified as 312 less the 24-line vertical interval, which
/// reads across from a fixed-raster machine. This one emits 310 lines, so
/// there was no 312 to subtract from and the position that arithmetic implied
/// was wrong by 16 lines. Where the window sits is [`FIRST_VISIBLE_LINE`],
/// derived from the ROM's pads; see #1116.
///
/// This was 240, which cropped 48 lines a set would have shown. That matters
/// here more than on a fixed-raster machine: the ZX80's vertical position is
/// software-timed, so a program that shifts its timing moves the picture, and
/// a window this much tighter than the set's clipped the movement. See #1054.
pub const FB_HEIGHT: u32 = 288;

/// A ceiling on the frame, not its length.
///
/// The ROM decides when a field ends, and Sinclair's emits 310 lines. This
/// only bounds firmware that never syncs; see [`TSTATES_PER_LINE`].
const LINES_PER_FRAME: u32 = 312;

/// T-states in a 50 Hz field at 3.25 MHz.
///
/// The ZX80 has no frame timer — the ROM decides when a field ends, so a
/// machine running code that never enters the display routine never ends
/// one. A television does not wait: absent sync its flywheel free-runs at
/// the nominal rate. Without this bound `run_frame` never returns, which
/// is what the synthetic-firmware images (which never `HALT`) hit.
const TSTATES_PER_LINE: u32 = 207;

/// How far a line may overrun before the clock gives up waiting for a sync.
///
/// A sync is authoritative when it arrives; this only decides how long to
/// hold the line open for one. Too tight and a line that syncs slightly late
/// is counted twice — once by the clock, once by the sync — which halves the
/// picture. EightyOne allows the same slack
/// (`ZX80MaximumSupportedScanlineOverhang`).
const MAX_LINE_T: u32 = TSTATES_PER_LINE + 40;
const TSTATES_PER_FRAME: u32 = TSTATES_PER_LINE * LINES_PER_FRAME;

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
    /// chip. There is no beam running to a timetable: the CPU emits a line,
    /// ends it with a `HALT`, and the ROM decides how many lines there are.
    /// It counts them in `C`, and the pads are the same loop with a bigger
    /// number — 56 for the first and 63 for the last, against 8 for each of
    /// the 24 character rows. The last is larger because it carries the
    /// bottom pad and the six lines of sync that follow it together.
    display_line: u32,
    /// 74LS373. Holds the character code the last display fetch put on the
    /// memory side of the bus.
    char_latch: u8,
    /// The ULA's job on a ZX81; here a 3-bit counter advanced by each line
    /// sync, selecting which of the eight rows of the character is shown.
    line_counter: u8,
    /// T-states since the field began, so a field ends on time even when
    /// the software never asks for one.
    frame_tstate: u32,
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
            frame_tstate: 0,
            vsync: false,
            frame_complete: false,
            painted: 0,
            pending: false,
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

    /// Advance one T-state.
    pub fn tick(&mut self) {
        self.tstate += 1;
        // The line clock free-runs. A television's horizontal oscillator
        // does not wait to be told: it runs at its own rate, and incoming
        // sync pulses pull it into step. Counting lines *only* from syncs
        // works for firmware that emits one per line — Sinclair's ROM does —
        // and stops dead for anything that does not, which is most machine
        // code. Cross Chase painted a few scanlines of each character row
        // and dropped the rest.
        if self.tstate >= MAX_LINE_T {
            self.next_line();
        }
        self.frame_tstate += 1;
        if self.frame_tstate >= TSTATES_PER_FRAME {
            self.end_frame();
        }
    }

    /// A horizontal sync: the interrupt that released the `HALT` has been
    /// acknowledged and the beam starts a new line.
    ///
    /// This *locks* the free-running clock rather than being the only thing
    /// that advances it. Sinclair's ROM syncs every line, a little under the
    /// nominal 207 T-states, so the lock wins and the picture sits exactly
    /// where the firmware puts it. Code that syncs irregularly, or not at
    /// all, still gets a raster — drifting, as it would on a real set.
    pub fn hsync(&mut self) {
        self.next_line();
    }

    /// Starts a line, whether from a sync or from the clock running out.
    fn next_line(&mut self) {
        self.tstate = 0;
        self.display_line += 1;
        self.line_counter = (self.line_counter + 1) & 0x07;
        if self.display_line >= LINES_PER_FRAME {
            self.end_frame();
        }
    }

    /// An `IN` with A0 low: start the vertical sync, and with it the frame.
    pub fn vsync_start(&mut self) {
        self.vsync = true;
    }

    /// An `OUT`: stop the vertical sync. The frame ends here on a real
    /// machine, which is why the rate is a software constant.
    pub fn vsync_stop(&mut self) {
        if self.vsync {
            self.vsync = false;
            self.end_frame();
        }
        self.line_counter = 0;
    }

    fn end_frame(&mut self) {
        self.tstate = 0;
        self.frame_tstate = 0;
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
        if !std::mem::take(&mut self.pending) {
            return;
        }
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
        let Some(y) = line.checked_sub(FIRST_VISIBLE_LINE) else {
            return;
        };
        if y >= FB_HEIGHT {
            return;
        }
        let Some(active) = line_tstate.checked_sub(FIRST_CHAR_TSTATE) else {
            return;
        };
        let x0 = active * PIXELS_PER_TSTATE + LEFT_BORDER;
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

/// The text area: 24 character rows of 8 lines.
const TEXT_LINES: u32 = 192;

/// The frame line the first character row starts on.
///
/// Frame line 0 is the end of the vertical sync pulse — see
/// [`Zx80Video::vsync_stop`] — so the ROM's top pad *is* this offset.
///
/// `reference/by-system/sinclair-zx80/zx80-video-generation-tynemouth.txt`
/// tabulates the whole UK field: 6 lines of sync, 56 of pad, 192 of text, 56
/// of pad, 310 in total. The model emits that field exactly. The real ROM's
/// frame measures 64,167 T-states, which at 207 a line is 310.0, and the
/// power-on cursor lands on frame lines 240-247 — row 23 of a text area
/// starting at 56.
const FIRST_TEXT_LINE: u32 = 56;

/// Where the set's window starts: centred on the text area.
///
/// The pads are what this machine has instead of a video chip's blanking,
/// and placing the text area is their entire purpose. The proof is that the
/// UK pad (56 lines) and the USA one (32) differ by 24 — exactly half the
/// difference between the 288 lines a PAL set shows and the 240 an NTSC one
/// does. Both pad to their own region's active area plus the same 8 lines
/// of overscan allowance. So the window holds the text area with
/// `(FB_HEIGHT - TEXT_LINES) / 2` of pad either side, and 8 lines of each
/// pad fall outside it.
///
/// This was 24 — `LINES_PER_FRAME - FB_HEIGHT`, reading the whole vertical
/// interval as following the sync pulse. That is roughly how a broadcast
/// field is laid out, but it is not this machine: 312 is a free-run ceiling
/// here, not a field length, and a ROM that emits 310 lines and centres its
/// own picture does not inherit the arithmetic. It sat the text area 16
/// lines high. See #1116.
const FIRST_VISIBLE_LINE: u32 = FIRST_TEXT_LINE - (FB_HEIGHT - TEXT_LINES) / 2;

/// Framebuffer row the text area's first line lands on.
///
/// The window is centred on the text area, so this is the pad it keeps. It
/// is public because the geometry had been copied into five test files as a
/// literal, every copy fitted to a framebuffer height that later changed and
/// none of them updated — which is #1116. There is one derivation now, and
/// callers ask for it.
pub const TEXT_TOP: u32 = (FB_HEIGHT - TEXT_LINES) / 2;

/// How long after the `HALT` releases the line's first character is
/// fetched: the interrupt is acknowledged, the handler at `$0038` counts
/// the row down and reloads `R`, and `JP (HL)` re-enters the display file.
/// Measured at 73 T-states against the real ROM, and constant because the
/// handler's path does not vary with the row's contents.
const FIRST_CHAR_TSTATE: u32 = 73;

/// 32 characters are 256 pixels; centring them in a 320-pixel framebuffer
/// leaves 32 either side.
const LEFT_BORDER: u32 = 32;

impl Default for Zx80Video {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window is centred on the text area, and the derivation is the
    /// point: written as a literal it went stale twice, and #1116 is the
    /// second time. Two independent statements of the same geometry, so a
    /// hand-written `FIRST_VISIBLE_LINE` cannot satisfy both by accident.
    #[test]
    fn the_window_is_centred_on_the_text_area() {
        assert_eq!(
            TEXT_TOP * 2 + TEXT_LINES,
            FB_HEIGHT,
            "the pads the window keeps should be equal, and with the text \
             area should fill it"
        );
        assert_eq!(
            FIRST_VISIBLE_LINE + TEXT_TOP,
            FIRST_TEXT_LINE,
            "the frame line the window opens on, plus the rows of pad it \
             keeps, is where the ROM starts the text -- the old 24 gave 72"
        );
    }

    /// The figure the placement is derived from is not `LINES_PER_FRAME`.
    ///
    /// It is the ROM\'s pad, and this is here to make the difference fail
    /// loudly rather than quietly: `LINES_PER_FRAME - FB_HEIGHT` is 24, and
    /// nothing about this machine\'s geometry is 24.
    #[test]
    fn the_free_run_ceiling_is_not_the_placement() {
        assert_ne!(
            FIRST_VISIBLE_LINE,
            LINES_PER_FRAME - FB_HEIGHT,
            "312 is a ceiling for firmware that never syncs; the ROM emits \
             310 and pads {FIRST_TEXT_LINE} lines. See #1116."
        );
    }
}
