//! Postcard-encoded snapshot envelope for the MSX1 runtime.
//!
//! Serialises the **live machine** (Z80, TMS9918 VDP, AY-3-8910 PSG, 8255 PPI,
//! ROM, RAM, and slot/bank state) so a restore resumes exactly, rather than the
//! old bootstrap envelope that cold-booted from the BIOS/cart. Mirrors the
//! SVI-328 / SG-1000 / Game Boy shape: a borrowing envelope for encode (no
//! clone), an owning envelope for decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_msx::Msx;
use serde::{Deserialize, Serialize};

use crate::runtime::MsxRuntime;

/// Bumped to 5 when the machine gained the live M1 wait-state latch. Restoring
/// an older snapshot without that latch could insert the wait twice or omit it
/// when resuming in the middle of an opcode fetch.
const SNAPSHOT_VERSION: u16 = 5;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct MsxRuntimeSnapshotRefV5<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a Msx>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct MsxRuntimeSnapshotV5 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<Msx>,
}

pub(crate) fn encode(runtime: &MsxRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = MsxRuntimeSnapshotRefV5 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut MsxRuntime, bytes: &[u8]) -> Result<(), MachineError> {
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
    let snapshot: MsxRuntimeSnapshotV5 =
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
        machine.cpu_mut().rehydrate_walker_sequence();
    }
    runtime.set_machine(machine);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SNAPSHOT_VERSION, decode};
    use crate::profiles::Model;
    use crate::runtime::MsxRuntime;
    use emu198x_shell::MachineError;

    #[test]
    fn decode_rejects_future_version_before_payload_decode() {
        let mut runtime = MsxRuntime::blank(Model::Msx1Ntsc);
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

    /// Version 2 cannot preserve the accepted Z80 interrupt sequence identity.
    #[test]
    fn decode_rejects_version_2_before_payload_decode() {
        let mut runtime = MsxRuntime::blank(Model::Msx1Ntsc);
        let bytes = postcard::to_allocvec(&2_u16).expect("legacy version should encode");

        let err = decode(&mut runtime, &bytes).expect_err("version 2 should reject");
        match err {
            MachineError::InvalidSnapshot { reason } => {
                assert!(
                    reason.contains("unsupported snapshot version 2"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected InvalidSnapshot, got {other:?}"),
        }
    }
}
