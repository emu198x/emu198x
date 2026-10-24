//! The layout as the header describes it.

use format_nintendo_nes_ines::{HeaderMirroring, InesError, parse};

fn header(prg: u8, chr: u8, flags6: u8, flags7: u8) -> Vec<u8> {
    let mut data = vec![0u8; 16];
    data[0..4].copy_from_slice(b"NES\x1a");
    data[4] = prg;
    data[5] = chr;
    data[6] = flags6;
    data[7] = flags7;
    data
}

#[test]
fn an_ines_file_locates_its_roms() {
    let mut data = header(1, 1, 0x13, 0x40);
    data.extend(std::iter::repeat_n(0xAA, 16 * 1024));
    data.extend(std::iter::repeat_n(0xBB, 8 * 1024));
    let image = parse(&data).expect("valid file");
    assert_eq!(image.header.mapper_number, 0x41);
    assert_eq!(image.header.mirroring, HeaderMirroring::Vertical);
    assert!(image.header.has_battery);
    assert!(!image.header.is_nes_2_0);
    assert_eq!(image.trainer, None);
    assert!(image.prg_rom.len() == 16 * 1024 && image.prg_rom.iter().all(|&b| b == 0xAA));
    assert!(image.chr_rom.len() == 8 * 1024 && image.chr_rom.iter().all(|&b| b == 0xBB));
}

#[test]
fn a_trainer_sits_between_header_and_prg() {
    let mut data = header(1, 0, 0x04, 0);
    data.extend(std::iter::repeat_n(0x11, 512));
    data.extend(std::iter::repeat_n(0x22, 16 * 1024));
    let image = parse(&data).expect("valid file");
    assert_eq!(image.trainer, Some(&[0x11; 512][..]));
    assert_eq!(image.prg_rom[0], 0x22);
    assert!(image.chr_rom.is_empty());
    assert_eq!(image.header.mirroring, HeaderMirroring::Horizontal);
}

#[test]
fn four_screen_overrides_the_mirroring_bit() {
    let mut data = header(1, 0, 0x09, 0);
    data.extend(vec![0; 16 * 1024]);
    let image = parse(&data).expect("valid file");
    assert_eq!(image.header.mirroring, HeaderMirroring::FourScreen);
}

#[test]
fn nes2_widens_the_mapper_and_sizes() {
    // Mapper $123, PRG 0x101 banks would be huge, so use the exponent form
    // instead: E = 2, M = 1 is 2^2 * 3 = 12 bytes.
    let mut data = header(0b0000_1001, 0, 0x30, 0x28);
    data[8] = 0x01;
    data[9] = 0x0F;
    data.extend(vec![0x5A; 12]);
    let image = parse(&data).expect("valid file");
    assert!(image.header.is_nes_2_0);
    assert_eq!(image.header.mapper_number, 0x123);
    assert_eq!(image.prg_rom.len(), 12);
}

#[test]
fn malformed_files_are_rejected() {
    assert_eq!(parse(&[0; 15]), Err(InesError::TooShort));
    assert_eq!(parse(&[0; 16]), Err(InesError::BadMagic));
    assert_eq!(
        parse(&header(1, 0, 0, 0)),
        Err(InesError::Truncated {
            expected: 16 + 16 * 1024,
            actual: 16
        })
    );
    let mut huge = header(0xFC, 0, 0, 0x08);
    huge[9] = 0x0F; // 2^63 * 1
    let error = parse(&huge).expect_err("too large");
    assert!(matches!(
        error,
        InesError::PrgSizeOverflow | InesError::ChrExtentOverflow | InesError::Truncated { .. }
    ));
    let mut huge_chr = header(0, 0xFC, 0, 0x08);
    huge_chr[9] = 0xF0;
    assert!(parse(&huge_chr).is_err());
    let mut overflow = header(0, 0x00, 0, 0x08);
    overflow[4] = 0xFF;
    overflow[9] = 0x0F; // 2^63 * 7 overflows
    assert_eq!(parse(&overflow), Err(InesError::PrgSizeOverflow));
    for error in [
        InesError::TooShort,
        InesError::BadMagic,
        InesError::PrgSizeOverflow,
        InesError::ChrSizeOverflow,
        InesError::PrgExtentOverflow,
        InesError::ChrExtentOverflow,
    ] {
        assert!(!error.to_string().is_empty());
    }
}
