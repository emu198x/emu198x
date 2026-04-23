//! Paula 8364 Phase 2 machine-level integration tests.
//!
//! Verifies the port moved INTENA/INTREQ/ADKCON from the inline
//! chipset into Paula without behavioural regression. The chip-level
//! behaviour is covered in `crates/commodore-paula-8364-archive/tests/`;
//! these tests exercise the *wiring* through the Amiga custom-register
//! bus, the 68000 IPL line, and the CIA→Paula edge latches.

use machine_commodore_amiga_ocs::{AmigaOcs, AudioField, IntSource};

fn zero_rom() -> Vec<u8> {
    vec![0; 512 * 1024]
}

// ─── CPU custom-register access → Paula ────────────────────────────

#[test]
fn intena_write_lands_in_paula_and_intenar_read_round_trips() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // Write INTENA at $DFF09A — SET INTEN + VERTB.
    amiga.poke_word(0x00DF_F09A, 0xC020);
    assert_eq!(amiga.intena(), 0x4020);

    // INTENAR at $DFF01C must read back the same value.
    assert_eq!(amiga.read_word(0x00DF_F01C), 0x4020);
}

#[test]
fn intreq_write_lands_in_paula_and_intreqr_read_round_trips() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09C, 0x8040); // SET BLIT
    assert_eq!(amiga.intreq(), 0x0040);
    assert_eq!(amiga.read_word(0x00DF_F01E), 0x0040);
}

#[test]
fn adkcon_write_lands_in_paula_and_adkconr_read_round_trips() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09E, 0x8100); // SET FAST
    assert_eq!(amiga.adkcon(), 0x0100);
    assert_eq!(amiga.read_word(0x00DF_F010), 0x0100);
}

// ─── INTENA clear-doesn't-touch-master guards read-modify-write ────

#[test]
fn intena_clear_of_one_source_leaves_master_untouched() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09A, 0xC020); // SET INTEN + VERTB
    assert_eq!(amiga.intena(), 0x4020);
    amiga.poke_word(0x00DF_F09A, 0x0020); // CLEAR VERTB only
    assert_eq!(amiga.intena(), 0x4000, "master-enable must persist");
}

// ─── CIA /IRQ edges reach Paula's INTREQ ──────────────────────────

#[test]
fn cia_a_irq_edge_sets_intreq_ports() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // Enable ICR.SP mask on CIA-A and inject a serial byte — that
    // raises CIA-A /IRQ level-sensitively. Paula edge-latches.
    amiga.poke_byte(0x00BF_ED01, 0x88); // ICR mask SET | SP bit
    amiga.cia_a_mut().receive_serial_byte(0);
    for _ in 0..20 {
        amiga.tick();
    }
    assert_ne!(amiga.intreq() & IntSource::Ports.mask(), 0);
}

#[test]
fn cia_b_flag_pulse_sets_intreq_exter() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_byte(0x00BF_DD00, 0x90); // CIA-B ICR: SET | FLG
    amiga.cia_b_mut().flag_falling_edge();
    for _ in 0..20 {
        amiga.tick();
    }
    assert_ne!(amiga.intreq() & IntSource::Exter.mask(), 0);
}

// ─── IPL reflects Paula state ──────────────────────────────────────

#[test]
fn master_enable_clear_keeps_cpu_ipl_at_zero() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // Raise a high-priority source without enabling INTEN.
    amiga.poke_word(0x00DF_F09C, 0xA000); // SET EXTER
    // tick once to let compute_ipl propagate to the CPU.
    amiga.tick();
    assert_eq!(
        amiga.cpu().ipl,
        0,
        "INTEN clear → IPL = 0 regardless of pending bits"
    );
}

#[test]
fn master_enable_plus_source_raises_cpu_ipl_to_matching_level() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09A, 0xE000); // SET INTEN + EXTER
    amiga.poke_word(0x00DF_F09C, 0xA000); // SET EXTER
    amiga.tick();
    assert_eq!(
        amiga.cpu().ipl,
        6,
        "EXTER enabled + pending + INTEN → IPL 6"
    );
}

// ─── Audio channel register storage (#124) ────────────────────────

#[test]
fn aud0_registers_round_trip_through_the_custom_bus() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F0A0, 0x0012); // AUD0LC  hi
    amiga.poke_word(0x00DF_F0A2, 0x3456); // AUD0LC  lo
    amiga.poke_word(0x00DF_F0A4, 0x0008); // AUD0LEN
    amiga.poke_word(0x00DF_F0A6, 500); // AUD0PER
    amiga.poke_word(0x00DF_F0A8, 32); // AUD0VOL
    amiga.poke_word(0x00DF_F0AA, 0xAABB); // AUD0DAT

    assert_eq!(amiga.paula().read_audio(0, AudioField::LcHi), 0x0012);
    assert_eq!(amiga.paula().read_audio(0, AudioField::LcLo), 0x3456);
    assert_eq!(amiga.paula().read_audio(0, AudioField::Len), 0x0008);
    assert_eq!(amiga.paula().read_audio(0, AudioField::Per), 500);
    assert_eq!(amiga.paula().read_audio(0, AudioField::Vol), 32);
    assert_eq!(amiga.paula().read_audio(0, AudioField::Dat), 0xAABB);
}

#[test]
fn aud_channels_1_through_3_decode_at_their_custom_offsets() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F0B8, 16); // AUD1VOL
    amiga.poke_word(0x00DF_F0C8, 24); // AUD2VOL
    amiga.poke_word(0x00DF_F0D8, 48); // AUD3VOL
    assert_eq!(amiga.paula().read_audio(1, AudioField::Vol), 16);
    assert_eq!(amiga.paula().read_audio(2, AudioField::Vol), 24);
    assert_eq!(amiga.paula().read_audio(3, AudioField::Vol), 48);
}

#[test]
fn cpu_bus_read_of_audio_register_returns_paula_state() {
    // The CPU bus servicer (the side-effecting read path) is separate
    // from bus_read_word. Exercise it via `cpu_read_word`.
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F0A6, 0x1234); // AUD0PER
    let val = amiga.cpu_read_word(0x00DF_F0A6);
    assert_eq!(
        val, 0x1234,
        "CPU-side bus read must see Paula audio state, not floating bus"
    );
}

#[test]
fn audio_lc_low_word_masks_off_bit_0_from_bus_writes() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F0A2, 0xFFFF);
    assert_eq!(
        amiga.paula().read_audio(0, AudioField::LcLo),
        0xFFFE,
        "chip enforces word-alignment on low LC at the register layer"
    );
}

#[test]
fn audio_vol_clamps_to_64_at_the_chip_layer() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F0A8, 0x00FF);
    assert_eq!(amiga.paula().read_audio(0, AudioField::Vol), 64);
}

// ─── Audio DMA engine + AUDx IRQs (#125) ──────────────────────────

/// Helper: poke a canonical AUD0 program (one-word block at $1000
/// with full volume and minimum period), enable DMAEN + AUD0EN.
fn program_aud0_one_word_block(amiga: &mut AmigaOcs) {
    amiga.poke_word(0x00DF_F0A0, 0x0000); // AUD0LC hi
    amiga.poke_word(0x00DF_F0A2, 0x1000); // AUD0LC lo
    amiga.poke_word(0x00DF_F0A4, 0x0001); // AUD0LEN
    amiga.poke_word(0x00DF_F0A6, 124); // AUD0PER (minimum)
    amiga.poke_word(0x00DF_F0A8, 64); // AUD0VOL (max)
    amiga.poke_word(0x00DF_F096, 0x8201); // DMAEN + AUD0EN
}

#[test]
fn audio_dma_enable_rising_edge_raises_aud0_irq_within_one_cck() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // Pre-enable INTENA so we can see the IRQ flow but mask the master
    // enable so the CPU doesn't actually service.
    amiga.poke_word(0x00DF_F09A, 0x8080); // SET INT_AUD0

    program_aud0_one_word_block(&mut amiga);

    // Tick one CCK — Paula sees DMAEN rising and raises INT_AUD0.
    // (Tick loop pulses Paula once per CCK at phase 0.)
    amiga.tick(); // phase 0
    amiga.tick(); // phase 1

    assert_ne!(
        amiga.paula().intreq() & (1 << 7),
        0,
        "INT_AUD0 must latch on the DMA-enable rising edge"
    );
}

#[test]
fn audio_dma_fetch_at_slot_advances_channel_pointer() {
    // Put a recognisable pattern in chip RAM at the AUD0 base, run
    // long enough for the channel's DMA slot to fire (hpos 0x0E on
    // the first line), and confirm the fetched word is observable.
    let mut rom = vec![0u8; 512 * 1024];
    // Writing to chip RAM here isn't possible — ROM is passed in.
    // Instead, use the CPU backdoor after construction.
    rom[0] = 0; // keep rom simple
    let mut amiga = AmigaOcs::new(rom);

    // Plant a sample pair at chip RAM $1000: bytes $7F, $80 (max +/-).
    amiga.poke_byte(0x0000_1000, 0x7F);
    amiga.poke_byte(0x0000_1001, 0x80);

    program_aud0_one_word_block(&mut amiga);

    // 227 CCKs per line + a bit extra → guarantees we cross at least
    // one DMA slot and the full DMA return latency.
    for _ in 0..(machine_commodore_amiga_ocs::PAL_LINE_TICKS as u32 * 4) {
        amiga.tick();
    }

    // The archive's `audio_state` surfaces the live output sample.
    let snap = amiga.paula().audio_state(0).expect("ch 0 exists");
    assert_ne!(
        snap.sample, 0,
        "audio sample should have advanced through the DAC; got {:?}",
        snap
    );
}

// ─── Disk register storage (#126) ─────────────────────────────────

#[test]
fn dsklen_write_lands_in_paula_and_drives_the_arming_flip_flop() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // First DSKLEN write with DMAEN arms but doesn't start DMA.
    amiga.poke_word(0x00DF_F024, 0x8200);
    assert!(
        !amiga.paula().disk_dma_pending(),
        "first DMAEN write only arms"
    );
    // Second identical write triggers.
    amiga.poke_word(0x00DF_F024, 0x8200);
    assert!(
        amiga.paula().disk_dma_pending(),
        "second DMAEN write starts DMA"
    );
}

#[test]
fn dsksync_write_routes_to_paula_and_note_read_word_matches() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F07E, 0x4489);
    assert_eq!(amiga.paula().dsksync(), 0x4489);
}

#[test]
fn dskdat_writes_queue_in_program_order_through_the_bus() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F026, 0xAAAA);
    amiga.poke_word(0x00DF_F026, 0xBBBB);
    assert_eq!(amiga.paula().dskdat_queue_len(), 2);
}

#[test]
fn dskbytr_peek_read_via_bus_is_side_effect_free() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.paula_mut().note_disk_read_word(0xABCD);

    // `read_word` uses the peek path — DSKBYT should stay latched.
    assert_ne!(amiga.read_word(0x00DF_F01A) & 0x8000, 0);
    assert_ne!(
        amiga.read_word(0x00DF_F01A) & 0x8000,
        0,
        "repeat peek shows DSKBYT still latched"
    );

    // A direct Paula-level read (what the side-effecting CPU bus
    // servicer invokes) does clear it.
    let _ = amiga.paula_mut().read_dskbytr(0);
    assert_eq!(
        amiga.paula().peek_dskbytr(0) & 0x8000,
        0,
        "side-effecting path cleared DSKBYT"
    );
}

// ─── Disk DMA completion + MFM sync IRQs (#127) ───────────────────

#[test]
fn complete_disk_dma_raises_dskblk_through_the_machine_intreq() {
    // The drive peripheral calls complete_disk_dma() after its DMA
    // transfer is finished. INT_DSKBLK must be visible through the
    // machine's intreq accessor.
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F024, 0x8200); // arm DSKLEN
    amiga.poke_word(0x00DF_F024, 0x8200); // trigger
    assert!(amiga.paula().disk_dma_pending());

    amiga.paula_mut().complete_disk_dma();
    assert!(!amiga.paula().disk_dma_pending());
    assert_ne!(
        amiga.intreq() & IntSource::DskBlk.mask(),
        0,
        "DSKBLK should be set on machine INTREQ after DMA completion"
    );
}

#[test]
fn dskblk_reaches_cpu_ipl_when_enabled() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09A, 0xC002); // SET INTEN + DSKBLK
    amiga.paula_mut().complete_disk_dma();
    amiga.tick();
    assert_eq!(amiga.cpu().ipl, 1, "DSKBLK is IPL 1 per HRM priority table");
}

#[test]
fn sync_match_via_wordsync_raises_dsksyn_through_machine_intreq() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09E, 0x8400); // ADKCON SET + WORDSYNC (bit 10)
    amiga.poke_word(0x00DF_F07E, 0x4489); // DSKSYNC

    // Without WORDSYNC gate → nothing would fire; with it, INT_DSKSYN.
    amiga.paula_mut().note_disk_read_word(0x4489);
    assert_ne!(
        amiga.intreq() & IntSource::DskSyn.mask(),
        0,
        "machine INTREQ should show DSKSYN after a sync-gated match"
    );
}

#[test]
fn dsksyn_reaches_cpu_ipl_when_enabled() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09E, 0x8400); // ADKCON SET + WORDSYNC (bit 10)
    amiga.poke_word(0x00DF_F07E, 0x4489);
    amiga.poke_word(0x00DF_F09A, 0xD000); // SET INTEN + DSKSYN
    amiga.paula_mut().note_disk_read_word(0x4489);
    amiga.tick();
    assert_eq!(amiga.cpu().ipl, 5, "DSKSYN is IPL 5 per HRM priority table");
}

#[test]
fn sync_match_without_wordsync_does_not_raise_dsksyn() {
    // ADKCON.WORDSYNC clear → the sync comparator is inert for IRQ
    // purposes. DSKBYTR.WORDEQUAL still latches (different contract).
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F07E, 0x4489); // DSKSYNC set, WORDSYNC clear
    amiga.paula_mut().note_disk_read_word(0x4489);
    assert_eq!(amiga.intreq() & IntSource::DskSyn.mask(), 0);
    assert_ne!(
        amiga.paula().peek_dskbytr(0) & 0x1000,
        0,
        "WORDEQUAL latches regardless of WORDSYNC"
    );
}

// ─── Serial UART (#128) ───────────────────────────────────────────

#[test]
fn serdat_write_via_bus_raises_int_tbe() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F030, 0x0100 | 0x41); // stop-bit + 'A'
    assert_ne!(
        amiga.intreq() & IntSource::Tbe.mask(),
        0,
        "SERDAT write through the bus must raise INT_TBE"
    );
    assert_eq!(amiga.paula().serdat(), 0x0141);
}

#[test]
fn serper_write_via_bus_stores_baud_divisor() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F032, 0x81FB); // MIDI divisor in LONG mode
    assert_eq!(amiga.paula().serper(), 0x81FB);
}

#[test]
fn serdatr_peek_via_bus_shows_rbf_without_clearing() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.paula_mut().receive_serial(0x77);
    // bus_read_word (peek path) — must not clear RBF.
    let v1 = amiga.read_word(0x00DF_F018);
    let v2 = amiga.read_word(0x00DF_F018);
    assert_ne!(v1 & 0x4000, 0, "RBF visible");
    assert_ne!(v2 & 0x4000, 0, "peek path is side-effect-free");
    assert_eq!(v1 & 0x00FF, 0x77);
}

#[test]
fn receive_serial_raises_rbf_that_reaches_cpu_ipl_5() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09A, 0xC800); // SET INTEN + RBF
    amiga.paula_mut().receive_serial(0xA1);
    amiga.tick();
    assert_eq!(amiga.cpu().ipl, 5, "RBF → IPL 5 per HRM");
}

#[test]
fn int_tbe_gated_behind_intena_tbe_bit() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09A, 0xC001); // SET INTEN + TBE
    amiga.poke_word(0x00DF_F030, 0x0100);
    amiga.tick();
    assert_eq!(amiga.cpu().ipl, 1, "TBE → IPL 1");
}

// ─── POTGO + POTxDAT + POTGOR (#129) ──────────────────────────────

#[test]
fn potgo_write_via_bus_stores_out_and_dat_bits() {
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F034, 0x8000 | 0x4000); // OUT_RY + DATRY
    assert_eq!(amiga.paula().potgo(), 0xC000);
}

#[test]
fn pot0dat_and_pot1dat_read_back_zero_on_reset() {
    let amiga = AmigaOcs::new(zero_rom());
    assert_eq!(amiga.read_word(0x00DF_F012), 0);
    assert_eq!(amiga.read_word(0x00DF_F014), 0);
}

#[test]
fn potgor_reads_back_button_pins_floating_high() {
    // DAT bits 14, 12, 10, 8 all high = idle.
    let amiga = AmigaOcs::new(zero_rom());
    let v = amiga.read_word(0x00DF_F016);
    assert_eq!(
        v & 0x5500,
        0x5500,
        "all four pot button pins idle-high at reset"
    );
}

#[test]
fn set_pot_pin_level_from_peripheral_reflects_through_bus_read() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // Simulate right mouse button (port 0 LX pin) pressed.
    amiga.paula_mut().set_pot_pin_level(0x0100, false);
    let v = amiga.read_word(0x00DF_F016);
    assert_eq!(v & 0x0100, 0, "pressed button shows as 0 in POTGOR");
}

#[test]
fn dskbytr_byte_pacing_advances_on_per_cck_disk_tick() {
    // With ADKCON.FAST set, the chip delivers the next byte 14 CCKs
    // after a word arrives. The machine's tick loop ticks Paula's
    // disk engine once per CCK.
    let mut amiga = AmigaOcs::new(zero_rom());
    amiga.poke_word(0x00DF_F09E, 0x8100); // ADKCON SET + FAST
    amiga.paula_mut().note_disk_read_word(0xAABB);

    // Drain the immediate high-byte DSKBYT via the side-effecting
    // Paula read.
    let _ = amiga.paula_mut().read_dskbytr(0);
    assert_eq!(amiga.paula().peek_dskbytr(0) & 0x8000, 0);

    // 14 CCKs = 28 master/4 ticks. Run that, then DSKBYT should have
    // re-latched with the low byte.
    for _ in 0..(14 * 2) {
        amiga.tick();
    }
    assert_ne!(
        amiga.paula().peek_dskbytr(0) & 0x8000,
        0,
        "FAST disk pacing delivers next byte after 14 CCKs"
    );
}

#[test]
fn audio_dma_disabled_leaves_channel_silent() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // Program regs but leave DMACON.AUD0 clear.
    amiga.poke_word(0x00DF_F0A0, 0x0000);
    amiga.poke_word(0x00DF_F0A2, 0x1000);
    amiga.poke_word(0x00DF_F0A4, 0x0001);
    amiga.poke_word(0x00DF_F0A6, 124);
    amiga.poke_word(0x00DF_F0A8, 64);
    // Master DMA on, but AUD0 off.
    amiga.poke_word(0x00DF_F096, 0x8200);

    for _ in 0..(machine_commodore_amiga_ocs::PAL_LINE_TICKS as u32 * 4) {
        amiga.tick();
    }

    assert_eq!(
        amiga.paula().intreq() & (1 << 7),
        0,
        "with AUD0EN clear, no audio IRQ should fire"
    );
    let snap = amiga.paula().audio_state(0).expect("ch 0 exists");
    assert_eq!(snap.sample, 0, "no DMA → no sample delivered");
}
