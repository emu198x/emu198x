//! Small reusable KS 1.3 ROM disassembly dump for the WB 1.3 boot path.
//!
//! We repeatedly end up re-disassembling the same STRAP / trackdisk / Exec
//! ranges by hand while debugging Workbench boot. This test keeps the ranges
//! and the disassembler entry point in-tree so those snippets are reproducible.

use std::path::PathBuf;

use motorola_68000::disasm::disassemble;

const KS13_ROM_BASE: u32 = 0x00FC_0000;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let path = home.join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        emu198x_test_skip::record(&format!(
            "skipping: Kickstart 1.3 ROM missing at {}",
            path.display()
        ));
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn rom_byte(rom: &[u8], addr: u32) -> u8 {
    let idx = addr
        .checked_sub(KS13_ROM_BASE)
        .expect("address should be inside KS 1.3 ROM") as usize;
    rom[idx]
}

fn raw_words(rom: &[u8], addr: u32, len: u8) -> String {
    let mut out = String::new();
    let words = usize::from(len.max(2)) / 2;
    for i in 0..words {
        if i != 0 {
            out.push(' ');
        }
        let base = addr + (i as u32) * 2;
        let hi = rom_byte(rom, base);
        let lo = rom_byte(rom, base + 1);
        out.push_str(&format!("{hi:02X}{lo:02X}"));
    }
    out
}

fn dump_region(rom: &[u8], label: &str, start: u32, end: u32) {
    println!("\n=== {label} ${start:08X}..${end:08X} ===");
    let mut pc = start;
    while pc < end {
        let (mnemonic, len) = disassemble(pc, |addr| rom_byte(rom, addr));
        let len = len.max(2);
        let words = raw_words(rom, pc, len);
        println!("  ${pc:08X}: {:<19} {mnemonic}", words);
        pc = pc.wrapping_add(u32::from(len));
    }
}

#[test]
#[ignore = "FIXTURE: needs KS 1.3 ROM locally"]
fn dump_ks13_trackdisk_bootblock_regions() {
    let Some(rom) = load_kickstart() else { return };

    dump_region(
        &rom,
        "exec dispatch / idle path around task-state writers",
        0x00FC_0EF0,
        0x00FC_0FD0,
    );
    dump_region(
        &rom,
        "exec idle loop around hot PC",
        0x00FC_0F80,
        0x00FC_0FA8,
    );
    dump_region(
        &rom,
        "exec signal delivery / readying path",
        0x00FC_1E70,
        0x00FC_1EE0,
    );
    dump_region(&rom, "exec Wait / scheduler tail", 0x00FC_1EF0, 0x00FC_1F64);
    dump_region(
        &rom,
        "exec list-link helpers used during IDCMP setup",
        0x00FC_1680,
        0x00FC_171C,
    );
    dump_region(
        &rom,
        "exec struct clear helper used during IDCMP setup",
        0x00FC_1808,
        0x00FC_1824,
    );
    dump_region(
        &rom,
        "exec msg-port list init helper",
        0x00FC_1B54,
        0x00FC_1B6C,
    );
    dump_region(
        &rom,
        "STRAP bootblock read / retry loop",
        0x00FE_8570,
        0x00FE_8604,
    );
    dump_region(
        &rom,
        "trackdisk READ validate/extract loop",
        0x00FE_A480,
        0x00FE_A5D8,
    );
    dump_region(
        &rom,
        "trackdisk request-block handoff",
        0x00FE_A39A,
        0x00FE_A3D6,
    );
    dump_region(
        &rom,
        "trackdisk READ decode blit setup",
        0x00FE_A932,
        0x00FE_A9C0,
    );
    dump_region(&rom, "trackdisk validation path", 0x00FE_AC62, 0x00FE_AD28);
    dump_region(
        &rom,
        "exec block init around later one-sector limit",
        0x00FF_4408,
        0x00FF_4528,
    );
    dump_region(
        &rom,
        "exec validator dispatch around one-sector request",
        0x00FF_45C0,
        0x00FF_4610,
    );
    dump_region(
        &rom,
        "exec validator helper stubs",
        0x00FF_4128,
        0x00FF_4148,
    );
    dump_region(
        &rom,
        "exec file-system helper around FF4648",
        0x00FF_4640,
        0x00FF_46A8,
    );
    dump_region(
        &rom,
        "exec validator helper around FF4E24",
        0x00FF_4E10,
        0x00FF_4E34,
    );
    dump_region(&rom, "validator late wait site", 0x00FE_0230, 0x00FE_0260);
    dump_region(
        &rom,
        "validator IDCMP owner-struct writes",
        0x00FD_56F0,
        0x00FD_5768,
    );
    dump_region(
        &rom,
        "validator signal-port poll / empty wait path",
        0x00FD_E3B8,
        0x00FD_E408,
    );
    dump_region(
        &rom,
        "validator requester-state path into IDCMP setup",
        0x00FD_EDD0,
        0x00FD_EEA0,
    );
    dump_region(
        &rom,
        "validator requester helper around FDF266",
        0x00FD_F250,
        0x00FD_F2D0,
    );
    dump_region(
        &rom,
        "validator IDCMP helper around FDED70",
        0x00FD_ED68,
        0x00FD_ED98,
    );
    dump_region(
        &rom,
        "validator IDCMP helper around FDEFF8",
        0x00FD_EFEC,
        0x00FD_F004,
    );
    dump_region(
        &rom,
        "graphics copper-list slot clear helper around FC81F6",
        0x00FC_81E0,
        0x00FC_8218,
    );
    dump_region(
        &rom,
        "graphics copper-list build helper around FD1728",
        0x00FD_1700,
        0x00FD_1760,
    );
    dump_region(
        &rom,
        "graphics copper-template builder around FCC780",
        0x00FC_C780,
        0x00FC_C940,
    );
    dump_region(
        &rom,
        "graphics display-mode template builder around FCC940",
        0x00FC_C940,
        0x00FC_D000,
    );
    dump_region(
        &rom,
        "graphics copper-template patch helper around FCFFE0",
        0x00FC_FFE0,
        0x00FD_0018,
    );
    dump_region(
        &rom,
        "graphics mode-word init helper around FCADE0",
        0x00FC_ADE0,
        0x00FC_AE60,
    );
    dump_region(
        &rom,
        "graphics mode-word compute helper around FCAF96",
        0x00FC_AF80,
        0x00FC_B040,
    );
    dump_region(
        &rom,
        "graphics copper-template init helper around FE3908",
        0x00FE_3908,
        0x00FE_3930,
    );
    dump_region(
        &rom,
        "graphics copper-template alloc helper around FF456C",
        0x00FF_456C,
        0x00FF_4590,
    );
    dump_region(
        &rom,
        "graphics copper-template clear helper around FF4B38",
        0x00FF_4B38,
        0x00FF_4B5C,
    );
    dump_region(
        &rom,
        "validator IDCMP port-name / signal-bit setup",
        0x00FE_00A4,
        0x00FE_0124,
    );
    dump_region(
        &rom,
        "validator IDCMP helper around FE01BC",
        0x00FE_0190,
        0x00FE_0228,
    );
    dump_region(
        &rom,
        "validator IDCMP helper stubs around FE0284",
        0x00FE_0280,
        0x00FE_02A8,
    );
    dump_region(
        &rom,
        "input.device late wait site",
        0x00FE_5F20,
        0x00FE_5F50,
    );
    dump_region(&rom, "trackdisk late wait site", 0x00FE_AAE0, 0x00FE_AB10);
}
