//! Small reusable KS 1.2 ROM disassembly dump for the early boot / alert path.
//!
//! We now know the first shared KS 1.2 alert happens after Exec init, through
//! `$FC026E..$FC02A8` and `$FC30E4..$FC30EC`. Keep those ranges in-tree so we
//! can inspect them reproducibly instead of rediscovering them from raw PCs.

use std::path::PathBuf;

use motorola_68000::disasm::disassemble;

const KS12_ROM_BASE: u32 = 0x00FC_0000;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let path = home.join(".emu198x/roms/commodore-amiga/kick12.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.2 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.2 ROM"))
}

fn rom_byte(rom: &[u8], addr: u32) -> u8 {
    let idx = addr
        .checked_sub(KS12_ROM_BASE)
        .expect("address should be inside KS 1.2 ROM") as usize;
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
#[ignore = "needs KS 1.2 ROM locally"]
fn dump_ks12_early_boot_alert_regions() {
    let Some(rom) = load_kickstart() else { return };

    dump_region(&rom, "exec init tail before first alert", 0x00FC_026E, 0x00FC_02B0);
    dump_region(&rom, "slow RAM probe", 0x00FC_061A, 0x00FC_0690);
    dump_region(&rom, "warm-start validation branch into alert", 0x00FC_30E0, 0x00FC_30F4);
    dump_region(&rom, "exec init helper at FC0546", 0x00FC_0546, 0x00FC_05B4);
    dump_region(&rom, "early alert handler entry", 0x00FC_05B4, 0x00FC_05C8);
}
