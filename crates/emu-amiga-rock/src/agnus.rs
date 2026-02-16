//! Agnus - Beam counter and DMA slot allocation.

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
        if self.hpos >= 227 { // Simple PAL-like limit
            self.hpos = 0;
            self.vpos += 1;
            if self.vpos >= 312 {
                self.vpos = 0;
            }
        }
    }

    /// Determine who owns the current CCK slot.
    pub fn current_slot(&self) -> SlotOwner {
        // Very minimal stub: Refresh in some slots, CPU elsewhere.
        // This is where we'll put the exact OCS/ECS slot map.
        match self.hpos {
            0..=3 => SlotOwner::Refresh,
            _ => SlotOwner::Cpu,
        }
    }
}
