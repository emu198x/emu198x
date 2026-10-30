//! Postcard-encoded snapshot envelope for the NES runtime.
//!
//! Splits the snapshot/restore plumbing out of `runtime.rs` so the
//! versioned envelope and the encode/decode machinery have one home.
//! `MachineCore::snapshot` and `MachineCore::restore` are thin
//! delegators that call into [`encode`] and [`decode`].

use crate::Nes;
use common_nintendo_nes::NesSnapshot;
use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::NesRuntime;

/// Version 3 retains the CPU open-bus latch; older values are unrecoverable.
const SNAPSHOT_VERSION: u16 = 3;

/// Persistable NES runtime envelope. Wraps the machine's chip
/// snapshot (when present) with the surrounding runtime context
/// (time, cartridge mapper id, cartridge bytes for replay).
#[derive(Serialize)]
struct NesRuntimeSnapshotRefV1<'a> {
    version: u16,
    time: u64,
    cartridge_mapper: Option<u16>,
    cartridge_bytes: Option<&'a [u8]>,
    machine: Option<NesSnapshot>,
}

#[derive(Deserialize)]
struct NesRuntimeSnapshotV1 {
    version: u16,
    time: u64,
    cartridge_mapper: Option<u16>,
    cartridge_bytes: Option<Vec<u8>>,
    machine: Option<NesSnapshot>,
}

/// Encode a runtime as postcard bytes. Caller-side error type is
/// [`MachineError::InvalidSnapshot`] with the postcard reason.
pub(crate) fn encode(runtime: &NesRuntime) -> Result<Vec<u8>, MachineError> {
    postcard::to_allocvec(&NesRuntimeSnapshotRefV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        cartridge_mapper: runtime.cartridge_mapper(),
        cartridge_bytes: runtime.cartridge_bytes(),
        machine: runtime.machine().map(Nes::snapshot),
    })
    .map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

/// Decode postcard bytes into a runtime. Validates the version and
/// restores the machine state, the cartridge bytes / mapper, and the
/// time stamp atomically.
pub(crate) fn decode(runtime: &mut NesRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    // Inspect only the version prefix before attempting the changed payload.
    let (version, _): (u16, _) =
        postcard::take_from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    if version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "unsupported NES snapshot version {version}; expected {SNAPSHOT_VERSION}"
            ),
        });
    }
    let snapshot: NesRuntimeSnapshotV1 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    debug_assert_eq!(snapshot.version, SNAPSHOT_VERSION);

    let machine = snapshot
        .machine
        .map(|state| Nes::from_snapshot(state, runtime.model().machine_region()))
        .transpose()
        .map_err(|reason| MachineError::InvalidSnapshot {
            reason: reason.into(),
        })?;
    if machine
        .as_ref()
        .is_some_and(|machine| machine.region() != runtime.model().machine_region())
    {
        return Err(MachineError::InvalidSnapshot {
            reason: "snapshot region does not match the selected NES model".to_owned(),
        });
    }

    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_cartridge_bytes(snapshot.cartridge_bytes);
    runtime.set_cartridge_mapper(snapshot.cartridge_mapper);
    runtime.set_machine(machine);
    runtime.refresh_rgba_framebuffer();
    Ok(())
}

#[cfg(test)]
mod fidelity_tests {
    use common_nintendo_nes::{Nes, NesSnapshot};
    use nintendo_nes_cartridge::{Mirroring, Nrom};
    #[test]
    fn malformed_output_buffers_are_rejected_by_snapshot_decode() {
        let live = Nes::new(Box::new(Nrom::new(
            vec![0xea; 16_384],
            vec![],
            Mirroring::Horizontal,
        )));
        let original = serde_json::to_value(live.snapshot()).expect("state value");
        for codes in [vec![], vec![512u16; 256 * 240]] {
            let mut malformed = original.clone();
            malformed["signal_codes"] = serde_json::to_value(codes).expect("codes");
            let result = serde_json::from_value::<NesSnapshot>(malformed);
            assert!(
                result.is_err(),
                "invalid mirror must fail before a live machine can be constructed"
            );
        }
    }

    #[test]
    fn serialized_ppu_bus_decay_deadlines_resume_identically() {
        let mut live = Nes::new(Box::new(Nrom::new(
            vec![0xea; 16_384],
            vec![],
            Mirroring::Horizontal,
        )));
        // A bus refresh late enough that substituting the reset deadline
        // would falsely expire it. Only the PPU is relevant to this storage test.
        live.ppu.run(
            live.mapper.as_mut(),
            emu198x_ricoh_ppu_2c02::OPEN_BUS_DECAY_TICKS + 4,
        );
        live.ppu.cpu_write(0, 0x1f, live.mapper.as_mut());
        let bytes = postcard::to_allocvec(&live.snapshot()).expect("serialize");
        let snapshot: NesSnapshot = postcard::from_bytes(&bytes).expect("decode");
        let mut restored = Nes::from_snapshot(snapshot);
        let expected = live.ppu.cpu_read(0, live.mapper.as_mut());
        assert_eq!(expected, 0x1f, "a freshly written bus has not decayed");
        assert_eq!(
            restored.ppu.cpu_read(0, restored.mapper.as_mut()),
            expected,
            "saved per-bit refresh deadlines must survive serialization"
        );
    }
}
