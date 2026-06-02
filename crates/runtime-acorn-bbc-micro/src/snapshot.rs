//! Minimal snapshot envelope for the BBC Micro runtime.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::BbcMicroRuntime;

const SNAPSHOT_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
struct BbcMicroRuntimeSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    mos_bytes: Option<Vec<u8>>,
    sideways_roms: Vec<(usize, Vec<u8>)>,
}

pub(crate) fn encode(runtime: &BbcMicroRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = BbcMicroRuntimeSnapshotV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id().to_owned(),
        mos_bytes: runtime.mos_bytes().map(<[u8]>::to_vec),
        sideways_roms: runtime.sideways_roms().to_vec(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut BbcMicroRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: BbcMicroRuntimeSnapshotV1 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    if snapshot.version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {}", snapshot.version),
        });
    }
    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_mos_bytes(snapshot.mos_bytes);
    runtime.set_sideways_roms(snapshot.sideways_roms);
    runtime.rebuild_after_restore();
    Ok(())
}
