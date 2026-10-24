//! Atari ST GLUE and MMU.
//!
//! Two of the ST's custom chips share the work of putting a picture on the
//! screen and RAM on the bus:
//!
//! - **GLUE** generates the video timing: the line and frame lengths, the sync
//!   pulses, the display-enable window, and the HBL and VBL interrupts.
//! - **MMU** owns the RAM. It decodes the memory configuration register
//!   (`$FF8001`) into DRAM row and column addresses, holds the video base and
//!   counter (`$FF8201`–`$FF8209`) and fetches one video word every four CPU
//!   cycles while the display is enabled. It also holds the DMA address
//!   counter (`$FF8609`–`$FF860D`) the floppy and hard-disk DMA chip uses.
//!
//! [`GlueMmu::tick`] is one CPU cycle (8 MHz). After each tick the host reads
//! the display-enable line ([`GlueMmu::display_enable`]), takes the video
//! fetch address if one is due ([`GlueMmu::take_fetch`]), and the HBL and VBL
//! pulses ([`GlueMmu::take_hbl`], [`GlueMmu::take_vbl`]). The timer B input
//! ([`GlueMmu::timer_b_input`]) is display enable as the MFP sees it.
//!
//! # Timing
//!
//! | Mode | Cycles per line | Lines per frame | DE cycles | DE lines |
//! |---|---|---|---|---|
//! | 50 Hz colour | 512 | 313 | 56–375 | 63–262 |
//! | 60 Hz colour | 508 | 263 | 52–371 | 34–233 |
//! | 71 Hz mono | 224 | 501 | 0–159 | 34–433 |
//!
//! The line length is chosen when a line starts, from the sync register's
//! 50 Hz bit and the resolution (high resolution is the 71 Hz monitor's). The
//! HBL pulse comes at the start of every line and the VBL pulse at cycle 64 of
//! line 0. The video counter reloads from the base three lines before the end
//! of the frame, at cycle 56. The timer B input follows display enable 24
//! cycles late, so the MFP's event counter counts the end of each displayed
//! line where hardware sees it.
//!
//! # DRAM mapping
//!
//! The configuration register gives each of the two banks a size of 128 KB,
//! 512 KB or 2 MB (bits 3–2 bank 0, bits 1–0 bank 1). The MMU always drives
//! ten multiplexed address lines: the row is CPU address bits 1–10, and the
//! column starts at bit 9, 10 or 11 for the configured size. A DRAM chip of
//! 64, 256 or 1024 kbit latches only the low 8, 9 or 10 lines of each.
//! [`GlueMmu::translate`] applies that rule, so a configuration larger than
//! the fitted chips aliases RAM exactly the way TOS's memory test expects.
//!
//! # Not modelled
//!
//! - Mid-line changes of frequency or resolution (border removal and other
//!   overscan tricks). They take effect at the next line.
//! - The exact cycle the GLUE samples the sync register for the vertical
//!   border. A change takes effect at the next frame.
//!
//! # Reference
//!
//! Atari ST documentation in `198x/reference/by-system/atari-st/`. Facts
//! cross-checked against Hatari's `video.h`, `video.c` and `stMemory.c`
//! (GPL-2, not used as a code source). The DRAM row/column rule is Christian
//! Zietz's reverse-engineering of the MMU, which Hatari also documents.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// The video timing a line runs at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frequency {
    Hz50,
    Hz60,
    Hz71,
}

impl Frequency {
    #[must_use]
    pub fn cycles_per_line(self) -> u16 {
        match self {
            Self::Hz50 => 512,
            Self::Hz60 => 508,
            Self::Hz71 => 224,
        }
    }

    #[must_use]
    pub fn lines_per_frame(self) -> u16 {
        match self {
            Self::Hz50 => 313,
            Self::Hz60 => 263,
            Self::Hz71 => 501,
        }
    }

    /// The display-enable window: first cycle, end cycle, first line, end
    /// line.
    #[must_use]
    pub fn display_window(self) -> (u16, u16, u16, u16) {
        match self {
            Self::Hz50 => (56, 376, 63, 263),
            Self::Hz60 => (52, 372, 34, 234),
            Self::Hz71 => (0, 160, 34, 434),
        }
    }
}

const VBL_CYCLE: u16 = 64;
const COUNTER_RELOAD_CYCLE: u16 = 56;
const TIMER_B_DELAY: u16 = 24;

/// A bank size in bytes for a two-bit configuration field.
fn bank_size(field: u8) -> u32 {
    match field & 3 {
        0 => 128 * 1024,
        1 => 512 * 1024,
        _ => 2048 * 1024,
    }
}

/// Address bits per row or column for a bank size.
fn bank_bits(size: u32) -> u32 {
    match size {
        0..=0x2_0000 => 8,
        0x2_0001..=0x8_0000 => 9,
        _ => 10,
    }
}

/// The ST GLUE and MMU.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlueMmu {
    /// Physical RAM fitted in each bank, in bytes: 0, 128 KB, 512 KB or 2 MB.
    physical: [u32; 2],
    config: u8,
    sync: u8,
    /// The resolution register as the GLUE sees it (2 = 71 Hz monitor).
    resolution: u8,
    video_base: u32,
    video_counter: u32,
    dma_address: u32,

    frequency: Frequency,
    line: u16,
    cycle: u16,
    display_enable: bool,
    /// Display enable over the last [`TIMER_B_DELAY`] cycles, newest in bit 0.
    de_history: u32,
    fetch: Option<u32>,
    hbl: bool,
    vbl: bool,
    frames: u64,
}

impl GlueMmu {
    /// A GLUE and MMU with `bank0` and `bank1` bytes of RAM fitted (0, 128
    /// KB, 512 KB or 2 MB each).
    #[must_use]
    pub fn new(bank0: u32, bank1: u32) -> Self {
        Self {
            physical: [bank0, bank1],
            config: 0,
            sync: 0x02,
            resolution: 0,
            video_base: 0,
            video_counter: 0,
            dma_address: 0,
            frequency: Frequency::Hz50,
            line: 0,
            cycle: 0,
            display_enable: false,
            de_history: 0,
            fetch: None,
            hbl: false,
            vbl: false,
            frames: 0,
        }
    }

    /// Bytes of RAM fitted across both banks.
    #[must_use]
    pub fn physical_ram(&self) -> usize {
        (self.physical[0] + self.physical[1]) as usize
    }

    /// The memory configuration value matching the fitted RAM, the value TOS
    /// arrives at after its memory test.
    #[must_use]
    pub fn matching_config(&self) -> u8 {
        let field = |size: u32| match size {
            0..=0x2_0000 => 0,
            0x2_0001..=0x8_0000 => 1,
            _ => 2,
        };
        field(self.physical[0]) << 2 | field(self.physical[1])
    }

    // ---- Timing ----

    /// One CPU cycle.
    pub fn tick(&mut self) {
        // A fetch is this cycle's or nobody's.
        self.fetch = None;
        if self.cycle == 0 {
            self.frequency = if self.resolution & 2 != 0 {
                Frequency::Hz71
            } else if self.sync & 0x02 != 0 {
                Frequency::Hz50
            } else {
                Frequency::Hz60
            };
            self.hbl = true;
        }
        let (de_start, de_end, first_line, end_line) = self.frequency.display_window();
        let lines = self.frequency.lines_per_frame();
        if self.line == 0 && self.cycle == VBL_CYCLE {
            self.vbl = true;
        }
        if self.line == lines - 3 && self.cycle == COUNTER_RELOAD_CYCLE {
            self.video_counter = self.video_base;
        }
        self.display_enable =
            (first_line..end_line).contains(&self.line) && (de_start..de_end).contains(&self.cycle);
        if self.display_enable && (self.cycle - de_start).is_multiple_of(4) {
            self.fetch = Some(self.video_counter);
            self.video_counter = (self.video_counter + 2) & 0x3F_FFFE;
        }
        self.de_history = self.de_history << 1 | u32::from(self.display_enable);

        self.cycle += 1;
        if self.cycle >= self.frequency.cycles_per_line() {
            self.cycle = 0;
            self.line += 1;
            if self.line >= lines {
                self.line = 0;
                self.frames += 1;
            }
        }
    }

    /// The display-enable line for the cycle just ticked.
    #[must_use]
    pub fn display_enable(&self) -> bool {
        self.display_enable
    }

    /// Display enable as the MFP's timer B input sees it.
    #[must_use]
    pub fn timer_b_input(&self) -> bool {
        self.de_history >> TIMER_B_DELAY & 1 != 0
    }

    /// The video word the MMU fetched this cycle, if any.
    pub fn take_fetch(&mut self) -> Option<u32> {
        self.fetch.take()
    }

    /// The HBL pulse, once per line.
    pub fn take_hbl(&mut self) -> bool {
        std::mem::take(&mut self.hbl)
    }

    /// The VBL pulse, once per frame.
    pub fn take_vbl(&mut self) -> bool {
        std::mem::take(&mut self.vbl)
    }

    /// The timing in force on this line.
    #[must_use]
    pub fn frequency(&self) -> Frequency {
        self.frequency
    }

    /// The line and cycle within the frame.
    #[must_use]
    pub fn position(&self) -> (u16, u16) {
        (self.line, self.cycle)
    }

    /// Frames completed since power-on.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Set the resolution as the GLUE sees it, from a `$FF8260` write.
    pub fn set_resolution(&mut self, value: u8) {
        self.resolution = value & 3;
    }

    // ---- Registers ----

    /// Read a byte of the GLUE/MMU's registers at `$FF8000 + offset`.
    /// `None` for an offset these chips do not answer.
    #[must_use]
    pub fn read(&self, offset: u16) -> Option<u8> {
        Some(match offset {
            0x001 => self.config,
            0x201 => (self.video_base >> 16) as u8,
            0x203 => (self.video_base >> 8) as u8,
            0x205 => (self.video_counter >> 16) as u8,
            0x207 => (self.video_counter >> 8) as u8,
            0x209 => self.video_counter as u8,
            0x20A => self.sync | 0xFC,
            0x609 => (self.dma_address >> 16) as u8,
            0x60B => (self.dma_address >> 8) as u8,
            0x60D => self.dma_address as u8,
            _ => return None,
        })
    }

    /// Write a byte at `$FF8000 + offset`. Returns false for an offset these
    /// chips do not answer.
    pub fn write(&mut self, offset: u16, value: u8) -> bool {
        let set = |reg: &mut u32, shift: u32, value: u8| {
            *reg = (*reg & !(0xFF << shift)) | u32::from(value) << shift;
        };
        match offset {
            0x001 => self.config = value & 0x0F,
            0x201 => set(&mut self.video_base, 16, value & 0x3F),
            0x203 => set(&mut self.video_base, 8, value),
            // The STF's counter is read-only; its writes go nowhere.
            0x205 | 0x207 | 0x209 => {}
            0x20A => self.sync = value & 0x03,
            0x609 => set(&mut self.dma_address, 16, value & 0x3F),
            0x60B => set(&mut self.dma_address, 8, value),
            0x60D => set(&mut self.dma_address, 0, value & 0xFE),
            _ => return false,
        }
        true
    }

    /// The video base address.
    #[must_use]
    pub fn video_base(&self) -> u32 {
        self.video_base
    }

    /// The memory configuration register.
    #[must_use]
    pub fn config(&self) -> u8 {
        self.config
    }

    /// Set the memory configuration register directly, as a snapshot or a
    /// warm start does.
    pub fn set_config(&mut self, config: u8) {
        self.config = config & 0x0F;
    }

    /// The DMA address counter.
    #[must_use]
    pub fn dma_address(&self) -> u32 {
        self.dma_address
    }

    /// Advance the DMA counter after a transfer of `bytes`.
    pub fn advance_dma(&mut self, bytes: u32) {
        self.dma_address = (self.dma_address + bytes) & 0x3F_FFFE;
    }

    // ---- RAM ----

    /// The physical RAM offset the MMU drives for logical address `address`,
    /// or `None` if no RAM answers there.
    #[must_use]
    pub fn translate(&self, address: u32) -> Option<usize> {
        let address = address & 0x3F_FFFF;
        let logical = [bank_size(self.config >> 2), bank_size(self.config)];
        let (bank, offset) = if address < logical[0] {
            (0, address)
        } else if address < logical[0] + logical[1] {
            (1, address - logical[0])
        } else {
            return None;
        };
        let physical = self.physical[bank];
        if physical == 0 {
            return None;
        }
        let configured = bank_bits(logical[bank]);
        let fitted = bank_bits(physical);
        let word = offset >> 1;
        let row = word & 0x3FF;
        let column = word >> configured & 0x3FF;
        let mask = (1 << fitted) - 1;
        let chip_word = (column & mask) << fitted | (row & mask);
        let base = if bank == 0 { 0 } else { self.physical[0] };
        Some((base + (chip_word << 1 | (address & 1))) as usize)
    }
}

#[cfg(test)]
mod tests;
