//! Inspect D2 and memory at (A1+$28) right before the AND.B instruction
//! at $FC469A in ciaa.resource handler. That AND is the gate between
//! "ICR bit set" and "dispatch to sub-handler". On the second Timer B
//! fire it unexpectedly yields zero.

use emu198x_shell::{MediaKind, read_media_asset};
use machine_commodore_amiga::Amiga;
use std::fs;
use std::path::Path;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);
    let disk_path = "/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip";
    let loaded = read_media_asset(Path::new(disk_path), MediaKind::Disk).unwrap();
    let adf = format_commodore_amiga_adf::Adf::from_bytes(loaded.bytes).unwrap();
    amiga.insert_disk(adf);
    amiga.floppy.acknowledge_disk_change();
    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    // Capture at the AND.B $28(A1), D2 instruction: $FC469A.
    const AND_PC: u32 = 0x00FC469A;

    let mut samples: Vec<(u64, u32, u8, u8, u32)> = Vec::new();
    let mut prev_hit = false;

    for tick in 0..(500 * ccks_per_frame) {
        amiga.tick_cck();
        let pc = amiga.cpu.instr_start_pc;
        if pc == AND_PC && !prev_hit {
            let d2 = amiga.cpu.regs.d[2] as u32;
            let a1 = amiga.cpu.regs.a[1];
            let byte_28 = amiga.memory.read_byte(a1.wrapping_add(0x28));
            let byte_29 = amiga.memory.read_byte(a1.wrapping_add(0x29));
            samples.push((tick, a1, byte_28, byte_29, d2));
        }
        prev_hit = pc == AND_PC;
    }

    println!("Samples of D2, A1+$28 (enable), A1+$29 (pending) at $FC469A (AND.B):");
    for (tick, a1, e, p, d2) in &samples {
        let d2_low = (*d2) & 0xFF;
        let and_result = d2_low & (*e as u32);
        println!(
            "  tick={tick} A1=${a1:08X} enable(+$28)=${e:02X} pending(+$29)=${p:02X} D2=${d2:08X} D2.b=${d2_low:02X} AND→${and_result:02X}"
        );
    }
}
