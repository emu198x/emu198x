//! Postcard-encoded snapshot envelope for the generic Spectrum runtime.
//!
//! Splits the snapshot/restore plumbing out of `runtime.rs` so the
//! versioned envelope and the encode/decode machinery have one home.
//! `MachineCore::snapshot` and `MachineCore::restore` are thin
//! delegators that call into [`encode`] and [`decode`].
//!
//! The envelope is generic over the wrapped machine `M`: `M: Serialize`
//! threads through the borrowed reference encoder, and a separate
//! owned form (with `M: Deserialize<'de>`) drives the decoder.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use serde::{Deserialize, Serialize};

use crate::runtime::{DiskCacheEntry, SpectrumMachine, SpectrumRuntime};

/// Snapshot envelope version. Bumped when the envelope schema
/// changes in a breaking way. Decoding rejects unknown versions
/// with a loud `InvalidSnapshot` rather than risking a field-shape
/// mismatch downstream.
///
/// **Version 1** (legacy): version + profile_id + time + keyboard_rows
/// + machine.
///
/// **Version 2** (Seam 3, 2026-05-20, current): adds `disk_images`
/// so a +3 snapshot taken with a disk mounted comes back with the
/// disk still inserted. The FDC's `disks` field stays
/// `#[serde(skip)]` (large, not all reconstructible from disk
/// state alone); the runtime caches the raw bytes alongside and
/// replays the insertion through `load_disk_image` after restore.
const SNAPSHOT_VERSION: u32 = 2;

/// Borrowed snapshot envelope used by [`encode`]. Generic over any
/// `M: Serialize`; the `SpectrumMachine` bound is enforced at the
/// `encode`/`decode` entry points so the envelope stays usable from
/// pure-serde contexts.
#[derive(Serialize)]
struct SpectrumRuntimeSnapshotRefV2<'a, M: Serialize> {
    version: u32,
    profile_id: &'a str,
    time: MachineTime,
    keyboard_rows: [u8; 8],
    machine: &'a M,
    disk_images: &'a [DiskCacheEntry],
}

/// Owned snapshot envelope used by [`decode`]. Generic over any
/// `M: Deserialize<'de>`.
#[derive(Deserialize)]
struct SpectrumRuntimeSnapshotV2<M> {
    version: u32,
    profile_id: String,
    time: MachineTime,
    keyboard_rows: [u8; 8],
    machine: M,
    /// Cached disk images that need re-inserting through
    /// `machine.load_disk_image` after restore. See Seam 3.
    /// Tape-only variants encode an empty vec.
    #[serde(default)]
    disk_images: Vec<DiskCacheEntry>,
}

/// Encode a runtime as postcard bytes. Caller-side error type is
/// [`MachineError::InvalidSnapshot`] with the postcard reason.
pub(crate) fn encode<M: SpectrumMachine>(
    runtime: &SpectrumRuntime<M>,
) -> Result<Vec<u8>, MachineError> {
    postcard::to_allocvec(&SpectrumRuntimeSnapshotRefV2 {
        version: SNAPSHOT_VERSION,
        profile_id: runtime.profile().profile_id.as_str(),
        time: runtime.time_value(),
        keyboard_rows: *runtime.keyboard_rows(),
        machine: runtime.machine(),
        disk_images: runtime.disk_images(),
    })
    .map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

/// Decode postcard bytes into a runtime. Validates the version and
/// the profile identifier; restores the machine state, the keyboard
/// matrix, and the time stamp atomically. After restoring the matrix
/// the cached rows are pushed back into the machine so its scan
/// register matches the runtime cache.
pub(crate) fn decode<M: SpectrumMachine>(
    runtime: &mut SpectrumRuntime<M>,
    bytes: &[u8],
) -> Result<(), MachineError> {
    let snapshot: SpectrumRuntimeSnapshotV2<M> =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;

    if snapshot.version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {}", snapshot.version),
        });
    }

    if snapshot.profile_id != runtime.profile().profile_id.as_str() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "snapshot profile {} does not match runtime profile {}",
                snapshot.profile_id,
                runtime.profile().profile_id.as_str()
            ),
        });
    }

    runtime.set_machine(snapshot.machine);
    runtime.set_time(snapshot.time);
    runtime.set_keyboard_rows(snapshot.keyboard_rows);
    let rows = *runtime.keyboard_rows();
    runtime.machine_mut().set_keyboard_rows(&rows);
    runtime.machine_mut().after_restore();
    // Replay disk images last — the FDC's `disks` field is
    // `#[serde(skip)]` so they didn't come back in `snapshot.machine`.
    // We re-insert from the cached bytes here. See Seam 3 of the
    // architecture review.
    runtime.restore_disk_images(snapshot.disk_images)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Model;
    use crate::Spectrum48kRuntime;
    use crate::variants::{Spectrum128kRuntime, SpectrumPlus3Runtime};
    use emu198x_shell::MachineCore;
    use machine_sinclair_zx_spectrum_128k::Spectrum128K;
    use machine_sinclair_zx_spectrum_plus3::SpectrumPlus3;

    fn synthetic_envelope(profile_id: &str, version: u32) -> Vec<u8> {
        // Hand-rolled envelope around a default-constructed 128K so we
        // can flip individual fields (version, profile_id) and exercise
        // the corresponding decode error paths without needing to touch
        // the real envelope schema.
        let machine = Spectrum128K::new();
        postcard::to_allocvec(&SpectrumRuntimeSnapshotRefV2 {
            version,
            profile_id,
            time: MachineTime::default(),
            keyboard_rows: [0xFF; 8],
            machine: &machine,
            disk_images: &[],
        })
        .expect("synthetic envelope should encode")
    }

    #[test]
    fn decode_rejects_completely_corrupt_bytes() {
        let mut runtime = Spectrum48kRuntime::blank();
        let err = runtime
            .restore(&[0xFFu8; 8])
            .expect_err("corrupt postcard bytes must not decode");
        assert!(matches!(err, MachineError::InvalidSnapshot { .. }));
    }

    #[test]
    fn decode_rejects_unsupported_snapshot_version() {
        let bytes = synthetic_envelope(Model::Spectrum128KPal.profile_id(), SNAPSHOT_VERSION + 1);
        let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
        let err = runtime
            .restore(&bytes)
            .expect_err("future snapshot versions must be rejected");
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
    fn decode_rejects_profile_mismatch_within_generic_envelope() {
        // Profile-id mismatch covers the generic decode arm without
        // needing two physically-distinct machine types.
        let bytes = synthetic_envelope("not-a-real-profile", SNAPSHOT_VERSION);
        let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
        let err = runtime
            .restore(&bytes)
            .expect_err("profile-id mismatch must be rejected");
        match err {
            MachineError::InvalidSnapshot { reason } => {
                assert!(
                    reason.contains("snapshot profile") && reason.contains("does not match"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected InvalidSnapshot, got {other:?}"),
        }
    }

    /// Constructs a minimal valid DSK image — 1 side, 1 track, 0
    /// sectors — so `format_amstrad_dsk::parse` accepts it without
    /// pulling in a real fixture file. The track has no sectors, so
    /// the parser produces a default `DiskTrack`. Enough to exercise
    /// the load-cache-snapshot-restore path; not enough to actually
    /// boot anything.
    fn synthetic_minimal_dsk() -> Vec<u8> {
        const HEADER_LEN: usize = 256;
        const TRACK_LEN: usize = 256;
        let mut data = vec![0u8; HEADER_LEN + TRACK_LEN];
        // Standard DSK signature (first 8 bytes of the 0x22-byte sig
        // are enough — the parser only checks `starts_with(b"MV - CPC")`).
        data[..8].copy_from_slice(b"MV - CPC");
        data[0x30] = 1; // tracks per side
        data[0x31] = 1; // sides
        data[0x32..0x34].copy_from_slice(&(TRACK_LEN as u16).to_le_bytes());
        // Track-Info header at offset 256
        data[HEADER_LEN..HEADER_LEN + 12].copy_from_slice(b"Track-Info\r\n");
        // sector_count = 0 → parse_track returns DiskTrack::default()
        data
    }

    /// **Seam 3 regression test.** A +3 snapshot taken with a disk
    /// mounted must come back with the disk still inserted. The
    /// FDC marks `disks` `#[serde(skip)]` (the parsed image is
    /// large and not all reconstructible from disk state alone), so
    /// without the runtime-layer cache + replay the disk silently
    /// vanishes on restore. This test would have failed before
    /// commit landing Seam 3.
    #[test]
    fn snapshot_restore_preserves_mounted_disk_on_plus3() {
        use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet};

        let mut original = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());
        let dsk = synthetic_minimal_dsk();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("disk-a", MediaKind::Disk, &dsk));
        original.load_media(&media).expect("disk should load");
        assert!(
            original.machine().fdc.has_disk(0),
            "disk must be mounted after load_media"
        );

        let bytes = original
            .snapshot()
            .expect("Plus3 snapshot with disk should encode");

        let mut restored = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());
        assert!(
            !restored.machine().fdc.has_disk(0),
            "fresh runtime must not have a disk yet"
        );

        restored.restore(&bytes).expect("snapshot should decode");
        assert!(
            restored.machine().fdc.has_disk(0),
            "disk must survive snapshot restore — Seam 3 of \
             knowledge/decisions/spectrum-architecture-review.md"
        );
    }

    #[test]
    fn encode_and_decode_round_trip_on_plus_runtime() {
        // Round-trips through the generic envelope on a non-128K, non-48K
        // variant so the whole serde-generic path stays exercised even
        // when the Plus3-specific runtime is constructed.
        let runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());
        let bytes = runtime.snapshot().expect("Plus3 snapshot should encode");

        let mut restored = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());
        restored
            .restore(&bytes)
            .expect("Plus3 snapshot should round-trip through the generic envelope");
    }
}
