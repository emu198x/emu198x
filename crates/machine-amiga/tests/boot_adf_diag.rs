//! ADF disk boot test.
//!
//! Boots Kickstart 1.3 on an A500 PAL with a bootable ADF image and
//! verifies the boot code executes. This validates the full disk read
//! pipeline: floppy motor spin-up, raw MFM DMA, sector decode with
//! checksum verification, bootblock validation, and boot code execution.

mod common;

use common::load_rom;
use machine_amiga::format_adf::{Adf, ADF_SIZE_DD};
use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};

/// Create a bootable ADF whose boot code writes $DEADBEEF to $7FC00.
fn make_bootable_adf() -> Adf {
    let mut data = vec![0u8; ADF_SIZE_DD];

    // DOS\0 header
    data[0] = b'D';
    data[1] = b'O';
    data[2] = b'S';
    data[3] = 0;

    // Root block pointer (standard: 880)
    let root_block: u32 = 880;
    data[8] = (root_block >> 24) as u8;
    data[9] = (root_block >> 16) as u8;
    data[10] = (root_block >> 8) as u8;
    data[11] = root_block as u8;

    // Boot code at offset 12:
    //   MOVE.L  #$DEADBEEF, ($7FC00).L
    //   MOVEQ   #0, D0      ; success
    //   RTS
    let code: &[u8] = &[
        0x23, 0xFC, // MOVE.L #imm, (xxx).L
        0xDE, 0xAD, 0xBE, 0xEF, //   #$DEADBEEF
        0x00, 0x07, 0xFC, 0x00, //   $0007FC00
        0x70, 0x00, // MOVEQ #0, D0
        0x4E, 0x75, // RTS
    ];
    data[12..12 + code.len()].copy_from_slice(code);

    // Bootblock checksum: sum of all 256 longwords (with carry) must be $FFFFFFFF.
    let mut sum: u32 = 0;
    for i in 0..256 {
        let offset = i * 4;
        let long = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        let (new_sum, carry) = sum.overflowing_add(long);
        sum = new_sum;
        if carry {
            sum = sum.wrapping_add(1);
        }
    }
    let checksum = (!sum).to_be_bytes();
    data[4..8].copy_from_slice(&checksum);

    Adf::from_bytes(data).expect("valid DD ADF")
}

/// Boot KS 1.3 A500 with a bootable ADF and verify the boot code runs.
#[test]
#[ignore]
fn test_adf_boot_executes_bootblock() {
    let Some(rom) = load_rom("../../roms/kick13.rom") else {
        return;
    };

    let mut amiga = Amiga::new_with_config(AmigaConfig {
        model: AmigaModel::A500,
        chipset: AmigaChipset::Ocs,
        region: AmigaRegion::Pal,
        kickstart: rom,
        slow_ram_size: 512 * 1024,
    });
    amiga.insert_disk(make_bootable_adf());

    // Boot for ~10 seconds — enough for motor spin-up + bootblock read + execution.
    let total_ticks: u64 = 300_000_000;
    let report_interval: u64 = 28_375_160;
    let mut last_report = 0u64;

    for i in 0..total_ticks {
        amiga.tick();

        if i % 4 != 0 || i - last_report < report_interval {
            continue;
        }
        last_report = i;
        println!(
            "[{:.1}s] PC=${:08X} cyl={} motor={} ready={}",
            i as f64 / 28_375_160.0,
            amiga.cpu.regs.pc,
            amiga.floppy.cylinder(),
            amiga.floppy.motor_on(),
            amiga.floppy.status().ready,
        );
    }

    // Verify boot code executed by checking for the $DEADBEEF signature.
    let sig = (u32::from(amiga.memory.read_chip_byte(0x7FC00)) << 24)
        | (u32::from(amiga.memory.read_chip_byte(0x7FC01)) << 16)
        | (u32::from(amiga.memory.read_chip_byte(0x7FC02)) << 8)
        | u32::from(amiga.memory.read_chip_byte(0x7FC03));

    assert_eq!(
        sig, 0xDEADBEEF,
        "Boot code should have written $DEADBEEF to $7FC00 — ADF boot failed"
    );
    println!("ADF boot successful: signature $DEADBEEF found at $7FC00");
}
