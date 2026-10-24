//! The Sega 315-5313: the Mega Drive's video display processor.
//!
//! This crate is being built in stages under emu198x/emu198x#2033. This
//! stage is the chip's bus side and its clock:
//!
//! - its three memories: 64 KB of VRAM, 64 words of CRAM and 40 of VSRAM;
//! - the control port's two-word command latch and the 24 registers;
//! - the four-entry write FIFO, drained one access per external slot, and
//!   the read prefetch;
//! - DMA: VRAM fill, VRAM copy and transfers from 68000 memory, with the VDP
//!   as bus master;
//! - the H and V counters, their latch, the status register, and the V-int,
//!   H-int and external interrupts.
//!
//! - planes A and B and the window, with every scroll mode, priority,
//!   shadow and highlight, and output to an ARGB framebuffer (see
//!   [`render`]);
//! - sprites, scanned, placed and drawn cell by cell in their own slots,
//!   with the per-line limits, masking, overflow and collision.
//!
//! The line is stepped slot by slot from the master clock, so the FIFO, DMA
//! and the CPU's reads are served exactly when the line's external slots
//! come round (see [`timing`]), and every display fetch happens in its own
//! slot. Mode 4, the Master System mode, is still to come.
//!
//! # Wiring it up
//!
//! Call [`Vdp::tick`] once per master clock (53.693175 MHz NTSC,
//! 53.203424 MHz PAL). The CPU side is the four port calls:
//! [`Vdp::write_data`] and [`Vdp::read_data`] return `None` while the FIFO is
//! full or a read is not ready, and the machine holds the CPU's DTACK until
//! they succeed. During a 68000-to-VDP DMA, [`Vdp::bus_request`] asks for the
//! 68000's bus, and [`Vdp::dma_read`] names the word the VDP wants; the
//! machine reads it and hands it over with [`Vdp::dma_word`]. The chip owns
//! no bus and makes no callbacks.

pub mod render;
mod sprites;
pub mod timing;

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use timing::{Slot, VINT_SLOT, VTiming, Width};

/// VRAM size: the Mega Drive fits 64 KB.
pub const VRAM_SIZE: usize = 0x10000;
/// CRAM entries: four palettes of 16 colours.
pub const CRAM_SIZE: usize = 64;
/// VSRAM entries on the Mega Drive VDP.
pub const VSRAM_SIZE: usize = 40;
/// Entries in the write FIFO.
pub const FIFO_SIZE: usize = 4;
/// Bytes of sprite attribute table the VDP caches internally: the Y, size
/// and link fields of 80 sprites.
const SAT_CACHE_SIZE: usize = 80 * 4;

/// The slots a written word waits in the FIFO before it can be written.
const FIFO_LATENCY: u64 = 3;
/// The slots between setting up a read and the first prefetch.
const READ_LATENCY: u64 = 3;
/// The slots between a 68000 DMA command and the first source read.
const DMA_START_SLOTS: u64 = 3;

/// The CRAM bits that exist: three per channel.
const CRAM_BITS: u16 = 0x0EEE;
/// The VSRAM bits that exist.
const VSRAM_BITS: u16 = 0x07FF;

// Register numbers.
const REG_MODE_1: usize = 0x00;
const REG_MODE_2: usize = 0x01;
const REG_BG_COLOUR: usize = 0x07;
const REG_HINT: usize = 0x0A;
const REG_MODE_3: usize = 0x0B;
const REG_MODE_4: usize = 0x0C;
const REG_SAT: usize = 0x05;
const REG_AUTOINC: usize = 0x0F;
const REG_DMA_LEN_L: usize = 0x13;
const REG_DMA_LEN_H: usize = 0x14;
const REG_DMA_SRC_L: usize = 0x15;
const REG_DMA_SRC_M: usize = 0x16;
const REG_DMA_SRC_H: usize = 0x17;

// Register bits.
const BIT_HINT_EN: u8 = 0x10;
const BIT_HV_LATCH: u8 = 0x02;
const BIT_128K: u8 = 0x80;
const BIT_DISPLAY: u8 = 0x40;
const BIT_VINT_EN: u8 = 0x20;
const BIT_DMA_EN: u8 = 0x10;
const BIT_V30: u8 = 0x08;
const BIT_MODE_5: u8 = 0x04;
const BIT_EXT_EN: u8 = 0x08;
const BIT_H40: u8 = 0x01;
const BIT_INTERLACE: u8 = 0x02;
const BIT_DOUBLE_RES: u8 = 0x04;

// DMA modes, register $17 bits 7-6.
const DMA_TYPE_MASK: u8 = 0xC0;
const DMA_FILL: u8 = 0x80;
const DMA_COPY: u8 = 0xC0;

// Command codes, CD3-CD0.
const VRAM_READ: u8 = 0x0;
const VRAM_WRITE: u8 = 0x1;
const CRAM_WRITE: u8 = 0x3;
const VSRAM_READ: u8 = 0x4;
const VSRAM_WRITE: u8 = 0x5;
const CRAM_READ: u8 = 0x8;
const VRAM_READ_8: u8 = 0xC;
/// CD5: the command starts a DMA.
const CD_DMA: u8 = 0x20;

/// Where the frame is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    /// V-blank: no display fetches.
    Inactive,
    /// Line `$1FF`: the fetches for line 0 under way.
    Preparing,
    /// The picture.
    Active,
}

/// One word waiting in the FIFO.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct FifoEntry {
    /// The master clock from which it may be written.
    ready_at: u64,
    address: u32,
    value: u16,
    cd: u8,
    /// A VRAM word whose first byte has been written, or a fill entry,
    /// which writes one byte.
    partial: bool,
}

/// The Mega Drive VDP.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vdp {
    /// A PAL machine: 50 Hz, 313 lines, and the status register's PAL bit.
    pub pal_machine: bool,

    vram: Vec<u8>,
    #[serde(with = "BigArray")]
    cram: [u16; CRAM_SIZE],
    #[serde(with = "BigArray")]
    vsram: [u16; VSRAM_SIZE],
    #[serde(with = "BigArray")]
    sat_cache: [u8; SAT_CACHE_SIZE],
    regs: [u8; 24],

    // The command latch.
    address: u32,
    address_latch: u32,
    cd: u8,
    pending: bool,

    // The FIFO.
    fifo: [FifoEntry; FIFO_SIZE],
    fifo_read: Option<usize>,
    fifo_write: usize,

    // Reads.
    prefetch: u16,
    read_fetched: bool,
    read_half: bool,
    read_ready_at: u64,

    // DMA.
    dma_run: bool,
    dma_start_at: Option<u64>,
    bus_request: bool,
    dma_read: Option<u32>,

    // The clock.
    mclk: u64,
    slot_left: u32,
    hslot: u8,
    vcounter: u16,
    state: State,
    even_field: bool,

    // Interrupts and status.
    hint_counter: u8,
    vint_pending: bool,
    hint_pending: bool,
    ext_pending: bool,
    hv_latch: u16,
    sprite_overflow: bool,
    sprite_collision: bool,

    // The display pipeline (see `render`).
    hscroll: [u16; 2],
    vscroll_latch: [u16; 2],
    map_entries: [u16; 2],
    v_offset: u8,
    in_window: bool,
    window_edge: bool,
    window_h_latch: u8,
    window_v_latch: u8,
    plane_buf: [[u8; 32]; 2],
    buf_off: [u8; 2],
    sprite_line: Vec<u8>,
    sprites: sprites::Sprites,
    composite: Vec<u8>,
    composite_valid: bool,
    #[serde(skip, default = "blank_frame")]
    frame: Vec<u32>,
    output_row: Option<usize>,
    frames: u64,
    frame_width: usize,
    frame_height: usize,
}

/// The tallest frame: PAL, borders included.
const FRAME_ROWS: usize = 294;

fn blank_frame() -> Vec<u32> {
    vec![0xFF00_0000; render::LINE_WIDTH * FRAME_ROWS]
}

impl Vdp {
    /// A VDP at power-on, for a PAL or NTSC machine.
    #[must_use]
    pub fn new(pal_machine: bool) -> Self {
        let mut regs = [0; 24];
        regs[REG_HINT] = 0xFF;
        Self {
            pal_machine,
            vram: vec![0; VRAM_SIZE],
            cram: [0; CRAM_SIZE],
            vsram: [0; VSRAM_SIZE],
            sat_cache: [0; SAT_CACHE_SIZE],
            regs,
            address: 0,
            address_latch: 0,
            cd: 0,
            pending: false,
            fifo: [FifoEntry::default(); FIFO_SIZE],
            fifo_read: None,
            fifo_write: 0,
            prefetch: 0,
            read_fetched: false,
            read_half: false,
            read_ready_at: 0,
            dma_run: false,
            dma_start_at: None,
            bus_request: false,
            dma_read: None,
            mclk: 0,
            slot_left: 0,
            hslot: 0,
            vcounter: 0,
            state: State::Inactive,
            even_field: false,
            hint_counter: 0xFF,
            vint_pending: false,
            hint_pending: false,
            ext_pending: false,
            hv_latch: 0,
            sprite_overflow: false,
            sprite_collision: false,
            hscroll: [0; 2],
            vscroll_latch: [0; 2],
            map_entries: [0; 2],
            v_offset: 0,
            in_window: false,
            window_edge: false,
            window_h_latch: 0,
            window_v_latch: 0,
            plane_buf: [[0; 32]; 2],
            buf_off: [0; 2],
            sprite_line: vec![0; render::LINE_WIDTH],
            sprites: sprites::Sprites::default(),
            composite: vec![0; render::LINE_WIDTH],
            composite_valid: false,
            frame: blank_frame(),
            output_row: None,
            frames: 0,
            frame_width: 256 + render::BORDER_LEFT + render::BORDER_RIGHT,
            frame_height: if pal_machine { 294 } else { 243 },
        }
    }

    // ---- Configuration read from the registers. ----

    fn width(&self) -> Width {
        if self.regs[REG_MODE_4] & BIT_H40 != 0 {
            Width::H40
        } else {
            Width::H32
        }
    }

    fn mode_5(&self) -> bool {
        self.regs[REG_MODE_2] & BIT_MODE_5 != 0
    }

    fn v_timing(&self) -> VTiming {
        VTiming::new(self.pal_machine, self.regs[REG_MODE_2] & BIT_V30 != 0)
    }

    /// Master clocks per slot in the current width, for latencies.
    fn slot_len(&self) -> u64 {
        u64::from(match self.width() {
            Width::H40 => timing::MCLKS_SLOT_H40,
            Width::H32 => timing::MCLKS_SLOT_H32,
        })
    }

    /// Whether the display is fetching: the picture is on and enabled.
    fn is_active(&self) -> bool {
        self.state != State::Inactive && self.regs[REG_MODE_2] & BIT_DISPLAY != 0 && self.mode_5()
    }

    fn dma_type(&self) -> u8 {
        self.regs[REG_DMA_SRC_H] & DMA_TYPE_MASK
    }

    // ---- The clock. ----

    /// One master clock.
    pub fn tick(&mut self) {
        if self.slot_left == 0 {
            self.begin_slot();
            self.slot_left = self.width().slot_mclks(self.hslot);
        }
        self.slot_left -= 1;
        self.mclk += 1;
        if self.slot_left == 0 {
            self.hslot = self.width().next_hslot(self.hslot);
        }
    }

    /// Everything that happens at the start of a slot.
    fn begin_slot(&mut self) {
        let width = self.width();
        let v = self.v_timing();
        if self.hslot == width.line_change() {
            self.advance_line();
        }
        // The picture ends after the line change onto the first blank line.
        if self.state == State::Active
            && self.vcounter == v.blank_start
            && self.hslot != width.line_change()
            && self.hslot != width.line_change() + 1
        {
            self.state = State::Inactive;
            self.end_sprite_picture();
        }
        if self.state != State::Active || !self.is_active() {
            if self.vcounter == v.blank_start && self.hslot == VINT_SLOT {
                self.vint_pending = true;
            } else if self.vcounter == v.blank_start
                && self.hslot == 1
                && self.regs[REG_MODE_4] & BIT_INTERLACE != 0
            {
                self.even_field = !self.even_field;
            }
        }
        if self.dma_start_at.is_some_and(|start| self.mclk >= start) {
            self.dma_start_at = None;
            self.dma_run = true;
        }

        let active = self.is_active();
        let hslot = self.hslot;
        let kind = if !active {
            if width.blank_refresh(hslot) {
                Slot::Refresh
            } else {
                Slot::External
            }
        } else if self.state == State::Preparing
            && (hslot == width.line_change() || hslot == width.line_change() + 1)
        {
            // The line before the picture has no sprites to draw yet.
            Slot::External
        } else {
            width.active_slot(hslot)
        };
        // Pixels leave before the slot's access, so a CRAM write here
        // colours the second of them.
        self.output_pixels(width, hslot, active);
        if !active && self.mode_5() {
            self.blank_sprite_slot(width, hslot);
        }
        if self.vscroll_latch_slot(width, hslot, active) {
            self.latch_vscroll();
        }
        match kind {
            Slot::External => self.external_slot(),
            Slot::Fetch => self.display_slot(width, hslot),
            Slot::Refresh => {}
        }
        if kind != Slot::Refresh {
            self.request_dma_source();
        }
    }

    /// The V counter steps, the window latches and the H-int counter runs.
    fn advance_line(&mut self) {
        let v = self.v_timing();
        self.vcounter = v.next(self.vcounter);
        if self.mode_5() {
            self.window_h_latch = self.regs[0x11];
            self.window_v_latch = self.regs[0x12];
        }
        if self.state == State::Preparing {
            self.state = State::Active;
        }
        if self.state != State::Active {
            self.hint_counter = self.regs[REG_HINT];
        } else if self.hint_counter != 0 {
            self.hint_counter -= 1;
        } else {
            self.hint_pending = true;
            self.hint_counter = self.regs[REG_HINT];
        }
        if self.vcounter == 0x1FF && self.mode_5() {
            self.state = State::Preparing;
        }
    }

    // ---- External slots: the FIFO, fill, copy and reads. ----

    fn external_slot(&mut self) {
        // A fill re-uses the last FIFO entry, writing its high byte.
        if self.dma_run && self.dma_type() == DMA_FILL && self.fifo_read.is_none() {
            let last = (self.fifo_write + FIFO_SIZE - 1) % FIFO_SIZE;
            self.fifo_read = Some(last);
            let entry = &mut self.fifo[last];
            entry.ready_at = self.mclk;
            entry.address = self.address;
            entry.partial = true;
            self.advance_dma();
        }
        if let Some(read) = self
            .fifo_read
            .filter(|&i| self.fifo[i].ready_at <= self.mclk)
        {
            if !self.write_fifo_entry(read) {
                return;
            }
            let next = (read + 1) % FIFO_SIZE;
            if next == self.fifo_write {
                self.fifo_read = None;
                if self.cd & CD_DMA != 0 && self.dma_type() == DMA_FILL {
                    self.dma_run = true;
                }
            } else {
                self.fifo_read = Some(next);
            }
        } else if self.dma_run && self.dma_type() == DMA_COPY {
            // A copy alternates a read slot with a write slot.
            if self.read_fetched {
                let value = self.prefetch as u8;
                self.write_vram_byte(self.address ^ 1, value);
                self.advance_dma();
                self.read_fetched = false;
            } else {
                let source =
                    u32::from(self.regs[REG_DMA_SRC_M]) << 8 | u32::from(self.regs[REG_DMA_SRC_L]);
                self.prefetch = u16::from(self.vram[(source ^ 1) as usize & 0xFFFF]);
                self.read_fetched = true;
            }
        } else if self.cd & 1 == 0
            && !self.read_fetched
            && !self.pending
            && self.read_ready_at <= self.mclk
        {
            self.prefetch_read();
        }
    }

    /// Write the FIFO entry at `index` into its memory. Returns false when
    /// it is a VRAM word that has written only its first byte.
    fn write_fifo_entry(&mut self, index: usize) -> bool {
        let entry = self.fifo[index];
        let next_value = self.fifo[self.fifo_write].value;
        match entry.cd & 0xF {
            VRAM_WRITE => {
                if self.regs[REG_MODE_2] & (BIT_128K | BIT_MODE_5) == BIT_128K | BIT_MODE_5 {
                    self.write_vram_word(entry.address, entry.value);
                } else {
                    // A byte per slot: the low byte to the address with bit
                    // 0 flipped, then the high byte back at the address.
                    let byte = if entry.partial {
                        (entry.value >> 8) as u8
                    } else {
                        entry.value as u8
                    };
                    let address = entry.address ^ 1;
                    self.write_vram_byte(address, byte);
                    if !entry.partial {
                        self.fifo[index].address = address;
                        self.fifo[index].partial = true;
                        return false;
                    }
                }
            }
            CRAM_WRITE => {
                let value = if entry.partial {
                    next_value
                } else {
                    entry.value
                };
                self.cram[(entry.address >> 1) as usize & (CRAM_SIZE - 1)] = value & CRAM_BITS;
                self.cram_dot(value & CRAM_BITS);
            }
            VSRAM_WRITE => {
                let index = (entry.address >> 1) as usize & 63;
                if index < VSRAM_SIZE {
                    let value = if entry.partial {
                        next_value
                    } else {
                        entry.value
                    };
                    self.vsram[index] = value & VSRAM_BITS;
                }
            }
            _ => {
                // A write with a read code still takes a VRAM write's two
                // slots, and writes nothing.
                if self.cd & 6 == 0 && !entry.partial && self.regs[REG_MODE_2] & BIT_128K == 0 {
                    self.fifo[index].partial = true;
                    return false;
                }
            }
        }
        true
    }

    /// Fetch the next word for a read: VRAM takes two slots, a byte each;
    /// CRAM and VSRAM one. Bits a memory lacks come from the FIFO.
    fn prefetch_read(&mut self) {
        let next_value = self.fifo[self.fifo_write].value;
        match self.cd & 0xF {
            VRAM_READ => {
                let base = (self.address & 0xFFFE) as usize;
                if self.read_half {
                    self.prefetch |= u16::from(self.vram[base | 1]);
                    self.read_fetched = true;
                    self.read_half = false;
                    self.increment_address();
                } else {
                    self.prefetch = u16::from(self.vram[base]) << 8;
                    self.read_half = true;
                }
            }
            VRAM_READ_8 => {
                let byte = self.vram[(self.address ^ 1) as usize & 0xFFFF];
                self.prefetch = u16::from(byte) | (next_value & 0xFF00);
                self.read_fetched = true;
                self.increment_address();
            }
            CRAM_READ => {
                let colour = self.cram[(self.address >> 1) as usize & (CRAM_SIZE - 1)];
                self.prefetch = colour | (next_value & !CRAM_BITS);
                self.read_fetched = true;
                self.increment_address();
            }
            VSRAM_READ => {
                let mut index = (self.address >> 1) as usize & 63;
                if index >= VSRAM_SIZE {
                    index = 0;
                }
                self.prefetch = self.vsram[index] | (next_value & 0xF800);
                self.read_fetched = true;
                self.increment_address();
            }
            _ => {}
        }
    }

    fn increment_address(&mut self) {
        self.address = self.address.wrapping_add(u32::from(self.regs[REG_AUTOINC]));
    }

    fn write_vram_byte(&mut self, address: u32, value: u8) {
        let address = address as usize & 0xFFFF;
        self.update_sat_cache(address as u32, value);
        self.vram[address] = value;
    }

    /// A word write in 128 KB mode, on a machine with 64 KB fitted: the
    /// address lines are wired for the 128 KB layout.
    fn write_vram_word(&mut self, address: u32, value: u16) {
        let mapped = ((address & 0x3FC) | (address >> 1 & 0xFC01) | (address >> 9 & 2)) ^ 1;
        self.update_sat_cache(address & !1, (value >> 8) as u8);
        self.update_sat_cache(address | 1, value as u8);
        self.vram[mapped as usize & 0xFFFF] = value as u8;
    }

    /// Keep the internal copy of the sprite table's first two words in step
    /// with VRAM writes inside the table.
    fn update_sat_cache(&mut self, address: u32, value: u8) {
        if !self.mode_5() || address & 4 != 0 {
            return;
        }
        let mut base = u32::from(self.regs[REG_SAT]) << 9;
        base &= 0xFFFF;
        if self.width() == Width::H40 {
            base &= 0x1FC00;
        }
        if address >= base && address < base + (SAT_CACHE_SIZE as u32) * 2 {
            let offset = address - base;
            let index = (offset & 3) | (offset >> 1 & 0x1FC);
            self.sat_cache[index as usize] = value;
        }
    }

    // ---- DMA. ----

    fn advance_dma(&mut self) {
        let src_l = self.regs[REG_DMA_SRC_L].wrapping_add(1);
        self.regs[REG_DMA_SRC_L] = src_l;
        if src_l == 0 {
            self.regs[REG_DMA_SRC_M] = self.regs[REG_DMA_SRC_M].wrapping_add(1);
        }
        self.increment_address();
        let len = (u16::from(self.regs[REG_DMA_LEN_H]) << 8 | u16::from(self.regs[REG_DMA_LEN_L]))
            .wrapping_sub(1);
        self.regs[REG_DMA_LEN_H] = (len >> 8) as u8;
        self.regs[REG_DMA_LEN_L] = len as u8;
        if len == 0 {
            self.dma_run = false;
            self.bus_request = false;
            self.cd &= 0xF;
        }
    }

    /// A 68000-to-VDP DMA asks for its next source word whenever the FIFO
    /// has room.
    fn request_dma_source(&mut self) {
        if !self.dma_run || self.regs[REG_DMA_SRC_H] & 0x80 != 0 || self.dma_read.is_some() {
            return;
        }
        if self.fifo_read == Some(self.fifo_write) {
            return;
        }
        let word = u32::from(self.regs[REG_DMA_SRC_H] & 0x7F) << 16
            | u32::from(self.regs[REG_DMA_SRC_M]) << 8
            | u32::from(self.regs[REG_DMA_SRC_L]);
        self.dma_read = Some(word << 1);
    }

    /// The 68000 bus address of the word a DMA wants read, if any. The
    /// machine reads it and passes it to [`Vdp::dma_word`].
    #[must_use]
    pub fn dma_read(&self) -> Option<u32> {
        self.dma_read
    }

    /// The word read for [`Vdp::dma_read`].
    pub fn dma_word(&mut self, value: u16) {
        if self.dma_read.take().is_none() {
            return;
        }
        let write = self.fifo_write;
        self.fifo[write] = FifoEntry {
            ready_at: self.mclk + FIFO_LATENCY * self.slot_len(),
            address: self.address,
            value,
            cd: self.cd,
            partial: false,
        };
        if self.fifo_read.is_none() {
            self.fifo_read = Some(write);
        }
        self.fifo_write = (write + 1) % FIFO_SIZE;
        self.advance_dma();
    }

    /// The VDP wants the 68000's bus for a DMA.
    #[must_use]
    pub fn bus_request(&self) -> bool {
        self.bus_request
    }

    // ---- The ports. ----

    /// Write the control port: a register write, or one half of a command.
    pub fn write_control(&mut self, value: u16) {
        if self.pending {
            self.address_latch = u32::from(value & 3) << 14;
            self.address = (self.address & 0x3FFF) | self.address_latch;
            // CD5 changes only with DMA enabled.
            let high = ((value >> 2) as u8) & !3;
            self.cd = if self.regs[REG_MODE_2] & BIT_DMA_EN != 0 {
                (self.cd & 0x3) | high
            } else {
                (self.cd & 0x23) | (high & !0x23)
            };
            self.pending = false;
            self.read_fetched = false;
            self.read_half = false;
            if self.cd & 1 == 0 {
                self.read_ready_at = self.mclk + READ_LATENCY * self.slot_len();
            }
            if self.cd & CD_DMA != 0 && self.dma_type() != DMA_FILL {
                if self.regs[REG_DMA_SRC_H] & 0x80 == 0 {
                    // The bus is taken at once; reading starts three slots
                    // later.
                    self.bus_request = true;
                    self.dma_start_at = Some(self.mclk + DMA_START_SLOTS * self.slot_len());
                } else {
                    self.dma_run = true;
                }
            }
        } else {
            self.address = self.address_latch | u32::from(value & 0x3FFF);
            self.cd = (self.cd & 0x3C) | (value >> 14) as u8;
            if value & 0xC000 == 0x8000 {
                self.write_register(((value >> 8) & 0x1F) as usize, value as u8);
            } else if self.mode_5() {
                self.pending = true;
            } else {
                self.read_fetched = false;
                self.read_half = false;
            }
        }
    }

    fn write_register(&mut self, reg: usize, value: u8) {
        let limit = if self.mode_5() { 24 } else { 11 };
        if reg >= limit {
            return;
        }
        if reg == REG_MODE_1 && value & BIT_HV_LATCH != 0 && self.regs[reg] & BIT_HV_LATCH == 0 {
            self.hv_latch = self.hv_counter_live();
        }
        self.regs[reg] = if reg == REG_BG_COLOUR {
            value & 0x3F
        } else {
            value
        };
        if reg == REG_MODE_4 && value & BIT_INTERLACE == 0 {
            self.even_field = false;
        }
    }

    /// Write the data port. Returns `None` while the FIFO is full: the CPU
    /// waits and tries again.
    pub fn write_data(&mut self, value: u16) -> Option<()> {
        if self.pending {
            self.pending = false;
            self.read_fetched = false;
            self.read_half = false;
        }
        if self.fifo_read == Some(self.fifo_write) {
            return None;
        }
        if self.cd & CD_DMA != 0 && self.dma_type() == DMA_FILL {
            self.dma_run = false;
        }
        let write = self.fifo_write;
        self.fifo[write] = FifoEntry {
            ready_at: self.mclk + FIFO_LATENCY * self.slot_len(),
            address: self.address,
            value,
            cd: self.cd,
            partial: false,
        };
        if self.fifo_read.is_none() {
            self.fifo_read = Some(write);
        }
        self.fifo_write = (write + 1) % FIFO_SIZE;
        self.increment_address();
        Some(())
    }

    /// Read the data port. Returns `None` until the prefetch has the word:
    /// the CPU waits and tries again.
    pub fn read_data(&mut self) -> Option<u16> {
        if self.pending {
            self.pending = false;
            self.read_fetched = false;
            self.read_half = false;
        }
        if self.cd & 1 != 0 || !matches!(self.cd, VRAM_READ | VSRAM_READ | CRAM_READ | VRAM_READ_8)
        {
            // A read with a write code locks the real 68000 up; it gets the
            // stale prefetch here.
            return Some(self.prefetch);
        }
        if !self.read_fetched {
            return None;
        }
        self.read_fetched = false;
        self.read_ready_at = self.mclk + (READ_LATENCY - 1) * self.slot_len();
        Some(self.prefetch)
    }

    /// Read the control port: the status register, which clears the pending
    /// command half and the sprite flags. Bits 15-10 are the 68000's open
    /// bus, which the machine supplies; they read as zero here.
    pub fn read_control(&mut self) -> u16 {
        let value = self.status();
        self.pending = false;
        self.sprite_overflow = false;
        self.sprite_collision = false;
        value
    }

    /// The status register, without the side effects of reading it.
    #[must_use]
    pub fn status(&self) -> u16 {
        let mut value = 0;
        if self.fifo_read.is_none() {
            value |= 0x200;
        }
        if self.fifo_read == Some(self.fifo_write) {
            value |= 0x100;
        }
        if self.vint_pending {
            value |= 0x80;
        }
        if self.sprite_overflow {
            value |= 0x40;
        }
        if self.sprite_collision {
            value |= 0x20;
        }
        if self.regs[REG_MODE_4] & BIT_INTERLACE != 0 && !self.even_field {
            value |= 0x10;
        }
        if !self.is_active() {
            value |= 0x8;
        }
        if self.width().hblank(self.hslot) {
            value |= 0x4;
        }
        if self.cd & CD_DMA != 0 {
            value |= 0x2;
        }
        if self.pal_machine {
            value |= 0x1;
        }
        value
    }

    /// The V counter as the CPU sees it, in the high byte. In interlace the
    /// low bit carries bit 8.
    fn external_vcounter(&self) -> u16 {
        let mut line = self.vcounter;
        if self.regs[REG_MODE_4] & BIT_INTERLACE != 0 {
            if self.regs[REG_MODE_4] & BIT_DOUBLE_RES != 0 {
                line <<= 1;
            } else {
                line &= 0x1FE;
            }
            if line & 0x100 != 0 {
                line |= 1;
            }
        }
        (line & 0xFF) << 8
    }

    fn hv_counter_live(&self) -> u16 {
        self.external_vcounter() | u16::from(self.hslot)
    }

    /// Read the HV counter: V in the high byte, H in the low; frozen while
    /// register 0's latch bit is set.
    #[must_use]
    pub fn read_hv(&self) -> u16 {
        if self.mode_5() && self.regs[REG_MODE_1] & BIT_HV_LATCH != 0 {
            self.hv_latch
        } else {
            self.hv_counter_live()
        }
    }

    // ---- Interrupts. ----

    /// The 68000 interrupt level the VDP drives: 6 for V-int, 4 for H-int,
    /// 2 for the external interrupt, 0 for none.
    #[must_use]
    pub fn interrupt_level(&self) -> u8 {
        if self.vint_pending && self.regs[REG_MODE_2] & BIT_VINT_EN != 0 {
            6
        } else if self.hint_pending && self.regs[REG_MODE_1] & BIT_HINT_EN != 0 {
            4
        } else if self.ext_pending && self.regs[REG_MODE_3] & BIT_EXT_EN != 0 {
            2
        } else {
            0
        }
    }

    /// The 68000 acknowledges an interrupt. The VDP clears whichever it is
    /// asserting now, not the one the CPU started to take.
    pub fn acknowledge_interrupt(&mut self) {
        if self.vint_pending && self.regs[REG_MODE_2] & BIT_VINT_EN != 0 {
            self.vint_pending = false;
        } else if self.hint_pending && self.regs[REG_MODE_1] & BIT_HINT_EN != 0 {
            self.hint_pending = false;
        } else if self.ext_pending && self.regs[REG_MODE_3] & BIT_EXT_EN != 0 {
            self.ext_pending = false;
        }
    }

    /// The external interrupt line (/HL, from a controller's TH pin) has
    /// gone active. With the latch bit set it also freezes the HV counter.
    pub fn external_interrupt(&mut self) {
        self.ext_pending = true;
        if self.regs[REG_MODE_1] & BIT_HV_LATCH != 0 {
            self.hv_latch = self.hv_counter_live();
        }
    }

    // ---- Inspection. ----

    /// The H counter: the current slot.
    #[must_use]
    pub fn hslot(&self) -> u8 {
        self.hslot
    }

    /// The full nine-bit V counter.
    #[must_use]
    pub fn vcounter(&self) -> u16 {
        self.vcounter
    }

    /// Where the frame is.
    #[must_use]
    pub fn state(&self) -> State {
        self.state
    }

    /// Master clocks since power-on.
    #[must_use]
    pub fn mclk(&self) -> u64 {
        self.mclk
    }

    /// A register's value.
    #[must_use]
    pub fn register(&self, reg: usize) -> u8 {
        self.regs.get(reg).copied().unwrap_or(0)
    }

    /// VRAM.
    #[must_use]
    pub fn vram(&self) -> &[u8] {
        &self.vram
    }

    /// CRAM: 64 colours, `0000 BBB0 GGG0 RRR0`.
    #[must_use]
    pub fn cram(&self) -> &[u16; CRAM_SIZE] {
        &self.cram
    }

    /// VSRAM: 40 eleven-bit vertical scroll values.
    #[must_use]
    pub fn vsram(&self) -> &[u16; VSRAM_SIZE] {
        &self.vsram
    }

    /// A DMA is moving data.
    #[must_use]
    pub fn dma_running(&self) -> bool {
        self.dma_run
    }
}

#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod sprite_tests;
#[cfg(test)]
mod tests;
