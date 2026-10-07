//! End-to-end coverage for the original Agnus fixed DDFSTOP boundary.
//!
//! These tests drive the real machine loop so Agnus arbitration, Denise
//! fetch service, pointer advancement, chipset selection and snapshots all
//! observe the same terminal fetch state.

use commodore_agnus_ocs::{DisplayDmaChannel, DmaTransferTarget};
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

fn configure_hires_clean_idle_candidate(amiga: &mut AmigaOcs) {
    configure_hires_overrun(amiga);
    amiga.poke_word(DDFSTRT, 0x0038);
    amiga.poke_word(DDFSTOP, 0x00D0);
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

fn run_reference_boundary_line(amiga: &mut AmigaOcs, case: u16) {
    let expected: Vec<_> =
        include_str!("../../../test-data/commodore/amiga/ddf-boundaries/registered-events.csv")
            .lines()
            .skip(1)
            .filter_map(|row| {
                let values: Vec<u16> = row
                    .split(',')
                    .map(|v| v.parse().expect("reference integer"))
                    .collect();
                assert_eq!(values.len(), 5);
                (values[0] == case && values[1] == 0).then_some((
                    values[2],
                    values[3],
                    values[4] != 0,
                ))
            })
            .collect();
    assert!(!expected.is_empty(), "reference case must contain requests");
    let line = amiga.agnus().vpos;
    let mut actual = Vec::new();
    for _ in 0..456 {
        let h = amiga.agnus().hpos;
        amiga.tick();
        let agnus = amiga.agnus();
        if agnus.vpos != line {
            assert_eq!(
                actual, expected,
                "reference reservation schedule, case {case}"
            );
            return;
        }
        if agnus.hpos != h
            && let Some(request) = agnus.dma_pipeline().reservation()
            && let DisplayDmaChannel::Bitplane(plane) = request.channel
        {
            actual.push((agnus.hpos, u16::from(plane), request.add_modulo));
        }
    }
    panic!("beam did not finish the boundary line");
}

#[test]
fn early_ocs_hard_stop_survives_snapshot_and_completes_terminal_transfers() {
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
            "BPL{} must complete 50 words including the service tail",
            plane + 1
        );
    }
}

#[test]
fn ocs_hard_stop_survives_a_copper_ddfstop_write_at_d8() {
    let mut amiga = early_ocs_machine();
    amiga.poke_byte(0x00BF_E201, 0x03);
    amiga.poke_byte(0x00BF_E001, 0x02);
    configure_lores_overrun(&mut amiga);
    amiga.poke_word(DDFSTOP, 0x00D8);

    // Admission at $D5/$D7, followed by IR1/IR2 service at $D6/$D8.
    // Enabling after $D5 has already run misses its admission opportunity.
    amiga.poke_word(0x0000_1000, 0x0094);
    amiga.poke_word(0x0000_1002, 0x0010);
    amiga.poke_word(0x0000_1004, 0xFFFF);
    amiga.poke_word(0x0000_1006, 0xFFFE);
    amiga.poke_word(COP1LCH, 0x0000);
    amiga.poke_word(COP1LCL, 0x1000);

    advance_to_line(&mut amiga, 0x0030);
    while amiga.agnus().hpos < 0x00D3 {
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

    // Registered boundary case 0: terminal requests at $D9/$E1 survive
    // the rewrite, with actual memory service two CCKs later (across wrap).
    let pointer = amiga.agnus().bpl_pt[0];
    let mut services = Vec::new();
    for _ in 0..32 {
        let beam = (amiga.agnus().vpos, amiga.agnus().hpos);
        amiga.tick();
        let agnus = amiga.agnus();
        if beam != (agnus.vpos, agnus.hpos)
            && let Some(transfer) = agnus.dma_pipeline().service()
            && matches!(transfer.target, DmaTransferTarget::Display { .. })
        {
            services.push((agnus.vpos, agnus.hpos, transfer.address));
        }
    }
    assert_eq!(services, [(0x30, 0xDB, pointer), (0x31, 0, pointer + 2)]);
    assert_eq!(amiga.agnus().bpl_pt[0], pointer + 4);
}

#[test]
fn fat_agnus_harddis_keeps_the_post_df_slots_available() {
    let mut fat = fat_agnus_machine();
    fat.poke_word(BEAMCON0, 0x4020); // HARDDIS | PAL
    // Finish prior lines in-line. Otherwise HARDDIS leaves an older terminal
    // unit crossing h=0 and refresh can replace its pointers before this run.
    configure_hires_clean_idle_candidate(&mut fat);
    advance_to_line(&mut fat, 0x0030);
    fat.poke_word(DDFSTRT, 0x0018);
    fat.poke_word(DDFSTOP, 0x00E0);
    let line_bases = fat.agnus().bpl_pt;
    run_reference_boundary_line(&mut fat, 4);

    // $E2 reserves BPL4; it has not reached memory at the h=0 boundary.
    // All four planes have completed 50 words at that observation point.
    for (plane, base) in line_bases.into_iter().enumerate().take(4) {
        assert_eq!(
            fat.agnus().bpl_pt[plane],
            base + 100,
            "BPL{} HARDDIS byte count",
            plane + 1
        );
    }
    let pending = fat
        .agnus()
        .dma_pipeline()
        .address()
        .expect("cross-wrap BPL4");
    assert!(
        matches!(pending, commodore_agnus_ocs::DmaAddressStage::Transfer(t)
        if matches!(t.target, DmaTransferTarget::Display { reservation, .. }
            if reservation.channel == DisplayDmaChannel::Bitplane(3))
        && t.address == line_bases[3] + 100)
    );
    while fat.agnus().hpos < 1 {
        fat.tick();
    }
    assert_eq!(fat.agnus().bpl_pt[3], line_bases[3] + 102);
    // The following BPL2 reservation shares the horizontal strobe's RGA.
    // Existing compiled RGA service evidence requires REFPTR+2, not BPL2+2.
    while fat.agnus().hpos < 3 {
        fat.tick();
    }
    let transfer = fat
        .agnus()
        .dma_pipeline()
        .service()
        .expect("combined strobe service");
    assert!(
        matches!(transfer.target, DmaTransferTarget::DisplayRefresh { reservation, fixed_register: 0x3c }
        if reservation.channel == DisplayDmaChannel::Bitplane(1))
    );
    assert_eq!(fat.agnus().bpl_pt[1], transfer.address + 2);
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

#[test]
fn equal_ddf_boundaries_are_not_an_empty_machine_fetch_window() {
    for (case, mut amiga, beamcon0, reference_case) in [
        ("early OCS", early_ocs_machine(), None, 5),
        ("Fat Agnus default", fat_agnus_machine(), Some(0x0020), 5),
        ("Fat Agnus HARDDIS", fat_agnus_machine(), Some(0x4020), 6),
    ] {
        if let Some(beamcon0) = beamcon0 {
            amiga.poke_word(BEAMCON0, beamcon0);
        }
        configure_hires_clean_idle_candidate(&mut amiga);
        advance_to_line(&mut amiga, 0x0030);
        assert_eq!(amiga.agnus().ddf_start_match(), None);
        amiga.poke_word(DDFSTOP, 0x0038);
        let line_bases = amiga.agnus().bpl_pt;
        run_reference_boundary_line(&mut amiga, reference_case);

        for ((plane, base), pointer) in line_bases
            .into_iter()
            .enumerate()
            .take(4)
            .zip(amiga.agnus().bpl_pt)
        {
            assert_eq!(pointer, base + 84, "BPL{} {case} byte count", plane + 1,);
        }
        assert_eq!(
            amiga.agnus().dma_pipeline().address().is_some(),
            reference_case == 6,
            "only HARDDIS retains the extra $E2 request across wrap"
        );
    }
}
