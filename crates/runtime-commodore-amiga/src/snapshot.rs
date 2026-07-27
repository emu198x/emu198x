//! Postcard-encoded snapshot envelope for the Amiga runtime.
//!
//! Splits the snapshot/restore plumbing out of `runtime.rs` so the
//! versioned envelope and the encode/decode machinery have one home.
//! `MachineCore::snapshot` and `MachineCore::restore` are thin
//! delegators that call into [`encode`] and [`decode`].
//!
//! The envelope is generic over `M: AmigaMachine` so it works for
//! any variant. The variant-specific bits (chip-stack snapshot +
//! reconstruction metadata) are typed as `M::Snapshot` and
//! `M::SnapshotMetadata`; the runtime-wide bits (model, time, frame
//! counters, audio accumulator, inserted DF0 bytes) are common to
//! every variant.

use emu198x_shell::{MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::Model;
use crate::runtime::AmigaRuntime;
use crate::variants::AmigaMachine;

// Version 23 rejects version-22 snapshots because MC68020/MC68030 logical data
// transfers now retain their remaining SIZ value, full write operand, partial
// read accumulator and physical bus outputs across split DSACK phases. A
// version-22 positional payload cannot resume midway through one of those
// transfers. The shared envelope version also advances for OCS/ECS models so
// every Amiga runtime has one unambiguous compatibility boundary.
const SNAPSHOT_VERSION: u32 = 23;

/// Persistable Amiga runtime envelope. Wraps the variant's chip-stack
/// snapshot (`M::Snapshot`) and the variant's reconstruction metadata
/// (`M::SnapshotMetadata`, e.g. `RamConfig` for OCS) with the
/// surrounding runtime context (model, time, frame counters, audio
/// accumulator, and the inserted DF0 bytes for re-mount on restore).
/// Versioned so future snapshot extensions can bump the major version
/// cleanly.
#[derive(Serialize, Deserialize)]
struct SnapshotEnvelopeV23<M: AmigaMachine> {
    version: u32,
    model: Model,
    metadata: M::SnapshotMetadata,
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
    let envelope = SnapshotEnvelopeV23::<M> {
        version: SNAPSHOT_VERSION,
        model: runtime.model(),
        metadata: runtime.metadata().clone(),
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

    let envelope: SnapshotEnvelopeV23<M> =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: reason.to_string(),
        })?;
    debug_assert_eq!(envelope.version, SNAPSHOT_VERSION);
    if envelope.model != runtime.model() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "model mismatch: snapshot was {:?}, runtime is {:?}",
                envelope.model,
                runtime.model()
            ),
        });
    }
    runtime.set_metadata(envelope.metadata);
    runtime.set_time(envelope.time);
    runtime
        .machine_mut()
        .restore_snapshot_state(envelope.machine);
    runtime.clear_floppy0_bytes();
    if let Some(bytes) = envelope.floppy0_bytes {
        runtime.insert_floppy_bytes_pub("floppy-0", &bytes)?;
    }
    runtime.set_frame_count(envelope.frame_count);
    runtime.set_non_black_pixels(envelope.non_black_pixels);
    runtime.set_non_white_pixels(envelope.non_white_pixels);
    runtime.set_first_active_row(envelope.first_active_row);
    runtime.set_audio_sample_accumulator(envelope.audio_sample_accumulator);
    runtime.clear_audio_buffer();
    runtime.refresh_rgba_framebuffer();
    Ok(())
}
