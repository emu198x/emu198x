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
    /// Sector ID (R) as it appears in the address mark — this is what
    /// the FDC matches against the sector ID in the read command, and
    /// it is *not* always equal to the physical position on the track.
    pub id: u8,
    pub data: Vec<u8>,
}

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

    /// Is this FDC electrically wired to the host's I/O bus?
    /// True on +3, false on +2A / +2B — both share the SpectrumPlus
    /// struct but only the +3 has an actual drive connector.
    pub enabled: bool,

    /// Disk images (up to 4 drives).
    #[serde(skip)]
    disks: [Option<DiskImage>; 4],
}

// Main status register bits
const MSR_CB: u8 = 0x10; // Controller busy
const MSR_EXM: u8 = 0x20; // Execution mode
const MSR_DIO: u8 = 0x40; // Data direction (1 = FDC → CPU)
const MSR_RQM: u8 = 0x80; // Request for master (ready for data)

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
            enabled: false,
            disks: [None, None, None, None],
        }
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
    pub fn read_status(&self) -> u8 {
        self.main_status
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
            Command::ReadData => {
                let drive = (self.cmd_buf[1] & 0x03) as usize;
                let head = (self.cmd_buf[1] >> 2) & 0x01;
                let track = self.cmd_buf[2];
                let sector = self.cmd_buf[4]; // R (sector ID)
                let n = self.cmd_buf[5]; // N (sector size: 0=128, 1=256, 2=512)
                let sector_size = 128usize << (n as usize);

                self.head = head;
                self.sector = sector;

                if let Some(data) = self.read_sector(drive, track, head, sector, sector_size) {
                    self.exec_buf = data;
                    self.exec_pos = 0;
                    self.phase = Phase::Execution;
                    self.main_status = MSR_RQM | MSR_EXM | MSR_DIO | MSR_CB;
                    self.st0 = (head << 2) | (drive as u8);
                    self.st1 = 0;
                    self.st2 = 0;
                } else {
                    // Sector not found
                    self.st0 = 0x40 | (head << 2) | (drive as u8); // Abnormal termination
                    self.st1 = 0x04; // No data
                    self.st2 = 0;
                    self.setup_result_read(track, head, sector, n);
                }
            }
            Command::Recalibrate => {
                let drive = (self.cmd_buf[1] & 0x03) as usize;
                self.track[drive] = 0;
                // ST0 IC bits: 00 = Normal Termination (real drive
                // present, seek to track 0 succeeded). 11 + EC bit
                // (0xD8 | drive) = Abnormal due to Drive Not Ready —
                // what the real µPD765A returns when a recalibrate is
                // issued against a drive whose TRACK 0 signal never
                // asserts (no drive connected). The +3 BIOS probes
                // drives by recalibrating each in turn; falsely
                // returning Normal Termination for non-existent drives
                // makes it think every drive is real.
                let st0 = if self.disks[drive].is_some() {
                    0x20 | (drive as u8) // Seek End | drive
                } else {
                    0xD0 | (drive as u8) // Abnormal | Not Ready | EC | drive
                };
                self.st0 = st0;
                self.seek_pending[drive] = Some(st0);
                self.interrupt = true;
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
                let drive = (self.cmd_buf[1] & 0x03) as usize;
                let new_track = self.cmd_buf[2];
                self.track[drive] = new_track;
                let st0 = 0x20 | (drive as u8); // Seek End | drive
                self.st0 = st0;
                self.seek_pending[drive] = Some(st0);
                self.interrupt = true;
                self.phase = Phase::Idle;
                self.main_status = MSR_RQM;
            }
            Command::SenseDriveStatus => {
                let drive = (self.cmd_buf[1] & 0x03) as usize;
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
                let drive = (self.cmd_buf[1] & 0x03) as usize;
                let head = (self.cmd_buf[1] >> 2) & 0x01;
                self.st0 = (head << 2) | (drive as u8);
                self.st1 = 0;
                self.st2 = 0;
                // Return current position
                self.result_buf.clear();
                self.result_buf.push(self.st0);
                self.result_buf.push(self.st1);
                self.result_buf.push(self.st2);
                self.result_buf.push(self.track[drive]);
                self.result_buf.push(head);
                self.result_buf.push(1); // Sector 1
                self.result_buf.push(2); // N=2 (512 bytes)
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

    /// Read a sector from a disk image by (track, head, sector ID).
    ///
    /// Returns the sector data truncated/padded to `sector_size` bytes,
    /// matching the N parameter from the read command.
    fn read_sector(
        &self,
        drive: usize,
        track: u8,
        head: u8,
        sector: u8,
        sector_size: usize,
    ) -> Option<Vec<u8>> {
        let disk = self.disks[drive].as_ref()?;
        let sec = disk.sector(track, head, sector)?;
        let mut out = Vec::with_capacity(sector_size);
        let take = sec.data.len().min(sector_size);
        out.extend_from_slice(&sec.data[..take]);
        out.resize(sector_size, sec.data.last().copied().unwrap_or(0));
        Some(out)
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
    fn recalibrate() {
        let mut fdc = Upd765a::new();
        fdc.track[0] = 10;

        fdc.write_data(0x07); // Recalibrate
        fdc.write_data(0x00); // Drive 0

        assert_eq!(fdc.track[0], 0);
        assert!(fdc.interrupt);
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
                id: 1,
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
                    id: 0xC2,
                    data: vec![0xAA; 512],
                },
                DiskSector {
                    id: 0xC1,
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
}
