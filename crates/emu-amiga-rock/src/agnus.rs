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
        }
    }

    pub fn start_blitter(&mut self, val: u16) {
        self.bltsize = val;
        // Instant stub: just don't stay busy
        self.blitter_busy = false;
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
                // Bitplane DMA
                let num_bpl = self.num_bitplanes();
                if self.dma_enabled(0x0100) && num_bpl > 0 && self.hpos >= self.ddfstrt && self.hpos <= self.ddfstop {
                    let pos_in_group = (self.hpos - self.ddfstrt) % 8;
                    if pos_in_group < u16::from(num_bpl) {
                        return SlotOwner::Bitplane(pos_in_group as u8);
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
