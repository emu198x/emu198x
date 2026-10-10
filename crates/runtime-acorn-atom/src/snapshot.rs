//! Postcard-encoded snapshot envelope for the Acorn Atom runtime.
//!
//! Serialises the **live machine** (6502, Intel 8255 PPI, MC6847 VDG, RAM, and
//! video RAM) so a restore resumes exactly, rather than the old bootstrap
//! envelope that cold-booted from the ROM. Mirrors the SG-1000 / Game Boy shape:
//! a borrowing envelope for encode (no clone), an owning envelope for decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_acorn_atom::AcornAtom;
use serde::{Deserialize, Serialize};

use crate::runtime::AtomRuntime;

// Version 3 preserves the VIA timer 2 load phase (#1677).
const SNAPSHOT_VERSION: u16 = 3;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct AtomRuntimeSnapshotRefV2<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a AcornAtom>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct AtomRuntimeSnapshotV2 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<AcornAtom>,
}

pub(crate) fn encode(runtime: &AtomRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = AtomRuntimeSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut AtomRuntime, bytes: &[u8]) -> Result<(), MachineError> {
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
    let snapshot: AtomRuntimeSnapshotV2 =
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
    use super::{AtomRuntimeSnapshotRefV2, decode};
    use crate::profiles::Model;
    use crate::runtime::AtomRuntime;
    use emu198x_shell::MachineError;

    #[test]
    fn restore_preserves_in_flight_via_strobes_and_future_printer_bytes() {
        use machine_acorn_atom::AcornAtom;

        for mode in [0x08, 0x0A] {
            for cut in 0..16 {
                let mut rom = vec![0xEA; 0x6000];
                let program = [
                    0xA9, 0xFF, 0x8D, 0x03, 0xB8, // DDRA
                    0xA9, mode, 0x8D, 0x0C, 0xB8, // handshake or pulse
                    0xA9, b'H', 0x8D, 0x01, 0xB8, // normal write
                    0xAD, 0x01, 0xB8, // normal read
                    0xAD, 0x0F, 0xB8, // alternate read
                    0xA9, b'I', 0x8D, 0x0F, 0xB8, // alternate write
                    0xA9, b'J', 0x8D, 0x01, 0xB8, // normal write
                    0x4C, 0x1F, 0xD0, // loop at $D01F
                ];
                rom[0x3000..0x3000 + program.len()].copy_from_slice(&program);
                rom[0x5FFC..0x5FFE].copy_from_slice(&0xD000u16.to_le_bytes());
                let mut original = AtomRuntime::blank(Model::AtomBase);
                original.set_machine(Some(AcornAtom::new(rom, 0x0A00)));
                let machine = original.machine_mut().expect("synthetic machine");
                for _ in 0..cut {
                    machine.step_instruction();
                }
                // Printer history is a consumed host output, not snapshot data.
                let mut printed = machine.take_printer_output();
                let saved = super::encode(&original).expect("snapshot pending strobe");
                let mut restored = AtomRuntime::blank(Model::AtomBase);
                decode(&mut restored, &saved).expect("restore pending strobe");
                assert_eq!(super::encode(&restored).expect("fixed point"), saved);
                for _ in 0..20 {
                    original.machine_mut().expect("machine").step_instruction();
                    restored
                        .machine_mut()
                        .expect("restored machine")
                        .step_instruction();
                    let expected = original
                        .machine_mut()
                        .expect("machine")
                        .take_printer_output();
                    let actual = restored
                        .machine_mut()
                        .expect("restored machine")
                        .take_printer_output();
                    assert_eq!(actual, expected, "mode={mode:02x}, cut={cut}");
                    printed.extend(expected);
                    assert_eq!(
                        super::encode(&restored).expect("restored state"),
                        super::encode(&original).expect("continuous state")
                    );
                }
                // No CA1 acknowledgement is supplied: handshake mode holds
                // its first strobe low; pulse mode generates three edges.
                let expected: &[u8] = if mode == 0x08 { b"H" } else { b"HHJ" };
                assert_eq!(printed, expected, "mode={mode:02x}, cut={cut}");
            }
        }
    }

    /// A future-version envelope is rejected before any state is touched.
    #[test]
    fn decode_rejects_unsupported_version() {
        let mut runtime = AtomRuntime::blank(Model::AtomBase);
        let bytes = postcard::to_allocvec(&AtomRuntimeSnapshotRefV2 {
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
        let mut runtime = AtomRuntime::blank(Model::AtomBase);
        let before = super::encode(&runtime).expect("snapshot");
        // Deliberately no payload: full deserialisation would fail first.
        let bytes = postcard::to_allocvec(&(super::SNAPSHOT_VERSION - 1)).expect("version");
        let err = decode(&mut runtime, &bytes).expect_err("old schema must reject");
        assert!(matches!(err, MachineError::InvalidSnapshot { reason }
            if reason == format!("unsupported snapshot version {}; expected {}",
                super::SNAPSHOT_VERSION - 1, super::SNAPSHOT_VERSION)));
        assert_eq!(super::encode(&runtime).expect("unchanged snapshot"), before);
    }
}
