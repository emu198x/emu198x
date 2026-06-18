//! On-screen diagnostic overlay for fatal/halt conditions.
//!
//! When a [`crate::UiSystem`] reports a halt (e.g. a CPU JAM — usually a bad
//! ROM dump), the harness renders this overlay instead of the frozen frame so
//! the cause is unmissable and self-explanatory, on every system, with no
//! external dependencies. Text is drawn with a small hand-authored 8x8 bitmap
//! font (uppercase ASCII + the punctuation our messages use).

/// RGBA clear colour for the overlay background (dark slate).
const BG: [u8; 4] = [16, 16, 24, 255];
/// Header text colour (amber).
const HEADER: [u8; 4] = [255, 200, 60, 255];
/// Accent bar colour (red).
const ACCENT: [u8; 4] = [200, 40, 40, 255];
/// Body text colour (near-white).
const BODY: [u8; 4] = [230, 230, 235, 255];
/// Hint text colour (grey).
const HINT: [u8; 4] = [150, 150, 160, 255];

const GLYPH_W: u32 = 6; // 5px cell + 1px gap
const GLYPH_H: u32 = 8;

const HEADER_TEXT: &str = "EMULATION HALTED";
const HINT_TEXT: &str = "F12 RESET   ESC QUIT";

/// Builds an RGBA8888 overlay frame of `width`x`height` showing `message`,
/// word-wrapped and centred under a header, with a reset/quit hint.
#[must_use]
pub fn build_halt_overlay(width: u32, height: u32, message: &str) -> Vec<u8> {
    let mut buf = vec![0u8; (width as usize) * (height as usize) * 4];
    for px in buf.chunks_exact_mut(4) {
        px.copy_from_slice(&BG);
    }

    // Larger framebuffers get a larger font so text stays legible.
    let scale = (width / 200).max(1);
    let line_h = (GLYPH_H + 2) * scale;
    let max_cols = ((width / (GLYPH_W * scale)).max(1)).saturating_sub(1) as usize;

    let wrapped = wrap(message, max_cols.max(1));
    let total_lines = 1 + 1 + wrapped.len() as u32 + 1 + 1; // header, gap, body…, gap, hint
    let block_h = total_lines * line_h;
    let mut y = (height.saturating_sub(block_h)) / 2;

    let mut canvas = Canvas {
        buf: &mut buf,
        width,
        height,
    };

    // Accent bar above the header, then header / wrapped body / hint.
    let bar_y = i32::try_from(y.saturating_sub(line_h / 2)).unwrap_or(0);
    canvas.fill_rect(0, bar_y, width, 2 * scale, ACCENT);
    canvas.draw_centered(y, HEADER_TEXT, HEADER, scale);
    y += line_h * 2;
    for line in &wrapped {
        canvas.draw_centered(y, line, BODY, scale);
        y += line_h;
    }
    y += line_h;
    canvas.draw_centered(y, HINT_TEXT, HINT, scale);

    buf
}

/// Greedy word-wrap to `max_cols` columns (uppercased).
fn wrap(message: &str, max_cols: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in message.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() <= max_cols {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// A mutable RGBA8888 pixel buffer the overlay draws into. Groups the
/// buffer + geometry so the draw helpers take few arguments.
struct Canvas<'a> {
    buf: &'a mut [u8],
    width: u32,
    height: u32,
}

impl Canvas<'_> {
    fn fill_rect(&mut self, x: i32, y: i32, w: u32, h: u32, c: [u8; 4]) {
        for dy in 0..h {
            for dx in 0..w {
                let px = x + i32::try_from(dx).unwrap_or(0);
                let py = y + i32::try_from(dy).unwrap_or(0);
                if px < 0 || py < 0 {
                    continue;
                }
                let (px, py) = (px as u32, py as u32);
                if px >= self.width || py >= self.height {
                    continue;
                }
                let idx = ((py * self.width + px) * 4) as usize;
                self.buf[idx..idx + 4].copy_from_slice(&c);
            }
        }
    }

    /// Blits `text` at (`x`,`y`), uppercased, scaled.
    fn draw_text(&mut self, x: i32, y: i32, text: &str, color: [u8; 4], scale: u32) {
        let mut cx = x;
        for ch in text.chars() {
            let g = glyph(ch.to_ascii_uppercase());
            for (row, bits) in g.iter().enumerate() {
                for col in 0..8u32 {
                    if bits & (0x80 >> col) == 0 {
                        continue;
                    }
                    let bx = cx + i32::try_from(col * scale).unwrap_or(0);
                    let by = y + i32::try_from(row as u32 * scale).unwrap_or(0);
                    self.fill_rect(bx, by, scale, scale, color);
                }
            }
            cx += i32::try_from(GLYPH_W * scale).unwrap_or(0);
        }
    }

    /// Draws `text` horizontally centred at row `y`.
    fn draw_centered(&mut self, y: u32, text: &str, color: [u8; 4], scale: u32) {
        let text_w = text.chars().count() as u32 * GLYPH_W * scale;
        let x = i32::try_from(self.width.saturating_sub(text_w) / 2).unwrap_or(0);
        self.draw_text(x, i32::try_from(y).unwrap_or(0), text, color, scale);
    }
}

/// 8x8 glyph for a character (bit `0x80` = leftmost). Unknown → blank.
fn glyph(c: char) -> [u8; 8] {
    match c {
        'A' => [0x70, 0x88, 0x88, 0x88, 0xF8, 0x88, 0x88, 0x00],
        'B' => [0xF0, 0x88, 0x88, 0xF0, 0x88, 0x88, 0xF0, 0x00],
        'C' => [0x70, 0x88, 0x80, 0x80, 0x80, 0x88, 0x70, 0x00],
        'D' => [0xF0, 0x88, 0x88, 0x88, 0x88, 0x88, 0xF0, 0x00],
        'E' => [0xF8, 0x80, 0x80, 0xF0, 0x80, 0x80, 0xF8, 0x00],
        'F' => [0xF8, 0x80, 0x80, 0xF0, 0x80, 0x80, 0x80, 0x00],
        'G' => [0x70, 0x88, 0x80, 0xB8, 0x88, 0x88, 0x70, 0x00],
        'H' => [0x88, 0x88, 0x88, 0xF8, 0x88, 0x88, 0x88, 0x00],
        'I' => [0xF8, 0x20, 0x20, 0x20, 0x20, 0x20, 0xF8, 0x00],
        'J' => [0x38, 0x10, 0x10, 0x10, 0x90, 0x90, 0x60, 0x00],
        'K' => [0x88, 0x90, 0xA0, 0xC0, 0xA0, 0x90, 0x88, 0x00],
        'L' => [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0xF8, 0x00],
        'M' => [0x88, 0xD8, 0xA8, 0xA8, 0x88, 0x88, 0x88, 0x00],
        'N' => [0x88, 0xC8, 0xA8, 0x98, 0x88, 0x88, 0x88, 0x00],
        'O' => [0x70, 0x88, 0x88, 0x88, 0x88, 0x88, 0x70, 0x00],
        'P' => [0xF0, 0x88, 0x88, 0xF0, 0x80, 0x80, 0x80, 0x00],
        'Q' => [0x70, 0x88, 0x88, 0x88, 0xA8, 0x90, 0x68, 0x00],
        'R' => [0xF0, 0x88, 0x88, 0xF0, 0xA0, 0x90, 0x88, 0x00],
        'S' => [0x70, 0x88, 0x80, 0x70, 0x08, 0x88, 0x70, 0x00],
        'T' => [0xF8, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x00],
        'U' => [0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x70, 0x00],
        'V' => [0x88, 0x88, 0x88, 0x88, 0x88, 0x50, 0x20, 0x00],
        'W' => [0x88, 0x88, 0x88, 0xA8, 0xA8, 0xD8, 0x88, 0x00],
        'X' => [0x88, 0x88, 0x50, 0x20, 0x50, 0x88, 0x88, 0x00],
        'Y' => [0x88, 0x88, 0x50, 0x20, 0x20, 0x20, 0x20, 0x00],
        'Z' => [0xF8, 0x08, 0x10, 0x20, 0x40, 0x80, 0xF8, 0x00],
        '0' => [0x70, 0x88, 0x98, 0xA8, 0xC8, 0x88, 0x70, 0x00],
        '1' => [0x20, 0x60, 0x20, 0x20, 0x20, 0x20, 0x70, 0x00],
        '2' => [0x70, 0x88, 0x08, 0x10, 0x20, 0x40, 0xF8, 0x00],
        '3' => [0xF8, 0x10, 0x20, 0x10, 0x08, 0x88, 0x70, 0x00],
        '4' => [0x10, 0x30, 0x50, 0x90, 0xF8, 0x10, 0x10, 0x00],
        '5' => [0xF8, 0x80, 0xF0, 0x08, 0x08, 0x88, 0x70, 0x00],
        '6' => [0x70, 0x88, 0x80, 0xF0, 0x88, 0x88, 0x70, 0x00],
        '7' => [0xF8, 0x08, 0x10, 0x20, 0x20, 0x20, 0x20, 0x00],
        '8' => [0x70, 0x88, 0x88, 0x70, 0x88, 0x88, 0x70, 0x00],
        '9' => [0x70, 0x88, 0x88, 0x78, 0x08, 0x88, 0x70, 0x00],
        '$' => [0x20, 0x78, 0xA0, 0x70, 0x28, 0xF0, 0x20, 0x00],
        '(' => [0x10, 0x20, 0x40, 0x40, 0x40, 0x20, 0x10, 0x00],
        ')' => [0x40, 0x20, 0x10, 0x10, 0x10, 0x20, 0x40, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x70, 0x00, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x60, 0x60, 0x00],
        ':' => [0x00, 0x60, 0x60, 0x00, 0x00, 0x60, 0x60, 0x00],
        '!' => [0x20, 0x20, 0x20, 0x20, 0x20, 0x00, 0x20, 0x00],
        '?' => [0x70, 0x88, 0x08, 0x10, 0x20, 0x00, 0x20, 0x00],
        '/' => [0x08, 0x08, 0x10, 0x20, 0x40, 0x80, 0x80, 0x00],
        _ => [0x00; 8], // space + unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_has_correct_size_and_draws_text() {
        let (w, h) = (160u32, 262u32);
        let buf = build_halt_overlay(w, h, "CPU HALTED (JAM) AT $18AC");
        assert_eq!(buf.len(), (w * h * 4) as usize);
        // Background present and some non-background (text/accent) pixels drawn.
        assert!(buf.chunks_exact(4).any(|p| p == BG));
        assert!(buf.chunks_exact(4).any(|p| p == BODY || p == HEADER));
    }

    #[test]
    fn glyphs_resolve_for_message_charset_and_blank_for_space() {
        assert_eq!(glyph(' '), [0x00; 8]);
        for ch in "CPU HALTED (JAM) AT $18AC - LIKELY BD ROM DUMP".chars() {
            if ch != ' ' {
                assert_ne!(glyph(ch), [0x00; 8], "missing glyph for {ch:?}");
            }
        }
    }

    #[test]
    fn wrap_splits_on_word_boundaries_within_columns() {
        let lines = wrap("ONE TWO THREE FOUR", 9);
        assert!(lines.iter().all(|l| l.len() <= 9));
        assert_eq!(lines.join(" "), "ONE TWO THREE FOUR");
    }

    /// Renders the overlay as ASCII so the font can be eyeballed under
    /// `cargo test -- --nocapture`. Prints only rows containing text.
    #[test]
    fn dump_overlay_ascii() {
        let (w, h) = (160u32, 100u32);
        let buf = build_halt_overlay(w, h, "CPU HALTED (JAM) AT $18AC - BAD ROM DUMP");
        for y in 0..h {
            let row: String = (0..w)
                .map(|x| {
                    let p = &buf[((y * w + x) * 4) as usize..][..4];
                    if p == BG { ' ' } else { '#' }
                })
                .collect();
            if row.contains('#') {
                println!("{row}");
            }
        }
    }
}
