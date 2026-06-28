//! Postcard-encoded snapshot envelope for the ZX80 runtime.
//!
//! Serialises the **live machine** (Z80, ULA, ROM/RAM, keyboard) so a restore
//! resumes exactly, rather than the old bootstrap envelope that cold-booted
//! from the ROM. Mirrors the ZX81 shape: a borrowing envelope for encode (no
//! clone), an owning envelope for decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_sinclair_zx80::Zx80;
use serde::{Deserialize, Serialize};

use crate::runtime::Zx80Runtime;

const SNAPSHOT_VERSION: u16 = 2;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct Zx80RuntimeSnapshotRefV2<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a Zx80>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct Zx80RuntimeSnapshotV2 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<Zx80>,
}

pub(crate) fn encode(runtime: &Zx80Runtime) -> Result<Vec<u8>, MachineError> {
    let snapshot = Zx80RuntimeSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut Zx80Runtime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: Zx80RuntimeSnapshotV2 =
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
    use super::{Zx80RuntimeSnapshotRefV2, decode};
    use crate::profiles::Model;
    use crate::runtime::Zx80Runtime;
    use emu198x_shell::MachineError;

    /// A future-version envelope is rejected before any state is touched.
    #[test]
    fn decode_rejects_unsupported_version() {
        let mut runtime = Zx80Runtime::blank(Model::Zx80);
        let bytes = postcard::to_allocvec(&Zx80RuntimeSnapshotRefV2 {
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
