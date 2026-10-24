//! ROM extents come from the encoding, independently of mapper bank windows.
//! NESdev NES 2.0 specification, PRG-ROM/CHR-ROM size sections.

use nintendo_nes_cartridge::parse_ines;

fn image(prg_size: usize, chr_size: usize, prg: u8, chr: u8, high: u8, trainer: bool) -> Vec<u8> {
    let start = 16 + if trainer { 512 } else { 0 };
    let mut bytes = vec![0; start + prg_size + chr_size];
    bytes[..4].copy_from_slice(b"NES\x1a");
    bytes[4] = prg;
    bytes[5] = chr;
    bytes[6] = 0x30 | if trainer { 4 } else { 0 }; // CNROM: both address spaces observable.
    bytes[7] = 8;
    bytes[9] = high;
    bytes[16..start].fill(0x5a);
    for (i, byte) in bytes[start..start + prg_size].iter_mut().enumerate() {
        *byte = ((i / 1024) as u8).wrapping_add(0x40);
    }
    bytes[start] = 0xff; // Bank-select writes must survive CNROM's bus conflict.
    for (i, byte) in bytes[start + prg_size..].iter_mut().enumerate() {
        *byte = ((i / 1024) as u8).wrapping_add(0x80);
    }
    bytes
}

#[test]
fn nes2_linear_and_exponent_images_deliver_identical_rom_bytes() {
    for trainer in [false, true] {
        let reference = image(32768, 8192, 2, 1, 0, trainer);
        for (prg, chr, high) in [(60, 1, 0x0f), (2, 52, 0xf0), (60, 52, 0xff)] {
            let encoded = image(32768, 8192, prg, chr, high, trainer);
            let mut expected = parse_ines(&reference).expect("linear image");
            let mut actual = parse_ines(&encoded).expect("exponent image");
            for addr in 0x8000..=0xffff {
                assert_eq!(actual.mapper.cpu_read(addr), expected.mapper.cpu_read(addr));
            }
            for addr in 0..0x2000 {
                assert_eq!(actual.mapper.chr_read(addr), expected.mapper.chr_read(addr));
            }
        }
    }
}

#[test]
fn nes2_odd_multipliers_place_chr_after_the_exact_prg_extent() {
    // Literal specification examples: E=13 for PRG, E=12 for CHR.
    // Odd multipliers are 1, 3, 5 and 7; sizes are bytes, not bank counts.
    for (prg, chr, prg_size, chr_size) in [
        (52, 48, 8192, 4096),
        (53, 49, 24576, 12288),
        (54, 50, 40960, 20480),
        (55, 51, 57344, 28672),
    ] {
        let data = image(prg_size, chr_size, prg, chr, 0xff, true);
        let mut parsed = parse_ines(&data).expect("odd-sized image");
        assert_eq!(parsed.mapper.cpu_read(0x8000), 0xff);
        for bank in 0..4u8 {
            parsed.mapper.cpu_write(0x8000, bank);
            for addr in 0..0x2000u16 {
                let offset = (usize::from(bank) * 8192 + usize::from(addr)) % chr_size;
                assert_eq!(parsed.mapper.chr_read(addr), data[528 + prg_size + offset]);
            }
        }
        let truncated = &data[..data.len() - 1];
        let error = match parse_ines(truncated) {
            Ok(_) => panic!("one missing CHR byte must be rejected"),
            Err(error) => error,
        };
        assert!(error.contains("too short"), "{error}");
        assert!(error.contains(&data.len().to_string()), "{error}");
    }
}

#[test]
fn nes2_extreme_sizes_return_errors_without_overflow_or_allocating_payloads() {
    for (prg, chr, high) in [(0xff, 0, 0x0f), (1, 0xff, 0xf0), (0xfc, 0xfc, 0xff)] {
        let mut data = [0u8; 16];
        data[..4].copy_from_slice(b"NES\x1a");
        data[4] = prg;
        data[5] = chr;
        data[7] = 8;
        data[9] = high;
        assert!(parse_ines(&data).is_err());
    }
}

#[test]
fn nes2_partial_banks_do_not_construct_mappers_that_can_index_past_rom() {
    for (mapper, prg_size, chr_size, prg, chr, high) in [
        (0, 32768, 4096, 2, 48, 0xf0),
        (1, 8192, 8192, 52, 1, 0x0f),
        (2, 8192, 8192, 52, 1, 0x0f),
        (4, 4096, 8192, 48, 1, 0x0f),
        (5, 32768, 4096, 2, 48, 0xf0),
        (11, 16384, 8192, 56, 1, 0x0f),
        (22, 4096, 8192, 48, 1, 0x0f),
        (28, 8192, 8192, 52, 1, 0x0f),
        (34, 16384, 0, 56, 0, 0x0f),
        (34, 32768, 2048, 2, 44, 0xf0),
        (68, 32768, 1024, 2, 40, 0xf0),
        (71, 8192, 0, 52, 0, 0x0f),
    ] {
        let mut data = image(prg_size, chr_size, prg, chr, high, false);
        data[6] = (mapper & 15) << 4;
        data[7] = (mapper & 0xf0) | 8;
        match parse_ines(&data) {
            Err(error) => assert!(error.contains("mapper"), "{error}"),
            Ok(mut parsed) => {
                // Prove why accepting this shape is unsafe before the guard exists.
                for addr in 0x8000..=0xffff {
                    let _ = parsed.mapper.cpu_read(addr);
                }
                for addr in 0..0x2000 {
                    let _ = parsed.mapper.chr_read(addr);
                }
                panic!("mapper {mapper} unexpectedly accepted unsupported partial banks");
            }
        }
    }
}

#[test]
fn nes2_supported_mapper_windows_remain_readable_with_exponent_headers() {
    for mapper in [0, 1, 2, 3, 4, 5, 7, 11, 22, 28, 34, 68, 71] {
        for chr_rom in [false, true] {
            let chr_size = if chr_rom { 8192 } else { 0 };
            let mut data = image(
                32768,
                chr_size,
                60,
                if chr_rom { 52 } else { 0 },
                if chr_rom { 0xff } else { 0x0f },
                false,
            );
            data[6] = (mapper & 15) << 4;
            data[7] = (mapper & 0xf0) | 8;
            let mut parsed = parse_ines(&data).expect("supported full-bank image");
            for addr in 0x8000..=0xffff {
                let _ = parsed.mapper.cpu_read(addr);
            }
            for addr in 0..0x2000 {
                let _ = parsed.mapper.chr_read(addr);
            }
        }
    }
}

#[test]
fn nes2_linear_high_nibbles_keep_prg_and_chr_payloads_separate() {
    let data = image(4 * 1024 * 1024, 2 * 1024 * 1024, 0, 0, 0x11, true);
    let mut parsed = parse_ines(&data).expect("extended linear image");
    assert_eq!(parsed.mapper.cpu_read(0x8000), 0xff);
    parsed.mapper.cpu_write(0x8000, 255);
    assert_eq!(
        parsed.mapper.chr_read(0x1fff),
        *data.last().expect("CHR bytes")
    );
    assert!(parse_ines(&data[..data.len() - 1]).is_err());
}
