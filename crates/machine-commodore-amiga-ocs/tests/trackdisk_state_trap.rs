//! Snapshot the state that trackdisk.device BeginIO is acting on:
//!
//!  - CIA-A PRA (disk status inputs: /RDY, /CHNG, /WPRO, /TK0)
//!  - CIA-B PRB (disk control outputs: motor, sel, side, dir, step)
//!  - Paula disk registers (DSKPT / DSKLEN / DSKSYNC)
//!  - INTENA, INTREQ, ADKCON
//!
//! Plus: check if BeginIO RETURNS after the CMD_READ call. trackdisk
//! BeginIO entry is $FE9C3E. Its natural returns are the RTS at
//! $FE9C94 and the RTS at $FE9CD2 (these are the ones I can spot in
//! the disassembly so far). If strap hangs inside BeginIO, these
//! RTS points hit fewer times than the entry.

use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};

const BEGIN_IO_ENTRY: u32 = 0x00FE_9C3E;
const BEGIN_IO_RTS_A: u32 = 0x00FE_9C94;
const BEGIN_IO_RTS_B: u32 = 0x00FE_9CD2;

fn effective_port(data: u8, direction: u8, input_lines: u8) -> u8 {
    (data & direction) | (input_lines & !direction)
}

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

    let mut entry = 0u64;
    let mut rts_a = 0u64;
    let mut rts_b = 0u64;
    let mut prev_pc = amiga.cpu().regs.pc;

    for _ in 0..(400 * PAL_FRAME_TICKS) {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        if pc == BEGIN_IO_ENTRY { entry += 1; }
        else if pc == BEGIN_IO_RTS_A { rts_a += 1; }
        else if pc == BEGIN_IO_RTS_B { rts_b += 1; }
        prev_pc = pc;
    }

    eprintln!("BeginIO entry hits:  {entry}");
    eprintln!("BeginIO RTS-A hits:  {rts_a}");
    eprintln!("BeginIO RTS-B hits:  {rts_b}");
    if entry > rts_a + rts_b {
        eprintln!(
            "→ {} call(s) entered BeginIO but never reached either RTS.\n  \
            The last call is stuck inside.",
            entry.saturating_sub(rts_a + rts_b)
        );
    }

    eprintln!("\n=== Hardware state ===");
    let cia_a = amiga.cia_a();
    let cia_b = amiga.cia_b();
    eprintln!(
        "CIA-A PRA effective = ${:02X}  (pa_input_lines=${:02X} data=${:02X} ddra=${:02X})",
        effective_port(cia_a.pra, cia_a.ddra, cia_a.pa_input_lines),
        cia_a.pa_input_lines, cia_a.pra, cia_a.ddra
    );
    eprintln!(
        "  bit 2 /CHNG = {}  bit 3 /WPRO = {}  bit 4 /TK0 = {}  bit 5 /RDY = {}",
        if (cia_a.pa_input_lines & 0x04) != 0 { "1 (no change)" } else { "0 (change pending)" },
        if (cia_a.pa_input_lines & 0x08) != 0 { "1 (not protected)" } else { "0 (protected)" },
        if (cia_a.pa_input_lines & 0x10) != 0 { "1 (not at trk0)" } else { "0 (at trk0)" },
        if (cia_a.pa_input_lines & 0x20) != 0 { "1 (NOT ready)" } else { "0 (ready)" },
    );
    eprintln!(
        "CIA-B PRB effective = ${:02X}  (pb_input_lines=${:02X} data=${:02X} ddrb=${:02X})",
        effective_port(cia_b.prb, cia_b.ddrb, cia_b.pb_input_lines),
        cia_b.pb_input_lines, cia_b.prb, cia_b.ddrb
    );
    eprintln!(
        "  bit 7 /MTR = {}  bit 3 /SEL0 = {}  bit 2 /SIDE = {}  bit 1 DIR = {}  bit 0 /STEP = {}",
        (cia_b.prb >> 7) & 1, (cia_b.prb >> 3) & 1, (cia_b.prb >> 2) & 1,
        (cia_b.prb >> 1) & 1, cia_b.prb & 1,
    );
    eprintln!();
    eprintln!("INTENA  = ${:04X}", amiga.intena());
    eprintln!("INTREQ  = ${:04X}", amiga.intreq());
    eprintln!("DMACON  = ${:04X}", amiga.dmacon());

    eprintln!("\n=== Disk-register write log ({} entries) ===", amiga.debug_dsk_log.len());
    for (cck, pc, reg, val) in &amiga.debug_dsk_log {
        eprintln!("  cck={cck:>10} pc=${pc:08X}  reg=${reg:03X}  val=${val:04X}");
    }
}

#[test]
#[ignore]
fn trackdisk_state_after_400_frames() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    run(&mut slow, "slow-RAM");

    let mut chip_only = AmigaOcs::new(rom);
    run(&mut chip_only, "chip-only");
}
