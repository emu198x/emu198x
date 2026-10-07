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

#[test]
fn cancelled_audio_startup_delivery_survives_live_restore() -> Result<(), Box<dyn Error>> {
    use commodore_agnus_ocs::{DmaAddressStage, DmaTransferTarget};
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
        channel: u16,
        second: bool,
        pending: bool,
    ) -> Result<(), Box<dyn Error>> {
        let base = 0x0A0 + channel * 16;
        let irq = 0x80 << channel;
        for (offset, value) in [(2, 0x1000), (4, 64), (6, 8)] {
            AmigaDriver::dispatch_custom_write(original.machine_mut(), base + offset, value);
        }
        AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x096, 0x8200 | (1 << channel));
        // Obtain stage identities through the machine's existing Paula
        // re-export; runtime does not depend directly on the chip crate.
        let mut phase = machine_commodore_amiga_ocs::Paula8364::new();
        let idle = phase.audio_diagnostic_snapshot().channels[0].state;
        phase.sync_audio_dma_control(0x0201);
        if second {
            phase.service_audio_dma_word(0, 0, false, 0);
        }
        let target = phase.audio_diagnostic_snapshot().channels[0].state;
        let mut admitted = None;
        for _ in 0..2000 {
            AmigaMachine::tick(original.machine_mut());
            if AmigaDriver::paula(original.machine())
                .audio_diagnostic_snapshot()
                .channels[usize::from(channel)]
            .state
                == target
                && let Some(DmaAddressStage::Transfer(transfer)) =
                    AmigaDriver::agnus(original.machine())
                        .dma_pipeline()
                        .address()
                && matches!(transfer.target, DmaTransferTarget::Audio { channel: c, .. } if u16::from(c) == channel)
            {
                admitted = Some(transfer);
                break;
            }
        }
        let transfer = admitted.expect("real startup transfer must be admitted");
        AmigaDriver::memory_mut(original.machine_mut()).write_word(transfer.address, 0x1122);
        AmigaDriver::dispatch_custom_write(original.machine_mut(), base + 2, 0x2000);
        AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x096, 0x0200 | (1 << channel));
        AmigaDriver::dispatch_custom_write(
            original.machine_mut(),
            0x09C,
            irq | if pending { 0x8000 } else { 0 },
        );
        assert_eq!(
            AmigaDriver::paula(original.machine())
                .audio_diagnostic_snapshot()
                .channels[usize::from(channel)]
            .state,
            idle
        );
        // Whole/half CCKs on both sides of the retained transfer's delivery.
        for checkpoint in 0..4 {
            let saved = original.snapshot()?;
            restored.restore(&saved)?;
            assert!(saved == restored.snapshot()?);
            let mut delivered = checkpoint >= 2;
            for _ in 0..40 {
                AmigaMachine::tick(original.machine_mut());
                AmigaMachine::tick(restored.machine_mut());
                let a = AmigaDriver::paula(original.machine());
                let b = AmigaDriver::paula(restored.machine());
                assert_eq!(a.audio_diagnostic_snapshot(), b.audio_diagnostic_snapshot());
                assert_eq!(a.intreq(), b.intreq());
                let ch = a.audio_diagnostic_snapshot().channels[usize::from(channel)];
                if ch.data == 0x1122 {
                    delivered = true;
                    assert_eq!(ch.dma_pointer, transfer.address + 2);
                    assert_eq!(ch.words_remaining, 64);
                }
            }
            assert!(delivered, "restored descriptor must deliver its word");
            let ch = AmigaDriver::paula(original.machine())
                .audio_diagnostic_snapshot()
                .channels[usize::from(channel)];
            assert_eq!(ch.state, idle);
            assert_eq!(ch.output_sample, if pending { 0 } else { 0x22 });
            assert_ne!(AmigaDriver::paula(original.machine()).intreq() & irq, 0);
            assert!(original.snapshot()? == restored.snapshot()?);
            original.restore(&saved)?;
            AmigaMachine::tick(original.machine_mut());
        }
        Ok(())
    }
    for channel in 0..4 {
        for second in [false, true] {
            for pending in [false, true] {
                check(
                    AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?,
                    AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?,
                    channel,
                    second,
                    pending,
                )?;
                check(
                    AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?,
                    AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?,
                    channel,
                    second,
                    pending,
                )?;
                check(
                    AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?,
                    AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?,
                    channel,
                    second,
                    pending,
                )?;
            }
        }
    }
    Ok(())
}

#[test]
fn paula_handover_sampling_history_survives_live_restore() -> Result<(), Box<dyn Error>> {
    fn advance<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        runtime: &mut AmigaRuntime<M>,
        elapsed: u32,
        period: u32,
        manual: bool,
    ) {
        if elapsed.is_multiple_of(2) {
            let clock = elapsed / 2 + 1;
            if clock == period + 1 {
                AmigaDriver::dispatch_custom_write(
                    runtime.machine_mut(),
                    0x096,
                    if manual { 0x8201 } else { 0x0201 },
                );
            }
            if manual && clock == period + 2 {
                AmigaDriver::dispatch_custom_write(runtime.machine_mut(), 0x096, 0x0201);
            }
            if clock == 2 * period {
                AmigaDriver::dispatch_custom_write(runtime.machine_mut(), 0x09C, 0x80);
            }
        }
        AmigaMachine::tick(runtime.machine_mut());
    }
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
        period: u32,
        manual: bool,
    ) -> Result<(), Box<dyn Error>> {
        // Prime the ordinary chip startup at a common origin. Subsequent mode
        // writes and all saved/replayed transitions use the shared board clock.
        for address in (0x1000..0x1010).step_by(2) {
            AmigaDriver::memory_mut(original.machine_mut()).write_word(address, 0x1122);
        }
        for (offset, value) in [(0x0A2, 0x1000), (0x0A4, 64), (0x0A6, period as u16)] {
            AmigaDriver::dispatch_custom_write(original.machine_mut(), offset, value);
        }
        if manual {
            AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x0AA, 0x1122);
        } else {
            AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x096, 0x8201);
            let p = AmigaDriver::paula_mut(original.machine_mut());
            p.tick_audio_cck(0x0201, None, |_| panic!("no fixture grant"));
            for word in [0xdead, 0x1122] {
                let (address, reload) = p.audio_dma_request(0).expect("startup request");
                p.service_audio_dma_word(0, address, reload, word);
            }
        }
        let playing = AmigaDriver::paula(original.machine())
            .audio_diagnostic_snapshot()
            .channels[0]
            .state;
        let mut elapsed = 0;
        for checkpoint in [
            2 * period,
            2 * period + 1,
            2 * (period + 2),
            2 * (period + 2) + 1,
        ] {
            while elapsed < checkpoint {
                advance(&mut original, elapsed, period, manual);
                elapsed += 1;
            }
            let ch = AmigaDriver::paula(original.machine())
                .audio_diagnostic_snapshot()
                .channels[0];
            assert_eq!(ch.manual_stop_sample_pending, manual);
            assert_eq!(ch.manual_stop_pending, None);
            assert_eq!(ch.output_sample, 0x22);
            let saved = original.snapshot()?;
            restored.restore(&saved)?;
            assert!(saved == restored.snapshot()?);
            let mut word_edge = false;
            for tick in elapsed..4 * period + 2 {
                advance(&mut original, tick, period, manual);
                advance(&mut restored, tick, period, manual);
                let a = AmigaDriver::paula(original.machine());
                let b = AmigaDriver::paula(restored.machine());
                assert_eq!(a.audio_diagnostic_snapshot(), b.audio_diagnostic_snapshot());
                assert_eq!(a.intreq(), b.intreq());
                if tick == 4 * period - 1 {
                    word_edge = true;
                    let ch = a.audio_diagnostic_snapshot().channels[0];
                    assert_eq!(ch.output_sample, if manual { 0x22 } else { 0x11 });
                    assert_eq!(ch.state == playing, !manual);
                    assert!(ch.interrupt_request_pending);
                    assert_eq!(a.intreq() & 0x80, 0);
                }
            }
            assert!(word_edge);
            assert_ne!(AmigaDriver::paula(original.machine()).intreq() & 0x80, 0);
            assert!(original.snapshot()? == restored.snapshot()?);
            original.restore(&saved)?;
        }
        Ok(())
    }
    for period in [8, 124, 65_536] {
        for manual in [false, true] {
            check(
                AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?,
                AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?,
                period,
                manual,
            )?;
            check(
                AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?,
                AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?,
                period,
                manual,
            )?;
            check(
                AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?,
                AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?,
                period,
                manual,
            )?;
        }
    }
    Ok(())
}

#[test]
fn paula_manual_stop_decisions_survive_live_restore() -> Result<(), Box<dyn Error>> {
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
        period: u16,
        stop: bool,
    ) -> Result<(), Box<dyn Error>> {
        let idle = AmigaDriver::paula(original.machine())
            .audio_diagnostic_snapshot()
            .channels[0]
            .state;
        AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x0A6, period);
        AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x0AA, 0x1122);
        let playing = AmigaDriver::paula(original.machine())
            .audio_diagnostic_snapshot()
            .channels[0]
            .state;
        assert_ne!(idle, playing);
        assert_eq!(
            AmigaDriver::paula(original.machine())
                .audio_diagnostic_snapshot()
                .channels[0]
                .manual_stop_pending,
            None
        );
        // For period one, acknowledge startup between delivery and output.
        // This is the same shared begin/write/finish order as a bus write.
        if !stop {
            AmigaDriver::paula_mut(original.machine_mut()).begin_audio_cck();
            AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x09C, 0x80);
        }
        let mut reached = false;
        for _ in 0..270_000 {
            let ch = AmigaDriver::paula(original.machine())
                .audio_diagnostic_snapshot()
                .channels[0];
            if ch.manual_stop_pending == Some(stop) {
                reached = true;
                break;
            }
            AmigaMachine::tick(original.machine_mut());
        }
        assert!(reached, "period={period} stop={stop}");
        // Preserve the decision while making a live recheck give the wrong answer.
        AmigaDriver::dispatch_custom_write(
            original.machine_mut(),
            0x09C,
            if stop { 0x80 } else { 0x8080 },
        );
        AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x0AA, 0x3344);
        for half_cck in [false, true] {
            let saved = original.snapshot()?;
            restored.restore(&saved)?;
            assert!(saved == restored.snapshot()?);
            let mut consumed = false;
            for _ in 0..2 {
                AmigaMachine::tick(original.machine_mut());
                AmigaMachine::tick(restored.machine_mut());
                let a = AmigaDriver::paula(original.machine());
                let b = AmigaDriver::paula(restored.machine());
                assert_eq!(a.audio_diagnostic_snapshot(), b.audio_diagnostic_snapshot());
                assert_eq!(a.intreq(), b.intreq());
                let ch = a.audio_diagnostic_snapshot().channels[0];
                if ch.manual_stop_pending.is_none() {
                    consumed = true;
                    assert_eq!(ch.state, if stop { idle } else { playing });
                    assert_eq!(ch.output_sample, if stop { 0x22 } else { 0x33 });
                    assert!(ch.interrupt_request_pending);
                }
            }
            assert!(consumed, "saved decision must govern the next edge");
            assert!(original.snapshot()? == restored.snapshot()?);
            original.restore(&saved)?;
            if !half_cck {
                AmigaMachine::tick(original.machine_mut());
            }
        }
        Ok(())
    }
    for period in [1, 2, 8, 0] {
        for stop in [false, true] {
            check(
                AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?,
                AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?,
                period,
                stop,
            )?;
            check(
                AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?,
                AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?,
                period,
                stop,
            )?;
            check(
                AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?,
                AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?,
                period,
                stop,
            )?;
        }
    }
    Ok(())
}

#[test]
fn paula_pending_loop_and_delayed_irq_survive_live_restore() -> Result<(), Box<dyn Error>> {
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
        attach: u16,
    ) -> Result<(), Box<dyn Error>> {
        AmigaDriver::memory_mut(original.machine_mut()).write_word(0x1000, 0x1122);
        for (offset, value) in [
            (0x0A2, 0x1000),
            (0x0A4, 1),
            (0x0A6, 124),
            (0x09E, 0x8000 | attach),
            (0x096, 0x8201),
        ] {
            AmigaDriver::dispatch_custom_write(original.machine_mut(), offset, value);
        }
        // Observe startup delivery, held wrap, selected edge and delivery.
        for stage in 0..4 {
            let mut reached = false;
            for _ in 0..10000 {
                let p = AmigaDriver::paula(original.machine());
                let ch = p.audio_diagnostic_snapshot().channels[0];
                let matches = match stage {
                    0 => ch.interrupt_request_pending && ch.current_word.is_none(),
                    1 => ch.loop_interrupt_pending && ch.period_counter > 1,
                    2 => ch.loop_interrupt_pending && ch.period_counter == 1,
                    _ => ch.interrupt_request_pending && ch.current_word.is_some(),
                };
                if matches {
                    reached = true;
                    break;
                }
                AmigaMachine::tick(original.machine_mut());
            }
            assert!(reached, "attach={attach} stage={stage}");
            // Clear already-visible startup/previous IRQs so replay must
            // deliver a new interrupt from the saved pending stage.
            AmigaDriver::paula_mut(original.machine_mut()).write_intreq(0x80);
            let saved = original.snapshot()?;
            restored.restore(&saved)?;
            assert!(saved == restored.snapshot()?);
            let mut delivered = false;
            for _ in 0..1200 {
                AmigaMachine::tick(original.machine_mut());
                AmigaMachine::tick(restored.machine_mut());
                let a = AmigaDriver::paula(original.machine());
                let b = AmigaDriver::paula(restored.machine());
                assert_eq!(a.audio_diagnostic_snapshot(), b.audio_diagnostic_snapshot());
                assert_eq!(a.intreq(), b.intreq());
                delivered |= a.intreq() & 0x80 != 0;
            }
            assert!(delivered, "saved pending IRQ must actually reach INTREQ");
            assert!(original.snapshot()? == restored.snapshot()?);
            original.restore(&saved)?;
            AmigaMachine::tick(original.machine_mut());
            AmigaMachine::tick(original.machine_mut());
        }
        Ok(())
    }
    for attach in [0, 0x10, 0x01, 0x11] {
        check(
            AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?,
            AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?,
            attach,
        )?;
        check(
            AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?,
            AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?,
            attach,
        )?;
        check(
            AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?,
            AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?,
            attach,
        )?;
    }
    Ok(())
}

#[test]
fn paula_modulation_and_pending_dma_survive_live_restore() -> Result<(), Box<dyn Error>> {
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
        attach: u16,
    ) -> Result<(), Box<dyn Error>> {
        for index in 0..512u32 {
            AmigaDriver::memory_mut(original.machine_mut())
                .write_word(0x1000 + index * 2, ((index % 3 + 1) * 17) as u16);
        }
        for (offset, value) in [
            (0x0A0, 0),
            (0x0A2, 0x1000),
            (0x0A4, 512),
            (0x0A6, 8),
            (0x0B6, 777),
            (0x0B8, 7),
            (0x09E, 0x8000 | attach),
            (0x096, 0x8201),
        ] {
            AmigaDriver::dispatch_custom_write(original.machine_mut(), offset, value);
        }
        for high_next in [false, true] {
            let mut reached = false;
            for _ in 0..4000 {
                let ch = AmigaDriver::paula(original.machine())
                    .audio_diagnostic_snapshot()
                    .channels[0];
                if ch.dma_active
                    && ch.current_word.is_some()
                    && ch.period_counter == 1
                    && ch.next_byte_is_high == high_next
                {
                    reached = true;
                    break;
                }
                AmigaMachine::tick(original.machine_mut());
            }
            assert!(
                reached,
                "live modulation boundary attach={attach} high_next={high_next}"
            );
            let saved = original.snapshot()?;
            restored.restore(&saved)?;
            assert!(saved == restored.snapshot()?);
            let before = AmigaDriver::paula(original.machine())
                .audio_diagnostic_snapshot()
                .channels[1];
            let mut target_changed = false;
            let mut request_seen = false;
            for _ in 0..1600 {
                AmigaMachine::tick(original.machine_mut());
                AmigaMachine::tick(restored.machine_mut());
                let a = AmigaDriver::paula(original.machine()).audio_diagnostic_snapshot();
                let b = AmigaDriver::paula(restored.machine()).audio_diagnostic_snapshot();
                assert_eq!(a, b);
                target_changed |=
                    a.channels[1].period != before.period || a.channels[1].volume != before.volume;
                request_seen |= a.channels[0].dma_requests_pending == 1;
                assert_eq!(
                    AmigaDriver::paula(original.machine()).mix_audio_stereo(),
                    AmigaDriver::paula(restored.machine()).mix_audio_stereo()
                );
            }
            assert!(
                target_changed && request_seen,
                "replay must exercise actual modulation and DMA"
            );
            assert!(original.snapshot()? == restored.snapshot()?);
            original.restore(&saved)?;
            AmigaMachine::tick(original.machine_mut());
            AmigaMachine::tick(original.machine_mut());
        }
        Ok(())
    }
    for attach in [0x10, 0x01, 0x11] {
        check(
            AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?,
            AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?,
            attach,
        )?;
        check(
            AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?,
            AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?,
            attach,
        )?;
        check(
            AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?,
            AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?,
            attach,
        )?;
    }
    Ok(())
}

#[test]
fn paula_zero_and_short_periods_survive_live_restore() -> Result<(), Box<dyn Error>> {
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
        period: u16,
    ) -> Result<(), Box<dyn Error>> {
        let expected = if period == 0 {
            65_536
        } else {
            u32::from(period)
        };
        AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x0A6, period);
        AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x0A8, 64);
        AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x0AA, 0x7F01);
        let targets = if period == 0 {
            vec![65_536, 65_535, 32_768, 2, 1]
        } else {
            (1..=expected).rev().collect()
        };
        let mut total_ticks = 0;
        for target in targets {
            while AmigaDriver::paula(original.machine())
                .audio_diagnostic_snapshot()
                .channels[0]
                .period_counter
                != target
            {
                AmigaMachine::tick(original.machine_mut());
                total_ticks += 1;
                assert!(total_ticks < 140_000, "counter must reach {target}");
            }
            let before = AmigaDriver::paula(original.machine()).audio_diagnostic_snapshot();
            assert_eq!(before.channels[0].period, period);
            assert_eq!(before.channels[0].effective_period, expected);
            let saved = original.snapshot()?;
            restored.restore(&saved)?;
            assert_eq!(
                before,
                AmigaDriver::paula(restored.machine()).audio_diagnostic_snapshot()
            );
            assert!(
                saved == restored.snapshot()?,
                "snapshot fixed point at {target}"
            );
            let mut transition_seen = false;
            let mut previous_sample = before.channels[0].output_sample;
            for _ in 0..8 {
                AmigaMachine::tick(original.machine_mut());
                AmigaMachine::tick(restored.machine_mut());
                let observed = AmigaDriver::paula(original.machine()).audio_diagnostic_snapshot();
                assert_eq!(
                    observed,
                    AmigaDriver::paula(restored.machine()).audio_diagnostic_snapshot()
                );
                assert_eq!(
                    AmigaDriver::paula(original.machine()).mix_audio_stereo(),
                    AmigaDriver::paula(restored.machine()).mix_audio_stereo(),
                );
                transition_seen |= observed.channels[0].output_sample != previous_sample;
                previous_sample = observed.channels[0].output_sample;
            }
            if target == 1 {
                assert!(
                    transition_seen,
                    "restore must cross a real sample transition"
                );
            }
            assert!(
                original.snapshot()? == restored.snapshot()?,
                "forward replay at {target}"
            );
            original.restore(&saved)?;
        }
        Ok(())
    }
    for period in [0, 1, 3] {
        check(
            AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?,
            AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?,
            period,
        )?;
        check(
            AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?,
            AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?,
            period,
        )?;
        check(
            AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?,
            AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?,
            period,
        )?;
    }
    Ok(())
}

#[test]
fn horizontal_display_window_survives_strobe_reset_and_half_cck_restore()
-> Result<(), Box<dyn Error>> {
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
        stop: u16,
    ) -> Result<(), Box<dyn Error>> {
        AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x08E, 0x2051);
        AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x090, 0x40_00 | stop);
        let mut ticks = 0;
        while {
            let a = AmigaDriver::agnus(original.machine());
            (a.vpos, a.hpos) != (0x21, 3)
        } {
            AmigaMachine::tick(original.machine_mut());
            ticks += 1;
            assert!(ticks < 16_000, "beam must reach the retained right edge");
        }
        let expected = stop == 0xD1;
        assert_eq!(
            original
                .machine()
                .denise_board_pipeline_diagnostic_snapshot()
                .horizontal_diw_active,
            expected,
            "only the ordinary stop has matched before counter reset"
        );
        for _ in 0..10 {
            restored.restore(&original.snapshot()?)?;
            AmigaMachine::tick(original.machine_mut());
            AmigaMachine::tick(restored.machine_mut());
            assert_eq!(
                original
                    .machine()
                    .denise_board_pipeline_diagnostic_snapshot()
                    .horizontal_diw_active,
                expected,
                "a strobe cannot close an unmatched overscan window"
            );
            assert_eq!(original.snapshot()?, restored.snapshot()?);
        }
        assert!(
            original
                .machine()
                .denise_board_pipeline_diagnostic_snapshot()
                .horizontal_counter
                .position()
                < 16
        );
        Ok(())
    }
    for stop in [0xC1, 0xD1] {
        check(
            AmigaOcsRuntime::blank(Model::A500OcsPal),
            AmigaOcsRuntime::blank(Model::A500OcsPal),
            stop,
        )?;
        check(
            AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
            AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
            stop,
        )?;
        check(
            AmigaA1200Runtime::blank(Model::A1200AgaPal),
            AmigaA1200Runtime::blank(Model::A1200AgaPal),
            stop,
        )?;
    }
    Ok(())
}

#[test]
fn malformed_combined_refresh_restore_preserves_the_destination() -> Result<(), Box<dyn Error>> {
    use commodore_agnus_ocs::{
        DisplayDmaChannel, DisplayDmaReservation, DmaTransfer, DmaTransferTarget,
    };
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut destination: AmigaRuntime<M>,
    ) -> Result<(), Box<dyn Error>> {
        AmigaDriver::agnus_mut(original.machine_mut()).hpos = 60;
        for _ in 0..2 {
            AmigaMachine::tick(original.machine_mut());
        }
        let reservation = DisplayDmaReservation {
            channel: DisplayDmaChannel::Bitplane(0),
            width_words: 1,
            fmode: 0,
            add_modulo: false,
        };
        assert!(
            AmigaDriver::agnus_mut(original.machine_mut()).admit_dma_transfer(DmaTransfer {
                target: DmaTransferTarget::DisplayRefresh {
                    reservation,
                    fixed_register: 0,
                },
                address: 0x2000,
            })
        );
        let before = destination.snapshot()?;
        let error = destination
            .restore(&original.snapshot()?)
            .expect_err("invalid fixed RGA signal");
        assert!(
            error
                .to_string()
                .contains("invalid saved DMA transfer target"),
            "{error}"
        );
        assert_eq!(
            destination.snapshot()?,
            before,
            "candidate rejection is transactional"
        );
        Ok(())
    }
    check(
        AmigaOcsRuntime::blank(Model::A500OcsPal),
        AmigaOcsRuntime::blank(Model::A500OcsPal),
    )?;
    check(
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
    )?;
    check(
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
    )
}

#[test]
fn combined_refresh_display_replays_through_live_service_on_every_chipset()
-> Result<(), Box<dyn Error>> {
    use commodore_agnus_ocs::{
        DisplayDmaChannel, DisplayDmaReservation, DmaAddressStage, DmaTransferTarget, SlotOwner,
    };
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
    ) -> Result<(), Box<dyn Error>> {
        // At h2, the timing register suppresses the display data strobe.
        // At h4, refresh's $1fe leaves BPL1DAT selected, using refresh's PT.
        for (reservation_hpos, data_selected) in [(1, false), (3, true)] {
            AmigaDriver::agnus_mut(original.machine_mut()).hpos = reservation_hpos - 1;
            for _ in 0..2 {
                AmigaMachine::tick(original.machine_mut());
            }
            let captured = AmigaDriver::agnus(original.machine()).refresh_dma_pointer();
            AmigaDriver::memory_mut(original.machine_mut()).write_word(captured, 0xABCD);
            let a = AmigaDriver::agnus_mut(original.machine_mut());
            a.bpl_pt[0] = 0x9000;
            a.bpl1mod = -6;
            let reservation = DisplayDmaReservation {
                channel: DisplayDmaChannel::Bitplane(0),
                width_words: 1,
                fmode: 0,
                add_modulo: true,
            };
            assert!(a.reserve_display_dma(reservation));
            restored.restore(&original.snapshot()?)?;
            for elapsed in 1..=6 {
                AmigaMachine::tick(original.machine_mut());
                AmigaMachine::tick(restored.machine_mut());
                let a = AmigaDriver::agnus(original.machine());
                if elapsed <= 2 {
                    let Some(DmaAddressStage::Transfer(t)) = a.dma_pipeline().address() else {
                        panic!("live addressing must retain the combined request");
                    };
                    assert!(matches!(t.target, DmaTransferTarget::DisplayRefresh { .. }));
                    assert_eq!(t.address, captured);
                    assert_eq!(
                        a.bpl_pt[0], 0x9000,
                        "fixed admission skips display pointer sampling"
                    );
                } else if elapsed <= 4 {
                    let plan = a.dma_service_plan().expect("retained combined service");
                    assert_eq!(plan.slot_owner, SlotOwner::Refresh);
                    assert!(!plan.cpu_chip_bus_granted);
                    let increment = if a.max_bitplanes == 8 {
                        0
                    } else if a.agnus_id >= 0x2000 {
                        0x200
                    } else {
                        2
                    };
                    assert_eq!(
                        a.refresh_dma_pointer(),
                        captured + increment,
                        "combined bitplane refresh retains REFPTR authority"
                    );
                    let payload = original
                        .machine()
                        .denise_board_pipeline_diagnostic_snapshot()
                        .pending_bitplane_dma;
                    if data_selected {
                        assert_eq!(
                            payload.expect("actual refresh-addressed RAM").words[0],
                            0xABCD
                        );
                        assert_eq!(
                            a.bpl_pt[0], captured,
                            "refresh suppresses display increment"
                        );
                    } else {
                        assert!(
                            payload.is_none(),
                            "combined timing register suppresses data"
                        );
                        assert_eq!(
                            a.bpl_pt[0],
                            captured + 2,
                            "combined bitplane skips MOD sampling"
                        );
                    }
                } else if data_selected {
                    assert_eq!(
                        original
                            .machine()
                            .denise_diagnostic_snapshot()
                            .bitplanes
                            .holding_data[0],
                        0xABCD,
                        "real RGA latch receives payload"
                    );
                }
                assert_eq!(original.snapshot()?, restored.snapshot()?);
                if elapsed == 2 {
                    let a = AmigaDriver::agnus_mut(original.machine_mut());
                    a.bpl_pt[0] = 0x1234;
                    a.bpl1mod = 100;
                }
                restored.restore(&original.snapshot()?)?;
            }
        }
        Ok(())
    }
    check(
        AmigaOcsRuntime::blank(Model::A500OcsPal),
        AmigaOcsRuntime::blank(Model::A500OcsPal),
    )?;
    check(
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
    )?;
    check(
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
    )
}

#[test]
fn overlapping_display_address_samples_before_outgoing_pointer_service()
-> Result<(), Box<dyn Error>> {
    use commodore_agnus_ocs::{
        DisplayDmaChannel, DisplayDmaReservation, DmaAddressStage, DmaTransfer, DmaTransferTarget,
        SlotOwner,
    };
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
    ) -> Result<(), Box<dyn Error>> {
        AmigaDriver::agnus_mut(original.machine_mut()).hpos = 60;
        for _ in 0..2 {
            AmigaMachine::tick(original.machine_mut());
        }
        AmigaDriver::memory_mut(original.machine_mut()).write_word(0x2000, 0x1111);
        AmigaDriver::memory_mut(original.machine_mut()).write_word(0x3000, 0x3333);
        let request = DisplayDmaReservation {
            channel: DisplayDmaChannel::Bitplane(0),
            width_words: 1,
            fmode: 0,
            add_modulo: true,
        };
        let transfer = |address, pointer_modulo| DmaTransfer {
            address,
            target: DmaTransferTarget::Display {
                reservation: request,
                pointer_modulo,
            },
        };
        let a = AmigaDriver::agnus_mut(original.machine_mut());
        a.bpl_pt[0] = 0x2000;
        a.bpl1mod = -4;
        assert!(a.reserve_display_dma(request));
        AmigaMachine::tick(original.machine_mut());
        assert_eq!(
            AmigaDriver::agnus(original.machine())
                .dma_pipeline()
                .address(),
            Some(DmaAddressStage::Transfer(transfer(0x2000, -4)))
        );
        let a = AmigaDriver::agnus_mut(original.machine_mut());
        assert!(a.reserve_display_dma(request));
        a.bpl_pt[0] = 0x3000;
        a.bpl1mod = 6;
        AmigaMachine::tick(original.machine_mut()); // second output of A's address CCK
        restored.restore(&original.snapshot()?)?;
        for elapsed in 1..=6 {
            AmigaMachine::tick(original.machine_mut());
            AmigaMachine::tick(restored.machine_mut());
            let a = AmigaDriver::agnus(original.machine());
            if elapsed <= 2 {
                assert_eq!(
                    a.dma_pipeline().address(),
                    Some(DmaAddressStage::Transfer(transfer(0x3000, 6))),
                    "next display PT must be sampled before the outgoing pointer update"
                );
                assert_eq!(a.dma_pipeline().service(), Some(transfer(0x2000, -4)));
                assert_eq!(
                    a.bpl_pt[0], 0x1FFE,
                    "outgoing A still updates its captured PT"
                );
            } else if elapsed <= 4 {
                assert_eq!(a.dma_pipeline().service(), Some(transfer(0x3000, 6)));
                assert_eq!(a.bpl_pt[0], 0x3008);
                assert_eq!(
                    original
                        .machine()
                        .denise_diagnostic_snapshot()
                        .bitplanes
                        .holding_data[0],
                    0x1111,
                    "A reaches the normal RGA latch"
                );
            } else {
                assert!(a.dma_pipeline().service().is_none());
                assert_eq!(a.bpl_pt[0], 0x3008);
                assert_eq!(
                    original
                        .machine()
                        .denise_diagnostic_snapshot()
                        .bitplanes
                        .holding_data[0],
                    0x3333,
                    "B fetches its separately captured source"
                );
            }
            if elapsed <= 4 {
                assert_eq!(
                    a.dma_service_plan().expect("outgoing owner").slot_owner,
                    SlotOwner::Bitplane(0)
                );
                let pending = original
                    .machine()
                    .denise_board_pipeline_diagnostic_snapshot()
                    .pending_bitplane_dma
                    .expect("actual outgoing memory data");
                assert_eq!(pending.words[0], if elapsed <= 2 { 0x1111 } else { 0x3333 });
            }
            assert_eq!(original.snapshot()?, restored.snapshot()?);
            restored.restore(&original.snapshot()?)?;
        }
        Ok(())
    }
    check(
        AmigaOcsRuntime::blank(Model::A500OcsPal),
        AmigaOcsRuntime::blank(Model::A500OcsPal),
    )?;
    check(
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
    )?;
    check(
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
    )
}

#[test]
fn bitplane_reservation_samples_ptmod_then_reads_memory_at_live_service()
-> Result<(), Box<dyn Error>> {
    use commodore_agnus_ocs::{
        DisplayDmaChannel, DisplayDmaReservation, DmaAddressStage, DmaTransfer, DmaTransferTarget,
    };
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
        widths: &[u8],
    ) -> Result<(), Box<dyn Error>> {
        AmigaDriver::agnus_mut(original.machine_mut()).hpos = 60;
        for _ in 0..2 {
            AmigaMachine::tick(original.machine_mut());
        }
        for &width in widths {
            let request = DisplayDmaReservation {
                channel: DisplayDmaChannel::Bitplane(0),
                width_words: width,
                fmode: 0,
                add_modulo: true,
            };
            let a = AmigaDriver::agnus_mut(original.machine_mut());
            a.bpl_pt[0] = 0x1000;
            a.bpl1mod = 100;
            assert!(a.reserve_display_dma(request));
            restored.restore(&original.snapshot()?)?;
            // Change the source before the address edge: this must be sampled.
            let a = AmigaDriver::agnus_mut(original.machine_mut());
            a.bpl_pt[0] = 0x2000;
            a.bpl1mod = -4;
            restored.restore(&original.snapshot()?)?;
            let addressed = DmaTransfer {
                address: 0x2000,
                target: DmaTransferTarget::Display {
                    reservation: request,
                    pointer_modulo: -4,
                },
            };
            for elapsed in 1..=6 {
                AmigaMachine::tick(original.machine_mut());
                AmigaMachine::tick(restored.machine_mut());
                let a = AmigaDriver::agnus(original.machine());
                if elapsed <= 2 {
                    assert_eq!(
                        a.dma_pipeline().address(),
                        Some(DmaAddressStage::Transfer(addressed))
                    );
                    assert!(a.dma_pipeline().service().is_none());
                    assert!(
                        original
                            .machine()
                            .denise_board_pipeline_diagnostic_snapshot()
                            .pending_bitplane_dma
                            .is_none(),
                        "addressing must not read RAM"
                    );
                } else if elapsed <= 4 {
                    assert_eq!(a.dma_pipeline().service(), Some(addressed));
                    let payload = original
                        .machine()
                        .denise_board_pipeline_diagnostic_snapshot()
                        .pending_bitplane_dma
                        .expect("actual service payload");
                    assert_eq!(
                        payload.width_words, 2,
                        "FMODE rewrite after addressing controls service width"
                    );
                    assert_eq!(
                        payload.words[0], 0xABCD,
                        "read RAM at service, after addressing"
                    );
                    assert_eq!(a.bpl_pt[0], 0x2000);
                } else {
                    assert!(
                        original
                            .machine()
                            .denise_board_pipeline_diagnostic_snapshot()
                            .pending_bitplane_dma
                            .is_none()
                    );
                    assert_eq!(
                        original
                            .machine()
                            .denise_diagnostic_snapshot()
                            .bitplanes
                            .holding_data[0],
                        0xABCD,
                        "addressed service must reach the real holding latch"
                    );
                }
                assert_eq!(original.snapshot()?, restored.snapshot()?);
                if elapsed == 1 {
                    let a = AmigaDriver::agnus_mut(original.machine_mut());
                    a.bpl_pt[0] = 0x9000;
                    a.bpl1mod = 200;
                    a.fmode = 2;
                    AmigaDriver::memory_mut(original.machine_mut()).write_word(0x2000, 0xABCD);
                }
                restored.restore(&original.snapshot()?)?;
            }
        }
        Ok(())
    }
    check(
        AmigaOcsRuntime::blank(Model::A500OcsPal),
        AmigaOcsRuntime::blank(Model::A500OcsPal),
        &[1],
    )?;
    check(
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        &[1],
    )?;
    check(
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        &[1, 2, 4],
    )
}

#[test]
fn addressed_bitplane_service_uses_saved_memory_and_replays_on_every_chipset()
-> Result<(), Box<dyn Error>> {
    use commodore_agnus_ocs::{
        DisplayDmaChannel, DisplayDmaReservation, DmaTransfer, DmaTransferTarget, SlotOwner,
    };
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
        widths: &[u8],
    ) -> Result<(), Box<dyn Error>> {
        AmigaDriver::agnus_mut(original.machine_mut()).hpos = 60;
        for _ in 0..2 {
            AmigaMachine::tick(original.machine_mut());
        }
        for &width in widths {
            let address = 0x2000;
            for (index, value) in [0x1111, 0x2222, 0x3333, 0x4444].into_iter().enumerate() {
                AmigaDriver::memory_mut(original.machine_mut())
                    .write_word(address + index as u32 * 2, value);
            }
            AmigaDriver::memory_mut(original.machine_mut()).write_word(0x9000, 0xEEEE);
            let a = AmigaDriver::agnus_mut(original.machine_mut());
            a.bpl_pt[0] = 0x9000;
            a.fmode = match width {
                1 => 0,
                2 => 2,
                4 => 3,
                _ => unreachable!(),
            };
            a.dmacon = 0; // An admitted transfer survives later DMA disable.
            let request = DmaTransfer {
                target: DmaTransferTarget::Display {
                    reservation: DisplayDmaReservation {
                        channel: DisplayDmaChannel::Bitplane(0),
                        width_words: 1,
                        fmode: 0,
                        add_modulo: false,
                    },
                    pointer_modulo: -4,
                },
                address,
            };
            assert!(a.admit_dma_transfer(request));
            for elapsed in 1..=4 {
                if elapsed == 1 {
                    AmigaMachine::tick(original.machine_mut());
                    restored.restore(&original.snapshot()?)?;
                } else {
                    AmigaMachine::tick(original.machine_mut());
                    AmigaMachine::tick(restored.machine_mut());
                }
                let a = AmigaDriver::agnus(original.machine());
                let pending = original
                    .machine()
                    .denise_board_pipeline_diagnostic_snapshot()
                    .pending_bitplane_dma;
                if elapsed <= 2 {
                    assert_eq!(a.dma_pipeline().service(), Some(request));
                    let plan = a.dma_service_plan().expect("retained bitplane owner");
                    assert_eq!(plan.slot_owner, SlotOwner::Bitplane(0));
                    assert!(!plan.cpu_chip_bus_granted);
                    let payload = pending.expect("serviced words in the normal RGA stage");
                    assert_eq!(payload.width_words, width);
                    let expected: &[u16] = match width {
                        1 => &[0x1111],
                        2 => &[0x1111, 0x1111], // Service-time FMODE page-mode lane.
                        4 => &[0x1111, 0x2222, 0x3333, 0x4444],
                        _ => unreachable!(),
                    };
                    assert_eq!(&payload.words[..usize::from(width)], expected);
                } else {
                    assert!(pending.is_none(), "normal RGA data retired once");
                    assert_eq!(
                        original
                            .machine()
                            .denise_diagnostic_snapshot()
                            .bitplanes
                            .holding_data[0],
                        0x1111,
                        "normal RGA must load the serviced word, despite a later RAM rewrite"
                    );
                }
                assert_eq!(
                    a.bpl_pt[0],
                    if elapsed == 1 {
                        address + u32::from(width) * 2 - 4
                    } else {
                        0xA000
                    }
                );
                assert_eq!(original.snapshot()?, restored.snapshot()?);
                // A post-service RAM change cannot replace the retained payload;
                // a pointer rewrite cannot be overwritten by duplicate retirement.
                if elapsed == 1 {
                    AmigaDriver::memory_mut(original.machine_mut()).write_word(address, 0xFFFF);
                    AmigaDriver::agnus_mut(original.machine_mut()).bpl_pt[0] = 0xA000;
                    AmigaDriver::agnus_mut(original.machine_mut()).fmode = 0;
                }
                restored.restore(&original.snapshot()?)?;
            }
        }
        Ok(())
    }
    check(
        AmigaOcsRuntime::blank(Model::A500OcsPal),
        AmigaOcsRuntime::blank(Model::A500OcsPal),
        &[1],
    )?;
    check(
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        &[1],
    )?;
    check(
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        &[1, 2, 4],
    )
}

#[test]
fn automatic_strobes_cross_the_live_pipeline_and_reset_denise() -> Result<(), Box<dyn Error>> {
    use commodore_agnus_ocs::{DmaAddressStage, DmaStrobe, DmaTransferTarget};
    use common_commodore_amiga::driver::AmigaDriver;
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut runtime: AmigaRuntime<M>,
        enhanced: bool,
    ) -> Result<(), Box<dyn Error>> {
        while AmigaDriver::agnus(runtime.machine()).vbl_count < 1 {
            AmigaMachine::tick(runtime.machine_mut());
        }
        // Observe every steady-field request, including the blank-start edge.
        for line in 0..312 {
            while AmigaDriver::agnus(runtime.machine()).vpos == line {
                AmigaMachine::tick(runtime.machine_mut());
                let a = AmigaDriver::agnus(runtime.machine());
                let strobe = if (enhanced && line == 0) || (1..=7).contains(&line) {
                    DmaStrobe::Equalisation
                } else if line <= 25 {
                    DmaStrobe::VerticalBlank
                } else {
                    DmaStrobe::Horizontal
                };
                if a.hpos == 2 {
                    assert!(
                        matches!(a.dma_pipeline().address(), Some(DmaAddressStage::Transfer(t))
                        if t.target == DmaTransferTarget::Strobe(strobe)),
                        "line {line}: automatic request absent or wrong"
                    );
                }
                if a.hpos == 3 {
                    assert!(
                        matches!(a.dma_pipeline().service(), Some(t)
                        if t.target == DmaTransferTarget::Strobe(strobe)),
                        "line {line}: serviced strobe absent or wrong"
                    );
                }
                // Two existing output ticks complete each CCK. At h=4 the
                // preceding serviced strobe commits after its normal stage.
                if a.hpos == 4
                    && AmigaDriver::cck_phase(runtime.machine()) == 0
                    && (enhanced || strobe != DmaStrobe::Equalisation)
                {
                    assert_eq!(
                        runtime
                            .machine()
                            .denise_board_pipeline_diagnostic_snapshot()
                            .horizontal_counter
                            .position(),
                        2,
                        "line {line}: reset commit"
                    );
                }
                if !(27..309).contains(&line)
                    && matches!(AmigaDriver::agnus(runtime.machine()).hpos, 2..=4)
                {
                    let saved = runtime.snapshot()?;
                    runtime.restore(&saved)?;
                    assert_eq!(runtime.snapshot()?, saved);
                }
            }
        }
        Ok(())
    }
    check(AmigaOcsRuntime::blank(Model::A500OcsPal), false)?;
    check(AmigaEcsRuntime::blank(Model::A500PlusEcsPal), true)?;
    check(AmigaA1200Runtime::blank(Model::A1200AgaPal), true)
}

#[test]
fn live_strobe_service_and_denise_counter_replay_on_every_chipset() -> Result<(), Box<dyn Error>> {
    use commodore_agnus_ocs::{DmaStrobe, DmaTransfer, DmaTransferTarget, SlotOwner};
    use common_commodore_amiga::driver::AmigaDriver;

    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
        enhanced: bool,
    ) -> Result<(), Box<dyn Error>> {
        AmigaDriver::agnus_mut(original.machine_mut()).hpos = 20;
        for _ in 0..40 {
            AmigaMachine::tick(original.machine_mut());
        }
        assert_eq!(
            original
                .machine()
                .denise_board_pipeline_diagnostic_snapshot()
                .horizontal_counter
                .position(),
            40,
            "Denise must advance on the actual master-derived output ticks"
        );
        for strobe in [
            DmaStrobe::Horizontal,
            DmaStrobe::VerticalBlank,
            DmaStrobe::Equalisation,
        ] {
            let before = original
                .machine()
                .denise_board_pipeline_diagnostic_snapshot()
                .horizontal_counter
                .position();
            let request = DmaTransfer {
                target: DmaTransferTarget::Strobe(strobe),
                address: 0,
            };
            assert!(AmigaDriver::agnus_mut(original.machine_mut()).admit_dma_transfer(request));
            let bytes = original.snapshot()?;
            restored.restore(&bytes)?;
            for elapsed in 1..=6 {
                AmigaMachine::tick(original.machine_mut());
                AmigaMachine::tick(restored.machine_mut());
                let counter = original
                    .machine()
                    .denise_board_pipeline_diagnostic_snapshot()
                    .horizontal_counter;
                let reset = enhanced || strobe != DmaStrobe::Equalisation;
                let expected = if reset && elapsed >= 4 {
                    elapsed - 2
                } else {
                    (before + elapsed) & 511
                };
                assert_eq!(
                    counter.position(),
                    expected,
                    "{strobe:?}, output tick {elapsed}"
                );
                if elapsed <= 2 {
                    assert_eq!(
                        AmigaDriver::agnus(original.machine())
                            .dma_pipeline()
                            .service(),
                        Some(request)
                    );
                    assert_eq!(
                        AmigaDriver::agnus(original.machine())
                            .dma_service_plan()
                            .expect("retained service owner")
                            .slot_owner,
                        SlotOwner::Refresh
                    );
                    assert!(
                        !AmigaDriver::agnus(original.machine())
                            .dma_service_plan()
                            .expect("retained service owner")
                            .cpu_chip_bus_granted
                    );
                } else {
                    assert!(
                        AmigaDriver::agnus(original.machine())
                            .dma_pipeline()
                            .service()
                            .is_none()
                    );
                }
                assert_eq!(
                    original.snapshot()?,
                    restored.snapshot()?,
                    "replay at output tick {elapsed}"
                );
                // Restore each populated boundary, including the half-CCK owner
                // and the pending counter commit, through the real envelope.
                restored.restore(&original.snapshot()?)?;
            }
        }
        Ok(())
    }
    check(
        AmigaOcsRuntime::blank(Model::A500OcsPal),
        AmigaOcsRuntime::blank(Model::A500OcsPal),
        false,
    )?;
    check(
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        true,
    )?;
    check(
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        true,
    )?;
    Ok(())
}
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
        // Hold DMA while the separate BLTSIZE write stage reaches Agnus.
        // This snapshots startup before either admitted startup operation.
        machine.poke_word(DMACON, 0x0040);
        machine.poke_word(BLTSIZE, (1 << 6) | 1);
        assert!(!machine.agnus().blitter_busy, "size strobe is still queued");
        machine.tick();
        machine.tick();
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
    assert_eq!(original.machine().agnus().blitter_ccks_remaining, 2);
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
    assert_eq!(restored_after.machine().agnus().blitter_ccks_remaining, 2);
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
    assert_eq!(original.machine().agnus().blitter_ccks_remaining, 2);

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
fn every_line_stage_and_pending_result_survive_snapshot_restore() -> Result<(), Box<dyn Error>> {
    for use_b in [false, true] {
        for target_phase in 0..6 {
            if !use_b && matches!(target_phase, 1 | 4) {
                continue;
            }
            let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
            {
                let machine = original.machine_mut();
                machine.poke_word(0x1000, 1);
                machine.poke_word(DMACON, DMACON_SET_DMA_BLITTER_NASTY);
                machine.poke_word(BLTCON0, 0x0BCA | if use_b { 0x0400 } else { 0 });
                machine.poke_word(BLTCON1, 0x0019);
                machine.poke_word(BLTAPTL, 0xFFFE);
                machine.poke_word(BLTBMOD, 0);
                machine.poke_word(BLTAMOD, 0);
                machine.poke_word(BLTCMOD, 0);
                machine.poke_word(BLTBDAT, 1);
                machine.poke_word(0x00DF_F04C, 0); // BLTBPTH
                machine.poke_word(0x00DF_F04E, 0x1000); // BLTBPTL
                machine.poke_word(BLTCPTH, 0);
                machine.poke_word(BLTCPTL, 0x2000);
                machine.poke_word(BLTDPTH, 0);
                machine.poke_word(BLTDPTL, 0x3000);
                machine.poke_word(BLTSIZE, (1 << 6) | 2);
            }
            let mut reached = false;
            for _ in 0..1_000 {
                let state = original.machine().agnus().blitter_diagnostic_snapshot();
                if state.execution.startup_ccks_remaining == 0
                    && state
                        .line
                        .as_ref()
                        .is_some_and(|line| line.phase == target_phase)
                {
                    reached = true;
                    break;
                }
                original.machine_mut().tick();
            }
            assert!(reached, "line stage {target_phase} never became visible");
            if target_phase == 5 {
                let line = original
                    .machine()
                    .agnus()
                    .blitter_diagnostic_snapshot()
                    .line
                    .ok_or("pending line absent")?;
                if use_b {
                    assert_eq!(line.pending_result, 0x8000);
                } else {
                    // Without B's reserved phase, D is selected while the
                    // preceding calculation is still in the address stage.
                    // Its result settles at service before D is admitted.
                    assert_eq!(line.pending_result, 0);
                    assert!(matches!(
                        original.machine().agnus().dma_pipeline().address(),
                        Some(commodore_agnus_ocs::DmaAddressStage::Transfer(
                            commodore_agnus_ocs::DmaTransfer {
                                target: commodore_agnus_ocs::DmaTransferTarget::BlitterInternal {
                                    operation: BlitterDmaOp::Internal,
                                    ..
                                },
                                ..
                            }
                        ))
                    ));
                }
                assert_eq!(line.pending_addr, 0x3000);
                assert_eq!(original.machine().read_chip_ram_byte(0x3000), 0);
            }
            let snapshot = original.snapshot()?;
            let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
            restored.restore(&snapshot)?;
            assert_eq!(snapshot, restored.snapshot()?);
            for _ in 0..100 {
                original.machine_mut().tick();
                restored.machine_mut().tick();
                assert_eq!(
                    original.snapshot()?,
                    restored.snapshot()?,
                    "line stage {target_phase} diverged after restore"
                );
            }
            assert!(!original.machine().agnus().blitter_busy);
            assert_eq!(original.machine().read_chip_ram_byte(0x3000), 0x80);
        }
    }
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
fn alice_source_finish_with_pending_d_survives_postcard_round_trip() -> Result<(), Box<dyn Error>> {
    const DESTINATION: u32 = 0x0000_2000;
    const DMA_BLITTER: u16 = 0x0040;

    let mut original = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    {
        let machine = original.machine_mut();
        machine.poke_word(DMACON, DMACON_SET_DMA_BLITTER_NASTY);
        machine.poke_word(BLTCON0, 0x01FF);
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
    original.machine_mut().poke_word(DMACON, DMA_BLITTER);
    while !original.machine().agnus().blitter_finish_emitted() {
        original.machine_mut().tick();
        guard += 1;
        assert!(
            guard < 2_000,
            "Alice source finish waited for disabled D DMA"
        );
    }
    assert!(original.machine().agnus().blitter_busy);
    assert_eq!(
        original.machine().agnus().blitter_completion_phase(),
        "final-write"
    );
    assert_eq!(original.machine().read_chip_ram_byte(DESTINATION), 0);
    assert_ne!(original.machine().intreq() & INT_BLIT, 0);

    let snapshot = original.snapshot()?;
    let mut restored = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    restored.restore(&snapshot)?;
    assert_eq!(snapshot, restored.snapshot()?);
    for _ in 0..32 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
        assert_eq!(original.snapshot()?, restored.snapshot()?);
        assert_eq!(original.machine().read_chip_ram_byte(DESTINATION), 0);
    }
    assert!(original.machine().agnus().blitter_busy);
    assert!(!original.machine().agnus().blitter_busy_visible());
    assert!(!original.machine().agnus().blitter_busy_copper());
    for runtime in [&mut original, &mut restored] {
        // A later D admission must not reassert the already acknowledged IRQ.
        runtime.machine_mut().poke_word(INTREQ, INT_BLIT);
        runtime
            .machine_mut()
            .poke_word(DMACON, 0x8000 | DMA_BLITTER);
    }
    while original.machine().agnus().blitter_busy {
        original.machine_mut().tick();
        restored.machine_mut().tick();
        guard += 1;
        assert!(guard < 3_000, "Alice pending D never drained");
        assert_eq!(original.snapshot()?, restored.snapshot()?);
    }
    assert_eq!(original.machine().read_chip_ram_byte(DESTINATION), 0xFF);
    assert_eq!(original.machine().read_chip_ram_byte(DESTINATION + 1), 0xFF);
    assert_eq!(original.machine().intreq() & INT_BLIT, 0);
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
        // The CPU may already have begun its next prefetch when the final
        // byte retires. Completion is the destination write, not bus idleness.
        if original.machine().read_chip_ram_byte(DESTINATION + 3) == 0xEF {
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
    use commodore_agnus_ocs::{DisplayDmaChannel, DmaTransferTarget};

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
    // The comparator is not a memory service. Registered boundary case 2
    // requests BPL4 at $12; its retained address reaches memory at $14.
    assert!(
        original
            .machine()
            .agnus()
            .dma_pipeline()
            .reservation()
            .is_none()
    );
    assert!(
        restored
            .machine()
            .agnus()
            .dma_pipeline()
            .reservation()
            .is_none()
    );
    let bases = original.machine().agnus().bpl_pt;
    let mut requests = Vec::new();
    let mut services = Vec::new();
    for _ in 0..14 {
        let previous_h = original.machine().agnus().hpos;
        original.machine_mut().tick();
        restored.machine_mut().tick();
        assert_eq!(
            original.snapshot()?,
            restored.snapshot()?,
            "half-CCK replay"
        );
        let agnus = original.machine().agnus();
        if agnus.hpos == previous_h {
            continue;
        }
        if let Some(request) = agnus.dma_pipeline().reservation()
            && let DisplayDmaChannel::Bitplane(plane) = request.channel
        {
            requests.push((agnus.hpos, plane));
        }
        if let Some(transfer) = agnus.dma_pipeline().service()
            && let DmaTransferTarget::Display { reservation, .. } = transfer.target
            && let DisplayDmaChannel::Bitplane(plane) = reservation.channel
        {
            services.push((agnus.hpos, plane, transfer.address));
            assert_eq!(agnus.bpl_pt[usize::from(plane)], transfer.address + 2);
        }
    }
    assert_eq!(
        requests,
        [
            (0x12, 3),
            (0x13, 1),
            (0x14, 2),
            (0x15, 0),
            (0x16, 3),
            (0x17, 1)
        ]
    );
    assert_eq!(
        services,
        [
            (0x14, 3, bases[3]),
            (0x15, 1, bases[1]),
            (0x16, 2, bases[2]),
            (0x17, 0, bases[0])
        ]
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
fn enhanced_vertical_close_replays_across_both_post_wrap_cells() -> Result<(), Box<dyn Error>> {
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut restored: AmigaRuntime<M>,
        vertical: fn(&M) -> bool,
    ) -> Result<(), Box<dyn Error>> {
        let a = AmigaDriver::agnus_mut(original.machine_mut());
        a.vpos = 0xF3;
        a.hpos = 226;
        AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x090, 0xF4C1);
        AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x08E, 0xF381);
        assert!(vertical(original.machine()));
        let mut observed = [0; 2];
        for _ in 0..8 {
            restored.restore(&original.snapshot()?)?;
            AmigaMachine::tick(original.machine_mut());
            AmigaMachine::tick(restored.machine_mut());
            let a = AmigaDriver::agnus(original.machine());
            assert_eq!(a.vpos, 0xF4);
            let active = a.hpos < 2;
            observed[usize::from(active)] += 1;
            assert_eq!(vertical(original.machine()), active);
            assert_eq!(vertical(restored.machine()), active);
            assert_eq!(original.snapshot()?, restored.snapshot()?);
        }
        assert_eq!(observed, [4, 4], "must cross the actual comparator edge");
        Ok(())
    }
    check(
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        |m| m.agnus_ecs().vertical_diw_active(),
    )?;
    check(
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        |m| m.agnus_aga().vertical_diw_active(),
    )
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
fn ecs_blanking_selector_edges_replay_at_both_normal_rga_stages() -> Result<(), Box<dyn Error>> {
    for register in [0x100, 0x106] {
        let mut original = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
        let partner = if register == 0x100 { 0x106 } else { 0x100 };
        original.machine_mut().poke_word(0x00DF_F000 + partner, 1);
        for _ in 0..4 {
            original.machine_mut().tick();
        }
        for enabled in [true, false, true, false] {
            original
                .machine_mut()
                .poke_word(0x00DF_F000 + register, u16::from(enabled));
            for retired_ticks in 1..=2 {
                let bytes = original.snapshot()?;
                let mut restored = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
                restored.restore(&bytes)?;
                assert!(
                    bytes == restored.snapshot()?,
                    "selector state must survive restore"
                );
                original.machine_mut().tick();
                restored.machine_mut().tick();
                let expected = if retired_ticks == 2 {
                    enabled
                } else {
                    !enabled
                };
                let selectors = original.machine().denise_ecs().output_selectors();
                assert_eq!(
                    selectors.ecsena_enabled && selectors.extblken_enabled,
                    expected
                );
                assert!(
                    original.snapshot()? == restored.snapshot()?,
                    "selector replay diverged"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn ecs_csync_blanking_in_flight_stages_survive_both_clock_phases() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?;
    for (register, value) in [
        (0x1C4, 0x40),
        (0x1C6, 0x48),
        (0x100, 1),
        (0x106, 1),
        (0x1DC, 0x28),
        (0x180, 0xFFF),
    ] {
        original
            .machine_mut()
            .poke_word(0x00DF_F000 + register, value);
    }
    // Restore during visible scanlines so the output comparison exercises
    // coloured and blank framebuffer samples, not only vertical blank.
    for _ in 0..25_000 {
        if original.machine().agnus_ecs().vpos == 44 && original.machine().agnus_ecs().hpos == 0 {
            break;
        }
        original.machine_mut().tick();
    }
    assert_eq!(original.machine().agnus_ecs().vpos, 44);
    let mut seen = std::collections::BTreeSet::new();
    let mut rising = false;
    let mut falling = false;
    for _ in 0..2_048 {
        let stages = original.machine().denise_ecs().csync_blanking();
        let key = (
            stages.cck_samples,
            stages.half_cck_sample,
            stages.output_level,
            AmigaDriver::cck_phase(original.machine()),
        );
        let levels = [
            stages.cck_samples[0],
            stages.cck_samples[1],
            stages.cck_samples[2],
            stages.half_cck_sample,
            stages.output_level,
        ];
        if levels.iter().all(|level| *level == levels[0]) || !seen.insert(key) {
            original.machine_mut().tick();
            continue;
        }
        rising |= stages.cck_samples[2] && !stages.output_level;
        falling |= !stages.cck_samples[2] && stages.output_level;
        let snapshot = original.snapshot()?;
        let mut replay = AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?;
        replay.restore(&snapshot)?;
        // Direct chip ticks leave host framebuffer statistics stale; restore
        // refreshes those statistics. Compare the complete machine payload.
        assert!(
            postcard::to_allocvec(&original.machine().snapshot_state())?
                == postcard::to_allocvec(&replay.machine().snapshot_state())?,
            "machine payload changed on restore at {key:?}"
        );
        assert_eq!(stages, replay.machine().denise_ecs().csync_blanking());
        original.machine_mut().tick();
        replay.machine_mut().tick();
        assert_eq!(
            original.machine().denise().framebuffer(),
            replay.machine().denise().framebuffer()
        );
        assert!(
            postcard::to_allocvec(&original.machine().snapshot_state())?
                == postcard::to_allocvec(&replay.machine().snapshot_state())?,
            "machine payload diverged after restore at {key:?}"
        );
    }
    assert!(
        rising && falling,
        "both signal edges must cross the retained stages"
    );
    for phase in 0..=1 {
        assert!(
            seen.iter().any(|state| state.3 == phase),
            "missing half-CCK phase {phase}"
        );
    }
    // Three pending CCK samples at both phases, plus the final half-CCK
    // boundary, for each edge: seven rising and seven falling states.
    assert_eq!(
        seen.len(),
        14,
        "missing in-flight restore boundary: {seen:?}"
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
fn ecs_dispatched_copper_color_survives_snapshot_round_trip() -> Result<(), Box<dyn Error>> {
    const NEW_COLOR: u16 = 0x0ABC;

    let mut original = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
    AmigaDriver::dispatch_copper_write(original.machine_mut(), 0x0180, NEW_COLOR);

    let board_before = original
        .machine()
        .denise()
        .board_pipeline_diagnostic_snapshot();
    assert!(board_before.pending_early_writes.is_empty());
    assert_eq!(original.machine().denise().color(0), NEW_COLOR);

    let snapshot = original.snapshot()?;
    let mut restored = AmigaEcsRuntime::blank(Model::A500PlusEcsPal);
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
    // Denise's serviced strobe drives its output position independently of
    // the Agnus beam. Phase zero is the preceding (even) output position.
    let position = original.machine().denise().output_comparator_position();
    assert_eq!(position, TARGET_HPOS * 2 - 7);
    let phase_zero_x = usize::from(position / 2 - VIEWPORT_H_START) * 8;
    let phase_zero_offset = row * framebuffer_width + phase_zero_x;
    assert_eq!(
        &original.machine().denise().framebuffer()[phase_zero_offset..phase_zero_offset + 4],
        &[OLD_ARGB, OLD_ARGB, OLD_ARGB, OLD_ARGB],
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

    let phase_one_offset = phase_zero_offset + 4;
    for (name, runtime) in [("original", &original), ("restored", &restored)] {
        assert_eq!(
            &runtime.machine().denise().framebuffer()[phase_one_offset..phase_one_offset + 4],
            &[OLD_ARGB, OLD_ARGB, NEW_ARGB, NEW_ARGB],
            "{name} phase one must retain the old colour for two 35 ns Lisa samples",
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
            "{name} Lisa delay must retire after one hires period",
        );
    }

    original.machine_mut().tick();
    restored.machine_mut().tick();

    let next_phase_zero_offset = phase_zero_offset + 8;
    for (name, runtime) in [("original", &original), ("restored", &restored)] {
        assert_eq!(
            &runtime.machine().denise().framebuffer()
                [next_phase_zero_offset..next_phase_zero_offset + 4],
            &[NEW_ARGB, NEW_ARGB, NEW_ARGB, NEW_ARGB],
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
    let position = original.machine().denise().output_comparator_position();
    assert_eq!(position, TARGET_HPOS * 2 - 7);
    let phase_zero_x = usize::from(position / 2 - VIEWPORT_H_START) * 8;
    let phase_zero_offset = row * framebuffer_width + phase_zero_x;
    assert_eq!(
        &original.machine().denise().framebuffer()[phase_zero_offset..phase_zero_offset + 4],
        &[OLD_ARGB, OLD_ARGB, OLD_ARGB, OLD_ARGB],
    );

    AmigaDriver::dispatch_copper_write(original.machine_mut(), 0x0180, 0x8ABC);
    let early = original
        .machine()
        .denise_aga()
        .diagnostic_snapshot()
        .delayed_color_write
        .expect("Copper COLOR00 must retain the two-sample Lisa palette stage");
    assert_eq!(early.palette_index, 0);
    assert_eq!(early.previous_rgb24, 0x0011_2233);
    assert_eq!(early.previous_rgb12, Some(0x0123));
    assert!(!early.previous_genlock);
    assert!(
        original
            .machine()
            .denise_aga()
            .diagnostic_snapshot()
            .pending_early_color_write
            .is_none(),
    );
    assert!(
        original
            .machine()
            .denise()
            .board_pipeline_diagnostic_snapshot()
            .pending_early_writes
            .is_empty(),
        "Lisa owns the selector-aware palette delay",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    restored.restore(&snapshot)?;
    assert_eq!(
        restored
            .machine()
            .denise_aga()
            .diagnostic_snapshot()
            .delayed_color_write,
        Some(early),
    );
    assert_eq!(
        restored.machine().denise_aga().diagnostic_snapshot(),
        original.machine().denise_aga().diagnostic_snapshot(),
    );

    original.machine_mut().tick();
    restored.machine_mut().tick();
    let phase_one_offset = phase_zero_offset + 4;
    for (name, runtime) in [("original", &original), ("restored", &restored)] {
        assert_eq!(
            &runtime.machine().denise().framebuffer()[phase_one_offset..phase_one_offset + 4],
            &[OLD_ARGB, OLD_ARGB, NEW_ARGB, NEW_ARGB],
            "{name} current tick must retain two 35 ns Lisa samples",
        );
        let diagnostic = runtime.machine().denise_aga().diagnostic_snapshot();
        assert!(diagnostic.pending_early_color_write.is_none());
        assert!(diagnostic.delayed_color_write.is_none());
    }

    original.machine_mut().tick();
    restored.machine_mut().tick();
    let next_phase_zero_offset = phase_zero_offset + 8;
    for (name, runtime) in [("original", &original), ("restored", &restored)] {
        assert_eq!(
            &runtime.machine().denise().framebuffer()
                [next_phase_zero_offset..next_phase_zero_offset + 4],
            &[NEW_ARGB; 4],
            "{name} next tick must use the new palette",
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
    let next_phase_one_offset = phase_zero_offset + 12;
    for (name, runtime) in [("original", &original), ("restored", &restored)] {
        assert_eq!(
            &runtime.machine().denise().framebuffer()
                [next_phase_one_offset..next_phase_one_offset + 4],
            &[NEW_ARGB, NEW_ARGB, NEW_ARGB, NEW_ARGB],
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
/// field back to 59, and confirm the version-mismatch arm fires with a
/// human-readable reason naming the snapshot version. The first byte
/// of a `SnapshotEnvelopeV60` is the postcard varint encoding of
/// `version`; for `SNAPSHOT_VERSION = 60` that byte is `0x3C`.
/// Replacing it with another single-byte value keeps the envelope
/// length stable and lands us inside the explicit version-mismatch
/// branch instead of the postcard-parse-error branch above.
#[test]
fn restore_rejects_mismatched_snapshot_version() -> Result<(), Box<dyn Error>> {
    let runtime = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let mut bytes = runtime.snapshot()?;
    assert_eq!(
        bytes[0], 60,
        "postcard varint for SNAPSHOT_VERSION = 60 should be 0x3C"
    );
    bytes[0] = 59;

    let mut other = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let err = other
        .restore(&bytes)
        .expect_err("version-59 snapshot should be rejected before payload decode");
    assert!(
        matches!(
            err,
            MachineError::InvalidSnapshot { ref reason }
                if reason == "unsupported snapshot version 59; expected 60"
        ),
        "expected version-mismatch reason, got {err:?}"
    );
    Ok(())
}

#[test]
fn ocs_fixed_blank_edges_survive_runtime_restore() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let mut visited = std::collections::BTreeSet::new();
    let mut levels = std::collections::BTreeSet::new();
    for _ in 0..30_000 {
        original.machine_mut().tick();
        let m = original.machine();
        let counter = m.denise().output_comparator_position();
        if m.agnus().vpos != 51
            || ![2, 13, 14, 15, 91, 92, 93].contains(&counter)
            || !visited.insert(counter)
        {
            continue;
        }
        let state = m.denise().ocs.fixed_hblank_active();
        levels.insert(state);
        let bytes = original.snapshot()?;
        let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
        restored.restore(&bytes)?;
        assert_eq!(state, restored.machine().denise().ocs.fixed_hblank_active());
        assert!(
            bytes == restored.snapshot()?,
            "restore at counter {counter}"
        );
        for _ in 0..200 {
            original.machine_mut().tick();
            restored.machine_mut().tick();
        }
        assert!(
            original.snapshot()? == restored.snapshot()?,
            "forward replay at counter {counter}"
        );
        original.restore(&bytes)?;
        if visited.len() == 7 {
            break;
        }
    }
    assert_eq!(visited.len(), 7, "both sides of each edge and the reset");
    assert_eq!(levels.len(), 2, "must save both real blank levels");
    Ok(())
}

#[test]
fn lisa_reset_blank_comparison_survives_half_cck_restore() -> Result<(), Box<dyn Error>> {
    for (start, stop) in [(1, 0xA0), (0x0301, 0xA0), (0x80, 1), (0x80, 0x0301)] {
        let mut original = AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?;
        for (reg, value) in [(0x100, 1), (0x106, 1), (0x1C4, start), (0x1C6, stop)] {
            original.machine_mut().poke_word(0x00DF_F000 + reg, value);
        }
        let mut visited = std::collections::BTreeSet::new();
        let mut levels = std::collections::BTreeSet::new();
        let mut pending_reset_seen = false;
        for _ in 0..100_000 {
            original.machine_mut().tick();
            let m = original.machine();
            let h = m.agnus().hpos;
            let phase = m.scheduler_diagnostic_snapshot().cck_phase;
            if m.agnus().vpos != 51 || !(3..=6).contains(&h) || !visited.insert((h, phase)) {
                continue;
            }
            let counter = m
                .denise_board_pipeline_diagnostic_snapshot()
                .horizontal_counter;
            pending_reset_seen |=
                counter.position() > 400 && counter.next_comparison_position() == 2;
            levels.insert(m.denise_aga().programmed_hblank_active());
            let bytes = original.snapshot()?;
            let mut restored = AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?;
            restored.restore(&bytes)?;
            assert!(
                bytes == restored.snapshot()?,
                "immediate reset-stage replay"
            );
            for _ in 0..400 {
                original.machine_mut().tick();
                restored.machine_mut().tick();
            }
            assert!(
                original.snapshot()? == restored.snapshot()?,
                "reset-stage forward replay"
            );
            original.restore(&bytes)?;
            if visited.len() == 8 {
                break;
            }
        }
        assert_eq!(visited.len(), 8);
        assert_eq!(levels.len(), 2, "must observe both sides of the blank edge");
        assert!(
            pending_reset_seen,
            "must save before the pending reset commits"
        );
    }
    Ok(())
}

#[test]
fn lisa_vertical_blank_strobes_and_edges_survive_runtime_restore() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?;
    for (register, value) in [(0x100, 1), (0x106, 1), (0x1C4, 0x80), (0x1C6, 0x07A0)] {
        original
            .machine_mut()
            .poke_word(0x00DF_F000 + register, value);
    }
    let mut visited = std::collections::BTreeSet::new();
    let mut pending_start = false;
    let mut pending_stop = false;
    let mut retained_blank = false;
    for _ in 0..300_000 {
        original.machine_mut().tick();
        let m = original.machine();
        let (v, h) = (m.agnus().vpos, m.agnus().hpos);
        let phase = m.scheduler_diagnostic_snapshot().cck_phase;
        if m.agnus().vbl_count != 1
            || ![0, 26].contains(&v)
            || ![3, 4, 5, 132, 164, 165].contains(&h)
            || !visited.insert((v, h, phase))
        {
            continue;
        }
        let state = m.denise_aga().diagnostic_snapshot().vertical_blanking;
        pending_start |= state.pending_programmed == Some(true);
        pending_stop |= state.pending_programmed == Some(false);
        retained_blank |= state.programmed_active;
        let bytes = original.snapshot()?;
        let mut restored = AmigaA1200Runtime::new(Model::A1200AgaPal, blank_kickstart())?;
        restored.restore(&bytes)?;
        assert_eq!(
            state,
            restored
                .machine()
                .denise_aga()
                .diagnostic_snapshot()
                .vertical_blanking
        );
        assert!(
            bytes == restored.snapshot()?,
            "restore must retain every saved byte at {v}:{h}.{phase}"
        );
        for _ in 0..400 {
            original.machine_mut().tick();
            restored.machine_mut().tick();
        }
        assert!(
            original.snapshot()? == restored.snapshot()?,
            "forward replay at {v}:{h}.{phase}"
        );
        original.restore(&bytes)?;
        if visited.len() == 24 {
            break;
        }
    }
    assert_eq!(visited.len(), 24, "all half-CCK strobe and edge boundaries");
    assert!(
        pending_start && pending_stop && retained_blank,
        "must save real pending transitions and a retained blank level"
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

#[test]
fn aga_collision_extension_survives_restore_and_base_write_resets_it() -> Result<(), Box<dyn Error>>
{
    let mut original = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    original.machine_mut().poke_word(0x00DF_F098, 0x0FFF);
    original.machine_mut().poke_word(0x00DF_F10E, 0x00C3);
    assert_eq!(
        original
            .machine()
            .denise_aga()
            .as_inner()
            .as_inner()
            .diagnostic_snapshot()
            .clxcon2,
        0x00C3
    );
    let bytes = original.snapshot()?;
    let mut restored = AmigaA1200Runtime::blank(Model::A1200AgaPal);
    restored.restore(&bytes)?;
    assert_eq!(restored.snapshot()?, bytes);
    assert_eq!(
        restored
            .machine()
            .denise_aga()
            .as_inner()
            .as_inner()
            .diagnostic_snapshot()
            .clxcon2,
        0x00C3
    );
    for _ in 0..512 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
    }
    assert_eq!(original.snapshot()?, restored.snapshot()?);
    for runtime in [&mut original, &mut restored] {
        runtime.machine_mut().poke_word(0x00DF_F098, 0x0000);
        assert_eq!(
            runtime
                .machine()
                .denise_aga()
                .as_inner()
                .as_inner()
                .diagnostic_snapshot()
                .clxcon2,
            0
        );
    }
    assert_eq!(original.snapshot()?, restored.snapshot()?);
    Ok(())
}

#[test]
fn a1200_prefetch_transfer_and_holding_register_survive_runtime_restore()
-> Result<(), Box<dyn Error>> {
    let mut rom = blank_kickstart();
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8..10].copy_from_slice(&0x4E71u16.to_be_bytes());
    rom[10..12].copy_from_slice(&0x7E01u16.to_be_bytes()); // MOVEQ #1,D7
    rom[12..14].copy_from_slice(&0x60FEu16.to_be_bytes());
    for held in [false, true] {
        let mut original = AmigaA1200Runtime::new(Model::A1200AgaPal, rom.clone())?;
        let mut reached = false;
        for _ in 0..10_000 {
            original.machine_mut().tick();
            let cpu = original.machine().cpu();
            let boundary = if held {
                cpu.next_fetch_addr == 0x00F8_000A
                    && cpu
                        .variant_icache
                        .as_ref()
                        .and_then(|cache| cache.holding_word(0x00F8_000A, true))
                        == Some(0x7E01)
            } else {
                matches!(
                    cpu.state,
                    State::BusCycle {
                        op: MicroOp::FetchIRC,
                        addr: 0x00F8_000A,
                        ..
                    }
                ) && cpu
                    .active_bus_transfer
                    .is_some_and(|transfer| transfer.remaining == TransferSize::Word)
            };
            if boundary {
                reached = true;
                break;
            }
        }
        assert!(reached, "prefetch boundary not reached: holding={held}");
        let bytes = original.snapshot()?;
        assert_eq!(bytes[0], 60);
        let mut restored = AmigaA1200Runtime::new(Model::A1200AgaPal, rom.clone())?;
        restored.restore(&bytes)?;
        assert_eq!(bytes, restored.snapshot()?);
        for _ in 0..1_000 {
            original.machine_mut().tick();
            restored.machine_mut().tick();
            assert_eq!(original.snapshot()?, restored.snapshot()?);
        }
        assert_eq!(original.machine().cpu().regs.d[7], 1);
    }
    Ok(())
}

#[test]
fn both_pending_copper_wait_stages_survive_runtime_restore() -> Result<(), Box<dyn Error>> {
    for initial_idle in [true, false] {
        let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
        {
            let machine = original.machine_mut();
            for (address, value) in [
                (0x1000, 0x0001),
                (0x1002, 0xFFFE),
                (0x1004, 0x0180),
                (0x1006, 0x0F00),
                (0x1008, 0xFFFF),
                (0x100A, 0xFFFE),
                (COP1LCH, 0),
                (COP1LCL, 0x1000),
                (COPJMP1, 0),
                (DMACON, 0x8280),
            ] {
                machine.poke_word(address, value);
            }
        }
        let mut reached = false;
        for _ in 0..1_000 {
            original.machine_mut().tick();
            let copper = original.machine().copper();
            if copper.pending_wait_delay && copper.pending_wait_idle == initial_idle {
                reached = true;
                break;
            }
        }
        assert!(
            reached,
            "pending WAIT stage not reached: idle={initial_idle}"
        );
        assert!(!original.machine().copper().pending_wait_is_skip);
        let bytes = original.snapshot()?;
        let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
        restored.restore(&bytes)?;
        assert_eq!(bytes, restored.snapshot()?);
        for _ in 0..64 {
            original.machine_mut().tick();
            restored.machine_mut().tick();
            assert_eq!(original.snapshot()?, restored.snapshot()?);
        }
        assert_eq!(original.machine().color(0) & 0x0FFF, 0x0F00);
    }
    Ok(())
}

#[test]
fn copper_first_word_survives_runtime_restore_and_later_ram_writes() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    for (address, value) in [
        (0x1000, 0x0180),
        (0x1002, 0x0F00),
        (0x1004, 0xFFFF),
        (0x1006, 0xFFFE),
        (COP1LCH, 0),
        (COP1LCL, 0x1000),
        (COPJMP1, 0),
        (DMACON, 0x8280),
    ] {
        original.machine_mut().poke_word(address, value);
    }
    let mut reached = false;
    for _ in 0..1_000 {
        original.machine_mut().tick();
        if original.machine().copper().cck_phase == 1 {
            reached = true;
            break;
        }
    }
    assert!(reached, "the live driver must service IR1 before saving");
    assert_eq!(original.machine().copper().pc, 0x1002);
    let bytes = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&bytes)?;
    assert_eq!(bytes, restored.snapshot()?);
    for runtime in [&mut original, &mut restored] {
        runtime.machine_mut().poke_word(0x1000, 0x0182);
        runtime.machine_mut().poke_word(0x1002, 0x00F0);
    }
    for _ in 0..64 {
        original.machine_mut().tick();
        restored.machine_mut().tick();
        assert_eq!(original.snapshot()?, restored.snapshot()?);
    }
    assert_eq!(original.machine().color(0) & 0x0FFF, 0x00F0);
    assert_eq!(original.machine().color(1) & 0x0FFF, 0);
    Ok(())
}

#[test]
fn area_channel_fill_holding_and_drain_stages_survive_runtime_restore() -> Result<(), Box<dyn Error>>
{
    use std::collections::BTreeSet;
    // ABCD reaches all four channel stages; ABD fill and D-only fill also
    // exercise trailing idle and the displaced A-hold stage.
    for (mode, fill) in [(15u16, false), (13, true), (1, true)] {
        let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
        {
            let machine = original.machine_mut();
            for (address, value) in [
                (0x1000, 0x8123),
                (0x1002, 0x4567),
                (0x1004, 0x89ab),
                (0x2000, 0xffff),
                (0x2002, 0xffff),
                (0x2004, 0xffff),
                (0x3000, 0x1111),
                (0x3002, 0x2222),
                (0x3004, 0x3333),
                (BLTCON0, 0x3000 | (mode << 8) | 0xca),
                (BLTCON1, if fill { 0x18 } else { 0 }),
                (0x00dff044, 0x0fff),
                (0x00dff046, 0xfff0),
                (0x00dff050, 0),
                (BLTAPTL, 0x1000),
                (0x00dff04c, 0),
                (0x00dff04e, 0x2000),
                (BLTCPTH, 0),
                (BLTCPTL, 0x3000),
                (BLTDPTH, 0),
                (BLTDPTL, 0x4000),
                (DMACON, DMACON_SET_DMA_BLITTER_NASTY),
                (BLTSIZE, 0x0043),
            ] {
                machine.poke_word(address, value);
            }
        }
        let mut visited = BTreeSet::new();
        let mut completed = false;
        for _ in 0..2_000 {
            let state = original.machine().agnus().blitter_diagnostic_snapshot();
            if !state.execution.busy {
                completed = true;
                break;
            }
            let completion = original.machine().agnus().blitter_completion_phase();
            let key = state.area.map_or((u8::MAX, true, completion), |area| {
                (area.phase, area.pipeline_primed, completion)
            });
            if state.execution.startup_ccks_remaining == 0 && visited.insert(key) {
                let bytes = original.snapshot()?;
                assert_eq!(bytes[0], 60);
                let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
                restored.restore(&bytes)?;
                assert_eq!(bytes, restored.snapshot()?);
                for _ in 0..64 {
                    original.machine_mut().tick();
                    restored.machine_mut().tick();
                    assert_eq!(
                        original.snapshot()?,
                        restored.snapshot()?,
                        "mode={mode},fill={fill},stage={key:?}"
                    );
                }
                // Continue the sampling walk from the original boundary so
                // replay verification cannot skip subsequent saved stages.
                original.restore(&bytes)?;
            }
            original.machine_mut().tick();
        }
        assert!(
            completed,
            "area blit never drained: mode={mode},fill={fill}"
        );
        let phases = if mode == 1 { 3 } else { 4 };
        for phase in 0..phases {
            for primed in [false, true] {
                assert!(
                    visited.contains(&(phase, primed, "running")),
                    "unvisited saved stage mode={mode},fill={fill},phase={phase},primed={primed}"
                );
            }
        }
        assert!(visited.iter().any(|key| key.2 == "final-result"));
        assert!(visited.iter().any(|key| key.2 == "final-write"));
    }
    Ok(())
}

#[test]
fn blocked_blitter_wait_and_wake_comparison_survive_restore() -> Result<(), Box<dyn Error>> {
    for wake_pending in [false, true] {
        let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
        {
            let m = original.machine_mut();
            for (address, value) in [
                (0x1000, 1),
                (0x1002, 0x7ffe),
                (0x1004, 0x180),
                (0x1006, 0xf00),
                (0x1008, 0xffff),
                (0x100a, 0xfffe),
                (COP1LCH, 0),
                (COP1LCL, 0x1000),
                (COPJMP1, 0),
                (DMACON, 0x8280),
            ] {
                m.poke_word(address, value);
            }
            // Exercise the live Copper chip directly to isolate the saved
            // boundary from board DMA timing. Runtime replay remains live.
            for h in 0..8 {
                let memory = m.memory().clone();
                assert_eq!(
                    m.copper_mut().tick_cck(&memory, 0, h + 2, h % 2 == 0, true),
                    None
                );
            }
            if wake_pending {
                let memory = m.memory().clone();
                assert_eq!(m.copper_mut().tick_cck(&memory, 0, 12, true, false), None);
            }
        }
        assert_eq!(
            original.machine().copper().wait_blitter_blocked,
            !wake_pending
        );
        assert_eq!(original.machine().copper().pending_wait_delay, wake_pending);
        let bytes = original.snapshot()?;
        let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
        restored.restore(&bytes)?;
        for _ in 0..64 {
            original.machine_mut().tick();
            restored.machine_mut().tick();
            assert_eq!(original.snapshot()?, restored.snapshot()?);
        }
        assert_eq!(original.machine().color(0) & 0x0fff, 0xf00);
    }
    Ok(())
}
