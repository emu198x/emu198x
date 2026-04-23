//! Diagnostic: reproduce the current KS 1.2 / Workbench 1.2 yellow-screen
//! failure on the smallest path that still shows it.
//!
//! This deliberately uses a plain A500 instead of the real A1000 bootstrap
//! route. If the same alert lands here, the remaining bug is in the shared
//! KS 1.2 / WB 1.2 boot path rather than in the A1000 bootstrap / WOM handoff.
//!
//! Run with:
//!   cargo test -p runtime-commodore-amiga --test diag_kick12_wb12_alert \
//!       -- --ignored --nocapture

use std::path::PathBuf;
use std::collections::VecDeque;

use format_commodore_amiga_adf::Adf;
use machine_commodore_amiga_ocs::{AmigaOcs, RamConfig};
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaRuntime, Model};

const KS12_ALERT_ENTRY: u32 = 0x00FC_05B4;
const KS12_ALERT_DIRECT_ENTRY: u32 = 0x00FC_05B8;
const KS12_ALERT_LOOP: u32 = 0x00FC_05DE;
const KS12_COLD_START: u32 = 0x00FC_01CE;
const KS12_CHIP_PROBE_ENTRY: u32 = 0x00FC_0208;
const KS12_CHIP_PROBE_RETURN: u32 = 0x00FC_021A;
const KS12_CHIP_PROBE_BRANCH: u32 = 0x00FC_0220;
const KS12_CHIP_ALERT_SETUP: u32 = 0x00FC_0238;
const KS12_EXEC_INIT: u32 = 0x00FC_0240;
const KS12_SLOW_PROBE_ENTRY: u32 = 0x00FC_061A;
const KS12_SLOW_PROBE_EXIT: u32 = 0x00FC_068E;
const KS12_SLOW_PROBE_RETURN: u32 = 0x00FC_01EA;
const MAX_FRAMES: u64 = 4000;

fn load_artifact(path: PathBuf, label: &str) -> Option<Vec<u8>> {
    if !path.exists() {
        eprintln!("skipping: {label} missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display())))
}

fn read_long(runtime: &AmigaRuntime, addr: u32) -> u32 {
    let hi = u32::from(runtime.machine().read_word(addr));
    let lo = u32::from(runtime.machine().read_word(addr.wrapping_add(2)));
    (hi << 16) | lo
}

fn read_word(runtime: &AmigaRuntime, addr: u32) -> u16 {
    runtime.machine().read_word(addr)
}

#[test]
#[ignore = "needs local kick12.rom and workbench-1.2.adf"]
fn trace_a500_kick12_wb12_first_alert() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(
        home.join(".emu198x/roms/commodore-amiga/kick12.rom"),
        "kick12.rom",
    ) else {
        return;
    };
    let Some(adf_bytes) = load_artifact(
        home.join(".emu198x/media/commodore-amiga/workbench-1.2.adf"),
        "workbench-1.2.adf",
    ) else {
        return;
    };

    let mut runtime = AmigaRuntime::new(Model::A500OcsPal, rom).expect("build KS 1.2 runtime");
    let adf = Adf::from_bytes(adf_bytes).expect("decode Workbench 1.2 ADF");
    runtime.machine_mut().insert_adf(adf);

    let mut prev_pc = runtime.machine().cpu().regs.pc;
    let mut alert_seen = false;

    for tick in 0..(MAX_FRAMES * A500_PAL_FRAME_TICKS) {
        runtime.machine_mut().tick();
        let amiga = runtime.machine();
        let regs = amiga.cpu().regs;
        let pc = regs.pc;

        if !alert_seen
            && (pc == KS12_ALERT_ENTRY || pc == KS12_ALERT_DIRECT_ENTRY || pc == KS12_ALERT_LOOP)
        {
            let frame = tick / A500_PAL_FRAME_TICKS;
            alert_seen = true;
            println!(
                "first alert transition at frame {} tick {}: prev_pc=${prev_pc:08X} pc=${pc:08X}",
                frame + 1,
                tick + 1,
            );
            println!(
                "  d0=${:08X} d6=${:08X} d7=${:08X} a5=${:08X} a6=${:08X} sr=${:04X}",
                regs.d[0], regs.d[6], regs.d[7], regs.a[5], regs.a[6], regs.sr
            );
            println!(
                "  color00=${:03X} color01=${:03X} bplcon0=${:04X}",
                amiga.color(0),
                amiga.color(1),
                amiga.bplcon0()
            );
            println!(
                "  disk: cyl={} head={} motor_on={} motor_spinning={} change_pending={} step_events={}",
                amiga.drive().cylinder(),
                amiga.drive().head(),
                amiga.drive().motor_on(),
                amiga.drive().motor_spinning(),
                amiga.drive().status().disk_change,
                amiga.drive().step_event_counter(),
            );
            println!(
                "final: pc=${:08X} d0=${:08X} d7=${:08X} color00=${:03X} bplcon0=${:04X} \
                 cyl={} motor_spinning={} step_events={}",
                regs.pc,
                regs.d[0],
                regs.d[7],
                amiga.color(0),
                amiga.bplcon0(),
                amiga.drive().cylinder(),
                amiga.drive().motor_spinning(),
                amiga.drive().step_event_counter(),
            );
            break;
        }

        prev_pc = pc;
    }

    assert!(
        alert_seen,
        "KS 1.2 / WB 1.2 run did not hit the expected early alert handler in {} frames",
        MAX_FRAMES
    );
}

#[test]
#[ignore = "needs local kick12.rom"]
fn trace_a500_kick12_early_boot_branch_to_alert() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(
        home.join(".emu198x/roms/commodore-amiga/kick12.rom"),
        "kick12.rom",
    ) else {
        return;
    };

    let mut runtime = AmigaRuntime::new(Model::A500OcsPal, rom).expect("build KS 1.2 runtime");

    let mut prev_pc = runtime.machine().cpu().regs.pc;
    let mut cold_start_hits = 0u32;
    let mut chip_probe_entry_hits = 0u32;
    let mut chip_probe_return_hits = 0u32;
    let mut chip_probe_branch_hits = 0u32;
    let mut chip_alert_setup_hits = 0u32;
    let mut exec_init_hits = 0u32;
    let mut alert_seen = false;

    for tick in 0..(MAX_FRAMES * A500_PAL_FRAME_TICKS) {
        runtime.machine_mut().tick();
        let amiga = runtime.machine();
        let regs = amiga.cpu().regs;
        let pc = regs.pc;
        let frame = tick / A500_PAL_FRAME_TICKS + 1;

        let hit = match pc {
            KS12_COLD_START => {
                cold_start_hits += 1;
                Some(("cold-start", cold_start_hits))
            }
            KS12_CHIP_PROBE_ENTRY => {
                chip_probe_entry_hits += 1;
                Some(("chip-probe-entry", chip_probe_entry_hits))
            }
            KS12_CHIP_PROBE_RETURN => {
                chip_probe_return_hits += 1;
                Some(("chip-probe-return", chip_probe_return_hits))
            }
            KS12_CHIP_PROBE_BRANCH => {
                chip_probe_branch_hits += 1;
                Some(("chip-probe-branch", chip_probe_branch_hits))
            }
            KS12_CHIP_ALERT_SETUP => {
                chip_alert_setup_hits += 1;
                Some(("chip-alert-setup", chip_alert_setup_hits))
            }
            KS12_EXEC_INIT => {
                exec_init_hits += 1;
                Some(("exec-init", exec_init_hits))
            }
            KS12_ALERT_ENTRY | KS12_ALERT_DIRECT_ENTRY | KS12_ALERT_LOOP => {
                if !alert_seen {
                    alert_seen = true;
                    Some(("alert", 1))
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some((label, count)) = hit {
            println!(
                "{label} hit #{count} frame={frame} tick={} prev_pc=${prev_pc:08X} pc=${pc:08X}",
                tick + 1
            );
            println!(
                "  d0=${:08X} d1=${:08X} d6=${:08X} d7=${:08X}",
                regs.d[0], regs.d[1], regs.d[6], regs.d[7]
            );
            println!(
                "  a0=${:08X} a1=${:08X} a3=${:08X} a4=${:08X} a5=${:08X} a6=${:08X} sr=${:04X}",
                regs.a[0], regs.a[1], regs.a[3], regs.a[4], regs.a[5], regs.a[6], regs.sr
            );
            println!(
                "  color00=${:03X} bplcon0=${:04X} overlay={} cyl={} motor_on={} spinning={} steps={}",
                amiga.color(0),
                amiga.bplcon0(),
                amiga.memory().overlay(),
                amiga.drive().cylinder(),
                amiga.drive().motor_on(),
                amiga.drive().motor_spinning(),
                amiga.drive().step_event_counter(),
            );
            if alert_seen {
                break;
            }
        }

        prev_pc = pc;
    }

    assert!(alert_seen, "expected to reach KS 1.2 alert path");
}

#[test]
#[ignore = "needs local kick12.rom"]
fn trace_a500_kick12_chip_probe_instruction_flow() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(
        home.join(".emu198x/roms/commodore-amiga/kick12.rom"),
        "kick12.rom",
    ) else {
        return;
    };

    let mut runtime = AmigaRuntime::new(Model::A500OcsPal, rom).expect("build KS 1.2 runtime");
    runtime.machine_mut().debug_watch_addr = Some((0x0000_0000, 0x0000_2000));
    runtime.machine_mut().debug_watch_writes.clear();

    let mut prev_instr_start_pc = runtime.machine().cpu().instr_start_pc;
    let mut prev_instr_count = runtime.machine().cpu().instruction_starts;
    let mut last_watch_len = 0usize;

    for tick in 0..(MAX_FRAMES * A500_PAL_FRAME_TICKS) {
        runtime.machine_mut().tick();

        let maybe_snapshot = {
            let amiga = runtime.machine();
            let cpu = amiga.cpu();
            if cpu.instruction_starts == prev_instr_count && cpu.instr_start_pc == prev_instr_start_pc
            {
                None
            } else {
                prev_instr_count = cpu.instruction_starts;
                prev_instr_start_pc = cpu.instr_start_pc;
                Some((
                    cpu.instruction_starts,
                    cpu.instr_start_pc,
                    cpu.regs.pc,
                    cpu.regs.d,
                    cpu.regs.a,
                    cpu.regs.sr,
                    amiga.color(0),
                    amiga.bplcon0(),
                    amiga.memory().overlay(),
                    amiga.drive().cylinder(),
                    amiga.drive().motor_on(),
                    amiga.drive().motor_spinning(),
                    amiga.drive().step_event_counter(),
                ))
            }
        };

        let frame = tick / A500_PAL_FRAME_TICKS + 1;

        while last_watch_len < runtime.machine().debug_watch_writes.len() {
            let (cck, pc, addr, val, is_word) = runtime.machine().debug_watch_writes[last_watch_len];
            last_watch_len += 1;
            let width = if is_word { "word" } else { "byte" };
            println!(
                "watch frame={} cck={cck} instr=${pc:08X} addr=${addr:08X} {width}=${val:04X}",
                cck / (A500_PAL_FRAME_TICKS / 4) + 1
            );
        }

        let Some((
            instr_count,
            instr_start_pc,
            pc,
            d,
            a,
            sr,
            color00,
            bplcon0,
            overlay,
            cyl,
            motor_on,
            motor_spinning,
            steps,
        )) = maybe_snapshot
        else {
            continue;
        };

        let interesting = matches!(
            instr_start_pc,
            0x00FC_0208
                | 0x00FC_021A
                | 0x00FC_0220
                | 0x00FC_0238
                | 0x00FC_0240
                | 0x00FC_0592
                | 0x00FC_0594
                | 0x00FC_0596
                | 0x00FC_0598
                | 0x00FC_059E
                | 0x00FC_05A2
                | 0x00FC_05A4
                | 0x00FC_05A6
                | 0x00FC_05A8
                | 0x00FC_05AA
                | 0x00FC_05AC
                | 0x00FC_05AE
                | 0x00FC_05B0
                | 0x00FC_05B2
                | KS12_ALERT_ENTRY
        );
        if !interesting {
            continue;
        }

        println!(
            "instr#{instr_count} frame={frame} instr=${instr_start_pc:08X} pc=${pc:08X} \
             d0=${:08X} d1=${:08X} a0=${:08X} a1=${:08X} a2=${:08X} a3=${:08X} a5=${:08X} sr=${sr:04X}",
            d[0], d[1], a[0], a[1], a[2], a[3], a[5]
        );
        println!(
            "  mem[0000]=${:08X} mem[1000]=${:08X} color00=${color00:03X} bplcon0=${bplcon0:04X} \
             overlay={overlay} cyl={cyl} motor_on={motor_on} spinning={motor_spinning} steps={steps}",
            read_long(&runtime, 0x0000_0000),
            read_long(&runtime, 0x0000_1000),
        );

        if instr_start_pc == KS12_ALERT_ENTRY {
            break;
        }
    }
}

#[test]
#[ignore = "needs local kick12.rom"]
fn trace_a500_kick12_last_instructions_before_alert() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(
        home.join(".emu198x/roms/commodore-amiga/kick12.rom"),
        "kick12.rom",
    ) else {
        return;
    };

    let mut runtime = AmigaRuntime::new(Model::A500OcsPal, rom).expect("build KS 1.2 runtime");
    let mut prev_instr_start_pc = runtime.machine().cpu().instr_start_pc;
    let mut prev_instr_count = runtime.machine().cpu().instruction_starts;
    let mut recent = VecDeque::<String>::with_capacity(48);

    for tick in 0..(MAX_FRAMES * A500_PAL_FRAME_TICKS) {
        runtime.machine_mut().tick();
        let amiga = runtime.machine();
        let cpu = amiga.cpu();
        if cpu.instruction_starts == prev_instr_count && cpu.instr_start_pc == prev_instr_start_pc {
            continue;
        }
        prev_instr_count = cpu.instruction_starts;
        prev_instr_start_pc = cpu.instr_start_pc;

        let frame = tick / A500_PAL_FRAME_TICKS + 1;
        let line = format!(
            "instr#{} frame={frame} instr=${:08X} pc=${:08X} d0=${:08X} d7=${:08X} a5=${:08X} a6=${:08X} sr=${:04X} color00=${:03X} bplcon0=${:04X}",
            cpu.instruction_starts,
            cpu.instr_start_pc,
            cpu.regs.pc,
            cpu.regs.d[0],
            cpu.regs.d[7],
            cpu.regs.a[5],
            cpu.regs.a[6],
            cpu.regs.sr,
            amiga.color(0),
            amiga.bplcon0(),
        );
        if recent.len() == 48 {
            recent.pop_front();
        }
        recent.push_back(line);

        if cpu.instr_start_pc == KS12_ALERT_ENTRY {
            println!("last instructions before first KS 1.2 alert:");
            for entry in recent {
                println!("  {entry}");
            }
            return;
        }
    }

    panic!("expected to reach KS 1.2 alert path");
}

#[test]
#[ignore = "needs local kick12.rom"]
fn trace_a500_kick12_cpu_detect_helper() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(
        home.join(".emu198x/roms/commodore-amiga/kick12.rom"),
        "kick12.rom",
    ) else {
        return;
    };

    let mut runtime = AmigaRuntime::new(Model::A500OcsPal, rom).expect("build KS 1.2 runtime");
    runtime.machine_mut().debug_watch_addr = Some((0x0000_0010, 0x0000_0020));
    runtime.machine_mut().debug_watch_writes.clear();

    let mut prev_instr_start_pc = runtime.machine().cpu().instr_start_pc;
    let mut prev_instr_count = runtime.machine().cpu().instruction_starts;
    let mut last_watch_len = 0usize;

    for tick in 0..(MAX_FRAMES * A500_PAL_FRAME_TICKS) {
        runtime.machine_mut().tick();
        let frame = tick / A500_PAL_FRAME_TICKS + 1;

        while last_watch_len < runtime.machine().debug_watch_writes.len() {
            let (cck, pc, addr, val, is_word) = runtime.machine().debug_watch_writes[last_watch_len];
            last_watch_len += 1;
            let width = if is_word { "word" } else { "byte" };
            println!(
                "watch frame={} cck={cck} instr=${pc:08X} addr=${addr:08X} {width}=${val:04X}",
                cck / (A500_PAL_FRAME_TICKS / 4) + 1
            );
        }

        let amiga = runtime.machine();
        let cpu = amiga.cpu();
        if cpu.instruction_starts == prev_instr_count && cpu.instr_start_pc == prev_instr_start_pc {
            continue;
        }
        prev_instr_count = cpu.instruction_starts;
        prev_instr_start_pc = cpu.instr_start_pc;

        let interesting = (0x00FC_0546..=0x00FC_0590).contains(&cpu.instr_start_pc)
            || cpu.instr_start_pc == KS12_ALERT_ENTRY;
        if !interesting {
            continue;
        }

        println!(
            "instr#{} frame={frame} instr=${:08X} pc=${:08X} d0=${:08X} d1=${:08X} d7=${:08X} a0=${:08X} a1=${:08X} a5=${:08X} a6=${:08X} sr=${:04X}",
            cpu.instruction_starts,
            cpu.instr_start_pc,
            cpu.regs.pc,
            cpu.regs.d[0],
            cpu.regs.d[1],
            cpu.regs.d[7],
            cpu.regs.a[0],
            cpu.regs.a[1],
            cpu.regs.a[5],
            cpu.regs.a[6],
            cpu.regs.sr,
        );
        println!(
            "  vec10=${:08X} vec2C=${:08X} color00=${:03X} bplcon0=${:04X}",
            read_long(&runtime, 0x0000_0010),
            read_long(&runtime, 0x0000_002C),
            amiga.color(0),
            amiga.bplcon0(),
        );

        if cpu.instr_start_pc == KS12_ALERT_ENTRY {
            return;
        }
    }

    panic!("expected to reach KS 1.2 alert path");
}

#[test]
#[ignore = "needs local kick12.rom"]
fn trace_a500_kick12_pre_helper_call_and_alert_frame() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(
        home.join(".emu198x/roms/commodore-amiga/kick12.rom"),
        "kick12.rom",
    ) else {
        return;
    };

    let mut runtime = AmigaRuntime::new(Model::A500OcsPal, rom).expect("build KS 1.2 runtime");
    let mut prev_instr_start_pc = runtime.machine().cpu().instr_start_pc;
    let mut prev_instr_count = runtime.machine().cpu().instruction_starts;

    for tick in 0..(MAX_FRAMES * A500_PAL_FRAME_TICKS) {
        runtime.machine_mut().tick();
        let amiga = runtime.machine();
        let cpu = amiga.cpu();
        if cpu.instruction_starts == prev_instr_count && cpu.instr_start_pc == prev_instr_start_pc {
            continue;
        }
        prev_instr_count = cpu.instruction_starts;
        prev_instr_start_pc = cpu.instr_start_pc;

        let interesting = matches!(
            cpu.instr_start_pc,
            0x00FC_0286
                | 0x00FC_0288
                | 0x00FC_028A
                | 0x00FC_028C
                | 0x00FC_028E
                | 0x00FC_0292
                | 0x00FC_0298
                | 0x00FC_029C
                | 0x00FC_02A0
                | 0x00FC_02A4
                | 0x00FC_02A8
                | 0x00FC_30E4
                | 0x00FC_30EA
                | KS12_ALERT_ENTRY
        );
        if !interesting {
            continue;
        }

        let frame = tick / A500_PAL_FRAME_TICKS + 1;
        let a7 = cpu.regs.active_sp();
        println!(
            "instr#{} frame={frame} instr=${:08X} pc=${:08X} d0=${:08X} d1=${:08X} a3=${:08X} a4=${:08X} a5=${:08X} a6=${:08X} a7=${a7:08X} sr=${:04X}",
            cpu.instruction_starts,
            cpu.instr_start_pc,
            cpu.regs.pc,
            cpu.regs.d[0],
            cpu.regs.d[1],
            cpu.regs.a[3],
            cpu.regs.a[4],
            cpu.regs.a[5],
            cpu.regs.a[6],
            cpu.regs.sr,
        );
        println!(
            "  vec08=${:08X} vec0C=${:08X} vec10=${:08X} vec2C=${:08X}",
            read_long(&runtime, 0x0000_0008),
            read_long(&runtime, 0x0000_000C),
            read_long(&runtime, 0x0000_0010),
            read_long(&runtime, 0x0000_002C),
        );
        println!(
            "  stack=${:04X} ${:04X} ${:04X} ${:04X} ${:04X} ${:04X} ${:04X}",
            read_word(&runtime, a7),
            read_word(&runtime, a7.wrapping_add(2)),
            read_word(&runtime, a7.wrapping_add(4)),
            read_word(&runtime, a7.wrapping_add(6)),
            read_word(&runtime, a7.wrapping_add(8)),
            read_word(&runtime, a7.wrapping_add(10)),
            read_word(&runtime, a7.wrapping_add(12)),
        );

        if cpu.instr_start_pc == KS12_ALERT_ENTRY {
            return;
        }
    }

    panic!("expected to reach KS 1.2 alert path");
}

#[test]
#[ignore = "needs local kick12.rom"]
fn trace_kick12_slow_ram_probe_on_a500_variants() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(
        home.join(".emu198x/roms/commodore-amiga/kick12.rom"),
        "kick12.rom",
    ) else {
        return;
    };

    for (model, label) in [
        (Model::A500OcsPal, "a500"),
        (Model::A500OcsPalA501, "a500-a501"),
    ] {
        println!("\n=== {label} ===");
        let mut runtime =
            AmigaRuntime::new(model, rom.clone()).unwrap_or_else(|err| panic!("{label}: {err}"));
        runtime.machine_mut().debug_watch_addr = Some((0x00C0_0000, 0x001C_0000));
        runtime.machine_mut().debug_watch_writes.clear();

        let mut prev_instr_start_pc = runtime.machine().cpu().instr_start_pc;
        let mut prev_instr_count = runtime.machine().cpu().instruction_starts;
        let mut last_watch_len = 0usize;

        for tick in 0..(MAX_FRAMES * A500_PAL_FRAME_TICKS) {
            runtime.machine_mut().tick();
            let frame = tick / A500_PAL_FRAME_TICKS + 1;

            while last_watch_len < runtime.machine().debug_watch_writes.len() {
                let (cck, pc, addr, val, is_word) = runtime.machine().debug_watch_writes[last_watch_len];
                last_watch_len += 1;
                let width = if is_word { "word" } else { "byte" };
                println!(
                    "watch frame={} cck={cck} instr=${pc:08X} addr=${addr:08X} {width}=${val:04X}",
                    cck / (A500_PAL_FRAME_TICKS / 4) + 1
                );
            }

            let amiga = runtime.machine();
            let cpu = amiga.cpu();
            if cpu.instruction_starts == prev_instr_count && cpu.instr_start_pc == prev_instr_start_pc {
                continue;
            }
            prev_instr_count = cpu.instruction_starts;
            prev_instr_start_pc = cpu.instr_start_pc;

            let interesting = (KS12_SLOW_PROBE_ENTRY..=KS12_SLOW_PROBE_EXIT)
                .contains(&cpu.instr_start_pc)
                || cpu.instr_start_pc == KS12_SLOW_PROBE_RETURN;
            if !interesting {
                continue;
            }

            println!(
                "instr#{} frame={frame} instr=${:08X} pc=${:08X} d0=${:08X} d1=${:08X} a0=${:08X} a1=${:08X} a2=${:08X} a4=${:08X} a5=${:08X} sr=${:04X}",
                cpu.instruction_starts,
                cpu.instr_start_pc,
                cpu.regs.pc,
                cpu.regs.d[0],
                cpu.regs.d[1],
                cpu.regs.a[0],
                cpu.regs.a[1],
                cpu.regs.a[2],
                cpu.regs.a[4],
                cpu.regs.a[5],
                cpu.regs.sr,
            );
            println!(
                "  read[C3F09A]=${:04X} read[C7F09A]=${:04X} read[CBF09A]=${:04X} read[D3F09A]=${:04X}",
                read_word(&runtime, 0x00C3_F09A),
                read_word(&runtime, 0x00C7_F09A),
                read_word(&runtime, 0x00CB_F09A),
                read_word(&runtime, 0x00D3_F09A),
            );

            if cpu.instr_start_pc == KS12_SLOW_PROBE_RETURN {
                break;
            }
        }
    }
}

#[test]
#[ignore = "needs local kick12.rom"]
fn trace_kick12_nodisk_on_plain_256k_chip_machine() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(
        home.join(".emu198x/roms/commodore-amiga/kick12.rom"),
        "kick12.rom",
    ) else {
        return;
    };

    let mut amiga = AmigaOcs::with_ram_config(
        rom,
        RamConfig {
            chip_kb: 256,
            slow_kb: 0,
            fast_kb: 0,
        },
    );

    let frame_targets = [400u64, 700, 1200];
    let mut next_target = 0usize;
    for tick in 0..(MAX_FRAMES * A500_PAL_FRAME_TICKS) {
        amiga.tick();
        let frame = tick / A500_PAL_FRAME_TICKS + 1;
        if next_target < frame_targets.len() && frame == frame_targets[next_target] {
            println!(
                "frame {}: pc=${:08X} color00=${:03X} bplcon0=${:04X}",
                frame,
                amiga.cpu().regs.pc,
                amiga.color(0),
                amiga.bplcon0(),
            );
            next_target += 1;
            if next_target == frame_targets.len() {
                break;
            }
        }
    }
}
