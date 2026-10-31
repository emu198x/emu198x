//! Postcard-encoded snapshot envelope for the Atari ST.
//!
//! Serialises the **live machine** (68000, GLUE/MMU, Shifter, MFP, ACIAs,
//! YM2149, floppy controller and disks, DMA chip, IKBD and RAM) so a restore
//! resumes exactly. TOS is firmware and stays out: the restoring runtime
//! supplies its own, which must be the image the snapshot was taken with
//! for the restore to mean anything.

use crate::machine::AtariSt;
use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::AtariStRuntime;

/// Version 1 is the first. The machine serialises positionally, so any
/// change to its layout needs a bump here.
const SNAPSHOT_VERSION: u16 = 1;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct AtariStSnapshotRefV1<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a AtariSt>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct AtariStSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<common_atari_st::AtariSt>,
}

pub(crate) fn encode(runtime: &AtariStRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = AtariStSnapshotRefV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut AtariStRuntime, bytes: &[u8]) -> Result<(), MachineError> {
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
    let snapshot: AtariStSnapshotV1 =
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
    let Some(machine) = snapshot.machine else {
        return Err(MachineError::InvalidSnapshot {
            reason: "the snapshot holds no machine".to_owned(),
        });
    };
    runtime.set_restored_machine(machine)?;
    runtime.set_time(MachineTime::new(snapshot.time));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SNAPSHOT_VERSION, decode};
    use crate::{Model, blank};
    use emu198x_shell::{MachineCore, MachineError};

    #[test]
    fn decode_rejects_future_version_before_payload_decode() {
        let mut runtime = blank(Model::St520Pal);
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

    #[test]
    fn decode_rejects_another_models_snapshot_and_an_empty_one() {
        let snapshot = blank(Model::St520Pal).snapshot().expect("encode");
        let mut runtime = blank(Model::St1040Pal);
        let err = runtime.restore(&snapshot).expect_err("model mismatch");
        assert!(
            matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("does not match")),
            "{err:?}"
        );
        let mut runtime = blank(Model::St520Pal);
        let err = runtime.restore(&snapshot).expect_err("no machine");
        assert!(
            matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("no machine")),
            "{err:?}"
        );
    }
}

#[cfg(test)]
mod board_payload_tests {
    use super::*;
    use crate::Model;
    #[test]
    fn foreign_hardware_under_matching_header_is_rejected_without_mutation() {
        for selected in Model::ALL {
            let mut runtime = AtariStRuntime::new(selected, vec![0; 0x30000]).expect("TOS");
            let before = runtime.snapshot().expect("before");
            for foreign in Model::ALL {
                if selected == foreign {
                    continue;
                }
                let machine = AtariSt::new(
                    foreign.machine_model(),
                    foreign.standard(),
                    common_atari_st::Monitor::Colour,
                    vec![0; 0x30000],
                )
                .expect("foreign board");
                let data = postcard::to_allocvec(&AtariStSnapshotRefV1 {
                    version: SNAPSHOT_VERSION,
                    time: 1234,
                    model_id: selected.model_id(),
                    machine: Some(&machine),
                })
                .expect("forged snapshot");
                assert!(runtime.restore(&data).is_err());
                assert!(runtime.snapshot().expect("after") == before);
            }
        }
    }
}
