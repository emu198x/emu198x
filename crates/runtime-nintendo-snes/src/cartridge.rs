//! Turning an inserted image into a cartridge board.
//!
//! An image with a recognisable internal header gets the board its header
//! describes. One without, which some homebrew and test images are, is
//! taken as a LoROM board with no RAM, the way most emulators treat a
//! headerless dump. A coprocessor board is refused.

use common_nintendo_snes::{Cartridge, CartridgeError, Map};

/// The board for `image`.
///
/// # Errors
///
/// A coprocessor board, which this machine does not model, or an empty
/// image.
pub fn board(image: &[u8]) -> Result<Cartridge, CartridgeError> {
    match Cartridge::from_image(image) {
        Ok(cart) => Ok(cart),
        Err(CartridgeError::Image(error)) => {
            if image.is_empty() {
                return Err(CartridgeError::Image(error));
            }
            let rom = if image.len() % 1024 == 512 {
                &image[512..]
            } else {
                image
            };
            Ok(Cartridge::from_rom(rom.to_vec(), Map::LoRom, 0))
        }
        Err(other) => Err(other),
    }
}

/// The header's destination code, if the image has a header.
///
/// The parser picks the likeliest header even in an image without one, on
/// the strength of a reset vector alone, so this asks for more: a valid
/// checksum pair or a map-mode byte in the `$20-$3F` range. Otherwise a
/// headerless image's zero byte would read as Japan.
#[must_use]
pub fn destination(image: &[u8]) -> Option<(u8, bool)> {
    let parsed = format_nintendo_snes_sfc::parse(image).ok()?;
    let header = &parsed.header;
    if !header.checksum_pair_valid() && header.map_mode_byte & 0xE0 != 0x20 {
        return None;
    }
    let pal = parsed.header.video_standard() == format_nintendo_snes_sfc::VideoStandard::Pal;
    Some((parsed.header.destination, pal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_headerless_image_is_a_plain_lorom_board() {
        let cart = board(&[0xEA; 2048]).expect("accepted");
        assert_eq!(cart.map, Map::LoRom);
        assert_eq!(cart.rom().len(), 2048);
        assert!(board(&[]).is_err());
        assert_eq!(destination(&[0; 2048]), None);
        // A reset vector alone is not a header.
        let mut headerless = vec![0u8; 0x8000];
        headerless[0x7FFD] = 0x80;
        assert_eq!(destination(&headerless), None);
        let with_copier = vec![0u8; 512 + 0x8000];
        assert_eq!(board(&with_copier).expect("ok").rom().len(), 0x8000);
    }
}
