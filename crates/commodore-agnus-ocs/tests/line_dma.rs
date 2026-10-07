//! Line-stage bus observations bounded by the standard width-two setup.
use commodore_agnus_ocs::{Agnus, BlitterBus, bits};

#[derive(Default)]
struct Bus {
    cck: u32,
    events: Vec<(u32, char, u32, u16)>,
}
impl BlitterBus for Bus {
    fn read_word(&mut self, addr: u32) -> u16 {
        let word = if addr == 0x1000 { 1 } else { 0 };
        self.events.push((self.cck, 'R', addr, word));
        word
    }
    fn write_word(&mut self, addr: u32, word: u16) {
        self.events.push((self.cck, 'W', addr, word));
    }
}

fn line(use_b: bool, use_c: bool, steps: u16) -> Agnus {
    let mut chip = Agnus::new();
    chip.bltcon0 = 0x08CA | if use_b { 0x0400 } else { 0 } | if use_c { 0x0200 } else { 0 };
    chip.bltcon1 = 0x19; // X-major, positive X, unchanged negative error
    chip.blt_apt = 0xFFFF;
    chip.blt_afwm = 0xFFFF;
    chip.blt_bdat = 0;
    chip.blt_bpt = 0x1000;
    chip.blt_bmod = -2;
    chip.blt_cpt = 0x2000;
    chip.blt_dpt = 0x3000;
    chip.bltsize = (steps << 6) | 2;
    chip.dmacon = bits::DMACON_DMAEN | bits::DMACON_BLTEN | bits::DMACON_BLTPRI;
    chip.hpos = 0x35;
    chip.start_blit();
    chip
}

#[test]
fn standard_line_has_four_stages_and_internal_cycles_leave_the_bus_free() {
    let mut chip = line(false, true, 1);
    let mut bus = Bus::default();
    for _ in 0..2 {
        let _ = chip.tick_blitter_cck(true, &mut bus);
    }
    for (i, expected_bus) in [false, true, false, true].into_iter().enumerate() {
        bus.cck = i as u32 + 1;
        let plan = chip.cck_bus_plan();
        assert_eq!(plan.blitter_chip_bus_granted, expected_bus);
        assert_eq!(plan.cpu_chip_bus_granted, !expected_bus);
        let outcome = chip.tick_blitter_cck(plan.blitter_dma_progress_granted, &mut bus);
        assert_eq!(outcome.bus_used, expected_bus);
        assert_eq!(outcome.interrupt, i == 3);
        if i == 2 {
            assert!(bus.events.iter().all(|event| event.1 != 'W'));
            assert_eq!(
                chip.blitter_diagnostic_snapshot()
                    .line
                    .map(|s| s.pending_addr),
                Some(0x3000)
            );
        }
    }
    assert_eq!(bus.events, [(2, 'R', 0x2000, 0), (4, 'W', 0x3000, 0)]);
    assert!(!chip.blitter_busy);
}

#[test]
fn optional_b_fetch_changes_texture_and_uses_modulo_without_area_increment() {
    let mut chip = line(true, true, 2);
    let mut bus = Bus::default();
    for _ in 0..2 {
        let _ = chip.tick_blitter_cck(true, &mut bus);
    }
    for cck in 1..=12 {
        bus.cck = cck;
        let bus_expected = matches!((cck - 1) % 6, 1 | 2 | 4 | 5);
        let plan = chip.cck_bus_plan();
        assert_eq!(plan.blitter_chip_bus_granted, bus_expected);
        let outcome = chip.tick_blitter_cck(plan.blitter_dma_progress_granted, &mut bus);
        assert_eq!(outcome.bus_used, bus_expected);
        assert_eq!(outcome.interrupt, cck == 12);
    }
    assert_eq!(
        bus.events,
        [
            (2, 'R', 0x1000, 1),
            (3, 'R', 0x2000, 0),
            (6, 'W', 0x3000, 0x8000),
            (8, 'R', 0x0FFE, 0),
            (9, 'R', 0x2000, 0),
            (12, 'W', 0x2000, 0),
        ]
    );
    assert_eq!(chip.blt_bpt, 0x0FFC);
    assert!(!chip.blitter_busy);
}

#[test]
fn c_disable_removes_both_c_read_and_d_write_independently_of_d_enable() {
    for use_b in [false, true] {
        let mut chip = line(use_b, false, 1);
        chip.bltcon0 |= 0x0100; // D alone cannot enable line drawing.
        // Restart to capture the same controls as a register-programmed blit.
        chip.start_blit();
        let mut bus = Bus::default();
        let _ = chip.run_blit_to_completion(&mut bus);
        assert!(
            bus.events
                .iter()
                .all(|event| event.1 == 'R' && event.2 == 0x1000)
        );
        assert_eq!(bus.events.len(), usize::from(use_b));
    }
}
