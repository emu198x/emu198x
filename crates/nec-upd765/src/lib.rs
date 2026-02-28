//! NEC uPD765 floppy disk controller.
//!
//! Standalone IC emulation with no dependencies, following the project's
//! chip-level library pattern (like `mos-via-6522` and `mos-sid-6581`).
//!
//! The uPD765 is used in the ZX Spectrum +3, Amstrad CPC, and IBM PC.
//! This implementation covers the command set needed for +3DOS.
//!
//! # Register interface
//!
//! Two externally visible registers:
//! - **Main Status Register (MSR)** — read-only, port $2FFD on the +3
//! - **Data Register** — read/write, port $3FFD on the +3
//!
//! # State machine
//!
//! Idle → Command (CPU writes parameter bytes) → Execution → Result
//! (CPU reads status bytes) → Idle.

#![allow(clippy::cast_possible_truncation)]

pub mod commands;
pub mod dsk;

pub use dsk::DskImage;

/// FDC state machine phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdcPhase {
    /// Waiting for a command byte.
    Idle,
    /// Receiving command parameter bytes.
    Command,
    /// Executing (data transfer in progress).
    Execution,
    /// CPU reads result bytes.
    Result,
}

/// NEC uPD765 floppy disk controller.
pub struct Upd765 {
    phase: FdcPhase,
    /// Command bytes received so far.
    command_buf: Vec<u8>,
    /// Expected total command length (set after first byte).
    command_len: usize,
    /// Result bytes to return during result phase.
    result_buf: Vec<u8>,
    /// Current read position in result_buf (including any data transfer bytes).
    result_index: usize,
    /// Number of bytes in result_buf that are sector data (before the 7 status bytes).
    data_len: usize,
    /// Status registers.
    st0: u8,
    st1: u8,
    st2: u8,
    /// Present cylinder number per drive (up to 4 drives).
    pcn: [u8; 4],
    /// Interrupt pending flag.
    interrupt_pending: bool,
    /// Inserted disk images (drives 0 and 1).
    disk: [Option<DskImage>; 2],
}

impl Upd765 {
    /// Create a new FDC with no disks inserted.
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: FdcPhase::Idle,
            command_buf: Vec::with_capacity(9),
            command_len: 0,
            result_buf: Vec::new(),
            result_index: 0,
            data_len: 0,
            st0: 0,
            st1: 0,
            st2: 0,
            pcn: [0; 4],
            interrupt_pending: false,
            disk: [None, None],
        }
    }

    /// Read the Main Status Register.
    ///
    /// Bit 7 (RQM): 1 = data register ready for transfer.
    /// Bit 6 (DIO): 0 = CPU→FDC (write), 1 = FDC→CPU (read).
    /// Bit 5 (EXM): 1 = in execution phase.
    /// Bits 0-3: drive busy flags.
    #[must_use]
    pub fn read_msr(&self) -> u8 {
        match self.phase {
            FdcPhase::Idle => 0x80,     // RQM=1, DIO=0 (ready to accept command)
            FdcPhase::Command => 0x90,  // RQM=1, DIO=0, busy
            FdcPhase::Execution => {
                // During data transfer: RQM=1, DIO depends on read/write, EXM=1
                0xF0 // RQM=1, DIO=1 (FDC→CPU for read), EXM=1
            }
            FdcPhase::Result => 0xD0,   // RQM=1, DIO=1 (FDC→CPU), busy
        }
    }

    /// Read from the data register.
    ///
    /// During the result phase, returns successive result bytes.
    /// When all result bytes are read, returns to idle.
    #[must_use]
    pub fn read_data(&mut self) -> u8 {
        match self.phase {
            FdcPhase::Result | FdcPhase::Execution => {
                if self.result_index < self.result_buf.len() {
                    let byte = self.result_buf[self.result_index];
                    self.result_index += 1;

                    // Transition from execution to result when data transfer is done
                    if self.phase == FdcPhase::Execution
                        && self.result_index >= self.data_len
                    {
                        self.phase = FdcPhase::Result;
                    }

                    // Return to idle when all result bytes (including status) are read
                    if self.result_index >= self.result_buf.len() {
                        self.phase = FdcPhase::Idle;
                        self.result_buf.clear();
                        self.result_index = 0;
                        self.data_len = 0;
                    }
                    byte
                } else {
                    self.phase = FdcPhase::Idle;
                    0xFF
                }
            }
            _ => 0xFF,
        }
    }

    /// Write to the data register.
    ///
    /// Accepts command bytes during command phase. When all expected bytes
    /// are received, executes the command.
    pub fn write_data(&mut self, value: u8) {
        match self.phase {
            FdcPhase::Idle => {
                // First byte of a new command
                self.command_buf.clear();
                self.command_buf.push(value);
                self.command_len = commands::command_length(value);

                if self.command_len <= 1 {
                    self.execute_command();
                } else {
                    self.phase = FdcPhase::Command;
                }
            }
            FdcPhase::Command => {
                self.command_buf.push(value);
                if self.command_buf.len() >= self.command_len {
                    self.execute_command();
                }
            }
            _ => {
                // Writes during execution/result are ignored
            }
        }
    }

    /// Insert a disk image into the specified drive (0 or 1).
    pub fn insert_disk(&mut self, drive: usize, image: DskImage) {
        if drive < 2 {
            self.disk[drive] = Some(image);
        }
    }

    /// Eject the disk from the specified drive.
    pub fn eject_disk(&mut self, drive: usize) -> Option<DskImage> {
        if drive < 2 {
            self.disk[drive].take()
        } else {
            None
        }
    }

    /// Whether an interrupt is pending (after seek/recalibrate).
    #[must_use]
    pub fn interrupt_pending(&self) -> bool {
        self.interrupt_pending
    }

    /// Take (clear) the pending interrupt, returning whether one was set.
    pub fn take_interrupt(&mut self) -> bool {
        let was = self.interrupt_pending;
        self.interrupt_pending = false;
        was
    }

    /// Current FDC phase (for testing/debugging).
    #[must_use]
    pub fn phase(&self) -> FdcPhase {
        self.phase
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn execute_command(&mut self) {
        let (result, interrupt) = commands::execute(
            &self.command_buf,
            &mut self.st0,
            &mut self.st1,
            &mut self.st2,
            &mut self.pcn,
            &mut self.disk,
            &mut self.phase,
        );

        if interrupt {
            self.interrupt_pending = true;
        }

        if !result.is_empty() {
            // For READ DATA, the result contains sector data + 7 status bytes.
            // We need to serve the sector data during execution phase, then
            // the 7 status bytes during result phase.
            let cmd_id = self.command_buf[0] & 0x1F;
            if cmd_id == 0x06 && result.len() > 7 {
                // READ DATA: data transfer then result
                self.data_len = result.len() - 7;
                self.phase = FdcPhase::Execution;
            } else {
                self.data_len = 0;
            }

            self.result_buf = result;
            self.result_index = 0;
        }
    }
}

impl Default for Upd765 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsk::parse_dsk;

    fn make_fdc_with_disk() -> Upd765 {
        let mut fdc = Upd765::new();

        // Build a minimal DSK: 1 track, 1 side, 1 sector (512 bytes)
        let mut raw = vec![0u8; 0x100];
        raw[..b"MV - CPCEMU Disk-File\r\nDisk-Info\r\n".len()]
            .copy_from_slice(b"MV - CPCEMU Disk-File\r\nDisk-Info\r\n");
        raw[0x30] = 1; // tracks
        raw[0x31] = 1; // sides
        raw[0x32] = 0x00;
        raw[0x33] = 0x03; // track size = 768

        let mut track = vec![0u8; 256];
        track[..12].copy_from_slice(b"Track-Info\r\n");
        track[0x10] = 0;
        track[0x11] = 0;
        track[0x14] = 2;
        track[0x15] = 1;
        track[0x18] = 0;    // C
        track[0x19] = 0;    // H
        track[0x1A] = 0x01; // R
        track[0x1B] = 2;    // N
        raw.extend_from_slice(&track);

        let mut sector = vec![0xE5u8; 512];
        sector[0] = 0xAA;
        sector[511] = 0xBB;
        raw.extend_from_slice(&sector);

        let img = parse_dsk(&raw).expect("test DSK");
        fdc.insert_disk(0, img);
        fdc
    }

    #[test]
    fn msr_idle_state() {
        let fdc = Upd765::new();
        let msr = fdc.read_msr();
        assert_eq!(msr & 0x80, 0x80, "RQM should be set");
        assert_eq!(msr & 0x40, 0x00, "DIO should be 0 (CPU→FDC)");
    }

    #[test]
    fn specify_command() {
        let mut fdc = Upd765::new();
        // SPECIFY: 3 bytes
        fdc.write_data(0x03); // Command
        assert_eq!(fdc.phase(), FdcPhase::Command);
        fdc.write_data(0xDF); // SRT=D, HUT=F
        fdc.write_data(0x02); // HLT=01, NDMA=0
        assert_eq!(fdc.phase(), FdcPhase::Idle);
    }

    #[test]
    fn recalibrate_and_sense_interrupt() {
        let mut fdc = make_fdc_with_disk();

        // Seek to track 5 first
        fdc.write_data(0x0F); // SEEK
        fdc.write_data(0x00); // Drive 0
        fdc.write_data(0x05); // Track 5
        assert!(fdc.interrupt_pending());
        fdc.take_interrupt();

        // RECALIBRATE
        fdc.write_data(0x07);
        fdc.write_data(0x00); // Drive 0
        assert!(fdc.take_interrupt());

        // SENSE INTERRUPT STATUS
        fdc.write_data(0x08);
        let st0 = fdc.read_data();
        let pcn = fdc.read_data();
        assert_eq!(st0 & 0x20, 0x20, "Seek end bit");
        assert_eq!(pcn, 0, "Should be at track 0");
    }

    #[test]
    fn seek_updates_pcn() {
        let mut fdc = Upd765::new();
        fdc.write_data(0x0F); // SEEK
        fdc.write_data(0x00); // Drive 0
        fdc.write_data(0x0A); // Track 10
        assert!(fdc.take_interrupt());

        // SENSE INTERRUPT to confirm
        fdc.write_data(0x08);
        let _st0 = fdc.read_data();
        let pcn = fdc.read_data();
        assert_eq!(pcn, 10);
    }

    #[test]
    fn read_data_from_disk() {
        let mut fdc = make_fdc_with_disk();

        // READ DATA: 9 bytes
        fdc.write_data(0x46); // READ DATA (MFM mode, bit 6 set)
        fdc.write_data(0x00); // Drive 0, head 0
        fdc.write_data(0x00); // C=0
        fdc.write_data(0x00); // H=0
        fdc.write_data(0x01); // R=1
        fdc.write_data(0x02); // N=2 (512 bytes)
        fdc.write_data(0x01); // EOT=1
        fdc.write_data(0x1B); // GPL
        fdc.write_data(0xFF); // DTL

        // Should be in execution phase with data to read
        assert_eq!(fdc.read_msr() & 0x20, 0x20, "EXM should be set");

        // Read 512 bytes of sector data
        let first = fdc.read_data();
        assert_eq!(first, 0xAA, "First data byte");

        // Skip to last data byte
        for _ in 1..511 {
            let _ = fdc.read_data();
        }
        let last = fdc.read_data();
        assert_eq!(last, 0xBB, "Last data byte");

        // Now read 7 result bytes
        let st0 = fdc.read_data();
        assert_eq!(st0 & 0x40, 0x00, "No error");
        let _st1 = fdc.read_data();
        let _st2 = fdc.read_data();
        let _c = fdc.read_data();
        let _h = fdc.read_data();
        let _r = fdc.read_data();
        let _n = fdc.read_data();

        assert_eq!(fdc.phase(), FdcPhase::Idle);
    }

    #[test]
    fn read_data_no_disk_errors() {
        let mut fdc = Upd765::new(); // No disk

        fdc.write_data(0x46);
        fdc.write_data(0x00);
        fdc.write_data(0x00);
        fdc.write_data(0x00);
        fdc.write_data(0x01);
        fdc.write_data(0x02);
        fdc.write_data(0x01);
        fdc.write_data(0x1B);
        fdc.write_data(0xFF);

        // Result phase — ST0 should indicate error
        let st0 = fdc.read_data();
        assert_eq!(st0 & 0x40, 0x40, "Abnormal termination");
    }

    #[test]
    fn read_id_returns_sector_header() {
        let mut fdc = make_fdc_with_disk();

        fdc.write_data(0x4A); // READ ID (MFM)
        fdc.write_data(0x00); // Drive 0, head 0

        let st0 = fdc.read_data();
        let _st1 = fdc.read_data();
        let _st2 = fdc.read_data();
        let c = fdc.read_data();
        let h = fdc.read_data();
        let r = fdc.read_data();
        let n = fdc.read_data();

        assert_eq!(st0 & 0x40, 0x00, "No error");
        assert_eq!(c, 0);
        assert_eq!(h, 0);
        assert_eq!(r, 1);
        assert_eq!(n, 2);
    }

    #[test]
    fn sense_drive_status() {
        let mut fdc = make_fdc_with_disk();

        fdc.write_data(0x04); // SENSE DRIVE STATUS
        fdc.write_data(0x00); // Drive 0

        let st3 = fdc.read_data();
        assert_eq!(st3 & 0x20, 0x20, "Ready");
        assert_eq!(st3 & 0x10, 0x10, "Track 0");
        assert_eq!(st3 & 0x08, 0x08, "Two-side");
    }

    #[test]
    fn invalid_command_returns_error() {
        let mut fdc = Upd765::new();
        fdc.write_data(0x1F); // Invalid
        let st0 = fdc.read_data();
        assert_eq!(st0, 0x80, "Invalid command flag");
    }

    #[test]
    fn insert_and_eject_disk() {
        let mut fdc = make_fdc_with_disk();

        // Verify disk is present
        fdc.write_data(0x04);
        fdc.write_data(0x00);
        let st3 = fdc.read_data();
        assert_eq!(st3 & 0x20, 0x20, "Ready with disk");

        // Eject
        let disk = fdc.eject_disk(0);
        assert!(disk.is_some());

        // Verify disk is gone
        fdc.write_data(0x04);
        fdc.write_data(0x00);
        let st3 = fdc.read_data();
        assert_eq!(st3 & 0x20, 0x00, "Not ready without disk");
    }

    #[test]
    fn write_data_command() {
        let mut fdc = make_fdc_with_disk();

        // WRITE DATA: 9 bytes
        fdc.write_data(0x45); // WRITE DATA (MFM)
        fdc.write_data(0x00); // Drive 0, head 0
        fdc.write_data(0x00); // C=0
        fdc.write_data(0x00); // H=0
        fdc.write_data(0x01); // R=1
        fdc.write_data(0x02); // N=2
        fdc.write_data(0x01); // EOT
        fdc.write_data(0x1B); // GPL
        fdc.write_data(0xFF); // DTL

        // Read 7 result bytes
        let st0 = fdc.read_data();
        assert_eq!(st0 & 0x40, 0x00, "No error on write");
    }
}
