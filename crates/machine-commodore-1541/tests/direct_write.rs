//! Exercise the DOS write/verify job with local ROMs, without a host BASIC loop.
use common_commodore_iec::IecBus;
use machine_commodore_1541::{Drive1541, Drive1541Config};

#[test]
#[ignore = "FIXTURE: requires local 1541.rom; run with --ignored"]
fn direct_write_completes_and_preserves_sector_data() {
    let directory = std::env::var_os("EMU198X_C64_ROM_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                .join(".emu198x/roms/commodore-c64")
        });
    let rom = std::fs::read(directory.join("1541.rom")).expect("local 1541 ROM");
    let disk = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../runtime-commodore-c64/tests/fixtures/blank-formatted.d64"
    ))
    .expect("existing blank disk fixture");
    let mut bus = IecBus::new();
    let mut drive = Drive1541::new(Drive1541Config { dos_rom: &rom }).expect("valid ROM");
    drive
        .load_d64_bytes_writable(&disk, true)
        .expect("valid disk");
    for _ in 0..2_000_000 {
        drive.tick_with_iec_bus(&mut bus);
    }
    // The host's I0 normally copies the header ID from the BAM into these bytes.
    drive.write_with_iec_bus(0x12, disk[357 * 256 + 162], &mut bus);
    drive.write_with_iec_bus(0x13, disk[357 * 256 + 163], &mut bus);
    for address in 0x400..0x500 {
        drive.write_with_iec_bus(address, 15, &mut bus);
    }
    drive.write_with_iec_bus(8, 1, &mut bus);
    drive.write_with_iec_bus(9, 0, &mut bus);
    drive.write_with_iec_bus(1, 0xB0, &mut bus);
    for _ in 0..2_000_000 {
        drive.tick_with_iec_bus(&mut bus);
        if drive.peek(1) < 128 {
            break;
        }
    }
    assert_eq!(
        drive.peek(1),
        1,
        "seek result at PC {:04X}",
        drive.cpu().regs.pc
    );
    for _ in 0..20_000 {
        drive.tick_with_iec_bus(&mut bus);
    }
    drive.write_with_iec_bus(1, 0x90, &mut bus);
    for _ in 0..5_000_000 {
        drive.tick_with_iec_bus(&mut bus);
        if drive.peek(1) < 128 {
            break;
        }
    }
    assert_eq!(drive.peek(1), 1, "write/verify must finish successfully");
    let exported = drive.flush_image().expect("writable disk");
    assert_eq!(&exported[..256], &[15; 256]);
}
