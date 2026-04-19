//! Phase 3 — silence custom chips, build exception table, set COLOR00=$0444.
//!
//! Per reference, Kickstart at very early boot:
//! - CIA-A PRA = $02 (OVL cleared, LED off)
//! - INTENA = $7FFF (all interrupts disabled)
//! - INTREQ = $7FFF (all pending cleared)
//! - DMACON = $7FFF (all DMA off)
//! - BPLCON0 = $0200
//! - BPLCON2 = $0000
//! - COLOR00 = $0444 (neutral grey)
//! - Exception vectors $08-$BF set to default handler

use machine_commodore_amiga::Amiga;
use std::fs;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);

    // Run until overlay flips (we know this is around frame 11).
    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    // Per reference, Phase 3 completes very early — Kickstart hits
    // overlay-clear at PC $FC0106, then writes $7FFF to INTENA etc. Then
    // builds exception vectors and sets grey COLOR00.
    //
    // We run long enough for Phase 3 to complete, but stop before Phase 5
    // starts mucking with memory layouts.

    let mut overlay_cleared_at = None;
    let mut intena_7fff_seen = false;
    let mut dmacon_7fff_seen = false;
    let mut color00_grey_seen_at = None;

    for frame in 0..50u64 {
        for _tick in 0..ccks_per_frame {
            amiga.tick_cck();

            if overlay_cleared_at.is_none() && !amiga.memory.overlay {
                overlay_cleared_at = Some(frame);
            }
            if !intena_7fff_seen {
                for e in amiga.paula.intena_write_log.iter() {
                    if *e == 0x7FFF {
                        intena_7fff_seen = true;
                    }
                }
            }
        }
        // If we've seen all phase 3 signals, we can stop.
        if amiga.denise.palette[0] == 0x0444 && color00_grey_seen_at.is_none() {
            color00_grey_seen_at = Some(frame);
        }
    }

    // Dump exception vector slice.
    let read_long = |a: &Amiga, addr: u32| {
        (u32::from(a.memory.read_word(addr)) << 16) | u32::from(a.memory.read_word(addr + 2))
    };

    println!("=== Phase 3 — check state after 50 frames ===\n");
    println!("Overlay:");
    println!("  overlay = {}  cleared_at_frame = {:?}", amiga.memory.overlay, overlay_cleared_at);
    println!(
        "  CIA-A PRA output = ${:02X}  (expected: $02 after OVL clear, bit 0 = 0)",
        amiga.cia_a.port_a_output()
    );
    println!();

    println!("Paula interrupt state:");
    println!("  intena = ${:04X}  (expected: 0 — all masked after boot)", amiga.paula.intena);
    println!("  intreq = ${:04X}", amiga.paula.intreq);
    println!("  Saw $7FFF written to INTENA: {}", intena_7fff_seen);
    println!("  intena write log (last 16):");
    for v in amiga.paula.intena_write_log.iter() {
        println!("    ${v:04X}");
    }
    println!();

    println!("Agnus:");
    println!("  dmacon = ${:04X}  (expected: some value with DMA bits set later — not 0 forever)", amiga.agnus.dmacon);
    println!("  bplcon0 = ${:04X}", amiga.agnus.bplcon0);

    println!("\nDenise:");
    println!(
        "  palette[0] (COLOR00) = ${:03X}  (expected: $444 during early Phase 3)",
        amiga.denise.palette[0]
    );
    println!("  COLOR00=$0444 seen at frame: {:?}", color00_grey_seen_at);

    println!("\nException vectors $08-$3C (CPU exceptions):");
    for i in 0..14 {
        let a = 0x08u32 + i * 4;
        let v = read_long(&amiga, a);
        let exc_name = match i {
            0 => "Bus error",
            1 => "Address error",
            2 => "Illegal instruction",
            3 => "Division by zero",
            4 => "CHK",
            5 => "TRAPV",
            6 => "Privilege violation",
            7 => "Trace",
            8 => "Line 1010 (A)",
            9 => "Line 1111 (F)",
            10 => "(reserved)",
            11 => "(reserved)",
            12 => "(reserved)",
            13 => "(reserved)",
            _ => "",
        };
        println!("  ${a:04X}: ${v:08X}  ({exc_name})");
    }

    println!("\nException vectors $64-$7C (autovector interrupts):");
    for i in 0..7 {
        let a = 0x64u32 + i * 4;
        let v = read_long(&amiga, a);
        let ipl = i + 1;
        let name = match ipl {
            1 => "TBE/SOFT",
            2 => "DSKBLK (CIA-A PORTS)",
            3 => "VERTB/COPER/BLIT",
            4 => "AUDIO",
            5 => "RBF/DSKSYNC",
            6 => "EXTER (CIA-B)",
            7 => "NMI",
            _ => "",
        };
        println!("  ${a:04X} (IPL{ipl}): ${v:08X}  {name}");
    }
}
