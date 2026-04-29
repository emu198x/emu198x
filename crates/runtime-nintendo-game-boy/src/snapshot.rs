//! Postcard-encoded snapshot envelope for the Game Boy runtime.
//!
//! Splits the snapshot/restore plumbing out of `runtime.rs` so the
//! versioned envelope and the encode/decode machinery have one home.
//! `MachineCore::snapshot` and `MachineCore::restore` are thin
//! delegators that call into [`encode`] and [`decode`].

use emu198x_shell::{MachineError, MachineTime};
use machine_nintendo_game_boy::GameBoy;
use serde::{Deserialize, Serialize};

use crate::runtime::GameBoyRuntime;

const SNAPSHOT_VERSION: u32 = 1;

/// Borrowing snapshot envelope used during encode. Avoids a clone of
/// the cartridge bytes and the live `GameBoy` machine.
#[derive(Serialize)]
struct GameBoyRuntimeSnapshotRefV1<'a> {
    version: u32,
    profile_id: &'a str,
    time: MachineTime,
    cartridge_bytes: Option<&'a [u8]>,
    machine: Option<&'a GameBoy>,
}

/// Owning snapshot envelope used during decode.
#[derive(Deserialize)]
struct GameBoyRuntimeSnapshotV1 {
    version: u32,
    profile_id: String,
    time: MachineTime,
    cartridge_bytes: Option<Vec<u8>>,
    machine: Option<GameBoy>,
}

/// Encode a runtime as postcard bytes. Caller-side error type is
/// [`MachineError::InvalidSnapshot`] with the postcard reason.
pub(crate) fn encode(runtime: &GameBoyRuntime) -> Result<Vec<u8>, MachineError> {
    postcard::to_allocvec(&GameBoyRuntimeSnapshotRefV1 {
        version: SNAPSHOT_VERSION,
        profile_id: runtime.profile().profile_id.as_str(),
        time: runtime.time_value(),
        cartridge_bytes: runtime.cartridge_bytes(),
        machine: runtime.machine(),
    })
    .map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

/// Decode postcard bytes into a runtime. Validates the version and
/// the profile identifier; restores the machine state, the cartridge
/// bytes, and the time stamp atomically. Clears the per-frame audio
/// drain buffer so the next `run_until` doesn't replay stale samples.
pub(crate) fn decode(runtime: &mut GameBoyRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: GameBoyRuntimeSnapshotV1 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;

    if snapshot.version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {}", snapshot.version),
        });
    }

    if snapshot.profile_id != runtime.profile().profile_id.as_str() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "snapshot profile {} does not match runtime profile {}",
                snapshot.profile_id,
                runtime.profile().profile_id.as_str()
            ),
        });
    }

    runtime.set_machine(snapshot.machine);
    runtime.set_cartridge_bytes(snapshot.cartridge_bytes);
    runtime.set_time(snapshot.time);
    runtime.clear_audio_buffer();
    Ok(())
}
