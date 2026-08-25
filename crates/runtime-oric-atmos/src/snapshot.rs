//! Postcard-encoded snapshot envelope for the Oric runtime.
//!
//! Serialises the **live machine** (6502, VIA 6522, AY-3-8910 PSG, ROM, RAM,
//! and framebuffer) so a restore resumes exactly, rather than the old bootstrap
//! envelope that cold-booted from the ROM. Mirrors the SG-1000 / Game Boy
//! shape: a borrowing envelope for encode (no clone), an owning envelope for
//! decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_oric_atmos::OricAtmos;
use serde::{Deserialize, Serialize};

use crate::runtime::OricRuntime;

// Bumped to 3: the machine now carries its frame base and
// current scanline, so instruction stepping crosses line
// boundaries the way running does (#1202). postcard is not
// self-describing, so added fields shift every byte after them.
const SNAPSHOT_VERSION: u16 = 3;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct OricRuntimeSnapshotRefV2<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a OricAtmos>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct OricRuntimeSnapshotV2 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<OricAtmos>,
}

pub(crate) fn encode(runtime: &OricRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = OricRuntimeSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut OricRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: OricRuntimeSnapshotV2 =
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
    use super::{OricRuntimeSnapshotRefV2, decode};
    use crate::profiles::Model;
    use crate::runtime::OricRuntime;
    use emu198x_shell::MachineError;

    /// A future-version envelope is rejected before any state is touched.
    #[test]
    fn decode_rejects_unsupported_version() {
        let mut runtime = OricRuntime::blank(Model::Atmos);
        let bytes = postcard::to_allocvec(&OricRuntimeSnapshotRefV2 {
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
