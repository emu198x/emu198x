//! The line: its slots, their lengths, and what each one does.
//!
//! The VDP reads and writes VRAM in slots, one access per slot. A line has
//! 210 slots in H40 and 171 in H32, and lasts 3420 master clocks in both:
//! H40 slots are 16 clocks long, except the 17 around HSYNC, which run 18 to
//! 20; H32 slots are all 20. The H counter the CPU reads is the slot number,
//! and it skips: H40 counts `$00-$B6` then `$E5-$FF`, H32 `$00-$93` then
//! `$E9-$FF`. (Some documents give H40's second run as `$E4-$FF`, which
//! would make 211 slots; BlastEm's test-ROM-checked counter and the
//! 3420-clock line both say `$E5`.)
//!
//! On an active line most slots fetch for the display. The rest are
//! *external* slots, the only ones in which the FIFO drains, a DMA moves a
//! byte or the CPU's reads are served, and *refresh* slots, which do
//! nothing the CPU can see. On a blank line, or with the display disabled,
//! every slot but the refresh slots is external. That is why DMA is fast in
//! V-blank and slow during the picture.
//!
//! The slot positions are hardware facts measured with a logic analyser and
//! test ROMs by Nemesis and by Mike Pavone for BlastEm (`vdp.c`), and agree
//! with the external-slot counts behind Sega's DMA capacity table: 18 a
//! line in H40 and 16 in H32 during the picture.

/// Master clocks in a line, in either width.
pub const MCLKS_PER_LINE: u32 = 3420;
/// Master clocks in an H40 slot outside HSYNC.
pub const MCLKS_SLOT_H40: u32 = 16;
/// Master clocks in every H32 slot.
pub const MCLKS_SLOT_H32: u32 = 20;

/// The first HSYNC slot in H40, and the lengths of the 17 HSYNC slots.
const HSYNC_SLOT_H40: u8 = 230;
const HSYNC_CYCLES_H40: [u32; 17] = [
    19, 20, 20, 20, 18, 20, 20, 20, 18, 20, 20, 20, 18, 20, 20, 20, 19,
];

/// The slot at which the V counter steps to the next line.
pub const LINE_CHANGE_H40: u8 = 165;
pub const LINE_CHANGE_H32: u8 = 133;

/// The slot on which V-int is raised, on the first blank line.
pub const VINT_SLOT: u8 = 0;

/// The last slot of the H-blank flag's off period: the flag reads set for
/// slots above these.
pub const HBLANK_START_H40: u8 = 178;
pub const HBLANK_START_H32: u8 = 233;

/// What the VDP does in one slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// The CPU's slot: drain the FIFO, step a fill or copy, or fetch a read.
    External,
    /// DRAM refresh. Not even a 68K DMA source read happens.
    Refresh,
    /// A display fetch: name table, pattern, sprite or scroll data.
    Fetch,
}

/// Which line schedule is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    H32,
    H40,
}

impl Width {
    /// The H counter value after `hslot`.
    #[must_use]
    pub const fn next_hslot(self, hslot: u8) -> u8 {
        match (self, hslot) {
            (Self::H40, 182) => 229,
            (Self::H32, 147) => 233,
            _ => hslot.wrapping_add(1),
        }
    }

    /// Master clocks in slot `hslot`.
    #[must_use]
    pub const fn slot_mclks(self, hslot: u8) -> u32 {
        match self {
            Self::H32 => MCLKS_SLOT_H32,
            Self::H40 => {
                if hslot >= HSYNC_SLOT_H40 && hslot < HSYNC_SLOT_H40 + 17 {
                    HSYNC_CYCLES_H40[(hslot - HSYNC_SLOT_H40) as usize]
                } else {
                    MCLKS_SLOT_H40
                }
            }
        }
    }

    /// The slot at which the line changes.
    #[must_use]
    pub const fn line_change(self) -> u8 {
        match self {
            Self::H32 => LINE_CHANGE_H32,
            Self::H40 => LINE_CHANGE_H40,
        }
    }

    /// Whether slot `hslot` is a refresh slot on a blank line.
    #[must_use]
    pub const fn blank_refresh(self, hslot: u8) -> bool {
        match self {
            Self::H40 => matches!(hslot, 250 | 26 | 59 | 90 | 122 | 154),
            Self::H32 => matches!(hslot, 243 | 19 | 51 | 83 | 115),
        }
    }

    /// What slot `hslot` does on an active line.
    #[must_use]
    pub const fn active_slot(self, hslot: u8) -> Slot {
        let last_column_slot = match self {
            Self::H40 => 160,
            Self::H32 => 128,
        };
        // The background columns: eight slots per two-cell column, starting
        // at slot 1. The second slot of each is external, except every
        // fourth column's, which is refresh.
        if hslot >= 1 && hslot <= last_column_slot {
            let offset = (hslot - 1) % 8;
            let block = (hslot - 1) / 8;
            if offset == 1 {
                return if block % 4 == 3 {
                    Slot::Refresh
                } else {
                    Slot::External
                };
            }
            return Slot::Fetch;
        }
        match (self, hslot) {
            // The two external slots after the last column, and the one in
            // the sprite fetch run.
            (Self::H40, 161 | 162 | 232) | (Self::H32, 129 | 130 | 145 | 243) => Slot::External,
            _ => Slot::Fetch,
        }
    }

    /// The H-blank status flag at slot `hslot`.
    #[must_use]
    pub const fn hblank(self, hslot: u8) -> bool {
        match self {
            Self::H40 => hslot > HBLANK_START_H40,
            Self::H32 => hslot > HBLANK_START_H32,
        }
    }

    /// The number of slots in a line.
    #[must_use]
    pub const fn slots_per_line(self) -> u32 {
        match self {
            Self::H40 => 210,
            Self::H32 => 171,
        }
    }
}

/// Where the V counter jumps backwards, and the first blank line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VTiming {
    /// The counter value after which it jumps.
    pub jump_from: u16,
    /// The value it jumps to.
    pub jump_to: u16,
    /// The first line of V-blank: 224 in V28, 240 in V30.
    pub blank_start: u16,
}

impl VTiming {
    /// The timing for a PAL or NTSC machine in V28 or V30 mode.
    #[must_use]
    pub const fn new(pal_machine: bool, v30: bool) -> Self {
        match (pal_machine, v30) {
            (false, false) => Self {
                jump_from: 0xEA,
                jump_to: 0x1E5,
                blank_start: 224,
            },
            // V30 on an NTSC machine: the picture never ends in time and the
            // counter wraps through the bottom of a 262-line frame.
            (false, true) => Self {
                jump_from: 0xFF,
                jump_to: 0x1FA,
                blank_start: 0xF0,
            },
            (true, false) => Self {
                jump_from: 0x102,
                jump_to: 0x1CA,
                blank_start: 224,
            },
            (true, true) => Self {
                jump_from: 0x10A,
                jump_to: 0x1D2,
                blank_start: 240,
            },
        }
    }

    /// The V counter after `vcounter`.
    #[must_use]
    pub const fn next(self, vcounter: u16) -> u16 {
        if vcounter == self.jump_from {
            self.jump_to
        } else {
            (vcounter + 1) & 0x1FF
        }
    }

    /// Lines in a frame.
    #[must_use]
    pub const fn lines(self) -> u32 {
        (self.jump_from as u32 + 1) + (0x200 - self.jump_to as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(width: Width) -> Vec<u8> {
        let mut slots = vec![0u8];
        let mut hslot = width.next_hslot(0);
        while hslot != 0 {
            slots.push(hslot);
            hslot = width.next_hslot(hslot);
        }
        slots
    }

    #[test]
    fn both_widths_take_3420_master_clocks_a_line() {
        for width in [Width::H32, Width::H40] {
            let slots = line(width);
            assert_eq!(slots.len() as u32, width.slots_per_line());
            let mclks: u32 = slots.iter().map(|&s| width.slot_mclks(s)).sum();
            assert_eq!(mclks, MCLKS_PER_LINE, "{width:?}");
        }
    }

    #[test]
    fn the_h_counter_skips_where_the_hardware_does() {
        let h40 = line(Width::H40);
        assert_eq!(
            h40.iter().position(|&s| s == 0xB6).map(|i| h40[i + 1]),
            Some(0xE5)
        );
        let h32 = line(Width::H32);
        assert_eq!(
            h32.iter().position(|&s| s == 0x93).map(|i| h32[i + 1]),
            Some(0xE9)
        );
    }

    #[test]
    fn active_lines_have_18_and_16_external_slots() {
        for (width, external, refresh) in [(Width::H40, 18, 5), (Width::H32, 16, 4)] {
            let slots = line(width);
            let count = |kind| {
                slots
                    .iter()
                    .filter(|&&s| width.active_slot(s) == kind)
                    .count()
            };
            assert_eq!(count(Slot::External), external, "{width:?} external");
            assert_eq!(count(Slot::Refresh), refresh, "{width:?} refresh");
        }
    }

    #[test]
    fn blank_lines_are_external_but_for_refresh() {
        for (width, external) in [(Width::H40, 204), (Width::H32, 166)] {
            let slots = line(width);
            let count = slots.iter().filter(|&&s| !width.blank_refresh(s)).count();
            assert_eq!(count, external, "{width:?}");
        }
    }

    #[test]
    fn frames_have_262_and_313_lines() {
        assert_eq!(VTiming::new(false, false).lines(), 262);
        assert_eq!(VTiming::new(true, false).lines(), 313);
        assert_eq!(VTiming::new(true, true).lines(), 313);
        assert_eq!(VTiming::new(false, true).lines(), 262);
    }
}
