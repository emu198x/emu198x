//! Atari 5200 cartridge handling.
//!
//! ROM mapping (no bank switching) for 4 KB, 8 KB, 16 KB, and 32 KB
//! cartridges across the `$4000-$BFFF` window.
//!
//! 4 KB, 8 KB and 32 KB carts have one layout each: the image sits at the
//! top of the window and mirrors downward to fill it. **16 KB carts have
//! two**, and size cannot tell them apart:
//!
//! - **Linear.** The image maps straight across `$8000-$BFFF` and mirrors
//!   into `$4000-$7FFF`. Robotron, Missile Command's 16 KB siblings, and
//!   22 other titles.
//! - **Two chip (EE_16).** Two 8 KB ROM chips decoded by CPU A15: the
//!   lower answers `$4000-$7FFF`, the upper `$8000-$BFFF`. A13/A14 are
//!   don't-care, so each chip mirrors twice within its 16 KB half.
//!   Pac-Man, Galaxian, Defender, Star Raiders, and 35 others.
//!
//! The layouts agree at `$4000-$5FFF`, `$A000-$BFFF`, and — crucially —
//! at the cart start vector in `$BFFE`. They disagree at `$6000-$7FFF`
//! and `$8000-$9FFF`, each serving the other's 8 KB. So the wrong choice
//! still loads, still runs, and still reports success; it just executes
//! the wrong half. Robotron's vector points at `$8000`, where linear
//! serves `LDA #$00 / STA $D40E` (disable NMIs, the canonical first act
//! of a 5200 cart) and two-chip serves `JSR $9D3D` into uninitialised
//! code.
//!
//! A headerless dump carries no cart-type byte to choose with, and the
//! library splits 39 two-chip against 23 linear, so neither default is
//! safe on its own. `cart_layouts` holds the CRC32 of every known
//! two-chip cart, distilled from MAME's CC0-licensed software list;
//! anything not in it is linear, which is the same default MAME applies
//! to a headerless dump. See
//! `knowledge/decisions/cart-layout-needs-positive-evidence.md`.
//!
//! Adapted from `Emu198x-Oldest/crates/machine-atari-5200/src/cartridge.rs`
//! (port 2026-06-01); 16 KB two-chip decode added 2026-06-04, then made
//! evidence-driven 2026-08-25.

use serde::{Deserialize, Serialize};

use crate::cart_layouts::TWO_CHIP_16K_CRC32;

/// How a cartridge image answers the `$4000-$BFFF` window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CartLayout {
    /// Image mirrors across the window from the top. Every 4 KB, 8 KB and
    /// 32 KB cart, and the majority of 16 KB ones.
    #[default]
    Linear,
    /// Two 8 KB chips selected by A15, each mirrored twice within its half.
    TwoChip16K,
}

#[derive(Serialize, Deserialize)]
pub struct Cartridge {
    rom: Vec<u8>,
    base_addr: u16,
    layout: CartLayout,
}

impl Cartridge {
    /// Load a raw, headerless cartridge image, choosing the layout from
    /// the known-cartridge table.
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        let layout = detect_layout(data);
        Self::from_rom_with_layout(data, layout)
    }

    /// Load an image with the layout given rather than detected. Used by
    /// the tests to exercise a decode without shipping a commercial ROM,
    /// and by any caller that has better evidence than the table — a
    /// `.a52` header's cart-type byte, say (#419).
    pub fn from_rom_with_layout(data: &[u8], layout: CartLayout) -> Result<Self, String> {
        let base_addr = match data.len() {
            4096 => 0xB000,
            8192 => 0xA000,
            16384 => 0x8000,
            32768 => 0x4000,
            other => return Err(format!("Unsupported cartridge size: {other} bytes")),
        };
        Ok(Self {
            rom: data.to_vec(),
            base_addr,
            layout,
        })
    }

    #[must_use]
    pub fn read(&self, addr: u16) -> u8 {
        if self.rom.is_empty() {
            return 0xFF;
        }
        let offset = match self.layout {
            // A15 selects the 8 KB chip, A0-A12 address within it, A13/A14
            // mirror. $8000-$BFFF -> upper 8 KB (ROM $2000-$3FFF),
            // $4000-$7FFF -> lower 8 KB (ROM $0000-$1FFF).
            CartLayout::TwoChip16K => {
                (addr as usize & 0x1FFF) | usize::from(addr & 0x8000 != 0) << 13
            }
            CartLayout::Linear => addr.wrapping_sub(self.base_addr) as usize % self.rom.len(),
        };
        self.rom[offset]
    }

    #[must_use]
    pub fn base_addr(&self) -> u16 {
        self.base_addr
    }

    #[must_use]
    pub fn layout(&self) -> CartLayout {
        self.layout
    }
}

/// Pick a layout for a headerless image. Only 16 KB is ambiguous, and only
/// a cart we can positively identify is treated as two-chip.
fn detect_layout(data: &[u8]) -> CartLayout {
    if data.len() == 16384 && TWO_CHIP_16K_CRC32.binary_search(&crc32(data)).is_ok() {
        CartLayout::TwoChip16K
    } else {
        CartLayout::Linear
    }
}

/// CRC-32/ISO-HDLC, to match the checksums MAME's software list records.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_4k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 4096]).expect("4K");
        assert_eq!(cart.base_addr(), 0xB000);
    }

    #[test]
    fn detect_8k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 8192]).expect("8K");
        assert_eq!(cart.base_addr(), 0xA000);
    }

    #[test]
    fn detect_16k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 16384]).expect("16K");
        assert_eq!(cart.base_addr(), 0x8000);
    }

    #[test]
    fn detect_32k_rom() {
        let cart = Cartridge::from_rom(&vec![0xEA; 32768]).expect("32K");
        assert_eq!(cart.base_addr(), 0x4000);
    }

    #[test]
    fn reject_invalid_size() {
        assert!(Cartridge::from_rom(&vec![0u8; 5000]).is_err());
    }

    #[test]
    fn reset_vector_at_bffc_for_8k() {
        let mut rom = vec![0u8; 8192];
        rom[0x1FFC] = 0x00;
        rom[0x1FFD] = 0xA0;
        let cart = Cartridge::from_rom(&rom).expect("8K");
        assert_eq!(cart.read(0xBFFC), 0x00);
        assert_eq!(cart.read(0xBFFD), 0xA0);
    }

    /// The check-value every CRC-32/ISO-HDLC implementation agrees on.
    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    /// `detect_layout` binary-searches the table, which is only valid if
    /// the generator emitted it sorted and free of duplicates.
    #[test]
    fn two_chip_table_is_sorted_and_unique() {
        assert!(
            TWO_CHIP_16K_CRC32.windows(2).all(|pair| pair[0] < pair[1]),
            "regenerate with tools/a5200-cart-layouts.py"
        );
    }

    /// Astro Chase is `a5200_2chips` in MAME's list; Robotron is
    /// `a5200_rom`. Both are 16 KB, which is the whole difficulty.
    #[test]
    fn known_carts_choose_their_own_layout() {
        assert!(
            TWO_CHIP_16K_CRC32.binary_search(&0x4019_ECEC).is_ok(),
            "Astro Chase"
        );
        assert!(
            TWO_CHIP_16K_CRC32.binary_search(&0x4252_ABD9).is_err(),
            "Robotron: 2084 is linear, and mapping it two-chip lands JSR $9D3D on the entry point"
        );
    }

    /// An unknown 16 KB image is linear, matching MAME's headerless guess.
    #[test]
    fn unknown_sixteen_kb_image_is_linear() {
        let cart = Cartridge::from_rom(&vec![0xEA; 16384]).expect("16K");
        assert_eq!(cart.layout(), CartLayout::Linear);
        // Linear serves $8000 from the start of the image, not its midpoint.
        let mut rom = vec![0u8; 16384];
        rom[0x0000] = 0xA9;
        rom[0x2000] = 0x20;
        let cart = Cartridge::from_rom(&rom).expect("16K");
        assert_eq!(cart.read(0x8000), 0xA9);
    }

    /// Sizes with only one layout never consult the table.
    #[test]
    fn unambiguous_sizes_are_always_linear() {
        for size in [4096usize, 8192, 32768] {
            let cart = Cartridge::from_rom(&vec![0xEA; size]).expect("cart");
            assert_eq!(cart.layout(), CartLayout::Linear, "{size} bytes");
        }
    }

    #[test]
    fn sixteen_kb_two_chip_decode() {
        // Lay a unique marker in each 8 KB chip so the decode is
        // unambiguous: lower chip = ROM $0000-$1FFF, upper = $2000-$3FFF.
        let mut rom = vec![0u8; 16384];
        rom[0x0000] = 0xA1; // lower chip, first byte
        rom[0x1FFF] = 0xA2; // lower chip, last byte
        rom[0x2000] = 0xB1; // upper chip, first byte
        rom[0x3FFF] = 0xB2; // upper chip, last byte
        rom[0x2386] = 0x78; // entry-point byte (cf. Pac-Man's $8386 = SEI)
        let cart =
            Cartridge::from_rom_with_layout(&rom, CartLayout::TwoChip16K).expect("16K two-chip");

        // Lower 8 KB answers $4000-$7FFF; upper 8 KB answers $8000-$BFFF.
        assert_eq!(cart.read(0x4000), 0xA1);
        assert_eq!(cart.read(0x8000), 0xB1);
        assert_eq!(cart.read(0xBFFF), 0xB2);
        // The cart entry vector ($BFFE) and its target both live in the
        // upper chip — the bug this guards against put $8386 in the lower
        // chip's empty space and the machine executed padding.
        assert_eq!(cart.read(0x8386), 0x78);

        // A13/A14 are don't-care, so each chip mirrors twice within its
        // 16 KB half: $6000-$7FFF repeats the lower chip, $A000-$BFFF the
        // upper.
        assert_eq!(cart.read(0x6000), 0xA1);
        assert_eq!(cart.read(0xA000), 0xB1);
        assert_eq!(cart.read(0x7FFF), 0xA2);
    }

    /// The two layouts agree everywhere except `$6000-$9FFF`, where each
    /// serves the other's 8 KB — which is why a mismapped cart still boots
    /// far enough to look like it worked.
    #[test]
    fn layouts_differ_only_between_6000_and_9fff() {
        // Give each 8 KB chip a distinguishable pattern, or the windows
        // that should disagree can coincide by accident.
        let rom: Vec<u8> = (0..16384usize)
            .map(|i| {
                let byte = (i & 0xFF) as u8;
                if i < 0x2000 { byte } else { byte ^ 0xFF }
            })
            .collect();
        let linear = Cartridge::from_rom_with_layout(&rom, CartLayout::Linear).expect("linear");
        let two_chip =
            Cartridge::from_rom_with_layout(&rom, CartLayout::TwoChip16K).expect("two-chip");

        const DISPUTED: std::ops::Range<u16> = 0x6000..0xA000;
        for addr in 0x4000..=0xBFFFu16 {
            if DISPUTED.contains(&addr) {
                continue;
            }
            assert_eq!(
                linear.read(addr),
                two_chip.read(addr),
                "${addr:04X} should read the same under either layout"
            );
        }

        // And they really do differ inside it, or the test proves nothing.
        assert_ne!(linear.read(0x6000), two_chip.read(0x6000));
        assert_ne!(linear.read(0x8000), two_chip.read(0x8000));
        assert_ne!(linear.read(0x9FFF), two_chip.read(0x9FFF));
    }
}
