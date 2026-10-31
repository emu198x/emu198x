//! Postcard-encoded snapshot envelope for the CPC runtime.
//!
//! Serialises the **live machine** — Z80, Gate Array, CRTC, PSG, PPI, RAM,
//! both ROMs and the cassette's position — so a restore resumes exactly rather
//! than cold-booting. Mirrors the Einstein / SG-1000 shape: a borrowing
//! envelope for encode (no clone), an owning envelope for decode.

use crate::machine::AmstradCpc;
use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::AmstradCpcRuntime;

// Bumped to 4: the machine carries the 664 and 6128's µPD765A and drive-motor
// latch.
//
// Bumped to 3: the 8255 began modelling its port directions and input lines, which changes its serialised fields.
//
// Bumped to 2: the 6845 now carries its interlace field (#163). postcard is
// not self-describing, so a field added to the CRTC shifts every byte after
// it.
const SNAPSHOT_VERSION: u16 = 4;

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
    machine: Option<common_amstrad_cpc::AmstradCpc>,
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
    let machine = snapshot
        .machine
        .map(|state| {
            AmstradCpc::from_state(state, runtime.model().machine_model())
                .map_err(|reason| MachineError::InvalidSnapshot { reason })
        })
        .transpose()?;
    runtime.set_time(MachineTime::new(snapshot.time));
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

#[cfg(test)]
mod board_payload_tests {
    use super::*;
    use crate::Model;
    #[test]
    fn matching_header_cannot_hide_a_foreign_board_and_rejection_is_atomic() {
        for selected in [Model::Cpc464, Model::Cpc664, Model::Cpc6128] {
            let mut live =
                AmstradCpcRuntime::new(selected, vec![0x11; 32768]).expect("live firmware");
            let before = live.snapshot().expect("before");
            for foreign in [Model::Cpc464, Model::Cpc664, Model::Cpc6128] {
                if selected == foreign {
                    continue;
                }
                let board = AmstradCpc::with_model(&[0x22; 32768], foreign.machine_model())
                    .expect("foreign");
                let bytes = postcard::to_allocvec(&CpcRuntimeSnapshotRefV1 {
                    version: SNAPSHOT_VERSION,
                    time: 1234,
                    model_id: selected.model_id(),
                    machine: Some(&board),
                })
                .expect("forged envelope");
                assert!(live.restore(&bytes).is_err());
                assert!(live.snapshot().expect("after") == before);
            }
        }
    }
}
