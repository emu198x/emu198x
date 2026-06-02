//! Minimal snapshot envelope for the Sord M5 runtime.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::M5Runtime;

const SNAPSHOT_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
struct M5RuntimeSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    rom_bytes: Option<Vec<u8>>,
    cart_bytes: Option<Vec<u8>>,
}

pub(crate) fn encode(runtime: &M5Runtime) -> Result<Vec<u8>, MachineError> {
    let snapshot = M5RuntimeSnapshotV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id().to_owned(),
        rom_bytes: runtime.rom_bytes().map(<[u8]>::to_vec),
        cart_bytes: runtime.cart_bytes().map(<[u8]>::to_vec),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut M5Runtime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: M5RuntimeSnapshotV1 =
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
    runtime.set_cart_bytes(snapshot.cart_bytes);
    runtime.rebuild_after_restore();
    Ok(())
}
