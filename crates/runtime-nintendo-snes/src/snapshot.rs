//! Postcard-encoded snapshot envelope for the Super NES.
//!
//! Serialises the live machine, mid-DMA included: the scheduler's frames
//! are part of it, so a restore resumes at the exact master clock. A
//! borrowing envelope for encode, an owning one for decode.

use common_nintendo_snes::Snes;
use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::SnesRuntime;

/// V2 retains the IPL even without a cartridge. V1 can be recovered exactly
/// only when its machine carries the firmware bytes.
const SNAPSHOT_VERSION: u16 = 2;

#[derive(Serialize)]
struct SnesSnapshotRefV2<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    ipl: Option<&'a [u8]>,
    machine: Option<&'a Snes>,
}

#[derive(Deserialize)]
struct SnesSnapshotV2 {
    version: u16,
    time: u64,
    model_id: String,
    ipl: Option<Vec<u8>>,
    machine: Option<Snes>,
}

#[derive(Deserialize)]
struct SnesSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<Snes>,
}

pub(crate) fn encode(runtime: &SnesRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = SnesSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model_id(),
        ipl: runtime.snapshot_ipl(),
        machine: runtime.machine().map(|m| m.state()),
    };
    postcard::to_allocvec(&snapshot).map_err(invalid_decode)
}

fn invalid_decode(reason: impl std::fmt::Display) -> MachineError {
    MachineError::InvalidSnapshot {
        reason: format!("snapshot codec: {reason}"),
    }
}

pub(crate) fn decode(runtime: &mut SnesRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let (version, _) = postcard::take_from_bytes::<u16>(bytes).map_err(invalid_decode)?;
    let (time, model_id, machine, ipl) = match version {
        1 => {
            let saved: SnesSnapshotV1 = postcard::from_bytes(bytes).map_err(invalid_decode)?;
            debug_assert_eq!(saved.version, 1);
            let Some(machine) = saved.machine else {
                return Err(invalid_decode(
                    "blank v1 saves omit the IPL and cannot be restored exactly",
                ));
            };
            let ipl = machine.apu.ipl.clone();
            (saved.time, saved.model_id, Some(machine), Some(ipl))
        }
        SNAPSHOT_VERSION => {
            let saved: SnesSnapshotV2 = postcard::from_bytes(bytes).map_err(invalid_decode)?;
            debug_assert_eq!(saved.version, SNAPSHOT_VERSION);
            (saved.time, saved.model_id, saved.machine, saved.ipl)
        }
        _ => {
            return Err(MachineError::InvalidSnapshot {
                reason: format!(
                    "unsupported snapshot version {version}; expected 1 or {SNAPSHOT_VERSION}"
                ),
            });
        }
    };
    if model_id != runtime.model_id() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "snapshot model {model_id} does not match runtime model {}",
                runtime.model_id()
            ),
        });
    }
    let ipl: Option<[u8; 64]> = ipl
        .map(|rom| {
            rom.try_into().map_err(|rom: Vec<u8>| {
                invalid_decode(format!("IPL must contain 64 bytes, found {}", rom.len()))
            })
        })
        .transpose()?;
    if let Some(machine) = &machine {
        machine
            .validate_board(runtime.model().machine_region())
            .map_err(invalid_decode)?;
        let expected = ipl
            .as_ref()
            .unwrap_or(&common_nintendo_snes::ipl::CLEAN_ROOM);
        if machine.apu.ipl.as_slice() != expected {
            return Err(invalid_decode("envelope IPL does not match machine IPL"));
        }
    }
    let machine = machine
        .map(|state| crate::SnesMachine::from_state(runtime.model(), state))
        .transpose()
        .map_err(invalid_decode)?;
    // Everything above is fallible; installation is atomic after validation.
    runtime.set_time(MachineTime::new(time));
    runtime.set_machine(machine);
    runtime.set_snapshot_ipl(ipl);
    Ok(())
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
