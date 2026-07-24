//! Agnus wiring for the OCS machine.
//!
//! As of task #139 (port session 2026-04-20), the Agnus implementation
//! lives in the standalone `commodore-agnus-ocs` crate. This module
//! re-exports the chip type and provides a few derived constants the
//! machine uses to convert between CCK time (Agnus's native unit) and
//! master/4 ticks (the machine's primary clock).

use std::ops::{Deref, DerefMut};

use commodore_agnus_ecs::AgnusEcs;

pub use commodore_agnus_ocs::{
    Agnus, AgnusRegion, CckBusPlan, NTSC_CCKS_PER_FRAME, NTSC_LINES_PER_FRAME, PAL_CCKS_PER_FRAME,
    PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME, SlotOwner, VBL_END_LINE, bits,
};

/// Agnus silicon installed in an OCS-shaped machine.
///
/// Later A500 and A2000 revisions pair ECS Fat Agnus 8372A with OCS
/// Denise. Keeping that combination in this machine preserves the board's
/// real mixed chip stack while reusing the ECS Agnus extension layer for
/// the registers and timing behavior owned by 8372A.
///
/// Variant order is part of the postcard snapshot schema. Reordering or
/// inserting variants requires an Amiga runtime snapshot-version bump.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) enum InstalledAgnus {
    EarlyOcs(Agnus),
    Fat8372A(AgnusEcs),
}

/// Result of an ECS-only blitter-extension write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExtendedBlitterWrite {
    Stored,
    Started,
}

impl InstalledAgnus {
    #[must_use]
    pub(crate) fn early_ocs(region: AgnusRegion) -> Self {
        Self::EarlyOcs(Agnus::new_with_region(region))
    }

    #[must_use]
    pub(crate) fn fat_8372a(region: AgnusRegion) -> Self {
        Self::Fat8372A(AgnusEcs::from_ocs(Agnus::new_with_region(region)))
    }

    #[must_use]
    pub(crate) const fn base(&self) -> &Agnus {
        match self {
            Self::EarlyOcs(agnus) => agnus,
            Self::Fat8372A(agnus) => agnus.as_inner(),
        }
    }

    #[must_use]
    pub(crate) fn base_mut(&mut self) -> &mut Agnus {
        match self {
            Self::EarlyOcs(agnus) => agnus,
            Self::Fat8372A(agnus) => agnus.as_inner_mut(),
        }
    }

    #[must_use]
    pub(crate) const fn is_fat_8372a(&self) -> bool {
        matches!(self, Self::Fat8372A(_))
    }

    pub(crate) fn tick_cck(&mut self) {
        match self {
            Self::EarlyOcs(agnus) => agnus.tick_cck(),
            Self::Fat8372A(agnus) => agnus.tick_cck(),
        }
    }

    #[must_use]
    pub(crate) fn cck_bus_plan(&self) -> CckBusPlan {
        match self {
            Self::EarlyOcs(agnus) => agnus.cck_bus_plan(),
            Self::Fat8372A(agnus) => agnus.cck_bus_plan(),
        }
    }

    #[must_use]
    pub(crate) fn vertical_diw_active(&self) -> bool {
        match self {
            Self::EarlyOcs(agnus) => agnus.vertical_diw_active(),
            Self::Fat8372A(agnus) => agnus.vertical_diw_active(),
        }
    }

    pub(crate) fn service_sprite_dma_cyc(
        &mut self,
        channel: usize,
        second_word: bool,
        width: u8,
        read: impl FnMut(u32) -> u16,
    ) -> Option<(bool, u64)> {
        match self {
            Self::EarlyOcs(agnus) => {
                agnus.service_sprite_dma_cyc(channel, second_word, width, read)
            }
            Self::Fat8372A(agnus) => {
                agnus.service_sprite_dma_cyc(channel, second_word, width, read)
            }
        }
    }

    pub(crate) fn poke_sprite_pos(&mut self, channel: usize, val: u16) {
        match self {
            Self::EarlyOcs(agnus) => agnus.poke_sprite_pos(channel, val),
            Self::Fat8372A(agnus) => agnus.poke_sprite_pos(channel, val),
        }
    }

    pub(crate) fn poke_sprite_ctl(&mut self, channel: usize, val: u16) {
        match self {
            Self::EarlyOcs(agnus) => agnus.poke_sprite_ctl(channel, val),
            Self::Fat8372A(agnus) => agnus.poke_sprite_ctl(channel, val),
        }
    }

    pub(crate) fn write_diwstrt(&mut self, val: u16) {
        match self {
            Self::EarlyOcs(agnus) => agnus.write_diwstrt(val),
            Self::Fat8372A(agnus) => agnus.write_diwstrt(val),
        }
    }

    pub(crate) fn write_diwstop(&mut self, val: u16) {
        match self {
            Self::EarlyOcs(agnus) => agnus.write_diwstop(val),
            Self::Fat8372A(agnus) => agnus.write_diwstop(val),
        }
    }

    /// Route one programmable-timing register to Fat Agnus.
    ///
    /// Early OCS Agnus does not decode this register block.
    pub(crate) fn write_timing_register(&mut self, offset: u16, val: u16) -> bool {
        match self {
            Self::EarlyOcs(_) => false,
            Self::Fat8372A(agnus) => agnus.write_timing_register(offset, val),
        }
    }

    /// Route one ECS blitter-extension register to Fat Agnus.
    ///
    /// Returns `None` for early OCS and for offsets outside the extension
    /// trio. The caller serializes the write against an in-flight blit
    /// before invoking this method.
    pub(crate) fn write_extended_blitter_register(
        &mut self,
        offset: u16,
        val: u16,
    ) -> Option<ExtendedBlitterWrite> {
        let Self::Fat8372A(agnus) = self else {
            return None;
        };
        match offset {
            0x05A => {
                agnus.write_bltcon0l(val);
                Some(ExtendedBlitterWrite::Stored)
            }
            0x05C => {
                agnus.write_bltsizv(val);
                Some(ExtendedBlitterWrite::Stored)
            }
            0x05E => {
                agnus.write_bltsizh(val);
                Some(ExtendedBlitterWrite::Started)
            }
            _ => None,
        }
    }
}

impl Deref for InstalledAgnus {
    type Target = Agnus;

    fn deref(&self) -> &Self::Target {
        self.base()
    }
}

impl DerefMut for InstalledAgnus {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.base_mut()
    }
}

/// PAL line length in CCKs — alias for the chip-crate constant under
/// the name the machine has historically used.
pub const PAL_LINE_CCKS: u16 = PAL_CCKS_PER_LINE;

/// PAL frame line count.
pub const PAL_FRAME_LINES: u16 = PAL_LINES_PER_FRAME;

/// PAL line length in master/4 ticks (= lores pixels = 68000 CPU
/// clocks). One CCK = 2 ticks.
pub const PAL_LINE_TICKS: u16 = PAL_CCKS_PER_LINE * 2;

/// PAL frame length in master/4 ticks: 227 × 312 × 2 = 141,648.
pub const PAL_FRAME_TICKS: u64 = (PAL_LINE_TICKS as u64) * (PAL_FRAME_LINES as u64);

/// NTSC frame length in master/4 ticks. NTSC alternates short (227
/// CCK) and long (228 CCK) lines per HRM p. 785, so the frame total
/// is 131 × 227 + 131 × 228 = 59,605 CCK = 119,210 ticks.
pub const NTSC_FRAME_TICKS: u64 = (NTSC_CCKS_PER_FRAME as u64) * 2;
