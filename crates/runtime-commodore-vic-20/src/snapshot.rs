//! Postcard-encoded snapshot envelope for the VIC-20 runtime.
//!
//! Serialises the **live machine** (6502, both 6522 VIAs, VIC-I, keyboard, all
//! RAM banks, colour RAM, and the ROMs) so a restore resumes exactly, rather
//! than the old bootstrap envelope that cold-booted from the ROMs. Mirrors the
//! SG-1000 shape: a borrowing envelope for encode (no clone), an owning
//! envelope for decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_commodore_vic_20::Vic20;
use serde::{Deserialize, Serialize};

use crate::runtime::Vic20Runtime;

/// Version 3 made the framebuffer region-sized. Version 4 retains the original
/// cartridge container alongside the live mapped machine, so a reset after
/// restore can reinsert the same cartridge before the KERNAL cold-start probe.
/// Version 5 replaced the machine's composite RAM-expansion size with the
/// per-cartridge block set (#1363), which changes the serialised machine.
/// Version 6 adds the VIC-I's frame-latched row count, and version 7 its
/// line-latched column count (#362).
/// Version 8 preserves the VIA timer 2 load phase (#1677).
const SNAPSHOT_VERSION: u16 = 8;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct Vic20RuntimeSnapshotRefV2<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a Vic20>,
    cartridge_image: Option<&'a [u8]>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct Vic20RuntimeSnapshotV2 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<Vic20>,
    cartridge_image: Option<Vec<u8>>,
}

pub(crate) fn encode(runtime: &Vic20Runtime) -> Result<Vec<u8>, MachineError> {
    let snapshot = Vic20RuntimeSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id(),
        machine: runtime.machine(),
        cartridge_image: runtime.cartridge_image(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut Vic20Runtime, bytes: &[u8]) -> Result<(), MachineError> {
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
    let snapshot: Vic20RuntimeSnapshotV2 =
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
    runtime.set_cartridge_image(snapshot.cartridge_image);
    runtime.set_machine(snapshot.machine);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Vic20RuntimeSnapshotRefV2, decode};
    use crate::profiles::Model;
    use crate::runtime::Vic20Runtime;
    use emu198x_shell::MachineError;

    /// A future-version envelope is rejected before any state is touched.
    #[test]
    fn decode_rejects_unsupported_version() {
        let mut runtime = Vic20Runtime::blank(Model::Vic20Ntsc);
        let bytes = postcard::to_allocvec(&Vic20RuntimeSnapshotRefV2 {
            version: 999,
            time: 0,
            model_id: runtime.model().model_id(),
            machine: None,
            cartridge_image: None,
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
        let mut runtime = Vic20Runtime::blank(Model::Vic20Ntsc);
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
    fn restore_preserves_timer2_load_underflow_and_countdown_in_both_vias() {
        use emu198x_shell::MachineCore;
        use machine_commodore_vic_20::{Vic20, Vic20Model, Vic20RamExpansion};

        for (model, machine_model) in [
            (Model::Vic20Pal, Vic20Model::Pal),
            (Model::Vic20Ntsc, Vic20Model::Ntsc),
        ] {
            for initial in [0_u16, 1, 3, 0xFEFE] {
                for elapsed_instructions in 0..3 {
                    let mut kernal = vec![0xEA; 8192];
                    kernal[..3].copy_from_slice(&[0x4C, 0x00, 0xE0]); // JMP $E000
                    kernal[0x1FFC..0x1FFE].copy_from_slice(&[0x00, 0xE0]);
                    let mut machine = Vic20::new(
                        kernal,
                        vec![0; 8192],
                        vec![0; 4096],
                        machine_model,
                        Vic20RamExpansion::NONE,
                    );
                    machine.run_frame();
                    for base in [0x9110, 0x9120] {
                        machine.poke(base + 8, initial as u8);
                        machine.poke(base + 9, (initial >> 8) as u8);
                    }
                    let start = machine.master_clock();
                    for _ in 0..elapsed_instructions {
                        machine.step_instruction();
                    }
                    let mut original = Vic20Runtime::blank(model);
                    original.set_machine(Some(machine));
                    let saved = super::encode(&original).expect("snapshot timer load phase");
                    let mut restored = Vic20Runtime::blank(model);
                    decode(&mut restored, &saved).expect("restore timer load phase");
                    assert_eq!(restored.snapshot().expect("fixed point"), saved);
                    for _ in 0..4 {
                        original.machine_mut().expect("machine").step_instruction();
                        let machine = restored.machine_mut().expect("restored machine");
                        machine.step_instruction();
                        let elapsed =
                            u16::try_from(machine.master_clock() - start).expect("short countdown");
                        // One load cycle, then one decrement per Phi2 cycle.
                        let expected = initial.wrapping_sub(elapsed - 1);
                        for base in [0x9110, 0x9120] {
                            let actual = u16::from(machine.peek(base + 8))
                                | (u16::from(machine.peek(base + 9)) << 8);
                            assert_eq!(actual, expected, "{model:?}, VIA at {base:04x}");
                        }
                        assert_eq!(
                            restored.snapshot().expect("restored state"),
                            original.snapshot().expect("uninterrupted state")
                        );
                    }
                }
            }
        }
    }
}
