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

/// Version 6 adds source-resolved VIC-II badline BA, sprite BA and c-access
/// state, plus the pending `$D011` write phase and far-edge badline DMA-window
/// marker. These make an arbitrary-cycle restore retain the exact externally
/// inspectable bus phase rather than recomputing it from the following cycle.
///
/// Version 5 adds the VIC-II's live BA-to-AEC delay counter. Without it, a
/// restore during the three-cycle bus handover could make the next matrix or
/// sprite access valid at the wrong Phi2 phase.
///
/// Version 4 makes arbitrary mid-frame snapshots lossless by including the
/// live VIC-II sprite fetch/draw pipeline and SID samples queued since the last
/// frame boundary. Those fields were omitted from earlier machine snapshots.
///
/// Version 3 adds the runtime-level expansion bookkeeping — the inserted
/// cartridge image and the GeoRAM/REU sizes and 1351-mouse port — so a restored
/// snapshot re-attaches them on the next reset (previously those fields
/// defaulted to `None`, so a reset dropped the cartridge and expansions).
///
/// Version 2 moved from the fixed 1541-plus-1581 pair to a per-port array of
/// model-tagged drive snapshots (IEC devices 8–11), so a snapshot records
/// whichever drive the user chose on each port.
const SNAPSHOT_VERSION: u32 = 6;

/// Persistable C64 runtime envelope. Wraps the machine's chip snapshot with the
/// surrounding runtime context (model identifier, time, the live IEC bus state,
/// the per-port drive snapshots, each port's cycle-accumulator phase, and the
/// expansion bookkeeping a reset rebuilds from).
#[derive(Serialize, Deserialize)]
struct SnapshotEnvelopeV6 {
    version: u32,
    profile_id: String,
    time: MachineTime,
    machine: C64Snapshot,
    drives: [Option<IecDriveSnapshot>; 4],
    drive_cycle_accum: [u64; 4],
    iec_bus: IecBus,
    cartridge_image: Option<Vec<u8>>,
    georam_kb: Option<usize>,
    reu_kb: Option<usize>,
    mouse_1351_port: Option<u8>,
}

/// Encode a runtime as postcard bytes. Caller-side error type is
/// [`MachineError::InvalidSnapshot`] with the postcard reason.
pub(crate) fn encode(runtime: &C64Runtime) -> Result<Vec<u8>, MachineError> {
    postcard::to_allocvec(&SnapshotEnvelopeV6 {
        version: SNAPSHOT_VERSION,
        profile_id: runtime.profile().profile_id.as_str().to_owned(),
        time: runtime.time(),
        machine: runtime.machine().snapshot_state(),
        drives: runtime.drives_snapshot(),
        drive_cycle_accum: runtime.drive_cycle_accum_all(),
        iec_bus: runtime.iec_bus().clone(),
        cartridge_image: runtime.cartridge_image_bytes().map(<[u8]>::to_vec),
        georam_kb: runtime.georam_kb(),
        reu_kb: runtime.reu_kb(),
        mouse_1351_port: runtime.mouse_1351_port(),
    })
    .map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

/// Decode postcard bytes into a runtime. Validates the version and
/// the profile identifier; restores the machine state, the per-port
/// drives, the IEC bus, and the time stamp atomically.
pub(crate) fn decode(runtime: &mut C64Runtime, bytes: &[u8]) -> Result<(), MachineError> {
    // Read the leading version varint before deserialising the versioned
    // payload. Nested VIC/SID schema changes can otherwise fail inside
    // postcard before the explicit version check explains the incompatibility.
    let (version, _) = postcard::take_from_bytes::<u32>(bytes).map_err(|reason| {
        MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        }
    })?;
    if version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {version}; expected {SNAPSHOT_VERSION}"),
        });
    }

    let snapshot: SnapshotEnvelopeV6 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    debug_assert_eq!(snapshot.version, SNAPSHOT_VERSION);

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
    runtime.restore_expansions(
        snapshot.cartridge_image,
        snapshot.georam_kb,
        snapshot.reu_kb,
        snapshot.mouse_1351_port,
    );
    runtime.set_time(snapshot.time);
    Ok(())
}
