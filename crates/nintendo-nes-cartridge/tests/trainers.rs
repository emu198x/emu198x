//! The optional trainer is initial RAM content, not padding before PRG ROM.
use nintendo_nes_cartridge::{mapper_from_snapshot, parse_ines};

fn image(mapper: u8, nes2: bool, battery: bool, trainer: bool) -> Vec<u8> {
    let mut rom = vec![0; 16];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 2;
    rom[5] = 1;
    rom[6] = (mapper << 4) | (u8::from(battery) * 2) | (u8::from(trainer) * 4);
    rom[7] = (mapper & 0xF0) | if nes2 { 8 } else { 0 };
    if nes2 {
        rom[10] = if battery { 0x70 } else { 7 }; // 8 KiB NVRAM or work RAM
    }
    if trainer {
        rom.extend((0..512).map(trainer_byte));
    }
    rom.extend(vec![0xA6; 32768]);
    rom.extend(vec![0xC3; 8192]);
    rom
}

fn trainer_byte(index: usize) -> u8 {
    (index as u8).wrapping_mul(37).wrapping_add(11)
}

#[test]
fn trainers_initialize_only_their_ram_window() {
    for mapper in [0, 1, 3, 4, 5, 34, 68] {
        for nes2 in [false, true] {
            for battery in [false, true] {
                for trainer in [false, true] {
                    let rom = image(mapper, nes2, battery, trainer);
                    let mut cart = parse_ines(&rom).expect("supported RAM-backed mapper");
                    assert_eq!(cart.has_battery, battery);
                    let ram = cart.mapper.save_ram();
                    assert!(ram.len() >= 0x2000, "fixture must have RAM");
                    for (i, byte) in ram.iter().enumerate() {
                        let expected = if trainer && (0x1000..0x1200).contains(&i) {
                            trainer_byte(i - 0x1000)
                        } else {
                            0
                        };
                        assert_eq!(
                            *byte, expected,
                            "mapper {mapper}, NES2 {nes2}, battery {battery}, trainer {trainer}, RAM {i:04x}"
                        );
                    }
                    assert_eq!(cart.mapper.cpu_read(0xFFFC), 0xA6);
                    assert_eq!(cart.mapper.chr_read(0), 0xC3);
                    assert_eq!(cart.mapper.chr_read(0x1FFF), 0xC3);
                    let restored = mapper_from_snapshot(cart.mapper.snapshot());
                    assert_eq!(restored.save_ram(), cart.mapper.save_ram());
                    assert_eq!(rom, image(mapper, nes2, battery, trainer));
                }
            }
        }
    }
}

#[test]
fn trainer_and_rom_truncation_is_rejected_before_loading_ram() {
    for nes2 in [false, true] {
        let rom = image(0, nes2, false, true);
        for size in [16, 527, 528, 528 + 32768 - 1, rom.len() - 1] {
            assert!(parse_ines(&rom[..size]).is_err(), "length {size}");
        }
    }
}

#[test]
fn existing_ramless_mappers_do_not_gain_synthetic_ram() {
    for mapper in [2, 7, 11, 22, 28, 71] {
        let cart = parse_ines(&image(mapper, false, false, true)).expect("existing mapper");
        assert!(cart.mapper.save_ram().is_empty());
    }
    let mut bxrom = image(34, false, false, true);
    bxrom[5] = 0;
    bxrom.truncate(16 + 512 + 32768);
    let cart = parse_ines(&bxrom).expect("existing BxROM");
    assert!(cart.mapper.save_ram().is_empty());
}
