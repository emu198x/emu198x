//! Postcard-encoded snapshot envelope for the Super NES.
//!
//! Serialises the live machine, mid-DMA included: the scheduler's frames
//! are part of it, so a restore resumes at the exact master clock. A
//! borrowing envelope for encode, an owning one for decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_nintendo_snes::Snes;
use serde::{Deserialize, Serialize};

use crate::runtime::SnesRuntime;

/// Version 1 is the first. The machine serialises positionally, so any
/// change to its layout needs a bump here.
const SNAPSHOT_VERSION: u16 = 1;

#[derive(Serialize)]
struct SnesSnapshotRefV1<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a Snes>,
}

#[derive(Deserialize)]
struct SnesSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<Snes>,
}

pub(crate) fn encode(runtime: &SnesRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = SnesSnapshotRefV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut SnesRuntime, bytes: &[u8]) -> Result<(), MachineError> {
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
    let snapshot: SnesSnapshotV1 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    debug_assert_eq!(snapshot.version, SNAPSHOT_VERSION);
    if snapshot.model_id != runtime.model_id() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "snapshot model {} does not match runtime model {}",
                snapshot.model_id,
                runtime.model_id()
            ),
        });
    }
    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_machine(snapshot.machine);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SNAPSHOT_VERSION, decode};
    use crate::{Model, blank};
    use emu198x_shell::{MachineCore, MachineError};

    #[test]
    fn decode_rejects_a_future_version_before_the_payload() {
        let mut runtime = blank(Model::SnesNtsc);
        let bytes = postcard::to_allocvec(&(SNAPSHOT_VERSION + 1)).expect("encodes");
        let err = decode(&mut runtime, &bytes).expect_err("rejected");
        assert!(
            matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("unsupported snapshot version")),
            "{err:?}"
        );
        assert!(decode(&mut runtime, &[]).is_err());
    }

    #[test]
    fn decode_rejects_another_models_snapshot() {
        let snapshot = blank(Model::SnesNtsc).snapshot().expect("encode");
        let mut runtime = blank(Model::SnesPal);
        let err = runtime.restore(&snapshot).expect_err("model mismatch");
        assert!(
            matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("does not match")),
            "{err:?}"
        );
    }
}
