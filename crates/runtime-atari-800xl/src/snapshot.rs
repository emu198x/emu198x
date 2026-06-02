//! Minimal snapshot envelope for the Atari 800XL runtime.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::Atari800xlRuntime;

const SNAPSHOT_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
struct Atari800xlRuntimeSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    os_bytes: Option<Vec<u8>>,
    basic_bytes: Option<Vec<u8>>,
    cart_bytes: Option<Vec<u8>>,
    basic_enabled: bool,
}

pub(crate) fn encode(runtime: &Atari800xlRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = Atari800xlRuntimeSnapshotV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id().to_owned(),
        os_bytes: runtime.os_bytes().map(<[u8]>::to_vec),
        basic_bytes: runtime.basic_bytes().map(<[u8]>::to_vec),
        cart_bytes: runtime.cart_bytes().map(<[u8]>::to_vec),
        basic_enabled: runtime.basic_enabled(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut Atari800xlRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: Atari800xlRuntimeSnapshotV1 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    if snapshot.version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {}", snapshot.version),
        });
    }
    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_state(
        snapshot.os_bytes,
        snapshot.basic_bytes,
        snapshot.cart_bytes,
        snapshot.basic_enabled,
    );
    runtime.rebuild_after_restore()
}
