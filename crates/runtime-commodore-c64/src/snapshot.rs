//! Postcard-encoded snapshot envelope for the C64 runtime.
//!
//! Splits the snapshot/restore plumbing out of `runtime.rs` so the
//! versioned envelope and the encode/decode machinery have one home.
//! `MachineCore::snapshot` and `MachineCore::restore` are thin
//! delegators that call into [`encode`] and [`decode`].

use common_commodore_iec::IecBus;
use emu198x_shell::{MachineError, MachineTime};
use machine_commodore_c64::C64Snapshot;
use serde::{Deserialize, Serialize};

use crate::drives::IecDriveSnapshot;
use crate::runtime::C64Runtime;

/// Version 2 moved from the fixed 1541-plus-1581 pair to a per-port array of
/// model-tagged drive snapshots (IEC devices 8–11), so a snapshot records
/// whichever drive the user chose on each port.
const SNAPSHOT_VERSION: u32 = 2;

/// Persistable C64 runtime envelope. Wraps the machine's chip snapshot with the
/// surrounding runtime context (model identifier, time, the live IEC bus state,
/// the per-port drive snapshots, and each port's cycle-accumulator phase).
#[derive(Serialize, Deserialize)]
struct SnapshotEnvelopeV2 {
    version: u32,
    profile_id: String,
    time: MachineTime,
    machine: C64Snapshot,
    drives: [Option<IecDriveSnapshot>; 4],
    drive_cycle_accum: [u64; 4],
    iec_bus: IecBus,
}

/// Encode a runtime as postcard bytes. Caller-side error type is
/// [`MachineError::InvalidSnapshot`] with the postcard reason.
pub(crate) fn encode(runtime: &C64Runtime) -> Result<Vec<u8>, MachineError> {
    postcard::to_allocvec(&SnapshotEnvelopeV2 {
        version: SNAPSHOT_VERSION,
        profile_id: runtime.profile().profile_id.as_str().to_owned(),
        time: runtime.time(),
        machine: runtime.machine().snapshot_state(),
        drives: runtime.drives_snapshot(),
        drive_cycle_accum: runtime.drive_cycle_accum_all(),
        iec_bus: runtime.iec_bus().clone(),
    })
    .map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

/// Decode postcard bytes into a runtime. Validates the version and
/// the profile identifier; restores the machine state, the per-port
/// drives, the IEC bus, and the time stamp atomically.
pub(crate) fn decode(runtime: &mut C64Runtime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: SnapshotEnvelopeV2 =
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
    runtime
        .restore_drives(snapshot.drives)
        .map_err(|reason| MachineError::InvalidSnapshot { reason })?;
    runtime.set_iec_bus(snapshot.iec_bus);
    runtime.set_drive_cycle_accum_all(snapshot.drive_cycle_accum);
    runtime.set_time(snapshot.time);
    Ok(())
}
