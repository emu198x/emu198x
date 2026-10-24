//! Postcard-encoded snapshot envelope for the Oric runtime.
//!
//! Serialises the **live machine** (6502, VIA 6522, AY-3-8910 PSG, ROM, RAM,
//! and framebuffer) so a restore resumes exactly, rather than the old bootstrap
//! envelope that cold-booted from the ROM. Mirrors the SG-1000 / Game Boy
//! shape: a borrowing envelope for encode (no clone), an owning envelope for
//! decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_oric_atmos::OricAtmos;
use serde::{Deserialize, Serialize};

use crate::runtime::OricRuntime;

// Bumped to 6: the machine now carries the expansion port's ROMDIS and MAP
// lines. postcard is not self-describing, so added fields shift every byte
// after them.
// Version 7 preserves the VIA timer 2 load phase (#1677).
// Version 8 adds the optional 6551 serial card and its crystal remainder
// (#1303).
const SNAPSHOT_VERSION: u16 = 8;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct OricRuntimeSnapshotRefV2<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a OricAtmos>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct OricRuntimeSnapshotV2 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<OricAtmos>,
}

pub(crate) fn encode(runtime: &OricRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = OricRuntimeSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut OricRuntime, bytes: &[u8]) -> Result<(), MachineError> {
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
    let snapshot: OricRuntimeSnapshotV2 =
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
    use super::{OricRuntimeSnapshotRefV2, decode};
    use crate::profiles::Model;
    use crate::runtime::OricRuntime;
    use emu198x_shell::MachineError;

    /// A future-version envelope is rejected before any state is touched.
    #[test]
    fn decode_rejects_unsupported_version() {
        let mut runtime = OricRuntime::blank(Model::Atmos);
        let bytes = postcard::to_allocvec(&OricRuntimeSnapshotRefV2 {
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

    #[test]
    fn decode_rejects_old_schema_before_reading_payload() {
        let mut runtime = OricRuntime::blank(Model::Atmos);
        let before = super::encode(&runtime).expect("snapshot");
        // Deliberately no payload: full deserialisation would fail first.
        let bytes = postcard::to_allocvec(&(super::SNAPSHOT_VERSION - 1)).expect("version");
        let err = decode(&mut runtime, &bytes).expect_err("old schema must reject");
        assert!(matches!(err, MachineError::InvalidSnapshot { reason }
            if reason == format!("unsupported snapshot version {}; expected {}",
                super::SNAPSHOT_VERSION - 1, super::SNAPSHOT_VERSION)));
        assert_eq!(super::encode(&runtime).expect("unchanged snapshot"), before);
    }

    /// The serial card and its registers come back with the machine, and the
    /// runtime's fitting follows the restored machine.
    ///
    /// The live machine holds 64 KB of RAM inline, and a debug build keeps
    /// several copies on the stack across encode and decode, so this runs on
    /// a thread with room for them.
    #[test]
    fn the_serial_card_round_trips() {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(serial_card_round_trip)
            .expect("spawn")
            .join()
            .expect("round trip");
    }

    fn serial_card_round_trip() {
        let mut rom = vec![0xEA; 0x4000];
        rom[0x3FFD] = 0xC0;
        let mut runtime = OricRuntime::new(Model::Atmos, rom.clone()).expect("runtime");
        runtime.set_serial_card(true);
        runtime.machine_mut().expect("machine").poke(0x031E, 0x0B);
        let bytes = super::encode(&runtime).expect("snapshot");

        let mut restored = OricRuntime::new(Model::Atmos, rom).expect("runtime");
        decode(&mut restored, &bytes).expect("restore");
        assert!(restored.serial_card());
        let command = restored
            .machine()
            .and_then(machine_oric_atmos::OricAtmos::acia_registers)
            .map(|registers| registers.command);
        assert_eq!(command, Some(0x0B));
    }
}
