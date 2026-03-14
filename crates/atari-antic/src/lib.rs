//! Atari ANTIC (Alpha-Numeric Television Interface Controller) display list
//! processor emulator.
//!
//! ANTIC reads a display list from RAM and generates video data for GTIA.
//! It handles DMA (stealing CPU cycles), character set lookup, bitmap data
//! fetch, player/missile DMA, scrolling, and display list interrupts.
//!
//! Used in the Atari 5200 and 8-bit computer line (400/800/XL/XE).

pub use atari_gtia::AnticMode;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Colour clocks per scan line.
pub const COLOUR_CLOCKS_PER_LINE: u16 = 228;

/// CPU cycles per scan line (`colour_clock` / 2).
pub const CPU_CYCLES_PER_LINE: u8 = 114;

/// Memory refresh DMA cycles stolen every line.
const REFRESH_DMA_CYCLES: u8 = 9;

/// First visible scan line (approximate).
const VISIBLE_START: u16 = 8;

/// Last visible scan line (exclusive).
const VISIBLE_END: u16 = 248;

// ---------------------------------------------------------------------------
// Region
// ---------------------------------------------------------------------------

/// ANTIC region (NTSC vs PAL), controlling total lines per frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnticRegion {
    /// NTSC: 262 lines per frame.
    Ntsc,
    /// PAL: 312 lines per frame.
    Pal,
}

impl AnticRegion {
    /// Total scan lines per frame.
    const fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 262,
            Self::Pal => 312,
        }
    }
}

// ---------------------------------------------------------------------------
// LineResult
// ---------------------------------------------------------------------------

/// Result of processing one scan line.
pub struct LineResult {
    /// ANTIC display mode for this line.
    pub mode: AnticMode,
    /// Pixel data as colour register indices.
    pub playfield: Vec<u8>,
    /// Playfield width in colour clocks.
    pub playfield_width: u16,
    /// CPU cycles stolen by DMA this line.
    pub dma_cycles: u8,
    /// Player graphics bytes (if PM DMA enabled).
    pub player_data: [u8; 4],
    /// Missile graphics byte (all 4 missiles, 2 bits each).
    pub missile_data: u8,
    /// Whether player/missile data was fetched this line.
    pub pm_dma: bool,
}

// ---------------------------------------------------------------------------
// Mode descriptors
// ---------------------------------------------------------------------------

/// Internal descriptor for an ANTIC display mode.
struct ModeDesc {
    /// Number of playfield bytes per line (at normal width).
    bytes_per_line: u8,
    /// Scan lines per mode-line row.
    scan_lines_per_row: u8,
    /// Whether this is a character mode (needs charset lookup).
    char_mode: bool,
    /// Bits per pixel (1, 2, or 4 for colour-clock grouping).
    bits_per_pixel: u8,
    /// Corresponding `AnticMode` for GTIA.
    antic_mode: AnticMode,
}

/// Look up a mode descriptor by the 4-bit mode field (2..=0xF).
/// Returns `None` for invalid/blank modes (0, 1).
const fn mode_desc(mode: u8) -> Option<ModeDesc> {
    match mode {
        0x02 => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 8,
            char_mode: true,
            bits_per_pixel: 1,
            antic_mode: AnticMode::Mode2,
        }),
        0x03 => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 10,
            char_mode: true,
            bits_per_pixel: 1,
            antic_mode: AnticMode::Mode3,
        }),
        0x04 => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 8,
            char_mode: true,
            bits_per_pixel: 1,
            antic_mode: AnticMode::Mode4,
        }),
        0x05 => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 16,
            char_mode: true,
            bits_per_pixel: 1,
            antic_mode: AnticMode::Mode5,
        }),
        0x06 => Some(ModeDesc {
            bytes_per_line: 20,
            scan_lines_per_row: 8,
            char_mode: true,
            bits_per_pixel: 2,
            antic_mode: AnticMode::Mode6,
        }),
        0x07 => Some(ModeDesc {
            bytes_per_line: 20,
            scan_lines_per_row: 16,
            char_mode: true,
            bits_per_pixel: 2,
            antic_mode: AnticMode::Mode7,
        }),
        0x08 => Some(ModeDesc {
            bytes_per_line: 10,
            scan_lines_per_row: 8,
            char_mode: false,
            bits_per_pixel: 2,
            antic_mode: AnticMode::Mode8,
        }),
        0x09 => Some(ModeDesc {
            bytes_per_line: 10,
            scan_lines_per_row: 4,
            char_mode: false,
            bits_per_pixel: 1,
            antic_mode: AnticMode::Mode9,
        }),
        0x0A => Some(ModeDesc {
            bytes_per_line: 20,
            scan_lines_per_row: 4,
            char_mode: false,
            bits_per_pixel: 2,
            antic_mode: AnticMode::ModeA,
        }),
        0x0B => Some(ModeDesc {
            bytes_per_line: 20,
            scan_lines_per_row: 2,
            char_mode: false,
            bits_per_pixel: 1,
            antic_mode: AnticMode::ModeB,
        }),
        0x0C => Some(ModeDesc {
            bytes_per_line: 20,
            scan_lines_per_row: 1,
            char_mode: false,
            bits_per_pixel: 1,
            antic_mode: AnticMode::ModeC,
        }),
        0x0D => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 2,
            char_mode: false,
            bits_per_pixel: 2,
            antic_mode: AnticMode::ModeD,
        }),
        0x0E => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 1,
            char_mode: false,
            bits_per_pixel: 2,
            antic_mode: AnticMode::ModeE,
        }),
        0x0F => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 1,
            char_mode: false,
            bits_per_pixel: 1,
            antic_mode: AnticMode::ModeF,
        }),
        _ => None,
    }
}

/// Adjust bytes per line for playfield width setting.
/// DMACTL bits 0-1: 00=off, 01=narrow(32B for 40B modes), 10=normal, 11=wide(48B for 40B modes).
fn adjust_bytes_for_width(base: u8, width_bits: u8) -> u8 {
    match width_bits {
        0 => 0, // playfield DMA disabled
        1 => {
            // Narrow: 3/4 of normal for 40-byte modes, 1/2 for 20-byte, etc.
            // Standard scaling: narrow = base * 4/5 (32 for 40, 16 for 20, 8 for 10)
            base * 4 / 5
        }
        3 => {
            // Wide: 6/5 of normal (48 for 40, 24 for 20, 12 for 10)
            base * 6 / 5
        }
        _ => base, // Normal (2)
    }
}

/// Playfield width in colour clocks for a given width setting and mode.
fn playfield_width_cc(width_bits: u8) -> u16 {
    match width_bits {
        0 => 0,
        1 => 128, // narrow
        3 => 192, // wide
        _ => 160, // normal
    }
}

// ---------------------------------------------------------------------------
// ANTIC chip
// ---------------------------------------------------------------------------

/// Atari ANTIC display list processor.
pub struct Antic {
    // -- Write registers --
    dmactl: u8,
    chactl: u8,
    dlist: u16,
    hscrol: u8,
    vscrol: u8,
    pmbase: u8,
    chbase: u8,
    wsync: bool,
    nmien: u8,
    nmist: u8,

    // -- Internal state --
    scan_line: u16,
    mode_line: u8,
    current_mode: u8,
    current_dli: bool,
    memory_scan: u16,
    scan_lines_per_row: u8,
    vscrol_enabled: bool,
    hscrol_enabled: bool,
    dl_active: bool,

    // -- NMI outputs --
    vbi_pending: bool,
    dli_pending: bool,

    // -- DMA --
    dma_cycles: u8,

    // -- Character code buffer (reused across scan lines within a mode line) --
    char_codes: Vec<u8>,

    // -- Frame state --
    region: AnticRegion,
    frame_complete: bool,
}

impl Antic {
    /// Create a new ANTIC in its power-on state.
    #[must_use]
    pub fn new(region: AnticRegion) -> Self {
        Self {
            dmactl: 0,
            chactl: 0,
            dlist: 0,
            hscrol: 0,
            vscrol: 0,
            pmbase: 0,
            chbase: 0,
            wsync: false,
            nmien: 0,
            nmist: 0,

            scan_line: 0,
            mode_line: 0,
            current_mode: 0,
            current_dli: false,
            memory_scan: 0,
            scan_lines_per_row: 0,
            vscrol_enabled: false,
            hscrol_enabled: false,
            dl_active: false,

            vbi_pending: false,
            dli_pending: false,

            dma_cycles: 0,

            char_codes: Vec::new(),

            region,
            frame_complete: false,
        }
    }

    // -----------------------------------------------------------------------
    // Register access
    // -----------------------------------------------------------------------

    /// Write an ANTIC register. `addr` is the offset within $D400-$D40F (0-15).
    pub fn write(&mut self, addr: u8, value: u8) {
        match addr & 0x0F {
            0x00 => self.dmactl = value,
            0x01 => self.chactl = value,
            0x02 => self.dlist = (self.dlist & 0xFF00) | u16::from(value),
            0x03 => self.dlist = (self.dlist & 0x00FF) | (u16::from(value) << 8),
            0x04 => self.hscrol = value & 0x0F,
            0x05 => self.vscrol = value & 0x0F,
            // 0x06 unused
            0x07 => self.pmbase = value,
            // 0x08 unused
            0x09 => self.chbase = value,
            0x0A => self.wsync = true,
            // 0x0B-0x0D are read-only
            0x0E => self.nmien = value,
            0x0F => self.nmist = 0, // NMIRES: write clears NMI status
            _ => {}
        }
    }

    /// Read an ANTIC register. `addr` is the offset within $D400-$D40F (0-15).
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        match addr & 0x0F {
            0x0B => self.vcount(),
            0x0C => 0, // PENH (not implemented)
            0x0D => 0, // PENV (not implemented)
            0x0F => self.nmist,
            _ => 0, // write-only or unused registers read as 0
        }
    }

    // -----------------------------------------------------------------------
    // Status queries
    // -----------------------------------------------------------------------

    /// Current scan line.
    #[must_use]
    pub fn scan_line(&self) -> u16 {
        self.scan_line
    }

    /// VCOUNT register value (`scan_line` / 2).
    #[must_use]
    pub fn vcount(&self) -> u8 {
        (self.scan_line / 2) as u8
    }

    /// Whether WSYNC is active (CPU should be halted).
    #[must_use]
    pub fn wsync_halt(&self) -> bool {
        self.wsync
    }

    /// Clear the WSYNC halt at the end of a scan line.
    pub fn clear_wsync(&mut self) {
        self.wsync = false;
    }

    /// Check and clear VBI pending flag.
    pub fn take_vbi(&mut self) -> bool {
        let pending = self.vbi_pending;
        self.vbi_pending = false;
        pending
    }

    /// Check and clear DLI pending flag.
    pub fn take_dli(&mut self) -> bool {
        let pending = self.dli_pending;
        self.dli_pending = false;
        pending
    }

    /// Whether the frame is complete (wrap occurred).
    #[must_use]
    pub fn frame_complete(&self) -> bool {
        self.frame_complete
    }

    /// Clear the frame-complete flag.
    pub fn clear_frame_complete(&mut self) {
        self.frame_complete = false;
    }

    // -----------------------------------------------------------------------
    // Line processing
    // -----------------------------------------------------------------------

    /// Process one scan line. Reads display list instructions and screen data
    /// from `ram`. Returns a `LineResult` describing the output.
    pub fn process_line(&mut self, ram: &[u8]) -> LineResult {
        self.dma_cycles = REFRESH_DMA_CYCLES;

        let lines_per_frame = self.region.lines_per_frame();
        let in_vblank = self.scan_line < VISIBLE_START || self.scan_line >= VISIBLE_END;

        // VBI at the start of vertical blank
        if self.scan_line == VISIBLE_END {
            if self.nmien & 0x80 != 0 {
                self.vbi_pending = true;
                self.nmist |= 0x40;
            }
            // Reset display list state for next frame
            self.mode_line = 0;
            self.current_mode = 0;
            self.scan_lines_per_row = 0;
            self.dl_active = false;
        }

        if in_vblank {
            let result = blank_result(self.dma_cycles);
            self.advance_scan_line(lines_per_frame);
            return result;
        }

        // Display list DMA disabled?
        let dl_dma = self.dmactl & 0x20 != 0;
        if !dl_dma {
            let result = blank_result(self.dma_cycles);
            self.advance_scan_line(lines_per_frame);
            return result;
        }

        let width_bits = self.dmactl & 0x03;

        // Start of a new mode line — fetch the next display list instruction
        if !self.dl_active || self.mode_line == 0 {
            self.fetch_dl_instruction(ram);
        }

        // Generate playfield data for this scan line
        let result = if self.current_mode == 0 {
            // Blank instruction
            blank_result(self.dma_cycles)
        } else if let Some(desc) = mode_desc(self.current_mode) {
            self.render_mode_line(ram, &desc, width_bits)
        } else {
            blank_result(self.dma_cycles)
        };

        // Advance mode_line within the current row
        self.mode_line += 1;
        if self.mode_line >= self.scan_lines_per_row {
            // End of this mode line — check for DLI
            if self.current_dli && (self.nmien & 0x40 != 0) {
                self.dli_pending = true;
                self.nmist |= 0x20;
            }
            self.mode_line = 0;
            self.dl_active = false;
        }

        self.advance_scan_line(lines_per_frame);
        result
    }

    /// Fetch and decode the next display list instruction.
    fn fetch_dl_instruction(&mut self, ram: &[u8]) {
        let instr = ram[self.dlist as usize & (ram.len() - 1)];
        self.dlist = self.dlist.wrapping_add(1);
        self.dma_cycles += 1; // DL fetch costs 1 cycle

        let mode = instr & 0x0F;
        let has_lms = instr & 0x80 != 0;
        let has_dli = instr & 0x40 != 0;
        let has_hscrol = instr & 0x10 != 0;
        let has_vscrol = instr & 0x20 != 0;

        self.current_dli = has_dli;
        self.hscrol_enabled = has_hscrol;
        self.vscrol_enabled = has_vscrol;

        match mode {
            0x00 => {
                // Blank line instruction: bits 6-4 = number of blank lines - 1
                let blank_count = ((instr >> 4) & 0x07) + 1;
                self.current_mode = 0;
                self.scan_lines_per_row = blank_count;
                self.mode_line = 0;
                self.dl_active = true;
            }
            0x01 => {
                // Jump instruction
                let lo = ram[self.dlist as usize & (ram.len() - 1)];
                self.dlist = self.dlist.wrapping_add(1);
                let hi = ram[self.dlist as usize & (ram.len() - 1)];
                self.dlist = self.dlist.wrapping_add(1);
                self.dma_cycles += 2;

                let target = u16::from(lo) | (u16::from(hi) << 8);
                self.dlist = target;

                if instr & 0x40 != 0 {
                    // JVB: jump and wait for vertical blank
                    self.current_mode = 0;
                    // Fill remaining visible lines with blank
                    let remaining = VISIBLE_END.saturating_sub(self.scan_line);
                    self.scan_lines_per_row = if remaining > 0 {
                        remaining as u8
                    } else {
                        1
                    };
                    self.mode_line = 0;
                    self.dl_active = true;
                } else {
                    // Plain jump — immediately fetch from new address
                    self.dl_active = false;
                    self.mode_line = 0;
                    // Re-fetch from the new address on this same call
                    self.fetch_dl_instruction(ram);
                }
            }
            0x02..=0x0F => {
                // Mode line
                self.current_mode = mode;

                if let Some(desc) = mode_desc(mode) {
                    self.scan_lines_per_row = desc.scan_lines_per_row;
                } else {
                    self.scan_lines_per_row = 1;
                }

                if has_lms {
                    let lo = ram[self.dlist as usize & (ram.len() - 1)];
                    self.dlist = self.dlist.wrapping_add(1);
                    let hi = ram[self.dlist as usize & (ram.len() - 1)];
                    self.dlist = self.dlist.wrapping_add(1);
                    self.memory_scan = u16::from(lo) | (u16::from(hi) << 8);
                    self.dma_cycles += 2;
                }

                self.mode_line = 0;
                self.dl_active = true;

                // For character modes, fetch character codes now (reused for
                // each scan line within this mode line row)
                if let Some(desc) = mode_desc(mode)
                    && desc.char_mode
                {
                    let width_bits = self.dmactl & 0x03;
                    let bytes = adjust_bytes_for_width(desc.bytes_per_line, width_bits);
                    self.char_codes.clear();
                    for i in 0..u16::from(bytes) {
                        let addr = self.memory_scan.wrapping_add(i) as usize & (ram.len() - 1);
                        self.char_codes.push(ram[addr]);
                    }
                    self.dma_cycles += bytes;
                    // Memory scan advances past character codes
                    self.memory_scan = self.memory_scan.wrapping_add(u16::from(bytes));
                }
            }
            _ => unreachable!(),
        }
    }

    /// Render pixel data for the current mode line.
    fn render_mode_line(&mut self, ram: &[u8], desc: &ModeDesc, width_bits: u8) -> LineResult {
        let bytes = adjust_bytes_for_width(desc.bytes_per_line, width_bits);
        let pf_width = playfield_width_cc(width_bits);

        // Player/missile DMA
        let (player_data, missile_data, pm_active) = self.fetch_pm_data(ram);

        let playfield = if desc.char_mode {
            self.render_char_line(ram, desc, bytes)
        } else {
            self.render_bitmap_line(ram, desc, bytes)
        };

        LineResult {
            mode: desc.antic_mode,
            playfield,
            playfield_width: pf_width,
            dma_cycles: self.dma_cycles,
            player_data,
            missile_data,
            pm_dma: pm_active,
        }
    }

    /// Render a character mode scan line.
    fn render_char_line(&mut self, ram: &[u8], desc: &ModeDesc, bytes: u8) -> Vec<u8> {
        let chbase_addr = u16::from(self.chbase) << 8;
        let char_height = desc.scan_lines_per_row;
        let row_in_char = self.mode_line;
        let inverse_mask = self.chactl & 0x01 != 0;
        let blank_inverted = self.chactl & 0x02 != 0;
        let reflect = self.chactl & 0x04 != 0;

        let count = usize::min(self.char_codes.len(), bytes as usize);

        // Each character produces pixels based on bits_per_pixel
        let mut pixels = Vec::new();

        // DMA for character bitmap fetch: 1 byte per character per scan line
        self.dma_cycles += bytes;

        for i in 0..count {
            let raw_code = self.char_codes[i];
            let char_index = raw_code & 0x7F;
            let inverse_bit = raw_code & 0x80 != 0;

            // Character row within the glyph
            let effective_row = if reflect {
                char_height.saturating_sub(1) - row_in_char
            } else {
                row_in_char
            };

            let glyph_addr = chbase_addr
                .wrapping_add(u16::from(char_index) * u16::from(char_height))
                .wrapping_add(u16::from(effective_row));
            let mut bitmap = ram[glyph_addr as usize & (ram.len() - 1)];

            // Handle inverse/blank for character bit 7
            if inverse_bit {
                if blank_inverted {
                    bitmap = 0; // blank the character
                } else if inverse_mask {
                    bitmap ^= 0xFF; // invert the bitmap
                }
            }

            // Decode bitmap into pixels
            if desc.bits_per_pixel == 1 {
                // 1 bit per pixel — 8 pixels per byte
                for bit in (0..8).rev() {
                    let px = (bitmap >> bit) & 1;
                    // 0 = background (register 0), 1 = foreground (register 1)
                    pixels.push(u8::from(px != 0));
                }
            } else {
                // 2 bits per pixel — 4 pixels per byte (modes 6, 7)
                // Shifts: 6, 4, 2, 0 (high pair is leftmost pixel)
                for pair in 0..4u8 {
                    let shift = 6 - pair * 2;
                    let px = (bitmap >> shift) & 0x03;
                    pixels.push(px);
                }
            }
        }

        pixels
    }

    /// Render a bitmap mode scan line.
    fn render_bitmap_line(&mut self, ram: &[u8], desc: &ModeDesc, bytes: u8) -> Vec<u8> {
        let mut pixels = Vec::new();

        // Fetch playfield data bytes
        for i in 0..u16::from(bytes) {
            let addr = self.memory_scan.wrapping_add(i) as usize & (ram.len() - 1);
            let data = ram[addr];

            if desc.bits_per_pixel == 1 {
                // 1 bit per pixel — 8 pixels per byte
                for bit in (0..8).rev() {
                    let px = (data >> bit) & 1;
                    pixels.push(u8::from(px != 0));
                }
            } else {
                // 2 bits per pixel — 4 pixels per byte
                // Shifts: 6, 4, 2, 0 (high pair is leftmost pixel)
                for pair in 0..4u8 {
                    let shift = 6 - pair * 2;
                    let px = (data >> shift) & 0x03;
                    pixels.push(px);
                }
            }
        }

        // DMA for playfield data fetch
        self.dma_cycles += bytes;

        // Memory scan advances only after all scan lines for this row complete
        if self.mode_line + 1 >= self.scan_lines_per_row {
            self.memory_scan = self.memory_scan.wrapping_add(u16::from(bytes));
        }

        pixels
    }

    /// Fetch player/missile DMA data if enabled.
    fn fetch_pm_data(&mut self, ram: &[u8]) -> ([u8; 4], u8, bool) {
        let player_dma = self.dmactl & 0x08 != 0;
        let missile_dma = self.dmactl & 0x04 != 0;
        let single_line = self.dmactl & 0x10 != 0;

        if !player_dma && !missile_dma {
            return ([0; 4], 0, false);
        }

        // PM base address alignment depends on resolution
        let pm_base = if single_line {
            // 2KB aligned for single-line resolution
            u16::from(self.pmbase & 0xF8) << 8
        } else {
            // 1KB aligned for double-line resolution
            u16::from(self.pmbase & 0xFC) << 8
        };

        let line = if single_line {
            self.scan_line
        } else {
            self.scan_line / 2
        };

        let mut player_data = [0u8; 4];
        let mut missile_data = 0u8;

        if missile_dma {
            // Missiles: base + $180 (single) or $C0 (double) + line
            let offset = if single_line { 0x0300 } else { 0x0180 };
            let addr = pm_base.wrapping_add(offset).wrapping_add(line) as usize;
            missile_data = ram[addr & (ram.len() - 1)];
            self.dma_cycles += 1;
        }

        if player_dma {
            // Players: base + $200/$300/$400/$500 (single) or
            //          $100/$180/$200/$280 (double) + line
            for p in 0..4u16 {
                let offset = if single_line {
                    0x0400 + p * 0x0100
                } else {
                    0x0200 + p * 0x0080
                };
                let addr = pm_base.wrapping_add(offset).wrapping_add(line) as usize;
                player_data[p as usize] = ram[addr & (ram.len() - 1)];
            }
            self.dma_cycles += 4;
        }

        // PM DMA overhead
        if player_dma || missile_dma {
            self.dma_cycles += 2;
        }

        (player_data, missile_data, true)
    }

    /// Advance scan line counter and handle frame wrap.
    fn advance_scan_line(&mut self, lines_per_frame: u16) {
        self.scan_line += 1;
        if self.scan_line >= lines_per_frame {
            self.scan_line = 0;
            self.frame_complete = true;
        }
    }
}

/// Create a blank `LineResult`.
fn blank_result(dma_cycles: u8) -> LineResult {
    LineResult {
        mode: AnticMode::Blank,
        playfield: Vec::new(),
        playfield_width: 0,
        dma_cycles,
        player_data: [0; 4],
        missile_data: 0,
        pm_dma: false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a minimal 64KB RAM array.
    fn make_ram() -> Vec<u8> {
        vec![0u8; 65536]
    }

    #[test]
    fn dmactl_write_read() {
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.write(0x00, 0x22); // DMACTL: normal width + DL DMA
        assert_eq!(antic.dmactl, 0x22);
    }

    #[test]
    fn display_list_pointer_update() {
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.write(0x02, 0x00); // DLISTL
        antic.write(0x03, 0x40); // DLISTH
        assert_eq!(antic.dlist, 0x4000);
    }

    #[test]
    fn vcount_reads_scan_line_divided_by_two() {
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.scan_line = 100;
        assert_eq!(antic.vcount(), 50);
        assert_eq!(antic.read(0x0B), 50);

        antic.scan_line = 261;
        assert_eq!(antic.vcount(), 130);
    }

    #[test]
    fn wsync_flag_set_and_clear() {
        let mut antic = Antic::new(AnticRegion::Ntsc);
        assert!(!antic.wsync_halt());

        antic.write(0x0A, 0x00); // Any write to WSYNC sets the flag
        assert!(antic.wsync_halt());

        antic.clear_wsync();
        assert!(!antic.wsync_halt());
    }

    #[test]
    fn nmi_enable_and_status() {
        let mut antic = Antic::new(AnticRegion::Ntsc);
        // Enable VBI and DLI
        antic.write(0x0E, 0xC0);
        assert_eq!(antic.nmien, 0xC0);

        // Simulate VBI pending
        antic.nmist = 0x40;
        assert_eq!(antic.read(0x0F), 0x40);

        // NMIRES clears status
        antic.write(0x0F, 0x00);
        assert_eq!(antic.read(0x0F), 0x00);
    }

    #[test]
    fn blank_line_instruction() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);

        // Set up display list at $4000: 3 blank lines ($20), then JVB to $4000
        ram[0x4000] = 0x20; // 3 blank lines (bits 6-4 = 010 → 2+1=3)
        ram[0x4001] = 0x41; // JVB
        ram[0x4002] = 0x00;
        ram[0x4003] = 0x40;

        antic.write(0x00, 0x22); // DMACTL: normal width + DL DMA
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);

        // Skip to visible area
        antic.scan_line = VISIBLE_START;
        antic.mode_line = 0;
        antic.dl_active = false;

        let result = antic.process_line(&ram);
        assert_eq!(result.mode, AnticMode::Blank);
        // Should set up 3 blank lines
        assert_eq!(antic.scan_lines_per_row, 3);
    }

    #[test]
    fn mode_d_line_processing() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);

        // Display list at $4000: Mode D with LMS pointing to $8000
        ram[0x4000] = 0x8D; // Mode D + LMS
        ram[0x4001] = 0x00; // LMS lo
        ram[0x4002] = 0x80; // LMS hi

        // Screen data at $8000: 40 bytes, first byte = $FF (4 pixels, all colour 3)
        ram[0x8000] = 0xFF;

        antic.write(0x00, 0x22); // DMACTL: normal width + DL DMA
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.scan_line = VISIBLE_START;

        let result = antic.process_line(&ram);
        assert_eq!(result.mode, AnticMode::ModeD);
        assert!(!result.playfield.is_empty());

        // First byte $FF → 4 pixels of value 3 (bits 11, 11, 11, 11)
        assert_eq!(result.playfield[0], 3);
        assert_eq!(result.playfield[1], 3);
        assert_eq!(result.playfield[2], 3);
        assert_eq!(result.playfield[3], 3);
    }

    #[test]
    fn mode_f_line_processing() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);

        // Display list: Mode F with LMS
        ram[0x4000] = 0x8F; // Mode F + LMS
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x80;

        // Screen data: first byte $A5 = 10100101
        ram[0x8000] = 0xA5;

        antic.write(0x00, 0x22);
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.scan_line = VISIBLE_START;

        let result = antic.process_line(&ram);
        assert_eq!(result.mode, AnticMode::ModeF);
        assert!(!result.playfield.is_empty());

        // $A5 = 1,0,1,0,0,1,0,1 → pixels: 1,0,1,0,0,1,0,1
        assert_eq!(result.playfield[0], 1);
        assert_eq!(result.playfield[1], 0);
        assert_eq!(result.playfield[2], 1);
        assert_eq!(result.playfield[3], 0);
        assert_eq!(result.playfield[4], 0);
        assert_eq!(result.playfield[5], 1);
        assert_eq!(result.playfield[6], 0);
        assert_eq!(result.playfield[7], 1);
    }

    #[test]
    fn mode_2_character_lookup() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);

        // Display list: Mode 2 with LMS
        ram[0x4000] = 0x82; // Mode 2 + LMS
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x80;

        // Screen data at $8000: character code 1
        ram[0x8000] = 0x01;

        // Character set at $E000 (chbase = $E0), char 1 row 0
        // Char 1 starts at $E000 + 1*8 = $E008
        ram[0xE008] = 0xFF; // all pixels set for row 0

        antic.write(0x00, 0x22);
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.write(0x09, 0xE0); // CHBASE
        antic.scan_line = VISIBLE_START;

        let result = antic.process_line(&ram);
        assert_eq!(result.mode, AnticMode::Mode2);
        assert!(!result.playfield.is_empty());

        // Character 1 with bitmap $FF → 8 pixels all set (value 1)
        assert_eq!(result.playfield[0], 1);
        assert_eq!(result.playfield[7], 1);

        // Character 0 (rest) with bitmap $00 → all clear
        assert_eq!(result.playfield[8], 0);
    }

    #[test]
    fn jump_jvb_resets_for_vblank() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);

        // Display list: JVB to $4000
        ram[0x4000] = 0x41; // JVB
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x40;

        antic.write(0x00, 0x22);
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.write(0x0E, 0xC0); // Enable VBI + DLI
        antic.scan_line = VISIBLE_START;

        let result = antic.process_line(&ram);
        assert_eq!(result.mode, AnticMode::Blank);
        // dlist should be reset to $4000
        assert_eq!(antic.dlist, 0x4000);
    }

    #[test]
    fn dma_cycle_counting() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);

        // Mode D + LMS: DL fetch(1) + LMS(2) + playfield(40) + refresh(9) = 52
        ram[0x4000] = 0x8D;
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x80;

        antic.write(0x00, 0x22); // normal width, DL DMA
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.scan_line = VISIBLE_START;

        let result = antic.process_line(&ram);
        // refresh(9) + DL(1) + LMS(2) + playfield(40) = 52
        assert_eq!(result.dma_cycles, 52);
    }

    #[test]
    fn frame_wraps_at_correct_line_count() {
        let mut antic_ntsc = Antic::new(AnticRegion::Ntsc);
        antic_ntsc.scan_line = 261;
        let ram = make_ram();

        antic_ntsc.process_line(&ram);
        assert!(antic_ntsc.frame_complete());
        assert_eq!(antic_ntsc.scan_line(), 0);

        let mut antic_pal = Antic::new(AnticRegion::Pal);
        antic_pal.scan_line = 311;

        antic_pal.process_line(&ram);
        assert!(antic_pal.frame_complete());
        assert_eq!(antic_pal.scan_line(), 0);
    }

    #[test]
    fn vblank_does_not_wrap_early() {
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.scan_line = 250;
        let ram = make_ram();

        antic.process_line(&ram);
        assert!(!antic.frame_complete());
        assert_eq!(antic.scan_line(), 251);
    }

    #[test]
    fn pm_dma_cycle_counting() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);

        // Mode D + LMS + player DMA + missile DMA
        ram[0x4000] = 0x8D;
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x80;

        antic.write(0x00, 0x2E); // normal width + DL DMA + player + missile
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.scan_line = VISIBLE_START;

        let result = antic.process_line(&ram);
        assert!(result.pm_dma);
        // refresh(9) + DL(1) + LMS(2) + playfield(40) + missile(1) + players(4) + overhead(2) = 59
        assert_eq!(result.dma_cycles, 59);
    }
}
