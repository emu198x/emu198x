//! Diagnostic probe for the chip-RAM-only boot failure on Kickstart 1.3.
//!
//! See `wiki/decisions/amiga-chip-only-boot-failure.md`. Runs Kickstart 1.3
//! to frame 250 in two configurations (chip-only vs. chip+slow) and prints
//! comparable state to stderr so we can spot where boot diverges.
//!
//! Run with:
//!   cargo test -p machine-commodore-amiga --test chip_only_boot_probe \
//!     -- --ignored --nocapture

use std::path::PathBuf;

use machine_commodore_amiga::Amiga;

const KICKSTART_BOOT_FRAMES: u64 = 250;

fn kickstart13_path() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME is set");
    PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom")
}

fn read_long(amiga: &Amiga, addr: u32) -> u32 {
    (u32::from(amiga.memory.read_word(addr)) << 16)
        | u32::from(amiga.memory.read_word(addr.wrapping_add(2)))
}

fn dump(label: &str, amiga: &Amiga) {
    eprintln!("===== {label} =====");
    eprintln!(
        "frame: cck_count={} vertb_count={} reset_count={}",
        amiga.cck_count, amiga.vertb_count, amiga.reset_count
    );
    eprintln!(
        "cpu: PC=${:08X} SR=${:04X} SSP=${:08X} USP=${:08X}",
        amiga.cpu.regs.pc, amiga.cpu.regs.sr, amiga.cpu.regs.ssp, amiga.cpu.regs.usp,
    );

    let dmacon = amiga.agnus.dmacon;
    let bplcon0 = amiga.agnus.bplcon0;
    let intena = amiga.paula.intena;
    let intreq = amiga.paula.intreq;
    eprintln!(
        "agnus/paula: DMACON=${dmacon:04X} BPLCON0=${bplcon0:04X} INTENA=${intena:04X} INTREQ=${intreq:04X}"
    );

    let cop1lc = amiga.copper.cop1lc;
    let cop_pc = amiga.copper.pc;
    eprintln!("copper: COP1LC=${cop1lc:08X} pc=${cop_pc:08X}");

    let color00 = amiga.denise.palette[0];
    let color01 = amiga.denise.palette[1];
    eprintln!("denise: COLOR00=${color00:04X} COLOR01=${color01:04X}");

    // Vector table samples
    let ssp = read_long(amiga, 0x000000);
    let pc_vec = read_long(amiga, 0x000004);
    let bus_err = read_long(amiga, 0x000008);
    let addr_err = read_long(amiga, 0x00000C);
    eprintln!(
        "vectors: $0=SSP=${ssp:08X} $4=PC=${pc_vec:08X} $8=BUSERR=${bus_err:08X} $C=ADDRERR=${addr_err:08X}"
    );

    let exec_base = pc_vec; // ExecBase pointer is at $00000004
    eprintln!("ExecBase candidate: ${exec_base:08X}");

    // Candidate ExecBase region — sample a few bytes
    if (0x00_0400..0x10_0000).contains(&exec_base) || (0xC0_0000..0xC8_0000).contains(&exec_base) {
        let chk_base = read_long(amiga, exec_base.wrapping_add(38));
        let cold_capture = read_long(amiga, exec_base.wrapping_add(42));
        let max_loc_mem = read_long(amiga, exec_base.wrapping_add(62));
        let last_alert = read_long(amiga, exec_base.wrapping_add(0x202));
        eprintln!(
            "ExecBase fields: ChkBase=${chk_base:08X} ColdCapture=${cold_capture:08X} MaxLocMem=${max_loc_mem:08X} LastAlert=${last_alert:08X}"
        );

        // MemList header pointer (Exec MemList at +322)
        let mem_list_head = read_long(amiga, exec_base.wrapping_add(322));
        let mem_list_tail_pred = read_long(amiga, exec_base.wrapping_add(322 + 8));
        eprintln!(
            "MemList: head=${mem_list_head:08X} tail_pred=${mem_list_tail_pred:08X}"
        );

        // Walk first MemHeader if valid
        if mem_list_head != 0
            && ((0x00_0400..0x10_0000).contains(&mem_list_head)
                || (0xC0_0000..0xC8_0000).contains(&mem_list_head))
        {
            // MemHeader: Node(14) + mh_Attributes(2) + mh_First(4)
            //   + mh_Lower(4) + mh_Upper(4) + mh_Free(4) = 32 bytes
            let mh_succ = read_long(amiga, mem_list_head); // ln_Succ
            let mh_attrs = amiga.memory.read_word(mem_list_head.wrapping_add(14));
            let mh_first = read_long(amiga, mem_list_head.wrapping_add(16));
            let mh_lower = read_long(amiga, mem_list_head.wrapping_add(20));
            let mh_upper = read_long(amiga, mem_list_head.wrapping_add(24));
            let mh_free = read_long(amiga, mem_list_head.wrapping_add(28));
            eprintln!(
                "MemHeader#1 @${mem_list_head:08X}: succ=${mh_succ:08X} attrs=${mh_attrs:04X} first=${mh_first:08X} lower=${mh_lower:08X} upper=${mh_upper:08X} free=${mh_free:08X}"
            );

            // Walk if succ valid
            if mh_succ != 0
                && ((0x00_0400..0x10_0000).contains(&mh_succ)
                    || (0xC0_0000..0xC8_0000).contains(&mh_succ))
            {
                let attrs2 = amiga.memory.read_word(mh_succ.wrapping_add(14));
                let lower2 = read_long(amiga, mh_succ.wrapping_add(20));
                let upper2 = read_long(amiga, mh_succ.wrapping_add(24));
                let free2 = read_long(amiga, mh_succ.wrapping_add(28));
                eprintln!(
                    "MemHeader#2 @${mh_succ:08X}: attrs=${attrs2:04X} lower=${lower2:08X} upper=${upper2:08X} free=${free2:08X}"
                );
            }
        }
    } else {
        eprintln!("(ExecBase pointer outside expected ranges)");
    }

    // Diagnostic display content quick-check
    let fb = amiga.framebuffer();
    let non_black = fb.iter().filter(|&&p| (p & 0x00FF_FFFF) != 0).count();
    eprintln!("framebuffer: {non_black} non-black pixels of {}", fb.len());

    eprintln!();
}

fn run(slow_ram: usize) -> Option<Amiga> {
    let path = kickstart13_path();
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    let kickstart = std::fs::read(&path).expect("read Kickstart 1.3 ROM");
    let mut amiga = if slow_ram == 0 {
        Amiga::new(kickstart)
    } else {
        Amiga::new_with_slow_ram(kickstart, slow_ram)
    };
    for _ in 0..KICKSTART_BOOT_FRAMES {
        amiga.run_frame();
    }
    Some(amiga)
}

#[test]
#[ignore]
fn probe_chip_only_vs_slow_ram() {
    let Some(chip_only) = run(0) else {
        return;
    };
    let Some(with_slow) = run(512 * 1024) else {
        return;
    };

    dump("chip-only (no slow RAM)", &chip_only);
    dump("chip + 512K slow RAM", &with_slow);
}
