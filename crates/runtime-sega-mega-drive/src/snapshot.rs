//! Postcard-encoded snapshot envelope for the Mega Drive.
//!
//! Serialises the **live machine** (68000, Z80, VDP, YM2612, PSG, pads,
//! cartridge ROM and both RAMs) so a restore resumes exactly. Mirrors the
//! Master System's shape: a borrowing envelope for encode (no clone), an
//! owning envelope for decode.

use crate::machine::MegaDrive;
use common_sega_mega_drive::MegaDriveState;
use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::MegaDriveRuntime;

/// Version 1 was the first; version 2 adds the Power Base Converter to the
/// machine and Mode 4 to the VDP. The machine serialises positionally, so
/// any change to its layout needs a bump here.
const SNAPSHOT_VERSION: u16 = 2;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct MegaDriveSnapshotRefV1<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a MegaDrive>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct MegaDriveSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<MegaDriveState>,
}

pub(crate) fn encode(runtime: &MegaDriveRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = MegaDriveSnapshotRefV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut MegaDriveRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let (version, _) = postcard::take_from_bytes::<u16>(bytes).map_err(|reason| {
        MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        }
    })?;
    if version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {version}; expected {SNAPSHOT_VERSION}"),
        });
    }
    let snapshot: MegaDriveSnapshotV1 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    debug_assert_eq!(snapshot.version, SNAPSHOT_VERSION);
    if snapshot.model_id != runtime.model_id() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "snapshot model {} does not match runtime model {}",
                snapshot.model_id,
                runtime.model_id()
            ),
        });
    }
    let mut machine = snapshot
        .machine
        .map(|state| MegaDrive::from_state(state, runtime.model()))
        .transpose()
        .map_err(|reason| MachineError::InvalidSnapshot { reason })?;
    if let Some(machine) = &mut machine {
        machine.finish_restore();
    }
    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_machine(machine);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SNAPSHOT_VERSION, decode};
    use crate::{Model, blank};
    use emu198x_shell::{MachineCore, MachineError};

    #[test]
    fn decode_rejects_future_version_before_payload_decode() {
        let mut runtime = blank(Model::Genesis);
        let future_version = SNAPSHOT_VERSION + 1;
        let bytes = postcard::to_allocvec(&future_version).expect("future version should encode");

        let err = decode(&mut runtime, &bytes).expect_err("future version should reject");
        match err {
            MachineError::InvalidSnapshot { reason } => {
                assert!(
                    reason.contains(&format!("unsupported snapshot version {future_version}")),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected InvalidSnapshot, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_another_models_snapshot() {
        let snapshot = blank(Model::Genesis).snapshot().expect("encode");
        let mut runtime = blank(Model::MegaDriveEurope);
        let err = runtime.restore(&snapshot).expect_err("model mismatch");
        assert!(
            matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("does not match")),
            "{err:?}"
        );
    }
}
