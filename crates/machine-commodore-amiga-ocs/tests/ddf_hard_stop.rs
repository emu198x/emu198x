//! End-to-end coverage for the original Agnus fixed DDFSTOP boundary.
//!
//! These tests drive the real machine loop so Agnus arbitration, Denise
//! fetch service, pointer advancement, chipset selection and snapshots all
//! observe the same terminal fetch state.

use machine_commodore_amiga_ocs::{AmigaOcs, RamConfig};

const CUSTOM_BASE: u32 = 0x00DF_F000;
const DIWSTRT: u32 = CUSTOM_BASE + 0x08E;
const DIWSTOP: u32 = CUSTOM_BASE + 0x090;
const DDFSTRT: u32 = CUSTOM_BASE + 0x092;
const DDFSTOP: u32 = CUSTOM_BASE + 0x094;
const DMACON: u32 = CUSTOM_BASE + 0x096;
const BPLCON0: u32 = CUSTOM_BASE + 0x100;
const BEAMCON0: u32 = CUSTOM_BASE + 0x1DC;
const COP1LCH: u32 = CUSTOM_BASE + 0x080;
const COP1LCL: u32 = CUSTOM_BASE + 0x082;
const COPJMP1: u32 = CUSTOM_BASE + 0x088;
const BPL_POINTER_REGS: [(u32, u32); 4] = [
    (CUSTOM_BASE + 0x0E0, CUSTOM_BASE + 0x0E2),
    (CUSTOM_BASE + 0x0E4, CUSTOM_BASE + 0x0E6),
    (CUSTOM_BASE + 0x0E8, CUSTOM_BASE + 0x0EA),
    (CUSTOM_BASE + 0x0EC, CUSTOM_BASE + 0x0EE),
];
const BITPLANE_BASES: [u32; 4] = [0x0001_0000, 0x0001_2000, 0x0001_4000, 0x0001_6000];

fn parked_cpu_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8] = 0x60; // BRA.S
    rom[9] = 0xFE; // -2: branch to self
    rom
}

fn early_ocs_machine() -> AmigaOcs {
    AmigaOcs::with_ram_config(parked_cpu_rom(), RamConfig::bare())
}

fn fat_agnus_machine() -> AmigaOcs {
    AmigaOcs::with_fat_agnus_ram_config(parked_cpu_rom(), RamConfig::bare())
}

fn advance_to_line(amiga: &mut AmigaOcs, target: u16) {
    let mut guard = 0;
    while amiga.agnus().vpos < target && guard < 100_000 {
        amiga.tick();
        guard += 1;
    }
    assert!(guard < 100_000, "beam did not reach line {target:#05x}");
    assert_eq!(amiga.agnus().hpos, 0);
}

fn configure_hires_overrun(amiga: &mut AmigaOcs) {
    amiga.poke_word(DIWSTRT, 0x3081);
    amiga.poke_word(DIWSTOP, 0xF0C1);
    amiga.poke_word(DDFSTRT, 0x0018);
    amiga.poke_word(DDFSTOP, 0x00E0);
    amiga.poke_word(BPLCON0, 0xC200); // hires, four planes, colour enabled
    for ((high, low), pointer) in BPL_POINTER_REGS.into_iter().zip(BITPLANE_BASES) {
        amiga.poke_word(high, (pointer >> 16) as u16);
        amiga.poke_word(low, pointer as u16);
    }
    amiga.poke_word(DMACON, 0x8300); // SETCLR | DMAEN | BPLEN
}

fn configure_lores_overrun(amiga: &mut AmigaOcs) {
    amiga.poke_word(DIWSTRT, 0x3081);
    amiga.poke_word(DIWSTOP, 0xF0C1);
    amiga.poke_word(DDFSTRT, 0x0018);
    amiga.poke_word(DDFSTOP, 0x00E0);
    amiga.poke_word(BPLCON0, 0x1200); // lores, one plane, colour enabled
    amiga.poke_word(BPL_POINTER_REGS[0].0, 0x0001);
    amiga.poke_word(BPL_POINTER_REGS[0].1, 0x0000);
    amiga.poke_word(DMACON, 0x8300); // SETCLR | DMAEN | BPLEN
}

fn run_to_next_line(amiga: &mut AmigaOcs) {
    let line = amiga.agnus().vpos;
    let mut guard = 0;
    while amiga.agnus().vpos == line && guard < 1_000 {
        amiga.tick();
        guard += 1;
    }
    assert!(guard < 1_000, "beam did not finish the test line");
}

#[test]
fn early_ocs_hard_stop_survives_pre_event_snapshot_and_releases_the_bus_after_df() {
    let mut original = early_ocs_machine();
    configure_hires_overrun(&mut original);
    advance_to_line(&mut original, 0x0030);
    let line_bases = original.agnus().bpl_pt;

    while original.agnus().hpos < 0x00D7 {
        original.tick();
    }
    assert_eq!(original.agnus().ddf_fetch_end(), None);

    let snapshot = original.snapshot_state();
    let mut restored = early_ocs_machine();
    restored.restore_snapshot_state(snapshot);
    assert_eq!(restored.agnus().ddf_fetch_end(), None);

    while original.agnus().hpos < 0x00D8 {
        original.tick();
    }
    while restored.agnus().hpos < 0x00D8 {
        restored.tick();
    }
    assert_eq!(original.agnus().ddf_stop_match(), None);
    assert_eq!(original.agnus().ddf_fetch_end(), Some(0x00DF));
    assert_eq!(restored.agnus().ddf_fetch_end(), Some(0x00DF));

    run_to_next_line(&mut original);
    run_to_next_line(&mut restored);
    assert_eq!(original.agnus().bpl_pt, restored.agnus().bpl_pt);
    for (plane, base) in line_bases.into_iter().enumerate().take(4) {
        assert_eq!(
            original.agnus().bpl_pt[plane],
            base + 100,
            "BPL{} must receive 50 words and no post-$DF grant",
            plane + 1
        );
    }
}

#[test]
fn ocs_hard_stop_precedes_a_same_cck_copper_ddfstop_write() {
    let mut amiga = early_ocs_machine();
    amiga.poke_byte(0x00BF_E201, 0x03);
    amiga.poke_byte(0x00BF_E001, 0x02);
    configure_lores_overrun(&mut amiga);
    amiga.poke_word(DDFSTOP, 0x00D8);

    // One MOVE followed by the end sentinel. Starting its two-cycle
    // fetch at $D6 makes the DDFSTOP write land at beam entry $D8.
    amiga.poke_word(0x0000_1000, 0x0094);
    amiga.poke_word(0x0000_1002, 0x0010);
    amiga.poke_word(0x0000_1004, 0xFFFF);
    amiga.poke_word(0x0000_1006, 0xFFFE);
    amiga.poke_word(COP1LCH, 0x0000);
    amiga.poke_word(COP1LCL, 0x1000);

    advance_to_line(&mut amiga, 0x0030);
    while amiga.agnus().hpos < 0x00D5 {
        amiga.tick();
    }
    amiga.poke_word(COPJMP1, 0);
    amiga.poke_word(DMACON, 0x8280); // SETCLR | DMAEN | COPEN
    while amiga.agnus().hpos < 0x00D8 {
        amiga.tick();
    }

    assert!(
        amiga
            .debug_copper_move_log
            .iter()
            .any(|&(_, vpos, hpos, reg, val)| {
                vpos == 0x0030 && hpos == 0x00D8 && reg == 0x0094 && val == 0x0010
            }),
        "Copper MOVE must land on the hard-stop CCK"
    );
    assert_eq!(
        amiga.agnus().ddf_stop_match(),
        Some(0x00D8),
        "the programmed comparator must match before Copper replaces DDFSTOP"
    );
    assert_eq!(
        amiga.agnus().ddf_fetch_end(),
        Some(0x00DF),
        "the pre-Copper hard event must retain the terminal unit"
    );
}

#[test]
fn fat_agnus_harddis_keeps_the_post_df_slots_available() {
    let mut fat = fat_agnus_machine();
    fat.poke_word(BEAMCON0, 0x4020); // HARDDIS | PAL
    configure_hires_overrun(&mut fat);
    advance_to_line(&mut fat, 0x0030);
    let line_bases = fat.agnus().bpl_pt;
    run_to_next_line(&mut fat);

    // E0 grants BPL4 and E1 grants BPL2. E2 remains the fixed
    // end-of-line refresh slot, so BPL3 cannot claim it even with
    // HARDDIS.
    let expected_bytes = [100, 102, 100, 102];
    for ((plane, base), bytes) in line_bases
        .into_iter()
        .enumerate()
        .take(4)
        .zip(expected_bytes)
    {
        assert_eq!(
            fat.agnus().bpl_pt[plane],
            base + bytes,
            "BPL{} HARDDIS byte count",
            plane + 1
        );
    }
}

#[test]
fn fat_agnus_defaults_to_the_fixed_right_limit_and_varvben_does_not_bypass_it() {
    for (case, beamcon0) in [("default", 0x0020), ("VARVBEN is vertical only", 0x1020)] {
        let mut fat = fat_agnus_machine();
        fat.poke_word(BEAMCON0, beamcon0);
        configure_hires_overrun(&mut fat);
        advance_to_line(&mut fat, 0x0030);
        let line_bases = fat.agnus().bpl_pt;

        while fat.agnus().hpos < 0x00D8 {
            fat.tick();
        }
        assert_eq!(
            fat.agnus().ddf_fetch_end(),
            Some(0x00DF),
            "{case} must retain the enhanced fixed right limit",
        );

        run_to_next_line(&mut fat);
        for (plane, base) in line_bases.into_iter().enumerate().take(4) {
            assert_eq!(
                fat.agnus().bpl_pt[plane],
                base + 100,
                "BPL{} {case} byte count",
                plane + 1,
            );
        }
    }
}
