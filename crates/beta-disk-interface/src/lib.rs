/// Beta 128 disk interface — the standard floppy disk system for
/// Pentagon, Scorpion, and other Eastern European Spectrum clones.
///
/// Provides:
/// 1. Magic ROM paging: M1 fetch in $3D00-$3DFF pages in TR-DOS ROM.
///    M1 fetch in RAM ($4000+) pages it back out.
/// 2. WD1793 floppy disk controller with TRD image support.
/// 3. System register at port $FF (drive select, side, etc.).
///
/// Integrates with each host machine through the `Peripheral` trait
/// from `common-sinclair-zx-spectrum::peripheral`. Phase 0.7 moved
/// `claims_port`, `read`, `write`, and `on_m1` out of inherent
/// methods and onto the trait, so Pentagon and Scorpion (and any
/// future host that wires in a Beta disk) drive the interface
/// uniformly.
use common_sinclair_zx_spectrum::peripheral::Peripheral;

/// TRD disk geometry.
const SECTORS_PER_TRACK: usize = 16;
const SECTOR_SIZE: usize = 256;
const SIDES: usize = 2;
const TRACKS_PER_SIDE: usize = 80;
const TRACK_SIZE: usize = SECTORS_PER_TRACK * SECTOR_SIZE; // 4096
#[cfg(test)]
const DISK_SIZE: usize = TRACKS_PER_SIDE * SIDES * TRACK_SIZE; // 655,360

/// WD1793 command types.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
enum CmdType {
    None,
    Restore,
    Seek,
    Step,
    ReadSector,
    WriteSector,
    ReadAddress,
    ForceInterrupt,
}

/// WD1793 status bits.
const ST_BUSY: u8 = 0x01;
const ST_DRQ: u8 = 0x02;
const ST_RECORD_NOT_FOUND: u8 = 0x10; // Same bit, different meaning for type II/III
const ST_WRITE_PROTECT: u8 = 0x40;
const ST_NOT_READY: u8 = 0x80;
const ST_TRACK_0: u8 = 0x04; // Type I only

/// Beta disk interface state.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct BetaDisk {
    /// Is the TR-DOS ROM currently paged in at $0000-$3FFF?
    pub trdos_paged: bool,

    // WD1793 registers
    status: u8,
    track: u8,
    sector: u8,
    data: u8,
    command: CmdType,

    // System register (port $FF)
    system: u8,

    /// Current side (0 or 1), from system register.
    side: u8,
    /// Current drive (0-3), from system register.
    drive: u8,

    // Sector read/write buffer
    sector_buf: Vec<u8>,
    buf_pos: usize,

    /// INTRQ flag (directly readable via system register bit 7).
    intrq: bool,
    /// DRQ flag.
    drq: bool,

    /// Disk images: up to 4 drives, each optionally loaded.
    /// TRD format: raw sector data, 655,360 bytes.
    disks: [Option<Vec<u8>>; 4],
}

impl BetaDisk {
    pub fn new() -> Self {
        Self {
            trdos_paged: false,
            status: 0,
            track: 0,
            sector: 1,
            data: 0,
            command: CmdType::None,
            system: 0,
            side: 0,
            drive: 0,
            sector_buf: vec![0u8; SECTOR_SIZE],
            buf_pos: 0,
            intrq: false,
            drq: false,
            disks: [None, None, None, None],
        }
    }

    /// Load a TRD disk image into a drive (0-3).
    pub fn insert_disk(&mut self, drive: usize, data: Vec<u8>) {
        if drive < 4 {
            self.disks[drive] = Some(data);
        }
    }

    /// Remove a disk from a drive.
    pub fn eject_disk(&mut self, drive: usize) {
        if drive < 4 {
            self.disks[drive] = None;
        }
    }

    /// Is a disk inserted in the current drive?
    fn disk_ready(&self) -> bool {
        self.disks[self.drive as usize].is_some()
    }

    /// Read a sector from the current disk.
    fn read_sector_data(&self) -> Option<Vec<u8>> {
        let disk = self.disks[self.drive as usize].as_ref()?;
        let offset = self.sector_offset()?;
        if offset + SECTOR_SIZE <= disk.len() {
            Some(disk[offset..offset + SECTOR_SIZE].to_vec())
        } else {
            None
        }
    }

    /// Compute the byte offset for the current track/side/sector.
    fn sector_offset(&self) -> Option<usize> {
        let track = self.track as usize;
        let sector = (self.sector as usize).wrapping_sub(1); // Sectors are 1-based in TR-DOS
        let side = self.side as usize;

        if track >= TRACKS_PER_SIDE || sector >= SECTORS_PER_TRACK || side >= SIDES {
            return None;
        }

        // TRD layout: track 0 side 0, track 0 side 1, track 1 side 0, ...
        let linear_track = track * SIDES + side;
        Some(linear_track * TRACK_SIZE + sector * SECTOR_SIZE)
    }

    /// Read from a Beta disk I/O port.
    fn read_port(&mut self, port: u16) -> u8 {
        match port & 0xFF {
            0x1F => {
                // Status register — reading clears INTRQ
                self.intrq = false;
                let mut s = self.status;
                if !self.disk_ready() {
                    s |= ST_NOT_READY;
                }
                s
            }
            0x3F => self.track,
            0x5F => self.sector,
            0x7F => {
                // Data register — during sector read, return next byte
                if self.command == CmdType::ReadSector && self.drq {
                    let byte = if self.buf_pos < self.sector_buf.len() {
                        self.sector_buf[self.buf_pos]
                    } else {
                        0
                    };
                    self.buf_pos += 1;
                    if self.buf_pos >= SECTOR_SIZE {
                        // Sector complete
                        self.drq = false;
                        self.status &= !ST_BUSY;
                        self.status &= !ST_DRQ;
                        self.intrq = true;
                        self.command = CmdType::None;
                    }
                    byte
                } else {
                    self.data
                }
            }
            0xFF => {
                // System register: bit 7 = INTRQ, bit 6 = DRQ
                let mut val = 0x3F; // bits 0-5 = active drives etc.
                if self.intrq {
                    val |= 0x80;
                }
                if self.drq {
                    val |= 0x40;
                }
                val
            }
            _ => 0xFF,
        }
    }

    /// Write to a Beta disk I/O port.
    fn write_port(&mut self, port: u16, val: u8) {
        match port & 0xFF {
            0x1F => self.execute_command(val),
            0x3F => self.track = val,
            0x5F => self.sector = val,
            0x7F => self.data = val,
            0xFF => {
                self.system = val;
                self.drive = val & 0x03;
                self.side = if val & 0x10 != 0 { 1 } else { 0 };
            }
            _ => {}
        }
    }

    /// Execute a WD1793 command.
    fn execute_command(&mut self, cmd: u8) {
        self.intrq = false;

        match cmd & 0xF0 {
            // Type I: Restore
            0x00 => {
                self.command = CmdType::Restore;
                self.track = 0;
                self.status = if self.disk_ready() {
                    ST_TRACK_0
                } else {
                    ST_NOT_READY
                };
                self.intrq = true;
            }
            // Type I: Seek
            0x10 => {
                self.command = CmdType::Seek;
                self.track = self.data;
                self.status = if self.track == 0 { ST_TRACK_0 } else { 0 };
                if !self.disk_ready() {
                    self.status |= ST_NOT_READY;
                }
                self.intrq = true;
            }
            // Type I: Step
            0x20 | 0x30 => {
                self.command = CmdType::Step;
                self.status = if self.track == 0 { ST_TRACK_0 } else { 0 };
                self.intrq = true;
            }
            // Type I: Step-In
            0x40 | 0x50 => {
                self.command = CmdType::Step;
                if self.track < 79 {
                    self.track += 1;
                }
                self.status = 0;
                self.intrq = true;
            }
            // Type I: Step-Out
            0x60 | 0x70 => {
                self.command = CmdType::Step;
                if self.track > 0 {
                    self.track -= 1;
                }
                self.status = if self.track == 0 { ST_TRACK_0 } else { 0 };
                self.intrq = true;
            }
            // Type II: Read Sector
            0x80 | 0x90 => {
                self.command = CmdType::ReadSector;
                if let Some(data) = self.read_sector_data() {
                    self.sector_buf[..SECTOR_SIZE].copy_from_slice(&data);
                    self.buf_pos = 0;
                    self.drq = true;
                    self.status = ST_BUSY | ST_DRQ;
                } else {
                    self.status = ST_RECORD_NOT_FOUND;
                    self.intrq = true;
                    self.command = CmdType::None;
                }
            }
            // Type II: Write Sector (not implemented — TRD is read-only for now)
            0xA0 | 0xB0 => {
                self.command = CmdType::WriteSector;
                self.status = ST_WRITE_PROTECT;
                self.intrq = true;
                self.command = CmdType::None;
            }
            // Type III: Read Address
            0xC0 => {
                self.command = CmdType::ReadAddress;
                self.status = 0;
                self.data = self.track;
                self.intrq = true;
                self.command = CmdType::None;
            }
            // Type III: Read Track / Write Track (not implemented)
            0xE0 | 0xF0 => {
                self.status = 0;
                self.intrq = true;
            }
            // Type IV: Force Interrupt
            0xD0 => {
                self.command = CmdType::ForceInterrupt;
                self.status &= !ST_BUSY;
                self.drq = false;
                self.command = CmdType::None;
                if cmd & 0x0F != 0 {
                    self.intrq = true;
                }
            }
            _ => {}
        }
    }
}

impl Default for BetaDisk {
    fn default() -> Self {
        Self::new()
    }
}

impl Peripheral for BetaDisk {
    /// Claims I/O ports `$1F`, `$3F`, `$5F`, `$7F`, and `$FF` — but
    /// only while TR-DOS ROM is paged in. When paged out, the Beta
    /// disk is electrically detached from the bus and these ports
    /// revert to whatever else the machine decodes for them
    /// (typically the Kempston joystick at `$1F` or the floating bus).
    fn claims_port(&self, port: u16) -> bool {
        if !self.trdos_paged {
            return false;
        }
        matches!(port & 0xFF, 0x1F | 0x3F | 0x5F | 0x7F | 0xFF)
    }

    fn read(&mut self, port: u16) -> u8 {
        self.read_port(port)
    }

    fn write(&mut self, port: u16, val: u8) {
        self.write_port(port, val)
    }

    /// Magic TR-DOS paging on M1 fetch.
    ///
    /// M1 fetch in `$3D00-$3DFF` while TR-DOS is paged out transitions
    /// to paged-in. M1 fetch at `$4000` or above while paged in
    /// transitions back to paged out. The `$3Dxx` trigger is always
    /// inside ROM space (all Spectrum variants map ROM to the first
    /// 16 KB), so no external "am I in ROM?" context is needed — the
    /// trigger address implies it.
    fn on_m1(&mut self, addr: u16) {
        if !self.trdos_paged {
            if (0x3D00..=0x3DFF).contains(&addr) {
                self.trdos_paged = true;
            }
        } else if addr >= 0x4000 {
            self.trdos_paged = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_paging() {
        let mut beta = BetaDisk::new();
        assert!(!beta.trdos_paged);

        beta.on_m1(0x3D00);
        assert!(beta.trdos_paged);

        beta.on_m1(0x0000);
        assert!(beta.trdos_paged); // Stays paged in ROM space

        beta.on_m1(0x4000);
        assert!(!beta.trdos_paged);
    }

    #[test]
    fn ports_only_active_when_paged() {
        let mut beta = BetaDisk::new();
        assert!(!beta.claims_port(0x1F));

        beta.trdos_paged = true;
        assert!(beta.claims_port(0x1F));
        assert!(beta.claims_port(0xFF));
        assert!(!beta.claims_port(0xFE));
    }

    #[test]
    fn restore_command() {
        let mut beta = BetaDisk::new();
        beta.track = 10;
        beta.execute_command(0x00); // Restore
        assert_eq!(beta.track, 0);
        assert!(beta.intrq);
    }

    #[test]
    fn seek_command() {
        let mut beta = BetaDisk::new();
        beta.data = 20;
        beta.execute_command(0x10); // Seek
        assert_eq!(beta.track, 20);
        assert!(beta.intrq);
    }

    #[test]
    fn read_sector() {
        let mut beta = BetaDisk::new();
        // Create a minimal TRD image
        let mut disk = vec![0u8; DISK_SIZE];
        // Put some data in track 0, side 0, sector 1 (offset 0)
        disk[0] = 0xAB;
        disk[255] = 0xCD;
        beta.insert_disk(0, disk);

        beta.track = 0;
        beta.sector = 1;
        beta.side = 0;
        beta.drive = 0;
        beta.execute_command(0x80); // Read Sector

        assert!(beta.drq);
        assert_eq!(beta.status & ST_BUSY, ST_BUSY);

        // Read first byte
        let b = beta.read(0x7F);
        assert_eq!(b, 0xAB);

        // Read remaining bytes
        for _ in 1..255 {
            beta.read(0x7F);
        }
        let last = beta.read(0x7F);
        assert_eq!(last, 0xCD);

        // Sector complete
        assert!(!beta.drq);
        assert!(beta.intrq);
    }
}
