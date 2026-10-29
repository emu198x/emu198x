//! Postcard-encoded snapshot envelope for the Atari 2600 runtime.
//!
//! Serialises the **live machine** (6507, TIA, RIOT, and cartridge) so a restore
//! resumes exactly, rather than the old bootstrap envelope that cold-booted from
//! the cart ROM. A borrowing envelope for encode (no clone), an owning envelope
//! for decode.

use crate::machine::Atari2600Machine as Atari2600;
use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::Atari2600Runtime;

const SNAPSHOT_VERSION: u16 = 2;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct Atari2600RuntimeSnapshotRefV2<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a Atari2600>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct Atari2600RuntimeSnapshotV2 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<common_atari_2600::Atari2600State>,
}

pub(crate) fn encode(runtime: &Atari2600Runtime) -> Result<Vec<u8>, MachineError> {
    let snapshot = Atari2600RuntimeSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut Atari2600Runtime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: Atari2600RuntimeSnapshotV2 =
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
    let machine = snapshot
        .machine
        .map(|state| Atari2600::from_state(runtime.model(), state))
        .transpose()
        .map_err(|reason| MachineError::InvalidSnapshot { reason })?;
    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_machine(machine);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Atari2600RuntimeSnapshotRefV2, decode};
    use crate::profiles::Model;
    use crate::runtime::Atari2600Runtime;
    use emu198x_shell::MachineError;

    /// A future-version envelope is rejected before any state is touched.
    #[test]
    fn decode_rejects_unsupported_version() {
        let mut runtime = Atari2600Runtime::blank(Model::Vcs2600Ntsc);
        let bytes = postcard::to_allocvec(&Atari2600RuntimeSnapshotRefV2 {
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

#[cfg(test)]
mod board_identity_tests {
    use super::*;
    use crate::profiles::Model;
    #[test]
    fn foreign_payload_under_matching_header_is_rejected_atomically() {
        let rom =
            include_bytes!("../../../test-data/synthetic-cartridges/atari-2600-logo.bin").to_vec();
        for (selected, foreign) in [
            (Model::Vcs2600Ntsc, Model::Vcs2600Pal),
            (Model::Vcs2600Pal, Model::Vcs2600Ntsc),
        ] {
            let mut receiver = Atari2600Runtime::new(selected, rom.clone()).expect("receiver");
            let before = receiver.snapshot().expect("before");
            let donor = Atari2600Runtime::new(foreign, rom.clone()).expect("donor");
            let forged = postcard::to_allocvec(&Atari2600RuntimeSnapshotRefV2 {
                version: SNAPSHOT_VERSION,
                time: 12345,
                model_id: selected.model_id(),
                machine: donor.machine(),
            })
            .expect("envelope");
            assert!(receiver.restore(&forged).is_err());
            assert_eq!(receiver.snapshot().expect("after"), before);
        }
    }
}
