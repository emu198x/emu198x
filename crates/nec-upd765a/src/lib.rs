/// NEC µPD765A floppy disk controller.
///
/// Used in the ZX Spectrum +3 (and +2A with the disk drive option).
/// The controller uses a multi-phase protocol:
/// 1. Command phase: CPU writes command bytes
/// 2. Execution phase: data transfer (read/write sectors)
/// 3. Result phase: CPU reads result bytes
///
/// Supports DSK/EDSK disk images. The +3 uses a single-sided 40-track
/// 3" drive (CF-2 format) or a 80-track 3.5" drive.
///
/// Implements `common_sinclair_zx_spectrum::peripheral::Peripheral`
/// so the host machine can dispatch I/O via the shared trait. The
/// `enabled` field gates port claims — machines that instantiate the
/// FDC without wiring it to the bus (Spectrum +2A / +2B, which share
/// the SpectrumPlus struct with the +3) set `enabled = false` at
/// construction so the trait's `claims_port` always returns false.
use common_sinclair_zx_spectrum::peripheral::Peripheral;

/// A floppy sector parsed from a disk image.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DiskSector {
    /// Cylinder (C) as recorded in the sector's address mark. May
    /// differ from the sector's physical track on copy-protected
    /// disks — Speedlock and similar schemes deliberately record
    /// non-matching values to defeat naïve copiers.
    #[serde(default)]
    pub c: u8,
    /// Head (H) as recorded in the address mark.
    #[serde(default)]
    pub h: u8,
    /// Sector ID (R) as it appears in the address mark — this is what
    /// the FDC matches against the sector ID in the read command, and
    /// it is *not* always equal to the physical position on the track.
    pub id: u8,
    /// Sector size code (N): 0 = 128 bytes, 1 = 256, 2 = 512, 3 = 1024.
    /// Copy-protected disks sometimes record N that doesn't match the
    /// actual data length — the FDC reports it via ReadID and the
    /// loader's CRC checks rely on the recorded value.
    #[serde(default = "default_sector_n")]
    pub n: u8,
    /// Recorded ST1 status the FDC would have returned reading this
    /// sector at dump time. Bit 5 (`DE` data error / CRC mismatch in
    /// the data field) is the load-bearing one for protection —
    /// Alkatraz and similar schemes write CRC-broken sectors that
    /// only succeed when the loader explicitly tolerates the error.
    #[serde(default)]
    pub st1: u8,
    /// Recorded ST2 status. Bit 6 (`CM` = Control Mark) is the
    /// DAM/DDAM flag: 0 = standard data address mark, 1 = deleted.
    /// Speedlock writes key sectors with DDAM and reads them back via
    /// the `ReadDeletedData` command (`0x0C`); a `ReadData` (`0x06`)
    /// must either skip these (SK=1 in the command byte) or set
    /// ST2.CM in its own result (SK=0). Bit 5 (`DD` data error in
    /// data field) parallels ST1.DE.
    #[serde(default)]
    pub st2: u8,
    pub data: Vec<u8>,
}

const fn default_sector_n() -> u8 {
    2
}

/// ST2 bit mask for the Control Mark / DAM-vs-DDAM flag. Set when
/// the sector was written with a *deleted* data address mark.
pub const ST2_CM: u8 = 0x40;

/// One physical track on one side of a floppy.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct DiskTrack {
    pub sectors: Vec<DiskSector>,
}

/// A structured floppy image: tracks × sides × sectors.
///
/// Stored in [side][track] order (matching how heads physically address
/// the medium). Sectors within a track are kept in physical order but
/// looked up by their ID.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct DiskImage {
    pub sides: u8,
    pub tracks_per_side: u8,
    /// Indexed as `tracks[side][track]`.
    pub tracks: Vec<Vec<DiskTrack>>,
}

impl DiskImage {
    /// Look up a sector by physical (track, side) and logical sector ID.
    pub fn sector(&self, track: u8, side: u8, sector_id: u8) -> Option<&DiskSector> {
        let side_tracks = self.tracks.get(side as usize)?;
        let trk = side_tracks.get(track as usize)?;
        trk.sectors.iter().find(|s| s.id == sector_id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
enum Phase {
    Idle,
    Command,
    Execution,
    Result,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
enum Command {
    None,
    ReadData,
    /// `ReadDeletedData` is the disk-protection cousin of `ReadData`:
    /// same parameters, same result phase, same per-sector data
    /// delivery, but matches sectors written with the *deleted* data
    /// address mark (DDAM) rather than the standard DAM. Speedlock
    /// (Ocean's +3 disk protection) writes its key sectors with DDAM
    /// and reads them back via this opcode; a `ReadData` issued
    /// against a DDAM sector would either skip it (SK=1) or flag
    /// ST2.CM (SK=0), neither of which delivers the bytes Speedlock's
    /// loader expects. Operation Wolf, RoboCop, and Where Time Stood
    /// Still all stop at the +3 BIOS empty screen if the chip drops
    /// this command on the floor.
    ReadDeletedData,
    WriteData,
    ReadId,
    Recalibrate,
    SenseInterruptStatus,
    Specify,
    SeekTrack,
    SenseDriveStatus,
    // Append only: existing postcard command tags must remain stable.
    FormatTrack,
    ReadTrack,
    Scan,
}

/// µPD765A state.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Upd765a {
    phase: Phase,
    command: Command,

    // Command buffer
    cmd_buf: Vec<u8>,
    cmd_len: usize,

    // Result buffer
    result_buf: Vec<u8>,
    result_pos: usize,

    // Execution buffer (sector data)
    exec_buf: Vec<u8>,
    exec_pos: usize,

    // Drive state (4 drives max, +3 typically uses 1)
    /// Physical head positions, selected through the external drive mask.
    track: [u8; 4],
    head: u8,
    sector: u8,

    // Status registers
    st0: u8,
    st1: u8,
    st2: u8,
    st3: u8,

    /// Main status register (read from port 2FFD).
    main_status: u8,

    /// Interrupt pending.
    pub interrupt: bool,

    /// Per-drive seek interrupt pending. The +3 BIOS issues
    /// Recalibrate or Seek across multiple drives in a row, then drains
    /// the resulting Seek End interrupts via repeated
    /// `SenseInterruptStatus` calls. The handler walks the drives in
    /// order and returns the first pending interrupt's ST0 + PCN; once
    /// every drive's seek-pending bit is cleared, subsequent calls
    /// return `ST0 = 0x80` (Invalid Command) per the µPD765A datasheet,
    /// which is how the BIOS knows the queue is drained.
    seek_pending: [Option<u8>; 4],

    /// Per-drive seek-completion countdown in `tick()` calls. While
    /// non-zero the corresponding drive-busy bit (`1 << drive`) is OR'd
    /// into the main status register's read value; the BIOS polls this
    /// to wait for a Seek/Recalibrate to finish before issuing
    /// `SenseInterruptStatus`. The staged ST0 lives in
    /// `seek_staged_st0` until the countdown reaches zero, at which
    /// point it moves into `seek_pending` and the busy bit clears.
    seek_remaining: [u32; 4],

    /// ST0 staged for each drive's in-flight Seek/Recalibrate. Moves
    /// into `seek_pending[drive]` when `seek_remaining[drive]` reaches
    /// zero.
    seek_staged_st0: [Option<u8>; 4],

    /// Is this FDC electrically wired to the host's I/O bus?
    /// True on +3, false on +2A / +2B — both share the SpectrumPlus
    /// struct but only the +3 has an actual drive connector.
    pub enabled: bool,

    /// AND-mask applied to the unit-select bits of every command byte
    /// before indexing `disks[]`.
    ///
    /// Standalone µPD765A wires US0 + US1 (4 drives, mask = `0b11`).
    /// The Spectrum +3 only routes US0 to its drive selector, so US1
    /// is electrically a don't-care and drive byte `0x02` (US1=1) is
    /// indistinguishable from `0x00` (US1=0) on the actual hardware
    /// — i.e. drive 2 aliases drive 0 and drive 3 aliases drive 1.
    /// The +3 BIOS's second-stage loader (after the boot sector
    /// runs) issues `ReadData` with US1=1 deliberately; without this
    /// mask we'd see "drive 2 not present" and the Loader would
    /// abort. FUSE's `specplus3_765_init` documents the same wiring
    /// quirk explicitly. Set to `0x01` on +3 via
    /// [`Upd765a::set_drive_select_mask`].
    #[serde(default = "default_drive_select_mask")]
    drive_select_mask: u8,

    /// Mounted images are live state, including bytes changed by Write Data.
    disks: [Option<DiskImage>; 4],
    /// Write protection is a mount property, independent of drive readiness.
    writable: [bool; 4],
    /// An initial transfer preamble or write/format inter-sector interval is in progress.
    write_start_pending: bool,

    /// Last sector successfully read via `ReadData` / `ReadDeletedData`,
    /// keyed by `(drive, track, head, sector_id)`. Used to detect
    /// consecutive re-reads of the same sector and drive the marginal-
    /// encoding model on sectors whose recorded ST1.DE / ST2.DD flags
    /// the medium is marginal. See `knowledge/decisions/marginal-encoding-model.md`.
    #[serde(default)]
    reread_key: Option<(usize, u8, u8, u8)>,

    /// Number of consecutive re-reads of `reread_key` after the first.
    /// Zero on the first read of a sector; increments by one each time
    /// the same sector is read again with no intervening read of a
    /// different sector. Used as the variation parameter in the
    /// marginal-encoding model.
    #[serde(default)]
    reread_count: u32,

    /// Per-drive rotational position used by `ReadID`. Real µPD765A
    /// returns whichever sector's address mark is currently passing
    /// under the head, so successive `ReadID` calls without any
    /// intervening Seek return *different* sectors in the order they
    /// were written to the track. Some protection schemes (Tetris's
    /// format-track-12, Turrican's track 1 probe) verify this by
    /// reading N IDs in a row and checking the sequence matches an
    /// expected layout. We approximate the rotational position with
    /// a per-drive counter that increments on each `ReadID` call and
    /// resets on Recalibrate or Seek.
    #[serde(default)]
    read_id_index: [usize; 4],

    /// Transfer countdown in host CPU T-state ticks. During a read, RQM
    /// distinguishes a pending byte deadline from the next byte's arrival.
    /// Reads do not restart this clock: late service cannot shift the stream.
    #[serde(default)]
    exec_timeout: u32,
    /// Only the selected field is buffered. CRC status is committed after its
    /// payload completes; unread fields cannot affect results or reread state.
    read_progress: Option<ReadProgress>,
    /// Optional external READY levels. None retains the legacy image-presence
    /// input for callers without a drive adapter. +3 always supplies its input.
    ready_inputs: [Option<bool>; 4],
    ready_sampled: [bool; 4],
    /// Rational phase at the existing +3 CPU T-state boundary. Each drive is
    /// sampled every8192 FDC clocks:2.048ms at4MHz (NEC A/B polling section).
    ready_poll_phase: u64,
    /// Seek IRQ must be acknowledged by SIS before another command (NEC p16).
    seek_attention_unacknowledged: bool,
    /// Four internal PCNs exist even when external US1 is not connected.
    /// Spectrum version-7 snapshots preserve these independently of head position.
    pcn: [u8; 4],
    /// Logical TS sample supplied by machine wiring, independent of image
    /// geometry. None retains the legacy input for existing chip consumers.
    two_side_inputs: [Option<bool>; 4],
    /// Independent logical samples for SDS. None retains legacy media/head
    /// fallbacks; these samples do not model multiplexed pin sampling timing.
    track_zero_inputs: [Option<bool>; 4],
    write_protect_inputs: [Option<bool>; 4],
    fault_inputs: [Option<bool>; 4],
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct ReadProgress {
    completed: u16,
    pending_st1: u8,
    pending_st2: u8,
}

const fn default_drive_select_mask() -> u8 {
    0x03
}

// Main status register bits
const MSR_CB: u8 = 0x10; // Controller busy
const MSR_EXM: u8 = 0x20; // Execution mode
const MSR_DIO: u8 = 0x40; // Data direction (1 = FDC → CPU)
const MSR_RQM: u8 = 0x80; // Request for master (ready for data)

/// Per-tick countdown for staged Seek/Recalibrate completion. Real
/// µPD765A step rates land in the millisecond range per cylinder; at
/// the +3's ~887 KHz T-state rate that's hundreds of T-states per
/// step. We use a single short countdown rather than modelling the
/// step-by-step process — long enough for a BIOS that polls MSR's
/// drive-busy bits to observe the busy → idle transition, short
/// enough that catalogue tests don't wait perceptibly.
const SEEK_TICKS: u32 = 256;

/// Diagnostic: when `EMU198X_FDC_TRACE` is set in the environment, log
/// every dispatched command and every result-phase byte block to
/// stderr. Speedlock loaders erase their own decision code after a
/// failed protection check, so a post-hoc memory dump can't recover
/// what the loader was checking; this side-channel trace captures the
/// FDC conversation directly. Off by default — zero cost when the env
/// var is unset.
fn fdc_trace_enabled() -> bool {
    std::env::var_os("EMU198X_FDC_TRACE").is_some()
}

fn fmt_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Sector-data variation modelling marginal magnetic encoding.
///
/// Applied to bytes coming off a sector whose recorded `ST1.DE` or
/// `ST2.DD` is set — the dumper observed the real µPD765A's read
/// amplifier failing to resolve the flux transitions cleanly on this
/// sector, which is what makes Speedlock-class protections detectable.
/// On real hardware the same sector returns different bytes each read
/// because the analog front-end's hysteresis flips on noise. We model
/// that physics with a deterministic per-re-read variation so save-
/// states round-trip and tests stay reproducible.
///
/// The recipe is taken verbatim from FUSE `peripherals/disk/upd_fdc.c`
/// (lines 1001-1010 in fuse-1.7.0), the closest-to-canonical reference
/// for what byte variation satisfies the relevant protection checks
/// across the Speedlock catalogue. See
/// `knowledge/decisions/marginal-encoding-model.md` for the rationale.
///
/// `count` is the re-read counter (zero on first read; this function
/// is a no-op then). Variation is applied to bytes at offsets that are
/// multiples of 29, scoped to the first 64 bytes unless `count >= 2`,
/// in which case the whole sector is mangled.
fn apply_marginal_variation(buf: &mut [u8], count: u32) {
    if count == 0 {
        return;
    }
    let aggressive = count >= 2;
    for (i, byte) in buf.iter_mut().enumerate() {
        if i % 29 != 0 {
            continue;
        }
        if !aggressive && i >= 64 {
            break;
        }
        *byte ^= (i & 0xFF) as u8;
    }
}

impl Upd765a {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            command: Command::None,
            cmd_buf: Vec::with_capacity(16),
            cmd_len: 0,
            result_buf: Vec::with_capacity(16),
            result_pos: 0,
            exec_buf: Vec::new(),
            exec_pos: 0,
            track: [0; 4],
            head: 0,
            sector: 0,
            st0: 0,
            st1: 0,
            st2: 0,
            st3: 0,
            main_status: MSR_RQM,
            interrupt: false,
            seek_pending: [None, None, None, None],
            seek_remaining: [0; 4],
            seek_staged_st0: [None, None, None, None],
            enabled: false,
            drive_select_mask: default_drive_select_mask(),
            disks: [None, None, None, None],
            writable: [false; 4],
            write_start_pending: false,
            reread_key: None,
            reread_count: 0,
            exec_timeout: 0,
            read_progress: None,
            pcn: [0; 4],
            read_id_index: [0; 4],
            ready_inputs: [None; 4],
            two_side_inputs: [None; 4],
            track_zero_inputs: [None; 4],
            write_protect_inputs: [None; 4],
            fault_inputs: [None; 4],
            ready_sampled: [false; 4],
            ready_poll_phase: 0,
            seek_attention_unacknowledged: false,
        }
    }

    /// Mask the unit-select bits of every command byte against `mask`
    /// before indexing the per-drive state. Use `0x01` on the +3 (only
    /// US0 wired) or leave the default `0x03` (full µPD765A,
    /// US0+US1 → 4 drives).
    pub fn set_drive_select_mask(&mut self, mask: u8) {
        self.drive_select_mask = mask & 0x03;
    }

    /// Supply a logical asserted READY input, independent of mounted data.
    /// This sets a level; polling between commands creates attention events.
    /// Unknown units are ignored, matching the existing media attachment API.
    pub fn set_ready(&mut self, drive: usize, ready: bool) {
        if let Some(input) = self.ready_inputs.get_mut(drive) {
            *input = Some(ready);
        }
    }

    /// Supply the asserted Two Side level sampled by Sense Drive Status.
    /// This is a drive-interface input, not the number of heads in an image.
    /// Invalid drives are ignored, as for media and READY attachment.
    pub fn set_two_side(&mut self, drive: usize, two_side: bool) {
        if let Some(input) = self.two_side_inputs.get_mut(drive) {
            *input = Some(two_side);
        }
    }

    /// Supply the asserted Track Zero sample for Sense Drive Status.
    /// This is independent of PCN and media presence. Seek/recalibration retain
    /// the current instantaneous sector-model motion, not pin-paced mechanics.
    /// Invalid drives are ignored, matching READY/TS attachment.
    pub fn set_track_zero(&mut self, drive: usize, track_zero: bool) {
        if let Some(input) = self.track_zero_inputs.get_mut(drive) {
            *input = Some(track_zero);
        }
    }

    /// Supply the asserted Write Protect sample for Sense Drive Status.
    /// Asserted protection also inhibits disk mutations. Deasserting this input
    /// cannot override the host's read-only mount policy. Invalid drives are ignored.
    pub fn set_write_protect(&mut self, drive: usize, protected: bool) {
        if let Some(input) = self.write_protect_inputs.get_mut(drive) {
            *input = Some(protected);
        }
    }

    /// Supply the asserted Fault sample for Sense Drive Status.
    /// This does not implement fault-reset signalling or mechanical failures.
    /// Invalid drives are ignored, matching READY/TS attachment.
    pub fn set_fault(&mut self, drive: usize, fault: bool) {
        if let Some(input) = self.fault_inputs.get_mut(drive) {
            *input = Some(fault);
        }
    }

    /// Whether the sector model's physical head counter is at cylinder zero.
    /// A machine adapter must separately establish that a mechanism exists;
    /// an unused counter being zero is not an asserted Track Zero sensor.
    #[must_use]
    pub fn head_at_track_zero(&self, drive: usize) -> bool {
        self.track.get(drive).is_some_and(|track| *track == 0)
    }

    fn writes_inhibited(&self, drive: usize) -> bool {
        !self.writable[drive] || self.write_protect_inputs[drive] == Some(true)
    }

    /// The existing sector-model Write Protect level for a physical drive.
    /// Machines may wire this level to another controller input (+3 pin34).
    /// Empty and invalid slots return false; this does not model an empty
    /// physical drive's electrical sensors.
    #[must_use]
    pub fn is_write_protected(&self, drive: usize) -> bool {
        self.has_disk(drive) && !self.writable[drive]
    }

    fn drive_ready(&self, drive: usize) -> bool {
        self.ready_inputs[drive]
            .unwrap_or(self.disks[drive & usize::from(self.drive_select_mask)].is_some())
    }

    fn poll_ready(&mut self) {
        // A/B datasheet:2.048ms per drive with4MHz FDC. Accumulate the
        // master-derived host T-state fraction rather than round every period.
        let period = 2048 * common_sinclair_zx_spectrum::timing::CPU_HZ_128K;
        self.ready_poll_phase += 1_000_000;
        if self.ready_poll_phase < period {
            return;
        }
        self.ready_poll_phase -= period;
        if self.phase != Phase::Idle {
            return;
        }
        for drive in 0..4 {
            let ready = self.drive_ready(drive);
            if ready != self.ready_sampled[drive] {
                self.ready_sampled[drive] = ready;
                // Preserve the first undrained event for a drive. MAME's
                // st0_filled latch supplies independently inspected precedent.
                if self.seek_pending[drive].is_none() {
                    self.seek_pending[drive] = Some(0xc0 | drive as u8 | if ready { 0 } else { 8 });
                    self.interrupt = true;
                }
            }
        }
    }

    /// Insert a parsed disk image into a drive.
    pub fn insert_disk(&mut self, drive: usize, image: DiskImage) {
        self.insert_disk_with_writable(drive, image, false);
    }

    /// Mount a private in-memory image with explicit write permission.
    /// No operation on the controller writes to a host filesystem path.
    pub fn insert_disk_with_writable(&mut self, drive: usize, image: DiskImage, writable: bool) {
        if drive < 4 {
            self.abort_for_media_change(drive);
            self.disks[drive] = Some(image);
            self.writable[drive] = writable;
            self.reread_key = None;
            self.reread_count = 0;
            self.read_id_index[drive] = 0;
        }
    }

    /// Re-mount an image during a save-state restore, preserving the
    /// controller's cached read state.
    ///
    /// The counterpart to [`insert_disk`](Self::insert_disk), and the
    /// distinction is the point. Inserting a disk is a physical event:
    /// the medium under the head changed, so the re-read key, re-read
    /// count and per-drive `ReadID` rotational position are all stale
    /// and must be invalidated. Restoring a snapshot is not a physical
    /// event — the image being re-mounted is the *same* image the
    /// cached state describes, and that state was faithfully carried in
    /// the snapshot.
    ///
    /// Legacy attachment helper for consumers restoring an image separately.
    /// Spectrum version-6 snapshots serialize live images directly and do not
    /// call this helper: replaying an old source image would erase writes.
    pub fn reattach_disk(&mut self, drive: usize, image: DiskImage) {
        if drive < 4 {
            self.disks[drive] = Some(image);
        }
    }

    pub fn eject_disk(&mut self, drive: usize) {
        if drive < 4 {
            self.abort_for_media_change(drive);
            self.disks[drive] = None;
            self.writable[drive] = false;
            self.reread_key = None;
            self.reread_count = 0;
            self.read_id_index[drive] = 0;
        }
    }

    /// Returns `true` if the given drive currently has a disk image
    /// mounted. Used by snapshot-restore regression tests to verify
    /// the disk survives the round-trip.
    #[must_use]
    pub fn has_disk(&self, drive: usize) -> bool {
        drive < 4 && self.disks[drive].is_some()
    }

    /// Borrow the authoritative live image for an explicit host export.
    ///
    /// # Errors
    /// Rejects unknown/empty drives and active disk-writing commands. Export
    /// must not imply that an in-flight sector or format operation completed.
    /// This does not advance time, flush, abort or otherwise change a transfer.
    pub fn disk_for_export(&self, drive: usize) -> Result<&DiskImage, &'static str> {
        let image = self
            .disks
            .get(drive)
            .ok_or("unknown disk drive")?
            .as_ref()
            .ok_or("no disk mounted")?;
        if self.phase == Phase::Execution
            && matches!(self.command, Command::WriteData | Command::FormatTrack)
        {
            return Err("finish the active disk write or format before exporting");
        }
        Ok(image)
    }

    /// Reset controller registers and transfers, retaining inserted media and
    /// its physical write-protection setting.
    pub fn reset(&mut self) {
        let disks = std::mem::take(&mut self.disks);
        let writable = self.writable;
        let enabled = self.enabled;
        let drive_select_mask = self.drive_select_mask;
        let ready_inputs = self.ready_inputs;
        let two_side_inputs = self.two_side_inputs;
        let track_zero_inputs = self.track_zero_inputs;
        let write_protect_inputs = self.write_protect_inputs;
        let fault_inputs = self.fault_inputs;
        let track = self.track;
        let pcn = self.pcn;
        *self = Self::new();
        self.disks = disks;
        self.writable = writable;
        self.enabled = enabled;
        self.drive_select_mask = drive_select_mask;
        self.ready_inputs = ready_inputs;
        self.two_side_inputs = two_side_inputs;
        self.track_zero_inputs = track_zero_inputs;
        self.write_protect_inputs = write_protect_inputs;
        self.fault_inputs = fault_inputs;
        self.track = track;
        self.pcn = pcn;
    }

    /// Repair active ID-error reads captured by earlier version-6 snapshots.
    ///
    /// Those snapshots could already advertise payload from a known bad ID.
    /// Call after deserialization, before exposing registers to the host. A
    /// completed result, Read Track, and valid active transfers stay unchanged.
    pub fn after_restore(&mut self) {
        if self.phase == Phase::Execution
            && matches!(self.command, Command::ReadData | Command::ReadDeletedData)
            && self.read_progress.as_ref().is_some_and(|progress| {
                progress.pending_st1 & 0x20 != 0 && progress.pending_st2 & 0x20 == 0
            })
        {
            self.st1 |= 0x20;
            // The old path may already have visited DAM. An invalid ID must
            // terminate before that mark can contribute CM to this result.
            self.st2 &= !ST2_CM;
            self.read_result();
        }
    }

    /// Read the main status register (port $2FFD on +3).
    ///
    /// Bits D0..D3 reflect per-drive seek-busy state (set while a
    /// Seek/Recalibrate is in flight on that drive, cleared on
    /// completion). The BIOS polls this register to wait for a seek
    /// before issuing `SenseInterruptStatus`.
    pub fn read_status(&self) -> u8 {
        let mut msr = self.main_status;
        for unit in 0..4 {
            if self.seek_remaining[unit] > 0
                || self.seek_pending[unit].is_some_and(|st0| st0 & 0x20 != 0)
            {
                msr |= 1 << unit;
            }
        }
        // Completing motion does not acknowledge a Seek/Recalibrate. The
        // selected logical unit stays busy until SIS reads ST0, while PCN can
        // remain unread (NEC A/B Seek; MAME fifo_r first-result handling).
        if self.phase == Phase::Result
            && self.command == Command::SenseInterruptStatus
            && self.result_pos == 0
            && self.result_buf.len() == 2
            && self.result_buf[0] & 0x20 != 0
        {
            msr |= 1 << (self.result_buf[0] & 0x03);
        }
        msr
    }

    /// Read data register (port $3FFD on +3).
    pub fn read_data(&mut self) -> u8 {
        match self.phase {
            Phase::Execution
                if matches!(
                    self.command,
                    Command::ReadData | Command::ReadDeletedData | Command::ReadTrack
                ) && self.exec_pos < self.exec_buf.len()
                    && self.main_status & MSR_RQM != 0 =>
            {
                let byte = self.exec_buf[self.exec_pos];
                self.exec_pos += 1;
                self.main_status &= !MSR_RQM;
                // Keep the outstanding byte-clock boundary. Host latency must
                // not delay the following byte or extend its service window.
                if self.exec_pos >= self.exec_buf.len() {
                    self.finish_read_sector();
                }
                byte
            }
            Phase::Result => {
                if self.result_pos < self.result_buf.len() {
                    let byte = self.result_buf[self.result_pos];
                    self.result_pos += 1;
                    if self.result_pos >= self.result_buf.len() {
                        self.phase = Phase::Idle;
                        self.main_status = MSR_RQM;
                    }
                    byte
                } else {
                    self.phase = Phase::Idle;
                    self.main_status = MSR_RQM;
                    0xFF
                }
            }
            _ => 0xFF,
        }
    }

    /// Write data register (port $3FFD on +3).
    pub fn write_data(&mut self, val: u8) {
        match self.phase {
            Phase::Idle => {
                // First byte of a new command
                self.cmd_buf.clear();
                self.cmd_buf.push(val);
                let (cmd, len) = if self.seek_attention_unacknowledged && val & 0x1f != 8 {
                    // NEC Dec1978p16 and A/B Invalid section require SIS.
                    // Fuse deliberately relaxes this for a compatibility case;
                    // preserve that disagreement rather than imply conformance.
                    (Command::None, 1)
                } else {
                    Self::decode_command(val)
                };
                self.command = cmd;
                self.cmd_len = len;

                if self.cmd_buf.len() >= self.cmd_len {
                    self.execute_command();
                } else {
                    self.phase = Phase::Command;
                    self.main_status = MSR_RQM | MSR_CB;
                }
            }
            Phase::Command => {
                self.cmd_buf.push(val);
                if self.cmd_buf.len() >= self.cmd_len {
                    self.execute_command();
                }
            }
            Phase::Execution => {
                if self.command == Command::Scan && self.main_status & MSR_RQM != 0 {
                    self.accept_scan_byte(val);
                } else if self.command == Command::WriteData && self.main_status & MSR_RQM != 0 {
                    self.accept_write_byte(val);
                } else if self.command == Command::FormatTrack && self.main_status & MSR_RQM != 0 {
                    self.accept_format_byte(val);
                }
            }
            Phase::Result => {}
        }
    }

    fn decode_command(byte: u8) -> (Command, usize) {
        match byte & 0x1F {
            0x11 | 0x19 | 0x1d => (Command::Scan, 9),
            0x02 => (Command::ReadTrack, 9),
            0x06 => (Command::ReadData, 9),        // Read Data
            0x0C => (Command::ReadDeletedData, 9), // Read Deleted Data (Speedlock keys)
            0x0D => (Command::FormatTrack, 6),
            0x05 | 0x09 => (Command::WriteData, 9), // Write Data
            0x0A => (Command::ReadId, 2),           // Read ID
            0x07 => (Command::Recalibrate, 2),      // Recalibrate
            0x08 => (Command::SenseInterruptStatus, 1), // Sense Interrupt Status
            0x03 => (Command::Specify, 3),          // Specify
            0x0F => (Command::SeekTrack, 3),        // Seek
            0x04 => (Command::SenseDriveStatus, 2), // Sense Drive Status
            _ => (Command::None, 1),
        }
    }

    fn execute_command(&mut self) {
        if fdc_trace_enabled() {
            eprintln!(
                "[FDC-CMD] {:?} cmd_buf=[{}]",
                self.command,
                fmt_bytes(&self.cmd_buf),
            );
            // For ReadID, also dump the recorded sector layout of the
            // current track on the current drive — that's the only way
            // to know what real-hardware ReadID could possibly have
            // returned, since the chip returns sectors in rotational
            // order.
            if matches!(self.command, Command::ReadId) {
                let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
                let head = (self.cmd_buf[1] >> 2) & 0x01;
                if let Some(img) = self.disks.get(drive).and_then(|d| d.as_ref())
                    && let Some(side) = img.tracks.get(head as usize)
                    && let Some(trk) = side.get(self.track[drive] as usize)
                {
                    eprintln!(
                        "[FDC-TRACK] drive={drive} head={head} track={} sector_count={}",
                        self.track[drive],
                        trk.sectors.len(),
                    );
                    for (idx, s) in trk.sectors.iter().enumerate() {
                        eprintln!(
                            "  [{idx}] c={:#04x} h={:#04x} r={:#04x} n={:#04x} st1={:#04x} st2={:#04x} data_len={}",
                            s.c,
                            s.h,
                            s.id,
                            s.n,
                            s.st1,
                            s.st2,
                            s.data.len(),
                        );
                    }
                }
            }
        }
        match self.command {
            Command::Scan => self.start_scan(),
            Command::ReadTrack => self.start_read_track(),
            Command::FormatTrack => self.start_format_track(),
            Command::WriteData => {
                self.st0 = self.cmd_buf[1] & 7;
                self.st1 = 0;
                self.st2 = 0;
                self.head = (self.cmd_buf[1] >> 2) & 1;
                self.sector = self.cmd_buf[4];
                self.start_write_sector();
            }
            Command::ReadData | Command::ReadDeletedData => self.start_read(),
            Command::Recalibrate => {
                let unit = usize::from(self.cmd_buf[1] & 0x03);
                let drive_echo = self.cmd_buf[1] & 0x03;
                let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
                self.track[drive] = 0;
                self.pcn[unit] = 0;
                // Initially-not-ready Recalibrate terminates abnormally with
                // NR and SE, as for Seek (NEC December 1978, pp. 15–16;
                // MAME recalibrate_start). It is not a READY-change event or
                // the distinct 77-step failure to find TRACK 0.
                let st0 = if self.drive_ready(drive) {
                    0x20 | drive_echo // Seek End | drive
                } else {
                    0x68 | drive_echo // Abnormal | Seek End | Not Ready | drive
                };
                self.st0 = st0;
                // Stage the seek; the busy bit is read out of MSR via
                // `seek_remaining`, and the interrupt is queued only
                // after the countdown completes via `tick()`.
                self.seek_staged_st0[unit] = Some(st0);
                self.seek_remaining[unit] = SEEK_TICKS;
                self.phase = Phase::Idle;
                self.main_status = MSR_RQM;
            }
            Command::SenseInterruptStatus => {
                self.result_buf.clear();
                if let Some(drive) = (0..4).find(|d| self.seek_pending[*d].is_some()) {
                    // Drain one pending seek interrupt.
                    let st0 = self.seek_pending[drive]
                        .take()
                        .expect("the find above matched seek_pending[drive].is_some()");
                    self.st0 = st0;
                    self.result_buf.push(st0);
                    self.result_buf.push(self.pcn[drive]);
                } else {
                    // No pending completion: use the existing Invalid Command
                    // classification. That result contains ST0 only (NEC,
                    // December 1978, p. 9); it has no PCN byte. Fuse/lib765/MAME
                    // also classify no-pending Sense Interrupt this way.
                    self.st0 = 0x80;
                    self.result_buf.push(0x80);
                }
                self.result_pos = 0;
                self.phase = Phase::Result;
                self.main_status = MSR_RQM | MSR_DIO | MSR_CB;
                // SIS acknowledges the interrupt signal, even when another
                // drive still has status to drain (NEC December 1978, p.16).
                // A later completion will raise the signal again.
                self.interrupt = false;
                self.seek_attention_unacknowledged = false;
                if fdc_trace_enabled() {
                    eprintln!(
                        "[FDC-RES] SenseInt result=[{}]",
                        fmt_bytes(&self.result_buf),
                    );
                }
            }
            Command::Specify => {
                // Just accept the parameters (step rate, head load/unload times)
                self.phase = Phase::Idle;
                self.main_status = MSR_RQM;
            }
            Command::SeekTrack => {
                let unit = usize::from(self.cmd_buf[1] & 0x03);
                let drive_echo = self.cmd_buf[1] & 0x07;
                let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
                // Use the same image-presence readiness model as ST3.RY.
                // NEC December 1978 datasheet p15 specifies IC=01|NR
                // for initially-not-ready Seek. MAME's seek_start also
                // sets SE and preserves PCN; Fuse instead uses IC=11.
                // READY changes during Seek and physical step timing
                // remain outside this staged completion model.
                let st0 = if self.drive_ready(drive) {
                    // Step from this logical unit's PCN. External US1 may
                    // alias another unit onto the same physical mechanism.
                    let target = self.cmd_buf[2];
                    let delta = i16::from(target) - i16::from(self.pcn[unit]);
                    let position = (i16::from(self.track[drive]) + delta).clamp(0, 255);
                    self.track[drive] = position as u8;
                    self.pcn[unit] = target;
                    0x20 | drive_echo // Seek End | drive
                } else {
                    0x68 | drive_echo // Abnormal | Seek End | Not Ready | drive
                };
                self.st0 = st0;
                self.seek_staged_st0[unit] = Some(st0);
                self.seek_remaining[unit] = SEEK_TICKS;
                self.phase = Phase::Idle;
                self.main_status = MSR_RQM;
            }
            Command::SenseDriveStatus => {
                let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
                let head = (self.cmd_buf[1] >> 2) & 0x01;
                let disk_present = self.disks[drive].is_some();
                self.st3 = (self.cmd_buf[1] & 0x07)        // US0/US1/HD copied from command
                    | if self.track_zero_inputs[drive].unwrap_or(self.head_at_track_zero(drive)) { 0x10 } else { 0 }
                    | if self.two_side_inputs[drive].unwrap_or(disk_present) { 0x08 } else { 0 }
                    | if self.drive_ready(drive) { 0x20 } else { 0 } // RY
                    | if self.write_protect_inputs[drive].unwrap_or(self.is_write_protected(drive)) { 0x40 } else { 0 }
                    | if self.fault_inputs[drive].unwrap_or(false) { 0x80 } else { 0 };
                self.head = head;
                self.result_buf.clear();
                self.result_buf.push(self.st3);
                self.result_pos = 0;
                self.phase = Phase::Result;
                self.main_status = MSR_RQM | MSR_DIO | MSR_CB;
                if fdc_trace_enabled() {
                    eprintln!(
                        "[FDC-RES] SenseDriveStatus result=[{}]",
                        fmt_bytes(&self.result_buf),
                    );
                }
            }
            Command::ReadId => {
                // Return the address-mark CHRN of the sector currently
                // under the head, then advance the per-drive
                // rotational index so successive `ReadID` calls walk
                // the track. Tetris's track 12 and Turrican's track 1
                // both use a multi-`ReadID` rotation check as part of
                // their protection — they read N consecutive IDs and
                // confirm the sequence matches an expected layout. A
                // chip that always returned sectors[0] from `ReadID`
                // would fail those checks; a chip that rotates passes
                // them, matching real silicon behaviour.
                let drive_echo = self.cmd_buf[1] & 0x03;
                let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
                let head = (self.cmd_buf[1] >> 2) & 0x01;
                self.st1 = 0;
                self.st2 = 0;

                let track_sectors = self
                    .disks
                    .get(drive)
                    .and_then(|d| d.as_ref())
                    .and_then(|img| img.tracks.get(head as usize))
                    .and_then(|side| side.get(self.track[drive] as usize))
                    .map(|trk| &trk.sectors[..]);

                self.st0 = (head << 2) | drive_echo;
                let (c, h, r, n) = match track_sectors {
                    Some(sectors) if !sectors.is_empty() => {
                        let idx = self.read_id_index[drive] % sectors.len();
                        let s = &sectors[idx];
                        // DE without DD represents an ID-field CRC error.
                        // Read ID never visits the data field: data CRC,
                        // deleted marks and other data status cannot leak in.
                        // NEC Dec 1978 pp17–18: ID CRC sets DE and ND;
                        // Fuse read_id/start_read_id also reports abnormal.
                        // DE+DD cannot establish an additional bad ID here.
                        if s.st1 & 0x20 != 0 && s.st2 & 0x20 == 0 {
                            self.st0 |= 0x40;
                            self.st1 = 0x24;
                        }
                        self.read_id_index[drive] = self.read_id_index[drive].wrapping_add(1);
                        (s.c, s.h, s.id, s.n)
                    }
                    _ => {
                        // NEC December 1978 data sheet, p14 (Read ID),
                        // pp17–18 (ST0/ST1): no readable ID terminates
                        // abnormally with MA and ND. This sector-image
                        // model does not time the two-index-pulse search
                        // or model drive-ready pins. CHRN is unspecified
                        // on this error; retain the existing filler bytes.
                        self.st0 |= 0x40;
                        self.st1 = 0x05;
                        (self.track[drive], head, 1, 2)
                    }
                };

                self.result_buf.clear();
                self.result_buf.push(self.st0);
                self.result_buf.push(self.st1);
                self.result_buf.push(self.st2);
                self.result_buf.push(c);
                self.result_buf.push(h);
                self.result_buf.push(r);
                self.result_buf.push(n);
                self.result_pos = 0;
                self.phase = Phase::Result;
                self.main_status = MSR_RQM | MSR_DIO | MSR_CB;
                if fdc_trace_enabled() {
                    eprintln!("[FDC-RES] ReadId result=[{}]", fmt_bytes(&self.result_buf),);
                }
            }
            Command::None => {
                // NEC December 1978 datasheet pp.9,16: an invalid
                // command has a one-byte ST0 result (IC=10). Fuse's
                // UPD_CMD_INVALID handling likewise returns 0x80.
                self.st0 = 0x80;
                self.result_buf.clear();
                self.result_buf.push(self.st0);
                self.result_pos = 0;
                self.phase = Phase::Result;
                self.main_status = MSR_RQM | MSR_DIO | MSR_CB;
            }
        }
    }

    // NEC December 1978 p15/Table4, Fuse upd_fdc_write_data: inequalities
    // compare disk against processor for every byte. A violating byte latches
    // SN for the whole sector; a later byte cannot undo it. FF is literal in
    // the held NEC/Fuse contract; no undocumented wildcard is invented.
    fn start_scan(&mut self) {
        self.st0 = self.cmd_buf[1] & 7;
        self.st1 = 0;
        self.st2 = 0;
        self.head = (self.cmd_buf[1] >> 2) & 1;
        self.sector = self.cmd_buf[4];
        if self.cmd_buf[5] > 7 || !matches!(self.cmd_buf[8], 1 | 2) {
            self.write_result(4, 0);
            return;
        }
        self.start_scan_sector();
    }

    fn start_scan_sector(&mut self) {
        let drive = usize::from(self.cmd_buf[1] & self.drive_select_mask);
        // At most 256 IDs on each of two heads. Skipped fields make progress
        // here without requesting fictitious host comparison bytes.
        loop {
            let Some(image) = &self.disks[drive] else {
                self.st0 |= 0x08;
                self.write_result(0, self.st2 & ST2_CM);
                return;
            };
            let sectors = image
                .tracks
                .get(usize::from(self.head))
                .and_then(|side| side.get(usize::from(self.track[drive])))
                .map_or(&[][..], |track| track.sectors.as_slice());
            let sector = sectors.iter().find(|s| {
                (s.c, s.h, s.id, s.n)
                    == (
                        self.cmd_buf[2],
                        self.cmd_buf[3],
                        self.sector,
                        self.cmd_buf[5],
                    )
            });
            let Some(sector) = sector else {
                let mut flags = self.st2 & ST2_CM;
                if sectors.iter().any(|s| s.c != self.cmd_buf[2]) {
                    flags |= 0x10;
                }
                if sectors
                    .iter()
                    .any(|s| s.c == 0xff && s.c != self.cmd_buf[2])
                {
                    flags |= 2;
                }
                self.write_result(4, flags);
                return;
            };
            if sector.st1 & 0x20 != 0 && sector.st2 & 0x20 == 0 {
                self.write_result(0x20, self.st2 & ST2_CM);
                return;
            }
            let deleted = sector.st2 & ST2_CM != 0;
            if deleted && self.cmd_buf[0] & 0x20 != 0 {
                self.st2 |= ST2_CM;
                if !self.advance_scan_sector() {
                    self.write_result(0x80, (self.st2 & ST2_CM) | 4);
                    return;
                }
                continue;
            }
            let size = 128_usize << self.cmd_buf[5];
            if sector.data.len() < size || sector.st1 & 1 != 0 {
                self.write_result(1, 1 | (self.st2 & ST2_CM));
                return;
            }
            self.exec_buf.clear();
            self.exec_buf.extend_from_slice(&sector.data[..size]);
            self.exec_pos = 0;
            self.st1 = sector.st1;
            self.st2 = (self.st2 & ST2_CM) | (sector.st2 & (ST2_CM | 0x20)) | 8;
            self.phase = Phase::Execution;
            self.main_status = MSR_EXM | MSR_CB;
            self.write_start_pending = true;
            self.exec_timeout =
                self.write_byte_ticks() * if self.cmd_buf[0] & 0x40 != 0 { 38 } else { 18 };
            return;
        }
    }

    fn accept_scan_byte(&mut self, host: u8) {
        let disk = self.exec_buf[self.exec_pos];
        if disk != host {
            self.st2 &= !8;
        }
        let violates = match self.cmd_buf[0] & 0x1f {
            0x11 => disk != host,
            0x19 => disk > host,
            0x1d => disk < host,
            _ => false,
        };
        if violates {
            self.st2 |= 4;
        }
        self.exec_pos += 1;
        self.main_status &= !MSR_RQM;
    }

    fn advance_scan_sector(&mut self) -> bool {
        let next = u16::from(self.sector) + u16::from(self.cmd_buf[8]);
        if next <= u16::from(self.cmd_buf[6]) {
            self.sector = next as u8;
            true
        } else if self.sector == self.cmd_buf[6] && self.cmd_buf[0] & 0x80 != 0 && self.head == 0 {
            self.head = 1;
            self.cmd_buf[1] |= 4;
            self.cmd_buf[3] = 1;
            self.st0 |= 4;
            self.sector = 1;
            true
        } else {
            false
        }
    }

    fn finish_scan_sector(&mut self) {
        if self.st1 & 0x20 != 0 || self.st2 & 0x20 != 0 {
            self.write_result(self.st1, self.st2);
        } else if self.st2 & 4 == 0 || (self.st2 & ST2_CM != 0 && self.cmd_buf[0] & 0x20 == 0) {
            self.exec_timeout = 0;
            self.write_start_pending = false;
            self.setup_result_read(
                self.cmd_buf[2],
                self.cmd_buf[3],
                self.sector,
                self.cmd_buf[5],
            );
        } else if self.advance_scan_sector() {
            self.start_scan_sector();
        } else {
            self.write_result(0x80, self.st2);
        }
    }

    // NEC December 1978 p14 and MAME read_track_continue: EOT is a
    // count, not a sector-ID limit. DSK represents ordered payloads and ID/error
    // metadata, not raw gaps, index timing or flux. Never invent those bytes.
    fn start_read_track(&mut self) {
        self.start_read();
    }

    fn start_read(&mut self) {
        self.st0 = self.cmd_buf[1] & 7;
        self.st1 = 0;
        self.st2 = 0;
        self.head = (self.cmd_buf[1] >> 2) & 1;
        self.sector = self.cmd_buf[4];
        self.read_progress = Some(ReadProgress {
            completed: 0,
            pending_st1: 0,
            pending_st2: 0,
        });
        if self.prepare_read_sector() {
            self.phase = Phase::Execution;
            self.start_read_preamble();
        }
    }

    // Select one represented field. Preserve the existing bounded size, SK,
    // fill and physical-track rules; do not predict the rest of the command.
    fn prepare_read_sector(&mut self) -> bool {
        let drive = usize::from(self.cmd_buf[1] & self.drive_select_mask);
        let track = self.track[drive];
        let is_track = self.command == Command::ReadTrack;
        self.exec_buf.clear();
        self.exec_pos = 0;
        let completed = self.read_progress.as_ref().map_or(0, |p| p.completed);
        loop {
            let sectors = self.disks[drive]
                .as_ref()
                .and_then(|image| image.tracks.get(usize::from(self.head)))
                .and_then(|side| side.get(usize::from(track)))
                .map_or(&[][..], |track| track.sectors.as_slice());
            if is_track && self.disks[drive].is_none() {
                self.st0 |= 8;
                self.read_result();
                return false;
            }
            if self.cmd_buf[5] > 7 || (is_track && (sectors.is_empty() || self.cmd_buf[6] == 0)) {
                self.st1 = if is_track { 1 } else { 4 };
                self.read_result();
                return false;
            }
            let sec = if is_track {
                sectors.get(usize::from(completed) % sectors.len())
            } else {
                // HD selected the physical side above; H is the recorded ID
                // byte and can differ. Fuse seek_id and MAME sector_matches
                // require all four ID fields before delivering data.
                sectors.iter().find(|s| {
                    (s.c, s.h, s.id, s.n)
                        == (
                            self.cmd_buf[2],
                            self.cmd_buf[3],
                            self.sector,
                            self.cmd_buf[5],
                        )
                })
            };
            let Some(sec) = sec else {
                // An ID search is now reached only after preceding data is
                // consumed. Wrong C=FF also sets BC (NEC Dec 1978 p18).
                self.st1 |= 4;
                for candidate in sectors.iter().filter(|s| s.c != self.cmd_buf[2]) {
                    self.st2 |= 0x10 | if candidate.c == 0xff { 2 } else { 0 };
                }
                self.read_result();
                return false;
            };
            // NEC December 1978 pp12/17; MAME read_data_continue rejects
            // header CRC before the data-field FIFO. DE without DD is the
            // represented ID error: it precedes DAM, including any SK choice.
            // Read Track continues past it; DD-set metadata remains a data
            // error and cannot tell us that the ID was independently corrupt.
            if !is_track && sec.st1 & 0x20 != 0 && sec.st2 & 0x20 == 0 {
                self.st1 |= 0x20;
                self.read_result();
                return false;
            }
            let mark_mismatch =
                (sec.st2 & ST2_CM != 0) != (self.command == Command::ReadDeletedData);
            if !is_track && mark_mismatch && self.cmd_buf[0] & 0x20 != 0 {
                if self.sector >= self.cmd_buf[6] {
                    // Keep the existing all-skipped/after-data convention.
                    if completed == 0 {
                        self.sector = self.cmd_buf[4];
                        self.st1 |= 4;
                    } else {
                        self.sector = self.sector.wrapping_add(1);
                    }
                    self.read_result();
                    return false;
                }
                self.sector = self.sector.wrapping_add(1);
                continue;
            }
            let size = 128_usize << self.cmd_buf[5];
            let take = sec.data.len().min(size);
            self.exec_buf.extend_from_slice(&sec.data[..take]);
            self.exec_buf.resize(
                size,
                if is_track {
                    0
                } else {
                    sec.data.last().copied().unwrap_or(0)
                },
            );
            if let Some(progress) = &mut self.read_progress {
                progress.pending_st1 = sec.st1;
                progress.pending_st2 = if is_track {
                    sec.st2
                } else {
                    (sec.st2 & !ST2_CM) | if mark_mismatch { ST2_CM } else { 0 }
                };
            }
            if is_track {
                // MAME read_track_continue recomputes ND for each ID, while
                // retaining prior errors. Fuse differs on DE/returned CHRN;
                // see the retained investigation, not a conformance claim.
                self.st1 &= !4;
                if (sec.c, sec.h, sec.id, sec.n)
                    != (
                        self.cmd_buf[2],
                        self.cmd_buf[3],
                        self.cmd_buf[4],
                        self.cmd_buf[5],
                    )
                {
                    self.st1 |= 4;
                }
            }
            self.write_start_pending = true;
            return true;
        }
    }

    fn begin_read_field(&mut self) {
        let Some(progress) = &self.read_progress else {
            return;
        };
        self.st2 |= progress.pending_st2 & ST2_CM;
        if self.command != Command::ReadTrack {
            let drive = usize::from(self.cmd_buf[1] & self.drive_select_mask);
            let key = (drive, self.track[drive], self.head, self.sector);
            if self.reread_key == Some(key) {
                self.reread_count = self.reread_count.saturating_add(1);
            } else {
                self.reread_key = Some(key);
                self.reread_count = 0;
            }
            // Existing Fuse-derived marginal recipe, applied only when this
            // field is actually visited. No future field advances its counter.
            if (progress.pending_st1 | progress.pending_st2) & 0x20 != 0 && self.reread_count > 0 {
                apply_marginal_variation(&mut self.exec_buf, self.reread_count);
            }
        }
    }

    fn finish_read_sector(&mut self) {
        let Some(progress) = &mut self.read_progress else {
            return;
        };
        self.st1 |= progress.pending_st1;
        self.st2 |= progress.pending_st2;
        progress.completed += 1;
        if self.command == Command::ReadTrack {
            if progress.completed >= u16::from(self.cmd_buf[6]) {
                self.st1 |= 0x80;
                self.read_result();
                return;
            }
        } else {
            if (self.st1 | self.st2) & 0x20 != 0 || self.st2 & ST2_CM != 0 {
                self.read_result();
                return;
            }
            if self.sector >= self.cmd_buf[6] {
                if self.st1 == 0 && self.st2 == 0 {
                    self.st1 |= 0x80;
                }
                self.read_result();
                return;
            }
            self.sector = self.sector.wrapping_add(1);
        }
        // Keep the pending byte-clock boundary, including host latency. This
        // does not add a new inter-sector timing model.
        self.prepare_read_sector();
    }

    fn read_result(&mut self) {
        // Read Track passes a bad ID CRC before transferring payload. DE
        // without DD represents that ID error; a data CRC is still deferred
        // until field completion. Derive it from existing progress here so
        // older v6 checkpoints taken inside the field retain it too. A
        // prepared but unvisited field must not contribute status.
        // Fuse read_id/start_read_diag, NEC December 1978 pp14/17.
        if self.command == Command::ReadTrack
            && !self.write_start_pending
            && let Some(progress) = &self.read_progress
            && progress.pending_st2 & 0x20 == 0
        {
            self.st1 |= progress.pending_st1 & 0x20;
        }
        if self.st1 != 0 || self.st2 != 0 || self.command == Command::ReadTrack {
            self.st0 |= 0x40;
        }
        let r = if self.command == Command::ReadTrack {
            self.cmd_buf[4]
        } else {
            self.sector
        };
        self.exec_timeout = 0;
        self.write_start_pending = false;
        self.read_progress = None;
        // ID H is a command field, independent of physical HD in ST0.
        // Fuse returns data_register[2]; MAME Read Data returns command[3].
        // Keep Read Track's existing result convention outside this fix.
        let h = if self.command == Command::ReadTrack {
            self.head
        } else {
            self.cmd_buf[3]
        };
        self.setup_result_read(self.cmd_buf[2], h, r, self.cmd_buf[5]);
    }

    fn start_read_preamble(&mut self) {
        // Use the existing sector-model pre-data interval (also used by Scan
        // and Write), plus one slot for the first completed read byte.
        // This is not a rotational/index/head-load timing model.
        self.main_status = MSR_DIO | MSR_EXM | MSR_CB;
        self.write_start_pending = true;
        self.exec_timeout = self.read_slot_boundary(self.read_preamble_slots());
    }

    fn read_preamble_slots(&self) -> u64 {
        if self.cmd_buf[0] & 0x40 != 0 { 39 } else { 19 }
    }

    // Nominal +3 sector-image cadence, not a physical jitter/service-limit
    // model. Round absolute boundaries, not each byte: 32 us is fractional
    // at the emulated CPU clock. Existing byte position preserves the phase
    // through snapshots without another serialized counter.
    fn read_slot_boundary(&self, slot: u64) -> u32 {
        let micros = if self.cmd_buf[0] & 0x40 != 0 { 32 } else { 64 };
        (slot * micros * common_sinclair_zx_spectrum::timing::CPU_HZ_128K).div_ceil(1_000_000)
            as u32
    }

    fn read_byte_ticks(&self) -> u32 {
        let slot = self.read_preamble_slots() + self.exec_pos as u64;
        self.read_slot_boundary(slot + 1) - self.read_slot_boundary(slot)
    }

    fn abort_for_media_change(&mut self, drive: usize) {
        if self.phase == Phase::Execution
            && matches!(
                self.command,
                Command::WriteData
                    | Command::FormatTrack
                    | Command::Scan
                    | Command::ReadData
                    | Command::ReadDeletedData
                    | Command::ReadTrack
            )
            && usize::from(self.cmd_buf[1] & self.drive_select_mask) == drive
        {
            self.st0 |= 0x08;
            if matches!(
                self.command,
                Command::ReadData | Command::ReadDeletedData | Command::ReadTrack
            ) {
                // Host media changes end this transfer before another cached
                // byte can arrive. Preserve only already encountered status;
                // this is not a model of physical READY transition timing.
                self.st0 |= 0x40;
                self.read_result();
            } else if self.command == Command::FormatTrack {
                self.format_result(0, true);
            } else {
                self.write_result(0, 0);
            }
        }
    }

    // NEC December 1978 datasheet p.13; Fuse 1.7.0 start_write_data /
    // upd_fdc_write_data. The +3 uses a 4 MHz FDC at 250 kbit/s MFM:
    // one byte every 32 us. tick() is called once per +3 CPU T-state,
    // not once per master half-cycle. Round up to avoid early deadlines.
    fn write_byte_ticks(&self) -> u32 {
        let micros = if self.cmd_buf[0] & 0x40 != 0 { 32 } else { 64 };
        (common_sinclair_zx_spectrum::timing::CPU_HZ_128K * micros).div_ceil(1_000_000) as u32
    }

    fn write_result(&mut self, st1: u8, st2: u8) {
        self.st0 |= 0x40;
        self.st1 = st1;
        self.st2 = st2;
        self.exec_timeout = 0;
        self.write_start_pending = false;
        self.setup_result_read(
            self.cmd_buf[2],
            self.cmd_buf[3],
            self.sector,
            self.cmd_buf[5],
        );
    }

    fn start_write_sector(&mut self) {
        let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
        let Some(image) = self.disks[drive].as_ref() else {
            self.st0 |= 0x08; // NR, independent of protection.
            self.write_result(0, 0);
            return;
        };
        if self.writes_inhibited(drive) {
            self.write_result(0x02, 0); // NW
            return;
        }
        let Some(sector) = image.sector(self.track[drive], self.head, self.sector) else {
            self.write_result(0x04, 0); // ND
            return;
        };
        if sector.c != self.cmd_buf[2] {
            let st2 = 0x10 | if sector.c == 0xff { 0x02 } else { 0 }; // WC / BC
            self.write_result(0x04, st2);
            return;
        }
        let n = self.cmd_buf[5];
        if sector.h != self.cmd_buf[3] || sector.n != n || n > 7 {
            self.write_result(0x04, 0);
            return;
        }
        // An ID-field CRC error prevents a write; a bad old data-field CRC
        // does not. Writing a complete new data field repairs that CRC.
        if sector.st1 & 0x20 != 0 && sector.st2 & 0x20 == 0 {
            self.write_result(0x20, 0);
            return;
        }
        let length = 128_usize << n;
        if sector.data.len() != length {
            // The sector-level model cannot safely rewrite truncated or
            // oversized/weak copies as if they were an ordinary data field.
            self.write_result(0x04, 0);
            return;
        }
        let transfer = if n == 0 {
            usize::from(self.cmd_buf[8]).min(128)
        } else {
            length
        };
        if transfer == 0 {
            self.write_result(0x04, 0);
            return;
        }
        self.exec_buf.resize(transfer, 0);
        self.exec_pos = 0;
        self.phase = Phase::Execution;
        self.main_status = MSR_EXM | MSR_CB;
        // The first byte is not requested at the final command strobe. After
        // finding the ID, the head crosses gap 2 and the data sync/mark. Fuse
        // start_write_data advances 22 gap bytes, 12 zeroes, 3 A1s and a DAM
        // in MFM (11 + 6 + 1 in FM). Our sector model retains that minimum
        // pre-data interval; it does not model rotational ID-search latency.
        self.write_start_pending = true;
        self.exec_timeout =
            self.write_byte_ticks() * if self.cmd_buf[0] & 0x40 != 0 { 38 } else { 18 };
    }

    // NEC December 1978 p13; Fuse start_write_data chooses FB/F8 from
    // the opcode. Retain the opcode in cmd_buf so mid-write v5 states need
    // neither a new field nor a reordered Command discriminant.
    fn write_data_mark(&self) -> u8 {
        if self.cmd_buf[0] & 0x1f == 0x09 {
            ST2_CM
        } else {
            0
        }
    }

    // The data mark reaches the medium before the first host byte. Fuse
    // start_write_data and MAME WRITE_SECTOR_DATA both write FB/F8 before
    // consuming payload. If that first byte overruns, retain the new mark
    // and an incomplete data CRC, with the old payload still untouched.
    fn begin_write_data_field(&mut self) -> bool {
        let mark = self.write_data_mark();
        let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
        if self.disks[drive].is_none() {
            self.st0 |= 0x08;
            self.write_result(0, 0);
            return false;
        }
        if self.writes_inhibited(drive) {
            self.write_result(0x02, 0);
            return false;
        }
        let Some(sector) = self.disks[drive]
            .as_mut()
            .and_then(|image| image.tracks.get_mut(self.head as usize))
            .and_then(|side| side.get_mut(self.track[drive] as usize))
            .and_then(|track| track.sectors.iter_mut().find(|s| s.id == self.sector))
        else {
            self.write_result(0x04, 0);
            return false;
        };
        sector.st1 = 0x20;
        sector.st2 = 0x20 | mark;
        true
    }

    fn accept_write_byte(&mut self, value: u8) {
        let mark = self.write_data_mark();
        let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
        let inhibited = self.writes_inhibited(drive);
        let Some(sector) = self.disks[drive]
            .as_mut()
            .and_then(|image| image.tracks.get_mut(self.head as usize))
            .and_then(|side| side.get_mut(self.track[drive] as usize))
            .and_then(|track| track.sectors.iter_mut().find(|s| s.id == self.sector))
        else {
            self.st0 |= 0x08;
            self.write_result(0, 0);
            return;
        };
        if inhibited {
            self.write_result(0x02, 0);
            return;
        }
        // Accepted bytes reach the medium. An interrupted write leaves its
        // prefix and an invalid CRC, rather than rolling the sector back.
        sector.data[self.exec_pos] = value;
        sector.st1 = 0x20;
        sector.st2 = 0x20 | mark; // selected DAM/DDAM, incomplete data CRC
        self.exec_buf[self.exec_pos] = value;
        self.exec_pos += 1;
        self.main_status &= !MSR_RQM;
    }

    fn finish_write_sector(&mut self) {
        let mark = self.write_data_mark();
        let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
        if self.writes_inhibited(drive) {
            self.write_result(0x02, 0);
            return;
        }
        let Some(sector) = self.disks[drive]
            .as_mut()
            .and_then(|image| image.tracks.get_mut(self.head as usize))
            .and_then(|side| side.get_mut(self.track[drive] as usize))
            .and_then(|track| track.sectors.iter_mut().find(|s| s.id == self.sector))
        else {
            self.st0 |= 0x08;
            self.write_result(0, 0);
            return;
        };
        // N=0/DTL writes pad the rest of the physical data field with zero.
        sector.data[self.exec_pos..].fill(0);
        sector.st1 = 0;
        sector.st2 = mark;
        self.reread_key = None;
        self.reread_count = 0;
        if self.sector >= self.cmd_buf[6] {
            self.sector = 1;
            if self.cmd_buf[0] & 0x80 != 0 && self.head == 0 {
                self.head = 1;
                self.cmd_buf[1] |= 4;
                self.cmd_buf[3] = 1;
                self.st0 |= 4;
                self.start_write_sector();
                return;
            }
            // +3 supplies no TC. Retain Fuse's next-CHRN convention with EN.
            // NEC Table 2 describes processor termination, not this case;
            // MAME retains C instead. Exact no-TC CHRN remains unresolved:
            // https://github.com/emu198x/emu198x/issues/1814
            self.cmd_buf[2] = self.cmd_buf[2].wrapping_add(1);
            if self.cmd_buf[0] & 0x80 != 0 {
                self.head = 0;
                self.cmd_buf[1] &= !4;
                self.cmd_buf[3] = 0;
                self.st0 &= !4;
            }
            self.write_result(0x80, 0);
        } else {
            self.sector = self.sector.wrapping_add(1);
            self.start_write_sector();
        }
    }

    // NEC December 1978 p.14: command N controls data length, while each
    // supplied CHRN is recorded verbatim. Fuse start_write_id / write_data
    // commits one ID tuple at a time. MAME WRITE_TRACK_* supplies the encoded
    // preamble and sector byte counts. This remains a sector-image model:
    // no flux transitions, angular phase, head-load latency or partial-ID CRC.
    fn format_layout(&self) -> (u32, u32, u32) {
        let mfm = self.cmd_buf[0] & 0x40 != 0;
        let preamble = if mfm { 146 } else { 73 };
        let sync = if mfm { 16 } else { 7 };
        let span =
            (if mfm { 62 } else { 33 }) + (128_u32 << self.cmd_buf[2]) + u32::from(self.cmd_buf[4]);
        (preamble, sync, span)
    }

    fn start_format_track(&mut self) {
        self.st0 = self.cmd_buf[1] & 7;
        self.st1 = 0;
        self.st2 = 0;
        self.head = (self.cmd_buf[1] >> 2) & 1;
        self.exec_buf.clear();
        self.exec_pos = 0;
        let drive = usize::from(self.cmd_buf[1] & self.drive_select_mask);
        let Some(image) = self.disks[drive].as_ref() else {
            self.st0 |= 0x08;
            self.format_result(0, true);
            return;
        };
        if self.writes_inhibited(drive) {
            self.format_result(0x02, true);
            return;
        }
        if image
            .tracks
            .get(usize::from(self.head))
            .and_then(|side| side.get(usize::from(self.track[drive])))
            .is_none()
            || self.cmd_buf[2] > 7
            || self.cmd_buf[3] == 0
        {
            self.format_result(0x04, true);
            return;
        }
        let (preamble, sync, span) = self.format_layout();
        let capacity = if self.cmd_buf[0] & 0x40 != 0 {
            6250
        } else {
            3125
        };
        if preamble + span * u32::from(self.cmd_buf[3]) > capacity {
            // The fixed 250-kbit/s, 300-rpm sector model cannot represent
            // an overfull track. Reject before allocating or altering media.
            self.format_result(0x04, true);
            return;
        }
        self.exec_buf.resize(usize::from(self.cmd_buf[3]) * 4, 0);
        self.phase = Phase::Execution;
        self.main_status = MSR_EXM | MSR_CB;
        self.write_start_pending = true;
        self.exec_timeout = (preamble + sync) * self.write_byte_ticks();
    }

    fn format_result(&mut self, st1: u8, abnormal: bool) {
        if abnormal {
            self.st0 |= 0x40;
        }
        self.st1 = st1;
        self.st2 = 0;
        self.exec_timeout = 0;
        self.write_start_pending = false;
        let complete = self.exec_pos / 4 * 4;
        let (c, h, r, n) = if complete == 0 {
            (0, 0, 0, self.cmd_buf[2])
        } else {
            let id = &self.exec_buf[complete - 4..complete];
            (
                id[0],
                id[1],
                if abnormal {
                    id[2]
                } else {
                    id[2].wrapping_add(1)
                },
                id[3],
            )
        };
        self.setup_result_read(c, h, r, n);
    }

    fn accept_format_byte(&mut self, value: u8) {
        let drive = usize::from(self.cmd_buf[1] & self.drive_select_mask);
        if self.writes_inhibited(drive) {
            self.format_result(0x02, true);
            return;
        }
        self.exec_buf[self.exec_pos] = value;
        self.exec_pos += 1;
        self.main_status &= !MSR_RQM;
        if !self.exec_pos.is_multiple_of(4) {
            return;
        }
        let drive = usize::from(self.cmd_buf[1] & self.drive_select_mask);
        let index = self.exec_pos / 4 - 1;
        let Some(track) = self.disks[drive]
            .as_mut()
            .and_then(|image| image.tracks.get_mut(usize::from(self.head)))
            .and_then(|side| side.get_mut(usize::from(self.track[drive])))
        else {
            self.st0 |= 0x08;
            self.format_result(0, true);
            return;
        };
        let id = &self.exec_buf[self.exec_pos - 4..self.exec_pos];
        let sector = DiskSector {
            c: id[0],
            h: id[1],
            id: id[2],
            n: id[3],
            st1: 0,
            st2: 0,
            data: vec![self.cmd_buf[5]; 128_usize << self.cmd_buf[2]],
        };
        if index < track.sectors.len() {
            track.sectors[index] = sector;
        } else {
            track.sectors.push(sector);
        }
        self.read_id_index[drive] = 0;
        self.reread_key = None;
        self.reread_count = 0;
        let (preamble, sync, span) = self.format_layout();
        let wait = if self.exec_pos == self.exec_buf.len() {
            let capacity = if self.cmd_buf[0] & 0x40 != 0 {
                6250
            } else {
                3125
            };
            capacity - (preamble + sync + index as u32 * span + 3)
        } else {
            span - 3
        };
        self.write_start_pending = true;
        // Host service latency must not stretch the encoded track interval.
        self.exec_timeout += (wait - 1) * self.write_byte_ticks();
    }

    fn tick_format(&mut self) {
        self.exec_timeout = self.exec_timeout.saturating_sub(1);
        if self.exec_timeout != 0 {
            return;
        }
        let drive = usize::from(self.cmd_buf[1] & self.drive_select_mask);
        if self.writes_inhibited(drive) {
            self.format_result(0x02, true);
            return;
        }
        if self.main_status & MSR_RQM != 0 {
            self.format_result(0x10, true); // missed ID-byte service window
        } else if self.exec_pos == self.exec_buf.len() {
            if let Some(track) = self.disks[drive]
                .as_mut()
                .and_then(|image| image.tracks.get_mut(usize::from(self.head)))
                .and_then(|side| side.get_mut(usize::from(self.track[drive])))
            {
                track.sectors.truncate(usize::from(self.cmd_buf[3]));
            }
            self.format_result(0, false);
        } else {
            self.write_start_pending = false;
            self.main_status |= MSR_RQM;
            self.exec_timeout = self.write_byte_ticks();
        }
    }

    fn setup_result_read(&mut self, track: u8, head: u8, sector: u8, n: u8) {
        self.result_buf.clear();
        self.result_buf.push(self.st0);
        self.result_buf.push(self.st1);
        self.result_buf.push(self.st2);
        self.result_buf.push(track);
        self.result_buf.push(head);
        self.result_buf.push(sector);
        self.result_buf.push(n);
        self.result_pos = 0;
        self.phase = Phase::Result;
        self.main_status = MSR_RQM | MSR_DIO | MSR_CB;
        self.interrupt = true;
        if fdc_trace_enabled() {
            eprintln!(
                "[FDC-RES] {:?} result=[{}]",
                self.command,
                fmt_bytes(&self.result_buf),
            );
        }
    }
}

impl Default for Upd765a {
    fn default() -> Self {
        Self::new()
    }
}

impl Peripheral for Upd765a {
    /// Claims the +3's FDC ports: `$2FFD` (main status register) and
    /// `$3FFD` (data register). Decoded by the Amstrad gate array as
    /// `A15=0 A14=0 A13=1` plus `A12` selecting status vs data, with
    /// `A1=0`. The low 8 bits alias so we check on a `port & 0xF002`
    /// mask.
    ///
    /// Returns false unconditionally when `enabled` is false —
    /// Spectrum +2A / +2B share the SpectrumPlus struct with the +3
    /// but don't wire a drive connector, so their FDC instance sits
    /// inert on the bus.
    fn claims_port(&self, port: u16) -> bool {
        if !self.enabled {
            return false;
        }
        let masked = port & 0xF002;
        masked == 0x2000 || masked == 0x3000
    }

    fn read(&mut self, port: u16) -> u8 {
        let masked = port & 0xF002;
        if masked == 0x2000 {
            self.read_status()
        } else if masked == 0x3000 {
            self.read_data()
        } else {
            0xFF
        }
    }

    fn write(&mut self, port: u16, val: u8) {
        // Only the data register (`$3FFD`) accepts writes. The main
        // status register at `$2FFD` is read-only; writes to it are
        // silently ignored by the real controller.
        if port & 0xF002 == 0x3000 {
            self.write_data(val);
        }
    }

    /// Per-T-state housekeeping. Decrements each drive's seek
    /// countdown; on transition to zero, moves the staged ST0 into
    /// `seek_pending` (where `SenseInterruptStatus` will drain it) and
    /// raises the interrupt flag. The drive-busy bit reads out of MSR
    /// via `seek_remaining` directly, so no MSR fix-up is needed here.
    fn tick(&mut self, _hc: u32) {
        if !self.enabled {
            return;
        }
        self.poll_ready();
        for drive in 0..4 {
            if self.seek_remaining[drive] == 0 {
                continue;
            }
            self.seek_remaining[drive] -= 1;
            if self.seek_remaining[drive] == 0
                && let Some(st0) = self.seek_staged_st0[drive].take()
            {
                let st0 = if self.drive_ready(drive) {
                    st0
                } else {
                    st0 | 0x48
                };
                self.seek_pending[drive] = Some(st0);
                self.seek_attention_unacknowledged = true;
                self.interrupt = true;
            }
        }

        if self.phase == Phase::Execution && self.command == Command::FormatTrack {
            self.tick_format();
            return;
        }

        if self.phase == Phase::Execution
            && matches!(self.command, Command::WriteData | Command::Scan)
        {
            self.exec_timeout = self.exec_timeout.saturating_sub(1);
            if self.exec_timeout == 0 {
                if self.write_start_pending {
                    if self.command == Command::WriteData && !self.begin_write_data_field() {
                        return;
                    }
                    self.write_start_pending = false;
                    self.main_status |= MSR_RQM;
                    self.exec_timeout = self.write_byte_ticks();
                } else if self.main_status & MSR_RQM != 0 {
                    self.write_result(0x10, self.st2 & ST2_CM); // OR: host missed its byte window.
                } else if self.exec_pos == self.exec_buf.len() {
                    if self.command == Command::Scan {
                        self.finish_scan_sector();
                    } else {
                        self.finish_write_sector();
                    }
                } else {
                    self.main_status |= MSR_RQM;
                    self.exec_timeout = self.write_byte_ticks();
                }
            }
            return;
        }

        if self.phase == Phase::Execution
            && matches!(
                self.command,
                Command::ReadData | Command::ReadDeletedData | Command::ReadTrack
            )
        {
            self.exec_timeout = self.exec_timeout.saturating_sub(1);
            if self.exec_timeout == 0 {
                if self.write_start_pending {
                    self.begin_read_field();
                    self.write_start_pending = false;
                    self.main_status |= MSR_RQM;
                    self.exec_timeout = self.read_byte_ticks();
                } else if self.main_status & MSR_RQM != 0 {
                    // Result assembly also preserves the current ID CRC.
                    // A later field cannot contribute EN/CRC/CHRN.
                    self.st1 |= 0x10;
                    self.read_result();
                } else {
                    self.main_status |= MSR_RQM;
                    self.exec_timeout = self.read_byte_ticks();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn legacy_bad_id_normalization_discards_a_previously_visited_data_mark() {
        for command in [Command::ReadData, Command::ReadDeletedData] {
            // A synthetic old active state isolates CM accumulated by the
            // faulty payload path. Actual low/high-RQM v6 fixtures live in
            // the runtime tests; no old result buffer is changed here.
            let mut fdc = Upd765a::new();
            fdc.phase = Phase::Execution;
            fdc.command = command;
            fdc.cmd_buf = vec![0x46, 0, 0, 0, 1, 2, 1, 0x2a, 0xff];
            fdc.sector = 1;
            fdc.st2 = ST2_CM;
            fdc.main_status = MSR_RQM | MSR_DIO | MSR_EXM | MSR_CB;
            fdc.exec_buf = vec![0xa5; 512];
            fdc.exec_pos = 179;
            fdc.read_progress = Some(ReadProgress {
                completed: 0,
                pending_st1: 0x20,
                pending_st2: ST2_CM,
            });
            fdc.after_restore();
            fdc.after_restore(); // Already assembled results remain untouched.
            assert_eq!(fdc.read_status(), 0xd0);
            let result: Vec<_> = (0..7).map(|_| fdc.read_data()).collect();
            assert_eq!(result, [0x40, 0x20, 0, 0, 0, 1, 2]);
            assert_eq!(fdc.read_status(), 0x80);
        }
    }

    impl Upd765a {
        fn read_ready(&mut self) -> u8 {
            for _ in 0..10_000 {
                if self.main_status & MSR_RQM != 0 {
                    return self.read_data();
                }
                assert_eq!(self.phase, Phase::Execution);
                <Self as Peripheral>::tick(self, 0);
            }
            panic!("read request deadline");
        }
    }

    /// Legacy separate attachment must preserve controller read caches.
    #[test]
    fn reattach_disk_preserves_cached_read_state() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.insert_disk(0, DiskImage::default());
        fdc.reread_key = Some((0, 12, 0, 24));
        fdc.reread_count = 17;
        fdc.read_id_index[0] = 3;

        fdc.reattach_disk(0, DiskImage::default());

        assert!(fdc.has_disk(0), "reattach must mount the image");
        assert_eq!(
            fdc.reread_key,
            Some((0, 12, 0, 24)),
            "reattach must preserve the re-read key"
        );
        assert_eq!(
            fdc.reread_count, 17,
            "reattach must preserve the re-read count"
        );
        assert_eq!(
            fdc.read_id_index[0], 3,
            "reattach must preserve the ReadID rotational position"
        );
    }

    /// The counterpart: genuine insertion still invalidates the cache,
    /// because the medium really did change.
    #[test]
    fn insert_disk_still_clears_cached_read_state() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.insert_disk(0, DiskImage::default());
        fdc.reread_key = Some((0, 12, 0, 24));
        fdc.reread_count = 17;
        fdc.read_id_index[0] = 3;

        fdc.insert_disk(0, DiskImage::default());

        assert_eq!(
            fdc.reread_key, None,
            "insertion invalidates the re-read key"
        );
        assert_eq!(fdc.reread_count, 0, "insertion resets the re-read count");
        assert_eq!(
            fdc.read_id_index[0], 0,
            "insertion resets the ReadID position"
        );
    }
    use super::*;

    #[test]
    fn write_data_enters_host_to_controller_execution() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.insert_disk_with_writable(
            0,
            DiskImage {
                sides: 1,
                tracks_per_side: 1,
                tracks: vec![vec![DiskTrack {
                    sectors: vec![DiskSector {
                        c: 0,
                        h: 0,
                        id: 1,
                        n: 2,
                        st1: 0,
                        st2: 0,
                        data: vec![0xe5; 512],
                    }],
                }]],
            },
            true,
        );
        for byte in [0x45, 0, 0, 0, 1, 2, 1, 0x2a, 0xff] {
            fdc.write_data(byte);
        }
        assert_eq!(
            fdc.read_status() & 0x70,
            0x30,
            "Write Data must enter CPU-to-FDC execution"
        );
    }

    #[test]
    fn format_track_enters_host_to_controller_execution() {
        let mut fdc = write_test_fdc(true);
        for byte in [0x4d, 0, 2, 2, 0x52, 0xa5] {
            fdc.write_data(byte);
        }
        assert_eq!(
            fdc.read_status() & 0x70,
            0x30,
            "Format Track must enter CPU-to-controller execution"
        );
    }

    fn send_format(fdc: &mut Upd765a, unit: u8, n: u8, count: u8, fill: u8) {
        for byte in [0x4d, unit, n, count, 0x52, fill] {
            fdc.write_data(byte);
        }
    }

    fn await_format_status(fdc: &mut Upd765a, expected: u8) {
        for _ in 0..800_000 {
            if fdc.read_status() & 0xf0 == expected {
                return;
            }
            assert_ne!(fdc.read_status() & 0xf0, 0xd0, "unexpected format result");
            fdc.tick(0);
        }
        panic!("format did not reach expected status {expected:02x}");
    }

    fn transfer_ids(fdc: &mut Upd765a, ids: &[u8]) {
        for &id in ids {
            await_format_status(fdc, 0xb0);
            fdc.write_data(id);
        }
    }

    #[test]
    fn format_read_id_and_data_agree_and_other_tracks_survive() {
        let mut fdc = write_test_fdc(true);
        send_format(&mut fdc, 0, 2, 2, 0xa7);
        transfer_ids(&mut fdc, &[0, 0, 9, 2, 0, 0, 3, 2]);
        await_format_status(&mut fdc, 0xd0);
        assert_eq!(result(&mut fdc), [0, 0, 0, 0, 0, 4, 2]);
        for id in [9, 3] {
            for byte in [0x4a, 0] {
                fdc.write_data(byte);
            }
            assert_eq!(result(&mut fdc), [0, 0, 0, 0, 0, id, 2]);
            for byte in [0x46, 0, 0, 0, id, 2, id, 0x2a, 0xff] {
                fdc.write_data(byte);
            }
            let data: Vec<_> = (0..512).map(|_| fdc.read_ready()).collect();
            assert_eq!(data, vec![0xa7; 512]);
            let _ = result(&mut fdc);
        }
        let image = fdc.disks[0].as_ref().expect("mounted");
        assert_eq!(
            image.tracks[0][0]
                .sectors
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            [9, 3]
        );
        for (head, cylinder) in [(0, 1), (1, 0), (1, 1)] {
            for sector in &image.tracks[head][cylinder].sectors {
                assert_eq!(sector.data, vec![0xe5; 512]);
                assert_eq!((sector.st1, sector.st2), (0, 0));
            }
        }
    }

    #[test]
    fn format_uses_physical_selection_and_command_size_not_id_fields() {
        let mut fdc = write_test_fdc(true);
        fdc.set_drive_select_mask(1);
        fdc.track[0] = 1;
        send_format(&mut fdc, 6, 0, 1, 0x69); // drive 2 aliases 0; physical head 1
        transfer_ids(&mut fdc, &[7, 8, 0xff, 3]);
        await_format_status(&mut fdc, 0xd0);
        assert_eq!(result(&mut fdc), [6, 0, 0, 7, 8, 0, 3]);
        let image = fdc.disks[0].as_ref().expect("mounted");
        let track = &image.tracks[1][1];
        assert_eq!(track.sectors.len(), 1, "old trailing sector removed");
        let sector = &track.sectors[0];
        assert_eq!((sector.c, sector.h, sector.id, sector.n), (7, 8, 255, 3));
        assert_eq!(
            sector.data,
            vec![0x69; 128],
            "command N controls the data field"
        );
        assert_eq!(image.tracks[0][0].sectors[0].data, vec![0xe5; 512]);
    }

    #[test]
    fn format_protection_readiness_and_bounds_fail_without_mutation() {
        for (unit, n, count, writable, status) in [
            (0, 2, 2, false, [0x40, 2]),
            (1, 2, 2, true, [0x49, 0]),
            (0, 8, 1, true, [0x40, 4]),
            (0, 2, 0, true, [0x40, 4]),
            (0, 7, 255, true, [0x40, 4]),
            (0, 2, 10, true, [0x40, 4]), // overfull 300-rpm track
        ] {
            let mut fdc = write_test_fdc(writable);
            send_format(&mut fdc, unit, n, count, 0);
            let observed = result(&mut fdc);
            assert_eq!(&observed[..2], &status);
            assert_eq!(
                fdc.disks[0].as_ref().expect("disk").tracks[0][0].sectors[0].data,
                vec![0xe5; 512]
            );
        }
        let mut fdc = write_test_fdc(true);
        fdc.track[0] = 40;
        send_format(&mut fdc, 0, 2, 2, 0);
        assert_eq!(&result(&mut fdc)[..3], &[0x40, 4, 0]);
    }

    #[test]
    fn format_readiness_direction_and_overrun_are_enforced() {
        let mut fdc = write_test_fdc(true);
        send_format(&mut fdc, 0, 2, 2, 0x42);
        fdc.write_data(99); // ignored during preamble
        assert_eq!(fdc.exec_pos, 0);
        for _ in 0..18_467 {
            fdc.tick(0);
        } // 162 MFM bytes * ceil(32us * CPU_HZ)
        assert_eq!(fdc.read_status() & 0xf0, 0x30);
        fdc.tick(0);
        assert_eq!(fdc.read_status() & 0xf0, 0xb0);
        assert_eq!(
            fdc.read_data(),
            0xff,
            "wrong-direction read cannot consume an ID"
        );
        fdc.write_data(0);
        fdc.write_data(99); // ignored with RQM low
        assert_eq!(fdc.exec_pos, 1);
        for _ in 0..114 {
            fdc.tick(0);
        }
        assert_eq!(fdc.read_status() & 0xf0, 0xb0);
        for _ in 0..114 {
            fdc.tick(0);
        }
        assert_eq!(&result(&mut fdc)[..3], &[0x40, 0x10, 0]);
        assert_eq!(
            fdc.disks[0].as_ref().expect("disk").tracks[0][0].sectors[0].data,
            vec![0xe5; 512]
        );
    }

    #[test]
    fn completed_format_sector_survives_overrun_and_media_ejection() {
        let mut fdc = write_test_fdc(true);
        send_format(&mut fdc, 0, 2, 2, 0x57);
        transfer_ids(&mut fdc, &[0, 0, 7, 2]);
        await_format_status(&mut fdc, 0xb0);
        for _ in 0..114 {
            fdc.tick(0);
        }
        assert_eq!(&result(&mut fdc)[..3], &[0x40, 0x10, 0]);
        assert_eq!(
            fdc.disks[0].as_ref().expect("disk").tracks[0][0].sectors[0].data,
            vec![0x57; 512]
        );
        send_format(&mut fdc, 0, 2, 2, 0x66);
        transfer_ids(&mut fdc, &[0, 0, 8, 2]);
        let removed = fdc.disks[0].clone().expect("mounted before eject");
        fdc.eject_disk(0);
        assert_eq!(&result(&mut fdc)[..3], &[0x48, 0, 0]);
        assert_eq!(removed.tracks[0][0].sectors[0].data, vec![0x66; 512]);
    }

    #[test]
    fn standard_plus3_format_fills_nine_sectors_without_host_latency_drift() {
        for latency in [0, 37, 113] {
            let mut fdc = write_test_fdc(true);
            fdc.disks[0].as_mut().expect("disk").tracks[0][0]
                .sectors
                .clear();
            send_format(&mut fdc, 0, 2, 9, 0xe5);
            let mut ticks = 0;
            for id in [1, 6, 2, 7, 3, 8, 4, 9, 5] {
                for byte in [0, 0, id, 2] {
                    while fdc.read_status() & 0xf0 != 0xb0 {
                        assert!(ticks < 712_500);
                        fdc.tick(0);
                        ticks += 1;
                    }
                    for _ in 0..latency {
                        fdc.tick(0);
                        ticks += 1;
                    }
                    fdc.write_data(byte);
                }
            }
            while fdc.read_status() & 0xf0 != 0xd0 {
                assert!(ticks < 712_500);
                fdc.tick(0);
                ticks += 1;
            }
            assert_eq!(
                ticks, 712_500,
                "6250 encoded MFM bytes, 114 CPU ticks per byte"
            );
            assert_eq!(result(&mut fdc), [0, 0, 0, 0, 0, 6, 2]);
            let track = &fdc.disks[0].as_ref().expect("disk").tracks[0][0];
            assert_eq!(
                track.sectors.iter().map(|s| s.id).collect::<Vec<_>>(),
                [1, 6, 2, 7, 3, 8, 4, 9, 5]
            );
            assert!(
                track
                    .sectors
                    .iter()
                    .all(|s| s.data == vec![0xe5; 512] && s.st1 == 0 && s.st2 == 0)
            );
        }
    }

    #[test]
    fn reset_preserves_completed_format_sectors_and_protection() {
        let mut fdc = write_test_fdc(true);
        send_format(&mut fdc, 0, 2, 2, 0x87);
        transfer_ids(&mut fdc, &[0, 0, 6, 2]);
        fdc.reset();
        assert_eq!(fdc.read_status(), 0x80);
        assert!(fdc.writable[0]);
        let track = &fdc.disks[0].as_ref().expect("disk").tracks[0][0];
        assert_eq!(track.sectors[0].id, 6);
        assert_eq!(track.sectors[0].data, vec![0x87; 512]);
    }

    #[test]
    fn fm_format_uses_slower_byte_period_and_preserves_sector_order() {
        let mut fdc = write_test_fdc(true);
        for byte in [0x0d, 0, 0, 2, 0x1b, 0x91] {
            fdc.write_data(byte);
        }
        transfer_ids(&mut fdc, &[0, 0, 5, 0, 0, 0, 2, 0]);
        await_format_status(&mut fdc, 0xd0);
        assert_eq!(result(&mut fdc), [0, 0, 0, 0, 0, 3, 0]);
        let track = &fdc.disks[0].as_ref().expect("disk").tracks[0][0];
        assert_eq!(
            track.sectors.iter().map(|s| s.id).collect::<Vec<_>>(),
            [5, 2]
        );
        assert!(track.sectors.iter().all(|s| s.data == vec![0x91; 128]));
    }

    fn write_test_fdc(writable: bool) -> Upd765a {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        let tracks = (0..2)
            .map(|head| {
                (0..2)
                    .map(|c| DiskTrack {
                        sectors: (1..=2)
                            .map(|id| DiskSector {
                                c,
                                h: head,
                                id,
                                n: 2,
                                st1: 0,
                                st2: 0,
                                data: vec![0xe5; 512],
                            })
                            .collect(),
                    })
                    .collect()
            })
            .collect();
        fdc.insert_disk_with_writable(
            0,
            DiskImage {
                sides: 2,
                tracks_per_side: 2,
                tracks,
            },
            writable,
        );
        fdc
    }

    fn send_write(fdc: &mut Upd765a, drive_head: u8, c: u8, h: u8, r: u8, eot: u8) {
        for byte in [0x45, drive_head, c, h, r, 2, eot, 0x2a, 0xff] {
            fdc.write_data(byte);
        }
    }

    #[test]
    fn export_refuses_active_writes_without_aborting_and_returns_committed_bytes() {
        let mut fdc = write_test_fdc(true);
        assert!(fdc.disk_for_export(4).is_err());
        assert!(fdc.disk_for_export(1).is_err());
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        transfer_write(&mut fdc, &[0x5a; 128]);
        let before = (fdc.phase, fdc.exec_pos, fdc.exec_timeout, fdc.main_status);
        assert!(fdc.disk_for_export(0).is_err());
        assert_eq!(
            before,
            (fdc.phase, fdc.exec_pos, fdc.exec_timeout, fdc.main_status)
        );
        transfer_write(&mut fdc, &[0x5a; 384]);
        result(&mut fdc);
        assert_eq!(
            fdc.disk_for_export(0).expect("completed image").tracks[0][0].sectors[0].data,
            [0x5a; 512]
        );
        fdc.eject_disk(0);
        assert!(fdc.disk_for_export(0).is_err());
    }

    #[test]
    fn export_refuses_active_format_and_allows_read_only_images() {
        let protected = write_test_fdc(false);
        assert!(protected.disk_for_export(0).is_ok());
        let mut fdc = write_test_fdc(true);
        send_format(&mut fdc, 0, 2, 1, 0xa5);
        let before = (fdc.phase, fdc.exec_pos, fdc.exec_timeout, fdc.main_status);
        assert!(fdc.disk_for_export(0).is_err());
        assert_eq!(
            before,
            (fdc.phase, fdc.exec_pos, fdc.exec_timeout, fdc.main_status)
        );
        transfer_ids(&mut fdc, &[0, 0, 1, 2]);
        await_format_status(&mut fdc, 0xd0);
        result(&mut fdc);
        assert_eq!(
            fdc.disk_for_export(0).expect("formatted image").tracks[0][0].sectors[0].data,
            [0xa5; 512]
        );
    }

    fn await_write_request(fdc: &mut Upd765a) {
        for _ in 0..5000 {
            if fdc.read_status() & 0x80 != 0 {
                return;
            }
            fdc.tick(0);
        }
        panic!("write request must appear within the pre-data interval");
    }

    fn transfer_write(fdc: &mut Upd765a, bytes: &[u8]) {
        for &byte in bytes {
            await_write_request(fdc);
            assert_eq!(fdc.read_status() & 0xf0, 0xb0);
            fdc.write_data(byte);
            for _ in 0..114 {
                fdc.tick(0);
            }
        }
    }

    fn result(fdc: &mut Upd765a) -> Vec<u8> {
        assert_eq!(fdc.read_status() & 0xf0, 0xd0);
        (0..7).map(|_| fdc.read_data()).collect()
    }

    #[test]
    fn write_protection_is_reported_and_prevents_mutation() {
        let mut fdc = write_test_fdc(false);
        fdc.write_data(4);
        fdc.write_data(0);
        assert_eq!(fdc.read_data() & 0x60, 0x60);
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        assert_eq!(result(&mut fdc), [0x40, 2, 0, 0, 0, 1, 2]);
        assert_eq!(
            fdc.disks[0]
                .as_ref()
                .expect("disk")
                .sector(0, 0, 1)
                .expect("sector")
                .data,
            vec![0xe5; 512]
        );
    }

    #[test]
    fn external_protection_inhibits_writes_without_overriding_host_policy() {
        for writable in [false, true] {
            for protected in [false, true] {
                let mut fdc = write_test_fdc(writable);
                fdc.set_write_protect(0, protected);
                let before = format!("{:?}", fdc.disks[0]);
                send_write(&mut fdc, 0, 0, 0, 1, 1);
                if !writable || protected {
                    assert_eq!(result(&mut fdc)[1], 2, "NW independent of host permission");
                    assert_eq!(format!("{:?}", fdc.disks[0]), before);
                } else {
                    assert_eq!(fdc.phase, Phase::Execution);
                }
            }
        }
    }

    #[test]
    fn external_protection_rechecks_each_accepted_write_byte() {
        let mut fdc = write_test_fdc(true);
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        await_write_request(&mut fdc);
        fdc.write_data(0x12);
        await_write_request(&mut fdc);
        let before = format!("{:?}", fdc.disks[0]);
        fdc.set_write_protect(0, true);
        fdc.write_data(0x34);
        assert_eq!(result(&mut fdc)[1], 2);
        assert_eq!(
            format!("{:?}", fdc.disks[0]),
            before,
            "no byte accepted after WP assertion"
        );
    }

    #[test]
    fn external_protection_after_last_byte_preserves_incomplete_crc() {
        let mut fdc = write_test_fdc(true);
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        transfer_write(&mut fdc, &[0x56; 511]);
        await_write_request(&mut fdc);
        fdc.write_data(0x56); // assert WP before advancing the final field deadline
        assert_eq!(fdc.phase, Phase::Execution);
        let before = format!("{:?}", fdc.disks[0]);
        fdc.set_write_protect(0, true);
        for _ in 0..1024 {
            if fdc.phase != Phase::Execution {
                break;
            }
            fdc.tick(0);
        }
        assert_eq!(fdc.phase, Phase::Result, "bounded final-field deadline");
        assert_eq!(result(&mut fdc)[1], 2);
        assert_eq!(format!("{:?}", fdc.disks[0]), before);
    }

    #[test]
    fn external_protection_blocks_format_start_id_mutation_and_final_truncation() {
        for stage in 0..3 {
            let mut fdc = write_test_fdc(true);
            if stage == 0 {
                fdc.set_write_protect(0, true);
            }
            send_format(&mut fdc, 0, 2, 1, 0xa7);
            if stage == 1 {
                await_format_status(&mut fdc, 0xb0);
                fdc.set_write_protect(0, true);
            } else if stage == 2 {
                transfer_ids(&mut fdc, &[0, 0, 9, 2]);
                fdc.set_write_protect(0, true);
            }
            let before = format!("{:?}", fdc.disks[0]);
            if stage == 1 {
                fdc.write_data(0);
            } else if stage == 2 {
                await_format_status(&mut fdc, 0xd0);
            }
            assert_eq!(result(&mut fdc)[1], 2, "stage={stage}");
            assert_eq!(format!("{:?}", fdc.disks[0]), before, "stage={stage}");
        }
    }

    #[test]
    fn write_data_reads_back_and_preserves_other_sectors() {
        let mut fdc = write_test_fdc(true);
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        let payload: Vec<_> = (0..512).map(|i| (i ^ (i >> 8)) as u8).collect();
        transfer_write(&mut fdc, &payload);
        assert_eq!(result(&mut fdc), [0x40, 0x80, 0, 1, 0, 1, 2]);
        for byte in [0x46, 0, 0, 0, 1, 2, 1, 0x2a, 0xff] {
            fdc.write_data(byte);
        }
        let observed: Vec<_> = (0..512).map(|_| fdc.read_ready()).collect();
        assert_eq!(observed, payload);
        let image = fdc.disks[0].as_ref().expect("disk");
        assert_eq!(image.sector(0, 0, 2).expect("sector").data, vec![0xe5; 512]);
        assert_eq!(image.sector(1, 0, 1).expect("sector").data, vec![0xe5; 512]);
        assert_eq!(image.sector(0, 1, 1).expect("sector").data, vec![0xe5; 512]);
    }

    #[test]
    fn writes_use_physical_track_head_and_drive_alias() {
        let mut fdc = write_test_fdc(true);
        fdc.set_drive_select_mask(1);
        fdc.track[0] = 1;
        send_write(&mut fdc, 6, 1, 1, 1, 2);
        transfer_write(&mut fdc, &vec![0x12; 512]);
        transfer_write(&mut fdc, &vec![0x34; 512]);
        assert_eq!(result(&mut fdc), [0x46, 0x80, 0, 2, 1, 1, 2]);
        let image = fdc.disks[0].as_ref().expect("disk");
        assert_eq!(image.sector(1, 1, 1).expect("sector").data, vec![0x12; 512]);
        assert_eq!(image.sector(1, 1, 2).expect("sector").data, vec![0x34; 512]);
        assert_eq!(image.sector(0, 1, 1).expect("sector").data, vec![0xe5; 512]);
    }

    #[test]
    fn write_direction_readiness_and_overrun_are_enforced() {
        let mut fdc = write_test_fdc(true);
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        await_write_request(&mut fdc);
        assert_eq!(fdc.read_data(), 0xff, "a read must not consume write data");
        fdc.write_data(0xa7);
        fdc.write_data(0xff); // RQM low: must not consume another byte.
        for _ in 0..113 {
            fdc.tick(0);
        }
        assert_eq!(fdc.read_status() & 0x80, 0);
        fdc.tick(0);
        assert_eq!(fdc.read_status() & 0x80, 0x80);
        for _ in 0..114 {
            fdc.tick(0);
        }
        assert_eq!(result(&mut fdc), [0x40, 0x10, 0, 0, 0, 1, 2]);
        let sector = fdc.disks[0]
            .as_ref()
            .expect("disk")
            .sector(0, 0, 1)
            .expect("sector");
        assert_eq!(&sector.data[..2], [0xa7, 0xe5]);
        assert_eq!((sector.st1, sector.st2), (0x20, 0x20));
    }

    #[test]
    fn write_errors_identify_missing_media_sector_and_wrong_chrn() {
        let mut fdc = write_test_fdc(true);
        send_write(&mut fdc, 1, 0, 0, 1, 1);
        assert_eq!(result(&mut fdc), [0x49, 0, 0, 0, 0, 1, 2]);
        send_write(&mut fdc, 0, 0, 0, 3, 3);
        assert_eq!(result(&mut fdc), [0x40, 4, 0, 0, 0, 3, 2]);
        send_write(&mut fdc, 0, 1, 0, 1, 1);
        assert_eq!(result(&mut fdc), [0x40, 4, 0x10, 1, 0, 1, 2]);
        send_write(&mut fdc, 0, 0, 1, 1, 1);
        assert_eq!(result(&mut fdc), [0x40, 4, 0, 0, 1, 1, 2]);
        for byte in [0x45, 0, 0, 0, 1, 7, 1, 0x2a, 0xff] {
            fdc.write_data(byte);
        }
        assert_eq!(result(&mut fdc), [0x40, 4, 0, 0, 0, 1, 7]);
    }

    #[test]
    fn missing_second_sector_preserves_completed_first_write() {
        let mut fdc = write_test_fdc(true);
        send_write(&mut fdc, 0, 0, 0, 2, 3);
        transfer_write(&mut fdc, &vec![0x76; 512]);
        assert_eq!(result(&mut fdc), [0x40, 4, 0, 0, 0, 3, 2]);
        assert_eq!(
            fdc.disks[0]
                .as_ref()
                .expect("disk")
                .sector(0, 0, 2)
                .expect("sector")
                .data,
            vec![0x76; 512]
        );
    }

    #[test]
    fn complete_write_replaces_deleted_mark_and_bad_data_crc() {
        let mut fdc = write_test_fdc(true);
        let sector = &mut fdc.disks[0].as_mut().expect("disk").tracks[0][0].sectors[0];
        sector.st1 = 0x20;
        sector.st2 = 0x60;
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        transfer_write(&mut fdc, &vec![0x66; 512]);
        let _ = result(&mut fdc);
        let sector = fdc.disks[0]
            .as_ref()
            .expect("disk")
            .sector(0, 0, 1)
            .expect("sector");
        assert_eq!((sector.st1, sector.st2), (0, 0));
    }

    #[test]
    fn reset_retains_written_prefix_and_mount_protection() {
        let mut fdc = write_test_fdc(true);
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        transfer_write(&mut fdc, &[0xb1, 0xb2]);
        fdc.reset();
        assert_eq!(fdc.read_status(), 0x80);
        assert!(fdc.enabled && fdc.writable[0]);
        let sector = fdc.disks[0]
            .as_ref()
            .expect("disk")
            .sector(0, 0, 1)
            .expect("sector");
        assert_eq!(&sector.data[..3], [0xb1, 0xb2, 0xe5]);
        assert_eq!((sector.st1, sector.st2), (0x20, 0x20));
    }

    #[test]
    fn media_change_aborts_transfer_before_replacement_can_be_written() {
        let mut fdc = write_test_fdc(true);
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        await_write_request(&mut fdc);
        fdc.write_data(0xb1);
        fdc.insert_disk_with_writable(0, DiskImage::default(), true);
        assert_eq!(result(&mut fdc), [0x48, 0, 0, 0, 0, 1, 2]);
    }

    #[test]
    fn multitrack_write_advances_to_second_head() {
        let mut fdc = write_test_fdc(true);
        for byte in [0xc5, 0, 0, 0, 1, 2, 1, 0x2a, 0xff] {
            fdc.write_data(byte);
        }
        transfer_write(&mut fdc, &vec![0x56; 512]);
        transfer_write(&mut fdc, &vec![0x78; 512]);
        assert_eq!(result(&mut fdc), [0x40, 0x80, 0, 1, 0, 1, 2]);
        let image = fdc.disks[0].as_ref().expect("disk");
        assert_eq!(image.sector(0, 0, 1).expect("sector").data, vec![0x56; 512]);
        assert_eq!(image.sector(0, 1, 1).expect("sector").data, vec![0x78; 512]);
    }

    #[test]
    fn write_rejects_id_crc_and_unrepresentable_sector_lengths() {
        let mut fdc = write_test_fdc(true);
        fdc.disks[0].as_mut().expect("disk").tracks[0][0].sectors[0].st1 = 0x20;
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        assert_eq!(result(&mut fdc), [0x40, 0x20, 0, 0, 0, 1, 2]);
        let sector = &mut fdc.disks[0].as_mut().expect("disk").tracks[0][0].sectors[0];
        sector.st1 = 0;
        sector.data.truncate(511);
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        assert_eq!(result(&mut fdc), [0x40, 4, 0, 0, 0, 1, 2]);
    }

    #[test]
    fn zero_size_code_uses_dtl_and_pads_remaining_data_field() {
        let mut fdc = write_test_fdc(true);
        let sector = &mut fdc.disks[0].as_mut().expect("disk").tracks[0][0].sectors[0];
        sector.n = 0;
        sector.data.truncate(128);
        for byte in [0x45, 0, 0, 0, 1, 0, 1, 0x2a, 2] {
            fdc.write_data(byte);
        }
        transfer_write(&mut fdc, &[0x12, 0x34]);
        assert_eq!(result(&mut fdc), [0x40, 0x80, 0, 1, 0, 1, 0]);
        let sector = fdc.disks[0]
            .as_ref()
            .expect("disk")
            .sector(0, 0, 1)
            .expect("sector");
        assert_eq!(&sector.data[..2], &[0x12, 0x34]);
        assert!(sector.data[2..].iter().all(|&byte| byte == 0));
    }

    #[test]
    fn preamble_rechecks_write_protection_before_changing_the_mark() {
        let mut fdc = write_test_fdc(true);
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        // Model protection changing while gap/sync is passing the head.
        fdc.writable[0] = false;
        await_write_request(&mut fdc);
        assert_eq!(result(&mut fdc), [0x40, 2, 0, 0, 0, 1, 2]);
        let sector = &fdc.disk_for_export(0).expect("protected image").tracks[0][0].sectors[0];
        assert_eq!((sector.st1, sector.st2), (0, 0));
        assert_eq!(sector.data, [0xe5; 512]);
    }

    #[test]
    fn preamble_rechecks_missing_media_before_requesting_payload() {
        let mut fdc = write_test_fdc(true);
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        // Also guard a missing attachment independently of eject's abort.
        fdc.disks[0] = None;
        await_write_request(&mut fdc);
        assert_eq!(result(&mut fdc), [0x48, 0, 0, 0, 0, 1, 2]);
    }

    #[test]
    fn first_write_request_follows_the_pre_data_gap() {
        let mut fdc = write_test_fdc(true);
        send_write(&mut fdc, 0, 0, 0, 1, 1);
        assert_eq!(fdc.read_status() & 0xf0, 0x30);
        fdc.write_data(0x99); // No request yet: must not reach the medium.
        for _ in 0..4331 {
            fdc.tick(0);
        }
        assert_eq!(fdc.read_status() & 0xf0, 0x30);
        fdc.tick(0);
        assert_eq!(fdc.read_status() & 0xf0, 0xb0);
        assert_eq!(
            fdc.disks[0]
                .as_ref()
                .expect("disk")
                .sector(0, 0, 1)
                .expect("sector")
                .data,
            vec![0xe5; 512]
        );
    }

    #[test]
    fn initial_state() {
        let fdc = Upd765a::new();
        assert_eq!(fdc.read_status(), MSR_RQM); // Ready for commands
        assert_eq!(fdc.phase, Phase::Idle);
    }

    #[test]
    fn recalibrate_zeros_track_then_completes_after_seek_ticks() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.track[0] = 10;

        fdc.write_data(0x07); // Recalibrate
        fdc.write_data(0x00); // Drive 0

        // Recalibrate is staged: track snaps to 0 immediately, but
        // the interrupt is held until the seek countdown expires.
        // The logical busy bit remains until SIS reads ST0.
        assert_eq!(fdc.track[0], 0);
        assert!(!fdc.interrupt);
        assert_eq!(fdc.read_status() & 0x01, 0x01); // drive 0 busy

        for _ in 0..SEEK_TICKS {
            <Upd765a as Peripheral>::tick(&mut fdc, 0);
        }
        assert!(fdc.interrupt);
        assert_eq!(fdc.read_status() & 0x01, 1); // completion awaits SIS ST0
        fdc.write_data(0x08);
        assert_eq!(fdc.read_data(), 0x68);
        assert_eq!(fdc.read_status() & 0x01, 0); // ST0 acknowledges drive 0
        assert_eq!(fdc.read_data(), 0);
    }

    #[test]
    fn sense_interrupt_drains_pending_then_returns_invalid_command() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        // Stage a pending seek interrupt for drive 0 (Seek End).
        fdc.seek_pending[0] = Some(0x20);

        fdc.write_data(0x08); // Sense Interrupt Status
        assert_eq!(fdc.phase, Phase::Result);
        assert_eq!(fdc.read_data(), 0x20); // ST0
        assert_eq!(fdc.read_data(), 0); // PCN

        // The existing no-pending Invalid classification has ST0 only.
        fdc.write_data(0x08);
        assert_eq!(fdc.read_data(), 0x80);
        assert_eq!(fdc.read_status(), MSR_RQM);
    }

    #[test]
    fn sense_interrupt_walks_drives_in_order() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.seek_pending[3] = Some(0x23); // Seek End | drive 3
        fdc.seek_pending[0] = Some(0x20); // Seek End | drive 0

        fdc.write_data(0x08);
        assert_eq!(fdc.read_data(), 0x20);
        assert_eq!(fdc.read_data(), 0);

        fdc.write_data(0x08);
        assert_eq!(fdc.read_data(), 0x23);
        assert_eq!(fdc.read_data(), 0);

        fdc.write_data(0x08);
        assert_eq!(fdc.read_data(), 0x80);
        assert_eq!(fdc.read_status(), MSR_RQM);
    }

    #[test]
    fn read_sector() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;

        // Build a one-track image with sector ID 1, 512 bytes, marker bytes
        // at the start and end so we can verify the byte stream.
        let mut sector_data = vec![0u8; 512];
        sector_data[0] = 0xDE;
        sector_data[511] = 0xAD;
        let track = DiskTrack {
            sectors: vec![DiskSector {
                c: 0,
                h: 0,
                id: 1,
                n: 2,
                st1: 0,
                st2: 0,
                data: sector_data,
            }],
        };
        let image = DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![track]],
        };
        fdc.insert_disk(0, image);

        // Read sector: command + 8 parameter bytes
        fdc.write_data(0x06); // Read Data
        fdc.write_data(0x00); // Drive 0, head 0
        fdc.write_data(0x00); // Track 0
        fdc.write_data(0x00); // Head 0
        fdc.write_data(0x01); // Sector 1
        fdc.write_data(0x02); // N=2 (512 bytes)
        fdc.write_data(0x09); // EOT
        fdc.write_data(0x2A); // GPL
        fdc.write_data(0xFF); // DTL

        assert_eq!(fdc.phase, Phase::Execution);

        let first = fdc.read_ready();
        assert_eq!(first, 0xDE);

        for _ in 1..511 {
            fdc.read_ready();
        }
        let last = fdc.read_ready();
        assert_eq!(last, 0xAD);

        // Should be in result phase now
        assert_eq!(fdc.phase, Phase::Result);
    }

    #[test]
    fn read_sector_by_id_not_position() {
        // Verify that sectors are looked up by ID, not by index. Build a
        // track where sector ID 0xC1 appears second in physical order;
        // a Read Data for sector 0xC1 must return its data regardless.
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        let track = DiskTrack {
            sectors: vec![
                DiskSector {
                    c: 0,
                    h: 0,
                    id: 0xC2,
                    n: 2,
                    st1: 0,
                    st2: 0,
                    data: vec![0xAA; 512],
                },
                DiskSector {
                    c: 0,
                    h: 0,
                    id: 0xC1,
                    n: 2,
                    st1: 0,
                    st2: 0,
                    data: {
                        let mut d = vec![0xBB; 512];
                        d[0] = 0xEE;
                        d
                    },
                },
            ],
        };
        let image = DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![track]],
        };
        fdc.insert_disk(0, image);

        fdc.write_data(0x06);
        fdc.write_data(0x00);
        fdc.write_data(0x00);
        fdc.write_data(0x00);
        fdc.write_data(0xC1);
        fdc.write_data(0x02);
        fdc.write_data(0xC9);
        fdc.write_data(0x2A);
        fdc.write_data(0xFF);

        assert_eq!(fdc.phase, Phase::Execution);
        assert_eq!(fdc.read_ready(), 0xEE);
    }

    /// `ReadDeletedData` reaches sectors that `ReadData` skips (with
    /// SK=1) — Speedlock writes its protection keys with DDAM marks
    /// and reads them back via 0x0C, expecting the chip to deliver
    /// the bytes while 0x06 with SK either skips them or flags
    /// ST2.CM and stops. Three cases here: SK=1 ReadData skips DDAM,
    /// SK=0 ReadData flags CM and stops, ReadDeletedData reads DDAM
    /// normally.
    #[test]
    fn read_deleted_data_matches_dam_then_ddam() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        let track = DiskTrack {
            sectors: vec![
                DiskSector {
                    c: 0,
                    h: 0,
                    id: 1,
                    n: 0, // N=0 → 128-byte sectors
                    st1: 0,
                    st2: 0,
                    data: vec![0x11; 128],
                },
                DiskSector {
                    c: 0,
                    h: 0,
                    id: 2,
                    n: 0,
                    st1: 0,
                    st2: ST2_CM, // ← deleted data mark (Speedlock key sector)
                    data: vec![0x22; 128],
                },
                DiskSector {
                    c: 0,
                    h: 0,
                    id: 3,
                    n: 0,
                    st1: 0,
                    st2: 0,
                    data: vec![0x33; 128],
                },
            ],
        };
        let image = DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![track]],
        };

        // Case A — ReadData with SK=1 across sectors 1..3 should
        // skip the DDAM sector (2) silently and deliver 1's bytes
        // and 3's bytes back-to-back.
        let mut a = fdc.clone();
        a.insert_disk(0, image.clone());
        a.write_data(0x26); // ReadData, SK=1, MFM=1
        a.write_data(0x00); // drive
        a.write_data(0x00); // C
        a.write_data(0x00); // H
        a.write_data(0x01); // R = 1
        a.write_data(0x00); // N = 128
        a.write_data(0x03); // EOT
        a.write_data(0x2A);
        a.write_data(0xFF);
        let bytes_a: Vec<u8> = (0..256).map(|_| a.read_ready()).collect();
        assert_eq!(bytes_a[0], 0x11, "sector 1 (DAM) first byte");
        assert_eq!(bytes_a[127], 0x11, "sector 1 last byte");
        assert_eq!(
            bytes_a[128], 0x33,
            "sector 3 (DAM) first — sector 2 (DDAM) was skipped"
        );

        // Case B — ReadData with SK=0 should deliver sectors 1 *and*
        // 2 (CM gets flagged) then stop. ST2.CM must be set in the
        // result phase and ST0's abnormal-termination bit must fire.
        let mut b = fdc.clone();
        b.insert_disk(0, image.clone());
        b.write_data(0x06); // ReadData, SK=0
        b.write_data(0x00);
        b.write_data(0x00);
        b.write_data(0x00);
        b.write_data(0x01);
        b.write_data(0x00);
        b.write_data(0x03);
        b.write_data(0x2A);
        b.write_data(0xFF);
        let bytes_b: Vec<u8> = (0..256).map(|_| b.read_ready()).collect();
        assert_eq!(bytes_b[0], 0x11, "sector 1 (DAM) delivered");
        assert_eq!(
            bytes_b[128], 0x22,
            "sector 2 (DDAM) also delivered when SK=0"
        );
        // Now in result phase — read the 7-byte status block.
        assert_eq!(b.phase, Phase::Result);
        let st0 = b.read_ready();
        let _st1 = b.read_ready();
        let st2 = b.read_ready();
        assert_ne!(
            st0 & 0xC0,
            0,
            "ST0 abnormal-termination IC set on mark mismatch"
        );
        assert_ne!(
            st2 & ST2_CM,
            0,
            "ST2.CM set when ReadData found a DDAM with SK=0"
        );

        // Case C — ReadDeletedData (SK=1) should *skip* the DAM
        // sectors and deliver sector 2 (DDAM). With R=1 EOT=3 we
        // expect just one sector's worth of bytes.
        let mut c = fdc.clone();
        c.insert_disk(0, image);
        c.write_data(0x2C); // ReadDeletedData, SK=1, MFM=1
        c.write_data(0x00);
        c.write_data(0x00);
        c.write_data(0x00);
        c.write_data(0x01);
        c.write_data(0x00);
        c.write_data(0x03);
        c.write_data(0x2A);
        c.write_data(0xFF);
        let bytes_c: Vec<u8> = (0..128).map(|_| c.read_ready()).collect();
        assert_eq!(
            bytes_c[0], 0x22,
            "sector 2 (DDAM) delivered by ReadDeletedData"
        );
        assert_eq!(bytes_c[127], 0x22);
    }

    fn marginal_image() -> DiskImage {
        // Track 0 with one clean sector and one marginal-encoded
        // sector (ST1.DE | ST2.DD set). Sector data filled with
        // distinctive byte 0x55 so we can spot the variation pattern.
        let track = DiskTrack {
            sectors: vec![
                DiskSector {
                    c: 0,
                    h: 0,
                    id: 1,
                    n: 0, // 128-byte sectors keep the tests small
                    st1: 0,
                    st2: 0,
                    data: vec![0x55; 128],
                },
                DiskSector {
                    c: 0,
                    h: 0,
                    id: 2,
                    n: 0,
                    st1: 0x20, // DE — data CRC error
                    st2: 0x20, // DD — data field CRC error
                    data: vec![0x55; 128],
                },
            ],
        };
        DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![track]],
        }
    }

    fn issue_read_sector_2(fdc: &mut Upd765a) -> Vec<u8> {
        fdc.write_data(0x06); // ReadData, SK=0
        fdc.write_data(0x00); // drive 0, head 0
        fdc.write_data(0x00); // C
        fdc.write_data(0x00); // H
        fdc.write_data(0x02); // R = 2
        fdc.write_data(0x00); // N = 128
        fdc.write_data(0x02); // EOT = 2
        fdc.write_data(0x2A);
        fdc.write_data(0xFF);
        let bytes: Vec<u8> = (0..128).map(|_| fdc.read_ready()).collect();
        // Drain the result-phase status block so the FDC returns to idle.
        while fdc.phase == Phase::Result {
            fdc.read_ready();
        }
        bytes
    }

    #[test]
    fn marginal_sector_first_read_is_recorded_bytes_verbatim() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.insert_disk(0, marginal_image());
        let bytes = issue_read_sector_2(&mut fdc);
        assert_eq!(
            bytes,
            vec![0x55; 128],
            "first read returns the recorded bytes"
        );
        assert_eq!(fdc.reread_count, 0, "counter starts at 0");
    }

    #[test]
    fn marginal_sector_second_read_varies_bytes() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.insert_disk(0, marginal_image());
        let first = issue_read_sector_2(&mut fdc);
        let second = issue_read_sector_2(&mut fdc);
        assert_eq!(fdc.reread_count, 1, "counter bumps on re-read");
        assert_ne!(
            first, second,
            "marginal sector returns different bytes on re-read"
        );
        // FUSE recipe: XOR every 29th byte with offset, scoped to first
        // 64 bytes when count == 1. Bytes outside that window are
        // unchanged.
        for (i, (&f, &s)) in first.iter().zip(second.iter()).enumerate() {
            if i % 29 == 0 && i < 64 {
                assert_eq!(s, f ^ (i as u8), "byte {i} should be XOR'd");
            } else {
                assert_eq!(s, f, "byte {i} unchanged at count=1");
            }
        }
    }

    #[test]
    fn marginal_sector_third_read_mangles_full_sector() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.insert_disk(0, marginal_image());
        let first = issue_read_sector_2(&mut fdc);
        let _second = issue_read_sector_2(&mut fdc);
        let third = issue_read_sector_2(&mut fdc);
        assert_eq!(fdc.reread_count, 2);
        // count >= 2 mangles every 29th byte across the full sector.
        for (i, (&f, &t)) in first.iter().zip(third.iter()).enumerate() {
            if i % 29 == 0 {
                assert_eq!(t, f ^ (i as u8), "byte {i} XOR'd at count=2");
            } else {
                assert_eq!(t, f, "byte {i} unchanged at non-29 offset");
            }
        }
    }

    #[test]
    fn intervening_different_sector_resets_marginal_counter() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.insert_disk(0, marginal_image());
        let _first = issue_read_sector_2(&mut fdc);

        // Read the clean sector 1 in between — counter resets, next
        // read of sector 2 is treated as a fresh first read.
        fdc.write_data(0x06);
        fdc.write_data(0x00);
        fdc.write_data(0x00);
        fdc.write_data(0x00);
        fdc.write_data(0x01); // R = 1
        fdc.write_data(0x00);
        fdc.write_data(0x01); // EOT = 1
        fdc.write_data(0x2A);
        fdc.write_data(0xFF);
        let _clean: Vec<u8> = (0..128).map(|_| fdc.read_ready()).collect();
        while fdc.phase == Phase::Result {
            fdc.read_ready();
        }

        let after = issue_read_sector_2(&mut fdc);
        assert_eq!(
            fdc.reread_count, 0,
            "key changed by intervening read → counter reset"
        );
        assert_eq!(
            after,
            vec![0x55; 128],
            "post-reset read returns recorded bytes verbatim"
        );
    }

    #[test]
    fn clean_sector_reads_deterministic_across_repeats() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.insert_disk(0, marginal_image());

        let read_clean = |fdc: &mut Upd765a| -> Vec<u8> {
            fdc.write_data(0x06);
            fdc.write_data(0x00);
            fdc.write_data(0x00);
            fdc.write_data(0x00);
            fdc.write_data(0x01); // sector 1 (no DE/DD)
            fdc.write_data(0x00);
            fdc.write_data(0x01);
            fdc.write_data(0x2A);
            fdc.write_data(0xFF);
            let bytes: Vec<u8> = (0..128).map(|_| fdc.read_ready()).collect();
            while fdc.phase == Phase::Result {
                fdc.read_ready();
            }
            bytes
        };

        let a = read_clean(&mut fdc);
        let b = read_clean(&mut fdc);
        let c = read_clean(&mut fdc);
        assert_eq!(a, b, "clean sector identical across reads");
        assert_eq!(b, c, "clean sector identical across reads");
    }

    /// Successive `ReadID` calls without a Seek between them should
    /// return *different* sectors, walking the track. Tetris's track
    /// 12 protection check reads multiple IDs and verifies the
    /// sequence matches its expected layout.
    #[test]
    fn read_id_rotates_through_sectors() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        let track = DiskTrack {
            sectors: (1..=3)
                .map(|i| DiskSector {
                    c: 0,
                    h: 0,
                    id: i,
                    n: 2,
                    st1: 0,
                    st2: 0,
                    data: vec![0; 512],
                })
                .collect(),
        };
        fdc.insert_disk(
            0,
            DiskImage {
                sides: 1,
                tracks_per_side: 1,
                tracks: vec![vec![track]],
            },
        );

        let issue_read_id = |fdc: &mut Upd765a| -> u8 {
            fdc.write_data(0x0A);
            fdc.write_data(0x00);
            fdc.read_data(); // ST0
            fdc.read_data(); // ST1
            fdc.read_data(); // ST2
            fdc.read_data(); // C
            fdc.read_data(); // H
            let r = fdc.read_data(); // R
            fdc.read_data(); // N
            r
        };

        assert_eq!(issue_read_id(&mut fdc), 1);
        assert_eq!(issue_read_id(&mut fdc), 2);
        assert_eq!(issue_read_id(&mut fdc), 3);
        // Wraps back to first sector on the next rotation.
        assert_eq!(issue_read_id(&mut fdc), 1);
    }

    /// Every successful read_data call in Execution should rearm the
    /// timeout. A host that's reading steadily must never trip it.
    /// Specify (0x03) accepts 3 parameter bytes (SRT/HUT/HLT and ND)
    /// and returns to Idle without entering a Result phase. The chip
    /// just stashes the parameters; we don't model the underlying step
    /// rate / head load timing in the runtime, so the test confirms
    /// the no-result-phase contract.
    #[test]
    fn specify_command_returns_to_idle_without_result_phase() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;

        fdc.write_data(0x03); // Specify
        fdc.write_data(0xDF); // SRT (4 bits) + HUT (4 bits)
        fdc.write_data(0x02); // HLT (7 bits) + ND (1 bit)

        // After 3 bytes the chip executes and returns to Idle, ready
        // for the next command. No result phase.
        assert_eq!(fdc.phase, Phase::Idle);
        assert_eq!(fdc.read_status(), MSR_RQM);
    }

    /// SeekTrack (0x0F) snaps the per-drive track counter to the
    /// requested cylinder and stages a Seek End interrupt that
    /// `SenseInterruptStatus` later drains. Unlike Recalibrate, the
    /// target track is caller-supplied.
    #[test]
    fn seek_track_sets_target_and_stages_interrupt() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.insert_disk(1, marginal_image());
        fdc.track[1] = 0;

        fdc.write_data(0x0F); // Seek
        fdc.write_data(0x01); // Drive 1
        fdc.write_data(20); // Target track

        assert_eq!(fdc.track[1], 20, "Seek snaps the per-drive track counter");
        assert_eq!(fdc.read_status() & 0x02, 0x02, "drive 1 busy bit set");

        for _ in 0..SEEK_TICKS {
            <Upd765a as Peripheral>::tick(&mut fdc, 0);
        }
        assert!(fdc.interrupt);

        // Drain the interrupt via SenseInterruptStatus.
        fdc.write_data(0x08);
        let st0 = fdc.read_data();
        let pcn = fdc.read_data();
        assert_eq!(st0 & 0x20, 0x20, "ST0 carries Seek End");
        assert_eq!(st0 & 0x03, 0x01, "ST0 carries drive 1");
        assert_eq!(pcn, 20, "PCN reports the new track");
    }

    /// SenseDriveStatus (0x04) returns ST3 with the disk-present (RY)
    /// and two-sided (TS) flags reflecting the mounted state, plus the
    /// track-zero (T0) flag derived from the per-drive track counter.
    #[test]
    fn sense_drive_status_reflects_disk_presence_and_track_zero() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        // Drive 0 has no disk yet; track = 0.
        fdc.write_data(0x04);
        fdc.write_data(0x00); // Drive 0
        let st3 = fdc.read_data();
        // T0 should be set (track == 0), RY clear (no disk).
        assert_eq!(st3 & 0x10, 0x10, "T0 set when track == 0");
        assert_eq!(st3 & 0x20, 0, "RY clear when no disk present");
    }

    #[test]
    fn sds_inputs_are_independent_of_media_head_position_and_each_other() {
        for mask in [1, 3] {
            for mounted in [false, true] {
                let mut fdc = Upd765a::new();
                fdc.enabled = true;
                fdc.set_drive_select_mask(mask);
                if mounted {
                    for drive in 0..4 {
                        fdc.insert_disk(drive, marginal_image());
                    }
                }
                for signals in 0_u8..32 {
                    for drive in 0..4 {
                        // Deliberately disagree with the supplied T0 sample.
                        fdc.track[drive] = if signals & 2 != 0 { 7 } else { 0 };
                        fdc.set_two_side(drive, signals & 1 != 0);
                        fdc.set_track_zero(drive, signals & 2 != 0);
                        fdc.set_ready(drive, signals & 4 != 0);
                        fdc.set_write_protect(drive, signals & 8 != 0);
                        fdc.set_fault(drive, signals & 16 != 0);
                    }
                    for selected in 0..8 {
                        fdc.write_data(4);
                        fdc.write_data(selected);
                        let expected = selected | (signals << 3);
                        assert_eq!(fdc.read_data(), expected, "mask={mask} mounted={mounted}");
                        assert_eq!(fdc.read_status(), MSR_RQM, "exactly one result byte");
                    }
                    fdc.reset();
                    fdc.enabled = true;
                    fdc.write_data(4);
                    fdc.write_data(0);
                    assert_eq!(fdc.read_data(), signals << 3, "reset retains input samples");
                }
            }
        }
    }

    #[test]
    fn sds_samples_selected_physical_inputs_and_latches_them() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        for (drive, bits) in [(0, 0x10_u8), (1, 0x40), (2, 0x80), (3, 0)] {
            fdc.set_track_zero(drive, bits & 0x10 != 0);
            fdc.set_write_protect(drive, bits & 0x40 != 0);
            fdc.set_fault(drive, bits & 0x80 != 0);
        }
        for selected in 0..8 {
            fdc.write_data(4);
            fdc.write_data(selected);
            let physical = usize::from(selected & 3);
            let bits = [0x10, 0x40, 0x80, 0][physical];
            assert_eq!(fdc.read_data(), selected | bits);
        }
        fdc.write_data(4);
        fdc.write_data(1);
        fdc.set_write_protect(1, false);
        fdc.set_fault(1, true);
        assert_eq!(
            fdc.read_data(),
            0x41,
            "sampled result survives later changes"
        );
        for invalid in [4, usize::MAX] {
            fdc.set_track_zero(invalid, true);
            fdc.set_write_protect(invalid, true);
            fdc.set_fault(invalid, true);
            assert!(!fdc.head_at_track_zero(invalid));
        }
        fdc.set_drive_select_mask(1);
        for selected in [1, 3, 5, 7] {
            fdc.write_data(4);
            fdc.write_data(selected);
            assert_eq!(fdc.read_data(), selected | 0x80, "B aliases physical unit1");
        }
    }

    /// SenseDriveStatus reports TS / RY when a disk is mounted, and
    /// T0 reflects whichever cylinder the head is currently over.
    #[test]
    fn sense_drive_status_with_disk_mounted() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.insert_disk(0, DiskImage::default());
        fdc.track[0] = 5; // Head not at track 0

        fdc.write_data(0x04);
        fdc.write_data(0x00);
        let st3 = fdc.read_data();
        assert_eq!(st3 & 0x20, 0x20, "RY set when disk present");
        assert_eq!(st3 & 0x08, 0x08, "TS set when disk present");
        assert_eq!(st3 & 0x10, 0, "T0 clear when head off cylinder 0");
    }

    /// `claims_port` accepts `$2FFD` (main status, read-only) and
    /// `$3FFD` (data, read/write). The decode mask is `0xF002`, so
    /// the port number's `A15-A12` and `A1` bits are the only ones
    /// that matter — shadow addresses with the same pattern also
    /// claim.
    #[test]
    fn claims_port_recognises_msr_and_data_ports() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        assert!(fdc.claims_port(0x2FFD), "MSR port");
        assert!(fdc.claims_port(0x3FFD), "data port");
        // Shadow with same masked pattern (A1 must stay clear).
        assert!(fdc.claims_port(0x2000));
        assert!(fdc.claims_port(0x3FFC));
        // Wrong A1: not claimed (mask isolates A1).
        assert!(!fdc.claims_port(0x3FFF), "A1 set falls outside the mask");
        // Outside the high-nibble mask: no claim.
        assert!(!fdc.claims_port(0x1FFD));
        assert!(!fdc.claims_port(0x4FFD));
    }

    /// When the FDC is disabled (e.g. on +2A / +2B which carry the
    /// crate but don't wire a drive), `claims_port` returns false
    /// regardless of port — the bus dispatcher then falls through.
    #[test]
    fn claims_port_returns_false_when_disabled() {
        let fdc = Upd765a::new(); // enabled = false by default
        assert!(!fdc.claims_port(0x2FFD));
        assert!(!fdc.claims_port(0x3FFD));
    }

    /// Reads from a port outside the FDC's mask range return `$FF`
    /// (open bus / no driver). The bus dispatcher should never reach
    /// here because `claims_port` returns false, but the read path
    /// has the defensive fall-through anyway.
    #[test]
    fn read_unknown_port_returns_ff() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        assert_eq!(fdc.read(0x1234), 0xFF);
        assert_eq!(fdc.read(0x4000), 0xFF);
    }

    /// Writes to the MSR port `$2FFD` are silently ignored — the
    /// register is read-only on real hardware. Only `$3FFD` (data)
    /// accepts writes through the bus dispatcher.
    #[test]
    fn write_to_msr_port_is_silently_ignored() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        let before = fdc.read_status();
        fdc.write(0x2FFD, 0xAA);
        assert_eq!(
            fdc.read_status(),
            before,
            "MSR write must not change status"
        );
    }

    /// `insert_disk` + `has_disk` + `eject_disk` round trip on a
    /// specific drive index. The Plus3 binary's File > Open Disk path
    /// drives this same surface.
    #[test]
    fn insert_then_eject_disk_round_trip() {
        let mut fdc = Upd765a::new();
        assert!(!fdc.has_disk(0));
        fdc.insert_disk(0, DiskImage::default());
        assert!(fdc.has_disk(0));
        // Other drive remains empty.
        assert!(!fdc.has_disk(1));
        fdc.eject_disk(0);
        assert!(!fdc.has_disk(0));
    }

    /// `set_drive_select_mask` changes how the FDC decodes the
    /// US0/US1 bits in command bytes. The +3 only wires US0 (mask
    /// 0x01) so US1 becomes a don't-care; defaults to all four drives
    /// addressable (mask 0x03).
    #[test]
    fn set_drive_select_mask_affects_command_drive_decode() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        fdc.set_drive_select_mask(0x01); // +3 wiring: US0 only
        fdc.insert_disk(0, DiskImage::default());

        // Issue SenseDriveStatus addressed to "drive 2" — with the +3
        // mask this aliases to drive 0 (which has a disk mounted).
        fdc.write_data(0x04);
        fdc.write_data(0x02);
        let st3 = fdc.read_data();
        assert_eq!(st3 & 0x20, 0x20, "drive 2 aliases drive 0 → RY set");
    }

    /// NEC December 1978 datasheet pp.9,16: invalid commands have a
    /// one-byte ST0 result, with interrupt-code bits 10 (0x80).
    #[test]
    fn invalid_command_returns_one_status_byte_and_accepts_next_command() {
        let mut fdc = Upd765a::new();
        fdc.enabled = true;
        for opcode in [0x00, 0x1B, 0xFF] {
            fdc.write_data(opcode);
            assert_eq!(fdc.read_status(), MSR_RQM | MSR_DIO | MSR_CB);
            // A host write cannot replace an unread result.
            fdc.write_data(0x04);
            assert_eq!(fdc.read_data(), 0x80);
            assert_eq!(fdc.read_status(), MSR_RQM);
            assert_eq!(fdc.read_data(), 0xFF, "exactly one result byte");
        }
        // A normal command after the invalid results must still work.
        fdc.write_data(0x04);
        fdc.write_data(0x00);
        assert_eq!(fdc.read_status(), MSR_RQM | MSR_DIO | MSR_CB);
        assert_eq!(fdc.read_data(), 0x10, "drive zero is on cylinder zero");
        assert_eq!(fdc.read_status(), MSR_RQM);
        // Do not leak the preceding command's status into a new result.
        fdc.write_data(0x1B);
        assert_eq!(fdc.read_data(), 0x80);
        assert_eq!(fdc.read_status(), MSR_RQM);
    }
}
