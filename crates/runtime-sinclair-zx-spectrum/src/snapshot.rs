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
/// **Version 2** (Seam 3, 2026-05-20): added `disk_images`
/// so a +3 snapshot taken with a disk mounted comes back with the
/// disk still inserted. The FDC's `disks` field stays
/// `#[serde(skip)]` (large, not all reconstructible from disk
/// state alone); the runtime caches the raw bytes alongside and
/// replays the insertion through `load_disk_image` after restore.
///
/// **Version 3**: preserves the explicit identity of an accepted Z80
/// NMI or maskable-interrupt response. Version 2 could serialise a
/// response in progress, but could not distinguish its skipped static
/// walker sequence after decode, so it is rejected.
///
/// **Version 4**: every Spectrum-family machine carries the
/// tape's `StopRelease` state (#1633), so a snapshot taken in the frame
/// after the tape stops still releases the input on time. Postcard is
/// positional, so version 3 machines no longer decode and are rejected.
/// **Version 5**: the FDC carries live mounted disks, protection and write
/// transfer state. Version 4 is rejected before decode; its raw image cache
/// cannot represent writes. Approved for the first +3 ROM SAVE qualification.
/// **Version 6** (current): the FDC preserves per-sector read progress and
/// encountered status separately from future sectors. Version 5 lacks that
/// positional controller state and is rejected before payload decode.
const SNAPSHOT_VERSION: u32 = 6;

/// Borrowed snapshot envelope used by [`encode`]. Generic over any
/// `M: Serialize`; the `SpectrumMachine` bound is enforced at the
/// `encode`/`decode` entry points so the envelope stays usable from
/// pure-serde contexts.
#[derive(Serialize)]
struct SpectrumRuntimeSnapshotRefV6<'a, M: Serialize> {
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
struct SpectrumRuntimeSnapshotV6<M> {
    version: u32,
    profile_id: String,
    time: MachineTime,
    keyboard_rows: [u8; 8],
    machine: M,
    /// Original input images retained for source identity. Mounted live
    /// media in the machine is authoritative; restore must not replay these.
    /// Tape-only variants encode an empty vec.
    #[serde(default)]
    disk_images: Vec<DiskCacheEntry>,
}

/// Encode a runtime as postcard bytes. Caller-side error type is
/// [`MachineError::InvalidSnapshot`] with the postcard reason.
pub(crate) fn encode<M: SpectrumMachine>(
    runtime: &SpectrumRuntime<M>,
) -> Result<Vec<u8>, MachineError> {
    postcard::to_allocvec(&SpectrumRuntimeSnapshotRefV6 {
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

    let snapshot: SpectrumRuntimeSnapshotV6<M> =
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

    runtime.set_machine(snapshot.machine);
    runtime.set_time(snapshot.time);
    runtime.set_keyboard_rows(snapshot.keyboard_rows);
    let rows = *runtime.keyboard_rows();
    runtime.machine_mut().set_keyboard_rows(&rows);
    runtime.machine_mut().after_restore();
    // Validate cached source slots against the serialized live controller.
    // Original source bytes must never replace modified mounted media.
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
        postcard::to_allocvec(&SpectrumRuntimeSnapshotRefV6 {
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
    fn decode_rejects_version_2_before_payload_decode() {
        let bytes = postcard::to_allocvec(&2_u32).expect("legacy version should encode");
        let mut runtime = Spectrum48kRuntime::blank();
        let err = runtime
            .restore(&bytes)
            .expect_err("version 2 snapshots must be rejected");
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
    fn decode_rejects_version_4_before_payload_decode() {
        let mut runtime = crate::Spectrum48kRuntime::blank();
        let bytes = postcard::to_allocvec(&4_u32).expect("version encodes");
        let error = decode(&mut runtime, &bytes).expect_err("old version must be rejected");
        assert!(
            error
                .to_string()
                .contains("unsupported snapshot version 4; expected 6")
        );
    }

    #[test]
    fn decode_rejects_version_5_before_payload_decode_without_changing_runtime() {
        let mut runtime = SpectrumPlus3Runtime::blank();
        let before = runtime
            .snapshot()
            .expect("snapshot before rejected restore");
        let bytes = postcard::to_allocvec(&5_u32).expect("version encodes");
        let error = runtime
            .restore(&bytes)
            .expect_err("version 5 lacks current read progress");
        assert!(
            error
                .to_string()
                .contains("unsupported snapshot version 5; expected 6")
        );
        assert_eq!(runtime.snapshot().expect("runtime unchanged"), before);
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

    /// A +3 runtime snapshot retains its mounted media. The serialized
    /// controller is authoritative even when original source bytes are cached.
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
    fn writable_sector_runtime(writable: bool) -> (SpectrumPlus3Runtime, Vec<u8>) {
        use emu198x_shell::{MediaImage, MediaKind, MediaSet};
        let mut dsk = synthetic_minimal_dsk();
        dsk.resize(1024, 0xe5);
        dsk[0x32..0x34].copy_from_slice(&768_u16.to_le_bytes());
        dsk[0x114] = 2;
        dsk[0x115] = 1;
        dsk[0x118..0x120].copy_from_slice(&[0, 0, 1, 2, 0, 0, 0, 2]);
        let mut runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());
        let mut media = MediaSet::new();
        media.push(MediaImage::new("disk-a", MediaKind::Disk, &dsk).writable(writable));
        runtime.load_media(&media).expect("mount sector");
        (runtime, dsk)
    }

    fn send_fdc_command(runtime: &mut SpectrumPlus3Runtime, command: &[u8]) {
        for &byte in command {
            runtime.machine_mut().fdc.write_data(byte);
        }
    }

    fn send_fdc_bytes(runtime: &mut SpectrumPlus3Runtime, bytes: &[u8]) {
        use common_sinclair_zx_spectrum::peripheral::Peripheral;
        for &byte in bytes {
            for _ in 0..5000 {
                if runtime.machine().fdc.read_status() & 0x80 != 0 {
                    break;
                }
                runtime.machine_mut().fdc.tick(0);
            }
            assert_eq!(runtime.machine().fdc.read_status() & 0xf0, 0xb0);
            runtime.machine_mut().fdc.write_data(byte);
            for _ in 0..114 {
                runtime.machine_mut().fdc.tick(0);
            }
        }
    }

    fn drain_fdc_result(runtime: &mut SpectrumPlus3Runtime) -> Vec<u8> {
        assert_eq!(runtime.machine().fdc.read_status() & 0xf0, 0xd0);
        (0..7)
            .map(|_| runtime.machine_mut().fdc.read_data())
            .collect()
    }

    fn read_fdc_byte(runtime: &mut SpectrumPlus3Runtime) -> u8 {
        use common_sinclair_zx_spectrum::peripheral::Peripheral;
        for _ in 0..10_000 {
            if runtime.machine().fdc.read_status() & 0x80 != 0 {
                return runtime.machine_mut().fdc.read_data();
            }
            assert_eq!(runtime.machine().fdc.read_status(), 0x70);
            runtime.machine_mut().fdc.tick(0);
        }
        panic!("read byte deadline");
    }

    fn assert_saved_bytes(runtime: &mut SpectrumPlus3Runtime, expected: &[u8]) {
        send_fdc_command(runtime, &[0x46, 0, 0, 0, 1, 2, 1, 0x2a, 0xff]);
        let observed: Vec<_> = (0..512).map(|_| read_fdc_byte(runtime)).collect();
        assert_eq!(observed, expected);
        let _ = drain_fdc_result(runtime);
    }

    fn two_sector_read_runtime(opcode: u8, first_crc: bool) -> (SpectrumPlus3Runtime, Vec<u8>) {
        use emu198x_shell::{MediaImage, MediaKind, MediaSet};
        let mut dsk = synthetic_minimal_dsk();
        dsk.resize(1536, 0);
        dsk[0x32..0x34].copy_from_slice(&1280_u16.to_le_bytes());
        dsk[0x114] = 2;
        dsk[0x115] = 2;
        let deleted = if opcode == 0x4c { 0x40 } else { 0 };
        let crc = if first_crc { 0x20 } else { 0 };
        dsk[0x118..0x120].copy_from_slice(&[0, 0, 1, 2, crc, deleted | crc, 0, 2]);
        dsk[0x120..0x128].copy_from_slice(&[0, 0, 2, 2, 0, deleted, 0, 2]);
        dsk[512..1024].fill(0x21);
        dsk[1024..1536].fill(0x22);
        let mut runtime = SpectrumPlus3Runtime::blank();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("disk-a", MediaKind::Disk, &dsk));
        runtime.load_media(&media).expect("mount two sectors");
        (runtime, dsk)
    }

    fn await_fdc_read_request(runtime: &mut SpectrumPlus3Runtime) {
        use common_sinclair_zx_spectrum::peripheral::Peripheral;
        for _ in 0..10_000 {
            if runtime.machine().fdc.read_status() & 0x80 != 0 {
                assert_eq!(runtime.machine().fdc.read_status(), 0xf0);
                return;
            }
            assert_eq!(runtime.machine().fdc.read_status(), 0x70);
            runtime.machine_mut().fdc.tick(0);
        }
        panic!("read request deadline");
    }

    #[test]
    fn current_read_snapshot_resumes_before_within_and_between_sectors() {
        for opcode in [0x46, 0x4c, 0x42] {
            for accepted in [0, 1, 511, 512, 513, 1023] {
                for at_request in [false, true] {
                    let (mut original, source) = two_sector_read_runtime(opcode, false);
                    let live_before = original.export_media("disk-a", "edsk").expect("live media");
                    send_fdc_command(&mut original, &[opcode, 0, 0, 0, 1, 2, 2, 0x2a, 0xff]);
                    for i in 0..accepted {
                        assert_eq!(read_fdc_byte(&mut original), 0x21 + (i / 512) as u8);
                    }
                    if at_request {
                        await_fdc_read_request(&mut original);
                    }
                    let saved = original.snapshot().expect("two-sector read checkpoint");
                    let mut restored = SpectrumPlus3Runtime::blank();
                    restored.restore(&saved).expect("restore current sector");
                    assert_eq!(restored.snapshot().expect("unchanged checkpoint"), saved);
                    for runtime in [&mut original, &mut restored] {
                        for i in accepted..1024 {
                            assert_eq!(read_fdc_byte(runtime), 0x21 + (i / 512) as u8);
                        }
                        assert_eq!(
                            drain_fdc_result(runtime),
                            [
                                0x40,
                                if opcode == 0x42 { 0x84 } else { 0x80 },
                                0,
                                0,
                                0,
                                if opcode == 0x42 { 1 } else { 2 },
                                2,
                            ]
                        );
                        assert_eq!(runtime.machine().fdc.read_status(), 0x80);
                        assert_eq!(runtime.disk_images()[0].bytes, source);
                        assert_eq!(
                            runtime
                                .export_media("disk-a", "edsk")
                                .expect("unchanged live media"),
                            live_before
                        );
                    }
                    assert_eq!(
                        restored.snapshot().expect("resumed state"),
                        original.snapshot().expect("uninterrupted state")
                    );
                }
            }
        }
    }

    #[test]
    fn current_read_snapshot_retains_position_errors_and_remaining_overrun_deadline() {
        use common_sinclair_zx_spectrum::peripheral::Peripheral;
        for (opcode, first_crc) in [(0x46, false), (0x4c, false), (0x42, false), (0x42, true)] {
            for accepted in [0, 1, 511, 512, 513, 1023] {
                for at_request in [false, true] {
                    let (mut original, source) = two_sector_read_runtime(opcode, first_crc);
                    let live_before = original.export_media("disk-a", "edsk").expect("live media");
                    send_fdc_command(&mut original, &[opcode, 0, 0, 0, 1, 2, 2, 0x2a, 0xff]);
                    for i in 0..accepted {
                        assert_eq!(read_fdc_byte(&mut original), 0x21 + (i / 512) as u8);
                    }
                    let boundary = |slot: u64| (slot * 32 * 3_546_895).div_ceil(1_000_000);
                    let slot = 39 + (accepted % 512) as u64;
                    let interval = boundary(slot + 1) - boundary(slot);
                    if at_request {
                        await_fdc_read_request(&mut original);
                        for _ in 0..interval - 1 {
                            original.machine_mut().fdc.tick(0);
                        }
                        assert_eq!(original.machine().fdc.read_status(), 0xf0);
                    }
                    let saved = original.snapshot().expect("read error checkpoint");
                    let mut restored = SpectrumPlus3Runtime::blank();
                    restored
                        .restore(&saved)
                        .expect("restore read error progress");
                    assert_eq!(restored.snapshot().expect("unchanged checkpoint"), saved);
                    let later_sector = accepted >= 512;
                    let completed_crc = first_crc && later_sector;
                    for runtime in [&mut original, &mut restored] {
                        await_fdc_read_request(runtime);
                        for _ in 0..if at_request { 1 } else { interval } {
                            runtime.machine_mut().fdc.tick(0);
                        }
                        assert_eq!(
                            drain_fdc_result(runtime),
                            [
                                0x40,
                                0x10 | if opcode == 0x42 && later_sector { 4 } else { 0 }
                                    | if completed_crc { 0x20 } else { 0 },
                                if completed_crc { 0x20 } else { 0 },
                                0,
                                0,
                                if opcode != 0x42 && later_sector { 2 } else { 1 },
                                2,
                            ]
                        );
                        assert_eq!(runtime.machine().fdc.read_status(), 0x80);
                        assert_eq!(runtime.disk_images()[0].bytes, source);
                        assert_eq!(
                            runtime
                                .export_media("disk-a", "edsk")
                                .expect("read-only live media"),
                            live_before
                        );
                    }
                    assert_eq!(
                        restored.snapshot().expect("resumed error state"),
                        original.snapshot().expect("uninterrupted error state")
                    );
                }
            }
        }
    }

    #[test]
    fn current_read_snapshot_resumes_partial_normal_and_overrun_results() {
        use common_sinclair_zx_spectrum::peripheral::Peripheral;
        for opcode in [0x46, 0x4c, 0x42] {
            for overrun in [false, true] {
                for drained in [0, 1, 3, 6] {
                    let (mut original, source) = two_sector_read_runtime(opcode, false);
                    send_fdc_command(&mut original, &[opcode, 0, 0, 0, 1, 2, 2, 0x2a, 0xff]);
                    let accepted = if overrun { 513 } else { 1024 };
                    for i in 0..accepted {
                        assert_eq!(read_fdc_byte(&mut original), 0x21 + (i / 512) as u8);
                    }
                    if overrun {
                        await_fdc_read_request(&mut original);
                        for _ in 0..114 {
                            original.machine_mut().fdc.tick(0);
                        }
                    }
                    let expected = [
                        0x40,
                        (if overrun { 0x10 } else { 0x80 }) | if opcode == 0x42 { 4 } else { 0 },
                        0,
                        0,
                        0,
                        if opcode == 0x42 { 1 } else { 2 },
                        2,
                    ];
                    for &byte in &expected[..drained] {
                        assert_eq!(original.machine().fdc.read_status(), 0xd0);
                        assert_eq!(original.machine_mut().fdc.read_data(), byte);
                    }
                    let saved = original.snapshot().expect("partial read result");
                    let mut restored = SpectrumPlus3Runtime::blank();
                    restored.restore(&saved).expect("restore result cursor");
                    assert_eq!(restored.snapshot().expect("unchanged result"), saved);
                    for runtime in [&mut original, &mut restored] {
                        for &byte in &expected[drained..] {
                            assert_eq!(runtime.machine().fdc.read_status(), 0xd0);
                            assert_eq!(runtime.machine_mut().fdc.read_data(), byte);
                        }
                        assert_eq!(runtime.machine().fdc.read_status(), 0x80);
                        assert_eq!(runtime.machine_mut().fdc.read_data(), 0xff);
                        assert_eq!(runtime.disk_images()[0].bytes, source);
                    }
                    assert_eq!(
                        restored.snapshot().expect("resumed result complete"),
                        original.snapshot().expect("uninterrupted result complete")
                    );
                }
            }
        }
    }

    #[test]
    fn paced_read_snapshot_preserves_request_and_deadline() {
        use common_sinclair_zx_spectrum::peripheral::Peripheral;
        for opcode in [0x46, 0x4c, 0x42] {
            for consumed in [false, true] {
                for elapsed in [0, 1, 113] {
                    let (mut original, _) = writable_sector_runtime(false);
                    send_fdc_command(&mut original, &[opcode, 0, 0, 0, 1, 2, 1, 0x2a, 0xff]);
                    for _ in 0..10_000 {
                        if original.machine().fdc.read_status() & 0x80 != 0 {
                            break;
                        }
                        original.machine_mut().fdc.tick(0);
                    }
                    assert_eq!(original.machine().fdc.read_status(), 0xf0);
                    if consumed {
                        assert_eq!(original.machine_mut().fdc.read_data(), 0xe5);
                    }
                    for _ in 0..elapsed {
                        original.machine_mut().fdc.tick(0);
                    }
                    let saved = original.snapshot().expect("paced read checkpoint");
                    let mut restored = SpectrumPlus3Runtime::blank();
                    restored.restore(&saved).expect("restore paced read");
                    assert_eq!(restored.snapshot().expect("identical state"), saved);
                    for _ in elapsed..114 {
                        original.machine_mut().fdc.tick(0);
                        restored.machine_mut().fdc.tick(0);
                    }
                    assert_eq!(
                        original.machine().fdc.read_status(),
                        if consumed { 0xf0 } else { 0xd0 }
                    );
                    assert_eq!(
                        original.machine_mut().fdc.read_data(),
                        if consumed { 0xe5 } else { 0x40 }
                    );
                    assert_eq!(
                        restored.machine_mut().fdc.read_data(),
                        if consumed { 0xe5 } else { 0x40 }
                    );
                    assert_eq!(
                        restored.snapshot().expect("continued"),
                        original.snapshot().expect("original")
                    );
                }
            }
        }
    }

    #[test]
    fn scan_and_track_read_resume_without_mutating_disk() {
        for accepted in [0, 1, 179, 511] {
            for opcode in [0x51, 0x59, 0x5d, 0x42] {
                let (mut original, source) = writable_sector_runtime(false);
                send_fdc_command(&mut original, &[opcode, 0, 0, 0, 1, 2, 1, 0x2a, 1]);
                if opcode == 0x42 {
                    for _ in 0..accepted {
                        assert_eq!(read_fdc_byte(&mut original), 0xe5);
                    }
                } else {
                    send_fdc_bytes(&mut original, &vec![0xe5; accepted]);
                }
                let saved = original.snapshot().expect("read/scan checkpoint");
                let mut restored = SpectrumPlus3Runtime::blank();
                restored.restore(&saved).expect("restore read/scan");
                assert_eq!(restored.snapshot().expect("identical state"), saved);
                // A comparison/read does not prohibit a read-only media export.
                assert_eq!(
                    restored
                        .export_media("disk-a", "edsk")
                        .expect("scan export"),
                    original
                        .export_media("disk-a", "edsk")
                        .expect("original export")
                );
                assert_eq!(restored.snapshot().expect("export unchanged"), saved);
                for runtime in [&mut original, &mut restored] {
                    if opcode == 0x42 {
                        for _ in accepted..512 {
                            assert_eq!(read_fdc_byte(runtime), 0xe5);
                        }
                        assert_eq!(drain_fdc_result(runtime), [0x40, 0x80, 0, 0, 0, 1, 2]);
                    } else {
                        send_fdc_bytes(runtime, &vec![0xe5; 512 - accepted]);
                        assert_eq!(drain_fdc_result(runtime), [0, 0, 8, 0, 0, 1, 2]);
                    }
                    assert_eq!(runtime.disk_images()[0].bytes, source);
                    assert_eq!(
                        runtime
                            .machine()
                            .fdc
                            .disk_for_export(0)
                            .expect("unchanged sector")
                            .tracks[0][0]
                            .sectors[0]
                            .data,
                        [0xe5; 512]
                    );
                }
                assert_eq!(
                    restored.snapshot().expect("continued"),
                    original.snapshot().expect("original")
                );
            }
        }
    }

    #[test]
    fn deleted_write_survives_active_state_export_and_fresh_reopen() {
        use emu198x_shell::{MediaImage, MediaKind, MediaSet};
        for accepted in [0, 1, 179, 511, 512] {
            let (mut original, source) = writable_sector_runtime(true);
            let payload: Vec<_> = (0..512).map(|i| (i as u8).wrapping_mul(17)).collect();
            send_fdc_command(&mut original, &[0x49, 0, 0, 0, 1, 2, 1, 0x2a, 0xff]);
            send_fdc_bytes(&mut original, &payload[..accepted]);
            let saved = original.snapshot().expect("deleted-write checkpoint");
            if accepted < 512 {
                assert!(original.export_media("disk-a", "edsk").is_err());
                assert_eq!(original.snapshot().expect("refusal is read-only"), saved);
            }
            let mut restored = SpectrumPlus3Runtime::blank();
            restored.restore(&saved).expect("restore deleted write");
            assert_eq!(restored.snapshot().expect("same state"), saved);
            send_fdc_bytes(&mut original, &payload[accepted..]);
            send_fdc_bytes(&mut restored, &payload[accepted..]);
            assert_eq!(drain_fdc_result(&mut original), [0x40, 0x80, 0, 1, 0, 1, 2]);
            assert_eq!(drain_fdc_result(&mut restored), [0x40, 0x80, 0, 1, 0, 1, 2]);
            assert_eq!(
                restored.snapshot().expect("continued"),
                original.snapshot().expect("original")
            );
            let exported = restored
                .export_media("disk-a", "edsk")
                .expect("export DDAM");
            let disk = format_amstrad_dsk::parse(&exported).expect("reparse EDSK");
            assert_eq!(disk.tracks[0][0].sectors[0].data, payload);
            assert_eq!(
                (
                    disk.tracks[0][0].sectors[0].st1,
                    disk.tracks[0][0].sectors[0].st2
                ),
                (0, 0x40)
            );
            assert_eq!(restored.disk_images()[0].bytes, source);
            assert_eq!(
                format_amstrad_dsk::parse(&source)
                    .expect("immutable source")
                    .tracks[0][0]
                    .sectors[0]
                    .st2,
                0
            );
            let mut fresh = SpectrumPlus3Runtime::blank();
            let mut media = MediaSet::new();
            media.push(MediaImage::new("disk-a", MediaKind::Disk, &exported));
            fresh.load_media(&media).expect("fresh EDSK mount");
            send_fdc_command(&mut fresh, &[0x4c, 0, 0, 0, 1, 2, 1, 0x2a, 0xff]);
            let observed: Vec<_> = (0..512).map(|_| read_fdc_byte(&mut fresh)).collect();
            assert_eq!(observed, payload);
            assert_eq!(drain_fdc_result(&mut fresh), [0x40, 0x80, 0, 0, 0, 1, 2]);
        }
    }

    #[test]
    fn no_pending_sense_interrupt_snapshot_keeps_one_byte_result() {
        let mut original = SpectrumPlus3Runtime::blank();
        send_fdc_command(&mut original, &[0x08]);
        let saved = original.snapshot().expect("snapshot before ST0");
        let mut restored = SpectrumPlus3Runtime::blank();
        restored.restore(&saved).expect("restore before ST0");
        assert_eq!(restored.snapshot().expect("unchanged encoding"), saved);
        for runtime in [&mut original, &mut restored] {
            assert_eq!(runtime.machine().fdc.read_status(), 0xd0);
            assert_eq!(runtime.machine_mut().fdc.read_data(), 0x80);
            assert_eq!(runtime.machine().fdc.read_status(), 0x80);
            send_fdc_command(runtime, &[4, 0]);
            assert_eq!(runtime.machine().fdc.read_status(), 0xd0);
            runtime.machine_mut().fdc.read_data();
            assert_eq!(runtime.machine().fdc.read_status(), 0x80);
        }
        assert_eq!(
            original.snapshot().expect("uninterrupted"),
            restored.snapshot().expect("resumed")
        );
    }

    #[test]
    fn completed_write_survives_runtime_snapshot_without_replaying_source_bytes() {
        let (mut original, source) = writable_sector_runtime(true);
        let source_before = source.clone();
        let payload: Vec<_> = (0..512).map(|i| (i as u8).wrapping_mul(13)).collect();
        send_fdc_command(&mut original, &[0x45, 0, 0, 0, 1, 2, 1, 0x2a, 0xff]);
        send_fdc_bytes(&mut original, &payload);
        assert_eq!(drain_fdc_result(&mut original), [0x40, 0x80, 0, 1, 0, 1, 2]);
        let saved = original.snapshot().expect("snapshot after write");
        let mut restored = SpectrumPlus3Runtime::blank();
        restored.restore(&saved).expect("restore after write");
        assert_eq!(restored.snapshot().expect("snapshot again"), saved);
        assert_saved_bytes(&mut restored, &payload);
        assert_eq!(
            source, source_before,
            "the input byte buffer remains immutable"
        );
        assert_eq!(
            restored.disk_images()[0].bytes,
            source_before,
            "cached source is provenance, not mutated media"
        );
        send_fdc_command(&mut restored, &[4, 0]);
        assert_eq!(
            restored.machine_mut().fdc.read_data() & 0x60,
            0x20,
            "writable mount survives"
        );
    }

    #[test]
    fn live_export_after_write_and_restore_never_uses_cached_source_bytes() {
        let (mut runtime, source) = writable_sector_runtime(true);
        assert!(runtime.export_media("disk-b", "edsk").is_err());
        assert!(runtime.export_media("disk-a", "dsk").is_err());
        let payload: Vec<_> = (0..512).map(|i| (i as u8).wrapping_mul(17)).collect();
        send_fdc_command(&mut runtime, &[0x45, 0, 0, 0, 1, 2, 1, 0x2a, 0xff]);
        send_fdc_bytes(&mut runtime, &payload[..128]);
        let active = runtime.snapshot().expect("active state");
        assert!(runtime.export_media("disk-a", "edsk").is_err());
        assert_eq!(runtime.snapshot().expect("unchanged active state"), active);
        send_fdc_bytes(&mut runtime, &payload[128..]);
        drain_fdc_result(&mut runtime);
        let saved = runtime.snapshot().expect("completed state");
        let exported = runtime.export_media("disk-a", "edsk").expect("live export");
        assert_eq!(runtime.snapshot().expect("export is read-only"), saved);
        assert_eq!(
            format_amstrad_dsk::parse(&exported)
                .expect("exported image")
                .tracks[0][0]
                .sectors[0]
                .data,
            payload
        );
        assert_ne!(
            format_amstrad_dsk::parse(&source).expect("source").tracks[0][0].sectors[0].data,
            payload
        );
        let mut restored = SpectrumPlus3Runtime::blank();
        restored.restore(&saved).expect("restore");
        assert_eq!(
            restored
                .export_media("disk-a", "edsk")
                .expect("restored live disk"),
            exported
        );
        assert_eq!(restored.disk_images()[0].bytes, source);
        let empty = SpectrumPlus3Runtime::blank();
        assert!(empty.export_media("disk-a", "edsk").is_err());
    }

    #[test]
    fn mid_write_snapshot_resumes_identically_including_byte_deadline() {
        use common_sinclair_zx_spectrum::peripheral::Peripheral;
        let (mut original, _) = writable_sector_runtime(true);
        let payload: Vec<_> = (0..512).map(|i| (i as u8).wrapping_add(71)).collect();
        send_fdc_command(&mut original, &[0x45, 0, 0, 0, 1, 2, 1, 0x2a, 0xff]);
        send_fdc_bytes(&mut original, &payload[..179]);
        original.machine_mut().fdc.write_data(payload[179]);
        for _ in 0..27 {
            original.machine_mut().fdc.tick(0);
        }
        let saved = original.snapshot().expect("mid-write snapshot");
        let mut restored = SpectrumPlus3Runtime::blank();
        restored.restore(&saved).expect("restore mid-write");
        assert_eq!(restored.snapshot().expect("restored snapshot"), saved);
        for _ in 27..114 {
            original.machine_mut().fdc.tick(0);
            restored.machine_mut().fdc.tick(0);
        }
        send_fdc_bytes(&mut original, &payload[180..]);
        send_fdc_bytes(&mut restored, &payload[180..]);
        assert_eq!(
            original.snapshot().expect("uninterrupted"),
            restored.snapshot().expect("resumed")
        );
        assert_eq!(
            drain_fdc_result(&mut original),
            drain_fdc_result(&mut restored)
        );
        assert_saved_bytes(&mut original, &payload);
        assert_saved_bytes(&mut restored, &payload);
    }

    #[test]
    fn released_v0330_controller_payload_is_rejected_before_old_version_decode() {
        // Original synthetic controller captured from 69f80d939; no ROM/CPU/RAM.
        // Prefix its unchanged bytes with the old envelope version. This is
        // deliberately not a complete old envelope: the version guard must
        // reject it before attempting to interpret any payload fields.
        let controller = include_bytes!("../tests/fixtures/upd765-v0330-write.postcard");
        assert_eq!(controller.len(), 1098, "released fixture identity");
        let mut bytes = postcard::to_allocvec(&5_u32).expect("old version encodes");
        bytes.extend_from_slice(controller);
        let (mut runtime, source) = writable_sector_runtime(true);
        let before = runtime.snapshot().expect("current mounted runtime");
        let error = runtime
            .restore(&bytes)
            .expect_err("version 5 cannot represent current read progress");
        match error {
            MachineError::InvalidSnapshot { reason } => {
                assert_eq!(reason, "unsupported snapshot version 5; expected 6");
            }
            other => panic!("expected InvalidSnapshot, got {other:?}"),
        }
        assert_eq!(runtime.snapshot().expect("runtime unchanged"), before);
        assert_eq!(runtime.disk_images()[0].bytes, source);
    }

    fn wait_format(runtime: &mut SpectrumPlus3Runtime, expected: u8) {
        use common_sinclair_zx_spectrum::peripheral::Peripheral;
        for _ in 0..800_000 {
            let status = runtime.machine().fdc.read_status() & 0xf0;
            if status == expected {
                return;
            }
            assert_ne!(status, 0xd0, "unexpected format result");
            runtime.machine_mut().fdc.tick(0);
        }
        panic!("format request/result deadline exceeded");
    }

    fn finish_format(runtime: &mut SpectrumPlus3Runtime, rest: &[u8]) {
        for &byte in rest {
            wait_format(runtime, 0xb0);
            runtime.machine_mut().fdc.write_data(byte);
        }
        wait_format(runtime, 0xd0);
        assert_eq!(drain_fdc_result(runtime), [0, 0, 0, 0, 0, 2, 2]);
        assert_saved_bytes(runtime, &vec![0x6d; 512]);
    }

    #[test]
    fn format_snapshot_resumes_every_id_boundary_and_gap_without_source_replay() {
        use common_sinclair_zx_spectrum::peripheral::Peripheral;
        // Before any ID, between each ID byte, and during the final fill/gap.
        for accepted in 0..=4 {
            let (mut original, source) = writable_sector_runtime(true);
            send_fdc_command(&mut original, &[0x4d, 0, 2, 1, 0x52, 0x6d]);
            let ids = [0, 0, 1, 2];
            for &byte in &ids[..accepted] {
                wait_format(&mut original, 0xb0);
                original.machine_mut().fdc.write_data(byte);
            }
            for _ in 0..27 {
                original.machine_mut().fdc.tick(0);
            }
            let saved = original.snapshot().expect("format checkpoint");
            let mut restored = SpectrumPlus3Runtime::blank();
            restored.restore(&saved).expect("restore format checkpoint");
            assert_eq!(restored.snapshot().expect("re-encode"), saved);
            finish_format(&mut original, &ids[accepted..]);
            finish_format(&mut restored, &ids[accepted..]);
            assert_eq!(
                original.snapshot().expect("uninterrupted"),
                restored.snapshot().expect("resumed")
            );
            assert_eq!(
                restored.disk_images()[0].bytes,
                source,
                "source cache is not the formatted media"
            );
            let completed = original.snapshot().expect("completed format");
            let mut reopened = SpectrumPlus3Runtime::blank();
            reopened
                .restore(&completed)
                .expect("restore completed format");
            assert_saved_bytes(&mut reopened, &vec![0x6d; 512]);
        }
    }

    #[test]
    fn protected_mount_stays_protected_after_restore() {
        let (original, _) = writable_sector_runtime(false);
        let saved = original.snapshot().expect("protected snapshot");
        let mut restored = SpectrumPlus3Runtime::blank();
        restored.restore(&saved).expect("restore protected mount");
        send_fdc_command(&mut restored, &[4, 0]);
        assert_eq!(restored.machine_mut().fdc.read_data() & 0x60, 0x60);
        send_fdc_command(&mut restored, &[0x45, 0, 0, 0, 1, 2, 1, 0x2a, 0xff]);
        assert_eq!(drain_fdc_result(&mut restored), [0x40, 2, 0, 0, 0, 1, 2]);
        assert_saved_bytes(&mut restored, &vec![0xe5; 512]);
    }
}
