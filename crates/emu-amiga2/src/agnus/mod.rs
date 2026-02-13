//! Agnus: beam counter and DMA bus arbiter.
//!
//! Agnus tracks the beam position (VPOS, HPOS) and allocates each colour
//! clock (CCK) slot to either a DMA channel or the CPU.

pub mod beam;
pub mod dma;

use crate::config::{AgnusVariant, Region};
use crate::custom_regs;

/// PAL lines per frame.
pub const PAL_LINES_PER_FRAME: u16 = 312;
/// NTSC lines per frame.
pub const NTSC_LINES_PER_FRAME: u16 = 262;
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
    /// Chip variant (determines address range, etc.).
    variant: AgnusVariant,
    /// Video region.
    region: Region,
    /// Vertical beam position.
    pub vpos: u16,
    /// Horizontal beam position in CCK units.
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
    /// Bitplane pointers (6 planes).
    pub bpl_pt: [u32; 6],
    /// Bitplane modulos.
    pub bpl1mod: u16,
    pub bpl2mod: u16,
    /// Long frame bit (LOF).
    pub lof: bool,
    /// Number of active bitplanes (from BPLCON0).
    num_bpl: u8,
}

impl Agnus {
    #[must_use]
    pub fn new(variant: AgnusVariant, region: Region) -> Self {
        Self {
            variant,
            region,
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

    /// Lines per frame for the current region.
    #[must_use]
    pub fn lines_per_frame(&self) -> u16 {
        match self.region {
            Region::Pal => PAL_LINES_PER_FRAME,
            Region::Ntsc => NTSC_LINES_PER_FRAME,
        }
    }

    /// Advance beam by one CCK. Returns the slot owner for this position.
    pub fn tick_cck(&mut self) -> SlotOwner {
        let owner = dma::allocate_slot(self);

        self.hpos += 1;
        if self.hpos >= CCKS_PER_LINE {
            self.hpos = 0;
            self.vpos += 1;
            if self.vpos >= self.lines_per_frame() {
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
    #[must_use]
    pub fn channel_enabled(&self, flag: u16) -> bool {
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

    /// Is this the start of VBlank (line 0, position 0)?
    #[must_use]
    pub fn is_vblank_start(&self) -> bool {
        self.vpos == 0 && self.hpos == 0
    }

    /// Read VPOSR register.
    #[must_use]
    pub fn read_vposr(&self) -> u16 {
        let lof_bit = if self.lof { 0x8000 } else { 0 };
        let vpos_hi = (self.vpos >> 8) & 1;
        // Agnus ID in bits 8-14 (OCS = $00, ECS = $20, AGA = $22)
        let agnus_id: u16 = match self.variant {
            AgnusVariant::Agnus8361 | AgnusVariant::FatAgnus8371 => 0x00,
            AgnusVariant::Agnus8372 => 0x20,
            AgnusVariant::Alice => 0x22,
        };
        lof_bit | (agnus_id << 8) | vpos_hi
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

    /// Chip variant.
    #[must_use]
    pub fn variant(&self) -> AgnusVariant {
        self.variant
    }

    /// Video region.
    #[must_use]
    pub fn region(&self) -> Region {
        self.region
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agnus() -> Agnus {
        Agnus::new(AgnusVariant::Agnus8361, Region::Pal)
    }

    #[test]
    fn beam_wraps_at_end_of_line() {
        let mut agnus = make_agnus();
        agnus.hpos = CCKS_PER_LINE - 1;
        agnus.vpos = 0;
        agnus.tick_cck();
        assert_eq!(agnus.hpos, 0);
        assert_eq!(agnus.vpos, 1);
    }

    #[test]
    fn beam_wraps_at_end_of_frame() {
        let mut agnus = make_agnus();
        agnus.hpos = CCKS_PER_LINE - 1;
        agnus.vpos = PAL_LINES_PER_FRAME - 1;
        agnus.tick_cck();
        assert_eq!(agnus.hpos, 0);
        assert_eq!(agnus.vpos, 0);
    }

    #[test]
    fn vposr_lof_and_vpos_hi() {
        let mut agnus = make_agnus();
        agnus.lof = true;
        agnus.vpos = 256;
        assert_eq!(agnus.read_vposr(), 0x8001);
    }

    #[test]
    fn vhposr_encoding() {
        let mut agnus = make_agnus();
        agnus.vpos = 0x2C;
        agnus.hpos = 0x40;
        assert_eq!(agnus.read_vhposr(), 0x2C40);
    }
}
