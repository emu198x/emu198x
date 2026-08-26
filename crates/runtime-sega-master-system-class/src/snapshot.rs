//! Postcard-encoded snapshot envelope for the Master System class.
//!
//! Serialises the **live machine** (Z80, Sega VDP, SN76489 PSG, cart ROM,
//! RAM, and mapper registers) so a restore resumes exactly, rather than the
//! old bootstrap envelope that cold-booted from the cart. Mirrors the SG-1000
//! shape: a borrowing envelope for encode (no clone), an owning envelope for
//! decode.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_sega_master_system::Sms;
use serde::{Deserialize, Serialize};

use crate::runtime::SmsRuntime;

/// Bumped to 4 when the VDP framebuffer became region-sized. A snapshot
/// carries the live chip, framebuffer included, so a version-3 PAL snapshot
/// holds a 240-line buffer that a version-4 PAL machine would never allocate.
/// Restoring it would resume into a geometry the machine disagrees with, and
/// silently — so the version check rejects it instead. The Game Gear is
/// unaffected either way: its LCD is 160x144 in both.
const SNAPSHOT_VERSION: u16 = 4;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct SmsRuntimeSnapshotRefV3<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a Sms>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct SmsRuntimeSnapshotV3 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<Sms>,
}

pub(crate) fn encode(runtime: &SmsRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = SmsRuntimeSnapshotRefV3 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut SmsRuntime, bytes: &[u8]) -> Result<(), MachineError> {
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
    let snapshot: SmsRuntimeSnapshotV3 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    debug_assert_eq!(snapshot.version, SNAPSHOT_VERSION);
    if snapshot.model_id != runtime.model_id() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "snapshot model {} does not match runtime model {}",
                snapshot.model_id,
                runtime.model_id()
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
    use crate::runtime::SmsRuntime;
    use emu198x_shell::{
        CapabilitySet, ClockDesc, ClockRate, Family, MachineCore, MachineError, MachineId,
        MachineProfile, MediaKind, MediaSlot, ProfileId, Region, WritebackPolicy,
    };
    use emu198x_zilog_z80::Z80Stepper;
    use emu198x_zilog_z80::z80::Phase;
    use machine_sega_master_system::SmsVariant;

    /// The envelope is class-level behaviour, so these tests build their own
    /// profile rather than reaching for a machine crate's catalogue — the
    /// class crate must not depend on the machines layered on top of it.
    fn test_runtime() -> SmsRuntime {
        let profile = MachineProfile {
            machine_id: MachineId::from("test-sega-class"),
            profile_id: ProfileId::from("test-sega-class-ntsc"),
            display_name: "Master System class test fixture".into(),
            family: Family::Other,
            region: Region::Ntsc,
            release_year: 1985,
            summary: "Fixture profile for envelope tests.".into(),
            clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(3_579_545)),
            firmware: vec![],
            media_slots: vec![MediaSlot::new(
                "cartridge-1",
                "Cartridge Slot",
                MediaKind::Cartridge,
                true,
                WritebackPolicy::InMemoryOnly,
            )],
            capabilities: CapabilitySet::default(),
        };
        SmsRuntime::blank(profile, SmsVariant::SmsNtsc, "test-sega-class-ntsc")
    }

    fn cartridge_runtime() -> SmsRuntime {
        let mut runtime = test_runtime();
        runtime.insert_cartridge(vec![0; 0x8000]);
        runtime
    }

    #[test]
    fn decode_rejects_future_version_before_payload_decode() {
        let mut runtime = test_runtime();
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
        let mut runtime = test_runtime();
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

    #[test]
    fn snapshot_mid_nmi_response_continues_identically() {
        let mut original = cartridge_runtime();
        original
            .machine_mut()
            .expect("cartridge should create a machine")
            .set_pause_pressed(true);

        let accepted = (0..128).any(|_| {
            let machine = original
                .machine_mut()
                .expect("cartridge should create a machine");
            machine.step_tick();
            matches!(machine.cpu().phase, Phase::NmiAck(_))
        });
        assert!(accepted, "pause edge should start an NMI response");

        original
            .machine_mut()
            .expect("machine should remain installed")
            .step_tick();
        assert!(
            matches!(
                original
                    .machine()
                    .expect("machine should remain installed")
                    .cpu()
                    .phase,
                Phase::NmiAck(_)
            ),
            "snapshot waypoint should remain inside the NMI acknowledge cycle"
        );

        let snapshot = original.snapshot().expect("snapshot should encode");
        let mut restored = test_runtime();
        restored
            .restore(&snapshot)
            .expect("snapshot should restore");
        assert_eq!(
            restored
                .snapshot()
                .expect("restored snapshot should encode"),
            snapshot,
            "restore should preserve the complete public snapshot envelope"
        );

        for _ in 0..128 {
            original
                .machine_mut()
                .expect("original machine should remain installed")
                .step_tick();
            restored
                .machine_mut()
                .expect("restored machine should remain installed")
                .step_tick();
        }

        assert_eq!(
            restored
                .snapshot()
                .expect("continued restored snapshot should encode"),
            original
                .snapshot()
                .expect("continued original snapshot should encode"),
            "restored execution should remain byte-identical after the NMI response"
        );
    }
}
