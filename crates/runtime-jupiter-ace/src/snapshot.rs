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

const SNAPSHOT_VERSION: u16 = 4;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct AceRuntimeSnapshotRefV4<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a JupiterAce>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct AceRuntimeSnapshotV4 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<JupiterAce>,
}

pub(crate) fn encode(runtime: &JupiterAceRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = AceRuntimeSnapshotRefV4 {
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
    let (version, _) = postcard::take_from_bytes::<u16>(bytes).map_err(|reason| {
        MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        }
    })?;
    if version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {version}; expected {SNAPSHOT_VERSION}"),
        });
    }
    let snapshot: AceRuntimeSnapshotV4 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    debug_assert_eq!(snapshot.version, SNAPSHOT_VERSION);
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
    let mut machine = snapshot.machine;
    if let Some(machine) = &mut machine {
        machine.cpu_mut().rehydrate_walker_sequence();
    }
    runtime.set_machine(machine);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SNAPSHOT_VERSION, decode};
    use crate::profiles::Model;
    use crate::runtime::JupiterAceRuntime;
    use emu198x_shell::MachineError;

    #[test]
    fn decode_rejects_future_version_before_payload_decode() {
        let mut runtime = JupiterAceRuntime::blank(Model::Ace3k);
        let future_version = SNAPSHOT_VERSION + 1;
        let bytes = postcard::to_allocvec(&future_version).expect("future version should encode");

        let err = decode(&mut runtime, &bytes).expect_err("future version should reject");
        match err {
            MachineError::InvalidSnapshot { reason } => {
                assert!(
                    reason.contains(&format!("unsupported snapshot version {future_version}")),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected InvalidSnapshot, got {other:?}"),
        }
    }

    /// Version 2 cannot preserve the accepted Z80 interrupt sequence identity.
    #[test]
    fn decode_rejects_version_2_before_payload_decode() {
        let mut runtime = JupiterAceRuntime::blank(Model::Ace3k);
        let bytes = postcard::to_allocvec(&2_u16).expect("legacy version should encode");

        let err = decode(&mut runtime, &bytes).expect_err("version 2 should reject");
        match err {
            MachineError::InvalidSnapshot { reason } => {
                assert!(
                    reason.contains("unsupported snapshot version 2"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected InvalidSnapshot, got {other:?}"),
        }
    }

    /// Version 3 has no partial-window beeper integration state, so accepting
    /// it would introduce an audio discontinuity at the restore boundary.
    #[test]
    fn decode_rejects_version_3_before_payload_decode() {
        let mut runtime = JupiterAceRuntime::blank(Model::Ace3k);
        let bytes = postcard::to_allocvec(&3_u16).expect("legacy version should encode");

        let err = decode(&mut runtime, &bytes).expect_err("version 3 should reject");
        match err {
            MachineError::InvalidSnapshot { reason } => {
                assert!(
                    reason.contains("unsupported snapshot version 3"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected InvalidSnapshot, got {other:?}"),
        }
    }
}
