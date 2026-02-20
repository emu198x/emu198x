//! Agnus - Beam counter and DMA slot allocation.

pub const PAL_CCKS_PER_LINE: u16 = 227;
pub const PAL_LINES_PER_FRAME: u16 = 312;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotOwner {
    Cpu,
    Refresh,
    Disk,
    Audio(u8),
    Sprite(u8),
    Bitplane(u8),
    Copper,
}

/// Maps ddfseq position (0-7) within an 8-CCK group to bitplane index.
/// From Minimig Verilog: plane = {~ddfseq[0], ~ddfseq[1], ~ddfseq[2]}.
/// None = free slot (available for copper/CPU).
const LOWRES_DDF_TO_PLANE: [Option<u8>; 8] = [
    None,     // 0: free
    Some(3),  // 1: BPL4
    Some(5),  // 2: BPL6
    Some(1),  // 3: BPL2
    None,     // 4: free
    Some(2),  // 5: BPL3
    Some(4),  // 6: BPL5
    Some(0),  // 7: BPL1 (triggers shift register load)
];

pub struct Agnus {
    pub vpos: u16,
    pub hpos: u16, // in CCKs
    
    // DMA Registers
    pub dmacon: u16,
    pub bplcon0: u16,
    pub bpl_pt: [u32; 6],
    pub ddfstrt: u16,
    pub ddfstop: u16,

    // Blitter Registers
    pub bltcon0: u16,
    pub bltcon1: u16,
    pub bltsize: u16,
    pub blitter_busy: bool,
    pub blt_apt: u32,
    pub blt_bpt: u32,
    pub blt_cpt: u32,
    pub blt_dpt: u32,
    pub blt_amod: i16,
    pub blt_bmod: i16,
    pub blt_cmod: i16,
    pub blt_dmod: i16,
    pub blt_adat: u16,
    pub blt_bdat: u16,
    pub blt_cdat: u16,
    pub blt_afwm: u16,
    pub blt_alwm: u16,

    // Display window
    pub diwstrt: u16,
    pub diwstop: u16,
    pub bpl1mod: i16,
    pub bpl2mod: i16,

    // Sprite pointers
    pub spr_pt: [u32; 8],
}

impl Agnus {
    pub fn new() -> Self {
        Self {
            vpos: 0,
            hpos: 0,
            dmacon: 0,
            bplcon0: 0,
            bpl_pt: [0; 6],
            ddfstrt: 0,
            ddfstop: 0,
            bltcon0: 0,
            bltcon1: 0,
            bltsize: 0,
            blitter_busy: false,
            blt_apt: 0,
            blt_bpt: 0,
            blt_cpt: 0,
            blt_dpt: 0,
            blt_amod: 0,
            blt_bmod: 0,
            blt_cmod: 0,
            blt_dmod: 0,
            blt_adat: 0,
            blt_bdat: 0,
            blt_cdat: 0,
            blt_afwm: 0xFFFF,
            blt_alwm: 0xFFFF,
            diwstrt: 0,
            diwstop: 0,
            bpl1mod: 0,
            bpl2mod: 0,
            spr_pt: [0; 8],
        }
    }

    pub fn num_bitplanes(&self) -> u8 {
        let bpl_bits = (self.bplcon0 >> 12) & 0x07;
        if bpl_bits > 6 { 6 } else { bpl_bits as u8 }
    }

    pub fn dma_enabled(&self, bit: u16) -> bool {
        (self.dmacon & 0x0200) != 0 && (self.dmacon & bit) != 0
    }

    /// Tick one CCK (8 crystal ticks).
    pub fn tick_cck(&mut self) {
        self.hpos += 1;
        if self.hpos >= PAL_CCKS_PER_LINE {
            self.hpos = 0;
            self.vpos += 1;
            if self.vpos >= PAL_LINES_PER_FRAME {
                self.vpos = 0;
            }
        }
    }

    /// Determine who owns the current CCK slot.
    pub fn current_slot(&self) -> SlotOwner {
        match self.hpos {
            // Fixed slots
            0x01..=0x03 | 0x1B => SlotOwner::Refresh,
            0x04..=0x06 => {
                if self.dma_enabled(0x0010) { SlotOwner::Disk } else { SlotOwner::Cpu }
            }
            0x07 => if self.dma_enabled(0x0001) { SlotOwner::Audio(0) } else { SlotOwner::Cpu },
            0x08 => if self.dma_enabled(0x0002) { SlotOwner::Audio(1) } else { SlotOwner::Cpu },
            0x09 => if self.dma_enabled(0x0004) { SlotOwner::Audio(2) } else { SlotOwner::Cpu },
            0x0A => if self.dma_enabled(0x0008) { SlotOwner::Audio(3) } else { SlotOwner::Cpu },
            0x0B..=0x1A => {
                if self.dma_enabled(0x0020) {
                    SlotOwner::Sprite(((self.hpos - 0x0B) / 2) as u8)
                } else {
                    SlotOwner::Cpu
                }
            }
            
            // Variable slots (Bitplane, Copper, CPU)
            0x1C..=0xE2 => {
                // Bitplane DMA: fetch window runs from DDFSTRT to DDFSTOP+7.
                // Within each 8-CCK group, planes are fetched in the Minimig
                // interleaved order (LOWRES_DDF_TO_PLANE), not sequentially.
                let num_bpl = self.num_bitplanes();
                if self.dma_enabled(0x0100) && num_bpl > 0
                    && self.hpos >= self.ddfstrt
                    && self.hpos <= self.ddfstop + 7
                {
                    let pos_in_group = ((self.hpos - self.ddfstrt) % 8) as usize;
                    if let Some(plane) = LOWRES_DDF_TO_PLANE[pos_in_group] {
                        if plane < num_bpl {
                            return SlotOwner::Bitplane(plane);
                        }
                    }
                }

                // Copper
                if self.dma_enabled(0x0080) && (self.hpos % 2 == 0) {
                    return SlotOwner::Copper;
                }

                SlotOwner::Cpu
            }

            _ => SlotOwner::Cpu,
        }
    }
}
