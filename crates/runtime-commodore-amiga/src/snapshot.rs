//! Postcard-encoded snapshot envelope for the Amiga runtime.
//!
//! Splits the snapshot/restore plumbing out of `runtime.rs` so the
//! versioned envelope and the encode/decode machinery have one home.
//! `MachineCore::snapshot` and `MachineCore::restore` are thin
//! delegators that call into [`encode`] and [`decode`].
//!
//! The envelope is generic over `M: AmigaMachine` so it works for
//! any variant. The variant-specific bits (chip-stack snapshot +
//! reconstruction state) is typed as `M::Snapshot`; the runtime-wide bits
//! (canonical configuration, time, frame
//! counters, audio accumulator, inserted DF0 bytes) are common to
//! every variant.

use emu198x_shell::{MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::AmigaConfig;
use crate::runtime::AmigaRuntime;
use crate::variants::AmigaMachine;

// Version 24 replaces the separate model + variant metadata fields with the
// canonical AmigaConfig. It also crosses the ActiveCpu/CpuClock machine
// snapshot boundary, so version-23 positional payloads cannot be resumed.
const SNAPSHOT_VERSION: u32 = 24;

/// Persistable Amiga runtime envelope. Wraps the variant's chip-stack
/// snapshot (`M::Snapshot`) with the surrounding runtime context
/// (canonical configuration, time, frame counters, audio
/// accumulator, and the inserted DF0 bytes for re-mount on restore).
/// Versioned so future snapshot extensions can bump the major version
/// cleanly.
#[derive(Serialize, Deserialize)]
struct SnapshotEnvelopeV24<M: AmigaMachine> {
    version: u32,
    config: AmigaConfig,
    time: MachineTime,
    machine: M::Snapshot,
    floppy0_bytes: Option<Vec<u8>>,
    frame_count: u64,
    non_black_pixels: u32,
    non_white_pixels: u32,
    first_active_row: Option<u32>,
    audio_sample_accumulator: u64,
}

/// Encode a runtime as postcard bytes. Caller-side error type is
/// [`MachineError::InvalidSnapshot`] with the postcard reason.
pub(crate) fn encode<M: AmigaMachine>(runtime: &AmigaRuntime<M>) -> Result<Vec<u8>, MachineError> {
    let envelope = SnapshotEnvelopeV24::<M> {
        version: SNAPSHOT_VERSION,
        config: runtime.config(),
        time: runtime.time_value(),
        machine: runtime.machine().snapshot_state(),
        floppy0_bytes: runtime.floppy0_bytes().map(<[u8]>::to_vec),
        frame_count: runtime.frame_count(),
        non_black_pixels: runtime.non_black_pixels(),
        non_white_pixels: runtime.non_white_pixels(),
        first_active_row: runtime.first_active_row(),
        audio_sample_accumulator: runtime.audio_sample_accumulator(),
    };
    postcard::to_allocvec(&envelope).map_err(|reason| MachineError::InvalidSnapshot {
        reason: reason.to_string(),
    })
}

/// Decode postcard bytes into a runtime. Validates the version and
/// the model identifier; restores the machine state, the inserted
/// disk image, and the time / counters atomically. Clears the per-
/// frame audio drain buffer and refreshes the RGBA framebuffer so the
/// next `run_until` doesn't replay stale data.
pub(crate) fn decode<M: AmigaMachine>(
    runtime: &mut AmigaRuntime<M>,
    bytes: &[u8],
) -> Result<(), MachineError> {
    // Read the leading version varint before deserializing the versioned
    // machine payload. A schema change can otherwise fail inside postcard
    // before the explicit version check gets a chance to explain it.
    let (version, _) = postcard::take_from_bytes::<u32>(bytes).map_err(|reason| {
        MachineError::InvalidSnapshot {
            reason: reason.to_string(),
        }
    })?;
    if version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {version}; expected {SNAPSHOT_VERSION}"),
        });
    }

    let envelope: SnapshotEnvelopeV24<M> =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: reason.to_string(),
        })?;
    debug_assert_eq!(envelope.version, SNAPSHOT_VERSION);
    if envelope.config.model() != runtime.model() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "model mismatch: snapshot was {:?}, runtime is {:?}",
                envelope.config.model(),
                runtime.model()
            ),
        });
    }
    if let Err(error) = crate::runtime::validate_config(envelope.config, runtime.model().chipset())
    {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("invalid Amiga configuration: {error}"),
        });
    }
    let persisted_adf = envelope
        .floppy0_bytes
        .as_ref()
        .map(|bytes| {
            format_commodore_amiga_adf::Adf::from_bytes(bytes.clone()).map_err(|reason| {
                MachineError::InvalidSnapshot {
                    reason: format!("invalid persisted DF0 image: {reason}"),
                }
            })
        })
        .transpose()?;

    // Restore into a fresh candidate. A failed nested-state validation must
    // not pass through the live machine: machine diagnostics and watch state
    // are intentionally outside the persisted snapshot.
    let mut candidate_machine = M::build(runtime.firmware_rom(), envelope.config);
    candidate_machine.restore_snapshot_state(envelope.machine);
    let expected_framebuffer_pixels =
        (M::CHIPSET_FB_WIDTH as usize).saturating_mul(M::CHIPSET_FB_HEIGHT as usize);
    validate_framebuffer_pixels(
        candidate_machine.chipset_framebuffer().len(),
        expected_framebuffer_pixels,
    )?;
    if let Err(reason) = candidate_machine.validate_configuration(envelope.config) {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("machine state does not match Amiga configuration: {reason}"),
        });
    }
    let tick_hz = candidate_machine.cck_hz().saturating_mul(2);
    if tick_hz == 0 || envelope.audio_sample_accumulator >= tick_hz {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "audio sample accumulator {} is outside canonical range 0..{tick_hz}",
                envelope.audio_sample_accumulator
            ),
        });
    }

    if let Some(adf) = persisted_adf {
        candidate_machine.insert_floppy0(adf, envelope.config.model().is_a1000());
    }

    // Every fallible parse and configuration check has completed. Commit the
    // candidate and its infallible runtime metadata as one restore operation.
    runtime.replace_machine(candidate_machine);
    runtime.set_config(envelope.config);
    runtime.set_time(envelope.time);
    runtime.set_floppy0_bytes(envelope.floppy0_bytes);
    runtime.set_frame_count(envelope.frame_count);
    runtime.set_non_black_pixels(envelope.non_black_pixels);
    runtime.set_non_white_pixels(envelope.non_white_pixels);
    runtime.set_first_active_row(envelope.first_active_row);
    runtime.set_audio_sample_accumulator(envelope.audio_sample_accumulator);
    runtime.clear_audio_buffer();
    runtime.reset_audio_filter();
    runtime.refresh_rgba_framebuffer();
    runtime.clear_cpu_trace_after_restore();
    Ok(())
}

fn validate_framebuffer_pixels(actual: usize, expected: usize) -> Result<(), MachineError> {
    if actual != expected {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("chipset framebuffer has {actual} pixels; expected exactly {expected}"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use format_commodore_amiga_adf::{ADF_SIZE_DD, Adf};
    use machine_commodore_amiga_ocs::AmigaOcs;
    use motorola_68000::CpuModel;

    use super::*;
    use crate::{AmigaOcsRuntime, Model};

    #[test]
    fn framebuffer_validation_requires_the_exact_machine_shape() {
        assert!(validate_framebuffer_pixels(768 * 576, 768 * 576).is_ok());
        assert!(validate_framebuffer_pixels(768 * 576 - 1, 768 * 576).is_err());
        assert!(validate_framebuffer_pixels(768 * 576 + 1, 768 * 576).is_err());
    }

    #[test]
    fn restore_rejects_out_of_range_audio_phase_without_mutating_runtime() {
        let source = AmigaOcsRuntime::blank(Model::A500OcsPal);
        let encoded = encode(&source).expect("encode source snapshot");
        let mut envelope: SnapshotEnvelopeV24<AmigaOcs> =
            postcard::from_bytes(&encoded).expect("decode internal envelope");
        envelope.audio_sample_accumulator = u64::MAX;
        let forged = postcard::to_allocvec(&envelope).expect("encode forged audio phase");

        let mut target = AmigaOcsRuntime::blank(Model::A500OcsPal);
        for _ in 0..100 {
            target.tick_traced();
        }
        let before = encode(&target).expect("encode target before failed restore");

        let error =
            decode(&mut target, &forged).expect_err("out-of-range audio phase must be rejected");
        assert!(matches!(error, MachineError::InvalidSnapshot { .. }));
        assert_eq!(
            encode(&target).expect("encode target after failed restore"),
            before,
            "failed audio validation must leave persisted runtime state untouched"
        );
    }

    #[test]
    fn restore_rejects_machine_state_that_disagrees_with_a530_configuration() {
        let stock = AmigaOcsRuntime::blank(Model::A500OcsPal);
        let encoded = encode(&stock).expect("encode stock snapshot");
        let mut envelope: SnapshotEnvelopeV24<AmigaOcs> =
            postcard::from_bytes(&encoded).expect("decode internal envelope");
        envelope.config = Model::A500OcsPalGvpA530.config();
        let forged = postcard::to_allocvec(&envelope).expect("encode forged envelope");

        let mut target = AmigaOcsRuntime::blank(Model::A500OcsPalGvpA530);
        target.machine_mut().poke_word(0x0020_0042, 0xA55A);
        target.cpu_trace_arm(None, 10_000);
        for _ in 0..2_000 {
            target.tick_traced();
        }
        assert!(
            !target.cpu_trace_entries().is_empty(),
            "test setup must capture trace entries"
        );
        target.machine_mut().debug_watch_addr = Some((0x0020_0042, 2));
        target
            .machine_mut()
            .debug_watch_writes
            .push((17, 0x00FC_0000, 0x0020_0042, 0xA55A, true));

        let persisted_before = encode(&target).expect("encode target before failed restore");
        let trace_before = target.cpu_trace_entries().to_vec();
        let watch_range_before = target.machine().debug_watch_addr;
        let watch_writes_before = target.machine().debug_watch_writes.clone();

        let error = decode(&mut target, &forged).expect_err("nested CPU mismatch must be rejected");

        assert!(
            matches!(error, MachineError::InvalidSnapshot { .. }),
            "unexpected restore error: {error:?}"
        );
        assert_eq!(
            encode(&target).expect("encode target after failed restore"),
            persisted_before,
            "failed nested validation must leave persisted runtime state untouched"
        );
        assert_eq!(
            target.cpu_trace_entries(),
            trace_before,
            "failed nested validation must retain observational trace state"
        );
        assert_eq!(
            target.machine().debug_watch_addr,
            watch_range_before,
            "failed nested validation must retain the public watch range"
        );
        assert_eq!(
            target.machine().debug_watch_writes,
            watch_writes_before,
            "failed nested validation must retain public watch diagnostics"
        );
        assert_eq!(
            target.machine().active_cpu().model(),
            CpuModel::M68EC030,
            "failed restore must leave the live machine in place"
        );
        assert!(target.machine().gvp_a530().is_some());
    }

    #[test]
    fn malformed_persisted_media_is_rejected_without_mutating_runtime_or_trace() {
        let source = AmigaOcsRuntime::blank(Model::A500OcsPal);
        let encoded = encode(&source).expect("encode source snapshot");
        let mut envelope: SnapshotEnvelopeV24<AmigaOcs> =
            postcard::from_bytes(&encoded).expect("decode internal envelope");
        envelope.floppy0_bytes = Some(vec![0; 17]);
        let forged = postcard::to_allocvec(&envelope).expect("encode forged envelope");

        let mut target = AmigaOcsRuntime::blank(Model::A500OcsPal);
        let target_media = vec![0; ADF_SIZE_DD];
        let target_adf = Adf::from_bytes(target_media.clone()).expect("decode valid target media");
        target.machine_mut().insert_adf(target_adf);
        target.set_floppy0_bytes(Some(target_media));
        target.cpu_trace_arm(None, 10_000);
        for _ in 0..2_000 {
            target.tick_traced();
        }
        assert!(
            !target.cpu_trace_entries().is_empty(),
            "test setup must capture trace entries"
        );
        let before = encode(&target).expect("encode target before failed restore");
        let trace_before = target.cpu_trace_entries().to_vec();

        let error = decode(&mut target, &forged).expect_err("malformed DF0 must be rejected");

        assert!(matches!(error, MachineError::InvalidSnapshot { .. }));
        assert_eq!(
            encode(&target).expect("encode target after failed restore"),
            before,
            "failed media validation must leave all persisted runtime state untouched"
        );
        assert_eq!(
            target.cpu_trace_entries(),
            trace_before,
            "failed restore must retain observational trace state"
        );
    }

    #[test]
    fn successful_restore_clears_captured_trace_but_keeps_it_armed() {
        let source = AmigaOcsRuntime::blank(Model::A500OcsPal);
        let encoded = encode(&source).expect("encode source snapshot");
        let mut target = AmigaOcsRuntime::blank(Model::A500OcsPal);
        target.cpu_trace_arm(None, 10_000);
        for _ in 0..2_000 {
            target.tick_traced();
        }
        assert!(
            !target.cpu_trace_entries().is_empty(),
            "test setup must capture trace entries"
        );

        decode(&mut target, &encoded).expect("restore valid snapshot");

        assert!(target.cpu_trace_armed());
        assert!(target.cpu_trace_entries().is_empty());
    }

    #[test]
    fn successful_restore_drops_transient_audio_filter_history() {
        let source = AmigaOcsRuntime::blank(Model::A500OcsPal);
        let encoded = encode(&source).expect("encode source snapshot");
        let mut target = AmigaOcsRuntime::blank(Model::A500OcsPal);

        for _ in 0..64 {
            let _ = target.filter_audio_for_test(0.75, -0.5, true);
        }
        decode(&mut target, &encoded).expect("restore valid snapshot");

        let restored = target.filter_audio_for_test(0.25, -0.125, false);
        let mut fresh = AmigaOcsRuntime::blank(Model::A500OcsPal);
        let expected = fresh.filter_audio_for_test(0.25, -0.125, false);
        assert_eq!(
            restored, expected,
            "restore must restart the non-persisted IIR history"
        );
    }
}
