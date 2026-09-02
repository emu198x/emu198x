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

/// CPU cycle at which ANTIC releases a CPU held by WSYNC — the start of
/// horizontal blank. A `STA WSYNC` halts the CPU until the beam reaches this
/// cycle, not until the next scan line.
pub const CYCLES_HSYNC: u16 = 105;

/// Missile DMA takes cycle 0, the display-list instruction byte cycle 1, the
/// four players cycles 2-5, and an LMS or jump address word cycles 6 and 7.
const MISSILE_DMA_CYCLE: u16 = 0;
const DL_INSTRUCTION_CYCLE: u16 = 1;
const PLAYER_DMA_CYCLES: std::ops::RangeInclusive<u16> = 2..=5;
const DL_OPERAND_CYCLES: (u16, u16) = (6, 7);

/// A character mode fetches the glyph bitmap three cycles after the name.
const CHARACTER_DATA_DELAY: u16 = 3;

/// Memory refresh takes nine cycles per line, the first at cycle 25 and one
/// every four after that. Playfield DMA outranks refresh; see
/// [`Antic::schedule_refresh`] for what happens when a slot is blocked.
const REFRESH_FIRST_CYCLE: u16 = 25;
const REFRESH_INTERVAL: u16 = 4;
const REFRESH_COUNT: u16 = 9;

/// The last cycle playfield DMA may occupy. A fetch that would land later does
/// not take the bus or halt the CPU, though ANTIC still reads the bus and
/// advances the memory scan counter — the "virtual DMA" of the manual.
const PLAYFIELD_LAST_CYCLE: u16 = 105;

/// Whether ANTIC takes cycle `line_cycle` of the line whose DMA is `dma_mask`.
///
/// `line_cycle` is in the hardware's own numbering: cycle 0 is missile DMA and
/// the first CPU cycle of the line, and the line runs to cycle 113.
#[must_use]
pub fn cpu_dma_stalled(line_cycle: u16, dma_mask: u128) -> bool {
    line_cycle < u16::from(CPU_CYCLES_PER_LINE) && dma_mask >> line_cycle & 1 != 0
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
// Memory
// ---------------------------------------------------------------------------

/// The memory ANTIC fetches through.
///
/// ANTIC addresses memory itself — display list, screen data, character set,
/// player/missile graphics — so it reads through the machine's live view
/// rather than a copy handed over in advance. A display list rewritten by a
/// DLI, a mid-frame character-set swap, and a page flip in the frame that
/// draws it all depend on the fetch seeing memory as it stands at that moment.
///
/// This is not the CPU bus interface [`RULES.md`] rule 6 forbids. That rule is
/// about how a *CPU* exposes its pins, so the chips sharing its bus can watch
/// them continuously. ANTIC is on the other side of that relationship: it
/// drives the address bus for its own fetches. `mos-vic-ii` reads its memory
/// the same way, through `VicMemory`.
pub trait AnticMemory {
    /// Read the byte ANTIC's address bus is pointing at.
    fn read(&self, addr: u16) -> u8;
}

/// A flat image of memory, for tests and for a machine with nothing banked.
/// Addresses wrap within the slice, which must be a power of two long.
impl AnticMemory for [u8] {
    fn read(&self, addr: u16) -> u8 {
        self[addr as usize & (self.len() - 1)]
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
    /// CPU cycles stolen by DMA this line — `dma_mask.count_ones()`.
    pub dma_cycles: u8,
    /// Which cycles ANTIC took, bit N = cycle N. Cycle 0 is missile DMA, and
    /// the line runs to cycle 113. This is what gates the CPU; the count above
    /// is for anything that only wants the total.
    pub dma_mask: u128,
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

/// The playfield ANTIC fetched for a scan line, once it has.
pub struct LinePlayfield {
    /// ANTIC display mode for this line.
    pub mode: AnticMode,
    /// Pixel data as colour register indices.
    pub playfield: Vec<u8>,
    /// Playfield width in colour clocks.
    pub playfield_width: u16,
}

/// What a scan line settled at its start and still has to fetch: the mode
/// line's row and memory position, and the DMACTL width that set its DMA
/// schedule. The fetch itself reads CHBASE, CHACTL and HSCROL when it runs.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct PendingLine {
    mode: u8,
    row: u8,
    memory_scan: u16,
    width_bits: u8,
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

/// The width ANTIC *fetches* at when a mode line enables horizontal scrolling.
///
/// A scrolled line fetches one width level wider than DMACTL asks for, so
/// there is data to shift in from the left. Wide has no wider level to go to
/// and fetches unchanged — which is why a scrolled wide playfield shifts
/// background in on the left instead of picture.
fn fetch_width_bits(width_bits: u8, hscrol_enabled: bool) -> u8 {
    match (width_bits, hscrol_enabled) {
        (0, _) => 0, // playfield DMA off — nothing to widen
        (1, true) => 2,
        (2, true) => 3,
        (w, _) => w,
    }
}

/// Where playfield DMA starts, how often it repeats, and how many fetches it
/// makes — the schedule ANTIC follows on the first scan line of a mode line.
///
/// Character modes fetch names at cycle 26 / 18 / 10 for narrow / normal / wide
/// fetch width, every two cycles (modes 2-5) or four (modes 6-7), and fetch the
/// character *data* three cycles after each name. Mapped modes fetch at cycle
/// 28 / 20 / 12, every eight cycles (modes 8-9), four (A-C) or two (D-F).
///
/// The width here is the *fetch* width, which horizontal scrolling widens by
/// one level, and scrolling delays the whole schedule by one cycle for every
/// two of HSCROL.
fn playfield_schedule(desc: &ModeDesc, fetch_bits: u8, hscrol: u8) -> (u16, u16, u16) {
    let step = match desc.antic_mode {
        AnticMode::Mode2 | AnticMode::Mode3 | AnticMode::Mode4 | AnticMode::Mode5 => 2,
        AnticMode::Mode6 | AnticMode::Mode7 => 4,
        AnticMode::Mode8 | AnticMode::Mode9 => 8,
        AnticMode::ModeA | AnticMode::ModeB | AnticMode::ModeC => 4,
        _ => 2,
    };
    let base = match (desc.char_mode, fetch_bits) {
        (true, 1) => 26,
        (true, 3) => 10,
        (true, _) => 18,
        (false, 1) => 28,
        (false, 3) => 12,
        (false, _) => 20,
    };
    let bytes = adjust_bytes_for_width(desc.bytes_per_line, fetch_bits);
    (base + u16::from(hscrol / 2), step, u16::from(bytes))
}

/// Playfield entries per colour clock. The hi-res modes carry two — GTIA
/// reads them at half-colour-clock resolution — and every other mode one.
fn entries_per_colour_clock(mode: AnticMode) -> usize {
    match mode {
        AnticMode::Mode2 | AnticMode::Mode3 | AnticMode::ModeF => 2,
        _ => 1,
    }
}

/// Window a fetched playfield to the displayed width and shift it right by
/// `hscrol` colour clocks.
///
/// The wider fetch of a scrolled line is centred on the same screen centre as
/// the narrower display, so at `hscrol == 0` the result is the fetched image
/// windowed to the requested width — the picture does not move. Each step of
/// `hscrol` then moves the window one colour clock left, which moves the
/// picture one colour clock right.
///
/// Anything the window reaches for beyond the fetched data comes back as
/// background. On a wide playfield, which has no wider level to fetch at, that
/// is the whole left edge.
fn shift_playfield(
    fetched: &[u8],
    per_cc: usize,
    fetch_width: u16,
    display_width: u16,
    hscrol: u8,
) -> Vec<u8> {
    let margin = usize::from(fetch_width.saturating_sub(display_width) / 2) * per_cc;
    let shift = usize::from(hscrol) * per_cc;
    (0..usize::from(display_width) * per_cc)
        .map(|i| {
            (i + margin)
                .checked_sub(shift)
                .and_then(|src| fetched.get(src).copied())
                .unwrap_or(0)
        })
        .collect()
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
    /// First value `mode_line` takes in the current mode line. Zero, until
    /// vertical scrolling starts the line partway down its rows.
    row_start: u8,
    /// Last value `mode_line` takes in the current mode line. One less than
    /// the mode's height, until vertical scrolling moves either end.
    row_end: u8,
    /// Whether the previous display-list mode line enabled vertical scrolling.
    /// A scrolling region's first and last lines are the ones where this
    /// disagrees with the current instruction, and those are the two that
    /// change height.
    prev_vscrol: bool,
    vscrol_enabled: bool,
    hscrol_enabled: bool,
    dl_active: bool,

    // -- NMI outputs --
    vbi_pending: bool,
    dli_pending: bool,

    // -- DMA --
    /// Which of the line's 114 cycles ANTIC takes, bit N = cycle N in the
    /// hardware's numbering (cycle 0 is missile DMA).
    dma_mask: u128,

    // -- Character code buffer (reused across scan lines within a mode line) --
    char_codes: Vec<u8>,
    /// The line begun by `begin_line` whose playfield `fetch_playfield` has
    /// yet to read.
    pending: Option<PendingLine>,

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
            row_start: 0,
            row_end: 0,
            prev_vscrol: false,
            vscrol_enabled: false,
            hscrol_enabled: false,
            dl_active: false,

            vbi_pending: false,
            dli_pending: false,

            dma_mask: 0,

            char_codes: Vec::new(),
            pending: None,

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
    /// Read an ANTIC register. `addr` is the offset within $D400-$D40F.
    ///
    /// Only VCOUNT, PENH, PENV and NMIST drive the data bus. Every other
    /// offset reads as `$FF`, and NMIST's unused low five bits read as 1,
    /// which programs rely on: `LDA $D40E / ORA #$40 / STA $D40E` is a
    /// common way to turn the VBI on, and it only ends with both NMIs
    /// enabled because the read returns `$FF` (Altirra `antic.cpp`
    /// `case 0x0E: return 0xFF; // needed or else Karateka breaks`;
    /// atari800 `antic.c` `ANTIC_GetByte` `default: return 0xff`).
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        match addr & 0x0F {
            0x0B => self.vcount(),
            0x0C => 0, // PENH (not implemented)
            0x0D => 0, // PENV (not implemented)
            0x0F => self.nmist | 0x1F,
            _ => 0xFF,
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

    /// Process one scan line in one step: [`begin_line`](Self::begin_line)
    /// and [`fetch_playfield`](Self::fetch_playfield) together, for callers
    /// that do not run a CPU between the two.
    pub fn process_line<M: AnticMemory + ?Sized>(&mut self, mem: &M) -> LineResult {
        let mut result = self.begin_line(mem);
        if let Some(fetched) = self.fetch_playfield(mem) {
            result.mode = fetched.mode;
            result.playfield = fetched.playfield;
            result.playfield_width = fetched.playfield_width;
        }
        result
    }

    /// Start one scan line: the VBI or DLI, player/missile DMA, the display
    /// list instruction and the line's DMA schedule. The playfield itself is
    /// not read yet; [`fetch_playfield`](Self::fetch_playfield) does that at
    /// the cycle [`playfield_fetch_cycle`](Self::playfield_fetch_cycle) names,
    /// so registers written earlier in the line shape it. The result's
    /// `playfield` is empty, and its `mode` and `playfield_width` describe the
    /// line the fetch will produce.
    pub fn begin_line<M: AnticMemory + ?Sized>(&mut self, mem: &M) -> LineResult {
        self.dma_mask = 0;
        self.pending = None;

        let lines_per_frame = self.region.lines_per_frame();
        let in_vblank = self.scan_line < VISIBLE_START || self.scan_line >= VISIBLE_END;

        // VBI at the start of vertical blank.
        // NMIEN bit 6 = VBI enable (bit 7 is DLI). NMIST bit 6 records VBI.
        // Each NMI source clears the other's status bit as it sets its own
        // (Altirra `antic.cpp`: `mNMIST |= 0x40; mNMIST &= ~0x80;`).
        if self.scan_line == VISIBLE_END {
            self.nmist = (self.nmist & !0x80) | 0x40;
            if self.nmien & 0x40 != 0 {
                self.vbi_pending = true;
            }
            // Reset display list state for next frame
            self.mode_line = 0;
            self.current_mode = 0;
            self.row_start = 0;
            self.row_end = 0;
            self.prev_vscrol = false;
            self.dl_active = false;
        }

        if in_vblank {
            // Vertical blank has no display fetch, so all nine refresh cycles
            // land on their slots.
            self.schedule_refresh();
            let result = blank_result(self.dma_mask);
            self.advance_scan_line(lines_per_frame);
            return result;
        }

        // Player/missile DMA has no display list of its own: once enabled it
        // runs on every displayed line, whether the playfield is drawing a
        // mode line, sitting in a blank instruction, idle after the JVB, or
        // switched off altogether.
        let (player_data, missile_data, pm_dma) = self.fetch_pm_data(mem);
        let pm_single_line = self.dmactl & 0x10 != 0;

        // Display list DMA disabled?
        let dl_dma = self.dmactl & 0x20 != 0;
        if !dl_dma {
            self.schedule_refresh();
            let mut result = blank_result(self.dma_mask);
            result.player_data = player_data;
            result.missile_data = missile_data;
            result.pm_dma = pm_dma;
            result.pm_single_line = pm_single_line;
            self.advance_scan_line(lines_per_frame);
            return result;
        }

        let width_bits = self.dmactl & 0x03;

        // Start of a new mode line — fetch the next display list instruction.
        // The row counter cannot stand in for this test: vertical scrolling
        // starts a mode line partway down its glyph, so row zero is not
        // necessarily where a line begins.
        if !self.dl_active {
            self.fetch_dl_instruction(mem);
        }

        // Claim the line's playfield DMA and note what the fetch has to read.
        let mut result = blank_result(0);
        if self.current_mode != 0
            && let Some(desc) = mode_desc(self.current_mode)
        {
            self.schedule_mode_line(&desc, width_bits);
            result.mode = desc.antic_mode;
            result.playfield_width = playfield_width_cc(width_bits);
        }
        result.player_data = player_data;
        result.missile_data = missile_data;
        result.pm_dma = pm_dma;
        result.pm_single_line = pm_single_line;
        // Refresh fills in around the playfield, which outranks it.
        self.schedule_refresh();
        result.dma_mask = self.dma_mask;
        result.dma_cycles = self.dma_mask.count_ones() as u8;

        // Advance the row counter, or close the mode line at its last row.
        if self.mode_line >= self.row_end {
            // End of this mode line — check for DLI.
            // NMIEN bit 7 = DLI enable. NMIST bit 7 records DLI.
            if self.current_dli {
                self.nmist = (self.nmist & !0x40) | 0x80;
                if self.nmien & 0x80 != 0 {
                    self.dli_pending = true;
                }
            }
            self.mode_line = 0;
            self.dl_active = false;
        } else {
            self.mode_line += 1;
        }

        self.advance_scan_line(lines_per_frame);
        result
    }

    /// The cycle of the line at which [`fetch_playfield`](Self::fetch_playfield)
    /// should run, or `None` when the line begun has no playfield.
    ///
    /// The playfield is sampled as a whole at the cycle its first colour
    /// clock is displayed: the three widths share a centre at clock 128, and
    /// the wide one loses 12 clocks off its left edge (Altirra Hardware
    /// Reference Manual, "Playfield width"), so clock 64 for narrow, 48 for
    /// normal and 44 for wide — cycle 32, 24 or 22. Writes in the cycles
    /// before it shape this line; writes from it onwards shape the next. The
    /// hardware fetches the line a byte at a time from cycle 26, 18 or 10
    /// (same manual, "Character mode playfield DMA"), so a write that lands
    /// between the first fetch and the display is taken here and not there.
    #[must_use]
    pub fn playfield_fetch_cycle(&self) -> Option<u16> {
        self.pending.map(|line| match line.width_bits {
            1 => 32,
            3 => 22,
            _ => 24,
        })
    }

    /// Read the playfield for the line [`begin_line`](Self::begin_line)
    /// started, with CHBASE, CHACTL and HSCROL as they stand now. Returns
    /// `None` when the line has no playfield, or it has already been fetched.
    pub fn fetch_playfield<M: AnticMemory + ?Sized>(&mut self, mem: &M) -> Option<LinePlayfield> {
        let line = self.pending.take()?;
        let desc = mode_desc(line.mode)?;
        Some(self.render_mode_line(mem, &desc, &line))
    }

    /// Fetch and decode the next display list instruction.
    fn fetch_dl_instruction<M: AnticMemory + ?Sized>(&mut self, mem: &M) {
        let instr = mem.read(self.dlist);
        self.dlist = self.dlist.wrapping_add(1);
        self.claim(DL_INSTRUCTION_CYCLE);

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
                self.mode_line = 0;
                self.row_start = 0;
                self.row_end = blank_count - 1;
                // Bit 5 of a blank instruction is part of the line count, not
                // VSCROL, so a blank line breaks a scrolling region.
                self.prev_vscrol = false;
                self.vscrol_enabled = false;
                self.dl_active = true;
            }
            0x01 => {
                // Jump instruction
                let lo = mem.read(self.dlist);
                self.dlist = self.dlist.wrapping_add(1);
                let hi = mem.read(self.dlist);
                self.dlist = self.dlist.wrapping_add(1);
                self.claim(DL_OPERAND_CYCLES.0);
                self.claim(DL_OPERAND_CYCLES.1);

                let target = u16::from(lo) | (u16::from(hi) << 8);
                self.dlist = target;

                if instr & 0x40 != 0 {
                    // JVB: jump and wait for vertical blank
                    self.current_mode = 0;
                    // Fill remaining visible lines with blank
                    let remaining = VISIBLE_END.saturating_sub(self.scan_line);
                    self.mode_line = 0;
                    self.row_start = 0;
                    self.row_end = (remaining.max(1) as u8) - 1;
                    self.prev_vscrol = false;
                    self.dl_active = true;
                } else {
                    // Plain jump — immediately fetch from new address
                    self.dl_active = false;
                    self.mode_line = 0;
                    // Re-fetch from the new address on this same call
                    self.fetch_dl_instruction(mem);
                }
            }
            0x02..=0x0F => {
                // Mode line
                self.current_mode = mode;

                let height = mode_desc(mode).map_or(1, |desc| desc.scan_lines_per_row);

                // Vertical fine scrolling reshapes the mode lines at the two
                // edges of a scrolling region, and leaves the ones inside it
                // alone. Entering the region (bit set, previous line clear),
                // the row counter *starts* at VSCROL, so the line is short by
                // that much at the top. Leaving it (bit clear, previous line
                // set), the line *ends* at VSCROL, so it is VSCROL + 1 rows
                // tall. Both counters are four bits whatever the mode's
                // height, so a VSCROL past the end of the glyph wraps rather
                // than truncating.
                let vscrol = self.vscrol & 0x0F;
                let first_row = if has_vscrol && !self.prev_vscrol {
                    vscrol
                } else {
                    0
                };
                let last_row = if !has_vscrol && self.prev_vscrol {
                    vscrol
                } else {
                    height - 1
                };
                let rows = (last_row.wrapping_sub(first_row) & 0x0F) + 1;
                self.mode_line = first_row;
                self.row_start = first_row;
                self.row_end = first_row + rows - 1;
                self.prev_vscrol = has_vscrol;

                if has_lms {
                    let lo = mem.read(self.dlist);
                    self.dlist = self.dlist.wrapping_add(1);
                    let hi = mem.read(self.dlist);
                    self.dlist = self.dlist.wrapping_add(1);
                    self.memory_scan = u16::from(lo) | (u16::from(hi) << 8);
                    self.claim(DL_OPERAND_CYCLES.0);
                    self.claim(DL_OPERAND_CYCLES.1);
                }

                self.dl_active = true;

                // For character modes, fetch character codes now (reused for
                // each scan line within this mode line row)
                if let Some(desc) = mode_desc(mode)
                    && desc.char_mode
                {
                    let width_bits = fetch_width_bits(self.dmactl & 0x03, self.hscrol_enabled);
                    let bytes = adjust_bytes_for_width(desc.bytes_per_line, width_bits);
                    self.char_codes.clear();
                    for i in 0..u16::from(bytes) {
                        self.char_codes
                            .push(mem.read(self.memory_scan.wrapping_add(i)));
                    }
                    let (start, step, count) = playfield_schedule(&desc, width_bits, self.hscrol);
                    self.claim_playfield(start, step, count);
                    // Memory scan advances past character codes
                    self.memory_scan = self.memory_scan.wrapping_add(u16::from(bytes));
                }
            }
            _ => unreachable!(),
        }
    }

    /// Claim a mode line's playfield DMA for this scan line and record what
    /// the fetch has to read. Character data is fetched on every scan line of
    /// the mode line, three cycles after the name fetch would sit; a mapped
    /// mode fills ANTIC's line buffer on the first scan line and replays it
    /// for the rest, so its DMA is charged once. The memory scan counter
    /// moves on after the mode line's last row, whether or not the playfield
    /// is fetched.
    fn schedule_mode_line(&mut self, desc: &ModeDesc, width_bits: u8) {
        let fetch_bits = fetch_width_bits(width_bits, self.hscrol_enabled);
        let (start, step, fetches) = playfield_schedule(desc, fetch_bits, self.hscrol);
        if desc.char_mode {
            self.claim_playfield(start + CHARACTER_DATA_DELAY, step, fetches);
        } else if self.mode_line == self.row_start {
            self.claim_playfield(start, step, fetches);
        }
        self.pending = Some(PendingLine {
            mode: self.current_mode,
            row: self.mode_line,
            memory_scan: self.memory_scan,
            width_bits,
        });
        if !desc.char_mode && self.mode_line >= self.row_end {
            let bytes = adjust_bytes_for_width(desc.bytes_per_line, fetch_bits);
            self.memory_scan = self.memory_scan.wrapping_add(u16::from(bytes));
        }
    }

    /// Render pixel data for a scan line of the current mode line.
    fn render_mode_line<M: AnticMemory + ?Sized>(
        &self,
        mem: &M,
        desc: &ModeDesc,
        line: &PendingLine,
    ) -> LinePlayfield {
        let fetch_bits = fetch_width_bits(line.width_bits, self.hscrol_enabled);
        let bytes = adjust_bytes_for_width(desc.bytes_per_line, fetch_bits);
        let pf_width = playfield_width_cc(line.width_bits);

        let mut playfield = if desc.char_mode {
            self.render_char_line(mem, desc, bytes, line.row)
        } else {
            render_bitmap_line(mem, desc, bytes, line.memory_scan)
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

        if self.hscrol_enabled {
            playfield = shift_playfield(
                &playfield,
                entries_per_colour_clock(desc.antic_mode),
                playfield_width_cc(fetch_bits),
                pf_width,
                self.hscrol & 0x0F,
            );
        }

        LinePlayfield {
            mode: desc.antic_mode,
            playfield,
            playfield_width: pf_width,
        }
    }

    /// Render a character mode scan line: row `mode_row` of the mode line.
    fn render_char_line<M: AnticMemory + ?Sized>(
        &self,
        mem: &M,
        desc: &ModeDesc,
        bytes: u8,
        mode_row: u8,
    ) -> Vec<u8> {
        let chbase_addr = u16::from(self.chbase) << 8;
        // CHACTL: bit 1 = inverse-video enable, bit 0 = blank, bit 2 = reflect.
        let inverse_video = self.chactl & 0x02 != 0;
        let blank = self.chactl & 0x01 != 0;
        let reflect = self.chactl & 0x04 != 0;

        // Every ANTIC text mode uses an 8-byte-per-glyph font. The
        // double-height modes (5, 7) show each font line on two scan lines,
        // so the font row is the mode-line row halved.
        let double_height = matches!(desc.antic_mode, AnticMode::Mode5 | AnticMode::Mode7);
        // The row counter is four bits whatever the mode's height, so a
        // vertically scrolled line that starts past the end of the glyph wraps
        // back to its top rather than reading into the next one.
        let row = mode_row & 0x0F;
        let raw_row = if double_height { row / 2 } else { row };
        let count = usize::min(self.char_codes.len(), bytes as usize);
        let mut pixels = Vec::new();

        let glyph_byte = |glyph: u16, font_row: u8| -> u8 {
            let addr = chbase_addr
                .wrapping_add(glyph.wrapping_mul(8))
                .wrapping_add(u16::from(font_row));
            mem.read(addr)
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

    /// Fetch player/missile DMA data if enabled.
    fn fetch_pm_data<M: AnticMemory + ?Sized>(&mut self, mem: &M) -> ([u8; 4], u8, bool) {
        let player_dma = self.dmactl & 0x08 != 0;
        // Enabling player DMA enables missile DMA with it; bit 2 only matters
        // on its own. The Hardware Manual states you cannot disable missile
        // DMA while enabling player DMA (quoted in the Complete and Essential
        // Map, part II), and atari800 gates missiles on DMACTL & $0C.
        let missile_dma = self.dmactl & 0x0C != 0;
        let single_line = self.dmactl & 0x10 != 0;

        if !missile_dma {
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
            missile_data = mem.read(pm_base.wrapping_add(offset).wrapping_add(line));
            self.claim(MISSILE_DMA_CYCLE);
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
                let addr = pm_base.wrapping_add(offset).wrapping_add(line);
                player_data[p as usize] = mem.read(addr);
            }
            for cycle in PLAYER_DMA_CYCLES {
                self.claim(cycle);
            }
        }

        (player_data, missile_data, true)
    }

    /// Claim `count` playfield fetches, `step` cycles apart, from `start`.
    /// Fetches past [`PLAYFIELD_LAST_CYCLE`] are virtual: ANTIC still reads
    /// them, but they neither take the bus nor halt the CPU.
    fn claim_playfield(&mut self, start: u16, step: u16, count: u16) {
        for i in 0..count {
            let cycle = start + i * step;
            if cycle > PLAYFIELD_LAST_CYCLE {
                break;
            }
            self.claim(cycle);
        }
    }

    /// Take one cycle of the line for DMA.
    fn claim(&mut self, cycle: u16) {
        if cycle < u16::from(CPU_CYCLES_PER_LINE) {
            self.dma_mask |= 1u128 << cycle;
        }
    }

    fn taken(&self, cycle: u16) -> bool {
        cycle < u16::from(CPU_CYCLES_PER_LINE) && self.dma_mask >> cycle & 1 != 0
    }

    /// Place the line's nine refresh cycles around whatever playfield DMA has
    /// already claimed, which outranks them.
    ///
    /// A refresh whose slot is blocked moves to the next free cycle. Only one
    /// can be waiting at a time; a second blocked refresh while one is still
    /// waiting is dropped. On the first scan line of modes 2-5 the bus is
    /// contended enough that only one or two of the nine survive, and in wide
    /// character modes the last one can be pushed past the end of playfield DMA
    /// to cycle 105 or 106.
    fn schedule_refresh(&mut self) {
        let mut waiting = false;
        let mut next = 0u16;
        for cycle in REFRESH_FIRST_CYCLE..u16::from(CPU_CYCLES_PER_LINE) {
            let due =
                next < REFRESH_COUNT && cycle == REFRESH_FIRST_CYCLE + next * REFRESH_INTERVAL;
            if due {
                next += 1;
            }
            if self.taken(cycle) {
                if due {
                    waiting = true;
                }
            } else if due {
                self.claim(cycle);
            } else if waiting {
                self.claim(cycle);
                waiting = false;
            }
        }
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
        data.push(self.row_start);
        data.push(self.row_end);
        data.push(u8::from(self.prev_vscrol));
        data.push(u8::from(self.vscrol_enabled));
        data.push(u8::from(self.hscrol_enabled));
        data.push(u8::from(self.dl_active));
        data.push(u8::from(self.vbi_pending));
        data.push(u8::from(self.dli_pending));
        data.extend_from_slice(&self.dma_mask.to_le_bytes());
        data.push(u8::from(self.frame_complete));
        data
    }

    /// Restore ANTIC state from a save state.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is too short.
    pub fn load_state(&mut self, data: &[u8]) -> Result<usize, String> {
        if data.len() < 41 {
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
        self.row_start = data[p];
        p += 1;
        self.row_end = data[p];
        p += 1;
        self.prev_vscrol = data[p] != 0;
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
        self.dma_mask = u128::from_le_bytes(
            data[p..p + 16]
                .try_into()
                .map_err(|_| "ANTIC state truncated")?,
        );
        p += 16;
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

/// Render a bitmap mode scan line from `bytes` bytes at `memory_scan`.
fn render_bitmap_line<M: AnticMemory + ?Sized>(
    mem: &M,
    desc: &ModeDesc,
    bytes: u8,
    memory_scan: u16,
) -> Vec<u8> {
    let mut pixels = Vec::new();

    // Fetch playfield data bytes
    for i in 0..u16::from(bytes) {
        let data = mem.read(memory_scan.wrapping_add(i));

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

    pixels
}

/// Create a blank `LineResult`.
fn blank_result(dma_mask: u128) -> LineResult {
    LineResult {
        mode: AnticMode::Blank,
        playfield: Vec::new(),
        playfield_width: 0,
        dma_cycles: dma_mask.count_ones() as u8,
        dma_mask,
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

        // Simulate VBI pending; the unused low bits read as 1.
        antic.nmist = 0x40;
        assert_eq!(antic.read(0x0F), 0x5F);

        // NMIRES clears status
        antic.write(0x0F, 0x00);
        assert_eq!(antic.read(0x0F), 0x1F);
    }

    #[test]
    fn write_only_and_unused_registers_read_as_ff() {
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.write(0x0E, 0xC0);
        for addr in [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0E,
        ] {
            assert_eq!(antic.read(addr), 0xFF, "offset {addr:#04x}");
        }
    }

    #[test]
    fn each_nmi_source_clears_the_others_status_bit() {
        let ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.nmist = 0x80;
        antic.scan_line = VISIBLE_END;
        antic.process_line(&ram[..]);
        assert_eq!(antic.read(0x0F), 0x5F);

        antic.scan_line = VISIBLE_START;
        antic.dmactl = 0x20;
        antic.current_dli = true;
        antic.current_mode = 0;
        antic.mode_line = 1;
        antic.row_end = 1;
        antic.dl_active = true;
        antic.process_line(&ram[..]);
        assert_eq!(antic.read(0x0F), 0x9F);
    }

    #[test]
    fn nmist_latches_vbi_when_vbi_nmi_is_disabled() {
        let ram = make_ram();
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.scan_line = VISIBLE_END;

        antic.process_line(&ram[..]);

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
        antic.row_end = 1;
        antic.dl_active = true;

        antic.process_line(&ram[..]);

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

        let result = antic.process_line(&ram[..]);
        assert_eq!(result.mode, AnticMode::Blank);
        // Should set up 3 blank lines
        assert_eq!(antic.row_end, 2);
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

        let result = antic.process_line(&ram[..]);
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

        let result = antic.process_line(&ram[..]);
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

        let result = antic.process_line(&ram[..]);
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

        // The byte immediately after glyph 1 is glyph 2 row 0. A raw
        // ten-line lookup would incorrectly display it on row 8.
        ram[0xE010] = 0xFF;

        let pixels = antic.render_char_line(
            &ram[..],
            &mode_desc(0x03).expect("ANTIC mode 3 has a descriptor"),
            1,
            8,
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
        assert_eq!(
            antic.render_char_line(
                &ram[..],
                &mode_desc(0x03).expect("ANTIC mode 3 has a descriptor"),
                1,
                0,
            ),
            vec![0; 8]
        );

        // On display row 8 the low three row-counter bits address glyph row
        // 0, exposing the portion of the character stored for the descender.
        let pixels = antic.render_char_line(
            &ram[..],
            &mode_desc(0x03).expect("ANTIC mode 3 has a descriptor"),
            1,
            8,
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

        let result = antic.process_line(&ram[..]);
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

        let result = antic.process_line(&ram[..]);
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

            let result = antic.process_line(&ram[..]);
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

        let result = antic.process_line(&ram[..]);
        assert_eq!(result.mode, AnticMode::Mode8);
        assert_eq!(result.playfield[0..4], [1, 1, 1, 1]);
        assert_eq!(result.playfield[4..8], [2, 2, 2, 2]);
        assert_eq!(result.playfield[8..12], [0, 0, 0, 0]);
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

        let result = antic.process_line(&ram[..]);
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

        let result = antic.process_line(&ram[..]);
        // refresh(9) + DL(1) + LMS(2) + playfield(40) = 52
        assert_eq!(result.dma_cycles, 52);
    }

    #[test]
    fn frame_wraps_at_correct_line_count() {
        let mut antic_ntsc = Antic::new(AnticRegion::Ntsc);
        antic_ntsc.scan_line = 261;
        let ram = make_ram();

        antic_ntsc.process_line(&ram[..]);
        assert!(antic_ntsc.frame_complete());
        assert_eq!(antic_ntsc.scan_line(), 0);

        let mut antic_pal = Antic::new(AnticRegion::Pal);
        antic_pal.scan_line = 311;

        antic_pal.process_line(&ram[..]);
        assert!(antic_pal.frame_complete());
        assert_eq!(antic_pal.scan_line(), 0);
    }

    #[test]
    fn vblank_does_not_wrap_early() {
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.scan_line = 250;
        let ram = make_ram();

        antic.process_line(&ram[..]);
        assert!(!antic.frame_complete());
        assert_eq!(antic.scan_line(), 251);
    }

    #[test]
    fn pm_dma_cycle_counting() {
        let result = first_line(0x4D, 0x2E, 0); // mode D + LMS, normal + P/M DMA
        assert!(result.pm_dma);

        // Mode D fetches every two cycles from cycle 20, so it takes the even
        // cycles and leaves every refresh slot — all of which are odd — alone.
        // missile(1) + DL(1) + players(4) + LMS(2) + playfield(40) + refresh(9)
        assert_eq!(result.dma_cycles, 57);
        assert_eq!(cycles(result.dma_mask, 0..10), vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    /// P/M data for the first displayed line with PMBASE at zero, two-line
    /// resolution: missiles at $180 and player 0 at $200, indexed by half
    /// the scan line.
    fn pm_ram() -> Vec<u8> {
        let mut ram = make_ram();
        ram[0x0180 + usize::from(VISIBLE_START / 2)] = 0x03;
        ram[0x0200 + usize::from(VISIBLE_START / 2)] = 0xFF;
        ram
    }

    /// P/M DMA has no display list of its own, so a blank instruction and a
    /// display list that is switched off both still feed GTIA.
    #[test]
    fn pm_dma_runs_without_a_mode_line() {
        for dmactl in [0x2E, 0x0E] {
            let result = first_line_in(pm_ram(), 0x70, dmactl, 0);
            assert!(result.pm_dma, "DMACTL {dmactl:#04x}");
            assert_eq!(result.player_data[0], 0xFF);
            assert_eq!(result.missile_data, 0x03);
            // The missile and player fetch cycles are taken either way; only
            // the display-list byte at cycle 1 depends on DL DMA.
            assert_eq!(cycles(result.dma_mask, 0..1), vec![0]);
            assert_eq!(cycles(result.dma_mask, 2..6), vec![2, 3, 4, 5]);
        }
    }

    /// Enabling player DMA enables missile DMA with it.
    #[test]
    fn player_dma_fetches_the_missiles_too() {
        let result = first_line_in(pm_ram(), 0x70, 0x2A, 0);
        assert_eq!(result.missile_data, 0x03);
        assert_eq!(cycles(result.dma_mask, 0..6), vec![0, 1, 2, 3, 4, 5]);

        let missiles_only = first_line_in(pm_ram(), 0x70, 0x26, 0);
        assert_eq!(missiles_only.missile_data, 0x03);
        assert_eq!(missiles_only.player_data, [0; 4]);
        assert_eq!(cycles(missiles_only.dma_mask, 0..6), vec![0, 1]);
    }

    /// Run one scan line of a fresh ANTIC: `instr` as the whole display list,
    /// `dmactl` as written, `hscrol` in the register.
    fn first_line(instr: u8, dmactl: u8, hscrol: u8) -> LineResult {
        first_line_in(make_ram(), instr, dmactl, hscrol)
    }

    /// [`first_line`] over caller-supplied RAM.
    fn first_line_in(mut ram: Vec<u8>, instr: u8, dmactl: u8, hscrol: u8) -> LineResult {
        ram[0x4000] = instr;
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x80;

        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.write(0x00, dmactl);
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.write(0x04, hscrol);
        antic.scan_line = VISIBLE_START;
        antic.process_line(&ram[..])
    }

    /// The cycles a mask claims within `range`.
    fn cycles(mask: u128, range: std::ops::Range<u16>) -> Vec<u16> {
        range.filter(|&c| mask >> c & 1 != 0).collect()
    }

    /// Cycle 0 is missile DMA, cycle 1 the display-list instruction byte,
    /// cycles 2-5 the four players, and cycles 6-7 an LMS or jump address
    /// word. *Altirra Hardware Reference Manual* §4.14.
    #[test]
    fn the_fixed_fetches_sit_where_the_hardware_puts_them() {
        // Mode 8 fetches every eight cycles from cycle 20, so nothing it does
        // reaches back into the first ten cycles.
        let with_lms = first_line(0x48, 0x2E, 0);
        assert_eq!(
            cycles(with_lms.dma_mask, 0..10),
            vec![0, 1, 2, 3, 4, 5, 6, 7]
        );

        // Without P/M DMA the missile and player cycles go back to the CPU.
        let no_pm = first_line(0x48, 0x22, 0);
        assert_eq!(cycles(no_pm.dma_mask, 0..10), vec![1, 6, 7]);
    }

    /// Character modes fetch names from cycle 18 at normal width, every two
    /// cycles in modes 2-5, and the glyph data three cycles after each name.
    #[test]
    fn character_names_and_data_fetch_at_the_documented_cycles() {
        let result = first_line(0x42, 0x22, 0); // mode 2 + LMS, normal width

        let names: Vec<u16> = (18..=96).step_by(2).collect();
        let data: Vec<u16> = (21..=99).step_by(2).collect();
        for cycle in names.iter().chain(data.iter()) {
            assert!(
                result.dma_mask >> cycle & 1 != 0,
                "cycle {cycle} should be a playfield fetch"
            );
        }
        // Nothing before the first name fetch but the display list.
        assert_eq!(cycles(result.dma_mask, 0..18), vec![1, 6, 7]);
    }

    /// Playfield DMA outranks refresh. On the first scan line of a mode 2 line
    /// the bus is solid from the first name fetch to the last data fetch, so
    /// every refresh slot is blocked and only the one deferred refresh lands.
    /// On later scan lines only the data fetches remain, and each refresh slips
    /// one cycle into the gap between them.
    #[test]
    fn refresh_gives_way_to_playfield_and_takes_the_next_free_cycle() {
        let mut ram = make_ram();
        ram[0x4000] = 0x42; // mode 2 + LMS
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x80;

        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.write(0x00, 0x22);
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.scan_line = VISIBLE_START;

        // First scan line: names on the even cycles 18-96, data on the odd
        // cycles 21-99. Every refresh slot is blocked, and the one that gets
        // deferred waits until cycle 98 — the first even cycle past the last
        // name fetch. The other eight are dropped.
        let first = antic.process_line(&ram[..]);
        assert_eq!(cycles(first.dma_mask, 98..114), vec![98, 99]);
        assert_eq!(first.dma_cycles, 1 + 2 + 40 + 40 + 1);

        // Later scan lines fetch only character data, so each refresh slot is
        // blocked but the cycle after it is free.
        let later = antic.process_line(&ram[..]);
        // Character data still holds every odd cycle from 21, so each refresh
        // slot is blocked and slips onto the even cycle after it. Those are the
        // only even cycles a replayed line takes.
        let evens: Vec<u16> = cycles(later.dma_mask, 0..114)
            .into_iter()
            .filter(|c| c % 2 == 0)
            .collect();
        assert_eq!(evens, (26..=58).step_by(4).collect::<Vec<_>>());
        assert_eq!(later.dma_cycles, 40 + 9);
    }

    /// Vertical blank has no display fetch, so all nine refresh cycles land on
    /// their own slots: cycle 25, then every four.
    #[test]
    fn vertical_blank_takes_all_nine_refresh_cycles() {
        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.write(0x00, 0x22);
        antic.scan_line = VISIBLE_END;

        let result = antic.process_line(&make_ram()[..]);
        assert_eq!(
            cycles(result.dma_mask, 0..114),
            (25..=57).step_by(4).collect::<Vec<_>>()
        );
    }

    /// A mapped mode loads ANTIC's line buffer on the first scan line of the
    /// mode line and replays it, so it takes no playfield DMA afterwards.
    #[test]
    fn a_mapped_mode_fetches_once_per_mode_line() {
        let mut ram = make_ram();
        ram[0x4000] = 0x4D; // mode D + LMS — two scan lines per row
        ram[0x4001] = 0x00;
        ram[0x4002] = 0x80;

        let mut antic = Antic::new(AnticRegion::Ntsc);
        antic.write(0x00, 0x22);
        antic.write(0x02, 0x00);
        antic.write(0x03, 0x40);
        antic.scan_line = VISIBLE_START;

        let first = antic.process_line(&ram[..]);
        assert_eq!(first.dma_cycles, 1 + 2 + 40 + 9);

        let second = antic.process_line(&ram[..]);
        assert_eq!(second.dma_cycles, 9, "only refresh on a replayed line");
    }

    /// Horizontal scrolling delays the whole fetch window by one cycle for
    /// every two of HSCROL, and odd values share the even value's timing.
    #[test]
    fn hscrol_delays_playfield_dma_one_cycle_per_two() {
        for hscrol in 0..16u8 {
            // Mode 8 + LMS + HSCROL, normal width — a slow fetch rate keeps the
            // playfield clear of the fixed cycles and of each refresh slot.
            let result = first_line(0x58, 0x22, hscrol);
            let first_fetch = cycles(result.dma_mask, 8..114)[0];
            assert_eq!(first_fetch, 12 + u16::from(hscrol / 2), "HSCROL {hscrol}");
        }
    }

    /// A playfield fetch that would land past cycle 105 is virtual: ANTIC still
    /// reads it and advances the memory scan counter, but it neither takes the
    /// bus nor halts the CPU.
    #[test]
    fn playfield_dma_stops_at_cycle_105() {
        // Wide mode 2 with the maximum scroll starts the name fetches at cycle
        // 17 and would run them to 111. The bus stays solid to 105, so the one
        // deferred refresh takes 106 — which the manual calls out as the
        // furthest a refresh can be pushed — and nothing at all lands later.
        let result = first_line(0x52, 0x23, 15);
        assert!(
            result.dma_mask >> 105 & 1 != 0,
            "cycle 105 is still fair game"
        );
        assert_eq!(cycles(result.dma_mask, 106..114), vec![106]);

        // A wide line is solid to 105 with or without scrolling, so the
        // contrast is a normal-width one: its playfield ends at 99, the
        // deferred refresh finds a gap before then, and the tail of the line
        // is the CPU's.
        let normal = first_line(0x42, 0x22, 0);
        assert!(cycles(normal.dma_mask, 100..114).is_empty());
    }

    /// Cycle 0 is missile DMA and the first CPU cycle of the line; the line
    /// runs to 113. Anything past that is not a cycle of this line.
    #[test]
    fn the_stall_predicate_reads_the_mask_in_hardware_numbering() {
        let mask = 1u128 | 1 << 113;
        assert!(cpu_dma_stalled(0, mask));
        assert!(!cpu_dma_stalled(1, mask));
        assert!(cpu_dma_stalled(113, mask));
        assert!(!cpu_dma_stalled(114, mask));
    }
}
