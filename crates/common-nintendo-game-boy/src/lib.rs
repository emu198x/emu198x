//! Shared hardware building blocks for the Nintendo Game Boy family.
//!
//! Hardware-only types and traits per
//! [`wiki/decisions/within-family-layering.md`](../../../wiki/decisions/within-family-layering.md):
//! no host-boundary plumbing here. Timing constants, the SM83's
//! `MemoryBus` view, the four-shade DMG palette, and the joypad
//! matrix are the building blocks every machine in the family will
//! compose with.

pub mod joypad;
pub mod memory;
pub mod palette;
pub mod timing;

pub use joypad::{JoypadButton, JoypadMatrix, JoypadSelect};
pub use memory::MemoryBus;
pub use palette::{DMG_GREYSCALE_RGBA, DmgPalette, dmg_palette_from_byte, dmg_pixel_rgba};
pub use timing::{
    DMG_MASTER_HZ, DMG_REFRESH_HZ, DOTS_PER_FRAME, DOTS_PER_SCANLINE, MCYCLES_PER_FRAME,
    MCYCLES_PER_SCANLINE, OAM_DMA_M_CYCLES, PPU_MODE2_DOTS, PPU_MODE3_MIN_DOTS,
    SCANLINES_PER_FRAME, SCREEN_HEIGHT, SCREEN_WIDTH, VBLANK_SCANLINES, VISIBLE_SCANLINES,
};
