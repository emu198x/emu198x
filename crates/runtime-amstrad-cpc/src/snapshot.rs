//! Postcard-encoded snapshot envelope for the CPC runtime.
//!
//! Serialises the **live machine** — Z80, Gate Array, CRTC, PSG, PPI, RAM,
//! both ROMs and the cassette's position — so a restore resumes exactly rather
//! than cold-booting. Mirrors the Einstein / SG-1000 shape: a borrowing
//! envelope for encode (no clone), an owning envelope for decode.

use crate::machine::AmstradCpc;
use common_amstrad_cpc::MonitorScan;
use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::AmstradCpcRuntime;

// CPU 0.9 samples I/O WAIT after its automatic TW, changing execution of
// complete v4/v5 CPC saves despite the unchanged CPU field layout (#2284).
// Approved policy: v6 only; reject older execution before decoding its payload.
const SNAPSHOT_VERSION: u16 = 6;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct CpcRuntimeSnapshotRefV2<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a AmstradCpc>,
    monitor: Option<MonitorScan>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct CpcRuntimeSnapshotV2 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<common_amstrad_cpc::AmstradCpc>,
    monitor: Option<MonitorScan>,
}

pub(crate) fn encode(runtime: &AmstradCpcRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = CpcRuntimeSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
        monitor: runtime.machine().map(AmstradCpc::monitor_scan),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut AmstradCpcRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    // Read the version alone first, so incompatible execution is rejected by
    // version rather than by a confusing payload-shape error.
    let (version, _) = postcard::take_from_bytes::<u16>(bytes).map_err(|reason| {
        MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        }
    })?;
    let undecodable = |reason: postcard::Error| MachineError::InvalidSnapshot {
        reason: format!("decode failed: {reason}"),
    };
    if version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {version}; expected {SNAPSHOT_VERSION}"),
        });
    }
    let snapshot = postcard::from_bytes::<CpcRuntimeSnapshotV2>(bytes).map_err(undecodable)?;
    debug_assert_eq!(snapshot.version, version);
    if snapshot.model_id != runtime.model().model_id() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "snapshot model {} does not match runtime model {}",
                snapshot.model_id,
                runtime.model().model_id()
            ),
        });
    }
    let mut machine = snapshot
        .machine
        .map(|state| {
            AmstradCpc::from_state(state, runtime.model().machine_model())
                .map_err(|reason| MachineError::InvalidSnapshot { reason })
        })
        .transpose()?;
    // Without a saved scan the monitor's phase is unknown, and it re-locks at
    // the next vertical sync.
    if let (Some(machine), Some(scan)) = (machine.as_mut(), snapshot.monitor) {
        machine.set_monitor_scan(scan);
    }
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
                let bytes = postcard::to_allocvec(&CpcRuntimeSnapshotRefV2 {
                    version: SNAPSHOT_VERSION,
                    time: 1234,
                    model_id: selected.model_id(),
                    machine: Some(&board),
                    monitor: None,
                })
                .expect("forged envelope");
                assert!(live.restore(&bytes).is_err());
                assert!(live.snapshot().expect("after") == before);
            }
        }
    }
}
