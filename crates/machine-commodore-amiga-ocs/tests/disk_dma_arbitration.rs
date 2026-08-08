//! Disk rotation and disk memory traffic are separate clocks.
//!
//! Paula receives encoded words from the drive independently. Only the
//! fixed disk cells granted by Agnus may move those words into chip RAM.

use machine_commodore_amiga_ocs::AmigaOcs;

fn halt_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 512 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8] = 0x60;
    rom[9] = 0xFE;
    rom
}

fn arm_read_dma(amiga: &mut AmigaOcs, base: u32, words: u16, enable_disk_dma: bool) {
    amiga.poke_word(0x00DF_F020, (base >> 16) as u16);
    amiga.poke_word(0x00DF_F022, base as u16);
    if enable_disk_dma {
        amiga.poke_word(0x00DF_F096, 0x8000 | 0x0200 | 0x0010);
    }
    let dsklen = 0x8000 | words;
    amiga.poke_word(0x00DF_F024, dsklen);
    amiga.poke_word(0x00DF_F024, dsklen);
}

#[test]
fn read_fifo_reaches_chip_ram_only_on_agnus_disk_cells() {
    let mut amiga = AmigaOcs::new(halt_rom());
    let base = 0x0000_1000;
    arm_read_dma(&mut amiga, base, 3, true);
    for word in [0x1111, 0x2222, 0x3333] {
        amiga.paula_mut().receive_disk_read_word(word);
    }

    let mut pointer = amiga.agnus().dsk_pt;
    let mut service_hpos = Vec::new();
    for _ in 0..64 {
        amiga.tick();
        if amiga.agnus().dsk_pt != pointer {
            pointer = amiga.agnus().dsk_pt;
            service_hpos.push(amiga.agnus().hpos);
        }
    }

    assert_eq!(service_hpos, [0x07, 0x09, 0x0B]);
    assert_eq!(amiga.memory().read_chip_ram_word(base), 0x1111);
    assert_eq!(amiga.memory().read_chip_ram_word(base + 2), 0x2222);
    assert_eq!(amiga.memory().read_chip_ram_word(base + 4), 0x3333);
    assert_eq!(amiga.agnus().dsk_pt, base + 6);
    assert_eq!(amiga.intreq() & 0x0002, 0x0002);
}

#[test]
fn partial_read_fifo_uses_the_trailing_fixed_cells() {
    let cases: &[(u16, &[u16])] = &[(1, &[0x0B]), (2, &[0x09, 0x0B])];

    for &(word_count, expected_hpos) in cases {
        let mut amiga = AmigaOcs::new(halt_rom());
        let base = 0x0000_1000;
        arm_read_dma(&mut amiga, base, word_count, true);
        for word in [0x1111, 0x2222].into_iter().take(usize::from(word_count)) {
            amiga.paula_mut().receive_disk_read_word(word);
        }

        let mut pointer = amiga.agnus().dsk_pt;
        let mut service_hpos = Vec::new();
        for _ in 0..64 {
            amiga.tick();
            if amiga.agnus().dsk_pt != pointer {
                pointer = amiga.agnus().dsk_pt;
                service_hpos.push(amiga.agnus().hpos);
            }
        }

        assert_eq!(service_hpos, expected_hpos, "word_count={word_count}");
    }
}

#[test]
fn rotation_updates_paula_while_a_cleared_dsken_blocks_memory_traffic() {
    let mut amiga = AmigaOcs::new(halt_rom());
    let base = 0x0000_1000;
    arm_read_dma(&mut amiga, base, 1, false);
    amiga.paula_mut().receive_disk_read_word(0xA55A);

    for _ in 0..64 {
        amiga.tick();
    }

    let disk = amiga.paula().disk_diagnostic_snapshot();
    assert_eq!(disk.dskdatr, 0xA55A);
    assert_eq!(disk.dskbytr_data, 0xA5);
    assert_eq!(disk.disk_dma_fifo, [0xA55A]);
    assert_eq!(amiga.agnus().dsk_pt, base);
    assert_eq!(amiga.memory().read_chip_ram_word(base), 0);
    assert!(amiga.paula().disk_dma_pending());
}

#[test]
fn idle_paula_releases_enabled_disk_cells_to_the_cpu() {
    let mut amiga = AmigaOcs::new(halt_rom());
    amiga.poke_word(0x00DF_F096, 0x8000 | 0x0200 | 0x0010);

    while amiga.agnus().hpos != 0x07 {
        amiga.tick();
    }

    let scheduled = amiga.agnus().cck_bus_plan();
    assert!(scheduled.disk_dma_slot_granted);

    let requested = amiga
        .agnus()
        .cck_bus_plan_with_disk_request(amiga.paula().disk_dma_slot_requested());
    assert_eq!(requested.slot_owner, commodore_agnus_ocs::SlotOwner::Cpu);
    assert!(!requested.disk_dma_slot_granted);
    assert!(requested.cpu_chip_bus_granted);
}
