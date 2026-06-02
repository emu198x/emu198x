//! Minimal snapshot envelope for the ZX81 runtime.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::Zx81Runtime;

const SNAPSHOT_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
struct Zx81RuntimeSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    rom_bytes: Option<Vec<u8>>,
    ram_bytes: usize,
}

pub(crate) fn encode(runtime: &Zx81Runtime) -> Result<Vec<u8>, MachineError> {
    let snapshot = Zx81RuntimeSnapshotV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id().to_owned(),
        rom_bytes: runtime.rom_bytes().map(<[u8]>::to_vec),
        ram_bytes: runtime.ram_bytes(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut Zx81Runtime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: Zx81RuntimeSnapshotV1 =
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
    runtime.set_ram_bytes_internal(snapshot.ram_bytes);
    runtime.rebuild_after_restore()
}
