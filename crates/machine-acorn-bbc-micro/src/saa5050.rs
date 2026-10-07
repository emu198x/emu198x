//! Mullard SAA5050 teletext character generator: the chip behind MODE 7.
//!
//! The SAA5050 takes one seven-bit character per microsecond and turns it
//! into a cell of six dots by ten lines a field. Everything about the picture
//! comes from three inputs besides the data, wired on the BBC to the 6845:
//!
//! - **LOSE** (DISPTMG): falls at the end of each displayed line. The chip
//!   counts these to know which of the ten lines of a character row it is on,
//!   and starts each line from the default attributes.
//! - **DEW** (VSYNC): falls at the end of vertical sync. It restarts the line
//!   count and steps the field counter that times flashing.
//! - **CRS** (RA0): picks which of the two interpolated lines character
//!   rounding draws. In MODE 7's interlace sync and video mode RA0 is the
//!   field, so the even field draws each glyph line smoothed towards the line
//!   above and the odd field towards the line below.
//!
//! The chip keeps its own line count rather than reading the 6845's raster
//! address, so MODE 7 depends on the 6845 giving it ten lines a field. The
//! Advanced User Guide warns against turning interlace off in MODE 7 because
//! "the character set stored in the SAA 5050 is designed to be used with
//! interlace on" (§2.20, `*TV`).
//!
//! # Sources
//!
//! The SAA5050 datasheet is not in the reference library. The control codes
//! and their behaviour come from the BBC's own teletext documentation — the
//! *Advanced Teletext System User Guide* (BBC Enterprises, 1986), §6.4, for
//! double height, hold graphics, flashing and concealed characters — and the
//! reference emulators: jsbeeb's `src/teletext.js` (the model this follows
//! most closely), b-em's `mode7_render` in `src/video.c`, MAME's
//! `saa5050.cpp`, and the BBC Micro MiSTer core's `saa5050.vhd`. Where they
//! disagree it is said at the point concerned.

use serde::{Deserialize, Serialize};

/// Half-dots across a cell: six dots, each split in two so character
/// rounding can light half of one. This module calls them pixels; the machine
/// draws the twelve across a microsecond of its framebuffer, sixteen pixels.
pub(crate) const CELL_WIDTH: usize = 12;

/// Lines of a character row the chip scans in one field.
const ROW_LINES: u8 = 10;

/// Glyph lines in a character, with rounding: two per dot row.
const GLYPH_LINES: i16 = 20;

/// Fields in one flash cycle, and how many of them hide flashing text.
///
/// jsbeeb, MAME and MiSTer agree on a 64-field cycle with text hidden for 16
/// (MiSTer cites the datasheet's "0.75 Hz with a 3:1 on/off ratio"). b-em
/// uses 48 fields, 32 shown and 16 hidden. Where the cycle starts at power-on
/// is not known; this starts with the hidden phase, as MAME and jsbeeb do.
const FLASH_FIELDS: u8 = 64;
const FLASH_HIDDEN_FIELDS: u8 = 16;

/// The character the chip shows for a control code with nothing held.
const SPACE: u8 = 0x20;

/// The display attributes, which control codes change as a line is scanned.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct Attributes {
    fg: u8,
    bg: u8,
    graphics: bool,
    separated: bool,
    hold: bool,
    /// The mosaic hold graphics repeats, and whether it was separated.
    held: u8,
    held_separated: bool,
    flash: bool,
    double: bool,
    conceal: bool,
}

impl Attributes {
    /// White alphanumerics on black, the state at the start of every line.
    const fn new() -> Self {
        Self {
            fg: 7,
            bg: 0,
            graphics: false,
            separated: false,
            hold: false,
            held: SPACE,
            held_separated: false,
            flash: false,
            double: false,
            conceal: false,
        }
    }

    /// Act on a control code (`$00-$1F`).
    ///
    /// Each code takes effect either at its own cell ("set-at") or from the
    /// next ("set-after"). Here every code changes the state at once, and
    /// [`Saa5050::character`] draws the code's own cell from the state before
    /// it wherever the code is set-after: the foreground colour, flash,
    /// double height and the end of hold and conceal.
    fn control(&mut self, code: u8) {
        match code {
            0x01..=0x07 => {
                self.graphics = false;
                self.fg = code;
                self.conceal = false;
            }
            0x08 => self.flash = true,
            0x09 => self.flash = false,
            0x0C => self.double = false,
            0x0D => self.double = true,
            0x11..=0x17 => {
                self.graphics = true;
                self.fg = code & 0x07;
                self.conceal = false;
            }
            // Conceal. b-em, jsbeeb and MAME make the foreground the
            // background colour; MiSTer keeps a separate flag, as here, so a
            // new background (`$1D`) after it still takes the real
            // foreground.
            0x18 => self.conceal = true,
            0x19 => self.separated = false,
            0x1A => self.separated = true,
            0x1C => self.bg = 0,
            0x1D => self.bg = self.fg,
            0x1E => self.hold = true,
            0x1F => self.hold = false,
            // Black ($00, $10) is not a teletext colour and does nothing, nor
            // do the box and shift codes on the BBC.
            _ => {}
        }
    }
}

/// One cell of output: a line of twelve half-dots, leftmost in bit 11, and
/// its colours as three-bit RGB.
pub(crate) struct Cell {
    pub(crate) pattern: u16,
    pub(crate) fg: u8,
    pub(crate) bg: u8,
}

/// The SAA5050's state between character clocks.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Saa5050 {
    attributes: Attributes,
    /// Line of the character row being scanned, 0-9, counted on LOSE.
    line: u8,
    /// A double-height code has appeared on the line being scanned.
    double_seen: bool,
    /// This row shows the lower halves of the double-height characters above.
    lower_half: bool,
    /// Fields counted on DEW, for flashing.
    flash_count: u8,
    /// The last levels of LOSE and DEW, to find their falling edges.
    lose: bool,
    dew: bool,
}

impl Saa5050 {
    pub(crate) const fn new() -> Self {
        Self {
            attributes: Attributes::new(),
            line: 0,
            double_seen: false,
            lower_half: false,
            flash_count: 0,
            lose: false,
            dew: false,
        }
    }

    /// Present the LOSE (display enable) and DEW (vertical sync) inputs for
    /// one character clock. The chip acts on their falling edges.
    pub(crate) fn clock_timing(&mut self, lose: bool, dew: bool) {
        if self.dew && !dew {
            self.line = 0;
            self.lower_half = false;
            self.flash_count = (self.flash_count + 1) % FLASH_FIELDS;
        }
        if self.lose && !lose {
            self.attributes = Attributes::new();
            self.line += 1;
            if self.line == ROW_LINES {
                self.line = 0;
                // A row with double height on it is followed by its lower
                // half, but a lower half is never itself the top of another.
                self.lower_half = !self.lower_half && self.double_seen;
            }
            self.double_seen = false;
        }
        self.lose = lose;
        self.dew = dew;
    }

    /// Draw one character. `code` is the seven bits on D0-D6, `crs` the
    /// character rounding select input, and `font` the 96-glyph character
    /// ROM, ten bytes a glyph with dot 0 in bit 5.
    pub(crate) fn character(&mut self, code: u8, crs: bool, font: &[u8]) -> Cell {
        let before = self.attributes;
        let mut shown = code;
        let mut graphics = before.graphics;
        let mut separated = before.separated;
        if code < 0x20 {
            self.attributes.control(code);
            if code == 0x0D {
                self.double_seen = true;
            }
            let after = &mut self.attributes;
            // A control code shows as a space, or under hold graphics as the
            // last mosaic, in the separation it was drawn with. Hold is
            // set-at and release set-after, so either being on shows it; a
            // change of height loses it.
            if before.graphics && (before.hold || after.hold) && after.double == before.double {
                shown = after.held;
                graphics = true;
                separated = after.held_separated;
            } else {
                // jsbeeb, MAME and b-em forget the held mosaic at any control
                // code that does not show it; the Advanced Teletext System
                // User Guide says only a change of alphanumerics/graphics or
                // of height does, and MiSTer forgets it only on height. This
                // follows the three that agree.
                after.held = SPACE;
                shown = SPACE;
            }
        } else if before.graphics {
            // Only mosaics are held; capitals in graphics mode leave the held
            // mosaic alone.
            if code & 0x20 != 0 {
                self.attributes.held = code;
                self.attributes.held_separated = before.separated;
            }
        } else {
            self.attributes.held = SPACE;
        }
        let after = self.attributes;

        // Double height draws the glyph's top half on its own row and the
        // bottom half on the next, each glyph line on two lines of a field.
        let glyph_line = if before.double {
            self.line + if self.lower_half { ROW_LINES } else { 0 }
        } else {
            self.line * 2 + u8::from(crs)
        };
        // Flash is set-after and steady set-at; conceal is set-at and ended,
        // set-after, by a colour. A lower-half row shows only double-height
        // characters.
        let hidden = (before.flash && after.flash && self.flash_count < FLASH_HIDDEN_FIELDS)
            || before.conceal
            || after.conceal
            || (self.lower_half && !after.double);
        let pattern = if hidden {
            0
        } else if graphics && shown & 0x20 != 0 {
            mosaic_line(shown, usize::from(glyph_line >> 1), separated)
        } else {
            rounded_line(font, shown, glyph_line)
        };
        Cell {
            pattern,
            fg: before.fg,
            bg: after.bg,
        }
    }
}

/// One line of an alphanumeric glyph with character rounding: `line` is
/// 0-19, two per row of the 5×9 dot matrix.
///
/// Each dot is drawn two pixels wide. Where the glyph has a diagonal — a dot
/// on this row and one beside it on the neighbouring row, with neither
/// corner between them lit — the chip lights the half of the empty corner
/// that touches this row's dot, so diagonals step by half a dot. The even
/// line of each pair is smoothed towards the row above and the odd line
/// towards the row below. This is jsbeeb's `combineRows` and MAME's
/// `character_rounding`, which agree.
fn rounded_line(font: &[u8], code: u8, line: u8) -> u16 {
    let glyph = usize::from(code.saturating_sub(SPACE)) * usize::from(ROW_LINES);
    let row = |line: i16| -> u16 {
        if !(0..GLYPH_LINES).contains(&line) {
            return 0;
        }
        let dots = font.get(glyph + (line >> 1) as usize).copied().unwrap_or(0);
        (0..6).fold(0, |pixels, dot| {
            if dots & (1 << dot) != 0 {
                pixels | (0b11 << (dot * 2))
            } else {
                pixels
            }
        })
    };
    let line = i16::from(line);
    let this = row(line);
    let next = row(if line & 1 == 1 { line + 1 } else { line - 1 });
    (this | ((this >> 1) & next & !(next >> 1)) | ((this << 1) & next & !(next << 1))) & 0x0FFF
}

/// One line of a 2×3 mosaic block. `row` is the dot row, 0-9. The bits of the
/// code are the sixels: 0 top left, 1 top right, 2 middle left, 3 middle
/// right, 4 bottom left, 6 bottom right; the rows split three, four, three.
/// Separated graphics blank the left column of each half and the last row of
/// each block. Mosaics are not rounded.
fn mosaic_line(code: u8, row: usize, separated: bool) -> u16 {
    let (left, right, last) = match row {
        0..=2 => (0x01u8, 0x02u8, 2),
        3..=6 => (0x04, 0x08, 6),
        _ => (0x10, 0x40, 9),
    };
    let mut pixels = 0u16;
    if code & left != 0 {
        pixels |= 0xFC0;
    }
    if code & right != 0 {
        pixels |= 0x03F;
    }
    if separated {
        pixels &= 0x3CF;
        if row == last {
            pixels = 0;
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font with one glyph, `A`, given as its ten dot rows.
    fn font_with_a(rows: [u8; 10]) -> Vec<u8> {
        let mut font = vec![0u8; 96 * 10];
        let a = usize::from(b'A' - SPACE) * 10;
        font[a..a + 10].copy_from_slice(&rows);
        font
    }

    /// The lit pixels of a 12-pixel line, leftmost first.
    fn lit(pattern: u16) -> Vec<usize> {
        (0..CELL_WIDTH)
            .filter(|&x| pattern & (1 << (CELL_WIDTH - 1 - x)) != 0)
            .collect()
    }

    /// A single diagonal step: dot 2 (pixels 6-7) on row 1, dot 3 (pixels
    /// 4-5) on row 2. The lower line of row 1 gains pixel 5, half of the
    /// corner towards row 2's dot, and the upper line of row 2 gains pixel 6.
    #[test]
    fn rounding_lights_half_a_dot_on_a_diagonal() {
        let font = font_with_a([0, 0x04, 0x08, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(lit(rounded_line(&font, b'A', 2)), [6, 7], "row 1, upper");
        assert_eq!(lit(rounded_line(&font, b'A', 3)), [5, 6, 7], "row 1, lower");
        assert_eq!(lit(rounded_line(&font, b'A', 4)), [4, 5, 6], "row 2, upper");
        assert_eq!(lit(rounded_line(&font, b'A', 5)), [4, 5], "row 2, lower");
    }

    /// A vertical stroke has no diagonal, so rounding adds nothing.
    #[test]
    fn rounding_leaves_straight_strokes_alone() {
        let font = font_with_a([0, 0x10, 0x10, 0x10, 0, 0, 0, 0, 0, 0]);
        for line in 2..8 {
            assert_eq!(lit(rounded_line(&font, b'A', line)), [2, 3], "line {line}");
        }
    }

    #[test]
    fn separated_mosaics_blank_the_left_column_and_last_row_of_each_block() {
        assert_eq!(
            lit(mosaic_line(0x7F, 0, false)),
            (0..12).collect::<Vec<_>>()
        );
        assert_eq!(lit(mosaic_line(0x7F, 0, true)), [2, 3, 4, 5, 8, 9, 10, 11]);
        assert_eq!(lit(mosaic_line(0x7F, 2, true)), Vec::<usize>::new());
    }

    /// Feed a scan line of codes and return the cells it draws.
    fn scan(chip: &mut Saa5050, codes: &[u8], crs: bool, font: &[u8]) -> Vec<Cell> {
        chip.clock_timing(true, false);
        let cells = codes
            .iter()
            .map(|&c| chip.character(c, crs, font))
            .collect();
        chip.clock_timing(false, false);
        cells
    }

    /// The fields between flash phase changes, from DEW's falling edges.
    #[test]
    fn flashing_text_is_hidden_for_16_fields_in_64() {
        let font = font_with_a([0x3F; 10]);
        let mut chip = Saa5050::new();
        let mut shown = Vec::new();
        for _ in 0..192 {
            chip.clock_timing(false, true);
            chip.clock_timing(false, false);
            let cells = scan(&mut chip, &[0x08, b'A'], false, &font);
            shown.push(cells[1].pattern != 0);
        }
        let mut runs = Vec::new();
        let mut run = 1;
        for pair in shown.windows(2) {
            if pair[0] == pair[1] {
                run += 1;
            } else {
                runs.push((pair[0], run));
                run = 1;
            }
        }
        assert_eq!(
            &runs[1..5],
            [(true, 48), (false, 16), (true, 48), (false, 16)]
        );
    }
}
