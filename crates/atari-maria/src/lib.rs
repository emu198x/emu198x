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
//! - Bits 6:5: DM -- DMA mode (`10`/`11` = MARIA renders, `00`/`01` = blank)
//! - Bit 7: CK -- colour kill (force monochrome)
//! - Bit 4: CW -- character width for indirect mode (1 = 2 bytes, 0 = 1 byte;
//!   MAME `m_cwidth = BIT(ctrl, 4)`, "two data bytes per map byte" when set)
//! - Bit 3: BC -- border control
//! - Bit 2: Kangaroo mode (transparency off)
//! - Bits 1:0: RM -- read mode
//!
//! The donor reading had bit 7 = DMA / bit 6 = colour-kill / bit 1 = Kangaroo,
//! which is wrong on all three: a game enabling DMA (`DM=10`, bit 6) read as
//! "DMA off + colour-kill on", so MARIA never walked the display list, never
//! raised the DLI, and 7800 games hung waiting on the NMI counter (black screen).
//!
//! # Graphics modes
//!
//! - **160A**: 2 bits per pixel, 4 colours per sprite (palette selected per DL entry)
//! - **320A**: 1 bit per pixel, 2 colours per sprite (transparent + palette foreground)
//!
//! 160B/320B/C/D variants exist but are not yet implemented.

mod palette;

pub use palette::{NTSC_PALETTE, PAL_PALETTE};

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// Framebuffer width: 320 pixels (hires resolution).
/// Active display area dimensions (the pixels MARIA actually draws
/// through its DLL/DL pipeline).
pub const ACTIVE_WIDTH: u32 = 320;

/// Pixel clock of the NTSC part: twice the 3.579545 MHz colour clock, because
/// the hires modes put two pixels in each. Gives 6:7 pixels — taller than
/// they are wide, the Atari 8-bit's published ratio.
pub const NTSC_PIXEL_CLOCK_HZ: f64 = 7_159_090.0;

/// The same on PAL, from the 3.546894 MHz colour clock.
pub const PAL_PIXEL_CLOCK_HZ: f64 = 7_093_788.0;

/// Active display height in scan lines — MARIA's maximum, the same on both
/// regions. What differs is how much field is left around it.
pub const ACTIVE_HEIGHT: u32 = 240;

// ---------------------------------------------------------------------------
// Internal constants
// ---------------------------------------------------------------------------

/// NTSC: 263 total scanlines per frame.
const NTSC_LINES: u16 = 263;
/// PAL: 313 total scanlines per frame.
const PAL_LINES: u16 = 313;

/// First scanline MARIA attempts to display.
///
/// Not an approximation, though this said it was for a long time.
/// `reference/by-system/atari-7800/atari-7800-reference.md` §3 gives the
/// raster budget outright: 262 per frame, "MARIA attempts display" on rasters
/// **16-258**, and 41-232 is the 192-line band "visible on all televisions".
///
/// The window this anchors is 240 lines — a set's field — so it runs 16 to
/// 255 and clips the last three of the 243 MARIA attempts. Centring 240 lines
/// on the safe area's midpoint would give 16 to 256, so starting where MARIA
/// starts is within a line of that and needs no figure of its own.
const VISIBLE_TOP: u16 = 16;

/// CTRL bit masks (MARIA `$3C`), bit positions per the hardware: read mode
/// `RM` = bits 1:0, Kangaroo = bit 2, border control = bit 3, character width
/// `CW` = bit 4, DMA mode `DM` = bits 6:5, colour kill `CK` = bit 7. DMA is
/// active when `DM` is `10`/`11` — i.e. bit 6 is set.
const CTRL_DMA_ENABLED: u8 = 0x40;
const CTRL_COLOUR_KILL: u8 = 0x80;
const CTRL_CW: u8 = 0x10;
// Kangaroo mode (bit 2) is a transparency option, not yet implemented — see the
// module CTRL table. The 4-vs-5-byte DL header choice is per-entry, not CTRL.

/// Upper bound on the DMA cycles a single scanline can steal. A 7800 line is
/// 454 MARIA colour clocks; this caps the display-list walk so a malformed list
/// can't loop unbounded (real MARIA's DMA simply aborts at end of line).
const MAX_DMA_CYCLES_PER_LINE: u16 = 512;

// ---------------------------------------------------------------------------
// Region
// ---------------------------------------------------------------------------

/// NTSC or PAL region selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Scan lines a set displays, which is what the framebuffer holds.
    ///
    /// Per `knowledge/decisions/the-framebuffer-is-the-sets-window.md`. One
    /// height cannot serve both regions: 288 lines on NTSC is a fifth more
    /// raster than a set shows, which is what this used to emit.
    #[must_use]
    pub const fn framebuffer_height(self) -> u32 {
        match self {
            Self::Ntsc => 240,
            Self::Pal => 288,
        }
    }

    /// Scan lines of border above the active display — whatever the field has
    /// left over, halved. NTSC has nothing left over.
    #[must_use]
    pub const fn border_top(self) -> u32 {
        (self.framebuffer_height() - ACTIVE_HEIGHT) / 2
    }

    /// Pixels a set displays along a line, which is the framebuffer's width.
    ///
    /// `pixel_clock x active_line_seconds`: 7.15909 MHz over 52.148 µs is 373
    /// on NTSC, and 7.093788 MHz over 52.0 µs is 369 on PAL, rounded to leave
    /// a whole border either side of the active 320.
    ///
    /// This used to be a fixed 32 pixels of border either side, giving 384 for
    /// both regions — 103% of an NTSC window and 104% of a PAL one, which is
    /// raster a set hides.
    #[must_use]
    pub const fn framebuffer_width(self) -> u32 {
        ACTIVE_WIDTH + 2 * self.border_left()
    }

    /// Pixels of border left of the active area — what the line has left over.
    #[must_use]
    pub const fn border_left(self) -> u32 {
        match self {
            Self::Ntsc => 27,
            Self::Pal => 24,
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
    /// Zone height in scanlines (1-16).
    zone_height: u8,
    /// OFFSET (bits 0-3 of the header byte): the high-byte address offset for
    /// the zone's top line. It also sets the zone height (`offset + 1`) and
    /// decrements one per scanline down the zone.
    offset: u8,
    /// Holey-DMA mask (header bits 6:5 → `H16` in bit 1, `H8` in bit 0). When
    /// set, graphics reads from the matching address window return 0.
    holey: u8,
    /// Display List address for this zone.
    dl_addr: u16,
}

impl DllEntry {
    fn parse(b0: u8, b1: u8, b2: u8) -> Self {
        // The header byte is `DLI(7) H16(6) H8(5) - OFFSET(3:0)`. There is a
        // single 4-bit OFFSET field — it is both the per-line address offset
        // and (offset + 1) the zone height. MAME `maria.cpp`: `m_offset =
        // header & 0x0f`, `m_holey = (header & 0x60) >> 5`. (An earlier version
        // misread a 3-bit height from bits 4-6, which garbled multi-line zones,
        // and dropped holey DMA entirely, which left holes filled with garbage.)
        let offset = b0 & 0x0F;
        Self {
            dli: b0 & 0x80 != 0,
            zone_height: offset + 1,
            offset,
            holey: (b0 & 0x60) >> 5,
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
    /// Width in graphics bytes (1-32).
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
#[derive(Serialize, Deserialize)]
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
    zone_holey: u8,
    zone_dli: bool,
    /// `true` once the DLL has been loaded for the current frame.
    dll_active: bool,

    // -- DMA ----------------------------------------------------------------
    dma_cycles: u16,

    // -- Framebuffer --------------------------------------------------------
    framebuffer: Vec<u32>,
    #[serde(with = "BigArray")]
    line_buffer: [u8; ACTIVE_WIDTH as usize],
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
            zone_holey: 0,
            zone_dli: false,
            dll_active: false,

            dma_cycles: 0,

            framebuffer: vec![
                0xFF00_0000;
                (region.framebuffer_width() * region.framebuffer_height()) as usize
            ],
            line_buffer: [0; ACTIVE_WIDTH as usize],
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
            0x08 if self.vblank => 0x80,
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
        self.region.framebuffer_width()
    }

    /// Framebuffer height in pixels.
    ///
    /// Read back off the buffer rather than stated a second time, so the
    /// height a caller sees is always the height that was allocated.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        (self.framebuffer.len() / self.region.framebuffer_width() as usize) as u32
    }

    /// DMA cycles stolen during the last `render_line` call. A populated zone's
    /// display list can steal more than 255 cycles, so this is a `u16`.
    #[must_use]
    pub fn dma_cycles(&self) -> u16 {
        self.dma_cycles
    }

    /// Serialize MARIA register and internal state for save states.
    #[must_use]
    pub fn save_state(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(64);
        data.push(self.backgrnd);
        for pal in &self.palettes {
            data.extend_from_slice(pal);
        }
        data.push(self.ctrl);
        data.push(u8::from(self.wsync));
        data.push(self.dppl);
        data.push(self.dpph);
        data.push(self.chbase);
        data.extend_from_slice(&self.scan_line.to_le_bytes());
        data.push(u8::from(self.vblank));
        data.push(u8::from(self.dli_pending));
        data.push(u8::from(self.frame_complete));
        data.extend_from_slice(&self.dll_addr.to_le_bytes());
        data.push(self.zone_scanline);
        data.push(self.zone_height);
        data.extend_from_slice(&self.zone_dl_addr.to_le_bytes());
        data.push(self.zone_offset);
        data.push(self.zone_holey);
        data.push(u8::from(self.zone_dli));
        data.push(u8::from(self.dll_active));
        data.extend_from_slice(&self.dma_cycles.to_le_bytes());
        data
    }

    /// Restore MARIA state from a save state.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is too short.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        if data.len() < 40 {
            return Err("MARIA state truncated".into());
        }
        let mut p = 0;
        self.backgrnd = data[p];
        p += 1;
        for pal in &mut self.palettes {
            pal.copy_from_slice(&data[p..p + 3]);
            p += 3;
        }
        self.ctrl = data[p];
        p += 1;
        self.wsync = data[p] != 0;
        p += 1;
        self.dppl = data[p];
        p += 1;
        self.dpph = data[p];
        p += 1;
        self.chbase = data[p];
        p += 1;
        self.scan_line = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.vblank = data[p] != 0;
        p += 1;
        self.dli_pending = data[p] != 0;
        p += 1;
        self.frame_complete = data[p] != 0;
        p += 1;
        self.dll_addr = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.zone_scanline = data[p];
        p += 1;
        self.zone_height = data[p];
        p += 1;
        self.zone_dl_addr = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.zone_offset = data[p];
        p += 1;
        self.zone_holey = data[p];
        p += 1;
        self.zone_dli = data[p] != 0;
        p += 1;
        self.dll_active = data[p] != 0;
        p += 1;
        self.dma_cycles = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        Ok(p)
    }

    // -- Scanline rendering -------------------------------------------------

    /// Advance one scanline.  The caller provides a `read_byte` closure that
    /// can access any address in the 64 KB address space (RAM, ROM, etc.).
    ///
    /// Returns the number of DMA cycles stolen from the CPU for this line.
    pub fn render_line(&mut self, read_byte: &mut dyn FnMut(u16) -> u8) -> u16 {
        self.dma_cycles = 0;

        let visible_bottom = VISIBLE_TOP + ACTIVE_HEIGHT as u16;
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
        self.zone_holey = entry.holey;
        self.zone_dli = entry.dli;
    }

    /// Walk the display list for the current zone and render each entry.
    ///
    /// Each entry is a 4- or 5-byte header, chosen *per entry* by its second
    /// byte (`b1`): `b1 & 0x5F == 0` ends the line's list; otherwise
    /// `b1 & 0x1F != 0` is a 4-byte (direct) header and `== 0` is a 5-byte
    /// (extended) header. Byte roles per the MARIA spec (cross-checked against
    /// the MiSTer `DMA.sv`):
    ///
    /// - **4-byte:** `b0` = addr low, `b1` = `PPPWWWWW` (palette 7:5, width 4:0),
    ///   `b2` = addr high, `b3` = HPOS. Always direct, default write mode.
    /// - **5-byte:** `b0` = addr low, `b1` = `WM·1·IND·00000` (write-mode bit 7,
    ///   indirect bit 5), `b2` = addr high, `b3` = `PPPWWWWW`, `b4` = HPOS.
    ///
    /// Width is a 5-bit two's-complement byte count: `((!W) & 0x1F) + 1`, i.e.
    /// 1–32 (`W = 0` → 32).
    fn process_display_list(&mut self, read_byte: &mut dyn FnMut(u16) -> u8) {
        let mut dl_addr = self.zone_dl_addr;

        loop {
            // A scanline can't sustain more DMA than its colour-clock budget;
            // on real hardware MARIA's DMA aborts at the end of the line. Cap
            // the walk at that bound so a malformed display list (no end-of-list
            // marker) terminates instead of running away.
            if self.dma_cycles >= MAX_DMA_CYCLES_PER_LINE {
                break;
            }

            let b0 = read_byte(dl_addr);
            let b1 = read_byte(dl_addr.wrapping_add(1));
            self.dma_cycles += 2;

            // End of the line's display list.
            if b1 & 0x5F == 0 {
                break;
            }

            let b2 = read_byte(dl_addr.wrapping_add(2));
            let b3 = read_byte(dl_addr.wrapping_add(3));
            self.dma_cycles += 2;

            let (palette, width_field, hpos, indirect, write_mode_320, entry_size) =
                if b1 & 0x1F != 0 {
                    // 4-byte direct header.
                    ((b1 >> 5) & 0x07, b1 & 0x1F, b3, false, None, 4u16)
                } else {
                    // 5-byte extended header: width/palette move to b3, HPOS to b4.
                    let b4 = read_byte(dl_addr.wrapping_add(4));
                    self.dma_cycles += 1;
                    (
                        (b3 >> 5) & 0x07,
                        b3 & 0x1F,
                        b4,
                        b1 & 0x20 != 0,
                        Some(b1 & 0x80 != 0),
                        5u16,
                    )
                };

            let entry = DlEntry {
                gfx_addr: u16::from(b2) << 8 | u16::from(b0),
                palette,
                hpos: u16::from(hpos),
                width: ((!width_field) & 0x1F) + 1,
                indirect,
                write_mode_320,
            };

            self.render_dl_entry(&entry, read_byte);

            dl_addr = dl_addr.wrapping_add(entry_size);
        }
    }

    /// Holey DMA: when the zone's `H8`/`H16` bits are set, graphics reads from
    /// the matching high-address windows return 0 (a "hole") instead of memory.
    /// MAME `maria.cpp` `is_holey`: `H16` blanks `addr & 0x9000 == 0x9000`,
    /// `H8` blanks `addr & 0x8800 == 0x8800`.
    fn is_holey(&self, addr: u16) -> bool {
        (self.zone_holey & 0x02 != 0 && addr & 0x9000 == 0x9000)
            || (self.zone_holey & 0x01 != 0 && addr & 0x8800 == 0x8800)
    }

    /// Render a single DL entry into the line buffer.
    fn render_dl_entry(&mut self, entry: &DlEntry, read_byte: &mut dyn FnMut(u16) -> u8) {
        let scanline_in_zone = self.zone_scanline;

        // Calculate the graphics data address for this scanline. MARIA loads
        // the high-byte page offset with the DLL OFFSET at the zone's top line
        // and decrements it one per scanline (MAME `maria.cpp`:
        // `data_addr = graph_adr + x + (m_offset << 8)`, `m_offset` counting
        // down to 0 on the zone's last line).
        let page_offset = u16::from(self.zone_offset).wrapping_sub(u16::from(scanline_in_zone));

        // Determine which mode to use.
        let use_320 = entry.write_mode_320.unwrap_or(false); // Default to 160A when not in Kangaroo mode.

        if entry.indirect {
            self.render_indirect(entry, page_offset, use_320, read_byte);
        } else {
            self.render_direct(entry, page_offset, use_320, read_byte);
        }
    }

    /// Blit one graphics byte into the line buffer at column `*x`, advancing
    /// `*x` by 8 framebuffer columns. 320 mode is 1 bit/pixel; 160 mode is
    /// 2 bits/pixel with each pixel doubled to two columns. Pixel value 0 is
    /// transparent.
    fn blit_byte(&mut self, byte: u8, x: &mut usize, use_320: bool, palette: u8) {
        let pal = palette as usize;
        if use_320 {
            // 320A: 1 bit per pixel, 8 pixels per byte.
            for bit in (0..8).rev() {
                if *x < ACTIVE_WIDTH as usize && (byte >> bit) & 1 != 0 {
                    self.line_buffer[*x] = self.palettes[pal][0];
                }
                *x += 1;
            }
        } else {
            // 160A: 2 bits per pixel, 4 pixels per byte, each doubled.
            for shift in [6, 4, 2, 0] {
                let pixel = (byte >> shift) & 0x03;
                if pixel != 0 {
                    let colour = self.palettes[pal][(pixel - 1) as usize];
                    if *x < ACTIVE_WIDTH as usize {
                        self.line_buffer[*x] = colour;
                    }
                    if *x + 1 < ACTIVE_WIDTH as usize {
                        self.line_buffer[*x + 1] = colour;
                    }
                }
                *x += 2;
            }
        }
    }

    /// Direct mode: graphics bytes are read sequentially from the DL entry's
    /// address plus the zone's page offset (`gfx_addr + i + offset << 8`).
    fn render_direct(
        &mut self,
        entry: &DlEntry,
        page_offset: u16,
        use_320: bool,
        read_byte: &mut dyn FnMut(u16) -> u8,
    ) {
        let base = entry.gfx_addr.wrapping_add(page_offset << 8);
        let mut x = entry.hpos as usize;
        for i in 0..u16::from(entry.width) {
            let addr = base.wrapping_add(i);
            let byte = if self.is_holey(addr) {
                0
            } else {
                read_byte(addr)
            };
            self.dma_cycles += 1;
            self.blit_byte(byte, &mut x, use_320, entry.palette);
        }
    }

    /// Indirect (character/tile) mode: the DL entry points to a character map.
    /// Each map byte `c` selects a character whose graphics live at
    /// `(CHBASE << 8 | c) + (offset << 8)` (MAME `maria.cpp`:
    /// `data_addr = (m_charbase | c) + (m_offset << 8)`). The map is read at
    /// `gfx_addr` with *no* page offset. When the CWIDTH bit is set, each map
    /// byte yields two consecutive graphics bytes (wide characters).
    fn render_indirect(
        &mut self,
        entry: &DlEntry,
        page_offset: u16,
        use_320: bool,
        read_byte: &mut dyn FnMut(u16) -> u8,
    ) {
        let two_byte = self.ctrl & CTRL_CW != 0;
        let charbase = u16::from(self.chbase) << 8;
        let mut x = entry.hpos as usize;

        for i in 0..u16::from(entry.width) {
            let c = read_byte(entry.gfx_addr.wrapping_add(i));
            self.dma_cycles += 1;
            let data_addr = (charbase | u16::from(c)).wrapping_add(page_offset << 8);

            let b0 = if self.is_holey(data_addr) {
                0
            } else {
                read_byte(data_addr)
            };
            self.dma_cycles += 1;
            self.blit_byte(b0, &mut x, use_320, entry.palette);

            if two_byte {
                let a1 = data_addr.wrapping_add(1);
                let b1 = if self.is_holey(a1) { 0 } else { read_byte(a1) };
                self.dma_cycles += 1;
                self.blit_byte(b1, &mut x, use_320, entry.palette);
            }
        }
    }

    // -- Helpers ------------------------------------------------------------

    /// Fill the line buffer with the background colour index.
    fn fill_background(&mut self) {
        self.line_buffer.fill(self.backgrnd);
    }

    /// Fill the entire framebuffer with the current BACKGRND colour.
    /// Called by the machine at frame start so the canonical TV-visible
    /// border around the active 320 x 240 region carries the current
    /// MARIA background colour. Mid-frame BACKGRND changes affect the
    /// *next* frame's border (v1 simplification, matches GTIA).
    pub fn fill_border(&mut self) {
        let palette = match self.region {
            MariaRegion::Ntsc => &NTSC_PALETTE,
            MariaRegion::Pal => &PAL_PALETTE,
        };
        let index = (self.backgrnd >> 1) as usize;
        let argb = palette.get(index).copied().unwrap_or(0xFF00_0000);
        self.framebuffer.fill(argb);
    }

    /// Convert line buffer colour indices to ARGB32 and write to framebuffer.
    fn flush_line_to_framebuffer(&mut self) {
        let active_y = self.scan_line.saturating_sub(VISIBLE_TOP) as usize;
        if active_y >= ACTIVE_HEIGHT as usize {
            return;
        }
        let fb_y = self.region.border_top() as usize + active_y;

        let palette = match self.region {
            MariaRegion::Ntsc => &NTSC_PALETTE,
            MariaRegion::Pal => &PAL_PALETTE,
        };

        let kill = self.ctrl & CTRL_COLOUR_KILL != 0;
        let row_start =
            fb_y * self.region.framebuffer_width() as usize + self.region.border_left() as usize;

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
    #[test]
    fn each_region_holds_exactly_the_field_a_set_shows() {
        // 240 lines on NTSC, 288 on PAL — `Display::Television`'s
        // `lines_per_tv_height`, and the rule in
        // `the-framebuffer-is-the-sets-window.md`.
        //
        // This held one height for both. It was 288, which is right for PAL
        // and a fifth more raster than an NTSC set displays; the #1054 audit
        // read the 7800's NTSC profile as 120%.
        for (region, field) in [(MariaRegion::Ntsc, 240), (MariaRegion::Pal, 288)] {
            let maria = Maria::new(region);
            assert_eq!(maria.framebuffer_height(), field, "{region:?}");
            assert_eq!(
                maria.framebuffer().len(),
                (region.framebuffer_width() * field) as usize,
                "{region:?} allocated a buffer of the wrong size"
            );
        }
    }

    #[test]
    fn the_active_display_fits_the_field_with_the_border_around_it() {
        for region in [MariaRegion::Ntsc, MariaRegion::Pal] {
            assert_eq!(
                region.border_top() * 2 + ACTIVE_HEIGHT,
                region.framebuffer_height(),
                "{region:?} does not account for every line of its field"
            );
        }
        assert_eq!(MariaRegion::Ntsc.border_top(), 0);
        assert_eq!(MariaRegion::Pal.border_top(), 24);
    }

    use super::*;

    #[test]
    fn framebuffer_dimensions() {
        let maria = Maria::new(MariaRegion::Ntsc);
        assert_eq!(maria.framebuffer_width(), maria.region.framebuffer_width());
        assert_eq!(
            maria.framebuffer_height(),
            maria.region.framebuffer_height()
        );
        assert_eq!(
            maria.framebuffer().len(),
            (maria.region.framebuffer_width() * maria.region.framebuffer_height()) as usize
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
        // DLI=1, OFFSET=5 → zone height 6, DL addr=$1234. (The single 4-bit
        // OFFSET in bits 0-3 sets both the address offset and `offset + 1` rows.)
        let entry = DllEntry::parse(0b1011_0101, 0x12, 0x34);
        assert!(entry.dli);
        assert_eq!(entry.zone_height, 6);
        assert_eq!(entry.offset, 5);
        assert_eq!(entry.dl_addr, 0x1234);
    }

    #[test]
    fn dll_entry_min_max() {
        // Minimum: no DLI, OFFSET=0 → height 1.
        let min = DllEntry::parse(0x00, 0x00, 0x00);
        assert!(!min.dli);
        assert_eq!(min.zone_height, 1);
        assert_eq!(min.offset, 0);

        // Maximum: DLI, OFFSET=15 → height 16.
        let max = DllEntry::parse(0xFF, 0xFF, 0xFF);
        assert!(max.dli);
        assert_eq!(max.zone_height, 16);
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
        for (bit, pixel) in pixels.iter_mut().enumerate() {
            *pixel = (byte >> (7 - bit)) & 1;
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

        // Every pixel of the active region on the first active row should be
        // the background colour. (The border rows around the active area are
        // painted by the machine via fill_border() at frame start; they're
        // outside the scope of this chip-level test.)
        let bg_argb = NTSC_PALETTE[(0x0E >> 1) as usize];
        let row_start = maria.region.border_top() as usize
            * maria.region.framebuffer_width() as usize
            + maria.region.border_left() as usize;
        let row = &maria.framebuffer[row_start..row_start + ACTIVE_WIDTH as usize];
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
        // 4-byte DL entry: 1 byte of graphics at $0500, palette 0, hpos 0.
        //   b0 = addr low ($00); b1 = PPPWWWWW = palette 0 | width-field $1F
        //   (two's-complement count of 1 byte); b2 = addr high ($05);
        //   b3 = HPOS (0).
        mem[0x3000] = 0x00;
        mem[0x3001] = 0x1F;
        mem[0x3002] = 0x05;
        mem[0x3003] = 0x00;
        // End marker: next header's b1 (`$3005`) has `& 0x5F == 0`.
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

        // Active region starts at (maria.region.border_left(), maria.region.border_top()). First two
        // framebuffer pixels of the active row (one 160A pixel = 2 FB
        // pixels) should be the foreground colour.
        let active_start = maria.region.border_top() as usize
            * maria.region.framebuffer_width() as usize
            + maria.region.border_left() as usize;
        assert_eq!(maria.framebuffer[active_start], fg_argb);
        assert_eq!(maria.framebuffer[active_start + 1], fg_argb);
        // Next pixels should be background (transparent).
        assert_eq!(maria.framebuffer[active_start + 2], bg_argb);
        assert_eq!(maria.framebuffer[active_start + 3], bg_argb);
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
