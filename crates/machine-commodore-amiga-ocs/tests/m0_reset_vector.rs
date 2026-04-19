//! M0: After reset, the CPU's SSP and PC reflect the ROM reset vector.
//!
//! Per `wiki/decisions/amiga-restart-plan.md` — first milestone of the
//! Amiga restart. Verifies the foundational triple of:
//!   - 256 KiB Kickstart ROM loads at $FC0000 with mirror filling
//!     $F80000-$FFFFFF.
//!   - OVL=1 (default after reset) maps the ROM into low chip RAM
//!     space, so reads from $0/$4 return ROM bytes.
//!   - 68000 CPU initialises from those reset-vector reads.
//!
//! No chip RAM, no chipset, no CIAs. Just CPU + ROM + OVL mapping.

use std::path::PathBuf;

use machine_commodore_amiga_ocs::AmigaOcs;

const KICKSTART_PATH: &str = ".emu198x/roms/commodore-amiga/kick13.rom";

fn kickstart_path() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME is set");
    PathBuf::from(home).join(KICKSTART_PATH)
}

fn load_kickstart() -> Option<Vec<u8>> {
    let path = kickstart_path();
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

#[test]
fn cpu_initialises_with_reset_vector_from_rom() {
    let Some(rom) = load_kickstart() else { return };

    // Reset vector lives in the first 8 bytes of ROM:
    //   bytes 0..4 = SSP (initial supervisor stack pointer)
    //   bytes 4..8 = PC  (initial program counter)
    let expected_ssp = u32::from_be_bytes([rom[0], rom[1], rom[2], rom[3]]);
    let expected_pc = u32::from_be_bytes([rom[4], rom[5], rom[6], rom[7]]);

    let amiga = AmigaOcs::new(rom);

    assert_eq!(
        amiga.cpu().regs.ssp,
        expected_ssp,
        "CPU SSP should match ROM[0..4]"
    );
    assert_eq!(
        amiga.cpu().regs.pc,
        expected_pc,
        "CPU PC should match ROM[4..8]"
    );
}

#[test]
fn rom_mirrors_into_low_memory_via_ovl() {
    let Some(rom) = load_kickstart() else { return };
    let amiga = AmigaOcs::new(rom.clone());

    // OVL=1 by default after reset: reads from $0..$3FFFF should
    // return ROM bytes. With a 256K ROM the entire OVL window is
    // covered by the ROM; for 512K ROMs only the lower half maps.
    for offset in [0x00_0000u32, 0x00_0004, 0x00_00FE, 0x00_FFFE] {
        let rom_idx = (offset as usize) % rom.len();
        let expected = u16::from_be_bytes([rom[rom_idx], rom[rom_idx + 1]]);
        assert_eq!(
            amiga.read_word(offset),
            expected,
            "OVL read at ${offset:06X} should return ROM byte"
        );
    }
}

#[test]
fn rom_visible_at_high_address_range() {
    let Some(rom) = load_kickstart() else { return };
    let amiga = AmigaOcs::new(rom.clone());

    // ROM is anchored at $FC0000 (256K ROM). Reads from $FC0000+
    // return ROM bytes directly.
    for (off, rom_idx) in [
        (0xFC_0000u32, 0usize),
        (0xFC_0004, 4),
        (0xFC_00D2, 0xD2),
        (0xFF_FFFE, rom.len() - 2),
    ] {
        let expected = u16::from_be_bytes([rom[rom_idx], rom[rom_idx + 1]]);
        assert_eq!(
            amiga.read_word(off),
            expected,
            "Read at ${off:06X} should return ROM byte"
        );
    }
}

#[test]
fn unmapped_reads_return_floating_bus() {
    let Some(rom) = load_kickstart() else { return };
    let amiga = AmigaOcs::new(rom);

    // Real A500 unmapped reads return floating-bus value, typically
    // $FF when nothing drives the bus low. Anything in chipset /
    // expansion space that isn't yet mapped should read as $FF.
    for off in [0xC0_0000u32, 0xA0_0000, 0xDC_0000] {
        assert_eq!(
            amiga.read_word(off),
            0xFFFF,
            "Unmapped read at ${off:06X} should return floating-bus $FFFF"
        );
    }
}
