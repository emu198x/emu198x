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

    /// Disk images (up to 4 drives).
    #[serde(skip)]
    disks: [Option<DiskImage>; 4],
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
        }
    }

    /// Mask the unit-select bits of every command byte against `mask`
    /// before indexing the per-drive state. Use `0x01` on the +3 (only
    /// US0 wired) or leave the default `0x03` (full µPD765A,
    /// US0+US1 → 4 drives).
    pub fn set_drive_select_mask(&mut self, mask: u8) {
        self.drive_select_mask = mask & 0x03;
    }

    /// Insert a parsed disk image into a drive.
    pub fn insert_disk(&mut self, drive: usize, image: DiskImage) {
        if drive < 4 {
            self.disks[drive] = Some(image);
        }
    }

    pub fn eject_disk(&mut self, drive: usize) {
        if drive < 4 {
            self.disks[drive] = None;
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
        for drive in 0..4 {
            if self.seek_remaining[drive] > 0 {
                msr |= 1 << drive;
            }
        }
        msr
    }

    /// Read data register (port $3FFD on +3).
    pub fn read_data(&mut self) -> u8 {
        match self.phase {
            Phase::Execution if self.exec_pos < self.exec_buf.len() => {
                let byte = self.exec_buf[self.exec_pos];
                self.exec_pos += 1;
                if self.exec_pos >= self.exec_buf.len() {
                    self.enter_result_phase();
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
                let (cmd, len) = Self::decode_command(val);
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
                // Write data to sector (write commands)
                if self.exec_pos < self.exec_buf.len() {
                    self.exec_buf[self.exec_pos] = val;
                    self.exec_pos += 1;
                    if self.exec_pos >= self.exec_buf.len() {
                        self.enter_result_phase();
                    }
                }
            }
            Phase::Result => {}
        }
    }

    fn decode_command(byte: u8) -> (Command, usize) {
        match byte & 0x1F {
            0x06 => (Command::ReadData, 9),             // Read Data
            0x0C => (Command::ReadDeletedData, 9),      // Read Deleted Data (Speedlock keys)
            0x05 => (Command::WriteData, 9),            // Write Data
            0x0A => (Command::ReadId, 2),               // Read ID
            0x07 => (Command::Recalibrate, 2),          // Recalibrate
            0x08 => (Command::SenseInterruptStatus, 1), // Sense Interrupt Status
            0x03 => (Command::Specify, 3),              // Specify
            0x0F => (Command::SeekTrack, 3),            // Seek
            0x04 => (Command::SenseDriveStatus, 2),     // Sense Drive Status
            _ => (Command::None, 1),
        }
    }

    fn execute_command(&mut self) {
        match self.command {
            Command::ReadData | Command::ReadDeletedData => {
                // `drive` indexes our per-drive state (disks/track) and
                // is masked by the host's wiring (+3 only routes US0,
                // so US1 is a don't-care). `drive_echo` keeps the
                // original US0:US1 bits the BIOS sent, so ST0's drive
                // bits reflect what the host requested even when the
                // physical drive is an alias.
                let drive_echo = self.cmd_buf[1] & 0x03;
                let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
                let head = (self.cmd_buf[1] >> 2) & 0x01;
                // Real µPD765A reads from the physical cylinder the
                // head is over (set by the last SeekTrack /
                // Recalibrate), not from `cmd_buf[2]` — the `C`
                // parameter is the *expected* cylinder header value
                // the chip then verifies against the address mark.
                // Speedlock-protected disks deliberately record
                // cylinder values that don't match physical track
                // number, and the loader knows the encoding; using
                // `cmd_buf[2]` here means we'd look up sectors on the
                // wrong physical track and miss the protected ones.
                let track = self.track[drive];
                let _expected_c = self.cmd_buf[2];
                let sector = self.cmd_buf[4]; // R (start sector ID)
                let n = self.cmd_buf[5]; // N (sector size code)
                let eot = self.cmd_buf[6]; // EOT (last sector to read)
                let sector_size = 128usize << (n as usize);

                // Command byte carries MT (bit 7), MFM (bit 6) and SK
                // (bit 5). SK changes how the chip handles a sector
                // whose recorded data mark *doesn't* match the
                // command's expected mark: SK=1 → skip silently and
                // advance R, SK=0 → read it anyway but set ST2.CM in
                // the result. Speedlock probes the disk by issuing
                // both commands and watching which sectors deliver
                // data; getting SK or the mark-match wrong shows up
                // as a CRC mismatch in the loader's check pass.
                let cmd_byte = self.cmd_buf[0];
                let sk = (cmd_byte & 0x20) != 0;
                let want_deleted = matches!(self.command, Command::ReadDeletedData);

                self.head = head;
                self.sector = sector;

                // Multi-sector read: real µPD765A reads sectors R..=EOT
                // back-to-back in one Execution phase, only stopping
                // (and entering Result) when R passes EOT or the host
                // signals TC. The +3 BIOS's second-stage loader relies
                // on this — it asks for sectors 2..9 in a single
                // command and would miss seven sectors of program data
                // if we stopped after the first.
                let mut buf = Vec::with_capacity(sector_size * 8);
                let mut last_ok = sector;
                let mut missing_after_some = false;
                // Did at least one sector during this run have a
                // mismatched mark relative to the command? When this
                // is set (and SK=0) the chip sets ST2.CM in the
                // result phase, terminating after that sector.
                let mut hit_mark_mismatch = false;
                // Recorded ST1/ST2 from the last sector delivered.
                // The chip OR's the address-mark error bits from each
                // sector it reads into the eventual result-phase ST1
                // and ST2 — so CRC errors recorded in the EDSK SIL
                // ("this sector has bad data CRC") surface to the
                // host exactly as the real drive would. Speedlock
                // probes by reading sectors expected to flag DE/DD
                // and bails when the chip claims a clean read.
                let mut acc_st1: u8 = 0;
                let mut acc_st2: u8 = 0;
                let mut r = sector;
                loop {
                    // Look up the recorded sector entry — we need its
                    // ST2 to decide whether the mark matches before
                    // copying data into the exec buffer.
                    let sec = self
                        .disks
                        .get(drive)
                        .and_then(|d| d.as_ref())
                        .and_then(|img| img.tracks.get(head as usize))
                        .and_then(|side| side.get(track as usize))
                        .and_then(|trk| trk.sectors.iter().find(|s| s.id == r));

                    let Some(sec) = sec else {
                        // Sector ID not found on this track. For
                        // multi-sector runs we treat the first miss
                        // as a hard stop (real chip sets ST1.ND and
                        // terminates with abnormal completion).
                        if !buf.is_empty() {
                            missing_after_some = true;
                        }
                        break;
                    };

                    let sec_is_deleted = (sec.st2 & ST2_CM) != 0;
                    let mark_match = sec_is_deleted == want_deleted;
                    if !mark_match {
                        if sk {
                            // Skip this sector silently, advance R.
                            // The real chip can still terminate at EOT
                            // even though it skipped, so we honour the
                            // same loop end condition.
                            if r >= eot {
                                break;
                            }
                            r = r.wrapping_add(1);
                            continue;
                        }
                        // SK=0: read the sector but flag CM and
                        // terminate after this one (datasheet: "read
                        // ID information…until the FDC encounters
                        // mismatch, then stop").
                        hit_mark_mismatch = true;
                    }

                    let take = sec.data.len().min(sector_size);
                    buf.extend_from_slice(&sec.data[..take]);
                    if take < sector_size {
                        let fill = sec.data.last().copied().unwrap_or(0);
                        buf.resize(buf.len() + (sector_size - take), fill);
                    }
                    last_ok = r;
                    // Mix in whatever error bits the SIL recorded for
                    // this sector. DE (ST1.5) / DD (ST2.5) terminate
                    // the multi-sector run the same way CM does — the
                    // chip stops on the first sector whose data field
                    // has a CRC error.
                    acc_st1 |= sec.st1;
                    acc_st2 |= sec.st2;
                    let sector_has_data_error = (sec.st1 & 0x20) != 0 || (sec.st2 & 0x20) != 0;

                    if hit_mark_mismatch || sector_has_data_error {
                        break;
                    }
                    if r >= eot {
                        break;
                    }
                    r = r.wrapping_add(1);
                }

                if !buf.is_empty() {
                    self.exec_buf = buf;
                    self.exec_pos = 0;
                    self.phase = Phase::Execution;
                    self.main_status = MSR_RQM | MSR_EXM | MSR_DIO | MSR_CB;
                    self.st0 = (head << 2) | drive_echo;
                    // Carry forward the OR'd-in error bits from every
                    // sector read in the run, then layer the
                    // multi-sector outcome flags on top.
                    self.st1 = acc_st1;
                    self.st2 = acc_st2 & !ST2_CM; // CM gets set below if applicable
                    if missing_after_some {
                        self.st0 |= 0x40; // Abnormal termination
                        self.st1 |= 0x04; // ND — caller learns where the run stopped
                    } else if hit_mark_mismatch {
                        // Datasheet: CM set + Abnormal Termination
                        // when SK=0 and the mark didn't match.
                        self.st0 |= 0x40;
                        self.st2 |= ST2_CM;
                    } else if (acc_st1 & 0x20) != 0 || (acc_st2 & 0x20) != 0 {
                        // Real chip flags Abnormal Termination when a
                        // sector's data CRC fails, even though it
                        // still delivers the (corrupted) bytes.
                        self.st0 |= 0x40;
                    }
                    // Result phase echoes the next sector ID after
                    // the one last successfully delivered — what real
                    // hardware reports for multi-sector terminations.
                    self.cmd_buf[4] = last_ok.wrapping_add(1);
                } else {
                    // No sector found at all (or every one was
                    // skipped via SK) — skip Execution and go straight
                    // to Result with abnormal termination.
                    self.st0 = 0x40 | (head << 2) | drive_echo;
                    self.st1 = 0x04;
                    self.st2 = 0;
                    self.setup_result_read(track, head, sector, n);
                }
            }
            Command::Recalibrate => {
                let drive_echo = self.cmd_buf[1] & 0x03;
                let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
                self.track[drive] = 0;
                // ST0 IC bits: 00 = Normal Termination (real drive
                // present, seek to track 0 succeeded). 11 + EC bit
                // (0xD0 | drive) = Abnormal due to Drive Not Ready —
                // what the real µPD765A returns when a recalibrate is
                // issued against a drive whose TRACK 0 signal never
                // asserts (no drive connected). The +3 BIOS probes
                // drives by recalibrating each in turn; falsely
                // returning Normal Termination for non-existent drives
                // makes it think every drive is real.
                let st0 = if self.disks[drive].is_some() {
                    0x20 | drive_echo // Seek End | drive
                } else {
                    0xD0 | drive_echo // Abnormal | Not Ready | EC | drive
                };
                self.st0 = st0;
                // Stage the seek; the busy bit is read out of MSR via
                // `seek_remaining`, and the interrupt is queued only
                // after the countdown completes via `tick()`.
                self.seek_staged_st0[drive] = Some(st0);
                self.seek_remaining[drive] = SEEK_TICKS;
                self.phase = Phase::Idle;
                self.main_status = MSR_RQM;
            }
            Command::SenseInterruptStatus => {
                self.result_buf.clear();
                if let Some(drive) = (0..4).find(|d| self.seek_pending[*d].is_some()) {
                    // Drain one pending seek interrupt.
                    let st0 = self.seek_pending[drive].take().unwrap();
                    self.st0 = st0;
                    self.result_buf.push(st0);
                    self.result_buf.push(self.track[drive]);
                } else {
                    // No pending interrupt — return ST0 = 0x80 (Invalid
                    // Command). Per the µPD765A datasheet this tells the
                    // BIOS the interrupt queue is drained.
                    self.st0 = 0x80;
                    self.result_buf.push(0x80);
                    self.result_buf.push(0);
                }
                self.result_pos = 0;
                self.phase = Phase::Result;
                self.main_status = MSR_RQM | MSR_DIO | MSR_CB;
                self.interrupt = self.seek_pending.iter().any(Option::is_some);
            }
            Command::Specify => {
                // Just accept the parameters (step rate, head load/unload times)
                self.phase = Phase::Idle;
                self.main_status = MSR_RQM;
            }
            Command::SeekTrack => {
                let drive_echo = self.cmd_buf[1] & 0x03;
                let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
                let new_track = self.cmd_buf[2];
                self.track[drive] = new_track;
                let st0 = 0x20 | drive_echo; // Seek End | drive
                self.st0 = st0;
                self.seek_staged_st0[drive] = Some(st0);
                self.seek_remaining[drive] = SEEK_TICKS;
                self.phase = Phase::Idle;
                self.main_status = MSR_RQM;
            }
            Command::SenseDriveStatus => {
                let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
                let head = (self.cmd_buf[1] >> 2) & 0x01;
                let disk_present = self.disks[drive].is_some();
                self.st3 = (self.cmd_buf[1] & 0x07)        // US0/US1/HD copied from command
                    | if self.track[drive] == 0 { 0x10 } else { 0 } // T0 (track 0)
                    | if disk_present { 0x08 | 0x20 } else { 0 };   // TS (two-sided) + RY
                self.head = head;
                self.result_buf.clear();
                self.result_buf.push(self.st3);
                self.result_pos = 0;
                self.phase = Phase::Result;
                self.main_status = MSR_RQM | MSR_DIO | MSR_CB;
            }
            Command::ReadId => {
                // Return the address-mark CHRN of the next sector the
                // head would see on the rotating medium. We don't
                // model rotation, so use the first sector in the
                // current track — but the *recorded* CHRN values, not
                // hardcoded (C=track, R=1, N=2) defaults. Copy
                // protection (Speedlock & friends) routinely records
                // C / H / N values that don't match the physical
                // track or sector size, and the loader compares the
                // ReadID result against expected bytes to decide
                // whether the disk has been tampered with. Hardcoded
                // defaults fail that check and drop the loader into
                // its error hang.
                let drive_echo = self.cmd_buf[1] & 0x03;
                let drive = (self.cmd_buf[1] & self.drive_select_mask) as usize;
                let head = (self.cmd_buf[1] >> 2) & 0x01;
                self.st1 = 0;
                self.st2 = 0;

                let recorded = self
                    .disks
                    .get(drive)
                    .and_then(|d| d.as_ref())
                    .and_then(|img| img.tracks.get(head as usize))
                    .and_then(|side| side.get(self.track[drive] as usize))
                    .and_then(|trk| trk.sectors.first());

                let (c, h, r, n) = match recorded {
                    Some(s) => (s.c, s.h, s.id, s.n),
                    // No disk or empty track — return the defaults so
                    // a probe-during-empty-drive still gets a coherent
                    // (if blank) reply.
                    None => (self.track[drive], head, 1, 2),
                };

                self.st0 = (head << 2) | drive_echo;
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
            }
            _ => {
                // Unknown command — return to idle
                self.phase = Phase::Idle;
                self.main_status = MSR_RQM;
            }
        }
    }

    fn enter_result_phase(&mut self) {
        let _drive = (self.cmd_buf[1] & 0x03) as usize;
        let track = self.cmd_buf[2];
        let head = (self.cmd_buf[1] >> 2) & 0x01;
        let sector = self.cmd_buf[4];
        let n = self.cmd_buf[5];
        self.setup_result_read(track, head, sector, n);
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
        for drive in 0..4 {
            if self.seek_remaining[drive] == 0 {
                continue;
            }
            self.seek_remaining[drive] -= 1;
            if self.seek_remaining[drive] == 0 {
                if let Some(st0) = self.seek_staged_st0[drive].take() {
                    self.seek_pending[drive] = Some(st0);
                    self.interrupt = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // the interrupt is held until the seek-busy countdown expires
        // and the drive-busy bit clears in MSR.
        assert_eq!(fdc.track[0], 0);
        assert!(!fdc.interrupt);
        assert_eq!(fdc.read_status() & 0x01, 0x01); // drive 0 busy

        for _ in 0..SEEK_TICKS {
            <Upd765a as Peripheral>::tick(&mut fdc, 0);
        }
        assert!(fdc.interrupt);
        assert_eq!(fdc.read_status() & 0x01, 0); // drive 0 idle
    }

    #[test]
    fn sense_interrupt_drains_pending_then_returns_invalid_command() {
        let mut fdc = Upd765a::new();
        // Stage a pending seek interrupt for drive 0 (Seek End).
        fdc.seek_pending[0] = Some(0x20);

        fdc.write_data(0x08); // Sense Interrupt Status
        assert_eq!(fdc.phase, Phase::Result);
        assert_eq!(fdc.read_data(), 0x20); // ST0
        assert_eq!(fdc.read_data(), 0); // PCN

        // Second SenseInt with no pending must return ST0 = 0x80
        // (Invalid Command) per the µPD765A datasheet — that's how the
        // BIOS knows the interrupt queue is drained.
        fdc.write_data(0x08);
        assert_eq!(fdc.read_data(), 0x80);
        assert_eq!(fdc.read_data(), 0);
    }

    #[test]
    fn sense_interrupt_walks_drives_in_order() {
        let mut fdc = Upd765a::new();
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
        assert_eq!(fdc.read_data(), 0);
    }

    #[test]
    fn read_sector() {
        let mut fdc = Upd765a::new();

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

        let first = fdc.read_data();
        assert_eq!(first, 0xDE);

        for _ in 1..511 {
            fdc.read_data();
        }
        let last = fdc.read_data();
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
        assert_eq!(fdc.read_data(), 0xEE);
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
        let fdc = Upd765a::new();
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
        let bytes_a: Vec<u8> = (0..256).map(|_| a.read_data()).collect();
        assert_eq!(bytes_a[0], 0x11, "sector 1 (DAM) first byte");
        assert_eq!(bytes_a[127], 0x11, "sector 1 last byte");
        assert_eq!(bytes_a[128], 0x33, "sector 3 (DAM) first — sector 2 (DDAM) was skipped");

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
        let bytes_b: Vec<u8> = (0..256).map(|_| b.read_data()).collect();
        assert_eq!(bytes_b[0], 0x11, "sector 1 (DAM) delivered");
        assert_eq!(bytes_b[128], 0x22, "sector 2 (DDAM) also delivered when SK=0");
        // Now in result phase — read the 7-byte status block.
        assert_eq!(b.phase, Phase::Result);
        let st0 = b.read_data();
        let _st1 = b.read_data();
        let st2 = b.read_data();
        assert_ne!(st0 & 0xC0, 0, "ST0 abnormal-termination IC set on mark mismatch");
        assert_ne!(st2 & ST2_CM, 0, "ST2.CM set when ReadData found a DDAM with SK=0");

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
        let bytes_c: Vec<u8> = (0..128).map(|_| c.read_data()).collect();
        assert_eq!(bytes_c[0], 0x22, "sector 2 (DDAM) delivered by ReadDeletedData");
        assert_eq!(bytes_c[127], 0x22);
    }
}
