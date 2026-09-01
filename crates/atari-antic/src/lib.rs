//! Atari ANTIC (Alpha-Numeric Television Interface Controller) display list
//! processor emulator.
//!
//! Adapted from `Emu198x-Oldest/crates/atari-antic` (port 2026-06-01) for
//! the Atari 5200 / 800XL / 130XE / XEGS family. Self-contained, no
//! external chip dependencies.
//!
//! ANTIC reads a display list from RAM and generates video data for GTIA.
//! It handles DMA (stealing CPU cycles), character set lookup, bitmap data
//! fetch, player/missile DMA, scrolling, and display list interrupts.
//!
//! Used in the Atari 5200 and 8-bit computer line (400/800/XL/XE).

pub use atari_gtia::AnticMode;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Colour clocks per scan line.
pub const COLOUR_CLOCKS_PER_LINE: u16 = 228;

/// CPU cycles per scan line (`colour_clock` / 2).
pub const CPU_CYCLES_PER_LINE: u8 = 114;

/// CPU cycle at which ANTIC's display-fetch DMA begins (MAME `CYCLES_HSTART`).
/// The CPU runs at full speed before this; the DMA steal lands in the fetch
/// window that follows.
pub const CYCLES_HSTART: u16 = 32;

/// CPU cycle of HSYNC — where the visible region ends and ANTIC releases a
/// CPU held by WSYNC (MAME `CYCLES_HSYNC`). A `STA WSYNC` halts the CPU until
/// the beam reaches this cycle, not until the next scan line.
pub const CYCLES_HSYNC: u16 = 104;

/// Memory refresh DMA cycles stolen every line.
const REFRESH_DMA_CYCLES: u8 = 9;

/// Whether the CPU is stalled by ANTIC DMA on cycle `line_cycle` (1-based,
/// within the 114-cycle line) given the line's total `dma_budget`.
///
/// Real ANTIC steals its DMA cycles spread through the display-fetch window
/// `[CYCLES_HSTART, CYCLES_HSYNC)` rather than in one block at the line start.
/// This distributes `dma_budget` stalls evenly across that 72-cycle window
/// (a Bresenham split), so a mid-line CPU write lands at roughly the right
/// beam position instead of being shoved late by a front-loaded block. When
/// the budget exceeds the window (heavy character modes), the overflow spills
/// into the cycles immediately before `CYCLES_HSTART`. Exactly `dma_budget`
/// cycles are stolen either way, preserving the per-line cycle count — up to
/// `CYCLES_HSYNC`, which is all a line has. Above that the CPU gets nothing
/// until the line ends, and the excess is simply lost.
///
/// This is an approximation: the *exact* per-character fetch positions are
/// mode-dependent and not modelled (a relaxation MAME shares — it block-steals
/// `m_steal_cycles`). It captures the window, not the fine structure.
#[must_use]
pub fn cpu_dma_stalled(line_cycle: u16, dma_budget: u16) -> bool {
    if dma_budget == 0 {
        return false;
    }
    let window = CYCLES_HSYNC - CYCLES_HSTART; // 72 fetch cycles
    if dma_budget >= window {
        // Steal the whole window plus the overflow just before it. A line has
        // only `CYCLES_HSYNC` cycles to give, so a budget past that takes
        // every one of them and no more — the first scan line of a wide mode 2
        // line asks for 108.
        let overflow = dma_budget - window;
        let start = CYCLES_HSTART.saturating_sub(overflow);
        return line_cycle > start && line_cycle <= CYCLES_HSYNC;
    }
    // Even spread across (CYCLES_HSTART, CYCLES_HSYNC]: steal on cycle c when
    // the running count floor(pos·budget/window) advances.
    if line_cycle <= CYCLES_HSTART || line_cycle > CYCLES_HSYNC {
        return false;
    }
    let pos = u32::from(line_cycle - CYCLES_HSTART); // 1..=window
    let budget = u32::from(dma_budget);
    let win = u32::from(window);
    (pos * budget) / win > ((pos - 1) * budget) / win
}

/// First visible scan line (approximate).
const VISIBLE_START: u16 = 8;

/// Last visible scan line (exclusive).
const VISIBLE_END: u16 = 248;

// ---------------------------------------------------------------------------
// Region
// ---------------------------------------------------------------------------

/// ANTIC region (NTSC vs PAL), controlling total lines per frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// One-line P/M resolution (DMACTL bit 4). GTIA needs it because VDELAY
    /// only shifts an object in the two-line display.
    pub pm_single_line: bool,
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
    /// Colour clocks one pixel of this mode covers. ANTIC repeats each pixel
    /// across them so `LineResult::playfield` is always one entry per colour
    /// clock — the contract GTIA reads it on. Cannot be derived from
    /// `bits_per_pixel`: modes 4 and 5 carry 1 there yet emit four pixels per
    /// byte, modes 6 and 7 carry 2 yet emit eight.
    cc_per_pixel: u8,
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
            cc_per_pixel: 1,
            antic_mode: AnticMode::Mode2,
        }),
        0x03 => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 10,
            char_mode: true,
            bits_per_pixel: 1,
            cc_per_pixel: 1,
            antic_mode: AnticMode::Mode3,
        }),
        0x04 => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 8,
            char_mode: true,
            bits_per_pixel: 1,
            cc_per_pixel: 1,
            antic_mode: AnticMode::Mode4,
        }),
        0x05 => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 16,
            char_mode: true,
            bits_per_pixel: 1,
            cc_per_pixel: 1,
            antic_mode: AnticMode::Mode5,
        }),
        0x06 => Some(ModeDesc {
            bytes_per_line: 20,
            scan_lines_per_row: 8,
            char_mode: true,
            bits_per_pixel: 2,
            cc_per_pixel: 1,
            antic_mode: AnticMode::Mode6,
        }),
        0x07 => Some(ModeDesc {
            bytes_per_line: 20,
            scan_lines_per_row: 16,
            char_mode: true,
            bits_per_pixel: 2,
            cc_per_pixel: 1,
            antic_mode: AnticMode::Mode7,
        }),
        0x08 => Some(ModeDesc {
            bytes_per_line: 10,
            scan_lines_per_row: 8,
            char_mode: false,
            bits_per_pixel: 2,
            cc_per_pixel: 4,
            antic_mode: AnticMode::Mode8,
        }),
        0x09 => Some(ModeDesc {
            bytes_per_line: 10,
            scan_lines_per_row: 4,
            char_mode: false,
            bits_per_pixel: 1,
            cc_per_pixel: 2,
            antic_mode: AnticMode::Mode9,
        }),
        0x0A => Some(ModeDesc {
            bytes_per_line: 20,
            scan_lines_per_row: 4,
            char_mode: false,
            bits_per_pixel: 2,
            cc_per_pixel: 2,
            antic_mode: AnticMode::ModeA,
        }),
        0x0B => Some(ModeDesc {
            bytes_per_line: 20,
            scan_lines_per_row: 2,
            char_mode: false,
            bits_per_pixel: 1,
            cc_per_pixel: 1,
            antic_mode: AnticMode::ModeB,
        }),
        0x0C => Some(ModeDesc {
            bytes_per_line: 20,
            scan_lines_per_row: 1,
            char_mode: false,
            bits_per_pixel: 1,
            cc_per_pixel: 1,
            antic_mode: AnticMode::ModeC,
        }),
        0x0D => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 2,
            char_mode: false,
            bits_per_pixel: 2,
            cc_per_pixel: 1,
            antic_mode: AnticMode::ModeD,
        }),
        0x0E => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 1,
            char_mode: false,
            bits_per_pixel: 2,
            cc_per_pixel: 1,
            antic_mode: AnticMode::ModeE,
        }),
        0x0F => Some(ModeDesc {
            bytes_per_line: 40,
            scan_lines_per_row: 1,
            char_mode: false,
            bits_per_pixel: 1,
            cc_per_pixel: 1,
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
#[derive(Serialize, Deserialize)]
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

    /// Current DMACTL value (write-only register; this is a debug
    /// accessor for tests and MCP-style chip inspection).
    #[must_use]
    pub const fn dmactl_value(&self) -> u8 {
        self.dmactl
    }
    /// Current NMIEN value (write-only register; debug accessor).
    #[must_use]
    pub const fn nmien_value(&self) -> u8 {
        self.nmien
    }
    /// Current display-list pointer (DLISTL/DLISTH); debug accessor.
    #[must_use]
    pub const fn dlist_value(&self) -> u16 {
        self.dlist
    }
    /// Current character-set base (CHBASE); debug accessor.
    #[must_use]
    pub const fn chbase_value(&self) -> u8 {
        self.chbase
    }
    /// Current CHACTL (character control: inverse/blank/reflect); debug accessor.
    #[must_use]
    pub const fn chactl_value(&self) -> u8 {
        self.chactl
    }
    /// Current HSCROL (horizontal fine scroll); debug accessor.
    #[must_use]
    pub const fn hscrol_value(&self) -> u8 {
        self.hscrol
    }
    /// Current VSCROL (vertical fine scroll); debug accessor.
    #[must_use]
    pub const fn vscrol_value(&self) -> u8 {
        self.vscrol
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

        // VBI at the start of vertical blank.
        // NMIEN bit 6 = VBI enable (bit 7 is DLI). NMIST bit 6 records VBI.
        if self.scan_line == VISIBLE_END {
            self.nmist |= 0x40;
            if self.nmien & 0x40 != 0 {
                self.vbi_pending = true;
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
            // End of this mode line — check for DLI.
            // NMIEN bit 7 = DLI enable. NMIST bit 7 records DLI.
            if self.current_dli {
                self.nmist |= 0x80;
                if self.nmien & 0x80 != 0 {
                    self.dli_pending = true;
                }
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

        // Display-list instruction option bits (matches ANTIC hardware):
        //   bit 7 = DLI (display-list interrupt)
        //   bit 6 = LMS (load memory scan — two-byte operand follows)
        //   bit 5 = VSCROL, bit 4 = HSCROL
        let mode = instr & 0x0F;
        let has_dli = instr & 0x80 != 0;
        let has_lms = instr & 0x40 != 0;
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
                    self.scan_lines_per_row = if remaining > 0 { remaining as u8 } else { 1 };
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

        let mut playfield = if desc.char_mode {
            self.render_char_line(ram, desc, bytes)
        } else {
            self.render_bitmap_line(ram, desc, bytes)
        };

        // Modes 8, 9 and A draw pixels wider than a colour clock. Repeat each
        // one across the clocks it covers so the buffer handed to GTIA is one
        // entry per colour clock whatever the mode.
        if desc.cc_per_pixel > 1 {
            playfield = playfield
                .iter()
                .flat_map(|&px| std::iter::repeat_n(px, usize::from(desc.cc_per_pixel)))
                .collect();
        }

        LineResult {
            mode: desc.antic_mode,
            playfield,
            playfield_width: pf_width,
            dma_cycles: self.dma_cycles,
            player_data,
            missile_data,
            pm_dma: pm_active,
            pm_single_line: self.dmactl & 0x10 != 0,
        }
    }

    /// Render a character mode scan line.
    fn render_char_line(&mut self, ram: &[u8], desc: &ModeDesc, bytes: u8) -> Vec<u8> {
        let chbase_addr = u16::from(self.chbase) << 8;
        // CHACTL: bit 1 = inverse-video enable, bit 0 = blank, bit 2 = reflect.
        let inverse_video = self.chactl & 0x02 != 0;
        let blank = self.chactl & 0x01 != 0;
        let reflect = self.chactl & 0x04 != 0;

        // Every ANTIC text mode uses an 8-byte-per-glyph font. The
        // double-height modes (5, 7) show each font line on two scan lines,
        // so the font row is the mode-line row halved.
        let double_height = matches!(desc.antic_mode, AnticMode::Mode5 | AnticMode::Mode7);
        let raw_row = if double_height {
            self.mode_line / 2
        } else {
            self.mode_line
        };
        let count = usize::min(self.char_codes.len(), bytes as usize);
        let mut pixels = Vec::new();

        // DMA for character bitmap fetch: 1 byte per character per scan line
        self.dma_cycles += bytes;

        let glyph_byte = |glyph: u16, font_row: u8| -> u8 {
            let addr = chbase_addr
                .wrapping_add(glyph.wrapping_mul(8))
                .wrapping_add(u16::from(font_row));
            ram[addr as usize & (ram.len() - 1)]
        };

        for i in 0..count {
            let raw_code = self.char_codes[i];
            // Mode 3's ten-line row still addresses an eight-byte glyph. The
            // hardware uses the low three row-counter bits, blanks rows 8-9
            // for ordinary characters, and blanks rows 0-1 for the $60-$7F
            // descender range. Those characters therefore expose glyph rows
            // 0-1 at display rows 8-9 without reading into the next glyph.
            let font_row = if desc.antic_mode == AnticMode::Mode3 {
                let descender = raw_code & 0x60 == 0x60;
                if (!descender && raw_row >= 8) || (descender && raw_row < 2) {
                    None
                } else {
                    Some(raw_row & 0x07)
                }
            } else {
                Some(raw_row)
            }
            .map(|row| if reflect { 7 - row } else { row });

            match desc.antic_mode {
                // 5-colour text (modes 6, 7): the low 6 bits are the glyph and
                // the top 2 bits select the playfield colour register. The
                // 8-pixel font is 1 bit per pixel — a set pixel takes the
                // chosen colour (COLPF0..COLPF3 → playfield index 1..=4), a
                // clear pixel is background.
                AnticMode::Mode6 | AnticMode::Mode7 => {
                    let colour = ((raw_code >> 6) & 0x03) + 1;
                    let bitmap = font_row
                        .map(|row| glyph_byte(u16::from(raw_code & 0x3F), row))
                        .unwrap_or(0);
                    for bit in (0..8).rev() {
                        pixels.push(if (bitmap >> bit) & 1 != 0 { colour } else { 0 });
                    }
                }
                // 4-colour text (modes 4, 5): 7-bit glyph; the 8-pixel font is
                // read 2 bits per pixel → 4 wide pixels. Pair value 3 selects
                // COLPF2, or COLPF3 when the code's high bit is set.
                AnticMode::Mode4 | AnticMode::Mode5 => {
                    let hi = raw_code & 0x80 != 0;
                    let bitmap = font_row
                        .map(|row| glyph_byte(u16::from(raw_code & 0x7F), row))
                        .unwrap_or(0);
                    for pair in 0..4u8 {
                        let value = (bitmap >> (6 - pair * 2)) & 0x03;
                        pixels.push(match value {
                            3 if hi => 4,   // COLPF3
                            other => other, // 0 bg, 1 PF0, 2 PF1, 3 PF2
                        });
                    }
                }
                // Hi-res 2-colour text (modes 2, 3): 7-bit glyph, the high bit
                // is the inverse-video attribute (subject to CHACTL); 8 px,
                // 1 bit per pixel.
                _ => {
                    let inverse_bit = raw_code & 0x80 != 0;
                    let mut bitmap = font_row
                        .map(|row| glyph_byte(u16::from(raw_code & 0x7F), row))
                        .unwrap_or(0);
                    if inverse_bit {
                        let blanked = if blank { 0 } else { bitmap };
                        bitmap = if inverse_video { !blanked } else { blanked };
                    }
                    for bit in (0..8).rev() {
                        pixels.push(u8::from((bitmap >> bit) & 1 != 0));
                    }
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

    /// Serialize ANTIC register and internal state for save states.
    #[must_use]
    pub fn save_state(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(32);
        data.push(self.dmactl);
        data.push(self.chactl);
        data.extend_from_slice(&self.dlist.to_le_bytes());
        data.push(self.hscrol);
        data.push(self.vscrol);
        data.push(self.pmbase);
        data.push(self.chbase);
        data.push(u8::from(self.wsync));
        data.push(self.nmien);
        data.push(self.nmist);
        data.extend_from_slice(&self.scan_line.to_le_bytes());
        data.push(self.mode_line);
        data.push(self.current_mode);
        data.push(u8::from(self.current_dli));
        data.extend_from_slice(&self.memory_scan.to_le_bytes());
        data.push(self.scan_lines_per_row);
        data.push(u8::from(self.vscrol_enabled));
        data.push(u8::from(self.hscrol_enabled));
        data.push(u8::from(self.dl_active));
        data.push(u8::from(self.vbi_pending));
        data.push(u8::from(self.dli_pending));
        data.push(self.dma_cycles);
        data.push(u8::from(self.frame_complete));
        data
    }

    /// Restore ANTIC state from a save state.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is too short.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        if data.len() < 24 {
            return Err("ANTIC state truncated".into());
        }
        let mut p = 0;
        self.dmactl = data[p];
        p += 1;
        self.chactl = data[p];
        p += 1;
        self.dlist = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.hscrol = data[p];
        p += 1;
        self.vscrol = data[p];
        p += 1;
        self.pmbase = data[p];
        p += 1;
        self.chbase = data[p];
        p += 1;
        self.wsync = data[p] != 0;
        p += 1;
        self.nmien = data[p];
        p += 1;
        self.nmist = data[p];
        p += 1;
        self.scan_line = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.mode_line = data[p];
        p += 1;
        self.current_mode = data[p];
        p += 1;
        self.current_dli = data[p] != 0;
        p += 1;
        self.memory_scan = u16::from_le_bytes([data[p], data[p + 1]]);
        p += 2;
        self.scan_lines_per_row = data[p];
        p += 1;
        self.vscrol_enabled = data[p] != 0;
        p += 1;
        self.hscrol_enabled = data[p] != 0;
        p += 1;
        self.dl_active = data[p] != 0;
        p += 1;
        self.vbi_pending = data[p] != 0;
        p += 1;
        self.dli_pending = data[p] != 0;
        p += 1;
        self.dma_cycles = data[p];
        p += 1;
        self.frame_complete = data[p] != 0;
        p += 1;
        Ok(p)
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
        pm_single_line: false,
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

    fn count_stalls(dma_budget: u16) -> u16 {
        (1..=u16::from(CPU_CYCLES_PER_LINE))
            .filter(|&c| cpu_dma_stalled(c, dma_budget))
            .count() as u16
    }

    #[test]
    fn dma_spread_preserves_the_cycle_budget() {
        // However the steal is distributed, exactly `dma_budget` cycles are
        // stolen per line — the count the CPU loses must not change.
        for budget in [0u16, 1, 9, 32, 50, 72, 73, 90, 100] {
            assert_eq!(
                count_stalls(budget),
                budget,
                "exactly {budget} cycles should be stolen"
            );
        }
    }

    #[test]
    fn dma_steal_lands_in_the_fetch_window() {
        // A modest budget steals only inside (HSTART, HSYNC] — the CPU runs at
        // full speed before the display fetch, unlike the old front-loaded
        // block that stalled from cycle 0.
        for c in 1..=CYCLES_HSTART {
            assert!(
                !cpu_dma_stalled(c, 40),
                "no steal before HSTART (cycle {c})"
            );
        }
        for c in CYCLES_HSYNC + 1..=u16::from(CPU_CYCLES_PER_LINE) {
            assert!(!cpu_dma_stalled(c, 40), "no steal after HSYNC (cycle {c})");
        }
    }

    #[test]
    fn dma_overflow_spills_before_the_window() {
        // A budget larger than the 72-cycle window fills the window and spills
        // into the cycles just before HSTART, but still steals exactly budget.
        let budget = 90; // window is 72 → overflow 18
        assert_eq!(count_stalls(budget), budget);
        assert!(
            cpu_dma_stalled(CYCLES_HSTART, budget),
            "overflow reaches HSTART"
        );
        assert!(
            !cpu_dma_stalled(CYCLES_HSTART - 18, budget),
            "but not earlier than the overflow"
        );
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
    fn nmist_latches_vbi_when_vbi_nmi_is_disabled() {
        let ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.scan_line = VISIBLE_END;

        antic.process_line(&ram);

        assert_eq!(antic.read(0x0F) & 0x40, 0x40);
        assert!(!antic.take_vbi(), "NMIEN must still gate the NMI request");
    }

    #[test]
    fn nmist_latches_dli_when_dli_nmi_is_disabled() {
        let ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.scan_line = VISIBLE_START;
        antic.dmactl = 0x20;
        antic.current_dli = true;
        antic.current_mode = 0;
        antic.mode_line = 1;
        antic.scan_lines_per_row = 2;
        antic.dl_active = true;

        antic.process_line(&ram);

        assert_eq!(antic.read(0x0F) & 0x80, 0x80);
        assert!(!antic.take_dli(), "NMIEN must still gate the NMI request");
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
        ram[0x4000] = 0x4D; // Mode D + LMS
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
        ram[0x4000] = 0x4F; // Mode F + LMS
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
        ram[0x4000] = 0x42; // Mode 2 + LMS
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
    fn mode_3_blanks_extra_rows_without_reading_the_next_glyph() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.chbase = 0xE0;
        antic.char_codes.push(0x01);
        antic.mode_line = 8;

        // The byte immediately after glyph 1 is glyph 2 row 0. A raw
        // ten-line lookup would incorrectly display it on row 8.
        ram[0xE010] = 0xFF;

        let pixels = antic.render_char_line(
            &ram,
            &mode_desc(0x03).expect("ANTIC mode 3 has a descriptor"),
            1,
        );
        assert_eq!(pixels, vec![0; 8]);
    }

    #[test]
    fn mode_3_descenders_wrap_glyph_rows_zero_and_one_to_the_bottom() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.chbase = 0xE0;
        antic.char_codes.push(0x60);

        // Descender characters blank their first two display rows.
        ram[0xE300] = 0x80;
        antic.mode_line = 0;
        assert_eq!(
            antic.render_char_line(
                &ram,
                &mode_desc(0x03).expect("ANTIC mode 3 has a descriptor"),
                1,
            ),
            vec![0; 8]
        );

        // On display row 8 the low three row-counter bits address glyph row
        // 0, exposing the portion of the character stored for the descender.
        antic.mode_line = 8;
        let pixels = antic.render_char_line(
            &ram,
            &mode_desc(0x03).expect("ANTIC mode 3 has a descriptor"),
            1,
        );
        assert_eq!(pixels[0], 1);
        assert_eq!(&pixels[1..], &[0; 7]);
    }

    #[test]
    fn mode_6_five_colour_text_uses_colour_bits() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);

        // Display list: Mode 6 + LMS → screen $8000.
        ram[0x4000] = 0x46;
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x80;

        // Screen byte $C1: low 6 bits = glyph 1, top 2 bits = 11 → COLPF3.
        ram[0x8000] = 0xC1;
        // Char 1, row 0 = $80: only the leftmost pixel is set.
        ram[0xE008] = 0x80;

        antic.write(0x00, 0x22);
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.write(0x09, 0xE0); // CHBASE
        antic.scan_line = VISIBLE_START;

        let result = antic.process_line(&ram);
        assert_eq!(result.mode, AnticMode::Mode6);

        // Mode 6 is 8 px/char (1bpp font), not 4: the lit pixel takes the
        // colour the code's top two bits chose (COLPF3 → playfield index 4),
        // the rest are background.
        assert_eq!(result.playfield[0], 4);
        assert_eq!(result.playfield[1], 0);
        // Colour bits must NOT leak into the glyph index: $C1 looks up glyph
        // $01, not $41. The old `& 0x7F` decode picked the wrong (blank)
        // glyph and rendered coloured text as garbage / lowercase.
        assert!(result.playfield[0..8].contains(&4));
    }

    #[test]
    fn mode_4_four_colour_text_high_bit_selects_pf3() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);

        // Display list: Mode 4 + LMS → screen $8000.
        ram[0x4000] = 0x44;
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x80;

        // Two chars: glyph 1 plain ($01) and glyph 1 with the high bit ($81).
        ram[0x8000] = 0x01;
        ram[0x8001] = 0x81;
        // Char 1, row 0 = $C0: top pixel-pair = 11.
        ram[0xE008] = 0xC0;

        antic.write(0x00, 0x22);
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.write(0x09, 0xE0);
        antic.scan_line = VISIBLE_START;

        let result = antic.process_line(&ram);
        assert_eq!(result.mode, AnticMode::Mode4);

        // 4 px/char (2bpp font). Pair value 3 → COLPF2 (index 3) for the
        // plain char, COLPF3 (index 4) when the code's high bit is set.
        assert_eq!(result.playfield[0], 3); // char $01, leftmost pair = 11
        assert_eq!(result.playfield[4], 4); // char $81, leftmost pair = 11 + hi
    }

    /// One entry per colour clock, whatever the mode. GTIA's
    /// `fill_playfield_line` indexes the buffer by colour clock (half colour
    /// clock in the hi-res modes) and leaves anything past its end at
    /// background, so a mode that emits one entry per *pixel* draws into the
    /// left quarter (mode 8) or half (modes 9, A) of the playfield.
    #[test]
    fn every_mode_fills_the_playfield_width() {
        for mode in 0x02u8..=0x0F {
            let mut ram = make_ram();
            let mut antic = Antic::new(AnticRegion::Ntsc);

            // Display list: `mode` + LMS → screen $8000.
            ram[0x4000] = mode | 0x40;
            ram[0x4001] = 0x00;
            ram[0x4002] = 0x80;

            antic.write(0x00, 0x22); // DMACTL: normal width + DL DMA
            antic.write(0x02, 0x00);
            antic.write(0x03, 0x40); // DLIST = $4000
            antic.scan_line = VISIBLE_START;

            let result = antic.process_line(&ram);
            let hires = matches!(
                result.mode,
                AnticMode::Mode2 | AnticMode::Mode3 | AnticMode::ModeF
            );
            let expected = usize::from(result.playfield_width) * if hires { 2 } else { 1 };
            assert_eq!(
                result.playfield.len(),
                expected,
                "ANTIC mode {mode:X} returned {} entries for a {}-colour-clock playfield",
                result.playfield.len(),
                result.playfield_width
            );
        }
    }

    /// Mode 8's pixels are four colour clocks wide, so each one occupies four
    /// consecutive entries.
    #[test]
    fn mode_8_pixels_span_four_colour_clocks() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);

        ram[0x4000] = 0x48; // Mode 8 + LMS
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x80;
        ram[0x8000] = 0b0110_0000; // pixel pairs: 01, 10, 00, 00

        antic.write(0x00, 0x22);
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.scan_line = VISIBLE_START;

        let result = antic.process_line(&ram);
        assert_eq!(result.mode, AnticMode::Mode8);
        assert_eq!(result.playfield[0..4], [1, 1, 1, 1]);
        assert_eq!(result.playfield[4..8], [2, 2, 2, 2]);
        assert_eq!(result.playfield[8..12], [0, 0, 0, 0]);
    }

    /// The first scan line of a wide mode 2 line asks for more DMA than the
    /// line can spare: 48 character codes, 48 character-data bytes, 9 refresh
    /// and 1 display-list cycle. Subtracting that from the fetch window's
    /// start used to underflow — a panic in debug, and in release a wrapped
    /// comparison that reported no stall at all on exactly the lines where
    /// ANTIC holds the CPU for nearly the whole line.
    #[test]
    fn a_budget_larger_than_the_line_stalls_every_cycle_it_can() {
        let mut ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);

        ram[0x4000] = 0x42; // mode 2 + LMS
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x80;

        antic.write(0x00, 0x23); // DMACTL: DL DMA + wide playfield
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.scan_line = VISIBLE_START;

        let budget = u16::from(antic.process_line(&ram).dma_cycles);
        assert!(
            budget > CYCLES_HSYNC,
            "budget {budget} should exceed the line"
        );

        assert_eq!(count_stalls(budget), CYCLES_HSYNC);
        assert!(!cpu_dma_stalled(0, budget));
        assert!(cpu_dma_stalled(1, budget));
        assert!(cpu_dma_stalled(CYCLES_HSYNC, budget));
        assert!(!cpu_dma_stalled(CYCLES_HSYNC + 1, budget));
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
        ram[0x4000] = 0x4D;
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
        ram[0x4000] = 0x4D;
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
