//! Minimal snapshot envelope for the Aquarius runtime.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::AquariusRuntime;

const SNAPSHOT_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
struct AquariusRuntimeSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    bios_bytes: Option<Vec<u8>>,
    cart_bytes: Option<Vec<u8>>,
    expansion_kb: usize,
}

pub(crate) fn encode(runtime: &AquariusRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = AquariusRuntimeSnapshotV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id().to_owned(),
        bios_bytes: runtime.bios_bytes().map(<[u8]>::to_vec),
        cart_bytes: runtime.cart_bytes().map(<[u8]>::to_vec),
        expansion_kb: runtime.expansion_kb(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut AquariusRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: AquariusRuntimeSnapshotV1 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    if snapshot.version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {}", snapshot.version),
        });
    }
    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_bios_bytes(snapshot.bios_bytes);
    runtime.set_cart_bytes(snapshot.cart_bytes);
    runtime.set_expansion_kb_internal(snapshot.expansion_kb);
    runtime.rebuild_after_restore();
    Ok(())
}
