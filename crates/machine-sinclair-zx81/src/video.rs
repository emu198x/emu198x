//! ZX81 display generation: the CPU draws the picture, and the ULA reads it
//! off the bus while it does.
//!
//! The ZX81 has no frame buffer and no display chip in the ordinary sense.
//! The ROM points the program counter into the display file with bit 15 set
//! and executes it. Every byte fetched from `$8000` upwards with bit 6 clear
//! is forced to `$00` — a `NOP` — while the real byte is latched as a
//! character code; the refresh cycle that follows composes a pattern address
//! and the ULA shifts eight pixels out at the beam. A byte with bit 6 set is
//! executed, which is why `$76` — `NEWLINE` in the display file and `HALT` as
//! an opcode — ends a line.
//!
//! `reference/by-system/sinclair-zx81/zx81-hardware-reference.md` §4 states
//! the sequence, the pattern address `I x 256 + CODE x 8 + COUNT`, and that
//! `COUNT` is a three-bit counter inside the ULA advanced by each line sync.
//! The bit-6 rule the sources do not state is in both reference emulators:
//! MAME's `zx_v.cpp` (`if (cdata & 0x40) return cdata;` then `m_ula_char_buffer
//! = cdata; return 0x00; // nop`) and EightyOne's `zx81_opcode_fetch`
//! (`bit6 = opcode & 64; if (!bit6) opcode = 0;`).
//!
//! # Why this is not shared with the ZX80
//!
//! `machine-sinclair-zx80`'s `video.rs` has the same shape, and the two were
//! deliberately left as two. The machines differ in what drives them —
//! [`nmi`](Zx81Video::nmi_line) and FAST/SLOW here against a ROM that counts
//! its own lines there — and in how the pattern address is composed: this
//! takes `I` whole through `$FE00`, the ZX80 takes `A9`-`A12` through `$1E00`
//! because it has multiplexers and no `I` register in the path at all.
//! `reference/by-system/sinclair-zx80/` says in terms not to port ZX81 ULA
//! behaviour across, so a shared crate would be re-coupling exactly what that
//! warning is about (#1033). Two machines is not enough to abstract over.

use serde::{Deserialize, Serialize};

/// Framebuffer width: the 256-pixel display with 32 of border either side.
///
/// **Narrower than a set's window**, which at 6.5 MHz over 52.0 µs is 338 —
/// the #1054 audit reads it as 95%. [`FIRST_CHAR_TSTATE`] is calibrated to
/// place the picture inside a window that was already chosen, so a width
/// derived from it would be circular. Unchanged here; #1032 is about the
/// mechanism, not the extent.
pub const FB_WIDTH: u32 = 320;

/// A PAL set displays 288 lines, and that is the whole of this figure: it is
/// the receiver's window. The ZX81's own field is 310 lines, so it is not a
/// remainder of anything here — where the window sits is
/// [`FIRST_VISIBLE_LINE`].
pub const FB_HEIGHT: u32 = 288;

/// A ceiling on the frame, not its length.
///
/// The ROM decides when a field ends, and Sinclair's emits 310 lines — the
/// SLOW frame measures 64,163 T-states, which at 207 a line is 310.0. This
/// bounds firmware that never syncs, and is not a figure to derive geometry
/// from; see [`FIRST_VISIBLE_LINE`].
const LINES_PER_FRAME: u32 = 312;

/// T-states in a line at 3.25 MHz — the ULA divides its 6.5 MHz dot clock by
/// two for the CPU, so a T-state is two pixels.
const TSTATES_PER_LINE: u32 = 207;

/// Where the line's horizontal sync begins.
///
/// From there to the line's end the ULA holds the processor, which is what
/// keeps every line's characters at the same T-states instead of drifting
/// with the ROM's loop. MAME does it at the same figure, stretching an
/// opcode fetch that lands at or after 192 to the end of the line.
const HSYNC_START_T: u32 = 192;

const TSTATES_PER_FRAME: u32 = TSTATES_PER_LINE * LINES_PER_FRAME;
/// How long a sync pulse has to be held to be a *field* sync rather than a
/// line one.
///
/// The ZX81 asserts and releases the same signal for both, so length is the
/// only thing that separates them — and the cassette output uses it too, in
/// pulses shorter still. MAME's `drop_sync` splits them at 1000 T-states.
const FIELD_SYNC_T: u32 = 1000;

/// The shortest frame that is allowed to end one. Without it a stray pair of
/// port accesses early in a field would restart it, and the picture would
/// walk. MAME uses the same guard at the same figure.
const MIN_FRAME_T: u32 = 52_000;

/// Two pixels a T-state: the 6.5 MHz dot clock over the 3.25 MHz CPU. A
/// forced `NOP` is four T-states and a character is eight pixels, so one
/// display fetch is exactly one character — which is the arithmetic that says
/// this model is the right shape.
const PIXELS_PER_TSTATE: u32 = 2;

/// The text area: 24 character rows of 8 lines.
const TEXT_LINES: u32 = 192;

/// The frame line the first character row starts on.
///
/// Measured: the power-on cursor lands on frame lines 240-247, which is row
/// 23 of a text area starting at 56.
///
/// ⚠ The ROM's own figure is one lower. `MARGIN` (`$4028`) holds the pad
/// depth and reads 55 on a booted machine, matching
/// `reference/by-system/sinclair-zx81/zx81-hardware-reference.md`. Whether
/// the loop emits `MARGIN` lines or one more is not settled here, and the
/// gap is #1118 — it shifts the whole picture by a line, and no reference to
/// hand resolves it. Naming the measured figure keeps the window derivation
/// below correct either way: settle 55, and only this constant changes.
///
/// ⚠ This is the 50 Hz figure and does not follow the board. A 60 Hz ZX81
/// pads 31 lines, not 55, so its text starts 24 lines earlier — and
/// [`FB_HEIGHT`] does not follow the board either. Both halves of that are
/// #1119; the 50 Hz path, which is what the tests and goldens exercise, is
/// unaffected.
const FIRST_TEXT_LINE: u32 = 56;

/// Where the set's window starts: centred on the text area.
///
/// The pad is what the ROM has instead of a video chip's blanking, and
/// placing the text area is its entire purpose. `MARGIN` is 55 on a 50 Hz
/// machine and 31 on a 60 Hz one — a difference of 24, exactly half the
/// difference between the 288 lines a PAL set shows and the 240 an NTSC one
/// does. Both pad to their own region's active area plus the same overscan
/// allowance. So the window holds the text area with
/// `(FB_HEIGHT - TEXT_LINES) / 2` of pad either side.
///
/// This was `LINES_PER_FRAME - FB_HEIGHT`, which reads the whole vertical
/// interval as following the sync pulse. That is roughly how a broadcast
/// field is laid out, but the ZX81 emits 310 lines, not 312 — the real
/// ROM's SLOW frame measures 64,163 T-states — so the subtraction was
/// against a number this machine never produces. It sat the text area 16
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

/// How far into a line the first character is fetched, in T-states.
///
/// Measured against the real ROM, not derived: the figure is a property of
/// the ROM's display loop and nothing in the frame's arithmetic predicts it.
/// Deriving a window width from it would be circular; see [`FB_WIDTH`].
///
/// The measurement is the one that says this model is right. Every display
/// line of the power-on screen leaves its `HALT` at T-state 172 and fetches
/// its characters from T-state 38 — thirty-two of them, four T-states apart,
/// which is one forced `NOP` each. 128 T-states at two pixels a T-state is
/// the 256-pixel display exactly, and the figure does not move from line to
/// line or from field to field.
/// The ZX80's module records 73 for the same constant; the two ROMs reach
/// their first character at different points in the line, which is one more
/// reason the machines are not sharing a module.
const FIRST_CHAR_TSTATE: u32 = 38;

/// Framebuffer pixels of border left of the first character.
const LEFT_BORDER: u32 = 32;

/// The ZX81 displays black on white.
const PAPER: u32 = 0xFFFF_FFFF;
const INK: u32 = 0xFF00_0000;

/// Display generation: a latch, a line counter, an NMI generator and a beam.
#[derive(Clone, Serialize, Deserialize)]
pub struct Zx81Video {
    framebuffer: Vec<u32>,
    /// T-states since the current line began — the horizontal position.
    tstate: u32,
    /// Which line is being drawn.
    display_line: u32,
    /// T-states since the field began, so a field ends on time.
    frame_tstate: u32,
    /// The character code the last display fetch put on the bus.
    char_latch: u8,
    /// `COUNT`: which of the character's eight rows this line shows.
    line_counter: u8,
    /// Set by a forced `NOP`, consumed by the refresh cycle after it.
    ///
    /// Every M1 has a refresh cycle and only a display fetch loads the latch,
    /// so without this the latch's stale contents would be shifted out during
    /// ordinary code and smear the last character across the picture.
    pending: bool,
    /// Whether the NMI generator is running — SLOW mode. FAST mode turns it
    /// off and with it the display.
    nmi_enabled: bool,
    /// Whether the software vertical sync is currently asserted.
    vsync: bool,
    /// The field T-state it was asserted at, so its length can be measured.
    vsync_start_tstate: u32,
    /// The NMI line as the ULA is driving it this T-state.
    nmi: bool,
    frame_complete: bool,
    /// Pixels painted this frame, so a test can tell a picture from a blank.
    painted: u32,
}

impl Default for Zx81Video {
    fn default() -> Self {
        Self::new()
    }
}

impl Zx81Video {
    #[must_use]
    pub fn new() -> Self {
        Self {
            framebuffer: vec![PAPER; (FB_WIDTH * FB_HEIGHT) as usize],
            tstate: 0,
            display_line: 0,
            frame_tstate: 0,
            char_latch: 0,
            line_counter: 0,
            pending: false,
            nmi_enabled: false,
            vsync: false,
            vsync_start_tstate: 0,
            nmi: false,
            frame_complete: false,
            painted: 0,
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    #[must_use]
    pub const fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    #[must_use]
    pub const fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    #[must_use]
    pub const fn painted_pixels(&self) -> u32 {
        self.painted
    }

    #[must_use]
    pub const fn line(&self) -> u32 {
        self.display_line
    }

    #[must_use]
    pub const fn line_tstate(&self) -> u32 {
        self.tstate
    }

    #[must_use]
    pub const fn tstates_per_frame() -> u32 {
        TSTATES_PER_FRAME
    }

    /// FAST mode writes `$FD` and SLOW writes `$FE`; the ULA's NMI generator
    /// follows, and the display with it.
    pub const fn set_nmi_enabled(&mut self, enabled: bool) {
        self.nmi_enabled = enabled;
    }

    #[must_use]
    pub const fn nmi_enabled(&self) -> bool {
        self.nmi_enabled
    }

    /// The NMI line this T-state.
    #[must_use]
    pub const fn nmi_line(&self) -> bool {
        self.nmi
    }

    /// Whether the ULA is holding the processor's clock this T-state.
    ///
    /// TR1's job (reference §1): its base on `/HALT` and emitter on `/NMI`,
    /// pulling `/WAIT` low so the processor is suspended for the length of
    /// the pulse. That suspension is what locks the CPU to the line: without
    /// it the ROM's loop drifts against the ULA's clock and the picture's
    /// left edge moves by a character or two from line to line.
    ///
    /// Clock gating rather than the `/WAIT` pin, which `zilog_z80` documents
    /// as the way a ULA does this — the pin models the +2A/+3 gate array's
    /// contention, which is a different mechanism.
    #[must_use]
    pub const fn holds_cpu(&self) -> bool {
        self.nmi_enabled && self.tstate >= HSYNC_START_T
    }

    /// Advance one T-state.
    pub fn tick(&mut self) {
        self.tstate += 1;

        // The generator asserts NMI across the line sync, not as an instant:
        // the pulse's *length* is what TR1 turns into the processor's WAIT
        // (reference §1), so a one-T-state pulse would have nothing to hold.
        // MAME drives it the same way, from the same window.
        self.nmi = self.nmi_enabled && self.tstate >= HSYNC_START_T;

        if self.tstate >= TSTATES_PER_LINE {
            self.next_line();
        }
        self.frame_tstate += 1;
        if self.frame_tstate >= TSTATES_PER_FRAME {
            self.end_frame();
        }
    }

    fn next_line(&mut self) {
        self.tstate = 0;
        self.display_line += 1;
        self.line_counter = (self.line_counter + 1) & 0x07;
        if self.display_line >= LINES_PER_FRAME {
            self.end_frame();
        }
    }

    fn end_frame(&mut self) {
        self.tstate = 0;
        self.frame_tstate = 0;
        self.display_line = 0;
        self.line_counter = 0;
        self.frame_complete = true;
    }

    /// An `IN` from a port with A0 low starts the vertical sync.
    ///
    /// The ZX81 generates vertical sync in software, as the ZX80 does — the
    /// ROM leaves the display routine, turns the NMI generator off, and holds
    /// the sync across the vertical interval. So the field begins where the
    /// firmware says rather than where a counter says, and the picture sits
    /// where the ROM puts it.
    ///
    /// **Only with the generator off.** In SLOW mode the ROM reads the
    /// keyboard through this same port many times a field and means nothing
    /// by it; taking every such read as a sync ends the field constantly and
    /// the display never reaches the screen. MAME gates it the same way:
    /// `if (!m_vsync_active && !m_nmi_generator_active)`.
    pub const fn vsync_start(&mut self) {
        if !self.vsync && !self.nmi_enabled {
            self.vsync = true;
            self.vsync_start_tstate = self.frame_tstate;
        }
    }

    /// An `OUT` releases it. A long pulse ends the field; a short one is a
    /// line sync, and shorter still is the cassette.
    pub fn vsync_stop(&mut self) {
        if !self.vsync {
            return;
        }
        self.vsync = false;
        let held = self.frame_tstate.saturating_sub(self.vsync_start_tstate);
        if held > FIELD_SYNC_T && self.frame_tstate > MIN_FRAME_T {
            self.end_frame();
        } else {
            self.line_counter = 0;
        }
    }

    pub fn take_frame_complete(&mut self) -> bool {
        std::mem::replace(&mut self.frame_complete, false)
    }

    /// Begin a new frame's picture, so a frame in which the CPU never reached
    /// the display routine comes out blank rather than holding the last one.
    pub fn clear(&mut self) {
        self.framebuffer.fill(PAPER);
        self.painted = 0;
        self.pending = false;
    }

    /// An opcode fetch as the ULA sees it. `Some(0x00)` when it forces a
    /// `NOP` and takes the byte for the display; `None` when the byte is the
    /// CPU's to execute.
    ///
    /// Bit 6 is the whole rule. Character codes are 0-63 and inverse ones
    /// 128-191, so both have it clear; `$76` has it set and executes as the
    /// `HALT` that ends the line.
    pub fn opcode_fetch(&mut self, addr: u16, byte: u8) -> Option<u8> {
        if addr & 0x8000 == 0 || byte & 0x40 != 0 {
            return None;
        }
        self.char_latch = byte;
        self.pending = true;
        Some(0x00)
    }

    /// The refresh cycle that follows a forced `NOP`, with `I:R` on the
    /// address bus.
    ///
    /// `address = I x 256 + CODE x 8 + COUNT` (reference §4). `I` reaches the
    /// pattern table through `$FE00` — bit 0 is not part of the address, which
    /// is why the firmware's `$1E` selects `$1E00`-`$1FFF` in the top of ROM.
    pub fn refresh(&mut self, refresh_addr: u16, read_mem: impl Fn(u16) -> u8) {
        if !std::mem::take(&mut self.pending) {
            return;
        }
        // `I*256 + CODE*8 + COUNT` -- the ULA forms the pattern address from
        // the processor's refresh output, the latched character code, and its
        // own three-bit line counter. *The Ins and Outs of the TS1000 & ZX81*,
        // Thomasson, p35-36, which states the formula in exactly that form.
        //
        // Both halves of that matter and neither is what this used to do. It
        // masked the refresh address with `0xFE00`, which drops bit 0 of `I`,
        // and it OR-ed the three terms together. With the stock `I` of `$1E`
        // the two are indistinguishable: `$1E00` has no bits below A9, so
        // nothing overlaps and nothing is lost. Any other `I` and they differ
        // -- an odd one addresses the wrong 256-byte page entirely, which is
        // why a character set pointed at RAM produced no picture.
        let code = u16::from(self.char_latch);
        let addr = (refresh_addr & 0xFF00)
            .wrapping_add((code & 0x3F) << 3)
            .wrapping_add(u16::from(self.line_counter));
        let pattern = read_mem(addr);
        self.paint(if self.char_latch & 0x80 != 0 {
            !pattern
        } else {
            pattern
        });
    }

    /// Shift eight pixels out at the beam.
    fn paint(&mut self, pattern: u8) {
        let Some(y) = self.display_line.checked_sub(FIRST_VISIBLE_LINE) else {
            return;
        };
        if y >= FB_HEIGHT {
            return;
        }
        let Some(active) = self.tstate.checked_sub(FIRST_CHAR_TSTATE) else {
            return;
        };
        let x0 = active * PIXELS_PER_TSTATE + LEFT_BORDER;
        for bit in 0..8u32 {
            let x = x0 + bit;
            if x >= FB_WIDTH {
                break;
            }
            self.framebuffer[(y * FB_WIDTH + x) as usize] = if pattern & (0x80 >> bit) != 0 {
                INK
            } else {
                PAPER
            };
            self.painted += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Record which address the ULA asks memory for, for one character.
    fn fetched_address(i: u8, r: u8, code: u8, count: u8) -> u16 {
        let mut video = Zx81Video::new();
        video.line_counter = count;
        // An M1 fetch from the display file latches the code.
        assert_eq!(video.opcode_fetch(0x8000 | 0x4000, code), Some(0x00));

        let seen = RefCell::new(None);
        video.refresh(u16::from(i) << 8 | u16::from(r), |addr| {
            *seen.borrow_mut() = Some(addr);
            0x00
        });
        seen.into_inner()
            .expect("the ULA should have read a pattern")
    }

    /// `I*256 + CODE*8 + COUNT`, from the hardware manual (Thomasson, p35-36).
    ///
    /// The stock character set makes this look like three separate fields:
    /// `I` is `$1E`, so `$1E00` has nothing below A9 and the terms cannot
    /// overlap. That is what let an implementation that masked `I` to seven
    /// bits and OR-ed the terms pass unnoticed for as long as nothing moved
    /// the character set.
    #[test]
    fn the_pattern_address_is_i_times_256_plus_code_times_8_plus_count() {
        // The stock set, where every reading agrees.
        assert_eq!(fetched_address(0x1E, 0x00, 0x00, 0), 0x1E00);
        assert_eq!(fetched_address(0x1E, 0x00, 0x3F, 7), 0x1E00 + 0x3F * 8 + 7);

        // An odd `I` is a different 256-byte page, not the even one below it.
        assert_eq!(fetched_address(0x21, 0x00, 0x00, 0), 0x2100);
        assert_eq!(fetched_address(0x21, 0x00, 0x05, 3), 0x2100 + 0x05 * 8 + 3);

        // `R` never reaches the address: the ULA supplies the low bits.
        assert_eq!(
            fetched_address(0x21, 0xFF, 0x05, 3),
            fetched_address(0x21, 0x00, 0x05, 3),
        );
    }

    /// The terms are summed, not OR-ed, which only shows when they overlap.
    ///
    /// `CODE*8` reaches A8 once the code is 32 or more, and that is the bit an
    /// odd `I` also occupies.
    #[test]
    fn a_high_code_carries_into_the_page_an_odd_i_selects() {
        // $21 * 256 + 32 * 8 = $2100 + $100 = $2200.
        assert_eq!(fetched_address(0x21, 0x00, 0x20, 0), 0x2200);
        // OR-ing would give $2100, and masking I first would give $2100 too.
        assert_ne!(fetched_address(0x21, 0x00, 0x20, 0), 0x2100);
    }

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
