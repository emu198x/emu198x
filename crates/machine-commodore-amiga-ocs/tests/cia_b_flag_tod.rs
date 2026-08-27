//! CIA-B FLAG (floppy `/INDEX`) and TOD (`/HSYNC`) wiring.
//!
//! On the Amiga, CIA-B's FLAG pin is the floppy index pulse and its TOD
//! pin is active-low horizontal sync. These tests cover both board
//! connections and the delayed, counter-visible TOD update.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_LINE_TICKS, RamConfig};
use peripheral_commodore_amiga_floppy::{Adf, DD};

const ICR_FLAG: u8 = 0x10; // CIA ICR bit 4 = FLAG.
const ICR_ALARM: u8 = 0x04;
const CIA_B_TOD_VISIBLE_HPOS: u16 = 0x66;

/// A ROM whose reset vector parks the CPU in a `BRA.S *` self-loop, so it
/// never writes CIA registers (which would halt TOD or clear ICR) while
/// we observe the chip-driven wiring.
fn parked_cpu_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes()); // initial SSP
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes()); // initial PC
    rom[8] = 0x60; // BRA.S
    rom[9] = 0xFE; // -2 → branch to self
    rom
}

fn run_until_position(amiga: &mut AmigaOcs, vpos: u16, hpos: u16) {
    for _ in 0..1000 {
        if amiga.agnus().vpos == vpos && amiga.agnus().hpos == hpos {
            return;
        }
        amiga.tick();
    }
    panic!("beam did not reach position ({vpos},{hpos})");
}

#[test]
fn cia_b_tod_updates_at_delayed_hsync_position() {
    let mut amiga = AmigaOcs::new(parked_cpu_rom());

    run_until_position(&mut amiga, 0, CIA_B_TOD_VISIBLE_HPOS - 1);
    assert_eq!(
        amiga.cia_b().tod_counter(),
        0,
        "CIA-B TOD must remain unchanged before the delayed update"
    );
    run_until_position(&mut amiga, 0, CIA_B_TOD_VISIBLE_HPOS);
    assert_eq!(
        amiga.cia_b().tod_counter(),
        1,
        "CIA-B TOD should update after the HSYNC-derived delay"
    );

    run_until_position(&mut amiga, 1, CIA_B_TOD_VISIBLE_HPOS - 1);
    assert_eq!(
        amiga.cia_b().tod_counter(),
        1,
        "CIA-B TOD must update only once per scanline"
    );
    run_until_position(&mut amiga, 1, CIA_B_TOD_VISIBLE_HPOS);
    assert_eq!(
        amiga.cia_b().tod_counter(),
        2,
        "next scanline should produce one further TOD update"
    );
}

#[test]
fn cia_b_tod_alarm_waits_for_delayed_hsync_update() {
    let mut amiga = AmigaOcs::new(parked_cpu_rom());

    // Program alarm $000001, return CRB to counter mode, then enable
    // the alarm interrupt source.
    amiga.cia_b_mut().write(0x0F, 0x80);
    amiga.cia_b_mut().write(0x0A, 0x00);
    amiga.cia_b_mut().write(0x09, 0x00);
    amiga.cia_b_mut().write(0x08, 0x01);
    amiga.cia_b_mut().write(0x0F, 0x00);
    amiga.cia_b_mut().write(0x0D, 0x84);

    run_until_position(&mut amiga, 0, CIA_B_TOD_VISIBLE_HPOS - 1);
    assert_eq!(
        amiga.cia_b().icr_status() & ICR_ALARM,
        0,
        "alarm must remain clear before the delayed TOD update"
    );

    run_until_position(&mut amiga, 0, CIA_B_TOD_VISIBLE_HPOS);
    assert_ne!(
        amiga.cia_b().icr_status() & ICR_ALARM,
        0,
        "delayed TOD update should latch the alarm"
    );
    for _ in 0..20 {
        if amiga.intreq() & 0x2000 != 0 {
            break;
        }
        amiga.tick();
    }
    assert_ne!(
        amiga.intreq() & 0x2000,
        0,
        "CIA-B alarm should reach Paula EXTER"
    );
}

#[test]
fn cia_b_tod_updates_once_on_ntsc_short_and_long_lines() {
    let mut amiga = AmigaOcs::with_ram_config_ntsc(parked_cpu_rom(), RamConfig::bare());

    // NTSC starts with a 227-CCK short line and alternates with a
    // 228-CCK long line. The delayed TOD event occurs once on each.
    for line in 0..3u16 {
        run_until_position(&mut amiga, line, CIA_B_TOD_VISIBLE_HPOS - 1);
        assert_eq!(
            amiga.cia_b().tod_counter(),
            u32::from(line),
            "CIA-B TOD must remain unchanged before line {line}'s event"
        );
        run_until_position(&mut amiga, line, CIA_B_TOD_VISIBLE_HPOS);
        assert_eq!(
            amiga.cia_b().tod_counter(),
            u32::from(line) + 1,
            "CIA-B TOD must update once on NTSC line {line}"
        );
    }
}

#[test]
fn cia_b_tod_ticks_once_per_scanline() {
    let mut amiga = AmigaOcs::new(parked_cpu_rom());

    // Start immediately after an update, then advance an integral
    // number of complete PAL lines to the same beam phase.
    run_until_position(&mut amiga, 0, CIA_B_TOD_VISIBLE_HPOS);
    let initial_tod = amiga.cia_b().tod_counter();
    const LINES: u32 = 32;
    for _ in 0..(u64::from(LINES) * u64::from(PAL_LINE_TICKS)) {
        amiga.tick();
    }

    assert_eq!(amiga.agnus().vpos, LINES as u16);
    assert_eq!(amiga.agnus().hpos, CIA_B_TOD_VISIBLE_HPOS);
    assert_eq!(
        amiga.cia_b().tod_counter(),
        initial_tod + LINES,
        "CIA-B TOD must tick once per /HSYNC scanline"
    );
}

#[test]
#[ignore = "SLOW: spins the drive a full revolution (~5M machine ticks); run with --include-ignored"]
fn cia_b_flag_raised_by_floppy_index_pulse() {
    let mut amiga = AmigaOcs::new(parked_cpu_rom());
    amiga.insert_adf(Adf::from_bytes(vec![0; DD.len()]).expect("valid blank ADF"));
    // CIA-B drive-control pins are outputs; the OS sets DDRB = $FF. Then
    // PRB ($BFD100) = $75 → motor on + DF0 selected. A parked CPU leaves
    // both set.
    amiga.poke_byte(0x00BF_D300, 0xFF);
    amiga.poke_byte(0x00BF_D100, 0x75);

    assert_eq!(
        amiga.cia_b().icr_status() & ICR_FLAG,
        0,
        "FLAG starts clear"
    );

    // Run until the spinning drive emits its first /INDEX pulse, which the
    // wiring routes to CIA-B FLAG. Motor spin-up + one full revolution.
    let mut fired = false;
    for _ in 0..6_000_000u32 {
        amiga.tick();
        if amiga.cia_b().icr_status() & ICR_FLAG != 0 {
            fired = true;
            break;
        }
    }
    assert!(
        fired,
        "floppy /INDEX pulse must latch CIA-B FLAG (ICR bit 4)"
    );
}
