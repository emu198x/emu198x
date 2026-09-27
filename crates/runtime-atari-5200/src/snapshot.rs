//! Postcard-encoded snapshot envelope for the Atari 5200 runtime.
//!
//! Serialises the **live machine** (6502 + ANTIC + GTIA + POKEY + cart ROM +
//! 16 KB RAM + the DMA shadow) so a restore resumes exactly, rather than the
//! old bootstrap envelope that cold-booted from the cart. Mirrors the SG-1000 /
//! 800XL shape: a borrowing envelope for encode (no clone), an owning envelope
//! for decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_atari_5200::Atari5200;
use serde::{Deserialize, Serialize};

use crate::runtime::Atari5200Runtime;

/// Bumped to 3 when the framebuffer became region-sized. A snapshot carries
/// the live chip, framebuffer included, so a version-2 NTSC snapshot holds a
/// 288-line buffer that a version-3 NTSC machine would never allocate.
/// Restoring it would resume into a geometry the machine disagrees with, and
/// silently — so the version check rejects it instead.
///
/// Bumped to 4 when the cartridge gained its layout field. Postcard is not
/// self-describing, so a version-3 payload decodes into a version-4 struct by
/// misreading whatever follows the ROM — the same silent-resume failure.
///
/// Bumped to 5 when GTIA gained its PAL register. Postcard is not
/// self-describing, so a version-4 payload decodes by misreading the
/// chip state that follows.
/// Version 6 adds GTIA sprite shift registers and divider phases.
/// Version 7 adds active sprite registers and pending propagation.
/// Version 8 adds display-list bus bytes and phantom DMA capture state.
/// Version 9 adds GTIA hires admission and its delayed PRIOR latch.
/// Version 10 adds the pending WSYNC assertion pipeline.
/// Version 11 adds ANTIC interrupt-source and enable-sampling state.
const SNAPSHOT_VERSION: u16 = 11;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct Atari5200RuntimeSnapshotRefV2<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a Atari5200>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct Atari5200RuntimeSnapshotV2 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<Atari5200>,
}

pub(crate) fn encode(runtime: &Atari5200Runtime) -> Result<Vec<u8>, MachineError> {
    let snapshot = Atari5200RuntimeSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut Atari5200Runtime, bytes: &[u8]) -> Result<(), MachineError> {
    // Check the envelope before decoding positional machine fields. An older
    // payload must not be interpreted using the new GTIA layout.
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
    let snapshot: Atari5200RuntimeSnapshotV2 =
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
    use super::{Atari5200RuntimeSnapshotRefV2, decode};
    use crate::profiles::Model;
    use crate::runtime::Atari5200Runtime;
    use emu198x_shell::MachineError;

    #[test]
    fn old_layout_is_rejected_before_decoding_its_body() {
        let mut runtime = Atari5200Runtime::blank(Model::A5200Ntsc);
        let bytes = postcard::to_allocvec(&10u16).expect("old version prefix");
        let error = decode(&mut runtime, &bytes).expect_err("old format must reject");
        assert!(
            matches!(error, MachineError::InvalidSnapshot { reason } if reason.contains("unsupported snapshot version 10"))
        );
    }

    /// A future-version envelope is rejected before any state is touched.
    #[test]
    fn decode_rejects_unsupported_version() {
        let mut runtime = Atari5200Runtime::blank(Model::A5200Ntsc);
        let bytes = postcard::to_allocvec(&Atari5200RuntimeSnapshotRefV2 {
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
