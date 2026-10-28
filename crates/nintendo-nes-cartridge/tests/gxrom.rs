//! Original marker cartridges, exercising mapper 66 through the real loader.
use nintendo_nes_cartridge::{Mirroring, mapper_from_snapshot, parse_ines};

fn image(prg_banks: u8, chr_banks: u8, vertical: bool) -> Vec<u8> {
    let prg_len = usize::from(prg_banks) * 16384;
    let mut data = vec![0xFF; 16 + prg_len + usize::from(chr_banks) * 8192];
    data[..16].fill(0);
    data[..4].copy_from_slice(b"NES\x1a");
    data[4] = prg_banks;
    data[5] = chr_banks;
    data[6] = 0x20 | u8::from(vertical);
    data[7] = 0x40;
    for bank in 0..prg_len / 32768 {
        data[16 + bank * 32768 + 1] = 0x80 + bank as u8;
        data[16 + bank * 32768 + 32767] = 0x90 + bank as u8;
    }
    for bank in 0..usize::from(chr_banks) {
        data[16 + prg_len + bank * 8192..16 + prg_len + (bank + 1) * 8192].fill(0xA0 + bank as u8);
    }
    data
}

#[test]
fn all_prg_chr_pairs_use_independent_banks_and_fixed_mirroring() {
    for vertical in [false, true] {
        let mut cart = parse_ines(&image(8, 4, vertical)).expect("GxROM loads");
        assert_eq!(cart.header.mapper_number, 66);
        for prg in 0..4u8 {
            for chr in 0..4u8 {
                cart.mapper.cpu_write(0x8000, 0xCC | prg << 4 | chr);
                assert_eq!(cart.mapper.cpu_read(0x8001), 0x80 + prg);
                assert_eq!(cart.mapper.cpu_read(0xFFFF), 0x90 + prg);
                assert_eq!(cart.mapper.chr_read(0), 0xA0 + chr);
                assert_eq!(cart.mapper.chr_read(0x1FFF), 0xA0 + chr);
                assert_eq!(
                    cart.mapper.mirroring(),
                    if vertical {
                        Mirroring::Vertical
                    } else {
                        Mirroring::Horizontal
                    }
                );
                assert!(!cart.mapper.irq_pending());
            }
        }
    }
}

#[test]
fn bus_conflict_uses_the_pre_write_bank_and_chr_is_read_only() {
    let mut bytes = image(8, 4, false);
    bytes[16 + 2] = 0x11;
    bytes[16 + 32768 + 2] = 0x22;
    let mut cart = parse_ines(&bytes).expect("GxROM loads");
    cart.mapper.cpu_write(0x8002, 0x33); // Bank 0 drives $11, not $33.
    assert_eq!(cart.mapper.cpu_read(0x8001), 0x81);
    assert_eq!(cart.mapper.chr_read(0), 0xA1);
    cart.mapper.cpu_write(0x8002, 0x33); // Now bank 1 drives $22.
    assert_eq!(cart.mapper.cpu_read(0x8001), 0x82);
    assert_eq!(cart.mapper.chr_read(0), 0xA2);
    cart.mapper.chr_write(0, 0x55);
    assert_eq!(cart.mapper.chr_read(0), 0xA2);
    cart.mapper.cpu_write(0x7FFF, 0);
    assert_eq!(cart.mapper.cpu_read(0x8001), 0x82);
    assert_eq!(cart.mapper.cpu_read(0x6000), 0);
    let mut restored = mapper_from_snapshot(cart.mapper.snapshot());
    assert_eq!(restored.cpu_read(0x8001), 0x82);
    assert_eq!(restored.chr_read(0), 0xA2);
    restored.cpu_write(0x8000, 0x30);
    assert_eq!(restored.cpu_read(0x8001), 0x83);
    assert_eq!(restored.chr_read(0), 0xA0);
}

#[test]
fn smaller_mhrom_banks_mirror_unconnected_address_lines() {
    let mut cart = parse_ines(&image(4, 2, false)).expect("MHROM loads");
    cart.mapper.cpu_write(0x8000, 0x33);
    assert_eq!(cart.mapper.cpu_read(0x8001), 0x81);
    assert_eq!(cart.mapper.chr_read(0), 0xA1);
}

#[test]
fn unsupported_board_extents_are_refused() {
    for (prg, chr) in [(1, 1), (6, 1), (16, 1), (2, 0), (2, 3), (2, 8)] {
        assert!(
            parse_ines(&image(prg, chr, false)).is_err(),
            "PRG {prg}, CHR {chr}"
        );
    }
}
