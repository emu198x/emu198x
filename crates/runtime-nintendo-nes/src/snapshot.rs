//! Postcard-encoded snapshot envelope for the NES runtime.
//!
//! Splits the snapshot/restore plumbing out of `runtime.rs` so the
//! versioned envelope and the encode/decode machinery have one home.
//! `MachineCore::snapshot` and `MachineCore::restore` are thin
//! delegators that call into [`encode`] and [`decode`].

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_nintendo_nes::{Nes, NesSnapshot};
use serde::{Deserialize, Serialize};

use crate::runtime::NesRuntime;

const SNAPSHOT_VERSION: u16 = 1;

/// Persistable NES runtime envelope. Wraps the machine's chip
/// snapshot (when present) with the surrounding runtime context
/// (time, cartridge mapper id, cartridge bytes for replay).
#[derive(Serialize)]
struct NesRuntimeSnapshotRefV1<'a> {
    version: u16,
    time: u64,
    cartridge_mapper: Option<u16>,
    cartridge_bytes: Option<&'a [u8]>,
    machine: Option<NesSnapshot>,
}

#[derive(Deserialize)]
struct NesRuntimeSnapshotV1 {
    version: u16,
    time: u64,
    cartridge_mapper: Option<u16>,
    cartridge_bytes: Option<Vec<u8>>,
    machine: Option<NesSnapshot>,
}

/// Encode a runtime as postcard bytes. Caller-side error type is
/// [`MachineError::InvalidSnapshot`] with the postcard reason.
pub(crate) fn encode(runtime: &NesRuntime) -> Result<Vec<u8>, MachineError> {
    postcard::to_allocvec(&NesRuntimeSnapshotRefV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        cartridge_mapper: runtime.cartridge_mapper(),
        cartridge_bytes: runtime.cartridge_bytes(),
        machine: runtime.machine().map(Nes::snapshot),
    })
    .map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

/// Decode postcard bytes into a runtime. Validates the version and
/// restores the machine state, the cartridge bytes / mapper, and the
/// time stamp atomically.
pub(crate) fn decode(runtime: &mut NesRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: NesRuntimeSnapshotV1 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;

    if snapshot.version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported NES snapshot version {}", snapshot.version),
        });
    }

    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_cartridge_bytes(snapshot.cartridge_bytes);
    runtime.set_cartridge_mapper(snapshot.cartridge_mapper);
    runtime.set_machine(snapshot.machine.map(Nes::from_snapshot));
    runtime.refresh_rgba_framebuffer();
    Ok(())
}
