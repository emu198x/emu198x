//! DMA slot allocation.
//!
//! Each colour clock (CCK) slot is assigned to a DMA channel or the CPU.
//! The fixed slots (refresh, disk, audio, sprite) have priority over
//! variable slots (bitplane, copper, CPU).

use super::{Agnus, SlotOwner};
use crate::custom_regs;

/// Allocate the current HPOS slot.
pub fn allocate_slot(agnus: &Agnus) -> SlotOwner {
    let h = agnus.hpos;

    match h {
        // Refresh: always allocated regardless of DMACON
        0x01..=0x03 | 0x1B => SlotOwner::Refresh,

        // Disk: $04-$06
        0x04..=0x06 => {
            if agnus.channel_enabled(custom_regs::DMAF_DSKEN) {
                SlotOwner::Disk
            } else {
                SlotOwner::Cpu
            }
        }

        // Audio: $07-$0A
        0x07 => audio_or_cpu(agnus, custom_regs::DMAF_AUD0EN, 0),
        0x08 => audio_or_cpu(agnus, custom_regs::DMAF_AUD1EN, 1),
        0x09 => audio_or_cpu(agnus, custom_regs::DMAF_AUD2EN, 2),
        0x0A => audio_or_cpu(agnus, custom_regs::DMAF_AUD3EN, 3),

        // Sprites: $0B-$1A (2 slots each, 8 sprites)
        0x0B..=0x1A => {
            if agnus.channel_enabled(custom_regs::DMAF_SPREN) {
                let sprite_num = ((h - 0x0B) / 2) as u8;
                SlotOwner::Sprite(sprite_num)
            } else {
                SlotOwner::Cpu
            }
        }

        // Bitplane/Copper/CPU variable region: $1C-$E2
        0x1C..=0xE2 => allocate_variable_region(agnus, h),

        // Everything else: CPU
        _ => SlotOwner::Cpu,
    }
}

fn audio_or_cpu(agnus: &Agnus, flag: u16, ch: u8) -> SlotOwner {
    if agnus.channel_enabled(flag) {
        SlotOwner::Audio(ch)
    } else {
        SlotOwner::Cpu
    }
}

/// Allocate slots in the variable region where bitplane, copper, and CPU compete.
fn allocate_variable_region(agnus: &Agnus, h: u16) -> SlotOwner {
    let ddfstrt = agnus.ddfstrt & 0x00FC;
    let ddfstop = agnus.ddfstop & 0x00FC;

    // Bitplane DMA within data fetch window
    if agnus.channel_enabled(custom_regs::DMAF_BPLEN)
        && agnus.num_bitplanes() > 0
        && h >= ddfstrt
        && h <= ddfstop + 8
    {
        let pos_in_group = h.wrapping_sub(ddfstrt) % 8;
        if pos_in_group < u16::from(agnus.num_bitplanes()) {
            return SlotOwner::Bitplane;
        }
    }

    // Copper gets even CCK positions when enabled
    if agnus.channel_enabled(custom_regs::DMAF_COPEN) && h % 2 == 0 {
        return SlotOwner::Copper;
    }

    SlotOwner::Cpu
}

/// Returns the number of wait cycles for a chip RAM access at the given HPOS.
///
/// When a DMA channel owns the current slot, the CPU must wait for the
/// next free slot. Returns 0 if the CPU has the bus, or 2 (one CCK) if
/// DMA is active.
#[must_use]
pub fn chip_ram_contention(agnus: &Agnus) -> u8 {
    let owner = allocate_slot(agnus);
    if matches!(owner, SlotOwner::Cpu) {
        0
    } else {
        2 // One CCK = 2 CPU clocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agnus::Agnus;
    use crate::config::{AgnusVariant, Region};

    fn make_agnus() -> Agnus {
        Agnus::new(AgnusVariant::Agnus8361, Region::Pal)
    }

    #[test]
    fn refresh_slots_always_allocated() {
        let mut agnus = make_agnus();
        agnus.hpos = 0x01;
        assert_eq!(allocate_slot(&agnus), SlotOwner::Refresh);
        agnus.hpos = 0x02;
        assert_eq!(allocate_slot(&agnus), SlotOwner::Refresh);
        agnus.hpos = 0x03;
        assert_eq!(allocate_slot(&agnus), SlotOwner::Refresh);
        agnus.hpos = 0x1B;
        assert_eq!(allocate_slot(&agnus), SlotOwner::Refresh);
    }

    #[test]
    fn cpu_gets_slot_when_dma_disabled() {
        let mut agnus = make_agnus();
        agnus.hpos = 0x04;
        assert_eq!(allocate_slot(&agnus), SlotOwner::Cpu);
        agnus.hpos = 0x07;
        assert_eq!(allocate_slot(&agnus), SlotOwner::Cpu);
    }

    #[test]
    fn contention_zero_when_cpu_owns_slot() {
        let mut agnus = make_agnus();
        agnus.hpos = 0xE3; // Beyond variable region
        assert_eq!(chip_ram_contention(&agnus), 0);
    }

    #[test]
    fn contention_two_when_dma_active() {
        let mut agnus = make_agnus();
        agnus.hpos = 0x01; // Refresh slot
        assert_eq!(chip_ram_contention(&agnus), 2);
    }
}
