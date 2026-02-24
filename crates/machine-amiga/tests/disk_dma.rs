use machine_amiga::format_adf::{Adf, ADF_SIZE_DD, SECTOR_SIZE};
use machine_amiga::memory::ROM_BASE;
use machine_amiga::{Amiga, AmigaBusWrapper, TICKS_PER_CCK};
use motorola_68000::bus::{BusStatus, FunctionCode, M68kBus};

const CUSTOM_DSKDATR_ADDR: u32 = 0x00DFF008;
const CUSTOM_DSKBYTR_ADDR: u32 = 0x00DFF01A;
const REG_DSKPTH: u16 = 0x020;
const REG_DSKPTL: u16 = 0x022;
const REG_DSKLEN: u16 = 0x024;
const REG_DMACON: u16 = 0x096;
const REG_DSKSYNC: u16 = 0x07E;

const DMACON_DSKEN: u16 = 0x0010;
const DMACON_DMAEN: u16 = 0x0200;
const INTREQ_DSKBLK: u16 = 0x0002;
const INTREQ_DSKSYN: u16 = 0x1000;

fn make_test_amiga() -> Amiga {
    let mut rom = vec![0u8; 256 * 1024];

    let ssp = 0x0007FFF0u32;
    rom[0] = (ssp >> 24) as u8;
    rom[1] = (ssp >> 16) as u8;
    rom[2] = (ssp >> 8) as u8;
    rom[3] = ssp as u8;

    let pc = ROM_BASE + 8;
    rom[4] = (pc >> 24) as u8;
    rom[5] = (pc >> 16) as u8;
    rom[6] = (pc >> 8) as u8;
    rom[7] = pc as u8;

    // BRA.S *
    rom[8] = 0x60;
    rom[9] = 0xFE;

    Amiga::new(rom)
}

fn tick_ccks(amiga: &mut Amiga, ccks: u32) {
    for _ in 0..ccks {
        for _ in 0..TICKS_PER_CCK {
            amiga.tick();
        }
    }
}

fn write_dsk_ptr(amiga: &mut Amiga, addr: u32) {
    amiga.write_custom_reg(REG_DSKPTH, (addr >> 16) as u16);
    amiga.write_custom_reg(REG_DSKPTL, (addr & 0xFFFF) as u16);
}

fn read_custom_word_via_cpu_bus(amiga: &mut Amiga, addr: u32) -> u16 {
    let mut bus = AmigaBusWrapper {
        agnus: &mut amiga.agnus,
        memory: &mut amiga.memory,
        denise: &mut amiga.denise,
        copper: &mut amiga.copper,
        cia_a: &mut amiga.cia_a,
        cia_b: &mut amiga.cia_b,
        paula: &mut amiga.paula,
        floppy: &mut amiga.floppy,
        keyboard: &mut amiga.keyboard,
    };
    match M68kBus::poll_cycle(
        &mut bus,
        addr,
        FunctionCode::SupervisorData,
        true,
        true,
        None,
    ) {
        BusStatus::Ready(v) => v,
        other => panic!("expected ready custom register read, got {other:?}"),
    }
}

fn make_test_adf() -> Adf {
    let mut adf = Adf::from_bytes(vec![0u8; ADF_SIZE_DD]).expect("valid DD ADF");

    let mut sector0 = vec![0u8; SECTOR_SIZE as usize];
    let mut sector1 = vec![0u8; SECTOR_SIZE as usize];
    for (i, b) in sector0.iter_mut().enumerate() {
        *b = i as u8;
    }
    for (i, b) in sector1.iter_mut().enumerate() {
        *b = 0xA5 ^ (i as u8);
    }
    adf.write_sector(0, 0, 0, &sector0);
    adf.write_sector(0, 0, 1, &sector1);
    adf
}

#[test]
fn disk_dma_read_is_slot_timed_and_fires_dskblk_on_completion() {
    let mut amiga = make_test_amiga();
    amiga.insert_disk(make_test_adf());

    let expected_mfm = amiga
        .floppy
        .encode_mfm_track()
        .expect("inserted disk should encode to MFM track data");

    let dst = 0x0000_2000u32;
    let word_count = 4u16;
    let byte_count = usize::from(word_count) * 2;
    for i in 0..(byte_count as u32 + 8) {
        amiga.memory.write_byte(dst + i, 0xEE);
    }

    // Deterministic beam position so early slot timing is predictable.
    amiga.agnus.vpos = 0;
    amiga.agnus.hpos = 0;

    write_dsk_ptr(&mut amiga, dst);
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_DSKEN);
    amiga.paula.intreq &= !INTREQ_DSKBLK;

    // Double-write starts disk DMA in Paula; machine starts the transfer on the
    // next CCK, but data movement should still wait for Agnus disk slots.
    amiga.write_custom_reg(REG_DSKLEN, 0x8000 | word_count);
    amiga.write_custom_reg(REG_DSKLEN, 0x8000 | word_count);
    assert_eq!(amiga.agnus.dsk_pt, dst);
    assert_eq!(amiga.paula.intreq & INTREQ_DSKBLK, 0);

    // Starting from hpos=0, the transfer is armed after the first CCK. The next
    // three CCKs are refresh slots (hpos 1..3), so no disk word should move yet.
    tick_ccks(&mut amiga, 4);
    assert_eq!(
        amiga.agnus.dsk_pt, dst,
        "disk DMA must not transfer before the first disk slot"
    );
    assert_eq!(
        amiga.paula.intreq & INTREQ_DSKBLK,
        0,
        "DSKBLK must not fire before the final disk slot"
    );

    let mut elapsed_ccks = 4u32;
    let mut disk_slot_grants = 0u32;
    let mut words_transferred = 0u32;
    while (amiga.paula.intreq & INTREQ_DSKBLK) == 0 && elapsed_ccks < 2_000 {
        let plan = amiga.agnus.cck_bus_plan();
        if plan.disk_dma_slot_granted {
            disk_slot_grants += 1;
        }

        let ptr_before = amiga.agnus.dsk_pt;
        tick_ccks(&mut amiga, 1);
        elapsed_ccks += 1;
        let ptr_after = amiga.agnus.dsk_pt;

        let delta = ptr_after.wrapping_sub(ptr_before);
        match delta {
            0 => {}
            2 => {
                words_transferred += 1;
                assert!(
                    plan.disk_dma_slot_granted,
                    "DSKPT advanced outside an Agnus disk slot"
                );
                if words_transferred < u32::from(word_count) {
                    assert_eq!(
                        amiga.paula.intreq & INTREQ_DSKBLK,
                        0,
                        "DSKBLK should not fire before the final disk word"
                    );
                }
            }
            _ => panic!("unexpected DSKPT delta {delta}"),
        }
    }

    assert_ne!(
        amiga.paula.intreq & INTREQ_DSKBLK,
        0,
        "DSKBLK should fire once the requested disk words have been serviced"
    );
    assert_eq!(
        words_transferred,
        u32::from(word_count),
        "disk DMA should transfer exactly one word per granted disk slot"
    );
    assert_eq!(
        disk_slot_grants,
        u32::from(word_count),
        "completion should occur on the final granted disk slot"
    );
    assert_eq!(
        amiga.agnus.dsk_pt,
        dst + u32::from(word_count) * 2,
        "DSKPT should advance by two bytes per transferred word"
    );

    for (i, expected) in expected_mfm.iter().take(byte_count).copied().enumerate() {
        let got = amiga.memory.read_chip_byte(dst + i as u32);
        assert_eq!(got, expected, "disk DMA byte mismatch at offset {i}");
    }
}

#[test]
fn disk_dma_read_raises_dsksyn_on_matching_stream_word() {
    let mut amiga = make_test_amiga();
    amiga.insert_disk(make_test_adf());

    let dst = 0x0000_2400u32;

    // Start from the beginning of an Amiga MFM sector stream:
    // first two words are $AAAA gap, then sync words $4489.
    let word_count = 4u16;
    amiga.agnus.vpos = 0;
    amiga.agnus.hpos = 0;
    write_dsk_ptr(&mut amiga, dst);
    amiga.write_custom_reg(REG_DSKSYNC, 0x4489);
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_DSKEN);
    amiga.paula.intreq &= !INTREQ_DSKSYN;

    amiga.write_custom_reg(REG_DSKLEN, 0x8000 | word_count);
    amiga.write_custom_reg(REG_DSKLEN, 0x8000 | word_count);

    let mut transferred_words = 0u32;
    let mut elapsed_ccks = 0u32;
    while transferred_words < u32::from(word_count) && elapsed_ccks < 2_000 {
        let ptr_before = amiga.agnus.dsk_pt;
        tick_ccks(&mut amiga, 1);
        elapsed_ccks += 1;
        let ptr_after = amiga.agnus.dsk_pt;
        if ptr_after.wrapping_sub(ptr_before) == 2 {
            transferred_words += 1;
            if transferred_words <= 2 {
                assert_eq!(
                    amiga.paula.intreq & INTREQ_DSKSYN,
                    0,
                    "DSKSYN should not fire on the leading gap words"
                );
            }
        }
    }

    assert_eq!(transferred_words, u32::from(word_count));
    assert_ne!(
        amiga.paula.intreq & INTREQ_DSKSYN,
        0,
        "DSKSYN should fire once a transferred disk DMA word matches DSKSYNC"
    );

    let sync_word_hi = amiga.memory.read_chip_byte(dst + 4);
    let sync_word_lo = amiga.memory.read_chip_byte(dst + 5);
    assert_eq!(
        (u16::from(sync_word_hi) << 8) | u16::from(sync_word_lo),
        0x4489,
        "test assumes the third DMA word is the first MFM sync word"
    );
}

#[test]
fn disk_dma_read_updates_dskdatr_with_last_transferred_word() {
    let mut amiga = make_test_amiga();
    amiga.insert_disk(make_test_adf());

    amiga.agnus.vpos = 0;
    amiga.agnus.hpos = 0;
    write_dsk_ptr(&mut amiga, 0x0000_2C00);
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_DSKEN);

    amiga.write_custom_reg(REG_DSKLEN, 0x8000 | 3);
    amiga.write_custom_reg(REG_DSKLEN, 0x8000 | 3);

    let mut transferred_words = 0u32;
    let mut last_word = 0u16;
    let mut elapsed_ccks = 0u32;
    while transferred_words < 3 && elapsed_ccks < 2_000 {
        let ptr_before = amiga.agnus.dsk_pt;
        tick_ccks(&mut amiga, 1);
        elapsed_ccks += 1;
        let ptr_after = amiga.agnus.dsk_pt;
        if ptr_after.wrapping_sub(ptr_before) == 2 {
            transferred_words += 1;
            let word = read_custom_word_via_cpu_bus(&mut amiga, CUSTOM_DSKDATR_ADDR);
            last_word = word;
            match transferred_words {
                1 | 2 => assert_eq!(word, 0xAAAA, "gap words should read back via DSKDATR"),
                3 => assert_eq!(word, 0x4489, "first sync word should read back via DSKDATR"),
                _ => unreachable!(),
            }
        }
    }

    assert_eq!(transferred_words, 3);
    assert_eq!(last_word, 0x4489);
}

#[test]
fn disk_dma_read_updates_dskbytr_flags_and_data() {
    let mut amiga = make_test_amiga();
    amiga.insert_disk(make_test_adf());

    amiga.agnus.vpos = 0;
    amiga.agnus.hpos = 0;
    write_dsk_ptr(&mut amiga, 0x0000_3000);
    amiga.write_custom_reg(REG_DSKSYNC, 0x4489);
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_DSKEN);

    amiga.write_custom_reg(REG_DSKLEN, 0x8000 | 3);
    amiga.write_custom_reg(REG_DSKLEN, 0x8000 | 3);

    let mut transferred_words = 0u32;
    let mut elapsed_ccks = 0u32;
    while transferred_words < 3 && elapsed_ccks < 2_000 {
        let ptr_before = amiga.agnus.dsk_pt;
        tick_ccks(&mut amiga, 1);
        elapsed_ccks += 1;
        let ptr_after = amiga.agnus.dsk_pt;
        if ptr_after.wrapping_sub(ptr_before) == 2 {
            transferred_words += 1;
            let first = read_custom_word_via_cpu_bus(&mut amiga, CUSTOM_DSKBYTR_ADDR);
            assert_ne!(first & 0x8000, 0, "DSKBYT should be set on first byte");
            assert_ne!(first & 0x4000, 0, "DMAON should reflect active disk DMA");
            assert_eq!(first & 0x2000, 0, "DISKWRITE should be clear for read DMA");

            let second = read_custom_word_via_cpu_bus(&mut amiga, CUSTOM_DSKBYTR_ADDR);
            assert_ne!(
                second & 0x8000,
                0,
                "DSKBYT should be set again for the second byte of the same word"
            );
            assert_ne!(
                second & 0x4000,
                0,
                "DMAON should remain set during disk DMA"
            );

            match transferred_words {
                1 | 2 => {
                    assert_eq!(
                        first & 0x00FF,
                        0x00AA,
                        "gap high byte should be visible first"
                    );
                    assert_eq!(second & 0x00FF, 0x00AA, "gap low byte should follow");
                    assert_eq!(first & 0x1000, 0, "WORDEQUAL should be clear on gap words");
                    assert_eq!(second & 0x1000, 0, "WORDEQUAL should be clear on gap words");
                }
                3 => {
                    assert_eq!(
                        first & 0x00FF,
                        0x0044,
                        "sync high byte should be visible first"
                    );
                    assert_eq!(second & 0x00FF, 0x0089, "sync low byte should follow");
                    assert_ne!(
                        first & 0x1000,
                        0,
                        "WORDEQUAL should be set on DSKSYNC match"
                    );
                    assert_ne!(
                        second & 0x1000,
                        0,
                        "WORDEQUAL should persist across the matched word bytes"
                    );
                }
                _ => unreachable!(),
            }

            let third = read_custom_word_via_cpu_bus(&mut amiga, CUSTOM_DSKBYTR_ADDR);
            assert_eq!(
                third & 0x8000,
                0,
                "DSKBYT must clear once both bytes are consumed"
            );
        }
    }

    assert_eq!(transferred_words, 3);
}
