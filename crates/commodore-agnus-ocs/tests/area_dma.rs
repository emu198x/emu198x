//! Actual area DMA streams versus the compiled registered reference table.
use commodore_agnus_ocs::{Agnus, BlitterBus, BlitterDmaOp, bits};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Bus {
    cck: u32,
    memory: BTreeMap<u32, u16>,
    events: Vec<(u32, char, u32, u16)>,
}
impl BlitterBus for Bus {
    fn read_word(&mut self, address: u32) -> u16 {
        let value = self.memory.get(&address).copied().unwrap_or(0);
        self.events.push((self.cck, 'R', address, value));
        value
    }
    fn write_word(&mut self, address: u32, value: u16) {
        self.events.push((self.cck, 'W', address, value));
        self.memory.insert(address, value);
    }
}
fn chip(mode: u16, fill: bool, width: u16) -> Agnus {
    let mut chip = Agnus::new();
    chip.bltcon0 = (mode << 8) | 0xff;
    chip.bltcon1 = if fill { 0x000a } else { 0 };
    chip.blt_apt = 0x1000;
    chip.blt_bpt = 0x2000;
    chip.blt_cpt = 0x3000;
    chip.blt_dpt = 0x4000;
    chip.blt_afwm = 0xffff;
    chip.blt_alwm = 0xffff;
    chip.bltsize = (1 << 6) | width;
    chip.dmacon = bits::DMACON_DMAEN | bits::DMACON_BLTEN | bits::DMACON_BLTPRI;
    chip.hpos = 0x35;
    chip.start_blit();
    chip
}
fn startup(chip: &mut Agnus, bus: &mut Bus) {
    for _ in 0..2 {
        let result = chip.tick_blitter_cck(true, bus);
        assert!(!result.bus_used);
    }
    assert!(bus.events.is_empty());
}
#[test]
fn all_32_channel_fill_programs_match_real_transfers_and_bus_ownership() {
    let mut count = 0;
    let mut identities = BTreeSet::new();
    for row in include_str!("../../../test-data/commodore/amiga/area-dma/reference-programs.tsv")
        .lines()
        .filter(|row| !row.starts_with('#'))
    {
        let values: Vec<u16> = row
            .split_whitespace()
            .map(|v| v.parse().expect("reference integer"))
            .collect();
        let (mode, fill) = (values[0], values[1] != 0);
        assert!(mode < 16 && values[1] < 2 && identities.insert((mode, fill)));
        let flags = &values[2..];
        assert!(flags.len() >= 2 && flags.last().is_some_and(|v| v & 2048 != 0));
        let mut chip = chip(mode, fill, 4);
        let mut bus = Bus::default();
        startup(&mut chip, &mut bus);
        let mut expected = Vec::new();
        let mut pointers = [0x1000, 0x2000, 0x3000, 0x4000];
        for word in 0..4 {
            for (phase, flag) in flags.iter().enumerate() {
                bus.cck = (word * flags.len() + phase + 1) as u32;
                let channel = if flag & 8 != 0 {
                    Some(0)
                } else if flag & 16 != 0 {
                    Some(1)
                } else if flag & 32 != 0 {
                    Some(2)
                } else if flag & 4 != 0 {
                    Some(3)
                } else {
                    None
                };
                let request = match channel {
                    Some(0) => BlitterDmaOp::ReadA,
                    Some(1) => BlitterDmaOp::ReadB,
                    Some(2) => BlitterDmaOp::ReadC,
                    Some(3) => BlitterDmaOp::WriteD,
                    _ => BlitterDmaOp::Internal,
                };
                assert_eq!(
                    chip.next_blitter_dma_request(),
                    Some(request),
                    "mode={mode},fill={fill},word={word},phase={phase}"
                );
                let uses_bus = channel.is_some_and(|ch| ch != 3 || word != 0);
                assert_eq!(chip.next_blitter_progress_uses_bus(), uses_bus);
                let plan = chip.cck_bus_plan();
                assert_eq!(plan.cpu_chip_bus_granted, !uses_bus);
                let outcome = chip.tick_blitter_cck(true, &mut bus);
                assert_eq!(outcome.bus_used, uses_bus);
                if let Some(ch) = channel.filter(|_| uses_bus) {
                    expected.push((bus.cck, if ch == 3 { 'W' } else { 'R' }, pointers[ch]));
                    pointers[ch] = if fill {
                        pointers[ch] - 2
                    } else {
                        pointers[ch] + 2
                    };
                }
            }
        }
        if mode & 1 != 0 {
            assert!(chip.blitter_busy);
            assert_eq!(chip.blitter_completion_phase(), "final-result");
            bus.cck += 1;
            let result = chip.tick_blitter_cck(false, &mut bus);
            assert!(!result.bus_used);
            bus.cck += 1;
            let result = chip.tick_blitter_cck(true, &mut bus);
            assert!(result.bus_used);
            expected.push((bus.cck, 'W', pointers[3]));
        }
        assert!(!chip.blitter_busy);
        assert_eq!(chip.blitter_ccks_remaining, 0);
        let actual: Vec<_> = bus
            .events
            .iter()
            .map(|&(time, kind, addr, _)| (time, kind, addr))
            .collect();
        assert_eq!(actual, expected, "mode={mode},fill={fill}");
        assert_eq!(
            bus.events.iter().filter(|event| event.1 == 'W').count(),
            if mode & 1 != 0 { 4 } else { 0 }
        );
        count += 1;
    }
    assert_eq!(count, 32, "missing reference rows must fail");
}
#[test]
fn source_reads_overlap_previous_d_without_losing_masked_shifted_words() {
    let mut chip = chip(9, false, 3);
    chip.bltcon0 = 0x39f0;
    chip.blt_dpt = 0x1002;
    chip.blt_afwm = 0x0fff;
    chip.blt_alwm = 0xfff0;
    chip.start_blit();
    let mut bus = Bus::default();
    bus.memory
        .extend([(0x1000, 0xf123), (0x1002, 0xabcd), (0x1004, 0x567f)]);
    startup(&mut chip, &mut bus);
    for cck in 1..=8 {
        bus.cck = cck;
        let _ = chip.tick_blitter_cck(true, &mut bus);
    }
    assert_eq!(
        bus.events,
        [
            (1, 'R', 0x1000, 0xf123),
            (3, 'R', 0x1002, 0xabcd),
            (4, 'W', 0x1002, 0x0024),
            (5, 'R', 0x1004, 0x567f),
            (6, 'W', 0x1004, 0x7579),
            (8, 'W', 0x1006, 0xaace)
        ]
    );
    assert_eq!(chip.blt_apt, 0x1006);
    assert_eq!(chip.blt_dpt, 0x1008);
    assert!(!chip.blitter_busy);
}
#[test]
fn idle_and_locked_d_need_a_free_cell_but_yield_the_physical_bus() {
    let mut chip = chip(1, true, 2);
    let mut bus = Bus::default();
    startup(&mut chip, &mut bus);
    for cck in 1..=3 {
        bus.cck = cck;
        assert!(!chip.next_blitter_progress_uses_bus());
        let before = chip.blitter_diagnostic_snapshot();
        let result = chip.tick_blitter_cck(false, &mut bus);
        assert!(!result.bus_used);
        assert_eq!(chip.blitter_diagnostic_snapshot(), before);
        let result = chip.tick_blitter_cck(true, &mut bus);
        assert!(!result.bus_used);
    }
    assert!(bus.events.is_empty());
    let _ = chip.run_blit_to_completion(&mut bus);
    assert_eq!(bus.events.len(), 2);
}

#[test]
fn descending_channels_apply_independent_modulos_before_delayed_d() {
    let mut chip = chip(15, false, 2);
    chip.bltcon0 = 0x0fcc; // D=B, while all source channels really fetch.
    chip.bltcon1 = 0x4002; // descending, B shift four
    chip.blt_apt = 0x1006;
    chip.blt_bpt = 0x2006;
    chip.blt_cpt = 0x3006;
    chip.blt_dpt = 0x4006;
    chip.blt_amod = 4;
    chip.blt_bmod = 6;
    chip.blt_cmod = 8;
    chip.blt_dmod = 10;
    chip.bltsize = 0x0082;
    chip.start_blit();
    let mut bus = Bus::default();
    bus.memory
        .extend([(0x2006, 1), (0x2004, 2), (0x1ffc, 4), (0x1ffa, 8)]);
    startup(&mut chip, &mut bus);
    for cck in 1..=18 {
        bus.cck = cck;
        let _ = chip.tick_blitter_cck(true, &mut bus);
        if cck == 5 {
            assert_eq!(
                chip.blitter_diagnostic_snapshot().area.map(|area| area.apt),
                Some(0x0ffe)
            );
        }
    }
    let writes: Vec<_> = bus
        .events
        .iter()
        .filter(|event| event.1 == 'W')
        .copied()
        .collect();
    assert_eq!(
        writes,
        [
            (8, 'W', 0x4006, 0x10),
            (12, 'W', 0x4004, 0x20),
            (16, 'W', 0x3ff8, 0x40),
            (18, 'W', 0x3ff6, 0x80)
        ]
    );
    let reads: Vec<_> = bus
        .events
        .iter()
        .filter(|event| event.1 == 'R')
        .map(|event| event.2)
        .collect();
    assert_eq!(
        reads,
        [
            0x1006, 0x2006, 0x3006, 0x1004, 0x2004, 0x3004, 0x0ffe, 0x1ffc, 0x2ffa, 0x0ffc, 0x1ffa,
            0x2ff8
        ]
    );
    assert_eq!(
        (chip.blt_apt, chip.blt_bpt, chip.blt_cpt, chip.blt_dpt),
        (0x0ff6, 0x1ff2, 0x2fee, 0x3fea)
    );
}

#[test]
fn fill_carry_reloads_for_each_result_row_before_the_output_pipeline_drains() {
    for use_d in [false, true] {
        let mut chip = chip(if use_d { 9 } else { 8 }, true, 2);
        chip.bltcon0 = if use_d { 0x09f0 } else { 0x08f0 }; // D=A
        chip.bltsize = 0x0082;
        chip.start_blit();
        let mut bus = Bus::default();
        bus.memory.insert(0x1000, 1);
        startup(&mut chip, &mut bus);
        let _ = chip.run_blit_to_completion(&mut bus);
        assert!(!chip.blitter_dzero);
        let writes: Vec<_> = bus
            .events
            .iter()
            .filter(|event| event.1 == 'W')
            .map(|event| (event.2, event.3))
            .collect();
        assert_eq!(
            writes,
            if use_d {
                vec![(0x4000, 0xffff), (0x3ffe, 0xffff), (0x3ffc, 0), (0x3ffa, 0)]
            } else {
                vec![]
            }
        );
        assert_eq!(bus.events.iter().filter(|event| event.1 == 'R').count(), 4);
    }
}

#[test]
fn disabled_b_retains_its_captured_hold_while_b_dma_advances_the_barrel_shifter() {
    for use_b in [false, true] {
        let mut chip = chip(if use_b { 5 } else { 1 }, false, 3);
        chip.bltcon0 = if use_b { 0x05cc } else { 0x01cc };
        chip.bltcon1 = 0x4000;
        chip.blt_bdat = 0xf00f;
        chip.start_blit();
        let mut bus = Bus::default();
        bus.memory
            .extend([(0x2000, 0xf00f), (0x2002, 0x8001), (0x2004, 0)]);
        startup(&mut chip, &mut bus);
        let _ = chip.run_blit_to_completion(&mut bus);
        let words: Vec<_> = bus
            .events
            .iter()
            .filter(|event| event.1 == 'W')
            .map(|event| event.3)
            .collect();
        assert_eq!(
            words,
            if use_b {
                vec![0x0f00, 0xf800, 0x1000]
            } else {
                vec![0x0f00; 3]
            }
        );
        assert_eq!(
            bus.events.iter().filter(|event| event.1 == 'R').count(),
            if use_b { 3 } else { 0 }
        );
    }
}
