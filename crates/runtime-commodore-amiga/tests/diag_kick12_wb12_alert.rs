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

use format_commodore_amiga_adf::Adf;
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaRuntime, Model};

const KS12_ALERT_ENTRY: u32 = 0x00FC_05B4;
const KS12_ALERT_DIRECT_ENTRY: u32 = 0x00FC_05B8;
const KS12_ALERT_LOOP: u32 = 0x00FC_05DE;
const MAX_FRAMES: u64 = 4000;

fn load_artifact(path: PathBuf, label: &str) -> Option<Vec<u8>> {
    if !path.exists() {
        eprintln!("skipping: {label} missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display())))
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
