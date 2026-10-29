//! Exercise DOS seek, write/verify, readback and export with the local 1541 ROM.
use common_commodore_iec::IecBus;
use machine_commodore_1541::{Drive1541, Drive1541Config};

fn sectors_on_track(track: u8) -> usize {
    match track {
        1..=17 => 21,
        18..=24 => 19,
        25..=30 => 18,
        31..=35 => 17,
        _ => panic!("test track outside the standard D64 geometry"),
    }
}

fn sector_offset(track: u8, sector: u8) -> usize {
    ((1..track).map(sectors_on_track).sum::<usize>() + usize::from(sector)) * 256
}

fn run_job(drive: &mut Drive1541, bus: &mut IecBus, job: u8, track: u8, sector: u8) {
    drive.write_with_iec_bus(8, track, bus);
    drive.write_with_iec_bus(9, sector, bus);
    drive.write_with_iec_bus(1, job, bus);
    for _ in 0..5_000_000 {
        drive.tick_with_iec_bus(bus);
        if drive.peek(1) < 128 {
            break;
        }
    }
    assert_eq!(
        drive.peek(1),
        1,
        "job {job:02X}, track {track}, sector {sector}, PC {:04X}",
        drive.cpu().regs.pc
    );
    for _ in 0..20_000 {
        drive.tick_with_iec_bus(bus);
    }
}

#[test]
#[ignore = "FIXTURE: requires local 1541.rom; run with --ignored"]
fn direct_write_covers_density_boundaries_and_preserves_other_sectors() {
    let directory = std::env::var_os("EMU198X_C64_ROM_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                .join(".emu198x/roms/commodore-c64")
        });
    let rom = std::fs::read(directory.join("1541.rom")).expect("local 1541 ROM");
    let mut expected = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../runtime-commodore-c64/tests/fixtures/blank-formatted.d64"
    ))
    .expect("existing blank disk fixture");
    // Distinct neighbouring bytes catch writes to the wrong sector or track.
    for track in 1..=35 {
        for sector in 0..sectors_on_track(track) {
            if track == 18 {
                continue;
            }
            let start = sector_offset(track, sector as u8);
            for (index, value) in expected[start..start + 256].iter_mut().enumerate() {
                *value = track.wrapping_mul(37) ^ (sector as u8).wrapping_mul(11) ^ index as u8;
            }
        }
    }
    let mut bus = IecBus::new();
    let mut drive = Drive1541::new(Drive1541Config { dos_rom: &rom }).expect("valid ROM");
    drive
        .load_d64_bytes_writable(&expected, true)
        .expect("valid disk");
    for _ in 0..2_000_000 {
        drive.tick_with_iec_bus(&mut bus);
    }
    // I0 normally copies the disk's header ID into these DOS workspace bytes.
    drive.write_with_iec_bus(0x12, expected[357 * 256 + 162], &mut bus);
    drive.write_with_iec_bus(0x13, expected[357 * 256 + 163], &mut bus);
    for (case, track) in [1, 17, 18, 24, 25, 30, 31, 35].into_iter().enumerate() {
        for sector in [0, (sectors_on_track(track) - 1) as u8] {
            let payload: [u8; 256] = std::array::from_fn(|index| match case % 4 {
                0 => {
                    if sector == 0 {
                        0x00
                    } else {
                        0xFF
                    }
                }
                1 => {
                    if index % 2 == 0 {
                        0x55
                    } else {
                        0xAA
                    }
                }
                2 => index as u8,
                _ => (index as u8).wrapping_mul(73).wrapping_add(track ^ sector),
            });
            run_job(&mut drive, &mut bus, 0xB0, track, sector);
            for (index, value) in payload.iter().enumerate() {
                drive.write_with_iec_bus(0x400 + index as u16, *value, &mut bus);
            }
            run_job(&mut drive, &mut bus, 0x90, track, sector);
            let offset = sector_offset(track, sector);
            expected[offset..offset + 256].copy_from_slice(&payload);
            let exported = drive.flush_image().expect("writable disk");
            assert_eq!(exported, expected, "export after write {track}/{sector}");
            for address in 0x400..0x500 {
                drive.write_with_iec_bus(address, 0xEE, &mut bus);
            }
            run_job(&mut drive, &mut bus, 0x80, track, sector);
            let readback: [u8; 256] = std::array::from_fn(|index| drive.peek(0x400 + index as u16));
            assert_eq!(readback, payload, "ROM readback {track}/{sector}");
        }
    }
}
