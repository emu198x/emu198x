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
    /// Bounty Bob Strikes Back: 40 KB behind two switchable 4 KB windows.
    ///
    /// The image is a fixed 8 KB followed by two banks of four 4 KB
    /// pages. `$8000-$BFFF` shows the fixed half mirrored twice;
    /// `$4000-$4FFF` and `$5000-$5FFF` each show one of their four
    /// pages, chosen by touching `$4FF6-$4FF9` and `$5FF6-$5FF9`.
    BountyBob,
}

/// Size of a Bounty Bob image: 8 KB fixed + 2 × 4 × 4 KB of banks.
const BBSB_LEN: usize = 0xA000;
/// Where each window's four pages begin in the image.
const BBSB_WINDOW_BASE: [usize; 2] = [0x2000, 0x6000];

#[derive(Serialize, Deserialize)]
pub struct Cartridge {
    rom: Vec<u8>,
    base_addr: u16,
    layout: CartLayout,
    /// Selected page for each Bounty Bob window; unused by every other
    /// layout. Both power on at 0.
    #[serde(default)]
    banks: [u8; 2],
}

impl Cartridge {
    /// Load a cartridge image, choosing the layout from the best evidence
    /// available.
    ///
    /// A `.a52`/`.car` header states the layout outright, so it wins. A
    /// headerless dump falls back to the known-cartridge table, and an
    /// image in neither is linear. See
    /// `knowledge/decisions/cart-layout-needs-positive-evidence.md`.
    pub fn from_rom(data: &[u8]) -> Result<Self, String> {
        if let Some((cart_type, body)) = split_header(data) {
            let layout = layout_for_cart_type(cart_type)?;
            return Self::from_rom_with_layout(body, layout);
        }
        let layout = detect_layout(data);
        Self::from_rom_with_layout(data, layout)
    }

    /// Load an image with the layout given rather than detected. Used by
    /// the tests to exercise a decode without shipping a commercial ROM,
    /// and by any caller that has better evidence than the table — a
    /// `.a52` header's cart-type byte, say (#419).
    pub fn from_rom_with_layout(data: &[u8], layout: CartLayout) -> Result<Self, String> {
        let base_addr = match (layout, data.len()) {
            (CartLayout::BountyBob, BBSB_LEN) => 0x4000,
            (CartLayout::BountyBob, other) => {
                return Err(format!(
                    "Bounty Bob needs a {BBSB_LEN}-byte image, got {other}"
                ));
            }
            (_, 4096) => 0xB000,
            (_, 8192) => 0xA000,
            (_, 16384) => 0x8000,
            (_, 32768) => 0x4000,
            (_, other) => return Err(format!("Unsupported cartridge size: {other} bytes")),
        };
        if layout == CartLayout::TwoChip16K && data.len() != 16384 {
            return Err(format!(
                "two-chip layout needs a 16384-byte image, got {}",
                data.len()
            ));
        }
        Ok(Self {
            rom: data.to_vec(),
            base_addr,
            layout,
            banks: [0; 2],
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
            CartLayout::BountyBob => match addr {
                // The overlay answers its own registers rather than the
                // ROM behind them. MAME returns $FF here and flags the
                // value as unconfirmed against hardware.
                0x4FF6..=0x4FF9 | 0x5FF6..=0x5FF9 => return 0xFF,
                // Two switchable 4 KB windows.
                0x4000..=0x5FFF => {
                    let window = usize::from(addr >= 0x5000);
                    let page = self.banks[window] as usize;
                    BBSB_WINDOW_BASE[window] + page * 0x1000 + (addr as usize & 0x0FFF)
                }
                // The fixed 8 KB, mirrored twice across $8000-$BFFF.
                _ => addr as usize & 0x1FFF,
            },
        };
        self.rom[offset]
    }

    /// Touch a Bounty Bob bank register, if `addr` is one.
    ///
    /// The overlay decodes `$xFF6-$xFF9` in both windows, and it decodes
    /// them for reads as much as writes — the game switches by reading.
    /// Returns the base of the window whose page changed, so the caller
    /// can refresh anything it has cached for that range.
    pub fn touch_bank_register(&mut self, addr: u16) -> Option<u16> {
        if self.layout != CartLayout::BountyBob {
            return None;
        }
        let (window, base) = match addr {
            0x4FF6..=0x4FF9 => (0usize, 0x4000u16),
            0x5FF6..=0x5FF9 => (1, 0x5000),
            _ => return None,
        };
        // $xFF6 selects page 0 and $xFF9 page 3 — the page is the
        // register's offset from $xFF6, not the low bits of the address.
        // ($FF6 & 3 is 2, which silently mis-selects every page.)
        let page = (addr & 0x0F) as u8 - 0x06;
        if self.banks[window] == page {
            return None;
        }
        self.banks[window] = page;
        Some(base)
    }

    /// Selected pages, for tests and chip inspection.
    #[must_use]
    pub const fn banks(&self) -> [u8; 2] {
        self.banks
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

/// Length of a `.a52`/`.car` header: `CART` magic, the cart type as a
/// big-endian `u32`, a checksum, and four reserved bytes.
const HEADER_LEN: usize = 16;
const HEADER_MAGIC: &[u8; 4] = b"CART";

/// Split a recognised header off the front of an image.
///
/// A headered dump is a whole number of 4 KB pages plus the 16-byte
/// header, which is the test MAME uses, and it starts with `CART`. Both
/// have to hold: a raw 4 KB dump is a whole number of pages with no
/// header, and requiring the magic keeps a coincidentally-sized file
/// from being decapitated.
///
/// The header's checksum is not verified. It is a plain sum of the data
/// bytes, and a dump whose checksum is stale is still the dump the user
/// asked us to run — refusing it would be a worse outcome than loading
/// it, and nothing downstream depends on the value.
fn split_header(data: &[u8]) -> Option<(u32, &[u8])> {
    if data.len() % 0x1000 != HEADER_LEN {
        return None;
    }
    let (header, body) = data.split_at(HEADER_LEN);
    if &header[0..4] != HEADER_MAGIC {
        return None;
    }
    let cart_type = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
    Some((cart_type, body))
}

/// The layout a header's cart-type code names.
///
/// Codes are the shared `.car` numbering, which spans the 8-bit machines
/// too; these are the six that mean "Atari 5200" (MAME
/// `a5200_cart_slot_device::identify_cart_type`).
fn layout_for_cart_type(cart_type: u32) -> Result<CartLayout, String> {
    match cart_type {
        4 | 16 | 19 | 20 => Ok(CartLayout::Linear),
        6 => Ok(CartLayout::TwoChip16K),
        7 => Ok(CartLayout::BountyBob),
        other => Err(format!(
            "cartridge header declares type {other}, which is not an Atari 5200 layout"
        )),
    }
}

/// Pick a layout for a headerless image. Only 16 KB is ambiguous, and only
/// a cart we can positively identify is treated as two-chip.
fn detect_layout(data: &[u8]) -> CartLayout {
    // 40 KB is Bounty Bob and nothing else in the 5200 library, so the
    // size is positive evidence on its own.
    if data.len() == BBSB_LEN {
        return CartLayout::BountyBob;
    }
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
    /// `expect_err` would need `Debug` on `Cartridge`, and a derived one
    /// would print the whole ROM.
    fn err_of(result: Result<Cartridge, String>) -> String {
        match result {
            Err(err) => err,
            Ok(_) => panic!("expected an error, got a cartridge"),
        }
    }

    fn headered(cart_type: u32, body: &[u8]) -> Vec<u8> {
        let mut image = b"CART".to_vec();
        image.extend_from_slice(&cart_type.to_be_bytes());
        image.extend_from_slice(&[0; 8]); // checksum + reserved
        image.extend_from_slice(body);
        image
    }

    /// A header states the layout, so it beats the CRC table. Robotron
    /// is linear and absent from the table; declaring it two-chip must
    /// be honoured, because the header knows and the table only guesses.
    #[test]
    fn header_outranks_the_known_cartridge_table() {
        let body = vec![0xEA; 16384];
        let cart = Cartridge::from_rom(&headered(6, &body)).expect("two-chip header");
        assert_eq!(cart.layout(), CartLayout::TwoChip16K);

        let cart = Cartridge::from_rom(&headered(16, &body)).expect("linear header");
        assert_eq!(cart.layout(), CartLayout::Linear);
    }

    #[test]
    fn header_is_stripped_before_sizing() {
        for (cart_type, size) in [(20u32, 4096usize), (19, 8192), (16, 16384), (4, 32768)] {
            let image = headered(cart_type, &vec![0xEA; size]);
            assert_eq!(image.len(), size + 16);
            let cart = Cartridge::from_rom(&image)
                .unwrap_or_else(|err| panic!("type {cart_type}, {size} bytes: {err}"));
            assert_eq!(cart.rom.len(), size, "header must not reach the ROM");
        }
    }

    #[test]
    fn unknown_cart_type_is_rejected() {
        let err = err_of(Cartridge::from_rom(&headered(3, &vec![0xEA; 8192])));
        assert!(err.contains("not an Atari 5200 layout"), "{err}");
    }

    /// Both conditions matter: a raw 4 KB dump is a whole number of
    /// pages, and a file that merely has the right remainder is not a
    /// header.
    #[test]
    fn only_a_real_header_is_stripped() {
        let raw = vec![0xEA; 8192];
        assert_eq!(Cartridge::from_rom(&raw).expect("raw 8K").rom.len(), 8192);

        let mut impostor = vec![0u8; 16];
        impostor.extend_from_slice(&vec![0xEA; 8192]);
        let err = err_of(Cartridge::from_rom(&impostor));
        assert!(err.contains("8208"), "{err}");
    }

    #[test]
    fn two_chip_needs_a_sixteen_kb_image() {
        let err = err_of(Cartridge::from_rom_with_layout(
            &vec![0xEA; 8192],
            CartLayout::TwoChip16K,
        ));
        assert!(err.contains("16384-byte image"), "{err}");
    }

    /// A Bounty Bob image with each 4 KB page filled with its own index,
    /// so a read says which page answered.
    fn bbsb_image() -> Vec<u8> {
        let mut rom = vec![0u8; BBSB_LEN];
        rom[0x0000..0x2000].fill(0xF1); // fixed half
        for (window, &base) in BBSB_WINDOW_BASE.iter().enumerate() {
            for page in 0..4usize {
                let at = base + page * 0x1000;
                rom[at..at + 0x1000].fill((window * 4 + page) as u8);
            }
        }
        rom
    }

    /// `$xFF6` selects page 0 and `$xFF9` page 3. Masking the address
    /// with 3 instead — `$FF6 & 3` is 2 — mis-selects every page, and
    /// Bounty Bob crashes into `$0001` within 3,212 instructions because
    /// its bank-switch trampoline returns into the wrong page.
    #[test]
    fn bank_registers_select_by_offset_not_by_low_bits() {
        let mut cart =
            Cartridge::from_rom_with_layout(&bbsb_image(), CartLayout::BountyBob).expect("40K");
        for (reg, page) in [(0x4FF6u16, 0u8), (0x4FF7, 1), (0x4FF8, 2), (0x4FF9, 3)] {
            cart.touch_bank_register(reg);
            assert_eq!(
                cart.banks()[0],
                page,
                "${reg:04X} should select page {page}"
            );
        }
        for (reg, page) in [(0x5FF6u16, 0u8), (0x5FF7, 1), (0x5FF8, 2), (0x5FF9, 3)] {
            cart.touch_bank_register(reg);
            assert_eq!(
                cart.banks()[1],
                page,
                "${reg:04X} should select page {page}"
            );
        }
    }

    #[test]
    fn bounty_bob_windows_read_their_selected_page() {
        let mut cart =
            Cartridge::from_rom_with_layout(&bbsb_image(), CartLayout::BountyBob).expect("40K");
        // Both windows start on page 0.
        assert_eq!(cart.read(0x4000), 0, "window 0, page 0");
        assert_eq!(cart.read(0x5000), 4, "window 1, page 0");

        cart.touch_bank_register(0x4FF9);
        assert_eq!(cart.read(0x4000), 3, "window 0 moved to page 3");
        assert_eq!(cart.read(0x5000), 4, "window 1 unmoved");

        cart.touch_bank_register(0x5FF8);
        assert_eq!(cart.read(0x5000), 6, "window 1 moved to page 2");
        assert_eq!(cart.read(0x4000), 3, "window 0 unmoved");
    }

    /// The fixed 8 KB answers `$8000-$BFFF`, mirrored twice, and carries
    /// the cart start vector at `$BFFE`.
    #[test]
    fn bounty_bob_fixed_half_mirrors_across_the_top() {
        let mut rom = bbsb_image();
        rom[0x1FFE] = 0x4F;
        rom[0x1FFF] = 0xA1;
        let cart = Cartridge::from_rom_with_layout(&rom, CartLayout::BountyBob).expect("40K");
        assert_eq!(cart.read(0x8000), 0xF1);
        assert_eq!(cart.read(0xA000), 0xF1, "mirrored");
        assert_eq!(cart.read(0xBFFE), 0x4F);
        assert_eq!(cart.read(0xBFFF), 0xA1, "start vector $A14F");
    }

    /// The overlay answers its own registers rather than the ROM behind
    /// them.
    #[test]
    fn bank_registers_read_as_open_bus() {
        let cart =
            Cartridge::from_rom_with_layout(&bbsb_image(), CartLayout::BountyBob).expect("40K");
        for reg in [0x4FF6u16, 0x4FF9, 0x5FF6, 0x5FF9] {
            assert_eq!(cart.read(reg), 0xFF, "${reg:04X}");
        }
        assert_ne!(cart.read(0x4FF5), 0xFF, "the byte below is ordinary ROM");
    }

    /// Touching a register that is already selected reports no change,
    /// so the machine does not re-bake ANTIC's shadow needlessly.
    #[test]
    fn reselecting_the_same_page_reports_no_change() {
        let mut cart =
            Cartridge::from_rom_with_layout(&bbsb_image(), CartLayout::BountyBob).expect("40K");
        assert_eq!(cart.touch_bank_register(0x4FF8), Some(0x4000));
        assert_eq!(cart.touch_bank_register(0x4FF8), None, "already on page 2");
        assert_eq!(cart.touch_bank_register(0x4321), None, "not a register");
    }

    /// 40 KB is Bounty Bob and nothing else in the library, so a
    /// headerless dump is identified by size, and a type-7 header says
    /// so outright.
    #[test]
    fn bounty_bob_is_recognised_headerless_and_headered() {
        let rom = bbsb_image();
        assert_eq!(
            Cartridge::from_rom(&rom).expect("headerless").layout(),
            CartLayout::BountyBob
        );
        assert_eq!(
            Cartridge::from_rom(&headered(7, &rom))
                .expect("headered")
                .layout(),
            CartLayout::BountyBob
        );
    }

    #[test]
    fn bounty_bob_needs_a_forty_kb_image() {
        let err = err_of(Cartridge::from_rom_with_layout(
            &vec![0xEA; 32768],
            CartLayout::BountyBob,
        ));
        assert!(err.contains("40960-byte image"), "{err}");
    }

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
