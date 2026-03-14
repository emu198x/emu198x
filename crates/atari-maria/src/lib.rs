//! Atari 7800 MARIA display processor.
//!
//! MARIA is fundamentally different from ANTIC/GTIA: it uses a zone-based
//! display system where a Display List List (DLL) points to per-zone Display
//! Lists (DL), each describing sprites or tiles to render.
//!
//! # Standalone IC
//!
//! This crate has no dependencies.  Memory reads are provided by the caller
//! through a closure, keeping MARIA decoupled from any particular bus model.
//!
//! # Register map ($20-$3F)
//!
//! Registers are interleaved with palette colours:
//!
//! | Addr | Name     | Description                                    |
//! |------|----------|------------------------------------------------|
//! | $20  | BACKGRND | Background colour                              |
//! | $21  | P0C1     | Palette 0, colour 1                            |
//! | $22  | P0C2     | Palette 0, colour 2                            |
//! | $23  | P0C3     | Palette 0, colour 3                            |
//! | $24  | WSYNC    | Write halts CPU until end of scanline           |
//! | $25  | P1C1     | Palette 1, colour 1                            |
//! | $26  | P1C2     | Palette 1, colour 2                            |
//! | $27  | P1C3     | Palette 1, colour 3                            |
//! | $28  | MSTAT    | Read: bit 7 = VBLANK status                    |
//! | $29  | P2C1     | Palette 2, colour 1                            |
//! | $2A  | P2C2     | Palette 2, colour 2                            |
//! | $2B  | P2C3     | Palette 2, colour 3                            |
//! | $2C  | DPPH     | Display List List pointer high                 |
//! | $2D  | P3C1     | Palette 3, colour 1                            |
//! | $2E  | P3C2     | Palette 3, colour 2                            |
//! | $2F  | P3C3     | Palette 3, colour 3                            |
//! | $30  | DPPL     | Display List List pointer low                  |
//! | $31  | P4C1     | Palette 4, colour 1                            |
//! | $32  | P4C2     | Palette 4, colour 2                            |
//! | $33  | P4C3     | Palette 4, colour 3                            |
//! | $34  | CHBASE   | Character base address high byte               |
//! | $35  | P5C1     | Palette 5, colour 1                            |
//! | $36  | P5C2     | Palette 5, colour 2                            |
//! | $37  | P5C3     | Palette 5, colour 3                            |
//! | $38  | (unused) | Palette 6 slot / reserved                      |
//! | $39  | P6C1     | Palette 6, colour 1                            |
//! | $3A  | P6C2     | Palette 6, colour 2                            |
//! | $3B  | P6C3     | Palette 6, colour 3                            |
//! | $3C  | CTRL     | MARIA control register                         |
//! | $3D  | P7C1     | Palette 7, colour 1                            |
//! | $3E  | P7C2     | Palette 7, colour 2                            |
//! | $3F  | P7C3     | Palette 7, colour 3                            |
//!
//! # CTRL register ($3C)
//!
//! - Bit 7: DMA enabled (1 = MARIA renders, 0 = blank)
//! - Bit 6: Colour kill (force monochrome)
//! - Bit 4: CW -- character width for indirect mode (0 = 2 bytes, 1 = 1 byte)
//! - Bit 1: Kangaroo mode (5-byte DL headers)
//!
//! # Graphics modes
//!
//! - **160A**: 2 bits per pixel, 4 colours per sprite (palette selected per DL entry)
//! - **320A**: 1 bit per pixel, 2 colours per sprite (transparent + palette foreground)
//!
//! 160B/320B/C/D variants exist but are not yet implemented.

mod palette;

pub use palette::{NTSC_PALETTE, PAL_PALETTE};

/// Framebuffer width: 320 pixels (hires resolution).
pub const FB_WIDTH: u32 = 320;

/// Framebuffer height: 240 scanlines (covers NTSC visible area; PAL uses up to 240).
pub const FB_HEIGHT: u32 = 240;

// ---------------------------------------------------------------------------
// Internal constants
// ---------------------------------------------------------------------------

/// NTSC: 263 total scanlines per frame.
const NTSC_LINES: u16 = 263;
/// PAL: 313 total scanlines per frame.
const PAL_LINES: u16 = 313;

/// First visible scanline (approximate; games vary).
const VISIBLE_TOP: u16 = 16;

/// CTRL bit masks.
const CTRL_DMA_ENABLED: u8 = 0x80;
const CTRL_COLOUR_KILL: u8 = 0x40;
const CTRL_CW: u8 = 0x10;
const CTRL_KANGAROO: u8 = 0x02;

// ---------------------------------------------------------------------------
// Region
// ---------------------------------------------------------------------------

/// NTSC or PAL region selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MariaRegion {
    /// NTSC: 263 lines per frame, ~192 visible.
    Ntsc,
    /// PAL: 313 lines per frame, ~240 visible.
    Pal,
}

impl MariaRegion {
    /// Total scanlines per frame (including blanking).
    #[must_use]
    pub const fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => NTSC_LINES,
            Self::Pal => PAL_LINES,
        }
    }
}

// ---------------------------------------------------------------------------
// DLL entry (parsed)
// ---------------------------------------------------------------------------

/// A parsed Display List List entry (3 bytes).
#[derive(Debug, Clone, Copy, Default)]
struct DllEntry {
    /// Trigger NMI at end of zone.
    dli: bool,
    /// Zone height in scanlines (1-8).
    zone_height: u8,
    /// OFFSET added to graphics address high byte.
    offset: u8,
    /// Display List address for this zone.
    dl_addr: u16,
}

impl DllEntry {
    fn parse(b0: u8, b1: u8, b2: u8) -> Self {
        Self {
            dli: b0 & 0x80 != 0,
            zone_height: ((b0 >> 4) & 0x07) + 1,
            offset: b0 & 0x0F,
            dl_addr: u16::from(b1) << 8 | u16::from(b2),
        }
    }
}

// ---------------------------------------------------------------------------
// DL entry (parsed)
// ---------------------------------------------------------------------------

/// A parsed Display List entry (4 or 5 bytes).
#[derive(Debug, Clone, Copy)]
struct DlEntry {
    /// Base graphics data address.
    gfx_addr: u16,
    /// Palette number (0-7).
    palette: u8,
    /// Horizontal position (0-319).
    hpos: u16,
    /// Width in bytes (1-8).
    width: u8,
    /// Indirect (character/tile) mode.
    indirect: bool,
    /// Write mode from 5-byte header (None = use CTRL default).
    /// `true` = 320-pixel mode, `false` = 160-pixel mode.
    write_mode_320: Option<bool>,
}

// ---------------------------------------------------------------------------
// Maria
// ---------------------------------------------------------------------------

/// Atari 7800 MARIA display processor.
pub struct Maria {
    // -- Registers ----------------------------------------------------------
    backgrnd: u8,
    /// 8 palettes, each with 3 colours (index 0 is always transparent).
    palettes: [[u8; 3]; 8],
    ctrl: u8,
    wsync: bool,
    dppl: u8,
    dpph: u8,
    chbase: u8,

    // -- Timing / state -----------------------------------------------------
    region: MariaRegion,
    scan_line: u16,
    vblank: bool,
    dli_pending: bool,
    frame_complete: bool,

    // -- DLL processing state -----------------------------------------------
    dll_addr: u16,
    zone_scanline: u8,
    zone_height: u8,
    zone_dl_addr: u16,
    zone_offset: u8,
    zone_dli: bool,
    /// `true` once the DLL has been loaded for the current frame.
    dll_active: bool,

    // -- DMA ----------------------------------------------------------------
    dma_cycles: u8,

    // -- Framebuffer --------------------------------------------------------
    framebuffer: Vec<u32>,
    line_buffer: [u8; FB_WIDTH as usize],
}

impl Maria {
    /// Create a new MARIA in the given region.
    #[must_use]
    pub fn new(region: MariaRegion) -> Self {
        Self {
            backgrnd: 0,
            palettes: [[0; 3]; 8],
            ctrl: 0,
            wsync: false,
            dppl: 0,
            dpph: 0,
            chbase: 0,

            region,
            scan_line: 0,
            vblank: true,
            dli_pending: false,
            frame_complete: false,

            dll_addr: 0,
            zone_scanline: 0,
            zone_height: 1,
            zone_dl_addr: 0,
            zone_offset: 0,
            zone_dli: false,
            dll_active: false,

            dma_cycles: 0,

            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
            line_buffer: [0; FB_WIDTH as usize],
        }
    }

    // -- Register access ----------------------------------------------------

    /// Write a MARIA register.  `addr` is the offset from $20 (0x00-0x1F).
    pub fn write(&mut self, addr: u8, value: u8) {
        match addr {
            0x00 => self.backgrnd = value,
            0x04 => self.wsync = true,
            0x0C => self.dpph = value,
            0x10 => self.dppl = value,
            0x14 => self.chbase = value,
            0x1C => self.ctrl = value,
            // Palette colours: three colours per palette, interleaved around
            // control registers at every fourth address.
            _ => {
                if let Some((pal, col)) = Self::palette_index(addr) {
                    self.palettes[pal as usize][col as usize] = value;
                }
                // Writes to unused / read-only positions are ignored.
            }
        }
    }

    /// Read a MARIA register.  `addr` is the offset from $20 (0x00-0x1F).
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        match addr {
            0x08 => {
                if self.vblank {
                    0x80
                } else {
                    0x00
                }
            }
            _ => 0,
        }
    }

    /// Map a register offset to `(palette_number, colour_index)`.
    /// Returns `None` for non-palette addresses.
    const fn palette_index(addr: u8) -> Option<(u8, u8)> {
        // Palette colours live at offsets $01-$03, $05-$07, $09-$0B, $0D-$0F,
        // $11-$13, $15-$17, $19-$1B, $1D-$1F.
        // Pattern: palette = (addr >> 2), colour = (addr & 3) - 1,
        // but only when (addr & 3) != 0.
        let within = addr & 0x03;
        if within == 0 {
            return None;
        }
        let pal = addr >> 2;
        if pal > 7 {
            return None;
        }
        Some((pal, within - 1))
    }

    // -- Status queries -----------------------------------------------------

    /// Returns `true` when a Display List Interrupt is pending, and clears it.
    pub fn take_dli(&mut self) -> bool {
        let pending = self.dli_pending;
        self.dli_pending = false;
        pending
    }

    /// Returns `true` when WSYNC has been written (CPU should halt).
    #[must_use]
    pub fn wsync_halt(&self) -> bool {
        self.wsync
    }

    /// Clear the WSYNC halt at end of scanline.
    pub fn clear_wsync(&mut self) {
        self.wsync = false;
    }

    /// Returns `true` during vertical blank.
    #[must_use]
    pub fn vblank(&self) -> bool {
        self.vblank
    }

    /// Current scanline number.
    #[must_use]
    pub fn scan_line(&self) -> u16 {
        self.scan_line
    }

    /// Returns `true` once when a frame has been completed, then resets.
    pub fn take_frame_complete(&mut self) -> bool {
        let done = self.frame_complete;
        self.frame_complete = false;
        done
    }

    /// Reference to the ARGB32 framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width in pixels.
    #[must_use]
    pub const fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height in pixels.
    #[must_use]
    pub const fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    /// DMA cycles stolen during the last `render_line` call.
    #[must_use]
    pub fn dma_cycles(&self) -> u8 {
        self.dma_cycles
    }

    // -- Scanline rendering -------------------------------------------------

    /// Advance one scanline.  The caller provides a `read_byte` closure that
    /// can access any address in the 64 KB address space (RAM, ROM, etc.).
    ///
    /// Returns the number of DMA cycles stolen from the CPU for this line.
    pub fn render_line(&mut self, read_byte: &mut dyn FnMut(u16) -> u8) -> u8 {
        self.dma_cycles = 0;

        let visible_bottom = VISIBLE_TOP + FB_HEIGHT as u16;
        let lines = self.region.lines_per_frame();

        // Determine VBLANK status.
        self.vblank = self.scan_line < VISIBLE_TOP || self.scan_line >= visible_bottom;

        if !self.vblank && self.ctrl & CTRL_DMA_ENABLED != 0 {
            self.render_visible_line(read_byte);
        } else if !self.vblank {
            // DMA off: fill with background.
            self.fill_background();
            self.flush_line_to_framebuffer();
        }

        // Clear WSYNC at end of every scanline.
        self.wsync = false;

        // Advance scanline.
        self.scan_line += 1;
        if self.scan_line >= lines {
            self.scan_line = 0;
            self.frame_complete = true;
            self.dll_active = false;
        }

        self.dma_cycles
    }

    /// Render one visible scanline with DMA enabled.
    fn render_visible_line(&mut self, read_byte: &mut dyn FnMut(u16) -> u8) {
        // On the first visible line, load the DLL pointer.
        if !self.dll_active {
            self.dll_addr = u16::from(self.dpph) << 8 | u16::from(self.dppl);
            self.zone_scanline = 0;
            self.zone_height = 0; // Force immediate DLL fetch.
            self.dll_active = true;
        }

        // If we've finished the current zone, fetch the next DLL entry.
        if self.zone_scanline >= self.zone_height {
            self.fetch_dll_entry(read_byte);
            self.zone_scanline = 0;
        }

        // Fill line buffer with background.
        self.fill_background();

        // Process the display list for this zone.
        self.process_display_list(read_byte);

        // Write line buffer to framebuffer.
        self.flush_line_to_framebuffer();

        // Advance within zone.
        self.zone_scanline += 1;

        // Fire DLI at end of zone.
        if self.zone_scanline >= self.zone_height && self.zone_dli {
            self.dli_pending = true;
        }
    }

    /// Read a 3-byte DLL entry and advance `dll_addr`.
    fn fetch_dll_entry(&mut self, read_byte: &mut dyn FnMut(u16) -> u8) {
        let b0 = read_byte(self.dll_addr);
        let b1 = read_byte(self.dll_addr.wrapping_add(1));
        let b2 = read_byte(self.dll_addr.wrapping_add(2));
        self.dma_cycles += 3;
        self.dll_addr = self.dll_addr.wrapping_add(3);

        let entry = DllEntry::parse(b0, b1, b2);
        self.zone_height = entry.zone_height;
        self.zone_dl_addr = entry.dl_addr;
        self.zone_offset = entry.offset;
        self.zone_dli = entry.dli;
    }

    /// Walk the display list for the current zone and render each entry.
    fn process_display_list(&mut self, read_byte: &mut dyn FnMut(u16) -> u8) {
        let kangaroo = self.ctrl & CTRL_KANGAROO != 0;
        let entry_size: u16 = if kangaroo { 5 } else { 4 };
        let mut dl_addr = self.zone_dl_addr;

        loop {
            // Read the first two bytes to check for end-of-list.
            let b0 = read_byte(dl_addr);
            let b1 = read_byte(dl_addr.wrapping_add(1));
            self.dma_cycles += 2;

            // End-of-list: byte0 == 0 and (byte1 & 0x5F) == 0.
            if b0 == 0 && (b1 & 0x5F) == 0 {
                break;
            }

            let b2 = read_byte(dl_addr.wrapping_add(2));
            let b3 = read_byte(dl_addr.wrapping_add(3));
            self.dma_cycles += 2;

            let write_mode_320 = if kangaroo {
                let b4 = read_byte(dl_addr.wrapping_add(4));
                self.dma_cycles += 1;
                Some(b4 & 0x80 != 0)
            } else {
                None
            };

            let entry = DlEntry {
                gfx_addr: u16::from(b1 & 0x1F) << 8 | u16::from(b0),
                palette: (b1 >> 5) & 0x07,
                hpos: u16::from(b2),
                width: ((b3 >> 5) & 0x07) + 1,
                indirect: b3 & 0x10 != 0,
                write_mode_320,
            };

            self.render_dl_entry(&entry, read_byte);

            dl_addr = dl_addr.wrapping_add(entry_size);
        }
    }

    /// Render a single DL entry into the line buffer.
    fn render_dl_entry(
        &mut self,
        entry: &DlEntry,
        read_byte: &mut dyn FnMut(u16) -> u8,
    ) {
        let scanline_in_zone = self.zone_scanline;

        // Calculate the graphics data address for this scanline.
        // Each scanline's data lives on a different 256-byte page, offset by
        // the DLL OFFSET field plus the scanline index within the zone.
        let page_offset = u16::from(self.zone_offset).wrapping_add(u16::from(scanline_in_zone));
        let line_addr = entry.gfx_addr.wrapping_add(page_offset << 8);

        // Determine which mode to use.
        let use_320 = entry
            .write_mode_320
            .unwrap_or(false); // Default to 160A when not in Kangaroo mode.

        if entry.indirect {
            self.render_indirect(entry, line_addr, use_320, read_byte);
        } else {
            self.render_direct(entry, line_addr, use_320, read_byte);
        }
    }

    /// Direct mode: graphics bytes are read sequentially from `line_addr`.
    fn render_direct(
        &mut self,
        entry: &DlEntry,
        line_addr: u16,
        use_320: bool,
        read_byte: &mut dyn FnMut(u16) -> u8,
    ) {
        let mut x = entry.hpos as usize;

        for i in 0..u16::from(entry.width) {
            let byte = read_byte(line_addr.wrapping_add(i));
            self.dma_cycles += 1;

            if use_320 {
                // 320A: 1 bit per pixel, 8 pixels per byte.
                for bit in (0..8).rev() {
                    if x < FB_WIDTH as usize {
                        let pixel = (byte >> bit) & 1;
                        if pixel != 0 {
                            // Colour 1 from the selected palette.
                            self.line_buffer[x] = self.palettes[entry.palette as usize][0];
                        }
                    }
                    x += 1;
                }
            } else {
                // 160A: 2 bits per pixel, 4 pixels per byte.
                // Each pixel spans 2 framebuffer columns (320 / 160 = 2).
                for shift in [6, 4, 2, 0] {
                    let pixel = (byte >> shift) & 0x03;
                    if pixel != 0 {
                        let colour =
                            self.palettes[entry.palette as usize][(pixel - 1) as usize];
                        if x < FB_WIDTH as usize {
                            self.line_buffer[x] = colour;
                        }
                        if x + 1 < FB_WIDTH as usize {
                            self.line_buffer[x + 1] = colour;
                        }
                    }
                    x += 2;
                }
            }
        }
    }

    /// Indirect (character/tile) mode: the DL entry points to a character
    /// map.  Each character index is looked up via CHBASE.
    fn render_indirect(
        &mut self,
        entry: &DlEntry,
        line_addr: u16,
        use_320: bool,
        read_byte: &mut dyn FnMut(u16) -> u8,
    ) {
        let cw_single = self.ctrl & CTRL_CW != 0;
        let char_height: u16 = if cw_single { 1 } else { 2 };
        let scanline_in_zone = self.zone_scanline;
        let mut x = entry.hpos as usize;

        for i in 0..u16::from(entry.width) {
            let char_index = read_byte(line_addr.wrapping_add(i));
            self.dma_cycles += 1;

            // Character graphics address:
            //   (CHBASE << 8) + (char_index * char_height) + scanline_in_zone
            let char_addr = (u16::from(self.chbase) << 8)
                .wrapping_add(u16::from(char_index) * char_height)
                .wrapping_add(u16::from(scanline_in_zone));

            let byte = read_byte(char_addr);
            self.dma_cycles += 1;

            if use_320 {
                for bit in (0..8).rev() {
                    if x < FB_WIDTH as usize {
                        let pixel = (byte >> bit) & 1;
                        if pixel != 0 {
                            self.line_buffer[x] = self.palettes[entry.palette as usize][0];
                        }
                    }
                    x += 1;
                }
            } else {
                for shift in [6, 4, 2, 0] {
                    let pixel = (byte >> shift) & 0x03;
                    if pixel != 0 {
                        let colour =
                            self.palettes[entry.palette as usize][(pixel - 1) as usize];
                        if x < FB_WIDTH as usize {
                            self.line_buffer[x] = colour;
                        }
                        if x + 1 < FB_WIDTH as usize {
                            self.line_buffer[x + 1] = colour;
                        }
                    }
                    x += 2;
                }
            }
        }
    }

    // -- Helpers ------------------------------------------------------------

    /// Fill the line buffer with the background colour index.
    fn fill_background(&mut self) {
        self.line_buffer.fill(self.backgrnd);
    }

    /// Convert line buffer colour indices to ARGB32 and write to framebuffer.
    fn flush_line_to_framebuffer(&mut self) {
        let fb_y = self.scan_line.saturating_sub(VISIBLE_TOP) as usize;
        if fb_y >= FB_HEIGHT as usize {
            return;
        }

        let palette = match self.region {
            MariaRegion::Ntsc => &NTSC_PALETTE,
            MariaRegion::Pal => &PAL_PALETTE,
        };

        let kill = self.ctrl & CTRL_COLOUR_KILL != 0;
        let row_start = fb_y * FB_WIDTH as usize;

        for (i, &colour_reg) in self.line_buffer.iter().enumerate() {
            let index = if kill {
                // Colour kill: force luminance only (hue 0).
                (colour_reg & 0x0F) >> 1
            } else {
                colour_reg >> 1
            } as usize;

            let argb = palette.get(index).copied().unwrap_or(0xFF00_0000);
            self.framebuffer[row_start + i] = argb;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_dimensions() {
        let maria = Maria::new(MariaRegion::Ntsc);
        assert_eq!(maria.framebuffer_width(), FB_WIDTH);
        assert_eq!(maria.framebuffer_height(), FB_HEIGHT);
        assert_eq!(
            maria.framebuffer().len(),
            (FB_WIDTH * FB_HEIGHT) as usize
        );
    }

    #[test]
    fn register_backgrnd_write() {
        let mut maria = Maria::new(MariaRegion::Ntsc);
        maria.write(0x00, 0x2A);
        // Background is internal; verify through rendering that it sticks.
        // We can only observe it indirectly via the framebuffer after a line
        // render.  Here we just check no panic.
        assert_eq!(maria.read(0x00), 0); // BACKGRND is write-only at read side.
    }

    #[test]
    fn palette_write_read_back() {
        let mut maria = Maria::new(MariaRegion::Ntsc);
        // Write palette 0, colour 1 at offset $01.
        maria.write(0x01, 0x42);
        assert_eq!(maria.palettes[0][0], 0x42);
        // Palette 3, colour 2 at offset $0E.
        maria.write(0x0E, 0x88);
        assert_eq!(maria.palettes[3][1], 0x88);
        // Palette 7, colour 3 at offset $1F.
        maria.write(0x1F, 0xFE);
        assert_eq!(maria.palettes[7][2], 0xFE);
    }

    #[test]
    fn ctrl_register() {
        let mut maria = Maria::new(MariaRegion::Ntsc);
        maria.write(0x1C, 0x82);
        assert_eq!(maria.ctrl, 0x82);
    }

    #[test]
    fn wsync_flag() {
        let mut maria = Maria::new(MariaRegion::Ntsc);
        assert!(!maria.wsync_halt());
        maria.write(0x04, 0x00); // Any write sets WSYNC.
        assert!(maria.wsync_halt());
        maria.clear_wsync();
        assert!(!maria.wsync_halt());
    }

    #[test]
    fn mstat_vblank_bit() {
        let maria = Maria::new(MariaRegion::Ntsc);
        // Initially at scanline 0, which is in VBLANK.
        assert!(maria.vblank());
        assert_eq!(maria.read(0x08), 0x80);
    }

    #[test]
    fn dll_entry_parsing() {
        // DLI=1, height=4 (raw 3), offset=5, DL addr=$1234.
        let entry = DllEntry::parse(0b1011_0101, 0x12, 0x34);
        assert!(entry.dli);
        assert_eq!(entry.zone_height, 4);
        assert_eq!(entry.offset, 5);
        assert_eq!(entry.dl_addr, 0x1234);
    }

    #[test]
    fn dll_entry_min_max() {
        // Minimum: no DLI, height=1 (raw 0), offset=0.
        let min = DllEntry::parse(0x00, 0x00, 0x00);
        assert!(!min.dli);
        assert_eq!(min.zone_height, 1);
        assert_eq!(min.offset, 0);

        // Maximum: DLI, height=8 (raw 7), offset=15.
        let max = DllEntry::parse(0xFF, 0xFF, 0xFF);
        assert!(max.dli);
        assert_eq!(max.zone_height, 8);
        assert_eq!(max.offset, 15);
        assert_eq!(max.dl_addr, 0xFFFF);
    }

    #[test]
    fn dl_entry_parsing() {
        // Build a 4-byte DL entry in memory:
        // byte0=$80 (gfx low), byte1=$A5 (pal=5, addr_hi=$05),
        // byte2=$40 (hpos), byte3=$40 (width=3, no indirect).
        let b0: u8 = 0x80;
        let b1: u8 = 0xA5; // palette 5 (bits 7-5 = 101), addr bits 12-8 = 0x05
        let _b2: u8 = 0x40;
        let b3: u8 = 0x40; // width = (0x40 >> 5) + 1 = 3, indirect = 0

        let entry = DlEntry {
            gfx_addr: u16::from(b1 & 0x1F) << 8 | u16::from(b0),
            palette: (b1 >> 5) & 0x07,
            hpos: 0x40,
            width: ((b3 >> 5) & 0x07) + 1,
            indirect: b3 & 0x10 != 0,
            write_mode_320: None,
        };

        assert_eq!(entry.gfx_addr, 0x0580);
        assert_eq!(entry.palette, 5);
        assert_eq!(entry.hpos, 0x40);
        assert_eq!(entry.width, 3);
        assert!(!entry.indirect);
    }

    #[test]
    fn mode_160a_pixel_decode() {
        // 160A: 2 bits per pixel. Byte $E4 = 11 10 01 00 → pixels 3,2,1,0.
        let byte: u8 = 0xE4;
        let mut pixels = [0u8; 4];
        for (i, shift) in [6, 4, 2, 0].iter().enumerate() {
            pixels[i] = (byte >> shift) & 0x03;
        }
        assert_eq!(pixels, [3, 2, 1, 0]);
    }

    #[test]
    fn mode_320a_pixel_decode() {
        // 320A: 1 bit per pixel. Byte $A5 = 10100101 → 8 pixels.
        let byte: u8 = 0xA5;
        let mut pixels = [0u8; 8];
        for bit in 0..8 {
            pixels[bit] = (byte >> (7 - bit)) & 1;
        }
        assert_eq!(pixels, [1, 0, 1, 0, 0, 1, 0, 1]);
    }

    #[test]
    fn background_fills_line() {
        let mut maria = Maria::new(MariaRegion::Ntsc);
        maria.write(0x00, 0x0E); // Set background to grey luminance 7.

        // Enable DMA so rendering happens.
        maria.write(0x1C, CTRL_DMA_ENABLED);

        // Set up a DLL that points to an empty display list (immediate end marker).
        maria.dpph = 0x20;
        maria.dppl = 0x00;

        // Memory: DLL at $2000, then DL with end marker.
        let mut mem = vec![0u8; 0x10000];
        // DLL entry: height=1, offset=0, DL at $3000.
        mem[0x2000] = 0x00; // no DLI, height=1, offset=0
        mem[0x2001] = 0x30; // DL addr high
        mem[0x2002] = 0x00; // DL addr low
        // DL at $3000: end marker (byte0=0, byte1 & 0x5F = 0).
        mem[0x3000] = 0x00;
        mem[0x3001] = 0x00;

        // Advance past VBLANK to the first visible line.
        for _ in 0..VISIBLE_TOP {
            maria.render_line(&mut |addr| mem[addr as usize]);
        }

        // Render one visible line.
        maria.render_line(&mut |addr| mem[addr as usize]);

        // Every pixel on the line should be the background colour.
        let bg_argb = NTSC_PALETTE[(0x0E >> 1) as usize];
        let row = &maria.framebuffer[0..FB_WIDTH as usize];
        assert!(row.iter().all(|&px| px == bg_argb));
    }

    #[test]
    fn transparent_pixels_dont_overwrite() {
        // In 160A mode, pixel value 0 is transparent and must not overwrite
        // the background.
        let mut maria = Maria::new(MariaRegion::Ntsc);
        maria.write(0x00, 0x0E); // Background = $0E.
        maria.write(0x1C, CTRL_DMA_ENABLED);
        maria.palettes[0] = [0x22, 0x44, 0x66];

        maria.dpph = 0x20;
        maria.dppl = 0x00;

        let mut mem = vec![0u8; 0x10000];
        // DLL → zone at DL $3000, height 1.
        mem[0x2000] = 0x00;
        mem[0x2001] = 0x30;
        mem[0x2002] = 0x00;
        // DL entry: 1 byte of graphics at $4000, palette 0, hpos 0, width 1.
        mem[0x3000] = 0x00; // gfx addr low
        mem[0x3001] = 0x40; // palette 0 (000), addr high = $40 → but only low 5 bits → $00
        // Actually: gfx_addr = (byte1 & 0x1F) << 8 | byte0 = 0x00<<8 | 0x00 = $0000.
        // Let me fix: put gfx data address at $4000.
        mem[0x3000] = 0x00; // gfx addr low = $00
        mem[0x3001] = 0x40; // pal=010 (palette 2), addr hi bits = $00... Hmm.
        // byte1 bits 4-0 = addr bits 12-8. To get $4000 we need bits 12-8 = $40>>8=doesn't work.
        // $4000 = 0100_0000_0000_0000. bits 12-8 = 0_0000 = $00. So addr high = $40 only if
        // we use the OFFSET mechanism.
        // Simpler: put graphics at $0500. addr = $0500, byte1_low5 = $05, byte0 = $00.
        mem[0x3000] = 0x00; // gfx addr low = $00
        mem[0x3001] = 0x05; // palette 0 (000), addr hi = $05 → gfx at $0500
        mem[0x3002] = 0x00; // hpos = 0
        mem[0x3003] = 0x00; // width = 1, no indirect
        // End marker.
        mem[0x3004] = 0x00;
        mem[0x3005] = 0x00;

        // Graphics byte at $0500: $C0 = 11 00 00 00 → pixel 0 is colour 3,
        // pixels 1-3 are transparent.
        mem[0x0500] = 0xC0;

        for _ in 0..VISIBLE_TOP {
            maria.render_line(&mut |addr| mem[addr as usize]);
        }
        maria.render_line(&mut |addr| mem[addr as usize]);

        let bg_argb = NTSC_PALETTE[(0x0E >> 1) as usize];
        let fg_argb = NTSC_PALETTE[(0x66 >> 1) as usize]; // palette 0, colour 3

        // First two framebuffer pixels (one 160A pixel = 2 FB pixels) should
        // be the foreground colour.
        assert_eq!(maria.framebuffer[0], fg_argb);
        assert_eq!(maria.framebuffer[1], fg_argb);
        // Next pixels should be background (transparent).
        assert_eq!(maria.framebuffer[2], bg_argb);
        assert_eq!(maria.framebuffer[3], bg_argb);
    }

    #[test]
    fn frame_completion() {
        let mut maria = Maria::new(MariaRegion::Ntsc);
        let mem = [0u8; 0x10000];

        assert!(!maria.take_frame_complete());

        // Run through an entire frame.
        for _ in 0..NTSC_LINES {
            maria.render_line(&mut |addr| mem[addr as usize]);
        }

        assert!(maria.take_frame_complete());
        // Second call returns false (one-shot).
        assert!(!maria.take_frame_complete());
    }

    #[test]
    fn dli_pending_flag() {
        let mut maria = Maria::new(MariaRegion::Ntsc);
        maria.write(0x1C, CTRL_DMA_ENABLED);
        maria.dpph = 0x20;
        maria.dppl = 0x00;

        let mut mem = vec![0u8; 0x10000];
        // DLL entry with DLI=1, height=1.
        mem[0x2000] = 0x80; // DLI set, height=1, offset=0
        mem[0x2001] = 0x30;
        mem[0x2002] = 0x00;
        // Second DLL entry (needed so zone 1 works).
        mem[0x2003] = 0x00;
        mem[0x2004] = 0x30;
        mem[0x2005] = 0x10;
        // DL at $3000: end marker.
        mem[0x3000] = 0x00;
        mem[0x3001] = 0x00;
        // DL at $3010: end marker.
        mem[0x3010] = 0x00;
        mem[0x3011] = 0x00;

        // No DLI initially.
        assert!(!maria.take_dli());

        // Advance through VBLANK.
        for _ in 0..VISIBLE_TOP {
            maria.render_line(&mut |addr| mem[addr as usize]);
        }

        // Render the first visible line (zone with DLI).
        maria.render_line(&mut |addr| mem[addr as usize]);

        // DLI should have fired at end of zone (height=1, so after 1 line).
        assert!(maria.take_dli());
        // Second call clears it.
        assert!(!maria.take_dli());
    }
}
