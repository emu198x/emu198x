//! Minimal snapshot envelope for the Einstein runtime.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::EinsteinRuntime;

const SNAPSHOT_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
struct EinsteinRuntimeSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    rom_bytes: Option<Vec<u8>>,
}

pub(crate) fn encode(runtime: &EinsteinRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = EinsteinRuntimeSnapshotV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id().to_owned(),
        rom_bytes: runtime.rom_bytes().map(<[u8]>::to_vec),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut EinsteinRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: EinsteinRuntimeSnapshotV1 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    if snapshot.version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {}", snapshot.version),
        });
    }
    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_rom_bytes(snapshot.rom_bytes);
    runtime.rebuild_after_restore();
    Ok(())
}
