//! Re-run the graphics.library + intuition.library LVO traps, but
//! this time start counting from frame 0 so we don't miss
//! anything the ROM does during its Exec InitCode phase.
//!
//! The other traps all started counting at frame 200 (so they
//! could resolve LVO jump-table entries first), which would miss
//! a one-shot OpenScreen / MakeVPort call fired during library
//! init. Here we hardcode the ROM targets found by the earlier
//! traps — same KS 1.3, same addresses.

use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};

// Resolved earlier on the actual Kickstart 1.3 ROM by lvo_trap
// and intuition_trap. Stable across runs (same ROM).
const GFX_LOAD_VIEW: u32 = 0x00FC_63CC;
const GFX_MAKE_VPORT: u32 = 0x00FC_63BC;
const GFX_MRG_COP: u32 = 0x00FC_582C;
const INT_OPEN_SCREEN: u32 = 0x00FD_FCE8;
const INT_OPEN_WINDOW: u32 = 0x00FD_FCF4;
const INT_DISPLAY_ALERT: u32 = 0x00FD_FBC0;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn run(amiga: &mut AmigaOcs, label: &str) {
    eprintln!("\n########## {label} ##########");

    let mut make_vport = 0u64;
    let mut mrg_cop = 0u64;
    let mut load_view = 0u64;
    let mut open_screen = 0u64;
    let mut open_window = 0u64;
    let mut display_alert = 0u64;
    let mut first_hits: Vec<(&'static str, u64, u32)> = Vec::new();
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut tick = 0u64;

    for _ in 0..(400 * PAL_FRAME_TICKS) {
        amiga.tick();
        tick += 1;
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        let name = if pc == GFX_MAKE_VPORT {
            make_vport += 1;
            Some("MakeVPort  ")
        } else if pc == GFX_MRG_COP {
            mrg_cop += 1;
            Some("MrgCop     ")
        } else if pc == GFX_LOAD_VIEW {
            load_view += 1;
            Some("LoadView   ")
        } else if pc == INT_OPEN_SCREEN {
            open_screen += 1;
            Some("OpenScreen ")
        } else if pc == INT_OPEN_WINDOW {
            open_window += 1;
            Some("OpenWindow ")
        } else if pc == INT_DISPLAY_ALERT {
            display_alert += 1;
            Some("DisplayAlert")
        } else {
            None
        };
        if let Some(n) = name
            && first_hits.iter().filter(|(m, _, _)| *m == n).count() < 2 {
            first_hits.push((n, tick, pc));
        }
        prev_pc = pc;
    }

    eprintln!(
        "=== 400-frame counts (from tick 0, {} total ticks) ===",
        tick
    );
    eprintln!("  graphics:MakeVPort    = {make_vport}");
    eprintln!("  graphics:MrgCop       = {mrg_cop}");
    eprintln!("  graphics:LoadView     = {load_view}");
    eprintln!("  intuition:OpenScreen  = {open_screen}");
    eprintln!("  intuition:OpenWindow  = {open_window}");
    eprintln!("  intuition:DisplayAlert= {display_alert}");

    if !first_hits.is_empty() {
        eprintln!("\n=== First hits ===");
        for (name, t, pc) in &first_hits {
            let cck = t / 2;
            let frame = cck / 70824;
            eprintln!(
                "  {name} at tick={t} (cck={cck}, frame~={frame})  pc=${pc:08X}"
            );
        }
    }

    let total = make_vport + mrg_cop + load_view + open_screen + open_window + display_alert;
    if total == 0 {
        eprintln!(
            "\n→ ZERO across 400 frames. Neither graphics.library nor\n  \
            intuition.library screen-setup entry points ever fire.\n  \
            The ROM's boot sequence genuinely never tries to open a\n  \
            display, not even during the early Exec InitCode phase."
        );
    } else {
        eprintln!("\n→ Hits detected — see first-hits list above.");
    }
}

#[test]
#[ignore]
fn trap_screen_setup_from_frame_zero() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    run(&mut slow, "slow-RAM");

    let mut chip_only = AmigaOcs::new(rom);
    run(&mut chip_only, "chip-only");
}
