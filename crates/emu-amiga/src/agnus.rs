//! Agnus: beam counter and DMA slot allocator.
//!
//! Agnus tracks the beam position (VPOS, HPOS) and allocates each colour
//! clock (CCK) slot to either a DMA channel or the CPU.
//!
//! PAL non-interlaced:
//! - 312 lines per frame (VPOS 0-311)
//! - 227 CCKs per line (HPOS 0-226)

#![allow(clippy::cast_possible_truncation)]

use crate::custom_regs;

/// PAL lines per frame.
pub const LINES_PER_FRAME: u16 = 312;

/// Colour clocks per line.
pub const CCKS_PER_LINE: u16 = 227;

/// Which DMA channel owns a CCK slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotOwner {
    Cpu,
    Refresh,
    Disk,
    Audio(u8),
    Sprite(u8),
    Bitplane,
    Copper,
}

/// Agnus beam counter and DMA controller.
pub struct Agnus {
    /// Vertical beam position (0-311).
    pub vpos: u16,
    /// Horizontal beam position in CCK units (0-226).
    pub hpos: u16,
    /// DMA control register.
    pub dmacon: u16,
    /// Display window start.
    pub diwstrt: u16,
    /// Display window stop.
    pub diwstop: u16,
    /// Data fetch start (CCK position).
    pub ddfstrt: u16,
    /// Data fetch stop (CCK position).
    pub ddfstop: u16,
    /// Bitplane pointers (6 planes, high/low combined).
    pub bpl_pt: [u32; 6],
    /// Bitplane modulos.
    pub bpl1mod: u16,
    pub bpl2mod: u16,
    /// Long frame bit (LOF).
    pub lof: bool,
    /// Number of active bitplanes (from BPLCON0, synced by bus layer).
    num_bpl: u8,
}

impl Agnus {
    #[must_use]
    pub fn new() -> Self {
        Self {
            vpos: 0,
            hpos: 0,
            dmacon: 0,
            diwstrt: 0x2C81,
            diwstop: 0x2CC1,
            ddfstrt: 0x0038,
            ddfstop: 0x00D0,
            bpl_pt: [0; 6],
            bpl1mod: 0,
            bpl2mod: 0,
            lof: true,
            num_bpl: 0,
        }
    }

    /// Advance beam by one CCK. Returns the slot owner for this position.
    pub fn tick_cck(&mut self) -> SlotOwner {
        let owner = self.allocate_slot();

        self.hpos += 1;
        if self.hpos >= CCKS_PER_LINE {
            self.hpos = 0;
            self.vpos += 1;
            if self.vpos >= LINES_PER_FRAME {
                self.vpos = 0;
                self.lof = !self.lof;
            }
        }

        owner
    }

    /// Is DMA master enabled?
    #[must_use]
    pub fn dma_enabled(&self) -> bool {
        self.dmacon & custom_regs::DMAF_DMAEN != 0
    }

    /// Is a specific DMA channel enabled (master + channel bit)?
    fn channel_enabled(&self, flag: u16) -> bool {
        self.dma_enabled() && (self.dmacon & flag != 0)
    }

    /// Number of active bitplanes.
    #[must_use]
    pub fn num_bitplanes(&self) -> u8 {
        self.num_bpl
    }

    /// Set the number of active bitplanes (from BPLCON0 bits 14-12).
    pub fn set_num_bitplanes(&mut self, n: u8) {
        self.num_bpl = n.min(6);
    }

    /// Allocate the current HPOS slot.
    fn allocate_slot(&self) -> SlotOwner {
        let h = self.hpos;

        match h {
            // Refresh: always allocated
            0x01..=0x03 | 0x1B => SlotOwner::Refresh,

            // Disk: $04-$06
            0x04..=0x06 => {
                if self.channel_enabled(custom_regs::DMAF_DSKEN) {
                    SlotOwner::Disk
                } else {
                    SlotOwner::Cpu
                }
            }

            // Audio: $07-$0A
            0x07 => self.audio_or_cpu(custom_regs::DMAF_AUD0EN, 0),
            0x08 => self.audio_or_cpu(custom_regs::DMAF_AUD1EN, 1),
            0x09 => self.audio_or_cpu(custom_regs::DMAF_AUD2EN, 2),
            0x0A => self.audio_or_cpu(custom_regs::DMAF_AUD3EN, 3),

            // Sprites: $0B-$1A (2 slots each, 8 sprites)
            0x0B..=0x1A => {
                if self.channel_enabled(custom_regs::DMAF_SPREN) {
                    let sprite_num = ((h - 0x0B) / 2) as u8;
                    SlotOwner::Sprite(sprite_num)
                } else {
                    SlotOwner::Cpu
                }
            }

            // Bitplane/Copper/CPU region: $1C-$E2
            0x1C..=0xE2 => self.allocate_variable_region(h),

            // Everything else: CPU
            _ => SlotOwner::Cpu,
        }
    }

    fn audio_or_cpu(&self, flag: u16, ch: u8) -> SlotOwner {
        if self.channel_enabled(flag) {
            SlotOwner::Audio(ch)
        } else {
            SlotOwner::Cpu
        }
    }

    /// Allocate slots in the variable region where bitplane, copper, and CPU compete.
    fn allocate_variable_region(&self, h: u16) -> SlotOwner {
        let ddfstrt = self.ddfstrt & 0x00FC;
        let ddfstop = self.ddfstop & 0x00FC;

        // Bitplane DMA within data fetch window
        if self.channel_enabled(custom_regs::DMAF_BPLEN) && self.num_bpl > 0 && h >= ddfstrt && h <= ddfstop + 8 {
            let pos_in_group = h.wrapping_sub(ddfstrt) % 8;
            if pos_in_group < u16::from(self.num_bpl) {
                return SlotOwner::Bitplane;
            }
        }

        // Copper gets even CCK positions when enabled
        if self.channel_enabled(custom_regs::DMAF_COPEN) && h.is_multiple_of(2) {
            return SlotOwner::Copper;
        }

        SlotOwner::Cpu
    }

    /// Is this the start of VBlank (line 0, position 0)?
    #[allow(clippy::doc_markdown)]
    #[must_use]
    pub fn is_vblank_start(&self) -> bool {
        self.vpos == 0 && self.hpos == 0
    }

    /// Read VPOSR register.
    #[must_use]
    pub fn read_vposr(&self) -> u16 {
        let lof_bit = if self.lof { 0x8000 } else { 0 };
        let vpos_hi = (self.vpos >> 8) & 1;
        lof_bit | vpos_hi
    }

    /// Read VHPOSR register.
    #[must_use]
    pub fn read_vhposr(&self) -> u16 {
        let v = self.vpos & 0xFF;
        let h = self.hpos & 0xFF;
        (v << 8) | h
    }

    /// Write DMACON using SET/CLR logic.
    pub fn write_dmacon(&mut self, val: u16) {
        custom_regs::set_clr_write(&mut self.dmacon, val);
    }
}

impl Default for Agnus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beam_wraps_at_end_of_line() {
        let mut agnus = Agnus::new();
        agnus.hpos = CCKS_PER_LINE - 1;
        agnus.vpos = 0;
        agnus.tick_cck();
        assert_eq!(agnus.hpos, 0);
        assert_eq!(agnus.vpos, 1);
    }

    #[test]
    fn beam_wraps_at_end_of_frame() {
        let mut agnus = Agnus::new();
        agnus.hpos = CCKS_PER_LINE - 1;
        agnus.vpos = LINES_PER_FRAME - 1;
        agnus.tick_cck();
        assert_eq!(agnus.hpos, 0);
        assert_eq!(agnus.vpos, 0);
    }

    #[test]
    fn refresh_slots_always_allocated() {
        let agnus = Agnus::new();
        // HPOS 1 is always refresh, even without DMA enabled
        assert_eq!(agnus.allocate_slot_at(0x01), SlotOwner::Refresh);
        assert_eq!(agnus.allocate_slot_at(0x02), SlotOwner::Refresh);
        assert_eq!(agnus.allocate_slot_at(0x03), SlotOwner::Refresh);
        assert_eq!(agnus.allocate_slot_at(0x1B), SlotOwner::Refresh);
    }

    #[test]
    fn cpu_gets_slot_when_dma_disabled() {
        let agnus = Agnus::new(); // dmacon = 0
        assert_eq!(agnus.allocate_slot_at(0x04), SlotOwner::Cpu);
        assert_eq!(agnus.allocate_slot_at(0x07), SlotOwner::Cpu);
    }

    #[test]
    fn vposr_lof_and_vpos_hi() {
        let mut agnus = Agnus::new();
        agnus.lof = true;
        agnus.vpos = 256; // Bit 8 set
        assert_eq!(agnus.read_vposr(), 0x8001);
    }

    #[test]
    fn vhposr_encoding() {
        let mut agnus = Agnus::new();
        agnus.vpos = 0x2C;
        agnus.hpos = 0x40;
        assert_eq!(agnus.read_vhposr(), 0x2C40);
    }

    impl Agnus {
        /// Test helper: allocate slot at a specific HPOS without advancing.
        fn allocate_slot_at(&self, h: u16) -> SlotOwner {
            let copy = Self {
                vpos: self.vpos,
                hpos: h,
                dmacon: self.dmacon,
                diwstrt: self.diwstrt,
                diwstop: self.diwstop,
                ddfstrt: self.ddfstrt,
                ddfstop: self.ddfstop,
                bpl_pt: self.bpl_pt,
                bpl1mod: self.bpl1mod,
                bpl2mod: self.bpl2mod,
                lof: self.lof,
                num_bpl: self.num_bpl,
            };
            copy.allocate_slot()
        }
    }
}
