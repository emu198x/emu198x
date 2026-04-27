//! Motorola MC6847 Video Display Generator helpers.
//!
//! This first slice models the alphanumeric text page shape used during Dragon
//! ROM bring-up. It deliberately exposes a text snapshot before pixel rendering
//! so we can validate the video RAM contents and SAM display-base wiring.

/// Alphanumeric text columns.
pub const TEXT_COLUMNS: usize = 32;
/// Alphanumeric text rows.
pub const TEXT_ROWS: usize = 16;
/// Text-mode character cell width in pixels.
pub const TEXT_CELL_WIDTH: usize = 8;
/// Text-mode character cell height in pixels.
pub const TEXT_CELL_HEIGHT: usize = 12;
/// Text-mode framebuffer width in pixels.
pub const TEXT_FRAMEBUFFER_WIDTH: usize = TEXT_COLUMNS * TEXT_CELL_WIDTH;
/// Text-mode framebuffer height in pixels.
pub const TEXT_FRAMEBUFFER_HEIGHT: usize = TEXT_ROWS * TEXT_CELL_HEIGHT;
/// Text-mode framebuffer size in pixels.
pub const TEXT_FRAMEBUFFER_PIXELS: usize = TEXT_FRAMEBUFFER_WIDTH * TEXT_FRAMEBUFFER_HEIGHT;
/// Left text-mode border width in pixels.
pub const TEXT_LEFT_BORDER_PIXELS: usize = 60;
/// Right text-mode border width in pixels.
pub const TEXT_RIGHT_BORDER_PIXELS: usize = 56;
/// Top text-mode border height in scanlines.
pub const TEXT_TOP_BORDER_LINES: usize = 25;
/// Bottom text-mode border height in scanlines.
pub const TEXT_BOTTOM_BORDER_LINES: usize = 26;
/// Visible text-mode framebuffer width including borders.
pub const TEXT_VISIBLE_FRAMEBUFFER_WIDTH: usize =
    TEXT_LEFT_BORDER_PIXELS + TEXT_FRAMEBUFFER_WIDTH + TEXT_RIGHT_BORDER_PIXELS;
/// Visible text-mode framebuffer height including borders.
pub const TEXT_VISIBLE_FRAMEBUFFER_HEIGHT: usize =
    TEXT_TOP_BORDER_LINES + TEXT_FRAMEBUFFER_HEIGHT + TEXT_BOTTOM_BORDER_LINES;
/// Visible text-mode framebuffer size in pixels.
pub const TEXT_VISIBLE_FRAMEBUFFER_PIXELS: usize =
    TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT;
/// Bytes consumed by one MC6847 text screen.
pub const TEXT_SCREEN_BYTES: usize = TEXT_COLUMNS * TEXT_ROWS;

/// Default Dragon alpha-mode border colour, ARGB8888.
///
/// This is MC6847 `VDG_BLACK` through XRoar's default "ideal" VDG voltage
/// palette and PAL display conversion. It is intentionally not pure black.
pub const DEFAULT_TEXT_BORDER: u32 = 0xFF05_0505;
/// Default Dragon alpha-mode background colour, ARGB8888.
///
/// This is MC6847 `VDG_DARK_GREEN` through the same reference conversion.
pub const DEFAULT_TEXT_BACKGROUND: u32 = 0xFF00_1000;
/// Default Dragon alpha-mode foreground colour, ARGB8888.
///
/// This is MC6847 `VDG_GREEN` through the same reference conversion.
pub const DEFAULT_TEXT_FOREGROUND: u32 = 0xFF1D_AA1D;

/// Decoded MC6847 text cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextCell {
    /// Original byte read from display memory.
    pub raw: u8,
    /// Displayable ASCII approximation for diagnostics.
    pub ch: char,
    /// Whether the cell has the inverse-video bit set.
    pub inverse: bool,
}

/// A 32x16 alphanumeric text snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextScreen {
    cells: [TextCell; TEXT_SCREEN_BYTES],
}

impl TextScreen {
    /// Capture a 32x16 text screen by reading sequential display bytes.
    #[must_use]
    pub fn capture(mut read_byte: impl FnMut(usize) -> u8) -> Self {
        let cells = std::array::from_fn(|index| decode_text_byte(read_byte(index)));
        Self { cells }
    }

    /// Return the decoded text cell at `row`, `column`.
    #[must_use]
    pub fn cell(&self, row: usize, column: usize) -> Option<TextCell> {
        if row >= TEXT_ROWS || column >= TEXT_COLUMNS {
            return None;
        }
        Some(self.cells[row * TEXT_COLUMNS + column])
    }

    /// Return all cells in row-major order.
    #[must_use]
    pub const fn cells(&self) -> &[TextCell; TEXT_SCREEN_BYTES] {
        &self.cells
    }

    /// Return plain diagnostic text with one line per VDG text row.
    #[must_use]
    pub fn to_plain_text(&self) -> String {
        let mut text = String::with_capacity((TEXT_COLUMNS + 1) * TEXT_ROWS);
        for row in 0..TEXT_ROWS {
            if row != 0 {
                text.push('\n');
            }
            for column in 0..TEXT_COLUMNS {
                text.push(self.cells[row * TEXT_COLUMNS + column].ch);
            }
        }
        text
    }

    /// Render the text screen to an ARGB8888 framebuffer.
    #[must_use]
    pub fn render_argb(&self, palette: TextPalette) -> Vec<u32> {
        let mut framebuffer = vec![palette.background; TEXT_FRAMEBUFFER_PIXELS];
        self.render_argb_into(palette, &mut framebuffer);
        framebuffer
    }

    /// Render the text screen into an existing ARGB8888 framebuffer.
    pub fn render_argb_into(&self, palette: TextPalette, framebuffer: &mut [u32]) {
        assert_eq!(framebuffer.len(), TEXT_FRAMEBUFFER_PIXELS);

        for row in 0..TEXT_ROWS {
            for column in 0..TEXT_COLUMNS {
                let cell = self.cells[row * TEXT_COLUMNS + column];
                render_cell(
                    row,
                    column,
                    cell,
                    palette,
                    RenderTarget {
                        framebuffer,
                        width: TEXT_FRAMEBUFFER_WIDTH,
                        x_origin: 0,
                        y_origin: 0,
                    },
                );
            }
        }
    }

    /// Render the text screen plus visible border to an ARGB8888 framebuffer.
    #[must_use]
    pub fn render_visible_argb(&self, palette: TextPalette) -> Vec<u32> {
        let mut framebuffer = vec![palette.border; TEXT_VISIBLE_FRAMEBUFFER_PIXELS];
        self.render_visible_argb_into(palette, &mut framebuffer);
        framebuffer
    }

    /// Render the text screen plus visible border into an existing ARGB8888 framebuffer.
    pub fn render_visible_argb_into(&self, palette: TextPalette, framebuffer: &mut [u32]) {
        assert_eq!(framebuffer.len(), TEXT_VISIBLE_FRAMEBUFFER_PIXELS);
        framebuffer.fill(palette.border);

        for row in 0..TEXT_ROWS {
            for column in 0..TEXT_COLUMNS {
                let cell = self.cells[row * TEXT_COLUMNS + column];
                render_cell(
                    row,
                    column,
                    cell,
                    palette,
                    RenderTarget {
                        framebuffer,
                        width: TEXT_VISIBLE_FRAMEBUFFER_WIDTH,
                        x_origin: TEXT_LEFT_BORDER_PIXELS,
                        y_origin: TEXT_TOP_BORDER_LINES,
                    },
                );
            }
        }
    }
}

/// Text-mode render palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextPalette {
    /// Border colour, ARGB8888.
    pub border: u32,
    /// Background colour, ARGB8888.
    pub background: u32,
    /// Foreground colour, ARGB8888.
    pub foreground: u32,
}

impl Default for TextPalette {
    fn default() -> Self {
        Self {
            border: DEFAULT_TEXT_BORDER,
            background: DEFAULT_TEXT_BACKGROUND,
            foreground: DEFAULT_TEXT_FOREGROUND,
        }
    }
}

/// Default MC6847 yellow colour, ARGB8888.
pub const DEFAULT_VDG_YELLOW: u32 = 0xFFFF_FF83;
/// Default MC6847 blue colour, ARGB8888.
pub const DEFAULT_VDG_BLUE: u32 = 0xFF24_3FA8;
/// Default MC6847 red colour, ARGB8888.
pub const DEFAULT_VDG_RED: u32 = 0xFFB8_2B25;
/// Default MC6847 buff colour, ARGB8888.
pub const DEFAULT_VDG_BUFF: u32 = 0xFFFF_DF91;
/// Default MC6847 cyan colour, ARGB8888.
pub const DEFAULT_VDG_CYAN: u32 = 0xFF46_BFC2;
/// Default MC6847 magenta colour, ARGB8888.
pub const DEFAULT_VDG_MAGENTA: u32 = 0xFFB0_4CC0;
/// Default MC6847 orange colour, ARGB8888.
pub const DEFAULT_VDG_ORANGE: u32 = 0xFFE6_7F2E;

/// Full MC6847 render palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VdgPalette {
    /// Border colour, ARGB8888.
    pub border: u32,
    /// Alpha-mode background colour, ARGB8888.
    pub text_background: u32,
    /// Alpha-mode foreground colour, ARGB8888.
    pub text_foreground: u32,
    /// Universal black colour, ARGB8888.
    pub black: u32,
    /// Eight MC6847 chroma colours, indexed as CSS set plus colour code.
    pub colours: [u32; 8],
}

impl Default for VdgPalette {
    fn default() -> Self {
        Self {
            border: DEFAULT_TEXT_BORDER,
            text_background: DEFAULT_TEXT_BACKGROUND,
            text_foreground: DEFAULT_TEXT_FOREGROUND,
            black: DEFAULT_TEXT_BORDER,
            colours: [
                DEFAULT_TEXT_FOREGROUND,
                DEFAULT_VDG_YELLOW,
                DEFAULT_VDG_BLUE,
                DEFAULT_VDG_RED,
                DEFAULT_VDG_BUFF,
                DEFAULT_VDG_CYAN,
                DEFAULT_VDG_MAGENTA,
                DEFAULT_VDG_ORANGE,
            ],
        }
    }
}

impl From<TextPalette> for VdgPalette {
    fn from(value: TextPalette) -> Self {
        Self {
            border: value.border,
            text_background: value.background,
            text_foreground: value.foreground,
            ..Self::default()
        }
    }
}

/// MC6847 control-line state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VdgControl {
    /// A/G line. `true` selects full graphics.
    pub graphics: bool,
    /// CSS colour-set select.
    pub css: bool,
    /// INT/EXT line in alpha mode.
    pub int_ext: bool,
    /// GM0..GM2 graphics mode value.
    pub gm: u8,
}

impl VdgControl {
    /// Decode the Dragon PIA1 port B VDG control bits.
    #[must_use]
    pub const fn from_dragon_pia1_port_b(value: u8) -> Self {
        Self {
            graphics: value & 0x80 != 0,
            css: value & 0x08 != 0,
            int_ext: value & 0x10 != 0,
            gm: (value >> 4) & 0x07,
        }
    }
}

/// Render the active MC6847 mode plus visible border to an ARGB8888 framebuffer.
#[must_use]
pub fn render_visible_argb(
    read_byte: impl FnMut(usize) -> u8,
    control: VdgControl,
    palette: VdgPalette,
) -> Vec<u32> {
    let mut framebuffer = vec![palette.border; TEXT_VISIBLE_FRAMEBUFFER_PIXELS];
    render_visible_argb_into(read_byte, control, palette, &mut framebuffer);
    framebuffer
}

/// Render the active MC6847 mode plus visible border into an existing framebuffer.
pub fn render_visible_argb_into(
    read_byte: impl FnMut(usize) -> u8,
    control: VdgControl,
    palette: VdgPalette,
    framebuffer: &mut [u32],
) {
    assert_eq!(framebuffer.len(), TEXT_VISIBLE_FRAMEBUFFER_PIXELS);
    framebuffer.fill(palette.border);

    if control.graphics {
        render_graphics_into(read_byte, control, palette, framebuffer);
    } else {
        render_alpha_semigraphics_into(read_byte, control, palette, framebuffer);
    }
}

/// Decode one MC6847 alphanumeric byte into a diagnostic text cell.
#[must_use]
pub fn decode_text_byte(raw: u8) -> TextCell {
    TextCell {
        raw,
        ch: diagnostic_char(raw),
        inverse: raw & 0x40 != 0,
    }
}

fn diagnostic_char(raw: u8) -> char {
    let code = raw & 0x3F;
    match code {
        0x00 => '@',
        0x01..=0x1A => char::from(b'A' + code - 1),
        0x1B => '[',
        0x1C => '\\',
        0x1D => ']',
        0x1E => '^',
        0x1F => '_',
        0x20 => ' ',
        0x21 => '!',
        0x22 => '"',
        0x23 => '#',
        0x24 => '$',
        0x25 => '%',
        0x26 => '&',
        0x27 => '\'',
        0x28 => '(',
        0x29 => ')',
        0x2A => '*',
        0x2B => '+',
        0x2C => ',',
        0x2D => '-',
        0x2E => '.',
        0x2F => '/',
        0x30..=0x39 => char::from(b'0' + code - 0x30),
        0x3A => ':',
        0x3B => ';',
        0x3C => '<',
        0x3D => '=',
        0x3E => '>',
        0x3F => '?',
        _ => unreachable!("6-bit character code is always in range"),
    }
}

struct RenderTarget<'a> {
    framebuffer: &'a mut [u32],
    width: usize,
    x_origin: usize,
    y_origin: usize,
}

fn render_cell(
    row: usize,
    column: usize,
    cell: TextCell,
    palette: TextPalette,
    target: RenderTarget<'_>,
) {
    let glyph_index = usize::from(cell.raw & 0x3F);
    let glyph_base = glyph_index * TEXT_CELL_HEIGHT;
    let (background, foreground) = if cell.inverse {
        (palette.foreground, palette.background)
    } else {
        (palette.background, palette.foreground)
    };

    for y in 0..TEXT_CELL_HEIGHT {
        let bits = FONT_6847[glyph_base + y];
        let framebuffer_y = target.y_origin + row * TEXT_CELL_HEIGHT + y;
        for x in 0..TEXT_CELL_WIDTH {
            let lit = glyph_pixel(bits, x);
            let colour = if lit { foreground } else { background };
            let framebuffer_x = target.x_origin + column * TEXT_CELL_WIDTH + x;
            target.framebuffer[framebuffer_y * target.width + framebuffer_x] = colour;
        }
    }
}

fn glyph_pixel(bits: u8, x: usize) -> bool {
    bits & (0x80 >> x) != 0
}

fn render_alpha_semigraphics_into(
    mut read_byte: impl FnMut(usize) -> u8,
    control: VdgControl,
    palette: VdgPalette,
    framebuffer: &mut [u32],
) {
    let text_palette = TextPalette {
        border: palette.border,
        background: palette.text_background,
        foreground: palette.text_foreground,
    };
    for row in 0..TEXT_ROWS {
        for column in 0..TEXT_COLUMNS {
            let raw = read_byte(row * TEXT_COLUMNS + column);
            if raw & 0x80 == 0 {
                render_cell(
                    row,
                    column,
                    decode_text_byte(raw),
                    text_palette,
                    RenderTarget {
                        framebuffer,
                        width: TEXT_VISIBLE_FRAMEBUFFER_WIDTH,
                        x_origin: TEXT_LEFT_BORDER_PIXELS,
                        y_origin: TEXT_TOP_BORDER_LINES,
                    },
                );
            } else if control.int_ext {
                render_semigraphics6_cell(row, column, raw, control, palette, framebuffer);
            } else {
                render_semigraphics4_cell(row, column, raw, palette, framebuffer);
            }
        }
    }
}

fn render_semigraphics4_cell(
    row: usize,
    column: usize,
    raw: u8,
    palette: VdgPalette,
    framebuffer: &mut [u32],
) {
    let colour = palette.colours[usize::from((raw >> 4) & 0x07)];
    for sub_y in 0..2 {
        for sub_x in 0..2 {
            let bit = semigraphics_bit(sub_y, sub_x, 2);
            let lit = raw & (1 << bit) != 0;
            let fill = if lit { colour } else { palette.black };
            fill_rect(
                framebuffer,
                TEXT_LEFT_BORDER_PIXELS + column * TEXT_CELL_WIDTH + sub_x * 4,
                TEXT_TOP_BORDER_LINES + row * TEXT_CELL_HEIGHT + sub_y * 6,
                4,
                6,
                fill,
            );
        }
    }
}

fn render_semigraphics6_cell(
    row: usize,
    column: usize,
    raw: u8,
    control: VdgControl,
    palette: VdgPalette,
    framebuffer: &mut [u32],
) {
    let colour_index = match (control.css, raw & 0x40 != 0) {
        (false, false) => 2,
        (false, true) => 3,
        (true, false) => 6,
        (true, true) => 7,
    };
    let colour = palette.colours[colour_index];
    for sub_y in 0..3 {
        for sub_x in 0..2 {
            let bit = semigraphics_bit(sub_y, sub_x, 3);
            let lit = raw & (1 << bit) != 0;
            let fill = if lit { colour } else { palette.black };
            fill_rect(
                framebuffer,
                TEXT_LEFT_BORDER_PIXELS + column * TEXT_CELL_WIDTH + sub_x * 4,
                TEXT_TOP_BORDER_LINES + row * TEXT_CELL_HEIGHT + sub_y * 4,
                4,
                4,
                fill,
            );
        }
    }
}

fn semigraphics_bit(sub_y: usize, sub_x: usize, rows: usize) -> usize {
    (rows - 1 - sub_y) * 2 + (1 - sub_x)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GraphicsSpec {
    row_bytes: usize,
    x_scale: usize,
    y_scale: usize,
    four_colour: bool,
}

impl GraphicsSpec {
    fn from_control(control: VdgControl) -> Self {
        match control.gm {
            0 => Self {
                row_bytes: 16,
                x_scale: 4,
                y_scale: 3,
                four_colour: true,
            },
            1 => Self {
                row_bytes: 16,
                x_scale: 2,
                y_scale: 3,
                four_colour: false,
            },
            2 => Self {
                row_bytes: 32,
                x_scale: 2,
                y_scale: 3,
                four_colour: true,
            },
            3 => Self {
                row_bytes: 16,
                x_scale: 2,
                y_scale: 2,
                four_colour: false,
            },
            4 => Self {
                row_bytes: 32,
                x_scale: 2,
                y_scale: 2,
                four_colour: true,
            },
            5 => Self {
                row_bytes: 16,
                x_scale: 2,
                y_scale: 1,
                four_colour: false,
            },
            6 => Self {
                row_bytes: 32,
                x_scale: 2,
                y_scale: 1,
                four_colour: true,
            },
            _ => Self {
                row_bytes: 32,
                x_scale: 1,
                y_scale: 1,
                four_colour: false,
            },
        }
    }
}

fn render_graphics_into(
    mut read_byte: impl FnMut(usize) -> u8,
    control: VdgControl,
    palette: VdgPalette,
    framebuffer: &mut [u32],
) {
    let spec = GraphicsSpec::from_control(control);
    let source_rows = TEXT_FRAMEBUFFER_HEIGHT / spec.y_scale;
    for source_y in 0..source_rows {
        for byte_x in 0..spec.row_bytes {
            let raw = read_byte(source_y * spec.row_bytes + byte_x);
            if spec.four_colour {
                render_colour_graphics_byte(
                    source_y,
                    byte_x,
                    raw,
                    spec,
                    control,
                    palette,
                    framebuffer,
                );
            } else {
                render_resolution_graphics_byte(
                    source_y,
                    byte_x,
                    raw,
                    spec,
                    control,
                    palette,
                    framebuffer,
                );
            }
        }
    }
}

fn render_colour_graphics_byte(
    source_y: usize,
    byte_x: usize,
    raw: u8,
    spec: GraphicsSpec,
    control: VdgControl,
    palette: VdgPalette,
    framebuffer: &mut [u32],
) {
    for pixel in 0..4 {
        let shift = 6 - pixel * 2;
        let colour_code = usize::from((raw >> shift) & 0x03);
        let colour_index = colour_code + if control.css { 4 } else { 0 };
        fill_rect(
            framebuffer,
            TEXT_LEFT_BORDER_PIXELS + (byte_x * 4 + pixel) * spec.x_scale,
            TEXT_TOP_BORDER_LINES + source_y * spec.y_scale,
            spec.x_scale,
            spec.y_scale,
            palette.colours[colour_index],
        );
    }
}

fn render_resolution_graphics_byte(
    source_y: usize,
    byte_x: usize,
    raw: u8,
    spec: GraphicsSpec,
    control: VdgControl,
    palette: VdgPalette,
    framebuffer: &mut [u32],
) {
    let foreground = if control.css {
        palette.colours[4]
    } else {
        palette.colours[0]
    };
    for bit in 0..8 {
        let lit = raw & (0x80 >> bit) != 0;
        let colour = if lit { foreground } else { palette.black };
        fill_rect(
            framebuffer,
            TEXT_LEFT_BORDER_PIXELS + (byte_x * 8 + bit) * spec.x_scale,
            TEXT_TOP_BORDER_LINES + source_y * spec.y_scale,
            spec.x_scale,
            spec.y_scale,
            colour,
        );
    }
}

fn fill_rect(
    framebuffer: &mut [u32],
    x_origin: usize,
    y_origin: usize,
    width: usize,
    height: usize,
    colour: u32,
) {
    for y in y_origin..y_origin + height {
        let row_start = y * TEXT_VISIBLE_FRAMEBUFFER_WIDTH + x_origin;
        framebuffer[row_start..row_start + width].fill(colour);
    }
}

// MC6847 internal 64-character font, 12 rows per glyph.
// Derived from XRoar's generated `font-6847.c` reference data.
const FONT_6847: [u8; 64 * TEXT_CELL_HEIGHT] = [
    0x00, 0x00, 0x00, 0x1c, 0x22, 0x02, 0x1a, 0x2a, 0x2a, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
    0x14, 0x22, 0x22, 0x3e, 0x22, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3c, 0x12, 0x12, 0x1c, 0x12,
    0x12, 0x3c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x22, 0x20, 0x20, 0x20, 0x22, 0x1c, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x3c, 0x12, 0x12, 0x12, 0x12, 0x12, 0x3c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3e,
    0x20, 0x20, 0x3c, 0x20, 0x20, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x20, 0x20, 0x3c, 0x20,
    0x20, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1e, 0x20, 0x20, 0x26, 0x22, 0x22, 0x1e, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x22, 0x22, 0x22, 0x3e, 0x22, 0x22, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c,
    0x08, 0x08, 0x08, 0x08, 0x08, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x02, 0x02, 0x02, 0x22,
    0x22, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22, 0x24, 0x28, 0x30, 0x28, 0x24, 0x22, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22,
    0x36, 0x2a, 0x2a, 0x22, 0x22, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22, 0x32, 0x2a, 0x26, 0x22,
    0x22, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x22, 0x22, 0x22, 0x22, 0x22, 0x3e, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x3c, 0x22, 0x22, 0x3c, 0x20, 0x20, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c,
    0x22, 0x22, 0x22, 0x2a, 0x24, 0x1a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3c, 0x22, 0x22, 0x3c, 0x28,
    0x24, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x22, 0x10, 0x08, 0x04, 0x22, 0x1c, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x3e, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22,
    0x22, 0x22, 0x22, 0x22, 0x22, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22, 0x22, 0x22, 0x14, 0x14,
    0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22, 0x22, 0x22, 0x2a, 0x2a, 0x36, 0x22, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x22, 0x22, 0x14, 0x08, 0x14, 0x22, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22,
    0x22, 0x14, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x02, 0x04, 0x08, 0x10,
    0x20, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x38, 0x20, 0x20, 0x20, 0x20, 0x20, 0x38, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x20, 0x20, 0x10, 0x08, 0x04, 0x02, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0e,
    0x02, 0x02, 0x02, 0x02, 0x02, 0x0e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x1c, 0x2a, 0x08, 0x08,
    0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x10, 0x3e, 0x10, 0x08, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
    0x08, 0x08, 0x08, 0x08, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x14, 0x14, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x14, 0x36, 0x00, 0x36, 0x14, 0x14, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x08, 0x1e, 0x20, 0x1c, 0x02, 0x3c, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x32,
    0x32, 0x04, 0x08, 0x10, 0x26, 0x26, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x28, 0x28, 0x10, 0x2a,
    0x24, 0x1a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x08, 0x10, 0x20, 0x20, 0x20, 0x10, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
    0x04, 0x02, 0x02, 0x02, 0x04, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x1c, 0x3e, 0x1c,
    0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x3e, 0x08, 0x08, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x30, 0x30, 0x10, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x30, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x02, 0x04, 0x08, 0x10, 0x20, 0x20, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x18, 0x24, 0x24, 0x24, 0x24, 0x24, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
    0x18, 0x08, 0x08, 0x08, 0x08, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x22, 0x02, 0x1c, 0x20,
    0x20, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x22, 0x02, 0x0c, 0x02, 0x22, 0x1c, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x04, 0x0c, 0x14, 0x3e, 0x04, 0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3e,
    0x20, 0x3c, 0x02, 0x02, 0x22, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x20, 0x20, 0x3c, 0x22,
    0x22, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x02, 0x04, 0x08, 0x10, 0x20, 0x20, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x1c, 0x22, 0x22, 0x1c, 0x22, 0x22, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c,
    0x22, 0x22, 0x1e, 0x02, 0x02, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x18,
    0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x08, 0x10, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x04, 0x08, 0x10, 0x20, 0x10, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x3e, 0x00, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x08, 0x04, 0x02, 0x04,
    0x08, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x24, 0x04, 0x08, 0x08, 0x00, 0x08, 0x00, 0x00,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_basic_alphanumeric_codes() {
        assert_eq!(decode_text_byte(0x20).ch, ' ');
        assert_eq!(decode_text_byte(0x01).ch, 'A');
        assert_eq!(decode_text_byte(0x1A).ch, 'Z');
        assert_eq!(decode_text_byte(0x30).ch, '0');
        assert_eq!(decode_text_byte(0x39).ch, '9');
    }

    #[test]
    fn preserves_inverse_flag_without_changing_diagnostic_character() {
        let cell = decode_text_byte(0x41);

        assert_eq!(cell.ch, 'A');
        assert!(cell.inverse);
    }

    #[test]
    fn captures_32_by_16_text_screen() {
        let screen = TextScreen::capture(|index| if index == 33 { 0x02 } else { 0x20 });

        assert_eq!(screen.cell(1, 1).expect("cell should exist").ch, 'B');
        assert_eq!(screen.cell(16, 0), None);
        assert_eq!(screen.to_plain_text().lines().count(), TEXT_ROWS);
        assert_eq!(
            screen.to_plain_text().lines().next().expect("line"),
            " ".repeat(32)
        );
    }

    #[test]
    fn renders_text_screen_to_argb_framebuffer() {
        let screen = TextScreen::capture(|index| if index == 0 { 0x01 } else { 0x20 });
        let framebuffer = screen.render_argb(TextPalette::default());

        assert_eq!(framebuffer.len(), TEXT_FRAMEBUFFER_PIXELS);
        assert_eq!(framebuffer[0], DEFAULT_TEXT_BACKGROUND);
        assert_eq!(
            framebuffer[3 * TEXT_FRAMEBUFFER_WIDTH + 4],
            DEFAULT_TEXT_FOREGROUND
        );
    }

    #[test]
    fn renders_inverse_text_by_swapping_foreground_and_background() {
        let screen = TextScreen::capture(|index| if index == 0 { 0x41 } else { 0x20 });
        let framebuffer = screen.render_argb(TextPalette::default());

        assert_eq!(framebuffer[0], DEFAULT_TEXT_FOREGROUND);
        assert_eq!(
            framebuffer[3 * TEXT_FRAMEBUFFER_WIDTH + 4],
            DEFAULT_TEXT_BACKGROUND
        );
    }

    #[test]
    fn renders_text_screen_with_visible_border() {
        let screen = TextScreen::capture(|index| if index == 0 { 0x01 } else { 0x20 });
        let framebuffer = screen.render_visible_argb(TextPalette::default());

        assert_eq!(framebuffer.len(), TEXT_VISIBLE_FRAMEBUFFER_PIXELS);
        assert_eq!(framebuffer[0], DEFAULT_TEXT_BORDER);
        assert_eq!(
            framebuffer
                [TEXT_TOP_BORDER_LINES * TEXT_VISIBLE_FRAMEBUFFER_WIDTH + TEXT_LEFT_BORDER_PIXELS],
            DEFAULT_TEXT_BACKGROUND
        );
        assert_eq!(
            framebuffer[(TEXT_TOP_BORDER_LINES + 3) * TEXT_VISIBLE_FRAMEBUFFER_WIDTH
                + TEXT_LEFT_BORDER_PIXELS
                + 4],
            DEFAULT_TEXT_FOREGROUND
        );
    }

    #[test]
    fn renders_semigraphics4_cells_from_alpha_memory() {
        let framebuffer = render_visible_argb(
            |index| if index == 0 { 0x94 } else { 0x20 },
            VdgControl::default(),
            VdgPalette::default(),
        );

        let first_cell =
            TEXT_TOP_BORDER_LINES * TEXT_VISIBLE_FRAMEBUFFER_WIDTH + TEXT_LEFT_BORDER_PIXELS;
        assert_eq!(framebuffer[first_cell], DEFAULT_TEXT_BORDER);
        assert_eq!(framebuffer[first_cell + 4], DEFAULT_VDG_YELLOW);
    }

    #[test]
    fn renders_rg6_resolution_graphics_pixels() {
        let framebuffer = render_visible_argb(
            |index| if index == 0 { 0x80 } else { 0x00 },
            VdgControl::from_dragon_pia1_port_b(0xF0),
            VdgPalette::default(),
        );

        let active_origin =
            TEXT_TOP_BORDER_LINES * TEXT_VISIBLE_FRAMEBUFFER_WIDTH + TEXT_LEFT_BORDER_PIXELS;
        assert_eq!(framebuffer[active_origin], DEFAULT_TEXT_FOREGROUND);
        assert_eq!(framebuffer[active_origin + 1], DEFAULT_TEXT_BORDER);
    }

    #[test]
    fn renders_cg6_colour_graphics_pixels() {
        let framebuffer = render_visible_argb(
            |index| if index == 0 { 0b01_10_11_00 } else { 0x00 },
            VdgControl::from_dragon_pia1_port_b(0xE0),
            VdgPalette::default(),
        );

        let active_origin =
            TEXT_TOP_BORDER_LINES * TEXT_VISIBLE_FRAMEBUFFER_WIDTH + TEXT_LEFT_BORDER_PIXELS;
        assert_eq!(framebuffer[active_origin], DEFAULT_VDG_YELLOW);
        assert_eq!(framebuffer[active_origin + 2], DEFAULT_VDG_BLUE);
        assert_eq!(framebuffer[active_origin + 4], DEFAULT_VDG_RED);
        assert_eq!(framebuffer[active_origin + 6], DEFAULT_TEXT_FOREGROUND);
    }
}
