//! Postcard-encoded snapshot envelope for the Commodore PET runtime.
//!
//! Serialises the **live machine** (6502, 6845 CRTC, 6520 PIA, 6522 VIA,
//! 32 KB RAM, 2 KB video RAM, keyboard matrix, ROMs) so a restore resumes
//! exactly, rather than the old bootstrap envelope that cold-booted from the
//! ROM set. A borrowing envelope for encode (no clone), an owning envelope
//! for decode.

use common_commodore_pet::Pet;
use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::PetRuntime;

// Bumped to 3: the 6845 now carries its interlace field (#163). postcard is
// not self-describing, so a field added to the CRTC shifts every byte after
// it.
// Version 4 preserves the VIA timer 2 load phase (#1677).
// Version 5 fixes board/video identity, queued audio and pending PRG fidelity.
const SNAPSHOT_VERSION: u16 = 5;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct PetRuntimeSnapshotRefV5<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a Pet>,
    pending_prg: Option<&'a [u8]>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct PetRuntimeSnapshotV5 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<Pet>,
    pending_prg: Option<Vec<u8>>,
}

pub(crate) fn encode(runtime: &PetRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = PetRuntimeSnapshotRefV5 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine().map(|machine| machine.state()),
        pending_prg: runtime.pending_prg(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut PetRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    // Reject an incompatible layout before postcard reads its chip state.
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
    let snapshot: PetRuntimeSnapshotV5 =
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
        .map(|state| crate::machine::Pet::from_state(runtime.model(), state))
        .transpose()
        .map_err(|reason| MachineError::InvalidSnapshot { reason })?;
    if machine
        .as_ref()
        .is_some_and(|m| m.master_clock() != snapshot.time)
        || snapshot
            .pending_prg
            .as_ref()
            .is_some_and(|prg| prg.len() < 3)
    {
        return Err(MachineError::InvalidSnapshot {
            reason: "inconsistent PET time or pending program".into(),
        });
    }
    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_pending_prg(snapshot.pending_prg);
    runtime.set_machine(machine);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PetRuntimeSnapshotRefV5, decode};
    use crate::profiles::Model;
    use crate::runtime::PetRuntime;
    use emu198x_shell::MachineError;

    /// A future-version envelope is rejected before any state is touched.
    #[test]
    fn decode_rejects_unsupported_version() {
        let mut runtime = PetRuntime::blank(Model::Pet40Col);
        let bytes = postcard::to_allocvec(&PetRuntimeSnapshotRefV5 {
            version: 999,
            time: 0,
            model_id: runtime.model().model_id(),
            machine: None,
            pending_prg: None,
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

    #[test]
    fn decode_rejects_old_schema_before_reading_payload() {
        let mut runtime = PetRuntime::blank(Model::Pet40Col);
        let before = super::encode(&runtime).expect("snapshot");
        // Deliberately no payload: full deserialisation would fail first.
        let bytes = postcard::to_allocvec(&(super::SNAPSHOT_VERSION - 1)).expect("version");
        let err = decode(&mut runtime, &bytes).expect_err("old schema must reject");
        assert!(matches!(err, MachineError::InvalidSnapshot { reason }
            if reason == format!("unsupported snapshot version {}; expected {}",
                super::SNAPSHOT_VERSION - 1, super::SNAPSHOT_VERSION)));
        assert_eq!(super::encode(&runtime).expect("unchanged snapshot"), before);
    }
    #[test]
    fn malformed_or_foreign_payload_does_not_mutate_live_runtime() {
        use common_commodore_pet::{Board, Pet};
        use emu198x_shell::{MachineCore, MachineTime};
        let mut runtime = PetRuntime::new(
            Model::Pet40Col,
            vec![0; 4096],
            vec![0; 12288],
            vec![0; 2048],
            vec![0; 2048],
        )
        .expect("firmware");
        runtime.set_pending_prg(Some(vec![0, 2, 0x42]));
        let before = runtime.snapshot().expect("before");
        for (board, kernal_size, time) in [
            (Board::Pet8032, 4096, 0),
            (Board::Pet4032, 2048, 0),
            (Board::Pet4032, 4096, 1),
        ] {
            let raw = Pet::new(
                vec![0; kernal_size],
                vec![0; 12288],
                vec![0; 2048],
                vec![0; 2048],
                board,
            );
            let bytes = postcard::to_allocvec(&PetRuntimeSnapshotRefV5 {
                version: super::SNAPSHOT_VERSION,
                time,
                model_id: Model::Pet40Col.model_id(),
                machine: Some(&raw),
                pending_prg: None,
            })
            .expect("forged");
            assert!(decode(&mut runtime, &bytes).is_err());
            assert_eq!(runtime.snapshot().expect("unchanged"), before);
        }
        assert!(decode(&mut runtime, &before[..before.len() - 1]).is_err());
        assert_eq!(runtime.time(), MachineTime::default());
        assert_eq!(runtime.snapshot().expect("unchanged"), before);
    }
}
