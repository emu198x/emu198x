//! Postcard-encoded snapshot envelope for the BBC Micro runtime.
//!
//! Serialises the **live machine** (6502, 6845 CRTC, Video ULA, both 6522 VIAs,
//! SN76489 PSG, 32 KB RAM, ADC, ACIA, latch, ROMs) so a restore resumes
//! exactly, rather than the old bootstrap envelope that cold-booted from the
//! MOS + sideways ROMs. A borrowing envelope for encode (no clone), an owning
//! envelope for decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_acorn_bbc_micro::BbcMicro;
use serde::{Deserialize, Serialize};

use crate::runtime::BbcMicroRuntime;

// Bumped to 4: the machine now carries its frame base and
// current scanline, so instruction stepping crosses line
// boundaries the way running does (#1202). postcard is not
// self-describing, so added fields shift every byte after them.
const SNAPSHOT_VERSION: u16 = 4;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct BbcMicroRuntimeSnapshotRefV2<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a BbcMicro>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct BbcMicroRuntimeSnapshotV2 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<BbcMicro>,
}

pub(crate) fn encode(runtime: &BbcMicroRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = BbcMicroRuntimeSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut BbcMicroRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: BbcMicroRuntimeSnapshotV2 =
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
    use super::{BbcMicroRuntimeSnapshotRefV2, decode};
    use crate::profiles::Model;
    use crate::runtime::BbcMicroRuntime;
    use emu198x_shell::MachineError;

    /// A future-version envelope is rejected before any state is touched.
    #[test]
    fn decode_rejects_unsupported_version() {
        let mut runtime = BbcMicroRuntime::blank(Model::BbcModelB);
        let bytes = postcard::to_allocvec(&BbcMicroRuntimeSnapshotRefV2 {
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
