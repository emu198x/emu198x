//! Sample CIA-A Timer B state at several frames so we can see
//! whether timer.device configured it for MICROHZ-unit ticking.
//!
//! Expected: timer.device opens ciaa.resource and installs an
//! ICR vector for Timer B (JSR -6(A6) on cia.resource with D0=1).
//! For the handler to run, TB must be started (CRB.START=1), in
//! continuous mode (CRB.RUNMODE=0), and ICR.TB must be unmasked.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn sample(amiga: &AmigaOcs, frame: u64, label: &str) {
    let cia_a = amiga.cia_a();
    eprintln!(
        "frame {frame:>4} {label}:\n  \
         TA counter=${:04X} CRA=${:02X}  \
         TB counter=${:04X} CRB=${:02X}\n  \
         ICR mask=${:02X} status=${:02X}  irq={}  TOD=${:06X}",
        cia_a.timer_a(),
        cia_a.cra(),
        cia_a.timer_b(),
        cia_a.crb(),
        cia_a.icr_mask(),
        cia_a.icr_status(),
        cia_a.irq_active(),
        cia_a.tod_counter(),
    );
}

#[test]
#[ignore]
fn snapshot_cia_a_timers() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);

    let checkpoints = [50u64, 100, 150, 180, 185, 190, 200, 210, 250, 400, 700];
    let mut last = 0u64;
    eprintln!("\n########## slow-RAM CIA-A Timer state ##########");
    for &cp in &checkpoints {
        for _ in 0..((cp - last) * PAL_FRAME_TICKS) {
            amiga.tick();
        }
        sample(&amiga, cp, "");
        last = cp;
    }

    eprintln!(
        "\n=== CIA-A register write log ({} entries) ===",
        amiga.debug_cia_a_cr_log.len()
    );
    for (cck, pc, reg, val) in &amiga.debug_cia_a_cr_log {
        let frame = cck / 70824;
        let name = match reg {
            0 => "PRA ",
            1 => "PRB ",
            2 => "DDRA",
            3 => "DDRB",
            4 => "TALO",
            5 => "TAHI",
            6 => "TBLO",
            7 => "TBHI",
            8 => "TODL",
            9 => "TODM",
            0xA => "TODH",
            0xD => "ICR ",
            0xE => "CRA ",
            0xF => "CRB ",
            _ => "??? ",
        };
        let extra = if *reg == 0xE || *reg == 0xF {
            let load = if val & 0x10 != 0 { " LOAD" } else { "" };
            let start = if val & 0x01 != 0 { " START" } else { " stop" };
            let runmode = if val & 0x08 != 0 { " oneshot" } else { " cont" };
            format!("{load}{start}{runmode}")
        } else if *reg == 0xD {
            let op = if val & 0x80 != 0 { "SET" } else { "CLR" };
            format!(" ICR-{op} bits=${:02X}", val & 0x1F)
        } else {
            String::new()
        };
        eprintln!("  frame~{frame:<3}  pc=${pc:08X}  {name}=${val:02X}{extra}");
    }
}
