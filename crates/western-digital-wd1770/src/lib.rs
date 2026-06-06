//! Western Digital WD1770 floppy disk controller.
//!
//! The WD1770 is the single-chip member of the WD177x/179x family (it folds
//! the data separator and write precompensation onto the die). It drives the
//! built-in 3" floppy of the **Tatung Einstein** (chip I042, "WD1770-PH"), and
//! the same command set covers its siblings (WD1772, WD1793, WD2797 …).
//!
//! # What is modelled
//!
//! The **register interface and the full command set** at the level software
//! actually observes:
//!
//! - **Type I** — Restore, Seek, Step, Step-In, Step-Out (track-register update
//!   per the `u` bit; Track-0 flag).
//! - **Type II** — Read Sector, Write Sector (single and multi-sector `m`).
//! - **Type III** — Read Address (the 6-byte ID field, with a real CRC-CCITT),
//!   Read Track, Write Track (the last two are accepted and complete cleanly but
//!   are not bit-cell modelled — they exist for formatting, which a flat sector
//!   image has no use for).
//! - **Type IV** — Force Interrupt.
//!
//! Status-register semantics are composed per command type (the WD177x reuses
//! the same status bits with different meanings for Type I vs Type II/III), and
//! `INTRQ` / `DRQ` are exposed as pins.
//!
//! # What is *not* modelled (relaxed timing)
//!
//! Timing is a **relaxed cycle-countdown**, not raw MFM bit-cell timing: each
//! command settles after a fixed host-cycle budget rather than being clocked off
//! a real index/byte cell rate, and `LOST DATA` (host fails to service `DRQ` in
//! time) is never raised. The once-per-revolution `INDEX` pulse is synthesised
//! so idle Type-I polling sees a spinning drive. This is enough for a sector
//! image to seek/read/write correctly; it is not enough to reproduce
//! copy-protection that times the bit stream.
//!
//! # Reference
//!
//! Command decode and status semantics cross-checked against MAME's
//! `src/devices/machine/wd_fdc.cpp` (the `0x00`-`0xf0` command table) and the
//! Einstein wiring in `src/mame/tatung/einstein.cpp` (FDC at ports `$18`-`$1B`,
//! drive/side latch at `$23`).

#![forbid(unsafe_code)]

// ---------------------------------------------------------------------------
// Status register bits
// ---------------------------------------------------------------------------
//
// The WD1770 reuses the same bit positions with different meanings depending on
// whether a Type I (head-movement) or Type II/III (data-transfer) command is in
// progress.

const ST_BUSY: u8 = 0x01;
const ST_DRQ: u8 = 0x02; // Type II/III: data request
const ST_INDEX: u8 = 0x02; // Type I (idle): index pulse — same bit
const ST_TRACK0: u8 = 0x04; // Type I: head over track 0
const ST_RECORD_NOT_FOUND: u8 = 0x10; // Type II/III — same bit as seek error
const ST_WRITE_PROTECT: u8 = 0x40;
const ST_MOTOR_ON: u8 = 0x80; // WD1770 has no READY input; bit 7 is Motor On

// The remaining status bits are part of the faithful register map but are never
// raised by the relaxed timing model (a clean flat image has no CRC/seek
// failures, the host never under-runs `DRQ`, and spin-up/deleted-data marks are
// not synthesised). Kept named so the layout is complete and future work can
// raise them.
#[allow(dead_code)]
mod unmodelled_status {
    pub const ST_LOST_DATA: u8 = 0x04; // Type II/III: host failed to service DRQ
    pub const ST_CRC_ERROR: u8 = 0x08;
    pub const ST_SEEK_ERROR: u8 = 0x10; // Type I
    pub const ST_SPIN_UP: u8 = 0x20; // Type I: spin-up complete
    pub const ST_RECORD_TYPE: u8 = 0x20; // Type II read: deleted-data mark
}

/// Host cycles a command "runs" before it settles. Relaxed — the real settle
/// depends on the step rate and disk rotation; this is a fixed budget chosen so
/// polling software sees `BUSY` assert and clear.
const COMMAND_CYCLES: u32 = 64;

/// Free-running period of the synthesised index pulse, in host cycles, and the
/// width of the asserted window. The Einstein MOS polls the Type-I index bit to
/// confirm the drive is spinning before it issues seeks.
const INDEX_PERIOD: u32 = 6000;
const INDEX_WIDTH: u32 = 400;

// ---------------------------------------------------------------------------
// Disk
// ---------------------------------------------------------------------------

/// A floppy image as a flat, side-interleaved sector dump plus its geometry.
///
/// Sectors are stored track-major, then side, then sector:
/// `(((track * sides) + side) * sectors_per_track + (id - first_sector_id)) * sector_size`.
/// This matches the common `.dsk`/raw layout for fixed-geometry CP/M disks.
#[derive(Clone, Debug)]
pub struct Disk {
    data: Vec<u8>,
    tracks: usize,
    sides: usize,
    sectors_per_track: usize,
    sector_size: usize,
    /// Lowest sector ID on a track (1 on the Einstein; some systems use 0 or
    /// higher, e.g. CP/M 1-based or IBM-style high bases).
    first_sector_id: u8,
    write_protected: bool,
    /// Set when a Write Sector has modified `data` since the last `take_dirty`.
    dirty: bool,
}

impl Disk {
    /// Build a disk from a flat sector dump. `data` must be at least
    /// `tracks * sides * sectors_per_track * sector_size` bytes; trailing bytes
    /// are ignored, a short buffer simply makes the missing sectors unreadable.
    #[must_use]
    pub fn new(
        data: Vec<u8>,
        tracks: usize,
        sides: usize,
        sectors_per_track: usize,
        sector_size: usize,
    ) -> Self {
        Self {
            data,
            tracks,
            sides,
            sectors_per_track,
            sector_size,
            first_sector_id: 1,
            write_protected: false,
            dirty: false,
        }
    }

    /// Set the lowest sector ID used on a track (default 1).
    #[must_use]
    pub fn with_first_sector_id(mut self, id: u8) -> Self {
        self.first_sector_id = id;
        self
    }

    /// Mark the image read-only; Write Sector then reports write-protect.
    #[must_use]
    pub fn write_protected(mut self, protected: bool) -> Self {
        self.write_protected = protected;
        self
    }

    /// Sector size in bytes.
    #[must_use]
    pub fn sector_size(&self) -> usize {
        self.sector_size
    }

    /// Whether a Write Sector has modified the image since the last
    /// [`take_dirty`](Self::take_dirty); useful for write-back to a file.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clear and return the dirty flag.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.dirty, false)
    }

    /// The backing bytes (for write-back to a file).
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Byte offset of (track, side, sector-id) in the flat dump, or `None` when
    /// the address is outside the geometry or the buffer is too short.
    fn offset(&self, track: usize, side: usize, sector_id: u8) -> Option<usize> {
        if side >= self.sides || track >= self.tracks {
            return None;
        }
        if sector_id < self.first_sector_id {
            return None;
        }
        let sector_index = usize::from(sector_id - self.first_sector_id);
        if sector_index >= self.sectors_per_track {
            return None;
        }
        let track_index = track * self.sides + side;
        let off = (track_index * self.sectors_per_track + sector_index) * self.sector_size;
        (off + self.sector_size <= self.data.len()).then_some(off)
    }
}

/// Map a sector size in bytes to the WD177x ID-field length code.
fn sector_size_code(size: usize) -> u8 {
    match size {
        128 => 0,
        256 => 1,
        1024 => 3,
        _ => 2, // 512 — the common default; also the fallback for odd sizes
    }
}

// ---------------------------------------------------------------------------
// Command engine
// ---------------------------------------------------------------------------

/// What the active command is doing once it settles, so [`tick`](Wd1770::tick)
/// knows how to finish it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Pending {
    /// No data phase: settle to an idle Type-I status.
    #[default]
    TypeI,
    /// Raise DRQ and stream `buf` to the host (Read Sector / Read Address).
    Read,
    /// Raise DRQ and accept `buf.capacity()` bytes from the host (Write Sector).
    Write,
    /// Settle reporting record-not-found.
    RecordNotFound,
    /// Settle reporting write-protect.
    WriteProtect,
}

/// Western Digital WD1770 floppy disk controller.
#[derive(Debug)]
pub struct Wd1770 {
    // Software-visible registers.
    status: u8,
    track: u8,
    sector: u8,
    data: u8,

    // Physical state.
    /// Head position; the track *register* can differ mid-seek or after a Step
    /// without update.
    head_track: u8,
    /// Last step direction: `true` = inward (towards higher track numbers).
    step_in: bool,
    side: u8,
    drive: usize,
    disks: [Option<Disk>; 4],

    // Pins.
    intrq: bool,
    drq: bool,

    // Active-command bookkeeping.
    busy_cycles: u32,
    pending: Pending,
    /// Whether the settled command keeps `BUSY` and enters a data phase.
    multi: bool,
    /// Transfer buffer (Read: bytes to hand out; Write: bytes accepted so far).
    buf: Vec<u8>,
    pos: usize,
    /// Sector being read/written for the data phase (for multi-sector advance).
    xfer_sector: u8,

    // Idle index-pulse synthesis.
    index_counter: u32,
}

impl Default for Wd1770 {
    fn default() -> Self {
        Self {
            status: 0,
            track: 0,
            sector: 1,
            data: 0,
            head_track: 0,
            step_in: true,
            side: 0,
            drive: 0,
            disks: [None, None, None, None],
            intrq: false,
            drq: false,
            busy_cycles: 0,
            pending: Pending::TypeI,
            multi: false,
            buf: Vec::new(),
            pos: 0,
            xfer_sector: 1,
            index_counter: 0,
        }
    }
}

impl Wd1770 {
    /// Create a controller with no disks inserted.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // -- Media --

    /// Insert a disk into a drive (0-3). A higher index is ignored.
    pub fn insert_disk(&mut self, drive: usize, disk: Disk) {
        if let Some(slot) = self.disks.get_mut(drive) {
            *slot = Some(disk);
        }
    }

    /// Remove and return the disk in a drive, if any.
    pub fn eject_disk(&mut self, drive: usize) -> Option<Disk> {
        self.disks.get_mut(drive).and_then(Option::take)
    }

    /// Borrow the disk in a drive (for write-back inspection).
    #[must_use]
    pub fn disk(&self, drive: usize) -> Option<&Disk> {
        self.disks.get(drive).and_then(Option::as_ref)
    }

    /// Mutably borrow the disk in a drive.
    pub fn disk_mut(&mut self, drive: usize) -> Option<&mut Disk> {
        self.disks.get_mut(drive).and_then(Option::as_mut)
    }

    // -- External glue (host decodes its own drive-select latch) --

    /// Select the active drive (0-3). The host decodes its own latch; on the
    /// Einstein the `$23` write picks the drive in bits 0-3.
    pub fn set_drive(&mut self, drive: usize) {
        if drive < self.disks.len() {
            self.drive = drive;
        }
    }

    /// Select the active side (0 or 1). On the Einstein this is `$23` bit 4.
    pub fn set_side(&mut self, side: u8) {
        self.side = u8::from(side != 0);
    }

    // -- Pins --

    /// Interrupt request — asserted at command completion, cleared by reading
    /// the status register or loading a new command.
    #[must_use]
    pub fn intrq(&self) -> bool {
        self.intrq
    }

    /// Data request — asserted while a byte is waiting (read) or expected
    /// (write).
    #[must_use]
    pub fn drq(&self) -> bool {
        self.drq
    }

    // -- Observation (debug / MCP) --

    /// Current status register without the side effects of a port read.
    #[must_use]
    pub fn peek_status(&self) -> u8 {
        self.status
    }

    /// Track register.
    #[must_use]
    pub fn track_register(&self) -> u8 {
        self.track
    }

    /// Physical head position.
    #[must_use]
    pub fn head_track(&self) -> u8 {
        self.head_track
    }

    // -- Register interface (ports $18-$1B on the Einstein) --

    /// Read a controller register: 0 = status, 1 = track, 2 = sector, 3 = data.
    pub fn read(&mut self, reg: u8) -> u8 {
        match reg & 0x03 {
            0 => {
                // Reading status clears INTRQ.
                self.intrq = false;
                self.status
            }
            1 => self.track,
            2 => self.sector,
            3 => self.read_data(),
            _ => unreachable!(),
        }
    }

    /// Write a controller register: 0 = command, 1 = track, 2 = sector,
    /// 3 = data.
    pub fn write(&mut self, reg: u8, value: u8) {
        match reg & 0x03 {
            0 => self.command(value),
            1 => self.track = value,
            2 => self.sector = value,
            3 => self.write_data(value),
            _ => unreachable!(),
        }
    }

    fn read_data(&mut self) -> u8 {
        if self.pending == Pending::Read && self.pos < self.buf.len() {
            self.data = self.buf[self.pos];
            self.pos += 1;
            if self.pos >= self.buf.len() {
                self.drq = false;
                self.finish_read_phase();
            }
        }
        self.data
    }

    fn write_data(&mut self, value: u8) {
        self.data = value;
        if self.pending == Pending::Write && self.drq && self.pos < self.buf.len() {
            self.buf[self.pos] = value;
            self.pos += 1;
            if self.pos >= self.buf.len() {
                self.drq = false;
                self.commit_write_phase();
            }
        }
    }

    // -- Command decode (matches MAME wd_fdc command table) --

    fn command(&mut self, cmd: u8) {
        // Force Interrupt is accepted at any time; every other command is
        // ignored while one is BUSY.
        if cmd & 0xF0 == 0xD0 {
            self.force_interrupt(cmd);
            return;
        }
        if self.status & ST_BUSY != 0 {
            return;
        }

        // Loading a command clears INTRQ.
        self.intrq = false;
        self.drq = false;
        self.buf.clear();
        self.pos = 0;
        self.multi = false;

        match cmd & 0xF0 {
            0x00 => self.start_restore(cmd),
            0x10 => self.start_seek(cmd),
            0x20 | 0x30 => self.start_step(cmd, None),
            0x40 | 0x50 => self.start_step(cmd, Some(true)),
            0x60 | 0x70 => self.start_step(cmd, Some(false)),
            0x80 | 0x90 => self.start_read_sector(cmd),
            0xA0 | 0xB0 => self.start_write_sector(cmd),
            0xC0 => self.start_read_address(),
            0xE0 => self.start_read_track(),
            0xF0 => self.start_write_track(),
            _ => unreachable!(),
        }
    }

    // -- Type I: head movement --

    fn begin_type_i(&mut self, pending: Pending) {
        self.busy_cycles = COMMAND_CYCLES;
        self.status = ST_MOTOR_ON | ST_BUSY;
        self.pending = pending;
    }

    fn start_restore(&mut self, _cmd: u8) {
        self.head_track = 0;
        self.track = 0;
        self.begin_type_i(Pending::TypeI);
    }

    fn start_seek(&mut self, cmd: u8) {
        // Seek to the track held in the data register.
        let target = self.data;
        self.step_in = target >= self.head_track;
        self.head_track = target;
        if cmd & 0x10 != 0 {
            self.track = self.head_track;
        } else {
            // SEEK always updates the track register (it is the whole point);
            // the u bit only differentiates the STEP variants. Keep the
            // register in step with the head.
            self.track = self.head_track;
        }
        self.begin_type_i(Pending::TypeI);
    }

    fn start_step(&mut self, cmd: u8, dir_in: Option<bool>) {
        if let Some(dir) = dir_in {
            self.step_in = dir;
        }
        if self.step_in {
            self.head_track = self.head_track.saturating_add(1);
        } else {
            self.head_track = self.head_track.saturating_sub(1);
        }
        // Bit 4 (u) = update the track register to follow the head.
        if cmd & 0x10 != 0 {
            self.track = self.head_track;
        }
        self.begin_type_i(Pending::TypeI);
    }

    // -- Type II: sector transfer --

    fn start_read_sector(&mut self, cmd: u8) {
        self.multi = cmd & 0x10 != 0;
        self.xfer_sector = self.sector;
        self.busy_cycles = COMMAND_CYCLES;
        self.status = ST_MOTOR_ON | ST_BUSY;

        match self.fetch_sector(self.xfer_sector) {
            Some(bytes) => {
                self.buf = bytes;
                self.pos = 0;
                self.pending = Pending::Read;
            }
            None => self.pending = Pending::RecordNotFound,
        }
    }

    fn start_write_sector(&mut self, cmd: u8) {
        self.multi = cmd & 0x10 != 0;
        self.xfer_sector = self.sector;
        self.busy_cycles = COMMAND_CYCLES;
        self.status = ST_MOTOR_ON | ST_BUSY;

        let size = self.disks[self.drive].as_ref().map(Disk::sector_size);
        let protected = self.disks[self.drive]
            .as_ref()
            .is_some_and(|d| d.write_protected);

        match (size, protected) {
            (_, true) => self.pending = Pending::WriteProtect,
            (Some(size), false) if self.sector_addressable(self.xfer_sector) => {
                self.buf = vec![0u8; size];
                self.pos = 0;
                self.pending = Pending::Write;
            }
            _ => self.pending = Pending::RecordNotFound,
        }
    }

    // -- Type III --

    fn start_read_address(&mut self) {
        self.busy_cycles = COMMAND_CYCLES;
        self.status = ST_MOTOR_ON | ST_BUSY;

        if let Some(disk) = self.disks[self.drive].as_ref() {
            // Report the first ID field on the current track: the head's track
            // number, the selected side, the lowest sector ID, the size code,
            // and a real CRC over the ID address mark + fields.
            let id_track = self.head_track;
            let id_side = self.side;
            let id_sector = disk.first_sector_id;
            let size_code = sector_size_code(disk.sector_size);
            let crc = id_field_crc(id_track, id_side, id_sector, size_code);
            self.buf = vec![
                id_track,
                id_side,
                id_sector,
                size_code,
                (crc >> 8) as u8,
                (crc & 0xFF) as u8,
            ];
            self.pos = 0;
            self.pending = Pending::Read;
            // Per the datasheet, Read Address loads the ID's track number into
            // the sector register.
            self.sector = id_track;
        } else {
            self.pending = Pending::RecordNotFound;
        }
    }

    fn start_read_track(&mut self) {
        // Raw-track read is not bit-cell modelled; accept and settle cleanly.
        self.begin_type_i(Pending::TypeI);
    }

    fn start_write_track(&mut self) {
        // Formatting against a fixed-geometry flat image is a no-op; accept and
        // settle (honouring write-protect so software sees a sane result).
        let protected = self.disks[self.drive]
            .as_ref()
            .is_some_and(|d| d.write_protected);
        self.busy_cycles = COMMAND_CYCLES;
        self.status = ST_MOTOR_ON | ST_BUSY;
        self.pending = if protected {
            Pending::WriteProtect
        } else {
            Pending::TypeI
        };
    }

    // -- Type IV --

    fn force_interrupt(&mut self, _cmd: u8) {
        self.status &= !ST_BUSY;
        self.busy_cycles = 0;
        self.drq = false;
        self.pending = Pending::TypeI;
        self.buf.clear();
        self.pos = 0;
        self.multi = false;
        // Settle to an idle Type-I status (Motor still on; Track-0 if applicable).
        self.status = ST_MOTOR_ON;
        if self.head_track == 0 {
            self.status |= ST_TRACK0;
        }
        self.intrq = true;
    }

    // -- Helpers --

    fn sector_addressable(&self, sector: u8) -> bool {
        self.disks[self.drive]
            .as_ref()
            .and_then(|d| d.offset(self.head_track as usize, self.side as usize, sector))
            .is_some()
    }

    fn fetch_sector(&self, sector: u8) -> Option<Vec<u8>> {
        let disk = self.disks[self.drive].as_ref()?;
        let off = disk.offset(self.head_track as usize, self.side as usize, sector)?;
        Some(disk.data[off..off + disk.sector_size].to_vec())
    }

    /// Called when a read data phase drains; advances to the next sector for a
    /// multi-sector read or settles the command.
    fn finish_read_phase(&mut self) {
        if self.multi {
            let next = self.xfer_sector.wrapping_add(1);
            if let Some(bytes) = self.fetch_sector(next) {
                self.xfer_sector = next;
                self.sector = next;
                self.buf = bytes;
                self.pos = 0;
                // Re-enter the data phase: keep BUSY, DRQ re-raised by tick.
                self.busy_cycles = COMMAND_CYCLES;
                self.status = ST_MOTOR_ON | ST_BUSY;
                self.pending = Pending::Read;
                return;
            }
        }
        self.status = ST_MOTOR_ON;
        self.pending = Pending::TypeI;
        self.intrq = true;
    }

    /// Called when a write data phase fills; commits the buffer to the image and
    /// advances or settles.
    fn commit_write_phase(&mut self) {
        let (track, side, sector) = (
            self.head_track as usize,
            self.side as usize,
            self.xfer_sector,
        );
        if let Some(disk) = self.disks[self.drive].as_mut()
            && let Some(off) = disk.offset(track, side, sector)
        {
            disk.data[off..off + disk.sector_size].copy_from_slice(&self.buf);
            disk.dirty = true;
        }
        if self.multi {
            let next = sector.wrapping_add(1);
            if self.sector_addressable(next) {
                self.xfer_sector = next;
                self.sector = next;
                self.buf.iter_mut().for_each(|b| *b = 0);
                self.pos = 0;
                self.busy_cycles = COMMAND_CYCLES;
                self.status = ST_MOTOR_ON | ST_BUSY;
                self.pending = Pending::Write;
                return;
            }
        }
        self.status = ST_MOTOR_ON;
        self.pending = Pending::TypeI;
        self.intrq = true;
    }

    // -- Timing --

    /// Advance one host cycle. Counts down the active command and synthesises
    /// the idle index pulse.
    pub fn tick(&mut self) {
        if self.busy_cycles > 0 {
            self.busy_cycles -= 1;
            if self.busy_cycles == 0 {
                self.settle();
            }
            return;
        }

        // Idle: synthesise the rotating INDEX pulse in the Type-I status. Only
        // while no data transfer is in flight (the bit doubles as DRQ).
        self.index_counter = self.index_counter.wrapping_add(1);
        if self.status & ST_BUSY == 0 && !self.drq {
            if self.index_counter % INDEX_PERIOD < INDEX_WIDTH {
                self.status |= ST_INDEX;
            } else {
                self.status &= !ST_INDEX;
            }
        }
    }

    /// Apply the settled status when a command's cycle budget expires.
    fn settle(&mut self) {
        match self.pending {
            Pending::TypeI => {
                self.status = ST_MOTOR_ON;
                if self.head_track == 0 {
                    self.status |= ST_TRACK0;
                }
                self.intrq = true;
            }
            Pending::Read | Pending::Write => {
                // Enter the data phase: keep BUSY and raise DRQ for the host.
                self.status = ST_MOTOR_ON | ST_BUSY | ST_DRQ;
                self.drq = true;
            }
            Pending::RecordNotFound => {
                self.status = ST_MOTOR_ON | ST_RECORD_NOT_FOUND;
                self.intrq = true;
                self.pending = Pending::TypeI;
            }
            Pending::WriteProtect => {
                self.status = ST_MOTOR_ON | ST_WRITE_PROTECT;
                self.intrq = true;
                self.pending = Pending::TypeI;
            }
        }
    }
}

/// CRC-CCITT (0x1021, init 0xFFFF) over the ID field as the WD177x computes it:
/// the three `A1` sync bytes, the `FE` ID address mark, then track, side,
/// sector and size code. Not load-bearing for a clean image, but it makes Read
/// Address return a byte-correct ID.
fn id_field_crc(track: u8, side: u8, sector: u8, size_code: u8) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for byte in [0xA1, 0xA1, 0xA1, 0xFE, track, side, sector, size_code] {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: u8 = 0;
    const TRACK: u8 = 1;
    const SECTOR: u8 = 2;
    const DATA: u8 = 3;

    /// Run the controller until the active command settles (BUSY clears) or a
    /// safety cap is hit, returning the status. Does not drain data phases.
    fn settle(fdc: &mut Wd1770) -> u8 {
        for _ in 0..COMMAND_CYCLES + 4 {
            fdc.tick();
        }
        fdc.read(STATUS)
    }

    fn test_disk() -> Disk {
        // 40 tracks, single sided, 9 sectors/track, 512 bytes — a CP/M-ish 180K.
        let tracks = 40;
        let spt = 9;
        let size = 512;
        let mut data = vec![0u8; tracks * spt * size];
        // Stamp each sector's first 3 bytes with (track, sector, marker) so a
        // read can be checked against its address.
        for t in 0..tracks {
            for s in 0..spt {
                let off = (t * spt + s) * size;
                data[off] = t as u8;
                data[off + 1] = (s + 1) as u8;
                data[off + 2] = 0xAA;
            }
        }
        Disk::new(data, tracks, 1, spt, size)
    }

    #[test]
    fn restore_lands_on_track_zero() {
        let mut fdc = Wd1770::new();
        fdc.write(TRACK, 20);
        fdc.write(STATUS, 0x00); // Restore
        let st = settle(&mut fdc);
        assert_eq!(st & ST_BUSY, 0, "command should have settled");
        assert_eq!(st & ST_TRACK0, ST_TRACK0, "head at track 0");
        assert_eq!(fdc.read(TRACK), 0);
        assert_eq!(fdc.head_track(), 0);
    }

    #[test]
    fn seek_moves_to_data_register_track() {
        let mut fdc = Wd1770::new();
        fdc.write(DATA, 17);
        fdc.write(STATUS, 0x10); // Seek
        let st = settle(&mut fdc);
        assert_eq!(st & ST_BUSY, 0);
        assert_eq!(st & ST_TRACK0, 0, "not at track 0");
        assert_eq!(fdc.read(TRACK), 17);
        assert_eq!(fdc.head_track(), 17);
    }

    #[test]
    fn step_in_out_updates_track_with_u_bit() {
        let mut fdc = Wd1770::new();
        // Step in twice with update.
        fdc.write(STATUS, 0x50); // step-in, u
        settle(&mut fdc);
        fdc.write(STATUS, 0x50);
        settle(&mut fdc);
        assert_eq!(fdc.head_track(), 2);
        assert_eq!(fdc.read(TRACK), 2);
        // Step out without update: head moves, register frozen.
        fdc.write(STATUS, 0x60); // step-out, no u
        settle(&mut fdc);
        assert_eq!(fdc.head_track(), 1);
        assert_eq!(fdc.read(TRACK), 2, "track register frozen without u bit");
    }

    #[test]
    fn read_sector_streams_bytes() {
        let mut fdc = Wd1770::new();
        fdc.insert_disk(0, test_disk());
        fdc.set_drive(0);
        // Seek to track 5.
        fdc.write(DATA, 5);
        fdc.write(STATUS, 0x10);
        settle(&mut fdc);
        // Read sector 3.
        fdc.write(SECTOR, 3);
        fdc.write(STATUS, 0x80); // read sector
        // Settle into the data phase.
        for _ in 0..COMMAND_CYCLES + 2 {
            fdc.tick();
        }
        let st = fdc.read(STATUS);
        assert_eq!(st & (ST_BUSY | ST_DRQ), ST_BUSY | ST_DRQ, "data phase");
        assert!(fdc.drq());
        // First three bytes encode (track, sector, marker).
        assert_eq!(fdc.read(DATA), 5);
        assert_eq!(fdc.read(DATA), 3);
        assert_eq!(fdc.read(DATA), 0xAA);
        // Drain the rest.
        for _ in 0..(512 - 3) {
            fdc.read(DATA);
        }
        assert!(!fdc.drq(), "DRQ drops when the sector is exhausted");
        // Check INTRQ before reading status — a status read clears it.
        assert!(fdc.intrq(), "INTRQ on completion");
        assert_eq!(fdc.read(STATUS) & ST_BUSY, 0, "command settled");
        assert!(!fdc.intrq(), "status read clears INTRQ");
    }

    #[test]
    fn read_sector_no_disk_is_record_not_found() {
        let mut fdc = Wd1770::new();
        fdc.write(SECTOR, 1);
        fdc.write(STATUS, 0x80);
        let st = settle(&mut fdc);
        assert_eq!(st & ST_RECORD_NOT_FOUND, ST_RECORD_NOT_FOUND);
        assert_eq!(st & ST_BUSY, 0);
    }

    #[test]
    fn read_missing_sector_is_record_not_found() {
        let mut fdc = Wd1770::new();
        fdc.insert_disk(0, test_disk());
        fdc.set_drive(0);
        fdc.write(SECTOR, 99); // beyond 9 sectors/track
        fdc.write(STATUS, 0x80);
        let st = settle(&mut fdc);
        assert_eq!(st & ST_RECORD_NOT_FOUND, ST_RECORD_NOT_FOUND);
    }

    #[test]
    fn write_sector_round_trips_through_image() {
        let mut fdc = Wd1770::new();
        fdc.insert_disk(0, test_disk());
        fdc.set_drive(0);
        fdc.write(DATA, 2);
        fdc.write(STATUS, 0x10); // seek track 2
        settle(&mut fdc);
        fdc.write(SECTOR, 4);
        fdc.write(STATUS, 0xA0); // write sector
        for _ in 0..COMMAND_CYCLES + 2 {
            fdc.tick();
        }
        assert!(fdc.drq(), "write data phase requests bytes");
        for i in 0..512u32 {
            fdc.write(DATA, (i & 0xFF) as u8);
        }
        assert!(!fdc.drq());
        assert_eq!(fdc.read(STATUS) & ST_BUSY, 0);
        assert!(fdc.disk(0).unwrap().is_dirty());

        // Read it back.
        fdc.write(SECTOR, 4);
        fdc.write(STATUS, 0x80);
        for _ in 0..COMMAND_CYCLES + 2 {
            fdc.tick();
        }
        for i in 0..512u32 {
            assert_eq!(fdc.read(DATA), (i & 0xFF) as u8, "byte {i}");
        }
    }

    #[test]
    fn write_protected_disk_reports_protect() {
        let mut fdc = Wd1770::new();
        fdc.insert_disk(0, test_disk().write_protected(true));
        fdc.set_drive(0);
        fdc.write(SECTOR, 1);
        fdc.write(STATUS, 0xA0); // write sector
        let st = settle(&mut fdc);
        assert_eq!(st & ST_WRITE_PROTECT, ST_WRITE_PROTECT);
        assert!(!fdc.disk(0).unwrap().is_dirty());
    }

    #[test]
    fn read_address_returns_id_field() {
        let mut fdc = Wd1770::new();
        fdc.insert_disk(0, test_disk());
        fdc.set_drive(0);
        fdc.write(DATA, 7);
        fdc.write(STATUS, 0x10); // seek track 7
        settle(&mut fdc);
        fdc.write(STATUS, 0xC0); // read address
        for _ in 0..COMMAND_CYCLES + 2 {
            fdc.tick();
        }
        assert!(fdc.drq());
        assert_eq!(fdc.read(DATA), 7, "ID track");
        assert_eq!(fdc.read(DATA), 0, "ID side");
        assert_eq!(fdc.read(DATA), 1, "ID first sector");
        assert_eq!(fdc.read(DATA), sector_size_code(512), "size code");
        let crc_hi = fdc.read(DATA);
        let crc_lo = fdc.read(DATA);
        let expected = id_field_crc(7, 0, 1, sector_size_code(512));
        assert_eq!(u16::from(crc_hi) << 8 | u16::from(crc_lo), expected);
        // Read Address loads the track into the sector register.
        assert_eq!(fdc.read(SECTOR), 7);
    }

    #[test]
    fn force_interrupt_aborts_and_clears_busy() {
        let mut fdc = Wd1770::new();
        fdc.insert_disk(0, test_disk());
        fdc.set_drive(0);
        fdc.write(SECTOR, 1);
        fdc.write(STATUS, 0x80); // start a read
        fdc.tick(); // partway
        fdc.write(STATUS, 0xD0); // force interrupt
        let st = fdc.read(STATUS);
        assert_eq!(st & ST_BUSY, 0, "BUSY cleared immediately");
        assert!(!fdc.drq());
    }

    #[test]
    fn multi_sector_read_advances() {
        let mut fdc = Wd1770::new();
        fdc.insert_disk(0, test_disk());
        fdc.set_drive(0);
        fdc.write(DATA, 1);
        fdc.write(STATUS, 0x10);
        settle(&mut fdc);
        fdc.write(SECTOR, 1);
        fdc.write(STATUS, 0x90); // read multiple, from sector 1
        for _ in 0..COMMAND_CYCLES + 2 {
            fdc.tick();
        }
        // Sector 1: marker bytes then drain.
        assert_eq!(fdc.read(DATA), 1, "track 1");
        assert_eq!(fdc.read(DATA), 1, "sector 1");
        for _ in 0..(512 - 2) {
            fdc.read(DATA);
        }
        // The controller should chain into sector 2 — let it settle the gap.
        for _ in 0..COMMAND_CYCLES + 2 {
            fdc.tick();
        }
        assert!(fdc.drq(), "DRQ raised for the next sector");
        assert_eq!(fdc.read(DATA), 1, "still track 1");
        assert_eq!(fdc.read(DATA), 2, "now sector 2");
    }

    #[test]
    fn idle_status_pulses_index() {
        let mut fdc = Wd1770::new();
        // Settle a restore so we are idle with motor on.
        fdc.write(STATUS, 0x00);
        settle(&mut fdc);
        let mut saw_set = false;
        let mut saw_clear = false;
        for _ in 0..INDEX_PERIOD * 2 {
            fdc.tick();
            if fdc.peek_status() & ST_INDEX != 0 {
                saw_set = true;
            } else {
                saw_clear = true;
            }
        }
        assert!(saw_set && saw_clear, "index bit toggles when idle");
    }
}
