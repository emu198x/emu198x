//! Postcard-encoded snapshot envelope for the Atari 7800 runtime.
//!
//! Serialises the **live machine** (6502C "Sally", MARIA, RIOT, TIA audio, and
//! cartridge) so a restore resumes exactly, rather than the old bootstrap
//! envelope that cold-booted from the cart ROM. A borrowing envelope for encode
//! (no clone), an owning envelope for decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_atari_7800::Atari7800;
use serde::{Deserialize, Serialize};

use crate::runtime::Atari7800Runtime;

/// Bumped to 3 when the framebuffer became region-sized. A snapshot carries
/// the live chip, framebuffer included, so a version-2 NTSC snapshot holds a
/// 288-line buffer that a version-3 NTSC machine would never allocate.
/// Restoring it would resume into a geometry the machine disagrees with, and
/// silently — so the version check rejects it instead.
/// Version 5 adds POKEY RANDOM initialisation and restart phase.
const SNAPSHOT_VERSION: u16 = 5;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct Atari7800RuntimeSnapshotRefV2<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a Atari7800>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct Atari7800RuntimeSnapshotV2 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<Atari7800>,
}

pub(crate) fn encode(runtime: &Atari7800Runtime) -> Result<Vec<u8>, MachineError> {
    let snapshot = Atari7800RuntimeSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut Atari7800Runtime, bytes: &[u8]) -> Result<(), MachineError> {
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
    let snapshot: Atari7800RuntimeSnapshotV2 =
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
    runtime.set_machine(snapshot.machine);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Atari7800RuntimeSnapshotRefV2, decode};
    use crate::profiles::Model;
    use crate::runtime::Atari7800Runtime;
    use emu198x_shell::MachineError;

    #[test]
    fn old_version_is_rejected_before_decoding_machine_fields() {
        let mut runtime = Atari7800Runtime::blank(Model::A7800Ntsc);
        let bytes = postcard::to_allocvec(&4u16).expect("version prefix");
        let err = decode(&mut runtime, &bytes).expect_err("old layout");
        assert!(
            matches!(err, MachineError::InvalidSnapshot { reason } if reason.contains("unsupported snapshot version 4"))
        );
    }

    /// A future-version envelope is rejected before any state is touched.
    #[test]
    fn decode_rejects_unsupported_version() {
        let mut runtime = Atari7800Runtime::blank(Model::A7800Ntsc);
        let bytes = postcard::to_allocvec(&Atari7800RuntimeSnapshotRefV2 {
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
