//! Minimal snapshot envelope for the PET runtime.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::PetRuntime;

const SNAPSHOT_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
struct PetRuntimeSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    kernal_bytes: Option<Vec<u8>>,
    basic_bytes: Option<Vec<u8>>,
    editor_bytes: Option<Vec<u8>>,
    char_bytes: Option<Vec<u8>>,
}

pub(crate) fn encode(runtime: &PetRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = PetRuntimeSnapshotV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id().to_owned(),
        kernal_bytes: runtime.kernal_bytes().map(<[u8]>::to_vec),
        basic_bytes: runtime.basic_bytes().map(<[u8]>::to_vec),
        editor_bytes: runtime.editor_bytes().map(<[u8]>::to_vec),
        char_bytes: runtime.char_bytes().map(<[u8]>::to_vec),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut PetRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: PetRuntimeSnapshotV1 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    if snapshot.version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {}", snapshot.version),
        });
    }
    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_rom_bytes(
        snapshot.kernal_bytes,
        snapshot.basic_bytes,
        snapshot.editor_bytes,
        snapshot.char_bytes,
    );
    runtime.rebuild_after_restore();
    Ok(())
}
