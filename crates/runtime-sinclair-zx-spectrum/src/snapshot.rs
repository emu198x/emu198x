//! Postcard-encoded snapshot envelope for the generic Spectrum runtime.
//!
//! Splits the snapshot/restore plumbing out of `runtime.rs` so the
//! versioned envelope and the encode/decode machinery have one home.
//! `MachineCore::snapshot` and `MachineCore::restore` are thin
//! delegators that call into [`encode`] and [`decode`].
//!
//! The envelope is generic over the wrapped machine `M`: `M: Serialize`
//! threads through the borrowed reference encoder, and a separate
//! owned form (with `M: Deserialize<'de>`) drives the decoder.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::{SpectrumMachine, SpectrumRuntime};

const SNAPSHOT_VERSION: u32 = 1;

/// Borrowed snapshot envelope used by [`encode`]. Generic over any
/// `M: Serialize`; the `SpectrumMachine` bound is enforced at the
/// `encode`/`decode` entry points so the envelope stays usable from
/// pure-serde contexts.
#[derive(Serialize)]
struct SpectrumRuntimeSnapshotRefV1<'a, M: Serialize> {
    version: u32,
    profile_id: &'a str,
    time: MachineTime,
    keyboard_rows: [u8; 8],
    machine: &'a M,
}

/// Owned snapshot envelope used by [`decode`]. Generic over any
/// `M: Deserialize<'de>`.
#[derive(Deserialize)]
struct SpectrumRuntimeSnapshotV1<M> {
    version: u32,
    profile_id: String,
    time: MachineTime,
    keyboard_rows: [u8; 8],
    machine: M,
}

/// Encode a runtime as postcard bytes. Caller-side error type is
/// [`MachineError::InvalidSnapshot`] with the postcard reason.
pub(crate) fn encode<M: SpectrumMachine>(
    runtime: &SpectrumRuntime<M>,
) -> Result<Vec<u8>, MachineError> {
    postcard::to_allocvec(&SpectrumRuntimeSnapshotRefV1 {
        version: SNAPSHOT_VERSION,
        profile_id: runtime.profile().profile_id.as_str(),
        time: runtime.time_value(),
        keyboard_rows: *runtime.keyboard_rows(),
        machine: runtime.machine(),
    })
    .map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

/// Decode postcard bytes into a runtime. Validates the version and
/// the profile identifier; restores the machine state, the keyboard
/// matrix, and the time stamp atomically. After restoring the matrix
/// the cached rows are pushed back into the machine so its scan
/// register matches the runtime cache.
pub(crate) fn decode<M: SpectrumMachine>(
    runtime: &mut SpectrumRuntime<M>,
    bytes: &[u8],
) -> Result<(), MachineError> {
    let snapshot: SpectrumRuntimeSnapshotV1<M> =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;

    if snapshot.version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {}", snapshot.version),
        });
    }

    if snapshot.profile_id != runtime.profile().profile_id.as_str() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "snapshot profile {} does not match runtime profile {}",
                snapshot.profile_id,
                runtime.profile().profile_id.as_str()
            ),
        });
    }

    runtime.set_machine(snapshot.machine);
    runtime.set_time(snapshot.time);
    runtime.set_keyboard_rows(snapshot.keyboard_rows);
    let rows = *runtime.keyboard_rows();
    runtime.machine_mut().set_keyboard_rows(&rows);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Model;
    use crate::Spectrum48kRuntime;
    use crate::variants::{Spectrum128kRuntime, SpectrumPlusRuntime};
    use emu198x_shell::MachineCore;
    use machine_sinclair_zx_spectrum_128k::Spectrum128K;
    use machine_sinclair_zx_spectrum_plus::{Model as PlusModel, SpectrumPlus};

    fn synthetic_envelope(profile_id: &str, version: u32) -> Vec<u8> {
        // Hand-rolled v1 envelope around a default-constructed 128K so we
        // can flip individual fields (version, profile_id) and exercise
        // the corresponding decode error paths without needing to touch
        // the real envelope schema.
        let machine = Spectrum128K::new();
        postcard::to_allocvec(&SpectrumRuntimeSnapshotRefV1 {
            version,
            profile_id,
            time: MachineTime::default(),
            keyboard_rows: [0xFF; 8],
            machine: &machine,
        })
        .expect("synthetic envelope should encode")
    }

    #[test]
    fn decode_rejects_completely_corrupt_bytes() {
        let mut runtime = Spectrum48kRuntime::blank();
        let err = runtime
            .restore(&[0xFFu8; 8])
            .expect_err("corrupt postcard bytes must not decode");
        assert!(matches!(err, MachineError::InvalidSnapshot { .. }));
    }

    #[test]
    fn decode_rejects_unsupported_snapshot_version() {
        let bytes = synthetic_envelope(Model::Spectrum128KPal.profile_id(), SNAPSHOT_VERSION + 1);
        let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
        let err = runtime
            .restore(&bytes)
            .expect_err("future snapshot versions must be rejected");
        match err {
            MachineError::InvalidSnapshot { reason } => {
                assert!(
                    reason.contains("unsupported snapshot version"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected InvalidSnapshot, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_profile_mismatch_within_generic_envelope() {
        // Profile-id mismatch covers the generic decode arm without
        // needing two physically-distinct machine types.
        let bytes = synthetic_envelope("not-a-real-profile", SNAPSHOT_VERSION);
        let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
        let err = runtime
            .restore(&bytes)
            .expect_err("profile-id mismatch must be rejected");
        match err {
            MachineError::InvalidSnapshot { reason } => {
                assert!(
                    reason.contains("snapshot profile") && reason.contains("does not match"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected InvalidSnapshot, got {other:?}"),
        }
    }

    #[test]
    fn encode_and_decode_round_trip_on_plus_runtime() {
        // Round-trips through the generic envelope on a non-128K, non-48K
        // variant so the whole serde-generic path stays exercised even
        // when the Plus3-specific runtime is constructed.
        let runtime =
            SpectrumPlusRuntime::new(Model::SpectrumPlus3, SpectrumPlus::new(PlusModel::Plus3));
        let bytes = runtime.snapshot().expect("Plus3 snapshot should encode");

        let mut restored =
            SpectrumPlusRuntime::new(Model::SpectrumPlus3, SpectrumPlus::new(PlusModel::Plus3));
        restored
            .restore(&bytes)
            .expect("Plus3 snapshot should round-trip through the generic envelope");
    }
}
