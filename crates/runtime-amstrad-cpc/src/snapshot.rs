//! Postcard-encoded snapshot envelope for the CPC runtime.
//!
//! Serialises the **live machine** — Z80, Gate Array, CRTC, PSG, PPI, RAM,
//! both ROMs and the cassette's position — so a restore resumes exactly rather
//! than cold-booting. Mirrors the Einstein / SG-1000 shape: a borrowing
//! envelope for encode (no clone), an owning envelope for decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_amstrad_cpc::AmstradCpc;
use serde::{Deserialize, Serialize};

use crate::runtime::AmstradCpcRuntime;

const SNAPSHOT_VERSION: u16 = 1;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct CpcRuntimeSnapshotRefV1<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a AmstradCpc>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct CpcRuntimeSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<AmstradCpc>,
}

pub(crate) fn encode(runtime: &AmstradCpcRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = CpcRuntimeSnapshotRefV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut AmstradCpcRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    // Read the version alone first, so a future snapshot is rejected by
    // version rather than by a confusing payload-shape error.
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
    let snapshot: CpcRuntimeSnapshotV1 =
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
        // The Z80's micro-op walker is a function pointer sequence, which
        // cannot survive serialisation; the core rebuilds it from the decoded
        // register state. Without this a restored machine resumes mid-
        // instruction with no walker and stalls.
        machine.cpu_mut().rehydrate_walker_sequence();
    }
    runtime.set_machine(machine);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SNAPSHOT_VERSION, decode};
    use crate::profiles::Model;
    use crate::runtime::AmstradCpcRuntime;
    use emu198x_shell::MachineError;

    #[test]
    fn decode_rejects_a_future_version_before_payload_decode() {
        let mut runtime = AmstradCpcRuntime::blank(Model::Cpc464);
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
}
