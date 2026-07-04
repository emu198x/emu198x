//! Postcard-encoded snapshot envelope for the C64 runtime.
//!
//! Splits the snapshot/restore plumbing out of `runtime.rs` so the
//! versioned envelope and the encode/decode machinery have one home.
//! `MachineCore::snapshot` and `MachineCore::restore` are thin
//! delegators that call into [`encode`] and [`decode`].

use common_commodore_iec::IecBus;
use emu198x_shell::{MachineError, MachineTime};
use machine_commodore_1541::{Drive1541, Drive1541Snapshot};
use machine_commodore_1581::{Drive1581, Drive1581Snapshot};
use machine_commodore_c64::C64Snapshot;
use serde::{Deserialize, Serialize};

use crate::runtime::C64Runtime;

const SNAPSHOT_VERSION: u32 = 1;

/// Persistable C64 runtime envelope. Wraps the machine's chip
/// snapshot with the surrounding runtime context (model identifier,
/// time, the live IEC bus state, the optional 1541 drive snapshot,
/// and the drive's cycle-accumulator phase).
#[derive(Serialize, Deserialize)]
struct SnapshotEnvelopeV1 {
    version: u32,
    profile_id: String,
    time: MachineTime,
    machine: C64Snapshot,
    drive8: Option<Drive1541Snapshot>,
    iec_bus: IecBus,
    drive8_cycle_accum: u64,
    #[serde(default)]
    drive_1581: Option<Drive1581Snapshot>,
    #[serde(default)]
    drive_1581_cycle_accum: u64,
}

/// Encode a runtime as postcard bytes. Caller-side error type is
/// [`MachineError::InvalidSnapshot`] with the postcard reason.
pub(crate) fn encode(runtime: &C64Runtime) -> Result<Vec<u8>, MachineError> {
    postcard::to_allocvec(&SnapshotEnvelopeV1 {
        version: SNAPSHOT_VERSION,
        profile_id: runtime.profile().profile_id.as_str().to_owned(),
        time: runtime.time(),
        machine: runtime.machine().snapshot_state(),
        drive8: runtime.drive8().map(Drive1541::snapshot_state),
        iec_bus: runtime.iec_bus().clone(),
        drive8_cycle_accum: runtime.drive8_cycle_accum(),
        drive_1581: runtime.drive_1581().map(Drive1581::snapshot_state),
        drive_1581_cycle_accum: runtime.drive_1581_cycle_accum(),
    })
    .map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

/// Decode postcard bytes into a runtime. Validates the version and
/// the profile identifier; restores the machine state, the optional
/// drive, the IEC bus, and the time stamp atomically.
pub(crate) fn decode(runtime: &mut C64Runtime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: SnapshotEnvelopeV1 =
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

    runtime
        .machine_mut()
        .restore_snapshot_state(snapshot.machine)
        .map_err(|reason| MachineError::InvalidSnapshot { reason })?;
    let drive = snapshot
        .drive8
        .map(Drive1541::from_snapshot)
        .transpose()
        .map_err(|reason| MachineError::InvalidSnapshot { reason })?;
    runtime.set_drive8(drive);
    let drive_1581 = snapshot
        .drive_1581
        .map(Drive1581::from_snapshot)
        .transpose()
        .map_err(|reason| MachineError::InvalidSnapshot { reason })?;
    runtime.set_drive_1581(drive_1581);
    runtime.set_iec_bus(snapshot.iec_bus);
    runtime.set_drive8_cycle_accum(snapshot.drive8_cycle_accum);
    runtime.set_drive_1581_cycle_accum(snapshot.drive_1581_cycle_accum);
    runtime.set_time(snapshot.time);
    Ok(())
}
