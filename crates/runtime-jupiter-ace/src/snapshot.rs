//! Postcard-encoded snapshot envelope for the Jupiter Ace runtime.
//!
//! Serialises the **live machine** (CPU, RAM, video/char RAM, display, and
//! keyboard) so a restore resumes exactly where it left off, rather than the
//! old bootstrap envelope that cold-booted from the ROM. Mirrors the Game Boy /
//! NES snapshot shape: a borrowing envelope for encode (no clone), an owning
//! envelope for decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_jupiter_ace::JupiterAce;
use serde::{Deserialize, Serialize};

use crate::runtime::JupiterAceRuntime;

const SNAPSHOT_VERSION: u16 = 2;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct AceRuntimeSnapshotRefV2<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a JupiterAce>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct AceRuntimeSnapshotV2 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<JupiterAce>,
}

pub(crate) fn encode(runtime: &JupiterAceRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = AceRuntimeSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut JupiterAceRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: AceRuntimeSnapshotV2 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    if snapshot.version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {}", snapshot.version),
        });
    }
    if snapshot.model_id != runtime.model().model_id() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "snapshot model {} does not match runtime model {}",
                snapshot.model_id,
                runtime.model().model_id()
            ),
        });
    }
    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_machine(snapshot.machine);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AceRuntimeSnapshotRefV2, decode};
    use crate::profiles::Model;
    use crate::runtime::JupiterAceRuntime;
    use emu198x_shell::MachineError;

    /// A future-version envelope is rejected before any state is touched.
    #[test]
    fn decode_rejects_unsupported_version() {
        let mut runtime = JupiterAceRuntime::blank(Model::Ace3k);
        let bytes = postcard::to_allocvec(&AceRuntimeSnapshotRefV2 {
            version: 999,
            time: 0,
            model_id: runtime.model().model_id(),
            machine: None,
        })
        .expect("synthetic envelope should encode");

        let err = decode(&mut runtime, &bytes).expect_err("future version should reject");
        match err {
            MachineError::InvalidSnapshot { reason } => {
                assert!(
                    reason.contains("unsupported snapshot version"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected InvalidSnapshot, got {other:?}"),
        }
    }
}
