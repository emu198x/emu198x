//! Motorola MC6883 Synchronous Address Multiplexer state.
//!
//! The Dragon/CoCo SAM exposes write-only set/reset addresses at `$FFC0..$FFDF`.
//! Even addresses clear the selected latch and odd addresses set it. This crate
//! starts with the latches needed for Dragon ROM bring-up: VDG mode bits,
//! display offset, page select, CPU rate, memory-size bits, and type select.

use serde::{Deserialize, Serialize};

const SAM_START: u16 = 0xFFC0;
const SAM_END: u16 = 0xFFDF;

/// MC6883 SAM latch state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sam6883 {
    video_mode: u8,
    display_offset: u8,
    page_select: bool,
    cpu_rate: u8,
    memory_size: u8,
    ty: bool,
}

impl Sam6883 {
    /// Create a reset SAM.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset all latches.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Apply a write to the SAM set/reset range.
    ///
    /// Returns `true` when `addr` selected a SAM latch.
    pub fn write(&mut self, addr: u16) -> bool {
        if !(SAM_START..=SAM_END).contains(&addr) {
            return false;
        }

        let latch = ((addr - SAM_START) >> 1) as u8;
        let set = addr & 1 != 0;
        self.write_latch(latch, set);
        true
    }

    /// Return the VDG mode latch bits V0..V2.
    #[must_use]
    pub const fn video_mode(&self) -> u8 {
        self.video_mode
    }

    /// Return the display-offset latch bits F0..F6.
    #[must_use]
    pub const fn display_offset(&self) -> u8 {
        self.display_offset
    }

    /// Return the byte address where the VDG starts reading display memory.
    ///
    /// The F latches map to VDG address bits in 512-byte units. For example,
    /// F1 set and all other F bits clear selects `$0400`, the Dragon BASIC text
    /// screen used by the ROM after reset.
    #[must_use]
    pub const fn display_base(&self) -> u16 {
        (self.display_offset as u16) << 9
    }

    /// Set the display-offset latches from a byte address.
    pub fn set_display_base(&mut self, base: u16) {
        self.display_offset = (base >> 9) as u8;
    }

    /// Return the P1 page-select latch.
    #[must_use]
    pub const fn page_select(&self) -> bool {
        self.page_select
    }

    /// Return CPU-rate latch bits R0..R1.
    #[must_use]
    pub const fn cpu_rate(&self) -> u8 {
        self.cpu_rate
    }

    /// Return memory-size latch bits M0..M1.
    #[must_use]
    pub const fn memory_size(&self) -> u8 {
        self.memory_size
    }

    /// Return the TY type-select latch.
    #[must_use]
    pub const fn ty(&self) -> bool {
        self.ty
    }

    fn write_latch(&mut self, latch: u8, set: bool) {
        match latch {
            0..=2 => set_bit(&mut self.video_mode, latch, set),
            3..=9 => set_bit(&mut self.display_offset, latch - 3, set),
            10 => self.page_select = set,
            11..=12 => set_bit(&mut self.cpu_rate, latch - 11, set),
            13..=14 => set_bit(&mut self.memory_size, latch - 13, set),
            15 => self.ty = set,
            _ => unreachable!("SAM write range only decodes 16 latches"),
        }
    }
}

fn set_bit(value: &mut u8, bit: u8, set: bool) {
    let mask = 1 << bit;
    if set {
        *value |= mask;
    } else {
        *value &= !mask;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_defaults_all_latches_to_zero() {
        let sam = Sam6883::new();

        assert_eq!(sam.video_mode(), 0);
        assert_eq!(sam.display_offset(), 0);
        assert_eq!(sam.display_base(), 0x0000);
        assert!(!sam.page_select());
        assert_eq!(sam.cpu_rate(), 0);
        assert_eq!(sam.memory_size(), 0);
        assert!(!sam.ty());
    }

    #[test]
    fn even_and_odd_addresses_clear_and_set_latches() {
        let mut sam = Sam6883::new();

        assert!(sam.write(0xFFC9));
        assert_eq!(sam.display_offset(), 0b000_0010);
        assert_eq!(sam.display_base(), 0x0400);

        assert!(sam.write(0xFFC8));
        assert_eq!(sam.display_offset(), 0);
        assert_eq!(sam.display_base(), 0);
    }

    #[test]
    fn write_decodes_mode_page_rate_and_memory_size_latches() {
        let mut sam = Sam6883::new();

        sam.write(0xFFC5);
        sam.write(0xFFD5);
        sam.write(0xFFD7);
        sam.write(0xFFDB);
        sam.write(0xFFDD);
        sam.write(0xFFDF);

        assert_eq!(sam.video_mode(), 0b100);
        assert!(sam.page_select());
        assert_eq!(sam.cpu_rate(), 0b01);
        assert_eq!(sam.memory_size(), 0b11);
        assert!(sam.ty());
    }

    #[test]
    fn display_base_can_be_restored_from_snapshot_address() {
        let mut sam = Sam6883::new();

        sam.set_display_base(0x0600);

        assert_eq!(sam.display_offset(), 0x03);
        assert_eq!(sam.display_base(), 0x0600);
    }

    #[test]
    fn writes_outside_sam_range_are_ignored() {
        let mut sam = Sam6883::new();

        assert!(!sam.write(0xFFBF));
        assert!(!sam.write(0xFFE0));
        assert_eq!(sam, Sam6883::new());
    }
}
