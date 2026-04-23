//! Task #96 bisect: capture d0 at every JSR return point in the
//! $FE8498..$FE8562 window of romboot, comparing chip-only and slow-
//! RAM. The first divergent d0 is the bug.
//!
//! JSRs identified by disassembly:
//!   $FE84AE post FindTask(NULL)        LVO -294
//!   $FE84CE post AllocSignal(-1)       LVO -330
//!   $FE8506 post OpenDevice(trackdisk) LVO -444
//!   $FE8536 post OpenLibrary("romboot.") LVO -552
//!   $FE8542 post Supervisor(...)       LVO -30 (on the opened lib)
//!   $FE854A post CloseLibrary          LVO -414
//!   $FE8560 post DoIO(CMD_CLEAR=5)     LVO -456  ← prime suspect

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const RETURN_POINTS: &[(u32, &str)] = &[
    (0x00FE_84AE, "post FindTask(NULL)"),
    (0x00FE_84CE, "post AllocSignal(-1)"),
    (0x00FE_8506, "post OpenDevice(trackdisk, 0, IOReq, 0)"),
    (0x00FE_8536, "post OpenLibrary(\"romboot.\")"),
    (0x00FE_8542, "post (lib)@(-30) call"),
    (0x00FE_854A, "post CloseLibrary"),
    (0x00FE_8560, "post DoIO(io_Cmd=5 / CMD_CLEAR)"),
    (0x00FE_9046, "trackdisk BeginIO entry"),
    (0x00FE_8574, "post DoIO(io_Cmd=13 / TD_CHANGESTATE)"),
    (0x00FE_85A0, "post DoIO(io_Cmd=2 / CMD_READ)"),
    (0x00FE_867C, "exit path — error bail"),
    (0x00FE_85F0, "success path — resident init"),
];

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn run(label: &str, use_slow_ram: bool) {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = if use_slow_ram {
        AmigaOcs::with_slow_ram(rom, 512 * 1024)
    } else {
        AmigaOcs::new(rom)
    };
    // (pc_idx, frame, d0) on every first hit of each point
    let mut first_hits: Vec<Option<(u64, u32)>> = vec![None; RETURN_POINTS.len()];
    let mut prev_pc = amiga.cpu().regs.pc;
    for tick in 0..(600u64 * PAL_FRAME_TICKS) {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        for (i, (tpc, _)) in RETURN_POINTS.iter().enumerate() {
            if pc == *tpc && first_hits[i].is_none() {
                let d0 = amiga.cpu().regs.d[0];
                let frame = tick / PAL_FRAME_TICKS;
                first_hits[i] = Some((frame, d0));
            }
        }
        prev_pc = pc;
    }
    eprintln!("\n########## {label} ##########");
    for ((pc, desc), hit) in RETURN_POINTS.iter().zip(first_hits.iter()) {
        match hit {
            Some((frame, d0)) => eprintln!("  ${pc:08X} frame~{frame:<3}  d0=${d0:08X}  {desc}"),
            None => eprintln!("  ${pc:08X}  <NEVER REACHED>        {desc}"),
        }
    }
}

#[test]
#[ignore]
fn bisect_romboot_d0() {
    run("slow-RAM", true);
    run("chip-only", false);
}
