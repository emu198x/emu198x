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
}

impl Agnus {
    pub fn new() -> Self {
        Self { vpos: 0, hpos: 0 }
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
    /// Based on the OCS DMA slot map.
    pub fn current_slot(&self) -> SlotOwner {
        match self.hpos {
            // Fixed slots
            0x01..=0x03 | 0x1B => SlotOwner::Refresh,
            0x04..=0x06 => SlotOwner::Disk,
            0x07 => SlotOwner::Audio(0),
            0x08 => SlotOwner::Audio(1),
            0x09 => SlotOwner::Audio(2),
            0x0A => SlotOwner::Audio(3),
            0x0B..=0x1A => SlotOwner::Sprite(((self.hpos - 0x0B) / 2) as u8),
            
            // Variable slots (Bitplane, Copper, CPU)
            // For now, we assume DMA is mostly off so CPU gets the bus.
            _ => SlotOwner::Cpu,
        }
    }
}
