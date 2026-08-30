//! Motorola 6845 CRT Controller (CRTC).
//!
//! Adapted from `Emu198x-Oldest/crates/motorola-6845` (port 2026-06-01)
//! as a foundation for BBC Micro. Self-contained port with no external
//! dependencies. The 6845 also drives Amstrad CPC (existing crate
//! `amstrad-ula-40077` and future CPC machine layer), later Commodore
//! PET models, and the Commodore 128's VDC 8563 — all of which can
//! adopt this crate.
//!
//! Generates timing signals and memory addresses for CRT display systems.
//! The 6845 does not generate pixels — it provides addresses (MA0-MA13)
//! and raster line counts (RA0-RA4) for external pixel generation hardware.
//!
//! Used by the BBC Micro, Amstrad CPC, later Commodore PET models,
//! and the Commodore 128's VDC 8563.
//!
//! 18 registers (R0-R17): horizontal/vertical timing, sync positions,
//! display start address, cursor position, and light pen latch.

use serde::{Deserialize, Serialize};

/// Which 6845 the machine actually fits.
///
/// The part number matters for register readback, cursor shape, and sync-width
/// programming. The CPC community numbers the variants 0 to 4, and
/// software detects them by reading registers and seeing what comes out. So a
/// machine that claims one part while reading back like another is detectable
/// by real software, not merely wrong on paper.
///
/// Masks follow Arnold's `crtc.c` read tables, which are per-part rather than
/// per-machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Crtc6845Variant {
    /// MC6845 and UM6845R — CPC types 2 and 1. The start address (R12/R13) is
    /// write-only; only cursor and light-pen registers read back.
    ///
    /// The default, because it is what this model has always done and the BBC
    /// Micro and PET were written against it.
    #[default]
    Mc6845,
    /// HD6845S, also sold as UM6845 — CPC type 0, and what a CPC464 fits. The
    /// start address reads back too.
    Hd6845s,
}

impl Crtc6845Variant {
    /// Does this part read register `reg` back?
    #[must_use]
    pub const fn reads_back(self, reg: usize) -> bool {
        match self {
            // R14-R17: cursor and light pen.
            Self::Mc6845 => matches!(reg, 14..=17),
            // R12-R17: the start address as well.
            Self::Hd6845s => matches!(reg, 12..=17),
        }
    }
}

/// Motorola 6845 CRTC.
#[derive(Serialize, Deserialize)]
pub struct Crtc6845 {
    /// Which part this is. Only affects register readback.
    #[serde(default)]
    variant: Crtc6845Variant,
    /// Selected register number (0-17).
    selected: u8,
    /// Registers R0-R17.
    regs: [u8; 18],

    // Counters
    /// Horizontal character counter (0 to R0).
    h_counter: u8,
    /// Raster counter (scanline within character row, 0 to R9).
    ra: u8,
    /// Vertical character row counter (0 to R4).
    v_counter: u8,
    /// Vertical total adjust counter (0 to R5).
    v_adjust: u8,
    /// Whether we're in the vertical adjust period.
    in_v_adjust: bool,

    // Memory address
    /// Memory address counter (14-bit, MA0-MA13).
    ma: u16,
    /// Address output for the character currently being displayed. The
    /// counter `ma` runs one ahead (it is advanced ready for the next
    /// character); `ma_output` latches the value valid for *this* one so a
    /// consumer that samples after `tick()` reads the right cell rather than
    /// the next.
    ma_output: u16,
    /// Row start address (latched at the beginning of each character row).
    row_start: u16,

    // Sync and display enable outputs
    /// Horizontal sync active.
    pub hsync: bool,
    /// Vertical sync active.
    pub vsync: bool,
    /// Display enable (active during visible area).
    pub display_enable: bool,

    // Sync counters
    hsync_counter: u8,
    vsync_counter: u8,

    /// Fields elapsed, used by R10's 16/32-field cursor blink modes.
    #[serde(default)]
    cursor_blink_count: u8,

    /// Cursor output after address, raster-shape, and blink gating.
    pub cursor_active: bool,
}

impl Crtc6845 {
    /// Create a new CRTC with default register values.
    #[must_use]
    pub fn new() -> Self {
        Self {
            variant: Crtc6845Variant::default(),
            selected: 0,
            regs: [0; 18],
            h_counter: 0,
            ra: 0,
            v_counter: 0,
            v_adjust: 0,
            in_v_adjust: false,
            ma: 0,
            ma_output: 0,
            row_start: 0,
            hsync: false,
            vsync: false,
            display_enable: false,
            hsync_counter: 0,
            vsync_counter: 0,
            cursor_blink_count: 0,
            cursor_active: false,
        }
    }

    /// Write the address register (selects which register to access).
    pub fn write_address(&mut self, value: u8) {
        self.selected = value & 0x1F;
    }

    /// Write the data register (writes to the currently selected register).
    pub fn write_data(&mut self, value: u8) {
        let reg = self.selected as usize;
        if reg < 18 {
            // R0-R13 are write-only, R14-R15 are R/W
            self.regs[reg] = value;
        }
    }

    /// Read the data register (R14-R17 are readable).
    #[must_use]
    pub fn read_data(&self) -> u8 {
        let reg = self.selected as usize;
        if self.variant.reads_back(reg) {
            self.regs[reg]
        } else {
            0 // Write-only on this part
        }
    }

    /// Select which 6845 this is. See [`Crtc6845Variant`].
    pub const fn set_variant(&mut self, variant: Crtc6845Variant) {
        self.variant = variant;
    }

    /// Which 6845 this is.
    #[must_use]
    pub const fn variant(&self) -> Crtc6845Variant {
        self.variant
    }

    /// Current memory address output (MA0-MA13, 14-bit). This is the address
    /// valid for the character being displayed *now*, not the counter's
    /// look-ahead value.
    #[must_use]
    pub fn memory_address(&self) -> u16 {
        self.ma_output & 0x3FFF
    }

    /// Current raster address (RA0-RA4, 5-bit).
    #[must_use]
    pub fn raster_address(&self) -> u8 {
        self.ra
    }

    /// Display start address (R12:R13).
    #[must_use]
    pub fn start_address(&self) -> u16 {
        (u16::from(self.regs[12] & 0x3F) << 8) | u16::from(self.regs[13])
    }

    /// Cursor address (R14:R15).
    #[must_use]
    pub fn cursor_address(&self) -> u16 {
        (u16::from(self.regs[14] & 0x3F) << 8) | u16::from(self.regs[15])
    }

    /// Register values (for observation).
    #[must_use]
    pub fn regs(&self) -> &[u8; 18] {
        &self.regs
    }

    /// Maximum scanline per character row (R9).
    #[must_use]
    pub fn max_scanline(&self) -> u8 {
        self.regs[9] & 0x1F
    }

    /// Whether the vertical raster is past the displayed rows (R6) — i.e. in
    /// the bottom border / vertical retrace. Consumers that route the CRTC's
    /// off-screen state to a status line (e.g. the PET's VIA PB5 "vertical
    /// retrace" bit) read this; it mirrors the `v_counter < R6` display-enable
    /// test used in `tick`.
    #[must_use]
    pub fn in_vertical_retrace(&self) -> bool {
        self.v_counter >= self.regs[6]
    }

    /// Tick one character clock. Call at the CRTC clock rate (1 or 2 MHz
    /// depending on mode). Returns true at the start of a new frame.
    pub fn tick(&mut self) -> bool {
        let mut new_frame = false;

        // Horizontal counter
        let h_total = self.regs[0];
        let h_displayed = self.regs[1];
        let h_sync_pos = self.regs[2];
        let h_sync_width = self.regs[3] & 0x0F;

        // Display enable: active when both H and V are in displayed area
        let h_visible = self.h_counter < h_displayed;
        let v_visible = self.v_counter < self.regs[6];
        self.display_enable = h_visible && v_visible && !self.in_v_adjust;

        // Latch the address for the character displayed this clock, then
        // advance the counter ready for the next one.
        if self.display_enable {
            self.ma_output = self.ma;
            self.ma = self.ma.wrapping_add(1) & 0x3FFF;
        }

        // Cursor detection — R14/R15 choose the cell, R10/R11 choose its
        // raster shape, and R10 also gates it steadily/off/at 16 or 32 fields.
        self.cursor_active = self.cursor_visible();

        // HSYNC generation
        // The original Motorola datasheet says zero suppresses HSYNC. The
        // HD6845S datasheet calls zero unprogrammable; its pulse-width table
        // likewise has no pulse for that value. See the primary sources in
        // `198x/reference/by-topic/crtc-6845/`.
        if self.h_counter == h_sync_pos && h_sync_width != 0 {
            self.hsync = true;
            self.hsync_counter = 0;
        }
        if self.hsync {
            self.hsync_counter += 1;
            if self.hsync_counter >= h_sync_width {
                self.hsync = false;
            }
        }

        // Advance horizontal counter. The line is R0 + 1 characters, so the
        // tick where the counter already equals R0 is the last one; testing
        // before the increment keeps that length while making R0 = 255 — a
        // legal value — wrap rather than increment past the top of a u8 (#162).
        if self.h_counter >= h_total {
            // End of line
            self.h_counter = 0;
            new_frame = self.advance_vertical();
        } else {
            self.h_counter += 1;
        }

        new_frame
    }

    fn cursor_visible(&self) -> bool {
        if !self.display_enable || self.ma_output != self.cursor_address() {
            return false;
        }

        let mode = self.regs[10] & 0x60;
        let blink_visible = match mode {
            0x00 => true,
            0x20 => false,
            0x40 => self.cursor_blink_count & 0x10 != 0,
            0x60 => self.cursor_blink_count & 0x20 != 0,
            _ => unreachable!(),
        };
        if !blink_visible {
            return false;
        }

        let start = self.regs[10] & 0x1f;
        let end = self.regs[11] & 0x1f;
        let max = self.max_scanline();
        if start > max {
            return false;
        }

        match self.variant {
            Crtc6845Variant::Mc6845 if start > end => self.ra <= end || self.ra >= start,
            Crtc6845Variant::Mc6845 if end > max => true,
            Crtc6845Variant::Mc6845 | Crtc6845Variant::Hd6845s => {
                start <= end && self.ra >= start && self.ra <= end
            }
        }
    }

    /// Advance vertical counters at end of each horizontal line.
    /// Returns true at frame start.
    fn advance_vertical(&mut self) -> bool {
        let max_scan = self.regs[9] & 0x1F;
        let v_total = self.regs[4] & 0x7F;
        let v_adjust = self.regs[5] & 0x1F;
        let v_sync_pos = self.regs[7] & 0x7F;
        // MC6845 vertical sync is fixed at 16 scanlines and does not use R3's
        // upper nibble. HD6845S makes that nibble programmable, with zero
        // encoding 16 scanlines.
        let programmed_v_sync_width = (self.regs[3] >> 4) & 0x0F;
        let v_sync_width = match self.variant {
            Crtc6845Variant::Mc6845 => 16,
            Crtc6845Variant::Hd6845s if programmed_v_sync_width == 0 => 16,
            Crtc6845Variant::Hd6845s => programmed_v_sync_width,
        };

        if self.in_v_adjust {
            self.v_adjust += 1;
            if self.v_adjust >= v_adjust {
                // Frame complete — restart
                self.in_v_adjust = false;
                self.v_counter = 0;
                self.ra = 0;
                self.ma = self.start_address();
                self.row_start = self.ma;
                self.cursor_blink_count = self.cursor_blink_count.wrapping_add(1);
                return true;
            }
            return false;
        }

        // Advance raster counter
        if self.ra >= max_scan {
            // End of character row
            self.ra = 0;

            self.v_counter += 1;

            // VSYNC generation, against the row just *entered*. Comparing
            // before the increment starts the pulse at the end of row R7
            // instead of its beginning — a whole character row late, which
            // moves the picture eight lines up the screen on any machine that
            // locks its display to the sync. MAME's `mc6845.cpp` increments
            // `m_line_counter` and then calls `match_line()`, and its raw
            // screen configuration puts `vsync_on_pos` at
            // `m_vert_sync_pos * video_char_height` — row R7's first line.
            //
            // A sync position of row 0 still never fires: the frame-restart
            // paths below reset the counter and return without testing. No
            // machine here configures R7 = 0, so that is left alone.
            if self.v_counter == v_sync_pos {
                self.vsync = true;
                self.vsync_counter = 0;
            }

            if self.v_counter > v_total {
                // Start vertical adjust period
                if v_adjust > 0 {
                    self.in_v_adjust = true;
                    self.v_adjust = 0;
                } else {
                    // No adjust — restart immediately
                    self.v_counter = 0;
                    self.ra = 0;
                    self.ma = self.start_address();
                    self.row_start = self.ma;
                    self.cursor_blink_count = self.cursor_blink_count.wrapping_add(1);
                    return true;
                }
            }

            // Latch row start address for next row
            self.row_start = self.ma;
        } else {
            self.ra += 1;
            // Restart MA from the beginning of this character row
            self.ma = self.row_start;
        }

        // VSYNC width
        if self.vsync {
            self.vsync_counter += 1;
            if self.vsync_counter >= v_sync_width {
                self.vsync = false;
            }
        }

        false
    }
    /// Serialize CRTC state for save states.
    #[must_use]
    pub fn save_state(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(37);
        data.push(self.selected);
        data.extend_from_slice(&self.regs);
        data.push(self.h_counter);
        data.push(self.ra);
        data.push(self.v_counter);
        data.push(self.v_adjust);
        data.push(u8::from(self.in_v_adjust));
        data.extend_from_slice(&self.ma.to_le_bytes());
        data.extend_from_slice(&self.ma_output.to_le_bytes());
        data.extend_from_slice(&self.row_start.to_le_bytes());
        data.push(u8::from(self.hsync));
        data.push(u8::from(self.vsync));
        data.push(u8::from(self.display_enable));
        data.push(self.hsync_counter);
        data.push(self.vsync_counter);
        data.push(u8::from(self.cursor_active));
        data.push(self.cursor_blink_count);
        data
    }

    /// Restore CRTC state from a save state.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is too short.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        if data.len() < 36 {
            return Err("CRTC state truncated".into());
        }
        let mut p = 0;
        self.selected = data[p];
        p += 1;
        self.regs.copy_from_slice(&data[p..p + 18]);
        p += 18;
        self.h_counter = data[p];
        p += 1;
        self.ra = data[p];
        p += 1;
        self.v_counter = data[p];
        p += 1;
        self.v_adjust = data[p];
        p += 1;
        self.in_v_adjust = data[p] != 0;
        p += 1;
        self.ma = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.ma_output = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.row_start = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.hsync = data[p] != 0;
        p += 1;
        self.vsync = data[p] != 0;
        p += 1;
        self.display_enable = data[p] != 0;
        p += 1;
        self.hsync_counter = data[p];
        p += 1;
        self.vsync_counter = data[p];
        p += 1;
        self.cursor_active = data[p] != 0;
        p += 1;
        self.cursor_blink_count = data.get(p).copied().unwrap_or(0);
        p += usize::from(data.len() > p);
        Ok(p)
    }
}

impl Default for Crtc6845 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_mode0(crtc: &mut Crtc6845) {
        // BBC Micro MODE 0: 80-column, 2 MHz CRTC clock
        let vals = [
            127, 80, 98, 0x28, 38, 0, 32, 34, 0, 7, 0, 0, 0x0C, 0x00, 0, 0, 0, 0,
        ];
        for (i, &v) in vals.iter().enumerate() {
            crtc.write_address(i as u8);
            crtc.write_data(v);
        }
    }

    #[test]
    fn a_horizontal_total_of_255_does_not_overflow_the_counter() {
        // R0 = 255 is a legal value. The counter must wrap at the end of the
        // line rather than incrementing past the top of a u8 (#162).
        let mut crtc = Crtc6845::new();
        crtc.write_address(0);
        crtc.write_data(255);
        for _ in 0..600 {
            crtc.tick();
        }
    }

    /// The wrap test moved from after the increment to before it. That is only
    /// safe if a line is still R0 + 1 characters, so measure it rather than
    /// trusting the reasoning.
    #[test]
    fn a_line_is_r0_plus_one_characters() {
        for h_total in [0_u8, 1, 63, 127, 254, 255] {
            let mut crtc = Crtc6845::new();
            crtc.write_address(0);
            crtc.write_data(h_total);
            // Count the ticks between one line start and the next. Detecting
            // the end by "the counter went back to zero" does not work for
            // R0 = 0, where it never leaves zero and the line is one tick.
            while crtc.h_counter != 0 {
                crtc.tick();
            }
            let mut ticks = 0_u32;
            loop {
                crtc.tick();
                ticks += 1;
                if crtc.h_counter == 0 {
                    break;
                }
                assert!(ticks < 1000, "line never ended for R0 = {h_total}");
            }
            assert_eq!(
                ticks,
                u32::from(h_total) + 1,
                "R0 = {h_total} should give a line of R0 + 1 characters"
            );
        }
    }

    #[test]
    fn zero_horizontal_sync_width_suppresses_hsync() {
        for variant in [Crtc6845Variant::Mc6845, Crtc6845Variant::Hd6845s] {
            let mut crtc = Crtc6845::new();
            crtc.set_variant(variant);
            crtc.regs[0] = 7;
            crtc.regs[2] = 2;
            crtc.regs[3] = 0;

            for _ in 0..24 {
                crtc.tick();
                assert!(!crtc.hsync, "{variant:?} generated HSYNC for width zero");
            }
        }
    }

    fn start_vertical_sync(crtc: &mut Crtc6845, r3: u8) {
        crtc.regs[3] = r3;
        crtc.regs[4] = 20;
        crtc.regs[7] = 1;
        crtc.regs[9] = 0;
        crtc.advance_vertical();
        assert!(crtc.vsync);
    }

    #[test]
    fn mc6845_vertical_sync_is_fixed_at_sixteen_scanlines() {
        let mut crtc = Crtc6845::new();
        start_vertical_sync(&mut crtc, 0x20);

        // R3's high nibble requests two lines on later parts, but the original
        // MC6845 ignores it and keeps the fixed 16-line pulse.
        crtc.advance_vertical();
        assert!(crtc.vsync);
    }

    #[test]
    fn hd6845s_programs_vertical_sync_and_maps_zero_to_sixteen() {
        let mut crtc = Crtc6845::new();
        crtc.set_variant(Crtc6845Variant::Hd6845s);
        start_vertical_sync(&mut crtc, 0x20);
        crtc.advance_vertical();
        assert!(!crtc.vsync, "a width of two must finish on the second line");

        let mut zero = Crtc6845::new();
        zero.set_variant(Crtc6845Variant::Hd6845s);
        start_vertical_sync(&mut zero, 0);
        for _ in 0..14 {
            zero.advance_vertical();
            assert!(zero.vsync);
        }
        zero.advance_vertical();
        assert!(!zero.vsync, "zero must encode a 16-line vertical pulse");
    }

    /// A mid-frame write that drops a vertical register below its live counter
    /// must not strand that counter above a reset condition it can never meet
    /// again (#162). These pass because every vertical test is `>=` or `>`
    /// against the value *after* the increment; recorded so a future
    /// restructure cannot quietly reintroduce the horizontal asymmetry.
    #[test]
    fn lowering_r4_or_r9_mid_frame_does_not_strand_a_counter() {
        for (reg, lowered) in [(4_u8, 1_u8), (9, 0), (5, 0)] {
            let mut crtc = Crtc6845::new();
            setup_mode0(&mut crtc);
            for _ in 0..20_000 {
                crtc.tick();
            }
            crtc.write_address(reg);
            crtc.write_data(lowered);
            for _ in 0..20_000 {
                crtc.tick();
            }
        }
    }

    #[test]
    fn register_write_and_read() {
        let mut crtc = Crtc6845::new();
        // R14 (cursor high) is read/write
        crtc.write_address(14);
        crtc.write_data(0x12);
        assert_eq!(crtc.read_data(), 0x12);
    }

    #[test]
    fn write_only_registers_return_zero() {
        let mut crtc = Crtc6845::new();
        crtc.write_address(0);
        crtc.write_data(127);
        assert_eq!(crtc.read_data(), 0); // R0 is write-only
    }

    #[test]
    fn start_address_from_r12_r13() {
        let mut crtc = Crtc6845::new();
        crtc.write_address(12);
        crtc.write_data(0x0C);
        crtc.write_address(13);
        crtc.write_data(0x00);
        assert_eq!(crtc.start_address(), 0x0C00);
    }

    #[test]
    fn horizontal_counter_wraps_at_r0() {
        let mut crtc = Crtc6845::new();
        setup_mode0(&mut crtc);
        // Tick 128 times (R0 = 127, wraps at 128)
        for _ in 0..128 {
            crtc.tick();
        }
        assert_eq!(crtc.h_counter, 0);
    }

    #[test]
    fn display_enable_during_visible() {
        let mut crtc = Crtc6845::new();
        setup_mode0(&mut crtc);
        // First tick should be in visible area (h=0, v=0)
        crtc.tick();
        assert!(crtc.display_enable);
    }

    #[test]
    fn display_enable_off_during_hblank() {
        let mut crtc = Crtc6845::new();
        setup_mode0(&mut crtc);
        // Tick past R1 (80 displayed chars)
        for _ in 0..81 {
            crtc.tick();
        }
        assert!(!crtc.display_enable);
    }

    #[test]
    fn frame_completes() {
        let mut crtc = Crtc6845::new();
        setup_mode0(&mut crtc);
        // MODE 0: 128 chars/line × 39 rows × 8 scanlines = 39936 ticks
        let mut frame_done = false;
        for _ in 0..40000 {
            if crtc.tick() {
                frame_done = true;
                break;
            }
        }
        assert!(frame_done, "frame should complete within 40000 ticks");
    }

    #[test]
    fn memory_address_increments_during_display() {
        let mut crtc = Crtc6845::new();
        setup_mode0(&mut crtc);
        // Run one full frame to load start address into MA
        loop {
            if crtc.tick() {
                break;
            }
        }
        // Tick once into the first visible character so the latched output
        // holds this frame's start, not the previous frame's last cell.
        crtc.tick();
        let ma_start = crtc.memory_address();
        // Tick a few visible characters
        for _ in 0..10 {
            crtc.tick();
        }
        // MA should have advanced from the start
        assert!(crtc.memory_address() > ma_start);
    }

    fn cursor_at(crtc: &mut Crtc6845, raster: u8, blink_count: u8) -> bool {
        crtc.display_enable = true;
        crtc.ma_output = 0x0123;
        crtc.regs[14] = 0x01;
        crtc.regs[15] = 0x23;
        crtc.regs[9] = 7;
        crtc.ra = raster;
        crtc.cursor_blink_count = blink_count;
        crtc.cursor_visible()
    }

    #[test]
    fn cursor_is_present_only_between_r10_and_r11_inclusive() {
        let mut crtc = Crtc6845::new();
        crtc.regs[10] = 2;
        crtc.regs[11] = 5;
        for raster in 0..=7 {
            assert_eq!(cursor_at(&mut crtc, raster, 0), (2..=5).contains(&raster));
        }
    }

    #[test]
    fn mc6845_wraps_a_split_cursor_but_hd6845s_does_not() {
        let mut crtc = Crtc6845::new();
        crtc.regs[10] = 6;
        crtc.regs[11] = 1;
        for raster in 0..=7 {
            assert_eq!(cursor_at(&mut crtc, raster, 0), raster <= 1 || raster >= 6);
        }
        crtc.set_variant(Crtc6845Variant::Hd6845s);
        for raster in 0..=7 {
            assert!(!cursor_at(&mut crtc, raster, 0));
        }
    }

    #[test]
    fn r10_selects_steady_hidden_and_16_or_32_field_blink() {
        let mut crtc = Crtc6845::new();
        crtc.regs[11] = 0;
        for (mode, count, visible) in [
            (0x00, 0, true),
            (0x20, 0, false),
            (0x40, 15, false),
            (0x40, 16, true),
            (0x40, 32, false),
            (0x60, 31, false),
            (0x60, 32, true),
            (0x60, 64, false),
        ] {
            crtc.regs[10] = mode;
            assert_eq!(cursor_at(&mut crtc, 0, count), visible);
        }
    }

    #[test]
    fn old_explicit_state_loads_with_a_reset_blink_phase() {
        let crtc = Crtc6845::new();
        let mut old = crtc.save_state();
        old.pop();
        let mut restored = Crtc6845::new();
        assert_eq!(
            restored
                .load_state(&old)
                .expect("the legacy state should load"),
            old.len()
        );
        assert_eq!(restored.cursor_blink_count, 0);
    }
}
#[cfg(test)]
mod variant_tests {
    use super::*;

    /// The start address is the whole difference between a CPC type 0 and a
    /// type 2, and it is what real software detects the part by.
    #[test]
    fn only_the_hd6845s_reads_the_start_address_back() {
        for (variant, r12_reads) in [
            (Crtc6845Variant::Mc6845, false),
            (Crtc6845Variant::Hd6845s, true),
        ] {
            let mut crtc = Crtc6845::new();
            crtc.set_variant(variant);
            crtc.write_address(12);
            crtc.write_data(0x30);
            assert_eq!(crtc.read_data() != 0, r12_reads, "{variant:?} R12 readback");

            // Cursor and light pen read back on every part.
            for reg in 14..=17u8 {
                crtc.write_address(reg);
                crtc.write_data(0x2A);
                assert_eq!(crtc.read_data(), 0x2A, "{variant:?} R{reg} readback");
            }

            // Timing registers never do.
            crtc.write_address(0);
            crtc.write_data(0x3F);
            assert_eq!(crtc.read_data(), 0, "{variant:?} R0 should be write-only");
        }
    }

    /// A snapshot written before the variant existed decodes as the part this
    /// model has always behaved like, not as the CPC's.
    #[test]
    fn the_default_is_the_old_behaviour() {
        assert_eq!(Crtc6845Variant::default(), Crtc6845Variant::Mc6845);
        assert!(!Crtc6845Variant::default().reads_back(12));
    }
}
