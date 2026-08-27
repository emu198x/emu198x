//! Round-trip determinism tests for the Amiga snapshot envelope.
//!
//! Two layers of proof:
//!
//! 1. `snapshot_then_restore_then_snapshot_is_a_fixed_point` — the
//!    snapshot envelope is deterministic across save/restore. Two
//!    successive snapshots taken from a runtime that was just
//!    restored from snapshot bytes must be byte-equal to the original.
//!    Catches any field that fails to round-trip cleanly.
//!
//! 2. `snapshot_then_restore_yields_bit_identical_forward_run` — after
//!    restoring a snapshot, running the machine forward a few frames
//!    produces the same observable state (snapshot bytes) as running
//!    the original forward by the same amount. Catches diagnostic-only
//!    fields that affect behaviour (they shouldn't).
//!
//! Both tests use a blank Kickstart so they're hermetic and run on
//! every `cargo test --workspace`. ROM-backed tests over real
//! Kickstart / Workbench live in the existing diagnostic harnesses
//! and stay there until A.2 promotes them to a boot-invariant suite.
//!
//! Pattern modelled on `runtime-sinclair-zx-spectrum/tests/runtime_48k.rs`.

mod common;

use std::error::Error;

use commodore_agnus_ocs::{BlitterDmaOp, OriginalAgnusRevision};
use common::dummy_a1000_bootstrap_rom;
use common_commodore_amiga::driver::AmigaDriver;
use emu198x_shell::{
    HostIo, MachineCore, MachineError, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink,
};
use motorola_68000::bus::TransferSize;
use motorola_68000::cpu::State;
use motorola_68000::microcode::MicroOp;
use peripheral_commodore_amiga_floppy::DD;
use peripheral_commodore_amiga_floppy::mfm::encode_mfm_track;
use runtime_commodore_amiga::{
    AmigaA1200Runtime, AmigaEcsRuntime, AmigaLiveAccess, AmigaMachine, AmigaOcsRuntime,
    AmigaRuntime, Model,
};

const BLTCON0: u32 = 0x00DF_F040;
const BLTCON1: u32 = 0x00DF_F042;
const BLTCPTH: u32 = 0x00DF_F048;
const BLTCPTL: u32 = 0x00DF_F04A;
const BLTAPTL: u32 = 0x00DF_F052;
const BLTDPTH: u32 = 0x00DF_F054;
const BLTDPTL: u32 = 0x00DF_F056;
const BLTSIZE: u32 = 0x00DF_F058;
const BLTCMOD: u32 = 0x00DF_F060;
const BLTBMOD: u32 = 0x00DF_F062;
const BLTAMOD: u32 = 0x00DF_F064;
const BLTBDAT: u32 = 0x00DF_F072;
const BLTADAT: u32 = 0x00DF_F074;
const BLTSIZV: u32 = 0x00DF_F05C;
const BLTSIZH: u32 = 0x00DF_F05E;
const COP1LCH: u32 = 0x00DF_F080;
const COP1LCL: u32 = 0x00DF_F082;
const COPJMP1: u32 = 0x00DF_F088;
const DMACON: u32 = 0x00DF_F096;
const INTREQ: u32 = 0x00DF_F09C;
const DMACON_SET_DMA_BLITTER_NASTY: u16 = 0x8640;
const INT_BLIT: u16 = 0x0040;

fn blank_kickstart() -> Vec<u8> {
    let mut kickstart = vec![0u8; 256 * 1024];
    // Minimal reset vector — supervisor stack at $00080000, PC at the
    // first ROM word. PC instruction is BRA.S * (loop forever), keeping
    // the CPU in a stable state while the chipset ticks around it.
    kickstart[0] = 0x00;
    kickstart[1] = 0x08;
    kickstart[2] = 0x00;
    kickstart[3] = 0x00;
    kickstart[4] = 0x00;
    kickstart[5] = 0xF8;
    kickstart[6] = 0x00;
    kickstart[7] = 0x08;
    kickstart[8] = 0x60;
    kickstart[9] = 0xFE;
    kickstart
}

fn odd_group1_handler_kickstart() -> Vec<u8> {
    let mut kickstart = vec![0u8; 256 * 1024];
    kickstart[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    kickstart[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    kickstart[8..10].copy_from_slice(&0x4AFCu16.to_be_bytes()); // ILLEGAL
    kickstart[12..16].copy_from_slice(&0x00F8_0030u32.to_be_bytes()); // address error
    kickstart[16..20].copy_from_slice(&0x00F8_0021u32.to_be_bytes()); // ILLEGAL vector
    kickstart[0x30] = 0x60; // BRA.S
    kickstart[0x31] = 0xFE; // -2: stable address-error handler
    kickstart
}

fn interrupt_acknowledge_kickstart() -> Vec<u8> {
    let mut kickstart = vec![0u8; 256 * 1024];
    kickstart[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    kickstart[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    kickstart[8..10].copy_from_slice(&0x46FCu16.to_be_bytes()); // MOVE.W #$2000,SR
    kickstart[10..12].copy_from_slice(&0x2000u16.to_be_bytes());
    kickstart[12..14].copy_from_slice(&0x60FEu16.to_be_bytes()); // BRA.S *
    kickstart
}

fn dynamic_long_write_kickstart() -> Vec<u8> {
    let mut kickstart = vec![0u8; 512 * 1024];
    kickstart[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    kickstart[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    let words: [u16; 8] = [
        0x203C, // MOVE.L #$DEADBEEF,D0
        0xDEAD, 0xBEEF, 0x207C, // MOVEA.L #$00001001,A0
        0x0000, 0x1001, 0x2080, // MOVE.L D0,(A0)
        0x60FE, // BRA.S *
    ];
    for (index, word) in words.into_iter().enumerate() {
        let offset = 8 + index * 2;
        kickstart[offset..offset + 2].copy_from_slice(&word.to_be_bytes());
    }
    kickstart
}

fn null_host() -> HostIo<'static> {
    HostIo {
        input_events: &[],
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    }
}

fn ocs_runtime_with_active_hires_ddf_at_line(
    target_line: u16,
) -> Result<AmigaOcsRuntime, MachineError> {
    let mut runtime = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    {
        let machine = runtime.machine_mut();
        machine.poke_word(0x00DF_F08E, 0x3081); // DIWSTRT
        machine.poke_word(0x00DF_F090, 0xF0C1); // DIWSTOP
        machine.poke_word(0x00DF_F092, 0x0038); // DDFSTRT
        machine.poke_word(0x00DF_F094, 0x00D0); // later ordinary DDFSTOP
        machine.poke_word(0x00DF_F100, 0xC200); // hires, four planes
        for (high, low, pointer) in [
            (0x00DF_F0E0, 0x00DF_F0E2, 0x0001_0000u32),
            (0x00DF_F0E4, 0x00DF_F0E6, 0x0001_2000),
            (0x00DF_F0E8, 0x00DF_F0EA, 0x0001_4000),
            (0x00DF_F0EC, 0x00DF_F0EE, 0x0001_6000),
        ] {
            machine.poke_word(high, (pointer >> 16) as u16);
            machine.poke_word(low, pointer as u16);
        }
        machine.poke_word(0x00DF_F096, 0x8300); // DMAEN | BPLEN
    }
    while runtime.machine().agnus().vpos < target_line {
        runtime.machine_mut().tick();
    }
    while runtime.machine().agnus().hpos < 0x0040 {
        runtime.machine_mut().tick();
    }
    assert_eq!(runtime.machine().agnus().ddf_start_match(), Some(0x0038));
    Ok(runtime)
}

#[test]
fn snapshot_then_restore_then_snapshot_is_a_fixed_point() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;

    // Run a handful of frames so the chipset has non-trivial state
    // (beam counters advanced, CIA timers run, copper has been kicked
    // by the VBL, etc.). The reset-loop CPU stays at $F80008 but
    // everything else ticks.
    let mut host = null_host();
    original.run_until(MachineTime::new(64_000), &mut host)?;

    let snapshot_a = original.snapshot()?;

    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot_a)?;

    let snapshot_b = restored.snapshot()?;

    assert_eq!(
        snapshot_a.len(),
        snapshot_b.len(),
        "snapshot lengths differ — indicates a non-deterministic field"
    );
    assert_eq!(
        snapshot_a, snapshot_b,
        "snapshot bytes differ after round-trip — see lib field list"
    );
    Ok(())
}

#[test]
fn rtc_time_and_subsecond_phase_survive_restore_and_forward_replay() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPalA501, blank_kickstart())?;
    for _ in 0..12_345 {
        original.machine_mut().tick();
    }
    let rtc_at_snapshot = original.machine().rtc_diagnostic_snapshot();
    assert_eq!(
        rtc_at_snapshot.stored_unix_seconds, rtc_at_snapshot.effective_unix_seconds,
        "the emulated clock must not consult wall time between machine ticks",
    );
    assert_eq!(rtc_at_snapshot.subsecond_system_ticks, 12_345);
    assert!(rtc_at_snapshot.system_ticks_per_second > 12_345);

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPalA501, blank_kickstart())?;
    restored.restore(&snapshot)?;

    assert_eq!(
        restored.machine().rtc_diagnostic_snapshot(),
        rtc_at_snapshot,
        "restore must preserve the emulated second and exact subsecond phase",
    );
    assert_eq!(
        restored.snapshot()?,
        snapshot,
        "the RTC-bearing snapshot envelope must be a byte-level fixed point",
    );

    for _ in 0..4_096 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
    }
    let replayed_rtc = restored.machine().rtc_diagnostic_snapshot();
    assert_eq!(
        replayed_rtc,
        original.machine().rtc_diagnostic_snapshot(),
        "restored RTC time and phase must replay exactly",
    );
    assert_eq!(replayed_rtc.subsecond_system_ticks, 16_441);
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "RTC replay must leave the complete runtime snapshot bit-identical",
    );
    Ok(())
}

#[test]
fn partial_disk_dma_fifo_is_a_snapshot_fixed_point() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    original.machine_mut().poke_word(0x00DF_F024, 0x8003);
    original.machine_mut().poke_word(0x00DF_F024, 0x8003);
    original
        .machine_mut()
        .paula_mut()
        .receive_disk_read_word(0x1111);
    original
        .machine_mut()
        .paula_mut()
        .receive_disk_read_word(0x2222);

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;

    let disk = restored.machine().paula().disk_diagnostic_snapshot();
    assert_eq!(disk.disk_dma_fifo, [0x1111, 0x2222]);
    assert_eq!(format!("{:?}", disk.disk_dma_fifo_direction), "Some(Read)");
    assert_eq!(
        restored.snapshot()?,
        snapshot,
        "a partially filled Paula FIFO must restore byte-identically"
    );
    Ok(())
}

#[test]
fn group1_handler_prefetch_context_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, odd_group1_handler_kickstart())?;
    let mut reached_odd_handler = false;
    for _ in 0..20_000 {
        original.machine_mut().tick();
        if original.machine().cpu().regs.pc == 0x00F8_0021 {
            reached_odd_handler = true;
            break;
        }
    }
    assert!(
        reached_odd_handler,
        "ILLEGAL exception did not reach its odd group-1 handler",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, odd_group1_handler_kickstart())?;
    restored.restore(&snapshot)?;
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "group-1 handler-prefetch state must survive postcard",
    );

    for _ in 0..512 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
    }
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "restored exception context must produce the same address-error frame and handler state",
    );
    Ok(())
}

#[test]
fn accepted_interrupt_acknowledge_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let kickstart = interrupt_acknowledge_kickstart();
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, kickstart.clone())?;
    original.machine_mut().poke_word(0x00DF_F09A, 0xC040); // INTEN | BLIT
    original.machine_mut().poke_word(0x00DF_F09C, 0x8040); // request BLIT

    let mut reached_acknowledge = false;
    for _ in 0..20_000 {
        original.machine_mut().tick();
        if let State::BusCycle { op, addr, .. } = &original.machine().cpu().state
            && *op == MicroOp::InterruptAck
        {
            assert_eq!(*addr, 0x00FF_FFF7);
            reached_acknowledge = true;
            break;
        }
    }
    assert!(
        reached_acknowledge,
        "the synthetic level-3 request should reach interrupt acknowledge"
    );
    assert_eq!(original.machine().cpu().target_ipl, 3);
    assert_eq!(original.machine().cpu().regs.interrupt_mask(), 3);

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, kickstart)?;
    restored.restore(&snapshot)?;
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "the accepted level and its active acknowledge cycle must round-trip byte-identically"
    );
    assert!(matches!(
        &restored.machine().cpu().state,
        State::BusCycle {
            op: MicroOp::InterruptAck,
            addr: 0x00FF_FFF7,
            ..
        }
    ));

    for _ in 0..64 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
    }
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "restored acknowledge state must select the same vector and continuation"
    );
    Ok(())
}

#[test]
fn a530_snapshot_is_a_fixed_point_with_clock_bridge_and_local_ram() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPalGvpA530, blank_kickstart())?;
    original.machine_mut().poke_word(0x00E8_004A, 0x0000);
    original.machine_mut().poke_word(0x00E8_0048, 0x2000);
    original.machine_mut().poke_word(0x0020_0042, 0xA55A);
    for _ in 0..17 {
        original.machine_mut().tick();
    }
    assert_ne!(original.machine().cpu_clock().phase(), 0);

    let snapshot_a = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPalGvpA530, blank_kickstart())?;
    restored.restore(&snapshot_a)?;

    assert_eq!(
        restored
            .machine()
            .gvp_a530()
            .expect("A530 survives restore")
            .mapped_base(),
        Some(0x0020_0000)
    );
    assert_eq!(restored.machine().read_word(0x0020_0042), 0xA55A);
    assert_eq!(restored.snapshot()?, snapshot_a);
    Ok(())
}

#[test]
fn snapshot_then_restore_yields_bit_identical_forward_run() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let mut host = null_host();
    original.run_until(MachineTime::new(32_000), &mut host)?;

    let snapshot = original.snapshot()?;

    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;

    // Run both runtimes forward by the same amount of machine time
    // and expect their snapshots to remain byte-equal afterwards.
    let target = original.time().saturating_add(8_000);
    let mut host_a = null_host();
    original.run_until(target, &mut host_a)?;
    let mut host_b = null_host();
    restored.run_until(target, &mut host_b)?;

    let after_original = original.snapshot()?;
    let after_restored = restored.snapshot()?;

    assert_eq!(
        after_original.len(),
        after_restored.len(),
        "post-run snapshot lengths differ — restore drifted"
    );
    assert_eq!(
        after_original, after_restored,
        "post-run snapshot bytes differ — restore is not bit-equivalent"
    );
    Ok(())
}

#[test]
fn ocs_hard_ddfstop_endpoint_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    {
        let machine = original.machine_mut();
        machine.poke_word(0x00DF_F08E, 0x3081); // DIWSTRT
        machine.poke_word(0x00DF_F090, 0xF0C1); // DIWSTOP
        machine.poke_word(0x00DF_F092, 0x0018); // DDFSTRT
        machine.poke_word(0x00DF_F094, 0x00E0); // DDFSTOP beyond hard stop
        machine.poke_word(0x00DF_F100, 0xC200); // hires, four planes
        for (high, low, pointer) in [
            (0x00DF_F0E0, 0x00DF_F0E2, 0x0001_0000u32),
            (0x00DF_F0E4, 0x00DF_F0E6, 0x0001_2000),
            (0x00DF_F0E8, 0x00DF_F0EA, 0x0001_4000),
            (0x00DF_F0EC, 0x00DF_F0EE, 0x0001_6000),
        ] {
            machine.poke_word(high, (pointer >> 16) as u16);
            machine.poke_word(low, pointer as u16);
        }
        machine.poke_word(0x00DF_F096, 0x8300); // DMAEN | BPLEN
    }
    while original.machine().agnus().vpos < 0x0030 {
        original.machine_mut().tick();
    }
    while original.machine().agnus().hpos < 0x00D7 {
        original.machine_mut().tick();
    }
    assert_eq!(original.machine().agnus().ddf_fetch_end(), None);

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert_eq!(restored.machine().agnus().ddf_fetch_end(), None);

    while original.machine().agnus().hpos < 0x00D8 {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().hpos < 0x00D8 {
        restored.machine_mut().tick();
    }
    assert_eq!(original.machine().agnus().ddf_fetch_end(), Some(0x00DF));
    assert_eq!(restored.machine().agnus().ddf_fetch_end(), Some(0x00DF));

    // A second round trip while the terminal unit is pending proves
    // the frozen endpoint itself remains part of postcard state.
    let pending_snapshot = original.snapshot()?;
    let mut pending_restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    pending_restored.restore(&pending_snapshot)?;
    assert_eq!(
        pending_restored.machine().agnus().ddf_fetch_end(),
        Some(0x00DF)
    );

    let line = original.machine().agnus().vpos;
    while original.machine().agnus().vpos == line {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().vpos == line {
        restored.machine_mut().tick();
    }
    while pending_restored.machine().agnus().vpos == line {
        pending_restored.machine_mut().tick();
    }
    assert_eq!(
        original.machine().agnus().bpl_pt,
        restored.machine().agnus().bpl_pt,
        "restored hard-stop state must produce the same terminal fetches"
    );
    assert_eq!(
        original.machine().agnus().bpl_pt,
        pending_restored.machine().agnus().bpl_pt,
        "post-event postcard state must preserve the frozen terminal fetches"
    );
    Ok(())
}

#[test]
fn ocs_phase_shifted_terminal_wrap_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    {
        let machine = original.machine_mut();
        machine.poke_word(0x00DF_F08E, 0x3081); // DIWSTRT
        machine.poke_word(0x00DF_F090, 0xF0C1); // DIWSTOP
        machine.poke_word(0x00DF_F092, 0x001C); // phase-shifted DDFSTRT
        machine.poke_word(0x00DF_F094, 0x00E0); // DDFSTOP beyond hard stop
        machine.poke_word(0x00DF_F100, 0xC200); // hires, four planes
        machine.poke_word(0x00DF_F096, 0x8300); // DMAEN | BPLEN
    }
    while original.machine().agnus().vpos < 0x0030 {
        original.machine_mut().tick();
    }
    while original.machine().agnus().hpos < 0x00D8 {
        original.machine_mut().tick();
    }
    assert_eq!(original.machine().agnus().ddf_fetch_end(), Some(0x00E3));
    while original.machine().agnus().hpos < 0x00E2 {
        original.machine_mut().tick();
    }
    assert_eq!(original.machine().agnus().hpos, 0x00E2);
    assert_eq!(original.machine().agnus().ddf_fetch_end(), Some(0x00E3));
    assert!(original.machine().agnus().ocs_ddf_hard_start_open());

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert_eq!(restored.machine().agnus().hpos, 0x00E2);
    assert_eq!(restored.machine().agnus().ddf_fetch_end(), Some(0x00E3));
    assert!(restored.machine().agnus().ocs_ddf_hard_start_open());
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "the pre-wrap logical endpoint must be byte-stable through postcard",
    );

    original.machine_mut().poke_word(0x00DF_F092, 0x0010);
    restored.machine_mut().poke_word(0x00DF_F092, 0x0010);
    let terminal_line = original.machine().agnus().vpos;
    while original.machine().agnus().vpos == terminal_line
        || original.machine().agnus().hpos < 0x0010
    {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().vpos == terminal_line
        || restored.machine().agnus().hpos < 0x0010
    {
        restored.machine_mut().tick();
    }

    assert_eq!(original.machine().agnus().ddf_fetch_end(), None);
    assert_eq!(restored.machine().agnus().ddf_fetch_end(), None);
    assert_eq!(original.machine().agnus().ddf_start_match(), None);
    assert_eq!(restored.machine().agnus().ddf_start_match(), None);
    assert!(
        !original.machine().agnus().ocs_ddf_hard_start_open()
            && !restored.machine().agnus().ocs_ddf_hard_start_open(),
        "the restored logical tail must inhibit the next-line $10 start",
    );
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "restored pre-wrap state must produce the same start-admission result",
    );
    Ok(())
}

#[test]
fn ocs_aborted_ddf_run_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = ocs_runtime_with_active_hires_ddf_at_line(0x0030)?;
    original.machine_mut().poke_word(0x00DF_F096, 0x0100); // clear BPLEN
    while original.machine().agnus().hpos < 0x0048 {
        original.machine_mut().tick();
    }
    original.machine_mut().poke_word(0x00DF_F096, 0x8100); // set BPLEN
    while original.machine().agnus().hpos < 0x0050 {
        original.machine_mut().tick();
    }
    assert!(original.machine().agnus().dma_enabled(0x0100));
    assert!(original.machine().agnus().ocs_ddf_run_aborted());
    assert_eq!(original.machine().agnus().ddf_start_match(), Some(0x0038));
    assert_eq!(original.machine().agnus().ddf_fetch_end(), None);
    let pointers_after_reenable = original.machine().agnus().bpl_pt;

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert!(restored.machine().agnus().dma_enabled(0x0100));
    assert!(restored.machine().agnus().ocs_ddf_run_aborted());
    assert_eq!(restored.machine().agnus().ddf_start_match(), Some(0x0038));
    assert_eq!(restored.machine().agnus().ddf_fetch_end(), None);
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "the post-re-enable abort history must be byte-stable through postcard",
    );

    while original.machine().agnus().hpos < 0x00D8 {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().hpos < 0x00D8 {
        restored.machine_mut().tick();
    }
    for runtime in [&original, &restored] {
        assert!(runtime.machine().agnus().ocs_ddf_run_aborted());
        assert_eq!(runtime.machine().agnus().ddf_stop_match(), None);
        assert_eq!(runtime.machine().agnus().ddf_fetch_end(), None);
        assert!(runtime.machine().agnus().ocs_ddf_hard_start_open());
        assert_eq!(
            runtime.machine().agnus().bpl_pt,
            pointers_after_reenable,
            "the restored stale origin must not advance bitplane pointers",
        );
    }
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "restored abort history must produce the same no-resume result",
    );

    let aborted_line = original.machine().agnus().vpos;
    while original.machine().agnus().vpos == aborted_line {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().vpos == aborted_line {
        restored.machine_mut().tick();
    }
    assert!(!original.machine().agnus().ocs_ddf_run_aborted());
    assert!(!restored.machine().agnus().ocs_ddf_run_aborted());
    while original.machine().agnus().hpos < 0x0038 {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().hpos < 0x0038 {
        restored.machine_mut().tick();
    }
    assert_eq!(original.machine().agnus().ddf_start_match(), Some(0x0038));
    assert_eq!(restored.machine().agnus().ddf_start_match(), Some(0x0038));
    assert_eq!(original.snapshot()?, restored.snapshot()?);
    Ok(())
}

#[test]
fn ocs_rewritten_future_ddf_start_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = ocs_runtime_with_active_hires_ddf_at_line(0x0030)?;
    original.machine_mut().poke_word(0x00DF_F096, 0x0100); // clear BPLEN
    while original.machine().agnus().hpos < 0x0048 {
        original.machine_mut().tick();
    }
    original.machine_mut().poke_word(0x00DF_F096, 0x8100); // set BPLEN
    while original.machine().agnus().hpos < 0x0050 {
        original.machine_mut().tick();
    }
    original.machine_mut().poke_word(0x00DF_F092, 0x0060); // future DDFSTRT
    while original.machine().agnus().hpos < 0x0054 {
        original.machine_mut().tick();
    }
    assert!(original.machine().agnus().ocs_ddf_run_aborted());
    assert_eq!(original.machine().agnus().ddf_start_match(), Some(0x0038));
    let pointers_before_fresh_start = original.machine().agnus().bpl_pt;

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert!(restored.machine().agnus().ocs_ddf_run_aborted());
    assert_eq!(restored.machine().agnus().ddf_start_match(), Some(0x0038));
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "the pending future comparator must be byte-stable through postcard",
    );

    while original.machine().agnus().hpos < 0x005F {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().hpos < 0x005F {
        restored.machine_mut().tick();
    }
    for runtime in [&original, &restored] {
        assert!(runtime.machine().agnus().ocs_ddf_run_aborted());
        assert_eq!(runtime.machine().agnus().ddf_start_match(), Some(0x0038));
        assert_eq!(
            runtime.machine().agnus().bpl_pt,
            pointers_before_fresh_start,
            "the restored old origin must stay inactive before the new comparator",
        );
    }

    while original.machine().agnus().hpos < 0x0060 {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().hpos < 0x0060 {
        restored.machine_mut().tick();
    }
    for runtime in [&original, &restored] {
        assert!(!runtime.machine().agnus().ocs_ddf_run_aborted());
        assert_eq!(runtime.machine().agnus().ddf_start_match(), Some(0x0060));
    }

    while original.machine().agnus().hpos < 0x0068 {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().hpos < 0x0068 {
        restored.machine_mut().tick();
    }
    for runtime in [&original, &restored] {
        assert_ne!(
            runtime.machine().agnus().bpl_pt,
            pointers_before_fresh_start,
            "the restored future comparator must establish new fetches",
        );
    }
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "restored future-start state must advance deterministically",
    );

    while original.machine().agnus().hpos < 0x00D0 {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().hpos < 0x00D0 {
        restored.machine_mut().tick();
    }
    for runtime in [&original, &restored] {
        assert_eq!(runtime.machine().agnus().ddf_stop_match(), Some(0x00D0));
        assert_eq!(runtime.machine().agnus().ddf_fetch_end(), Some(0x00D7));
    }
    assert_eq!(original.snapshot()?, restored.snapshot()?);
    Ok(())
}

#[test]
fn ocs_vertical_diw_history_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = ocs_runtime_with_active_hires_ddf_at_line(0x00B0)?;
    original.machine_mut().poke_word(0x00DF_F090, 0xB0C1); // current-line VSTOP
    while original.machine().agnus().hpos < 0x0048 {
        original.machine_mut().tick();
    }
    assert!(!original.machine().agnus().vertical_diw_active());
    assert!(original.machine().agnus().ocs_ddf_run_aborted());
    assert_eq!(original.machine().agnus().ddf_start_match(), Some(0x0038));

    original.machine_mut().poke_word(0x00DF_F090, 0xF0C1);
    while original.machine().agnus().hpos < 0x0050 {
        original.machine_mut().tick();
    }
    assert!(
        !original.machine().agnus().vertical_diw_active(),
        "restored register geometry cannot reconstruct the closed latch",
    );
    let pointers_after_close = original.machine().agnus().bpl_pt;

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert!(!restored.machine().agnus().vertical_diw_active());
    assert!(restored.machine().agnus().ocs_ddf_run_aborted());
    assert_eq!(restored.machine().agnus().ddf_start_match(), Some(0x0038));
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "closed vertical-DIW history must be byte-stable through postcard",
    );

    for runtime in [&mut original, &mut restored] {
        runtime.machine_mut().poke_word(0x00DF_F08E, 0xB081); // current-line VSTART
        while runtime.machine().agnus().hpos < 0x0058 {
            runtime.machine_mut().tick();
        }
        assert!(runtime.machine().agnus().vertical_diw_active());
        assert!(runtime.machine().agnus().ocs_ddf_run_aborted());
        assert_eq!(runtime.machine().agnus().ddf_start_match(), Some(0x0038));
        assert_eq!(
            runtime.machine().agnus().bpl_pt,
            pointers_after_close,
            "vertical reopening alone must not resume the stale DDF origin",
        );
        runtime.machine_mut().poke_word(0x00DF_F092, 0x0080); // future DDFSTRT
    }
    assert_eq!(original.snapshot()?, restored.snapshot()?);

    for runtime in [&mut original, &mut restored] {
        while runtime.machine().agnus().hpos < 0x0080 {
            runtime.machine_mut().tick();
        }
        assert_eq!(runtime.machine().agnus().ddf_start_match(), Some(0x0080));
        assert!(!runtime.machine().agnus().ocs_ddf_run_aborted());
        while runtime.machine().agnus().hpos < 0x0088 {
            runtime.machine_mut().tick();
        }
        assert_ne!(
            runtime.machine().agnus().bpl_pt,
            pointers_after_close,
            "the later comparator must establish fresh fetches after restore",
        );
    }
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "restored vertical history must evolve deterministically",
    );
    Ok(())
}

#[test]
fn a1000_hard_vertical_blank_identity_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())?;
    original.machine_mut().poke_word(0x00DF_F08E, 0xF081); // late VSTART
    original.machine_mut().poke_word(0x00DF_F090, 0xE0C1); // earlier VSTOP

    while original.machine().agnus().vpos < 0x00F0 {
        original.machine_mut().tick();
    }
    assert!(original.machine().agnus().vertical_diw_active());
    original.machine_mut().poke_word(0x00DF_F08E, 0x0081); // line-zero VSTART
    assert_eq!(original.machine().agnus().diwstrt, 0x0081);
    assert!(original.machine().agnus().vertical_diw_active());

    let final_line = original.machine().agnus().lines_per_frame - 1;
    while original.machine().agnus().vpos < final_line {
        original.machine_mut().tick();
    }
    assert_eq!(
        original.machine().agnus().original_revision(),
        OriginalAgnusRevision::A1000,
    );
    assert!(
        original.machine().agnus().vertical_diw_active(),
        "A1000 must remain open on its final physical field line",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())?;
    restored.restore(&snapshot)?;
    assert_eq!(
        restored.machine().agnus().original_revision(),
        OriginalAgnusRevision::A1000,
    );
    assert!(restored.machine().agnus().vertical_diw_active());
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "revision and held hard-blank state must be byte-stable through postcard",
    );

    for runtime in [&mut original, &mut restored] {
        while runtime.machine().agnus().vpos == final_line {
            runtime.machine_mut().tick();
        }
        assert_eq!(runtime.machine().agnus().vpos, 0);
        assert!(
            !runtime.machine().agnus().vertical_diw_active(),
            "restored A1000 line-zero force-off must beat VSTART",
        );
    }
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "restored A1000 hard-blank state must evolve deterministically",
    );

    // Snapshot the asserted, line-held force-off state itself. Revision
    // identity alone is insufficient here: DIW writes consume the held
    // event rather than recomputing it from vpos.
    let line_zero_snapshot = original.snapshot()?;
    let mut line_zero_restored =
        AmigaOcsRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())?;
    line_zero_restored.restore(&line_zero_snapshot)?;
    assert_eq!(line_zero_restored.machine().agnus().vpos, 0);
    assert!(!line_zero_restored.machine().agnus().vertical_diw_active());
    assert_eq!(line_zero_snapshot, line_zero_restored.snapshot()?);

    line_zero_restored
        .machine_mut()
        .poke_word(0x00DF_F08E, 0x0081);
    assert!(
        !line_zero_restored.machine().agnus().vertical_diw_active(),
        "restored line-held A1000 force-off must reject a matching DIWSTRT write",
    );
    Ok(())
}

#[test]
fn a1000_blitter_startup_phase_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())?;
    {
        let machine = original.machine_mut();
        machine.poke_word(BLTCON0, 0x01FF); // USED | D := 1
        machine.poke_word(DMACON, DMACON_SET_DMA_BLITTER_NASTY);
        machine.poke_word(BLTSIZE, (1 << 6) | 1);
        let mut guard = 0;
        while machine.agnus().blitter_busy {
            machine.tick();
            guard += 1;
            assert!(guard < 1_000, "setup blit never completed");
        }

        // Establish a known preceding non-zero BZERO result before starting
        // the operation whose startup state is snapshotted. A register write
        // no longer completes an in-flight blit as a hidden side effect.
        machine.poke_word(BLTCON0, 0);
        machine.poke_word(INTREQ, INT_BLIT); // clear first-blit completion
        assert!(!machine.agnus().blitter_dzero);
        machine.poke_word(BLTSIZE, (1 << 6) | 1);
    }

    assert!(original.machine().agnus().blitter_busy);
    assert!(!original.machine().agnus().blitter_busy_visible());
    assert!(
        !original.machine().agnus().blitter_dzero,
        "BLTSIZE must preserve the preceding non-zero BZERO result",
    );
    assert_eq!(
        original.machine().agnus().blitter_startup_ccks_remaining(),
        2,
    );
    assert_eq!(original.machine().intreq() & INT_BLIT, 0);

    let before_first_cck = original.snapshot()?;
    let mut restored_before =
        AmigaOcsRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())?;
    restored_before.restore(&before_first_cck)?;
    assert_eq!(before_first_cck, restored_before.snapshot()?);
    assert!(restored_before.machine().agnus().blitter_busy);
    assert!(!restored_before.machine().agnus().blitter_busy_visible());
    assert!(!restored_before.machine().agnus().blitter_dzero);
    assert_eq!(
        restored_before
            .machine()
            .agnus()
            .blitter_startup_ccks_remaining(),
        2,
    );

    for runtime in [&mut original, &mut restored_before] {
        runtime
            .machine_mut()
            .poke_word(DMACON, DMACON_SET_DMA_BLITTER_NASTY);
    }
    let mut guard = 0;
    while original.machine().agnus().blitter_startup_ccks_remaining() == 2 {
        original.machine_mut().tick();
        restored_before.machine_mut().tick();
        guard += 1;
        assert!(guard < 1_000, "A1000 never accepted its first startup CCK");
        assert_eq!(
            original.machine().agnus().blitter_startup_ccks_remaining(),
            restored_before
                .machine()
                .agnus()
                .blitter_startup_ccks_remaining(),
        );
    }

    assert_eq!(
        original.machine().agnus().blitter_startup_ccks_remaining(),
        1,
    );
    assert!(original.machine().agnus().blitter_busy_visible());
    assert!(
        original.machine().agnus().blitter_dzero,
        "first accepted startup CCK must reload BZERO",
    );
    assert_eq!(original.machine().agnus().blitter_ccks_remaining, 1);
    assert_eq!(original.machine().intreq() & INT_BLIT, 0);
    assert_eq!(original.snapshot()?, restored_before.snapshot()?);

    let after_first_cck = original.snapshot()?;
    let mut restored_after = AmigaOcsRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())?;
    restored_after.restore(&after_first_cck)?;
    assert_eq!(after_first_cck, restored_after.snapshot()?);
    assert!(restored_after.machine().agnus().blitter_busy_visible());
    assert!(restored_after.machine().agnus().blitter_dzero);
    assert_eq!(
        restored_after
            .machine()
            .agnus()
            .blitter_startup_ccks_remaining(),
        1,
    );
    assert_eq!(restored_after.machine().agnus().blitter_ccks_remaining, 1);
    assert_eq!(restored_after.machine().intreq() & INT_BLIT, 0);

    while original.machine().agnus().blitter_busy {
        original.machine_mut().tick();
        restored_after.machine_mut().tick();
        guard += 1;
        assert!(guard < 2_000, "A1000 blit never completed");
        assert_eq!(
            original.machine().agnus().blitter_busy,
            restored_after.machine().agnus().blitter_busy,
        );
    }
    assert!(
        original.machine().agnus().blitter_busy_visible(),
        "DMACONR must retain the completion source CCK",
    );
    assert!(
        original.machine().agnus().blitter_busy_copper(),
        "Copper BFD must retain its longer completion observation",
    );
    while original.machine().agnus().blitter_busy_visible() {
        original.machine_mut().tick();
        restored_after.machine_mut().tick();
        guard += 1;
        assert!(guard < 2_100, "A1000 DMACONR busy hold never drained");
        assert_eq!(
            original.machine().agnus().blitter_busy_visible(),
            restored_after.machine().agnus().blitter_busy_visible(),
        );
    }
    assert!(!original.machine().agnus().blitter_busy_visible());
    assert!(
        original.machine().agnus().blitter_busy_copper(),
        "Copper BFD remains busy for one CCK after DMACONR releases",
    );
    while original.machine().agnus().blitter_busy_copper() {
        original.machine_mut().tick();
        restored_after.machine_mut().tick();
        guard += 1;
        assert!(guard < 2_200, "A1000 Copper busy hold never drained");
        assert_eq!(
            original.machine().agnus().blitter_busy_copper(),
            restored_after.machine().agnus().blitter_busy_copper(),
        );
    }
    assert_ne!(original.machine().intreq() & INT_BLIT, 0);
    assert_eq!(
        original.snapshot()?,
        restored_after.snapshot()?,
        "restored mid-startup state must complete on the same CCK",
    );
    Ok(())
}

#[test]
fn pending_copper_skip_kind_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())?;
    {
        let machine = original.machine_mut();
        machine.poke_word(BLTCON0, 0);
        machine.poke_word(BLTSIZE, (1 << 6) | 1);
        machine.poke_word(0x0000_1000, 0x0001); // matching SKIP
        machine.poke_word(0x0000_1002, 0x7FFF); // BFD=0
        machine.poke_word(0x0000_1004, 0x0180); // MOVE COLOR00
        machine.poke_word(0x0000_1006, 0x0F00);
        machine.poke_word(0x0000_1008, 0xFFFF);
        machine.poke_word(0x0000_100A, 0xFFFE);
        machine.poke_word(COP1LCH, 0);
        machine.poke_word(COP1LCL, 0x1000);
        machine.poke_word(COPJMP1, 0);
        machine.poke_word(DMACON, 0x8280); // SETCLR | DMAEN | COPEN
    }

    let mut guard = 0;
    while !original.machine().copper().pending_wait_delay {
        original.machine_mut().tick();
        guard += 1;
        assert!(guard < 1_000, "Copper never decoded the SKIP");
    }
    assert!(original.machine().copper().pending_wait_is_skip);
    assert_eq!(original.machine().copper().pc, 0x1004);
    assert!(!original.machine().agnus().blitter_busy_visible());

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())?;
    restored.restore(&snapshot)?;
    assert_eq!(snapshot, restored.snapshot()?);
    assert!(restored.machine().copper().pending_wait_delay);
    assert!(
        restored.machine().copper().pending_wait_is_skip,
        "the pending comparison must restore as SKIP rather than WAIT",
    );
    assert_eq!(restored.machine().copper().pc, 0x1004);

    for runtime in [&mut original, &mut restored] {
        runtime.machine_mut().poke_word(DMACON, 0x0080); // clear COPEN
        runtime
            .machine_mut()
            .poke_word(DMACON, DMACON_SET_DMA_BLITTER_NASTY);
    }
    while original.machine().agnus().blitter_startup_ccks_remaining() == 2 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
        guard += 1;
        assert!(guard < 2_000, "A1000 never accepted its first startup CCK");
    }
    assert!(original.machine().agnus().blitter_busy_visible());
    assert!(restored.machine().agnus().blitter_busy_visible());
    assert!(original.machine().copper().pending_wait_is_skip);
    assert!(restored.machine().copper().pending_wait_is_skip);

    for runtime in [&mut original, &mut restored] {
        runtime.machine_mut().poke_word(DMACON, 0x8080); // SETCLR | COPEN
    }
    for _ in 0..64 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
    }
    assert_eq!(original.machine().color(0) & 0x0FFF, 0x0F00);
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "restored pending SKIP must sample the same post-restore BBUSY transition",
    );
    Ok(())
}

#[test]
fn ecs_blitter_startup_phase_survives_nested_snapshot() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?;
    original.machine_mut().poke_word(BLTCON0, 0);
    original.machine_mut().poke_word(BLTSIZE, (1 << 6) | 1);
    original
        .machine_mut()
        .poke_word(DMACON, DMACON_SET_DMA_BLITTER_NASTY);

    assert!(
        original.machine().agnus().blitter_busy_visible(),
        "enhanced Agnus must expose BBUSY immediately",
    );
    let mut guard = 0;
    while original.machine().agnus().blitter_startup_ccks_remaining() == 2 {
        original.machine_mut().tick();
        guard += 1;
        assert!(
            guard < 1_000,
            "enhanced Agnus never accepted its first startup CCK",
        );
    }
    assert_eq!(
        original.machine().agnus().blitter_startup_ccks_remaining(),
        1,
    );
    assert_eq!(original.machine().agnus().blitter_ccks_remaining, 1);

    let snapshot = original.snapshot()?;
    let mut restored = AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert_eq!(snapshot, restored.snapshot()?);
    assert_eq!(
        restored.machine().agnus().blitter_startup_ccks_remaining(),
        1,
    );
    assert!(restored.machine().agnus().blitter_busy_visible());

    while original.machine().agnus().blitter_busy {
        original.machine_mut().tick();
        restored.machine_mut().tick();
        guard += 1;
        assert!(guard < 2_000, "enhanced Agnus blit never completed");
    }
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "nested enhanced-Agnus startup state must continue deterministically",
    );
    Ok(())
}

#[test]
fn pre_aga_blitter_completion_pipeline_survives_postcard_round_trip() -> Result<(), Box<dyn Error>>
{
    const DESTINATION: u32 = 0x0000_2000;

    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    {
        let machine = original.machine_mut();
        machine.poke_word(DMACON, DMACON_SET_DMA_BLITTER_NASTY);
        machine.poke_word(BLTCON0, 0x01FF); // USED | D := all ones
        machine.poke_word(BLTDPTH, (DESTINATION >> 16) as u16);
        machine.poke_word(BLTDPTL, DESTINATION as u16);
        machine.poke_word(BLTSIZE, (1 << 6) | 1);
    }

    let mut guard = 0;
    while original.machine().agnus().blitter_completion_phase() != "final-result" {
        original.machine_mut().tick();
        guard += 1;
        assert!(guard < 1_000, "pre-AGA blitter never reached main finish");
    }
    assert!(original.machine().agnus().blitter_busy);
    assert!(original.machine().agnus().blitter_busy_visible());
    assert!(original.machine().agnus().blitter_busy_copper());
    assert_eq!(
        original
            .machine()
            .agnus()
            .blitter_completion_ccks_remaining(),
        2,
    );
    assert!(original.machine().agnus().blitter_final_d_pending());
    assert!(original.machine().agnus().blitter_dzero);
    assert_ne!(original.machine().intreq() & INT_BLIT, 0);
    assert_eq!(original.machine().read_chip_ram_byte(DESTINATION), 0);

    let at_finish = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&at_finish)?;
    assert_eq!(at_finish, restored.snapshot()?);

    while original.machine().agnus().blitter_completion_phase() != "final-write" {
        original.machine_mut().tick();
        restored.machine_mut().tick();
        guard += 1;
        assert!(guard < 2_000, "pre-AGA final result never settled");
    }
    assert_eq!(original.snapshot()?, restored.snapshot()?);
    assert!(original.machine().agnus().blitter_busy);
    assert!(!original.machine().agnus().blitter_busy_visible());
    assert!(original.machine().agnus().blitter_busy_copper());
    assert!(!original.machine().agnus().blitter_dzero);
    assert_eq!(
        original
            .machine()
            .agnus()
            .blitter_completion_ccks_remaining(),
        1,
    );
    assert_eq!(original.machine().read_chip_ram_byte(DESTINATION), 0);

    let before_final_d = original.snapshot()?;
    let mut restored_before_final_d = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored_before_final_d.restore(&before_final_d)?;
    assert_eq!(before_final_d, restored_before_final_d.snapshot()?);

    while original.machine().agnus().blitter_busy {
        original.machine_mut().tick();
        restored.machine_mut().tick();
        restored_before_final_d.machine_mut().tick();
        guard += 1;
        assert!(guard < 3_000, "pre-AGA final D never drained");
    }
    assert_eq!(original.machine().read_chip_ram_byte(DESTINATION), 0xFF);
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "finish-stage restore must preserve final-D continuation",
    );
    assert_eq!(
        original.snapshot()?,
        restored_before_final_d.snapshot()?,
        "final-write restore must preserve final-D continuation",
    );
    Ok(())
}

#[test]
fn line_onedot_suppression_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    const DESTINATION: u32 = 0x0000_2000;

    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    {
        let machine = original.machine_mut();
        machine.poke_word(DMACON, DMACON_SET_DMA_BLITTER_NASTY);
        machine.poke_word(BLTCON0, 0x0BCA); // USEA+C+D, standard line minterm
        machine.poke_word(BLTCON1, 0x001B); // X-major +X/+Y, ONEDOT, LINE
        machine.poke_word(BLTAPTL, 0xFFFE); // negative, unchanged line error
        machine.poke_word(BLTAMOD, 0);
        machine.poke_word(BLTBMOD, 0);
        machine.poke_word(BLTCMOD, 0xFFFE);
        machine.poke_word(BLTADAT, 0x8000);
        machine.poke_word(BLTBDAT, 0xFFFF);
        machine.poke_word(BLTCPTH, 0);
        machine.poke_word(BLTCPTL, DESTINATION as u16);
        machine.poke_word(BLTDPTH, 0);
        machine.poke_word(BLTDPTL, DESTINATION as u16);
        machine.poke_word(BLTSIZE, (2 << 6) | 2);
    }

    let mut guard = 0;
    while original.machine().agnus().blitter_ccks_remaining != 1 {
        original.machine_mut().tick();
        guard += 1;
        assert!(
            guard < 1_000,
            "line blitter never reached the second logical D operation",
        );
    }
    assert_eq!(
        original.machine().agnus().next_blitter_dma_request(),
        Some(BlitterDmaOp::WriteD),
    );
    assert_eq!(
        original.machine().read_chip_ram_byte(DESTINATION),
        0x80,
        "the first dot must be present before the suppressed second write",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert_eq!(snapshot, restored.snapshot()?);

    while original.machine().agnus().blitter_busy {
        original.machine_mut().tick();
        restored.machine_mut().tick();
        guard += 1;
        assert!(guard < 2_000, "restored ONEDOT line never completed");
    }
    assert_eq!(
        original.machine().read_chip_ram_byte(DESTINATION),
        0x80,
        "the same-row second D transfer must remain suppressed",
    );
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "the serialized ONEDOT and texture phases must continue deterministically",
    );
    Ok(())
}

#[test]
fn alice_blitter_completion_pipeline_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    const DESTINATION: u32 = 0x0000_2000;

    let mut original = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    {
        let machine = original.machine_mut();
        machine.poke_word(DMACON, DMACON_SET_DMA_BLITTER_NASTY);
        machine.poke_word(BLTCON0, 0x01FF); // USED | D := all ones
        machine.poke_word(BLTDPTH, (DESTINATION >> 16) as u16);
        machine.poke_word(BLTDPTL, DESTINATION as u16);
        machine.poke_word(BLTSIZV, 1);
        machine.poke_word(BLTSIZH, 1);
    }

    let mut guard = 0;
    while original.machine().agnus().blitter_completion_phase() != "final-write" {
        original.machine_mut().tick();
        guard += 1;
        assert!(guard < 1_000, "Alice final result never settled");
    }
    assert!(original.machine().agnus().blitter_busy);
    assert!(original.machine().agnus().blitter_busy_visible());
    assert!(original.machine().agnus().blitter_busy_copper());
    assert!(!original.machine().agnus().blitter_finish_emitted());
    assert!(!original.machine().agnus().blitter_dzero);
    assert_eq!(original.machine().intreq() & INT_BLIT, 0);
    assert_eq!(original.machine().read_chip_ram_byte(DESTINATION), 0);

    let snapshot = original.snapshot()?;
    let mut restored = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    restored.restore(&snapshot)?;
    assert_eq!(snapshot, restored.snapshot()?);

    while original.machine().agnus().blitter_busy {
        original.machine_mut().tick();
        restored.machine_mut().tick();
        guard += 1;
        assert!(guard < 2_000, "Alice final D never drained");
    }
    assert_eq!(original.machine().read_chip_ram_byte(DESTINATION), 0xFF);
    assert_ne!(original.machine().intreq() & INT_BLIT, 0);
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "Alice completion tail must continue deterministically",
    );
    Ok(())
}

#[test]
fn a1200_dynamic_bus_phase_survives_runtime_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    const DESTINATION: u32 = 0x0000_1001;

    let kickstart = dynamic_long_write_kickstart();
    let mut original = AmigaA1200Runtime::new(Model::A1200AgaPal, kickstart.clone())?;
    {
        let machine = original.machine_mut();
        machine.poke_byte(DESTINATION - 1, 0xA5);
        for offset in 0..4 {
            machine.poke_byte(DESTINATION + offset, 0xCC);
        }
        machine.poke_byte(DESTINATION + 4, 0x5A);
    }

    let mut reached_split = false;
    for _ in 0..10_000 {
        original.machine_mut().tick();
        let machine = original.machine();
        let split_state = machine
            .cpu()
            .active_bus_transfer
            .is_some_and(|transfer| transfer.remaining == TransferSize::Byte)
            && matches!(
                &machine.cpu().state,
                State::BusCycle {
                    addr,
                    op: MicroOp::WriteLong,
                    ..
                } if *addr == DESTINATION + 3
            );
        if split_state {
            reached_split = true;
            break;
        }
    }
    assert!(
        reached_split,
        "the odd long write never reached its final split phase"
    );
    assert_eq!(
        [
            original.machine().read_chip_ram_byte(DESTINATION - 1),
            original.machine().read_chip_ram_byte(DESTINATION),
            original.machine().read_chip_ram_byte(DESTINATION + 1),
            original.machine().read_chip_ram_byte(DESTINATION + 2),
            original.machine().read_chip_ram_byte(DESTINATION + 3),
            original.machine().read_chip_ram_byte(DESTINATION + 4),
        ],
        [0xA5, 0xDE, 0xAD, 0xBE, 0xCC, 0x5A]
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaA1200Runtime::new(Model::A1200AgaPal, kickstart)?;
    restored.restore(&snapshot)?;
    assert_eq!(snapshot, restored.snapshot()?);

    let mut completed = false;
    for _ in 0..100 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
        if original.machine().cpu().active_bus_transfer.is_none()
            && original.machine().read_chip_ram_byte(DESTINATION + 3) == 0xEF
        {
            completed = true;
            break;
        }
    }
    assert!(completed, "the restored final byte phase did not complete");
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "the in-flight dynamic transfer must continue deterministically"
    );
    assert_eq!(
        [
            original.machine().read_chip_ram_byte(DESTINATION - 1),
            original.machine().read_chip_ram_byte(DESTINATION),
            original.machine().read_chip_ram_byte(DESTINATION + 1),
            original.machine().read_chip_ram_byte(DESTINATION + 2),
            original.machine().read_chip_ram_byte(DESTINATION + 3),
            original.machine().read_chip_ram_byte(DESTINATION + 4),
        ],
        [0xA5, 0xDE, 0xAD, 0xBE, 0xEF, 0x5A]
    );
    Ok(())
}

#[test]
fn ocs_closed_ddf_hard_start_gate_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    {
        let machine = original.machine_mut();
        machine.poke_word(0x00DF_F08E, 0x3081); // DIWSTRT
        machine.poke_word(0x00DF_F090, 0xF0C1); // DIWSTOP
        machine.poke_word(0x00DF_F092, 0x0018); // DDFSTRT
        machine.poke_word(0x00DF_F094, 0x00D0); // in-line terminal unit
        machine.poke_word(0x00DF_F100, 0xC200); // hires, four planes
        machine.poke_word(0x00DF_F096, 0x8300); // DMAEN | BPLEN
    }
    while original.machine().agnus().vpos < 0x0030 {
        original.machine_mut().tick();
    }
    while original.machine().agnus().hpos < 0x00D7 {
        original.machine_mut().tick();
    }
    assert_eq!(original.machine().agnus().ddf_fetch_end(), Some(0x00D7));
    assert!(
        !original.machine().agnus().ocs_ddf_hard_start_open(),
        "the completed terminal unit must close the OCS hard-start gate",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert!(
        !restored.machine().agnus().ocs_ddf_hard_start_open(),
        "the non-default closed gate must survive postcard restore",
    );
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "the closed gate must be byte-stable through postcard",
    );

    original.machine_mut().poke_word(0x00DF_F092, 0x0010);
    restored.machine_mut().poke_word(0x00DF_F092, 0x0010);
    let completed_line = original.machine().agnus().vpos;
    while original.machine().agnus().vpos == completed_line
        || original.machine().agnus().hpos < 0x0010
    {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().vpos == completed_line
        || restored.machine().agnus().hpos < 0x0010
    {
        restored.machine_mut().tick();
    }

    assert_eq!(original.machine().agnus().ddf_start_match(), None);
    assert_eq!(restored.machine().agnus().ddf_start_match(), None);
    assert!(
        !original.machine().agnus().ocs_ddf_hard_start_open()
            && !restored.machine().agnus().ocs_ddf_hard_start_open(),
        "the restored closed gate must reject the next line's pre-$18 comparator",
    );
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "restored hard-start state must remain deterministic after the missed comparator",
    );
    Ok(())
}

#[test]
fn ocs_open_ddf_hard_start_gate_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    {
        let machine = original.machine_mut();
        machine.poke_word(0x00DF_F08E, 0x3081); // DIWSTRT
        machine.poke_word(0x00DF_F090, 0xF0C1); // DIWSTOP
        machine.poke_word(0x00DF_F092, 0x0018); // DDFSTRT
        machine.poke_word(0x00DF_F094, 0x00D0); // in-line terminal unit
        machine.poke_word(0x00DF_F100, 0xC200); // hires, four planes
        machine.poke_word(0x00DF_F096, 0x8300); // DMAEN | BPLEN
    }
    while original.machine().agnus().vpos < 0x0030 {
        original.machine_mut().tick();
    }
    while original.machine().agnus().hpos < 0x00D7 {
        original.machine_mut().tick();
    }
    assert!(!original.machine().agnus().ocs_ddf_hard_start_open());

    original.machine_mut().poke_word(0x00DF_F096, 0x0100); // clear BPLEN
    original.machine_mut().poke_word(0x00DF_F092, 0x0010);
    let completed_line = original.machine().agnus().vpos;
    while original.machine().agnus().vpos == completed_line {
        original.machine_mut().tick();
    }
    while original.machine().agnus().hpos < 0x0018 {
        original.machine_mut().tick();
    }
    assert!(original.machine().agnus().ocs_ddf_hard_start_open());
    assert_eq!(original.machine().agnus().ddf_start_match(), None);

    let idle_line = original.machine().agnus().vpos;
    while original.machine().agnus().vpos == idle_line {
        original.machine_mut().tick();
    }
    assert_eq!(original.machine().agnus().hpos, 0);
    assert!(
        original.machine().agnus().ocs_ddf_hard_start_open(),
        "the idle line must carry the reopened gate across EOL",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert!(
        restored.machine().agnus().ocs_ddf_hard_start_open(),
        "the true gate state must survive postcard restore",
    );
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "the open gate must be byte-stable through postcard",
    );

    original.machine_mut().poke_word(0x00DF_F096, 0x8300);
    restored.machine_mut().poke_word(0x00DF_F096, 0x8300);
    while original.machine().agnus().hpos < 0x0010 {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().hpos < 0x0010 {
        restored.machine_mut().tick();
    }

    assert_eq!(original.machine().agnus().ddf_start_match(), Some(0x0010),);
    assert_eq!(restored.machine().agnus().ddf_start_match(), Some(0x0010),);
    assert_eq!(
        original
            .machine()
            .agnus()
            .cck_bus_plan()
            .bitplane_dma_fetch_plane,
        Some(3),
    );
    assert_eq!(
        restored
            .machine()
            .agnus()
            .cck_bus_plan()
            .bitplane_dma_fetch_plane,
        Some(3),
    );
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "restored open-gate state must produce the same early DMA start",
    );
    Ok(())
}

#[test]
fn fat_agnus_hard_ddfstop_endpoint_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A2000OcsPal, blank_kickstart())?;
    assert!(original.machine().uses_fat_agnus_8372a());
    {
        let machine = original.machine_mut();
        machine.poke_word(0x00DF_F08E, 0x3081); // DIWSTRT
        machine.poke_word(0x00DF_F090, 0xF0C1); // DIWSTOP
        machine.poke_word(0x00DF_F092, 0x0018); // DDFSTRT
        machine.poke_word(0x00DF_F094, 0x00E0); // DDFSTOP beyond hard stop
        machine.poke_word(0x00DF_F100, 0xC200); // hires, four planes
        for (high, low, pointer) in [
            (0x00DF_F0E0, 0x00DF_F0E2, 0x0001_0000u32),
            (0x00DF_F0E4, 0x00DF_F0E6, 0x0001_2000),
            (0x00DF_F0E8, 0x00DF_F0EA, 0x0001_4000),
            (0x00DF_F0EC, 0x00DF_F0EE, 0x0001_6000),
        ] {
            machine.poke_word(high, (pointer >> 16) as u16);
            machine.poke_word(low, pointer as u16);
        }
        machine.poke_word(0x00DF_F096, 0x8300); // DMAEN | BPLEN
    }
    while original.machine().agnus().vpos < 0x0030 {
        original.machine_mut().tick();
    }
    let line_bases = original.machine().agnus().bpl_pt;
    while original.machine().agnus().hpos < 0x00D7 {
        original.machine_mut().tick();
    }
    assert_eq!(original.machine().agnus().ddf_fetch_end(), None);

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A2000OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert!(restored.machine().uses_fat_agnus_8372a());
    assert_eq!(restored.machine().agnus().ddf_fetch_end(), None);

    while original.machine().agnus().hpos < 0x00D8 {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().hpos < 0x00D8 {
        restored.machine_mut().tick();
    }
    assert_eq!(original.machine().agnus().ddf_fetch_end(), Some(0x00DF));
    assert_eq!(restored.machine().agnus().ddf_fetch_end(), Some(0x00DF));

    let pending_snapshot = original.snapshot()?;
    let mut pending_restored = AmigaOcsRuntime::new(Model::A2000OcsPal, blank_kickstart())?;
    pending_restored.restore(&pending_snapshot)?;
    assert_eq!(
        pending_restored.machine().agnus().ddf_fetch_end(),
        Some(0x00DF),
    );

    let line = original.machine().agnus().vpos;
    while original.machine().agnus().vpos == line {
        original.machine_mut().tick();
    }
    while restored.machine().agnus().vpos == line {
        restored.machine_mut().tick();
    }
    while pending_restored.machine().agnus().vpos == line {
        pending_restored.machine_mut().tick();
    }
    assert_eq!(
        original.machine().agnus().bpl_pt,
        restored.machine().agnus().bpl_pt,
        "pre-event Fat Agnus restore must preserve terminal fetches",
    );
    assert_eq!(
        original.machine().agnus().bpl_pt,
        pending_restored.machine().agnus().bpl_pt,
        "pending Fat Agnus restore must preserve terminal fetches",
    );
    for (plane, base) in line_bases.into_iter().enumerate().take(4) {
        assert_eq!(
            original.machine().agnus().bpl_pt[plane],
            base + 100,
            "BPL{} enhanced hard-stop byte count",
            plane + 1,
        );
    }
    Ok(())
}

#[test]
fn a2000_fat_agnus_snapshot_round_trips_extension_state() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A2000OcsPal, blank_kickstart())?;
    assert!(original.machine().uses_fat_agnus_8372a());

    // Populate wrapper-only state rather than proving only that the inner
    // OCS Agnus serializes. HTOTAL/VTOTAL/BEAMCON0 drive the concrete ECS
    // clock path; BLTSIZV remains sticky for a later BLTSIZH start.
    original.machine_mut().poke_word(0x00DF_F1C0, 3);
    original.machine_mut().poke_word(0x00DF_F1C8, 1);
    original.machine_mut().poke_word(0x00DF_F1DC, 0x00A0);
    original.machine_mut().poke_word(0x00DF_F05C, 2);
    let mut host = null_host();
    original.run_until(MachineTime::new(64), &mut host)?;

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A2000OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;

    assert!(restored.machine().uses_fat_agnus_8372a());
    assert_eq!(restored.machine().read_word(0x00DF_F07C), 0xFFFF);
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "Fat Agnus wrapper state must be byte-stable through postcard"
    );

    let target = original.time().saturating_add(64);
    let mut host_a = null_host();
    original.run_until(target, &mut host_a)?;
    let mut host_b = null_host();
    restored.run_until(target, &mut host_b)?;
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "programmed Fat Agnus timing must remain deterministic after restore"
    );
    Ok(())
}

#[test]
fn ecs_vertical_diw_latch_survives_snapshot_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?;
    original.machine_mut().poke_word(0x00DF_F090, 0x10C1);
    original.machine_mut().poke_word(0x00DF_F08E, 0x0081);
    original.machine_mut().poke_word(0x00DF_F1E4, 0x0000);
    original.machine_mut().poke_word(0x00DF_F1DC, 0x00A0);
    assert!(
        original.machine().agnus_ecs().vertical_diw_active(),
        "a line-zero VSTART comparator should open the vertical-DIW latch",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;

    assert!(restored.machine().agnus_ecs().vertical_diw_active());
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "the hidden vertical-DIW latch must be byte-stable through postcard",
    );
    Ok(())
}

#[test]
fn ecs_programmed_hblank_latch_survives_snapshot_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?;
    original.machine_mut().poke_word(0x00DF_F1C4, 0x0040); // HBSTRT
    original.machine_mut().poke_word(0x00DF_F1C6, 0x0080); // HBSTOP
    original.machine_mut().poke_word(0x00DF_F100, 0x0001); // ECSENA
    original.machine_mut().poke_word(0x00DF_F106, 0x0001); // EXTBLKEN

    // Reach HBSTRT with BLANKEN clear, then enable it too late. This is the
    // non-vacuous state that requires separate raw and routed ECS latches.
    for _ in 0..2_048 {
        original.machine_mut().tick();
        if original.machine().agnus_ecs().programmed_hblank_active() {
            break;
        }
    }
    assert!(
        original.machine().agnus_ecs().programmed_hblank_active(),
        "the test must observe the programmed HBSTRT event",
    );
    assert!(
        !original
            .machine()
            .agnus_ecs()
            .programmed_hblank_routed_active(),
        "BLANKEN was clear when HBSTRT matched",
    );
    original.machine_mut().poke_word(0x00DF_F1DC, 0x0028); // PAL | BLANKEN
    assert!(
        !original
            .machine()
            .agnus_ecs()
            .programmed_hblank_routed_active(),
        "enabling BLANKEN after HBSTRT must not synthesize routed blanking",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert!(restored.machine().agnus_ecs().programmed_hblank_active());
    assert!(
        !restored
            .machine()
            .agnus_ecs()
            .programmed_hblank_routed_active()
    );
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "the two hidden ECS horizontal-blank latches must be byte-stable",
    );

    let mut reached_stop = false;
    for _ in 0..2_048 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
        let original_active = original.machine().agnus_ecs().programmed_hblank_active();
        let restored_active = restored.machine().agnus_ecs().programmed_hblank_active();
        assert_eq!(original_active, restored_active);
        if !original_active {
            reached_stop = true;
            break;
        }
    }
    assert!(reached_stop, "HBSTOP must remain reachable after restore");
    assert_eq!(original.snapshot()?, restored.snapshot()?);
    Ok(())
}

#[test]
fn aga_programmed_hblank_latch_survives_snapshot_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    original.machine_mut().poke_word(0x00DF_F1C4, 0x0040); // HBSTRT
    original.machine_mut().poke_word(0x00DF_F1C6, 0x0080); // HBSTOP
    original.machine_mut().poke_word(0x00DF_F100, 0x0001); // ECSENA
    original.machine_mut().poke_word(0x00DF_F106, 0x0001); // EXTBLKEN
    assert!(
        !original.machine().agnus_aga().blanken_enabled(),
        "Lisa's programmed blanking path must not depend on BLANKEN",
    );

    for _ in 0..2_048 {
        original.machine_mut().tick();
        if original.machine().denise_aga().programmed_hblank_active() {
            break;
        }
    }
    assert!(
        original.machine().denise_aga().programmed_hblank_active(),
        "the test must observe Lisa's programmed HBSTRT event",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    restored.restore(&snapshot)?;
    assert!(restored.machine().denise_aga().programmed_hblank_active());
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "Lisa's hidden horizontal-blank latch must be byte-stable",
    );

    let mut reached_stop = false;
    for _ in 0..2_048 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
        let original_active = original.machine().denise_aga().programmed_hblank_active();
        let restored_active = restored.machine().denise_aga().programmed_hblank_active();
        assert_eq!(original_active, restored_active);
        if !original_active {
            reached_stop = true;
            break;
        }
    }
    assert!(
        reached_stop,
        "Lisa HBSTOP must remain reachable after restore"
    );
    assert_eq!(original.snapshot()?, restored.snapshot()?);
    Ok(())
}

#[test]
fn ocs_copper_color_stage_survives_snapshot_round_trip() -> Result<(), Box<dyn Error>> {
    const NEW_COLOR: u16 = 0x0ABC;

    let mut original = AmigaOcsRuntime::blank(Model::A500OcsPal);
    AmigaDriver::dispatch_copper_write(original.machine_mut(), 0x0180, NEW_COLOR);

    let board_before = original
        .machine()
        .denise()
        .board_pipeline_diagnostic_snapshot();
    assert_eq!(board_before.pending_early_writes.len(), 1);
    assert_eq!(board_before.pending_early_writes[0].register, 0x0180);
    assert_eq!(board_before.pending_early_writes[0].value, NEW_COLOR);
    assert_eq!(original.machine().denise().color(0), 0);

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::blank(Model::A500OcsPal);
    restored.restore(&snapshot)?;

    assert_eq!(
        restored
            .machine()
            .denise()
            .board_pipeline_diagnostic_snapshot(),
        board_before,
    );
    assert_eq!(snapshot, restored.snapshot()?);

    original.machine_mut().tick();
    restored.machine_mut().tick();

    for runtime in [&original, &restored] {
        assert!(
            runtime
                .machine()
                .denise()
                .board_pipeline_diagnostic_snapshot()
                .pending_early_writes
                .is_empty()
        );
        assert_eq!(runtime.machine().denise().color(0), NEW_COLOR);
    }
    assert_eq!(original.snapshot()?, restored.snapshot()?);
    Ok(())
}

#[test]
fn aga_delayed_color_write_survives_snapshot_round_trip() -> Result<(), Box<dyn Error>> {
    const TARGET_VPOS: u16 = 0x0032;
    const TARGET_HPOS: u16 = 0x0080;
    const VIEWPORT_V_START: u16 = 0x0019;
    const VIEWPORT_H_START: u16 = 0x002C;
    const OLD_ARGB: u32 = 0xFF11_2233;
    const NEW_ARGB: u32 = 0xFFAA_BBCC;

    let mut original = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    {
        let machine = original.machine_mut();
        machine.poke_word(0x00DF_F08E, 0x3081); // DIWSTRT
        machine.poke_word(0x00DF_F090, 0xF0C1); // DIWSTOP
        machine.poke_word(0x00DF_F100, 0x9000); // hires, one plane
        machine.poke_word(0x00DF_F180, 0x0123); // old COLOR00
    }

    // Stop after phase zero has rendered at a known visible beam position.
    // The next machine tick will render phase one's two adjacent hires
    // samples without advancing Alice's horizontal counter.
    let mut reached_target = false;
    for _ in 0..100_000 {
        let at_target = {
            let machine = original.machine();
            machine.agnus().vpos == TARGET_VPOS
                && machine.agnus().hpos == TARGET_HPOS
                && machine.scheduler_diagnostic_snapshot().cck_phase == 1
        };
        if at_target {
            reached_target = true;
            break;
        }
        original.machine_mut().tick();
    }
    assert!(
        reached_target,
        "the beam must reach the visible target before the guard expires",
    );
    assert!(original.machine().agnus_aga().vertical_diw_active());
    assert_eq!(
        original
            .machine()
            .denise()
            .board_pipeline_diagnostic_snapshot()
            .last_begin_line,
        Some(TARGET_VPOS),
    );
    assert!(
        original
            .machine()
            .denise_aga()
            .diagnostic_snapshot()
            .delayed_color_write
            .is_none(),
        "the setup COLOR00 write must have reached the output",
    );

    let framebuffer_width = original.machine().denise().framebuffer_size().0 as usize;
    let row = usize::from(TARGET_VPOS - VIEWPORT_V_START) * 2;
    let phase_zero_x = usize::from(TARGET_HPOS - VIEWPORT_H_START) * 4;
    let phase_zero_offset = row * framebuffer_width + phase_zero_x;
    assert_eq!(
        &original.machine().denise().framebuffer()[phase_zero_offset..phase_zero_offset + 2],
        &[OLD_ARGB, OLD_ARGB],
        "phase zero must already be rendering the old visible colour",
    );

    original.machine_mut().poke_word(0x00DF_F180, 0x8ABC); // new COLOR00 + genlock

    let delayed = original
        .machine()
        .denise_aga()
        .diagnostic_snapshot()
        .delayed_color_write
        .expect("the second COLOR00 write must retain Lisa's prior output sample");
    assert_eq!(delayed.palette_index, 0);
    assert_eq!(delayed.previous_rgb24, 0x0011_2233);
    assert_eq!(delayed.previous_rgb12, Some(0x0123));
    assert!(!delayed.previous_genlock);
    assert!(
        original
            .machine()
            .denise_aga()
            .diagnostic_snapshot()
            .pending_early_color_write
            .is_none(),
        "a post-output CPU/debug write must not re-enter the Copper stage",
    );
    assert!(
        original
            .machine()
            .denise_aga()
            .diagnostic_snapshot()
            .palette_genlock[0],
        "the new COLOR00 genlock flag must be live before the snapshot",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    restored.restore(&snapshot)?;
    assert_eq!(
        (
            restored.machine().agnus().vpos,
            restored.machine().agnus().hpos,
            restored.machine().scheduler_diagnostic_snapshot().cck_phase,
        ),
        (TARGET_VPOS, TARGET_HPOS, 1),
        "restore must resume at the same visible output phase",
    );
    assert!(restored.machine().agnus_aga().vertical_diw_active());
    assert_eq!(
        restored
            .machine()
            .denise_aga()
            .diagnostic_snapshot()
            .delayed_color_write,
        Some(delayed),
    );
    assert!(
        restored
            .machine()
            .denise_aga()
            .diagnostic_snapshot()
            .palette_genlock[0],
        "restore must retain the new COLOR00 genlock flag",
    );
    original.machine_mut().tick();
    restored.machine_mut().tick();

    let phase_one_offset = phase_zero_offset + 2;
    for (name, runtime) in [("original", &original), ("restored", &restored)] {
        assert_eq!(
            &runtime.machine().denise().framebuffer()[phase_one_offset..phase_one_offset + 2],
            &[OLD_ARGB, NEW_ARGB],
            "{name} phase one must retain the old colour for one Lisa sample",
        );
        assert!(
            runtime
                .machine()
                .denise_aga()
                .diagnostic_snapshot()
                .pending_early_color_write
                .is_none(),
            "{name} must not acquire a Copper-only early-stage write",
        );
        assert!(
            runtime
                .machine()
                .denise_aga()
                .diagnostic_snapshot()
                .delayed_color_write
                .is_none(),
            "{name} Lisa delay must retire after one output sample",
        );
    }

    original.machine_mut().tick();
    restored.machine_mut().tick();

    let next_phase_zero_offset = phase_zero_offset + 4;
    for (name, runtime) in [("original", &original), ("restored", &restored)] {
        assert_eq!(
            &runtime.machine().denise().framebuffer()
                [next_phase_zero_offset..next_phase_zero_offset + 2],
            &[NEW_ARGB, NEW_ARGB],
            "{name} next phase must use the new colour throughout",
        );
        assert!(
            runtime
                .machine()
                .denise_aga()
                .diagnostic_snapshot()
                .delayed_color_write
                .is_none(),
            "{name} delayed write must expire after one Lisa output sample",
        );
    }
    assert_eq!(
        original.machine().denise_aga().diagnostic_snapshot(),
        restored.machine().denise_aga().diagnostic_snapshot(),
    );
    assert_eq!(
        original
            .machine()
            .denise()
            .board_pipeline_diagnostic_snapshot(),
        restored
            .machine()
            .denise()
            .board_pipeline_diagnostic_snapshot(),
    );
    Ok(())
}

#[test]
fn aga_copper_color_stages_survive_snapshot_round_trip() -> Result<(), Box<dyn Error>> {
    const TARGET_VPOS: u16 = 0x0032;
    const TARGET_HPOS: u16 = 0x0080;
    const VIEWPORT_V_START: u16 = 0x0019;
    const VIEWPORT_H_START: u16 = 0x002C;
    const OLD_ARGB: u32 = 0xFF11_2233;
    const NEW_ARGB: u32 = 0xFFAA_BBCC;

    let mut original = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    {
        let machine = original.machine_mut();
        machine.poke_word(0x00DF_F08E, 0x3081); // DIWSTRT
        machine.poke_word(0x00DF_F090, 0xF0C1); // DIWSTOP
        machine.poke_word(0x00DF_F100, 0x9000); // hires, one plane
        machine.poke_word(0x00DF_F180, 0x0123); // old COLOR00
    }

    let mut reached_target = false;
    for _ in 0..100_000 {
        let at_target = {
            let machine = original.machine();
            machine.agnus().vpos == TARGET_VPOS
                && machine.agnus().hpos == TARGET_HPOS
                && machine.scheduler_diagnostic_snapshot().cck_phase == 1
        };
        if at_target {
            reached_target = true;
            break;
        }
        original.machine_mut().tick();
    }
    assert!(
        reached_target,
        "the beam must reach the visible target before the guard expires",
    );
    assert!(original.machine().agnus_aga().vertical_diw_active());
    assert!(
        original
            .machine()
            .denise_aga()
            .diagnostic_snapshot()
            .delayed_color_write
            .is_none(),
        "the setup COLOR00 write must have reached the output",
    );

    let framebuffer_width = original.machine().denise().framebuffer_size().0 as usize;
    let row = usize::from(TARGET_VPOS - VIEWPORT_V_START) * 2;
    let phase_zero_x = usize::from(TARGET_HPOS - VIEWPORT_H_START) * 4;
    let phase_zero_offset = row * framebuffer_width + phase_zero_x;
    assert_eq!(
        &original.machine().denise().framebuffer()[phase_zero_offset..phase_zero_offset + 2],
        &[OLD_ARGB, OLD_ARGB],
    );

    AmigaDriver::dispatch_copper_write(original.machine_mut(), 0x0180, 0x8ABC);
    let early = original
        .machine()
        .denise_aga()
        .diagnostic_snapshot()
        .pending_early_color_write
        .expect("Copper COLOR00 must remain in the pre-output stage");
    assert_eq!(early.palette_index, 0);
    assert_eq!(early.previous_rgb24, 0x0011_2233);
    assert_eq!(early.previous_rgb12, Some(0x0123));
    assert!(!early.previous_genlock);
    assert!(
        original
            .machine()
            .denise_aga()
            .diagnostic_snapshot()
            .delayed_color_write
            .is_none(),
    );
    assert!(
        original
            .machine()
            .denise()
            .board_pipeline_diagnostic_snapshot()
            .pending_early_writes
            .is_empty(),
        "Lisa owns the selector-aware pre-output COLOR stage",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    restored.restore(&snapshot)?;
    assert_eq!(
        restored
            .machine()
            .denise_aga()
            .diagnostic_snapshot()
            .pending_early_color_write,
        Some(early),
    );
    assert_eq!(
        restored.machine().denise_aga().diagnostic_snapshot(),
        original.machine().denise_aga().diagnostic_snapshot(),
    );

    original.machine_mut().tick();
    restored.machine_mut().tick();
    let phase_one_offset = phase_zero_offset + 2;
    for (name, runtime) in [("original", &original), ("restored", &restored)] {
        assert_eq!(
            &runtime.machine().denise().framebuffer()[phase_one_offset..phase_one_offset + 2],
            &[OLD_ARGB, OLD_ARGB],
            "{name} current board tick must retain the old colour",
        );
        let diagnostic = runtime.machine().denise_aga().diagnostic_snapshot();
        assert!(diagnostic.pending_early_color_write.is_none());
        assert_eq!(diagnostic.delayed_color_write, Some(early));
    }

    original.machine_mut().tick();
    restored.machine_mut().tick();
    let next_phase_zero_offset = phase_zero_offset + 4;
    for (name, runtime) in [("original", &original), ("restored", &restored)] {
        assert_eq!(
            &runtime.machine().denise().framebuffer()
                [next_phase_zero_offset..next_phase_zero_offset + 2],
            &[OLD_ARGB, NEW_ARGB],
            "{name} next tick must retain one additional Lisa sample",
        );
        assert!(
            runtime
                .machine()
                .denise_aga()
                .diagnostic_snapshot()
                .delayed_color_write
                .is_none(),
        );
    }

    original.machine_mut().tick();
    restored.machine_mut().tick();
    let next_phase_one_offset = phase_zero_offset + 6;
    for (name, runtime) in [("original", &original), ("restored", &restored)] {
        assert_eq!(
            &runtime.machine().denise().framebuffer()
                [next_phase_one_offset..next_phase_one_offset + 2],
            &[NEW_ARGB, NEW_ARGB],
            "{name} later output must use the new colour throughout",
        );
    }
    assert_eq!(
        original.machine().denise_aga().diagnostic_snapshot(),
        restored.machine().denise_aga().diagnostic_snapshot(),
    );
    assert_eq!(
        original
            .machine()
            .denise()
            .board_pipeline_diagnostic_snapshot(),
        restored
            .machine()
            .denise()
            .board_pipeline_diagnostic_snapshot(),
    );
    assert_eq!(
        (
            original.machine().agnus().vpos,
            original.machine().agnus().hpos,
            original.machine().scheduler_diagnostic_snapshot().cck_phase,
        ),
        (
            restored.machine().agnus().vpos,
            restored.machine().agnus().hpos,
            restored.machine().scheduler_diagnostic_snapshot().cck_phase,
        ),
    );
    Ok(())
}

#[test]
fn aga_display_register_pipelines_survive_snapshot_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    {
        let machine = original.machine_mut();
        machine.poke_word(0x00DF_F1C4, 0x0740); // HBSTRT
        machine.poke_word(0x00DF_F1C6, 0x0350); // HBSTOP
        machine.poke_word(0x00DF_F100, 0x0001); // BPLCON0.ECSENA
        machine.poke_word(0x00DF_F106, 0x0001); // BPLCON3.EXTBLKEN
        machine.tick();
        machine.poke_word(0x00DF_F180, 0x0ABC); // COLOR00
    }

    let board_before = original
        .machine()
        .denise()
        .board_pipeline_diagnostic_snapshot();
    let selectors_before = original
        .machine()
        .denise_aga()
        .as_inner()
        .output_selector_pipeline();
    let lisa_before = original.machine().denise_aga().diagnostic_snapshot();

    assert!(board_before.pending_early_writes.is_empty());
    assert!(lisa_before.pending_early_color_write.is_none());
    assert!(lisa_before.delayed_color_write.is_some());
    assert!(
        !original
            .machine()
            .denise_aga()
            .as_inner()
            .output_ecsena_enabled()
    );
    assert!(
        !original
            .machine()
            .denise_aga()
            .as_inner()
            .output_extblken_enabled()
    );
    assert!(selectors_before[1].ecsena_enabled);
    assert!(selectors_before[1].extblken_enabled);
    assert_eq!(lisa_before.programmed_hblank_visible.hbstrt, 0);
    assert_eq!(lisa_before.programmed_hblank_visible.hbstop, 0);
    assert_eq!(lisa_before.programmed_hblank_pipeline[1].hbstrt, 0x0740);
    assert_eq!(lisa_before.programmed_hblank_pipeline[1].hbstop, 0x0350);

    let snapshot = original.snapshot()?;
    let mut restored = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    restored.restore(&snapshot)?;

    assert_eq!(
        restored
            .machine()
            .denise()
            .board_pipeline_diagnostic_snapshot(),
        board_before,
    );
    assert_eq!(
        restored
            .machine()
            .denise_aga()
            .as_inner()
            .output_selector_pipeline(),
        selectors_before,
    );
    assert_eq!(
        restored.machine().denise_aga().diagnostic_snapshot(),
        lisa_before
    );
    assert_eq!(snapshot, restored.snapshot()?);

    for _ in 0..3 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
    }
    assert_eq!(original.snapshot()?, restored.snapshot()?);
    assert!(
        original
            .machine()
            .denise_aga()
            .as_inner()
            .output_ecsena_enabled()
    );
    assert!(
        original
            .machine()
            .denise_aga()
            .as_inner()
            .output_extblken_enabled()
    );
    assert_eq!(
        original
            .machine()
            .denise_aga()
            .programmed_hblank_visible()
            .hbstrt,
        0x0740,
    );
    assert_eq!(
        original
            .machine()
            .denise_aga()
            .programmed_hblank_visible()
            .hbstop,
        0x0350,
    );
    Ok(())
}

#[test]
fn restore_rejects_wrong_model() -> Result<(), Box<dyn Error>> {
    let original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let snapshot = original.snapshot()?;

    let mut other_model = AmigaOcsRuntime::new(Model::A500OcsPalA501, blank_kickstart())?;
    let result = other_model.restore(&snapshot);
    assert!(result.is_err(), "restoring across models should fail");
    Ok(())
}

#[test]
fn restore_rejects_unknown_version() -> Result<(), Box<dyn Error>> {
    let mut runtime = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    // Crafted bytes that won't deserialize as the current envelope — postcard rejects
    // mismatched length / shape and the restore returns an error.
    let result = runtime.restore(&[0xFFu8; 4]);
    assert!(result.is_err(), "garbage bytes should not restore");
    Ok(())
}

#[test]
fn ecs_snapshot_restore_preserves_model_specific_gayle_composition() -> Result<(), Box<dyn Error>> {
    for (model, expected) in [(Model::A500PlusEcsPal, false), (Model::A600EcsPal, true)] {
        let mut runtime = AmigaEcsRuntime::blank(model);
        assert_eq!(
            runtime.machine().gayle_diagnostic_snapshot().is_some(),
            expected,
        );
        assert_eq!(runtime.machine().gary().gayle_present(), expected);
        if expected {
            runtime.machine_mut().poke_byte(0x00DA_8000, 0x5A);
        }

        let snapshot = runtime.snapshot()?;
        runtime.restore(&snapshot)?;

        assert_eq!(
            runtime.machine().gayle_diagnostic_snapshot().is_some(),
            expected,
        );
        assert_eq!(runtime.machine().gary().gayle_present(), expected);
        if expected {
            assert_eq!(
                runtime
                    .machine()
                    .gayle_diagnostic_snapshot()
                    .expect("A600 should retain Gayle")
                    .registers
                    .card_status,
                0x5A,
            );
        }
    }
    Ok(())
}

/// Take a real snapshot, hand-patch the leading postcard varint version
/// field back to 34, and confirm the version-mismatch arm fires with a
/// human-readable reason naming the snapshot version. The first byte
/// of a `SnapshotEnvelopeV35` is the postcard varint encoding of
/// `version`; for `SNAPSHOT_VERSION = 35` that byte is `0x23`.
/// Replacing it with another single-byte value keeps the envelope
/// length stable and lands us inside the explicit version-mismatch
/// branch instead of the postcard-parse-error branch above.
#[test]
fn restore_rejects_mismatched_snapshot_version() -> Result<(), Box<dyn Error>> {
    let runtime = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let mut bytes = runtime.snapshot()?;
    assert_eq!(
        bytes[0], 35,
        "postcard varint for SNAPSHOT_VERSION = 35 should be 0x23"
    );
    bytes[0] = 34;

    let mut other = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let err = other
        .restore(&bytes)
        .expect_err("version-34 snapshot should be rejected before payload decode");
    assert!(
        matches!(
            err,
            MachineError::InvalidSnapshot { ref reason }
                if reason == "unsupported snapshot version 34; expected 35"
        ),
        "expected version-mismatch reason, got {err:?}"
    );
    Ok(())
}

/// Snapshot taken with an ADF inserted into DF0 round-trips through
/// restore — the `Some(bytes)` arm of `decode` validates the persisted
/// image and mounts it on the candidate machine before commit. Without
/// this test the floppy0 re-insert path stays uncovered.
#[test]
fn restore_remounts_persisted_floppy_image() -> Result<(), Box<dyn Error>> {
    let mut runtime = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let disk = vec![0u8; DD.len()];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));
    runtime.load_media(&media)?;
    assert!(runtime.machine().drive().has_disk());

    let snapshot = runtime.snapshot()?;

    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert!(
        restored.machine().drive().has_disk(),
        "restore should re-mount the persisted disk image"
    );
    assert!(
        restored.machine().drive().status().write_protect,
        "archive media must retain its read-only mount state"
    );
    assert_eq!(
        restored.snapshot()?,
        snapshot,
        "media reattachment must preserve the snapshot as a byte-level fixed point"
    );
    Ok(())
}

fn assert_writable_live_floppy_snapshot<M>(
    chipset: &str,
    mut runtime: AmigaRuntime<M>,
    mut restored: AmigaRuntime<M>,
) -> Result<(), Box<dyn Error>>
where
    M: AmigaDriver + AmigaLiveAccess + AmigaMachine,
{
    let source_disk = vec![0u8; DD.len()];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &source_disk).writable(true));
    runtime.load_media(&media)?;
    assert!(
        !AmigaDriver::drive(runtime.machine()).status().write_protect,
        "{chipset}: a writable mount must deassert /DSKPROT"
    );

    let replacement_track = vec![0xA5; 11 * 512];
    let replacement_mfm = encode_mfm_track(&replacement_track, 0, 11);
    let drive = runtime.machine_mut().drive_mut();
    for bytes in replacement_mfm.as_chunks::<2>().0.iter() {
        drive.note_write_mfm_word(u16::from_be_bytes([bytes[0], bytes[1]]));
    }
    assert_eq!(
        drive.flush_write_capture(),
        11,
        "{chipset}: the live writable image should accept every replacement sector"
    );

    let live_bytes = AmigaDriver::drive(runtime.machine())
        .save_adf()
        .expect("writable disk remains mounted");
    assert_eq!(
        &live_bytes[..replacement_track.len()],
        replacement_track.as_slice(),
        "{chipset}: snapshot input must come from the guest-modified live image"
    );
    assert_ne!(
        live_bytes, source_disk,
        "{chipset}: the live image must differ from its mounted source"
    );

    let snapshot = runtime.snapshot()?;
    restored.restore(&snapshot)?;

    let restored_drive = AmigaDriver::drive(restored.machine());
    assert!(
        !restored_drive.status().write_protect,
        "{chipset}: restore must preserve the writable mount's deasserted /DSKPROT"
    );
    assert_eq!(
        restored_drive.diagnostic_snapshot().disk_writable,
        Some(true),
        "{chipset}: the restored drive must retain host write permission"
    );
    assert_eq!(
        restored_drive.save_adf().as_deref(),
        Some(live_bytes.as_slice()),
        "{chipset}: restore must reattach the modified live ADF rather than the original host bytes"
    );
    assert_eq!(
        restored.snapshot()?,
        snapshot,
        "{chipset}: writable media reattachment must remain a byte-level fixed point"
    );

    let second_track = vec![0x3C; 11 * 512];
    let second_mfm = encode_mfm_track(&second_track, 0, 11);
    let restored_drive = restored.machine_mut().drive_mut();
    for bytes in second_mfm.as_chunks::<2>().0.iter() {
        restored_drive.note_write_mfm_word(u16::from_be_bytes([bytes[0], bytes[1]]));
    }
    assert_eq!(
        restored_drive.flush_write_capture(),
        11,
        "{chipset}: the restored writable mount must accept a subsequent guest write"
    );
    let rewritten_bytes = restored_drive
        .save_adf()
        .expect("rewritten disk remains mounted");
    assert_eq!(
        &rewritten_bytes[..second_track.len()],
        second_track.as_slice(),
        "{chipset}: the post-restore write must persist to the live image"
    );
    assert_ne!(
        rewritten_bytes, live_bytes,
        "{chipset}: the post-restore write must change the restored image"
    );
    Ok(())
}

#[test]
fn restore_preserves_writable_live_floppy_image_across_chipset_tiers() -> Result<(), Box<dyn Error>>
{
    assert_writable_live_floppy_snapshot(
        "OCS A500",
        AmigaOcsRuntime::blank(Model::A500OcsPal),
        AmigaOcsRuntime::blank(Model::A500OcsPal),
    )?;
    assert_writable_live_floppy_snapshot(
        "ECS A500+",
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
    )?;
    assert_writable_live_floppy_snapshot(
        "AGA A1200",
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
    )?;
    Ok(())
}
