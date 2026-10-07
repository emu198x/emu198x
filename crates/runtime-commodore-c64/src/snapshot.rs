//! Postcard-encoded snapshot envelope for the C64 runtime.
//!
//! Splits the snapshot/restore plumbing out of `runtime.rs` so the
//! versioned envelope and the encode/decode machinery have one home.
//! `MachineCore::snapshot` and `MachineCore::restore` are thin
//! delegators that call into [`encode`] and [`decode`].

use common_commodore_iec::IecBus;
use emu198x_shell::{MachineError, MachineTime};
use machine_commodore_c64::C64Snapshot;
use serde::{Deserialize, Serialize};

use crate::drives::IecDriveSnapshot;
use crate::runtime::C64Runtime;

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
const SNAPSHOT_VERSION: u32 = 15;

/// Persistable C64 runtime envelope. Wraps the machine's chip snapshot with the
/// surrounding runtime context (model identifier, time, the live IEC bus state,
/// the per-port drive snapshots, each port's cycle-accumulator phase, and the
/// expansion bookkeeping a reset rebuilds from).
#[derive(Serialize, Deserialize)]
struct SnapshotEnvelopeV13 {
    version: u32,
    profile_id: String,
    time: MachineTime,
    machine: C64Snapshot,
    drives: [Option<IecDriveSnapshot>; 4],
    drive_cycle_accum: [u64; 4],
    iec_bus: IecBus,
    cartridge_image: Option<Vec<u8>>,
    georam_kb: Option<usize>,
    reu_kb: Option<usize>,
    mouse_1351_port: Option<u8>,
}

/// Encode a runtime as postcard bytes. Caller-side error type is
/// [`MachineError::InvalidSnapshot`] with the postcard reason.
pub(crate) fn encode(runtime: &C64Runtime) -> Result<Vec<u8>, MachineError> {
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
    })
    .map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

/// Decode postcard bytes into a runtime. Validates the version and
/// the profile identifier; restores the machine state, the per-port
/// drives, the IEC bus, and the time stamp atomically.
pub(crate) fn decode(runtime: &mut C64Runtime, bytes: &[u8]) -> Result<(), MachineError> {
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

    let snapshot: SnapshotEnvelopeV13 =
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
    );
    runtime.set_time(snapshot.time);
    Ok(())
}
