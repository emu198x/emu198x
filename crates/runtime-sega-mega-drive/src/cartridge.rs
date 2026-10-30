//! Cartridge images: the formats they come in and what their headers say.
//!
//! Most images are a raw dump, the bytes in the order the 68000 reads them,
//! whatever the extension (`.bin`, `.md`, `.gen`). The other common format
//! is the Super Magic Drive copier's `.smd`: a 512-byte header, then the ROM
//! in 16 KB blocks, each holding its 8 KB of odd bytes and then its 8 KB of
//! even bytes. [`normalise`] turns either into a raw image.
//!
//! The header at `$100` holds a region field at `$1F0`. [`header_model`]
//! reads it, so a cartridge can choose its console when the user has not.
//!
//! A Master System cartridge (`.sms`) runs through the Power Base
//! Converter. [`is_master_system`] tells one from a Mega Drive image by its
//! headers, and [`normalise_master_system`] drops the 512-byte header some
//! copiers put in front of it.

use common_sega_mega_drive::is_master_system_image;

use crate::Model;

const SMD_HEADER: usize = 512;
const SMD_BLOCK: usize = 0x4000;
/// The SMD header's format bytes, at offsets 8 and 9.
const SMD_MAGIC: [u8; 2] = [0xAA, 0xBB];
const REGION_FIELD: usize = 0x1F0;
/// Where a Mega Drive cartridge's header says `SEGA`.
const SYSTEM_FIELD: usize = 0x100;
/// A Master System bank, which a copier's header leaves the size off by.
const SMS_BANK: usize = 0x4000;

/// Whether `rom`, normalised, is a Master System cartridge: it carries the
/// Master System's `TMR SEGA` header and not the Mega Drive's `SEGA` at
/// `$100`.
#[must_use]
pub fn is_master_system(rom: &[u8]) -> bool {
    let mega_drive = rom
        .get(SYSTEM_FIELD..SYSTEM_FIELD + 16)
        .is_some_and(|field| field.windows(4).any(|w| w == b"SEGA"));
    !mega_drive && is_master_system_image(rom)
}

/// A Master System image without the 512-byte copier header some carry.
#[must_use]
pub fn normalise_master_system(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.len() % SMS_BANK == SMD_HEADER {
        bytes.drain(..SMD_HEADER);
    }
    bytes
}

/// Whether `bytes` is an `.smd` image: a 512-byte header carrying the
/// format bytes, then whole 16 KB blocks.
#[must_use]
pub fn is_smd(bytes: &[u8]) -> bool {
    bytes.len() > SMD_HEADER
        && (bytes.len() - SMD_HEADER).is_multiple_of(SMD_BLOCK)
        && bytes[8..10] == SMD_MAGIC
}

/// The image as the 68000 reads it: an `.smd` image de-interleaved, any
/// other returned as it is.
#[must_use]
pub fn normalise(bytes: Vec<u8>) -> Vec<u8> {
    if !is_smd(&bytes) {
        return bytes;
    }
    let mut rom = vec![0; bytes.len() - SMD_HEADER];
    let (out_blocks, _) = rom.as_chunks_mut::<SMD_BLOCK>();
    let (in_blocks, _) = bytes[SMD_HEADER..].as_chunks::<SMD_BLOCK>();
    for (out, block) in out_blocks.iter_mut().zip(in_blocks) {
        let (odd, even) = block.split_at(SMD_BLOCK / 2);
        for (i, pair) in out.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            *pair = [even[i], odd[i]];
        }
    }
    rom
}

/// The console a raw image's header asks for, when it says.
///
/// The field is either letters (`J` Japan, `U` the Americas, `E` Europe)
/// or, on later cartridges, one hex digit whose bits are Japan (0), the
/// Americas (2) and Europe (3). Letters win, since `E` is a letter and a
/// digit both. A cartridge that names several markets runs on the first
/// of the Genesis, the European Mega Drive and the Japanese one it names.
#[must_use]
pub fn header_model(rom: &[u8]) -> Option<Model> {
    if is_master_system(rom) {
        return None;
    }
    let field = rom.get(REGION_FIELD..REGION_FIELD + 3)?;
    let has = |c: u8| field.contains(&c);
    let (japan, americas, europe) = if has(b'J') || has(b'U') || has(b'E') {
        (has(b'J'), has(b'U'), has(b'E'))
    } else {
        let bits = char::from(field[0]).to_digit(16)?;
        (bits & 1 != 0, bits & 4 != 0, bits & 8 != 0)
    };
    if americas {
        Some(Model::Genesis)
    } else if europe {
        Some(Model::MegaDriveEurope)
    } else if japan {
        Some(Model::MegaDriveJapan)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Interleave a raw image the way the copier stores it.
    fn to_smd(rom: &[u8]) -> Vec<u8> {
        let mut smd = vec![0; SMD_HEADER];
        smd[0] = (rom.len() / SMD_BLOCK) as u8;
        smd[1] = 3;
        smd[8..10].copy_from_slice(&SMD_MAGIC);
        for block in rom.as_chunks::<SMD_BLOCK>().0 {
            smd.extend(block.iter().skip(1).step_by(2));
            smd.extend(block.iter().step_by(2));
        }
        smd
    }

    fn raw(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 + i / 251) as u8).collect()
    }

    #[test]
    fn an_smd_image_de_interleaves_to_the_raw_rom() {
        let rom = raw(3 * SMD_BLOCK);
        let smd = to_smd(&rom);
        assert!(is_smd(&smd));
        assert_eq!(normalise(smd), rom);
    }

    #[test]
    fn a_raw_image_is_left_alone() {
        let rom = raw(SMD_BLOCK * 2);
        assert!(!is_smd(&rom));
        assert_eq!(normalise(rom.clone()), rom);
        // A raw image that happens to be 512 bytes over a block multiple
        // but lacks the format bytes is still raw.
        let mut odd_size = raw(SMD_BLOCK + SMD_HEADER);
        odd_size[8] = 0;
        assert_eq!(normalise(odd_size.clone()), odd_size);
    }

    fn with_region(field: &[u8; 3]) -> Vec<u8> {
        let mut rom = vec![0x20; 0x200];
        rom[REGION_FIELD..REGION_FIELD + 3].copy_from_slice(field);
        rom
    }

    #[test]
    fn the_region_field_chooses_a_model() {
        for (field, model) in [
            (b"J  ", Some(Model::MegaDriveJapan)),
            (b"U  ", Some(Model::Genesis)),
            (b"E  ", Some(Model::MegaDriveEurope)),
            (b"JUE", Some(Model::Genesis)),
            (b"JE ", Some(Model::MegaDriveEurope)),
            (b"1  ", Some(Model::MegaDriveJapan)),
            (b"4  ", Some(Model::Genesis)),
            (b"8  ", Some(Model::MegaDriveEurope)),
            (b"F  ", Some(Model::Genesis)),
            (b"   ", None),
            (b"2  ", None),
        ] {
            assert_eq!(
                header_model(&with_region(field)),
                model,
                "{}",
                String::from_utf8_lossy(field)
            );
        }
    }

    fn master_system(len: usize) -> Vec<u8> {
        let mut rom = vec![0; len];
        rom[0x7FF0..0x7FF8].copy_from_slice(b"TMR SEGA");
        rom
    }

    #[test]
    fn a_master_system_image_is_told_by_its_headers() {
        let rom = master_system(0x8000);
        assert!(is_master_system(&rom));
        assert_eq!(header_model(&rom), None, "no Mega Drive region field");
        let mut both = rom.clone();
        both[0x100..0x104].copy_from_slice(b"SEGA");
        assert!(!is_master_system(&both), "the Mega Drive header wins");
        assert!(!is_master_system(&raw(0x8000)));
    }

    #[test]
    fn a_copier_header_comes_off_a_master_system_image() {
        let rom = master_system(0x8000);
        let mut copied = vec![0xAA; SMD_HEADER];
        copied.extend(&rom);
        assert_eq!(normalise_master_system(copied), rom);
        assert_eq!(normalise_master_system(rom.clone()), rom);
    }

    #[test]
    fn a_short_image_has_no_region() {
        assert_eq!(header_model(&[0; 0x100]), None);
    }
}
