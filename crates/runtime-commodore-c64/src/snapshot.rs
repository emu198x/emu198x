//! Postcard-encoded snapshot envelope for the C64 runtime.
//!
//! Splits the snapshot/restore plumbing out of `runtime.rs` so the
//! versioned envelope and the encode/decode machinery have one home.
//! `MachineCore::snapshot` and `MachineCore::restore` are thin
//! delegators that call into [`encode`] and [`decode`].

use common_commodore_iec::IecBus;
use emu198x_shell::{MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::drives::IecDriveSnapshot;
use crate::runtime::C64FamilyRuntime;
use crate::variants::C64FamilyMachine;

/// Version 20: the 1571's and 1581's WD1770 carries its byte pacing, the
/// cycles between data bytes and the countdown to the next.
///
/// Version 19 preserves the 1541/1571 VIA timer 2 load phase (#1677).
///
/// Version 18 preserves the keyboard's mechanical SHIFT LOCK contact,
/// independently of ordinary left Shift and its different output contention.
///
/// Version 17 adds the VIC-II's last matrix and colour entries, which the
/// graphics sequencer keeps loading outside the display window (#1661).
///
/// Version 16 replaces the VIC-II's decoded cell and its carry with the
/// graphics sequencer's shift register, matrix and colour latches and its
/// XSCROLL and mode-bit pipes (#1660). Cells in the colour stage keep their
/// g-access and sprite pixels until the sequencer draws them.
///
/// Version 15 replaces the VIC-II's once-per-line XSCROLL latch with the
/// graphics sequencer's most recent load (#1620): the cell, the shift
/// register's carry before it and the XSCROLL it was loaded at. The cell
/// also records the colour its zero bits select.
///
/// Version 14 replaces the 6526's two-underflows-per-bit shift register with
/// the serial-port pipeline, CNT/SP pins and /PC strobe for #797 (in both
/// CIAs and any 1571 or 1581 drive): the SDR and shifter, the bit counter,
/// the `sdr_delay` line, the chip's own CNT/SP drive, the board's CNT/SP
/// input levels, the CNT edge pipeline to the timers, and the pending /PC.
///
/// Version 13 adds SID waveform-generator pipeline state for #1606: each
/// voice's OSC3 value, the 8580's one-cycle triangle/sawtooth stage, the
/// pulse level from last cycle's comparator, and the noise register's
/// two-cycle shift pipeline.
///
/// Version 12 adds the VIC-II's chip revision, which selects the colour
/// stage's first-dot rule: the NMOS 6567/6569 keep the old colour, the
/// HMOS-II 8562/8565 show the grey dot (stage 3a-D, #796).
///
/// Version 11 adds the VIC-II's two-tick colour stage: the two rendered cells
/// awaiting colour-register and border resolution, the colour-register write
/// since the last tick, and VICE's main border flip-flop with its two-tick
/// history (stage 3a-C).
///
/// Version 10 drops the VIC-II's geometric sprite-DMA flags, now derived from the
/// fetch chain, and adds the light-pen input level (stage 3a-B).
///
/// Version 9 adds SID state for #777: each voice's TEST-held noise-register
/// drift timer and latched (possibly floating) waveform DAC input, and the
/// decaying data-bus value that write-only reads return.
///
/// Version 8 replaces the fixture-specific forced-badline saved entry with the
/// live 12-bit C-data carry age and value. The carry can change the next
/// graphics fetch and therefore must survive arbitrary-cycle restore.
///
/// Version 7 adds the VIC-II's forced-badline C/V/G output-delay and bounded
/// C-data carry state so an arbitrary-cycle restore retains the two
/// already-pipelined idle cells and the displaced line-buffer entry.
///
/// Version 6 adds source-resolved VIC-II badline BA, sprite BA and c-access
/// state, plus the pending `$D011` write phase and far-edge badline DMA-window
/// marker. These make an arbitrary-cycle restore retain the exact externally
/// inspectable bus phase rather than recomputing it from the following cycle.
///
/// Version 5 adds the VIC-II's live BA-to-AEC delay counter. Without it, a
/// restore during the three-cycle bus handover could make the next matrix or
/// sprite access valid at the wrong Phi2 phase.
///
/// Version 22 adds each 1351 mouse's motion waiting for the next frame and
/// the counts it has let through this frame, and its counts become the
/// mouse's own, two pot steps each.
///
/// Version 21 adds the CMD SuperCPU: the machine carries the live board, and
/// the envelope its firmware and SuperRAM size, so a reset after a restore
/// fits it again.
///
/// Version 4 makes arbitrary mid-frame snapshots lossless by including the
/// live VIC-II sprite fetch/draw pipeline and SID samples queued since the last
/// frame boundary. Those fields were omitted from earlier machine snapshots.
///
/// Version 3 adds the runtime-level expansion bookkeeping — the inserted
/// cartridge image and the GeoRAM/REU sizes and 1351-mouse port — so a restored
/// snapshot re-attaches them on the next reset (previously those fields
/// defaulted to `None`, so a reset dropped the cartridge and expansions).
///
/// Version 2 moved from the fixed 1541-plus-1581 pair to a per-port array of
/// model-tagged drive snapshots (IEC devices 8–11), so a snapshot records
/// whichever drive the user chose on each port.
const SNAPSHOT_VERSION: u32 = 22;

/// Persistable C64 runtime envelope. Wraps the machine's chip snapshot with the
/// surrounding runtime context (model identifier, time, the live IEC bus state,
/// the per-port drive snapshots, each port's cycle-accumulator phase, and the
/// expansion bookkeeping a reset rebuilds from).
///
/// Generic over the machine's own snapshot type, so a C64's envelope encodes
/// exactly as it did before the runtime hosted a second machine. A C128's
/// carries empty drive ports and no expansions, which is what it has; its
/// profile id keeps it from being restored into a C64, and the C64's from a
/// C128.
#[derive(Serialize, Deserialize)]
struct SnapshotEnvelopeV13<S> {
    version: u32,
    profile_id: String,
    time: MachineTime,
    machine: S,
    drives: [Option<IecDriveSnapshot>; 4],
    drive_cycle_accum: [u64; 4],
    iec_bus: IecBus,
    cartridge_image: Option<Vec<u8>>,
    georam_kb: Option<usize>,
    reu_kb: Option<usize>,
    mouse_1351_port: Option<u8>,
    supercpu: Option<(Vec<u8>, usize)>,
}

/// Encode a runtime as postcard bytes. Caller-side error type is
/// [`MachineError::InvalidSnapshot`] with the postcard reason.
pub(crate) fn encode<M: C64FamilyMachine>(
    runtime: &C64FamilyRuntime<M>,
) -> Result<Vec<u8>, MachineError> {
    postcard::to_allocvec(&SnapshotEnvelopeV13 {
        version: SNAPSHOT_VERSION,
        profile_id: runtime.profile().profile_id.as_str().to_owned(),
        time: runtime.time(),
        machine: runtime.machine().snapshot_state(),
        drives: runtime.drives_snapshot(),
        drive_cycle_accum: runtime.drive_cycle_accum_all(),
        iec_bus: runtime.iec_bus().clone(),
        cartridge_image: runtime.cartridge_image_bytes().map(<[u8]>::to_vec),
        georam_kb: runtime.georam_kb(),
        reu_kb: runtime.reu_kb(),
        mouse_1351_port: runtime.mouse_1351_port(),
        supercpu: runtime
            .supercpu_fitting()
            .map(|(rom, superram)| (rom.to_vec(), superram)),
    })
    .map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

/// Decode postcard bytes into a runtime. Validates the version and
/// the profile identifier; restores the machine state, the per-port
/// drives, the IEC bus, and the time stamp atomically.
pub(crate) fn decode<M: C64FamilyMachine>(
    runtime: &mut C64FamilyRuntime<M>,
    bytes: &[u8],
) -> Result<(), MachineError> {
    // Read the leading version varint before deserialising the versioned
    // payload. Nested VIC/SID schema changes can otherwise fail inside
    // postcard before the explicit version check explains the incompatibility.
    let (version, _) = postcard::take_from_bytes::<u32>(bytes).map_err(|reason| {
        MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        }
    })?;
    if version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {version}; expected {SNAPSHOT_VERSION}"),
        });
    }

    let snapshot: SnapshotEnvelopeV13<M::Snapshot> =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    debug_assert_eq!(snapshot.version, SNAPSHOT_VERSION);

    if snapshot.profile_id != runtime.profile().profile_id.as_str() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "snapshot profile {} does not match runtime profile {}",
                snapshot.profile_id,
                runtime.profile().profile_id.as_str()
            ),
        });
    }

    runtime
        .machine_mut()
        .restore_snapshot_state(snapshot.machine)
        .map_err(|reason| MachineError::InvalidSnapshot { reason })?;
    runtime
        .restore_drives(snapshot.drives)
        .map_err(|reason| MachineError::InvalidSnapshot { reason })?;
    runtime.set_iec_bus(snapshot.iec_bus);
    runtime.set_drive_cycle_accum_all(snapshot.drive_cycle_accum);
    runtime.restore_expansions(
        snapshot.cartridge_image,
        snapshot.georam_kb,
        snapshot.reu_kb,
        snapshot.mouse_1351_port,
        snapshot.supercpu,
    );
    runtime.set_time(snapshot.time);
    Ok(())
}

#[cfg(test)]
mod board_payload_tests {
    use super::*;
    use crate::{C64Runtime, C128Model, C128Runtime, Model};
    use emu198x_shell::MachineCore;
    #[test]
    fn matching_c64_header_cannot_hide_foreign_hardware_or_mutate_the_runtime() {
        for selected in Model::ALL {
            let mut runtime = C64Runtime::blank(selected);
            let before = runtime.snapshot().expect("before");
            for foreign in Model::ALL {
                if foreign == selected {
                    continue;
                }
                let other = C64Runtime::blank(foreign);
                let mut envelope: SnapshotEnvelopeV13<common_commodore_c64::C64Snapshot> =
                    postcard::from_bytes(&before).expect("envelope");
                envelope.machine = other.machine().snapshot_state();
                envelope.time = MachineTime::new(1234);
                let forged = postcard::to_allocvec(&envelope).expect("forged");
                assert!(runtime.restore(&forged).is_err());
                assert!(runtime.snapshot().expect("unchanged") == before);
            }
        }
    }
    #[test]
    fn matching_c128_header_cannot_hide_foreign_hardware_or_mutate_the_runtime() {
        for selected in C128Model::ALL {
            let mut runtime = C128Runtime::blank(selected);
            let before = runtime.snapshot().expect("before");
            for foreign in C128Model::ALL {
                if foreign == selected {
                    continue;
                }
                let other = C128Runtime::blank(foreign);
                let mut envelope: SnapshotEnvelopeV13<common_commodore_c128::C128> =
                    postcard::from_bytes(&before).expect("envelope");
                envelope.machine = other.machine().snapshot_state();
                envelope.time = MachineTime::new(1234);
                let forged = postcard::to_allocvec(&envelope).expect("forged");
                assert!(runtime.restore(&forged).is_err());
                assert!(runtime.snapshot().expect("unchanged") == before);
            }
        }
    }
}
