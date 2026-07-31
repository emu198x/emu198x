//! Diagnostic: sample CPU PC at the end of a no-disk boot so we
//! can compare where the CPU idles with disk vs. without.

use std::path::PathBuf;

use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaOcsRuntime, Model};

#[test]
#[ignore = "explicit 900-frame no-disk PC diagnostic"]
fn no_disk_final_pc() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let rom_path = home.join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !rom_path.exists() {
        eprintln!("skipping: no kick13.rom");
        return;
    }
    let rom = std::fs::read(&rom_path).expect("read ROM");
    let mut rt = AmigaOcsRuntime::new(Model::A500OcsPalA501, rom).expect("build");

    // Match the wb13 settle count so the comparison is apples-to-apples.
    for _ in 0..(900 * A500_PAL_FRAME_TICKS) {
        rt.machine_mut().tick();
    }

    let m = rt.machine();
    let pc = m.cpu().regs.pc;
    let a0 = m.cpu().regs.a[0];
    let a6 = m.cpu().regs.a[6];
    println!(
        "no-disk final: pc=${pc:06X} a0=${a0:08X} a6=${a6:08X} \
         intena=${:04X} intreq=${:04X}",
        m.intena(),
        m.intreq()
    );
    // Bytes at PC — same window as wb13 diag for easy compare.
    let base = pc.saturating_sub(40);
    for row in 0..4 {
        let row_base = base + row * 16;
        let mut line = format!("  ${row_base:06X}: ");
        for i in 0..16 {
            let b = m.memory().read_byte(row_base + i);
            line.push_str(&format!("{b:02X} "));
        }
        println!("{line}");
    }
}
