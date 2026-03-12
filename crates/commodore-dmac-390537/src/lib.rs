//! Commodore 390537 SDMAC — SCSI DMA controller for the Amiga 3000.
//!
//! The SDMAC sits at `$DD0000–$DDFFFF` and provides a WD33C93(A) SCSI
//! bus interface controller plus DMA transfer logic. Fat Gary generates
//! the chip select.
//!
//! Supports zero or more SCSI disk targets (IDs 0–6). Selection of an
//! absent target produces an immediate timeout; selection of a present
//! target executes the SCSI command and fills the internal DMA buffer.
//! The machine-level bus wrapper transfers data between the DMAC buffer
//! and system memory using the ACR/WTC registers.

// ---------------------------------------------------------------------------
// WD33C93 registers (indirect access via SASR/SCMD)
// ---------------------------------------------------------------------------

/// WD33C93 register addresses (selected by writing to SASR).
#[allow(dead_code)]
mod wd_reg {
    pub const OWN_ID: u8 = 0x00;
    pub const CONTROL: u8 = 0x01;
    pub const TIMEOUT_PERIOD: u8 = 0x02;
    pub const TOTAL_SECTORS: u8 = 0x03;
    pub const TOTAL_HEADS: u8 = 0x04;
    pub const TOTAL_CYL_HI: u8 = 0x05;
    pub const TOTAL_CYL_LO: u8 = 0x06;
    pub const LOG_ADDR_HI: u8 = 0x07;
    pub const LOG_ADDR_2: u8 = 0x08;
    pub const LOG_ADDR_3: u8 = 0x09;
    pub const LOG_ADDR_LO: u8 = 0x0A;
    pub const SECTOR_NUMBER: u8 = 0x0B;
    pub const HEAD_NUMBER: u8 = 0x0C;
    pub const CYL_HI: u8 = 0x0D;
    pub const CYL_LO: u8 = 0x0E;
    pub const TARGET_LUN: u8 = 0x0F;
    pub const COMMAND_PHASE: u8 = 0x10;
    pub const SYNC_TRANSFER: u8 = 0x11;
    pub const TRANSFER_COUNT_HI: u8 = 0x12;
    pub const TRANSFER_COUNT_MID: u8 = 0x13;
    pub const TRANSFER_COUNT_LO: u8 = 0x14;
    pub const DESTINATION_ID: u8 = 0x15;
    pub const SOURCE_ID: u8 = 0x16;
    pub const SCSI_STATUS: u8 = 0x17;
    pub const COMMAND: u8 = 0x18;
    pub const DATA: u8 = 0x19;
    pub const AUXILIARY_STATUS: u8 = 0x1F;
}

/// WD33C93 command codes (written to the COMMAND register).
mod wd_cmd {
    pub const RESET: u8 = 0x00;
    pub const ABORT: u8 = 0x01;
    pub const SEL_ATN: u8 = 0x06;
    pub const SEL: u8 = 0x07;
    pub const SEL_ATN_XFER: u8 = 0x08;
    pub const SEL_XFER: u8 = 0x09;
}

/// WD33C93 Command Status Register (CSR) values.
mod wd_csr {
    /// Reset completed (no advanced features).
    pub const RESET: u8 = 0x00;
    /// Reset completed (advanced features enabled).
    pub const RESET_AF: u8 = 0x01;
    /// Selection timed out — no target responded.
    pub const TIMEOUT: u8 = 0x42;
    /// Transfer completed successfully (Select-and-Transfer).
    pub const XFER_DONE: u8 = 0x16;
    /// Selection completed, command phase (Select without Transfer).
    pub const SEL_COMPLETE: u8 = 0x11;
    /// Abort completed.
    #[allow(dead_code)]
    pub const SEL_ABORT: u8 = 0x22;
}

/// WD33C93 Auxiliary Status Register (ASR) bits.
#[allow(dead_code)]
mod wd_asr {
    /// Interrupt pending.
    pub const INT: u8 = 0x80;
    /// Busy (Level II command executing).
    pub const BSY: u8 = 0x20;
    /// Command in progress.
    pub const CIP: u8 = 0x10;
}

// ---------------------------------------------------------------------------
// SCSI command opcodes
// ---------------------------------------------------------------------------

mod scsi_cmd {
    pub const TEST_UNIT_READY: u8 = 0x00;
    pub const REQUEST_SENSE: u8 = 0x03;
    pub const READ_6: u8 = 0x08;
    pub const WRITE_6: u8 = 0x0A;
    pub const INQUIRY: u8 = 0x12;
    pub const MODE_SENSE_6: u8 = 0x1A;
    pub const READ_CAPACITY_10: u8 = 0x25;
    pub const READ_10: u8 = 0x28;
    pub const WRITE_10: u8 = 0x2A;
}

// ---------------------------------------------------------------------------
// SDMAC (Super DMAC) registers
// ---------------------------------------------------------------------------

/// SDMAC CNTR (control register) bits.
mod cntr_bits {
    /// Peripheral reset — drives WD33C93 /IOW and /IOR low.
    pub const PREST: u8 = 0x10;
    /// Interrupt enable.
    pub const INTEN: u8 = 0x04;
}

/// SDMAC ISTR (interrupt status register) bits.
mod istr_bits {
    /// Any interrupt source active (follow bit).
    pub const INT_F: u8 = 0x80;
    /// SCSI peripheral interrupt (from WD33C93 INT pin).
    pub const INTS: u8 = 0x40;
    /// Interrupt pending (only set when CNTR.INTEN = 1).
    pub const INT_P: u8 = 0x10;
    /// FIFO empty flag.
    pub const FE_FLG: u8 = 0x01;
}

// ---------------------------------------------------------------------------
// SDMAC register byte offsets within the $DD0000–$DD00FF block.
//
// The 68030 accesses these as word or longword cycles. The bus wrapper
// presents individual byte addresses, so we match on `addr & 0xFF`.
// ---------------------------------------------------------------------------

/// Byte offset for DAWR (DACK width, write-only).
const REG_DAWR: u8 = 0x02;
/// Byte offset for WTC high word.
const REG_WTC_HI: u8 = 0x04;
/// Byte offset for WTC low word.
const REG_WTC_LO: u8 = 0x06;
/// Byte offset for CNTR (control, read/write).
const REG_CNTR: u8 = 0x0A;
/// Byte offset for ACR high word.
const REG_ACR_HI: u8 = 0x0C;
/// Byte offset for ACR low word.
const REG_ACR_LO: u8 = 0x0E;
/// Byte offset for ST_DMA (start DMA, write strobe).
const REG_ST_DMA: u8 = 0x12;
/// Byte offset for FLUSH (flush FIFO, write strobe).
const REG_FLUSH: u8 = 0x16;
/// Byte offset for CINT (clear interrupts, write strobe).
const REG_CINT: u8 = 0x1A;
/// Byte offset for ISTR (interrupt status, read-only).
const REG_ISTR: u8 = 0x1E;
/// Byte offset for SP_DMA (stop DMA, write strobe).
const REG_SP_DMA: u8 = 0x3E;
/// Byte offset for SASR (WD register select write / ASR read).
const REG_SASR: u8 = 0x40;
/// Byte offset for SCMD (WD register data read/write).
const REG_SCMD: u8 = 0x42;
/// Byte offset for SASR alternate port.
const REG_SASR_ALT: u8 = 0x48;
/// Byte offset for SCMD alternate port.
const REG_SCMD_ALT: u8 = 0x4A;

// ---------------------------------------------------------------------------
// SCSI target
// ---------------------------------------------------------------------------

/// A SCSI hard disk target attached to the WD33C93 bus.
#[derive(Debug, Clone)]
struct ScsiTarget {
    /// Raw disk image (LBA-ordered, 512 bytes per sector).
    disk_image: Vec<u8>,
    /// Total number of 512-byte sectors.
    total_sectors: u32,
    /// Sense key from the last error (0 = no error).
    sense_key: u8,
    /// Additional sense code.
    sense_asc: u8,
    /// Additional sense code qualifier.
    sense_ascq: u8,
}

impl ScsiTarget {
    fn new(disk_image: Vec<u8>) -> Self {
        let total_sectors = (disk_image.len() / 512) as u32;
        Self {
            disk_image,
            total_sectors,
            sense_key: 0,
            sense_asc: 0,
            sense_ascq: 0,
        }
    }

    /// Clear any pending sense data (no error).
    fn clear_sense(&mut self) {
        self.sense_key = 0;
        self.sense_asc = 0;
        self.sense_ascq = 0;
    }

    /// Set an ILLEGAL REQUEST sense for invalid commands.
    fn set_illegal_request(&mut self) {
        self.sense_key = 0x05; // ILLEGAL REQUEST
        self.sense_asc = 0x20; // Invalid command operation code
        self.sense_ascq = 0x00;
    }

    /// Set a MEDIUM ERROR for out-of-range LBA.
    fn set_lba_out_of_range(&mut self) {
        self.sense_key = 0x05; // ILLEGAL REQUEST
        self.sense_asc = 0x21; // LBA out of range
        self.sense_ascq = 0x00;
    }
}

// ---------------------------------------------------------------------------
// WD33C93 state
// ---------------------------------------------------------------------------

/// WD33C93 SCSI controller state with optional target support.
///
/// Tracks the indirect register file, auxiliary status, and up to 7
/// SCSI targets (IDs 0–6; ID 7 is reserved for the initiator).
#[derive(Debug, Clone)]
struct Wd33c93 {
    /// Currently selected indirect register address.
    selected_reg: u8,
    /// Register file ($00–$1F). Only a handful are functionally
    /// significant; the rest are pure storage.
    regs: [u8; 32],
    /// Auxiliary Status Register (directly readable, not in the
    /// indirect register file proper).
    asr: u8,
    /// SCSI targets (IDs 0–6). ID 7 is the initiator.
    targets: [Option<ScsiTarget>; 7],
    /// Data buffer for DMA transfers (filled by SCSI commands).
    dma_buffer: Vec<u8>,
    /// Current read position in the DMA buffer.
    dma_read_pos: usize,
    /// Current write position in the DMA buffer.
    dma_write_pos: usize,
    /// True when a DMA transfer is pending (data in buffer).
    dma_pending: bool,
    /// Direction of the pending DMA: true = target→initiator (read).
    dma_direction_read: bool,
    /// SCSI CDB buffer for SEL_ATN_XFER / SEL_XFER commands.
    cdb: [u8; 12],
    /// Debug trace flag.
    trace: bool,
}

impl Wd33c93 {
    fn new() -> Self {
        // Power-on state: the WD33C93 generates a reset interrupt after
        // hardware reset completes. SCSI_STATUS = RESET ($00) and
        // ASR.INT is set so the host can detect the chip is ready.
        let mut regs = [0u8; 32];
        regs[wd_reg::SCSI_STATUS as usize] = wd_csr::RESET;
        Self {
            selected_reg: 0,
            regs,
            asr: wd_asr::INT,
            targets: [const { None }; 7],
            dma_buffer: Vec::new(),
            dma_read_pos: 0,
            dma_write_pos: 0,
            dma_pending: false,
            dma_direction_read: true,
            cdb: [0; 12],
            trace: false,
        }
    }

    /// Hardware reset (SDMAC PREST asserted).
    fn hardware_reset(&mut self) {
        self.regs = [0; 32];
        self.asr = 0;
        self.selected_reg = 0;
        self.dma_buffer.clear();
        self.dma_read_pos = 0;
        self.dma_write_pos = 0;
        self.dma_pending = false;
    }

    /// Read the Auxiliary Status Register (ASR). Does not clear INT.
    fn read_asr(&self) -> u8 {
        self.asr
    }

    /// Read a register through the indirect SCMD port.
    fn read_data(&mut self) -> u8 {
        let reg = self.selected_reg & 0x1F;
        let val = match reg {
            wd_reg::AUXILIARY_STATUS => self.asr,
            wd_reg::SCSI_STATUS => {
                let status = self.regs[wd_reg::SCSI_STATUS as usize];
                // Reading SCSI_STATUS clears ASR.INT.
                self.asr &= !wd_asr::INT;
                status
            }
            _ => self.regs[reg as usize],
        };
        // Auto-increment after read, except for ASR, DATA, COMMAND,
        // and SCSI_STATUS. WD33C93 skips SCSI_STATUS on read (but
        // not on write) — matches WinUAE behaviour.
        if reg != wd_reg::AUXILIARY_STATUS
            && reg != wd_reg::DATA
            && reg != wd_reg::COMMAND
            && reg != wd_reg::SCSI_STATUS
        {
            self.selected_reg = self.selected_reg.wrapping_add(1) & 0x1F;
        }
        val
    }

    /// Write a register through the indirect SCMD port.
    fn write_data(&mut self, val: u8) {
        let reg = self.selected_reg & 0x1F;
        match reg {
            wd_reg::COMMAND => self.execute_command(val),
            _ => self.regs[reg as usize] = val,
        }
        // Auto-increment after write, except for ASR, DATA, and
        // COMMAND. SCSI_STATUS *does* auto-increment on write
        // (only reads skip it).
        if reg != wd_reg::AUXILIARY_STATUS && reg != wd_reg::DATA && reg != wd_reg::COMMAND {
            self.selected_reg = self.selected_reg.wrapping_add(1) & 0x1F;
        }
    }

    /// Execute a WD33C93 command.
    fn execute_command(&mut self, cmd: u8) {
        if self.trace {
            let cmd_name = match cmd {
                wd_cmd::RESET => "RESET",
                wd_cmd::ABORT => "ABORT",
                wd_cmd::SEL_ATN => "SEL_ATN",
                wd_cmd::SEL => "SEL",
                wd_cmd::SEL_ATN_XFER => "SEL_ATN_XFER",
                wd_cmd::SEL_XFER => "SEL_XFER",
                _ => "UNKNOWN",
            };
            eprintln!(
                "[WD33C93] cmd=${:02X}({}) dest_id={} asr=${:02X}",
                cmd,
                cmd_name,
                self.regs[wd_reg::DESTINATION_ID as usize] & 0x07,
                self.asr
            );
        }
        match cmd {
            wd_cmd::RESET => {
                // Software reset. Check EAF (OWN_ID bit 3) to decide
                // the post-reset status code.
                let eaf = self.regs[wd_reg::OWN_ID as usize] & 0x08 != 0;
                self.regs = [0; 32];
                self.regs[wd_reg::SCSI_STATUS as usize] =
                    if eaf { wd_csr::RESET_AF } else { wd_csr::RESET };
                self.asr = wd_asr::INT;
                self.dma_buffer.clear();
                self.dma_pending = false;
            }
            wd_cmd::ABORT => {
                self.regs[wd_reg::SCSI_STATUS as usize] = 0x22; // CSR_SEL_ABORT
                self.asr = wd_asr::INT;
                self.dma_pending = false;
            }
            wd_cmd::SEL_ATN | wd_cmd::SEL => {
                let target_id = self.regs[wd_reg::DESTINATION_ID as usize] & 0x07;
                if target_id < 7 && self.targets[target_id as usize].is_some() {
                    // Target present — selection succeeds.
                    self.regs[wd_reg::SCSI_STATUS as usize] = wd_csr::SEL_COMPLETE;
                } else {
                    // No target — immediate timeout.
                    self.regs[wd_reg::SCSI_STATUS as usize] = wd_csr::TIMEOUT;
                }
                self.asr = wd_asr::INT;
            }
            wd_cmd::SEL_ATN_XFER | wd_cmd::SEL_XFER => {
                let target_id = self.regs[wd_reg::DESTINATION_ID as usize] & 0x07;
                if target_id < 7 && self.targets[target_id as usize].is_some() {
                    // Build CDB from the WD register file.
                    self.build_cdb();
                    let (status, direction_read) =
                        self.execute_scsi_command(target_id as usize);
                    self.dma_direction_read = direction_read;
                    self.dma_read_pos = 0;
                    self.dma_write_pos = 0;
                    self.dma_pending = !self.dma_buffer.is_empty();
                    self.regs[wd_reg::SCSI_STATUS as usize] = status;
                } else {
                    self.regs[wd_reg::SCSI_STATUS as usize] = wd_csr::TIMEOUT;
                }
                self.asr = wd_asr::INT;
            }
            _ => {
                // Unknown or unimplemented command — set LCI (Last
                // Command Ignored) in ASR. KS handles this gracefully.
                if self.trace {
                    eprintln!("[WD33C93] unknown cmd=${:02X} → LCI", cmd);
                }
                self.asr |= 0x40; // LCI bit
            }
        }
        if self.trace {
            eprintln!(
                "[WD33C93]   → status=${:02X} asr=${:02X}",
                self.regs[wd_reg::SCSI_STATUS as usize],
                self.asr
            );
        }
    }

    /// Build a SCSI CDB from the WD33C93 register file.
    ///
    /// The CDB length comes from the command group (high 3 bits of
    /// the opcode in the COMMAND_PHASE or TARGET_LUN register area).
    /// For Select-and-Transfer, the WD33C93 takes CDB bytes from
    /// registers $0F–$1B (TARGET_LUN through TRANSFER_COUNT).
    fn build_cdb(&mut self) {
        // CDB bytes are stored starting at TARGET_LUN ($0F).
        // Byte 0 = TARGET_LUN, Byte 1 = CDB opcode, etc.
        // Actually for WD33C93 SEL_ATN_XFER, the CDB comes from
        // the CDB register area starting at $03 (TOTAL_SECTORS).
        // The opcode is in the COMMAND_PHASE byte ($10).
        //
        // In practice, KS writes the CDB starting at register $03.
        // CDB[0] = reg[$03], CDB[1] = reg[$04], etc.
        for i in 0..12 {
            let reg_idx = (wd_reg::TOTAL_SECTORS as usize) + i;
            if reg_idx < 32 {
                self.cdb[i] = self.regs[reg_idx];
            }
        }
    }

    /// Execute a SCSI command for the given target. Returns the
    /// WD33C93 status code and whether the DMA direction is read
    /// (target→initiator).
    fn execute_scsi_command(&mut self, target_id: usize) -> (u8, bool) {
        let opcode = self.cdb[0];
        match opcode {
            scsi_cmd::TEST_UNIT_READY => {
                if let Some(target) = &mut self.targets[target_id] {
                    target.clear_sense();
                }
                self.dma_buffer.clear();
                (wd_csr::XFER_DONE, true)
            }
            scsi_cmd::REQUEST_SENSE => {
                self.dma_buffer.clear();
                let alloc_len = self.cdb[4] as usize;
                let len = alloc_len.min(18);
                self.dma_buffer.resize(len, 0);
                if let Some(target) = &self.targets[target_id] {
                    if len > 0 {
                        self.dma_buffer[0] = 0x70; // Current errors
                    }
                    if len > 2 {
                        self.dma_buffer[2] = target.sense_key;
                    }
                    if len > 7 {
                        self.dma_buffer[7] = 10; // Additional sense length
                    }
                    if len > 12 {
                        self.dma_buffer[12] = target.sense_asc;
                    }
                    if len > 13 {
                        self.dma_buffer[13] = target.sense_ascq;
                    }
                }
                if let Some(target) = &mut self.targets[target_id] {
                    target.clear_sense();
                }
                (wd_csr::XFER_DONE, true)
            }
            scsi_cmd::INQUIRY => {
                let alloc_len = self.cdb[4] as usize;
                let len = alloc_len.min(96);
                self.dma_buffer.clear();
                self.dma_buffer.resize(len, 0);
                if len > 0 {
                    self.dma_buffer[0] = 0x00; // Direct-access device (disk)
                }
                if len > 1 {
                    self.dma_buffer[1] = 0x00; // Not removable
                }
                if len > 2 {
                    self.dma_buffer[2] = 0x02; // SCSI-2
                }
                if len > 3 {
                    self.dma_buffer[3] = 0x02; // Response data format
                }
                if len > 4 {
                    self.dma_buffer[4] = 91; // Additional length
                }
                // Vendor (bytes 8-15): "EMU198X "
                let vendor = b"EMU198X ";
                for (i, &b) in vendor.iter().enumerate() {
                    if 8 + i < len {
                        self.dma_buffer[8 + i] = b;
                    }
                }
                // Product (bytes 16-31): "SCSI DISK       "
                let product = b"SCSI DISK       ";
                for (i, &b) in product.iter().enumerate() {
                    if 16 + i < len {
                        self.dma_buffer[16 + i] = b;
                    }
                }
                // Revision (bytes 32-35): "1.0 "
                let revision = b"1.0 ";
                for (i, &b) in revision.iter().enumerate() {
                    if 32 + i < len {
                        self.dma_buffer[32 + i] = b;
                    }
                }
                (wd_csr::XFER_DONE, true)
            }
            scsi_cmd::MODE_SENSE_6 => {
                let alloc_len = self.cdb[4] as usize;
                let len = alloc_len.min(12);
                self.dma_buffer.clear();
                self.dma_buffer.resize(len, 0);
                // Minimal mode parameter header (4 bytes).
                if len > 0 {
                    self.dma_buffer[0] = (len.saturating_sub(1)) as u8; // Mode data length
                }
                if len > 1 {
                    self.dma_buffer[1] = 0x00; // Medium type
                }
                if len > 2 {
                    self.dma_buffer[2] = 0x00; // Device-specific parameter
                }
                if len > 3 {
                    self.dma_buffer[3] = 0x00; // Block descriptor length
                }
                (wd_csr::XFER_DONE, true)
            }
            scsi_cmd::READ_CAPACITY_10 => {
                self.dma_buffer.clear();
                self.dma_buffer.resize(8, 0);
                if let Some(target) = &self.targets[target_id] {
                    // Last LBA (total_sectors - 1).
                    let last_lba = target.total_sectors.saturating_sub(1);
                    self.dma_buffer[0] = (last_lba >> 24) as u8;
                    self.dma_buffer[1] = (last_lba >> 16) as u8;
                    self.dma_buffer[2] = (last_lba >> 8) as u8;
                    self.dma_buffer[3] = last_lba as u8;
                    // Block size = 512.
                    self.dma_buffer[4] = 0x00;
                    self.dma_buffer[5] = 0x00;
                    self.dma_buffer[6] = 0x02;
                    self.dma_buffer[7] = 0x00;
                }
                (wd_csr::XFER_DONE, true)
            }
            scsi_cmd::READ_6 => {
                let lba = (u32::from(self.cdb[1] & 0x1F) << 16)
                    | (u32::from(self.cdb[2]) << 8)
                    | u32::from(self.cdb[3]);
                let count = if self.cdb[4] == 0 { 256u32 } else { u32::from(self.cdb[4]) };
                self.do_scsi_read(target_id, lba, count)
            }
            scsi_cmd::READ_10 => {
                let lba = (u32::from(self.cdb[2]) << 24)
                    | (u32::from(self.cdb[3]) << 16)
                    | (u32::from(self.cdb[4]) << 8)
                    | u32::from(self.cdb[5]);
                let count = (u32::from(self.cdb[7]) << 8) | u32::from(self.cdb[8]);
                self.do_scsi_read(target_id, lba, count)
            }
            scsi_cmd::WRITE_6 => {
                let lba = (u32::from(self.cdb[1] & 0x1F) << 16)
                    | (u32::from(self.cdb[2]) << 8)
                    | u32::from(self.cdb[3]);
                let count = if self.cdb[4] == 0 { 256u32 } else { u32::from(self.cdb[4]) };
                self.do_scsi_write_prepare(target_id, lba, count)
            }
            scsi_cmd::WRITE_10 => {
                let lba = (u32::from(self.cdb[2]) << 24)
                    | (u32::from(self.cdb[3]) << 16)
                    | (u32::from(self.cdb[4]) << 8)
                    | u32::from(self.cdb[5]);
                let count = (u32::from(self.cdb[7]) << 8) | u32::from(self.cdb[8]);
                self.do_scsi_write_prepare(target_id, lba, count)
            }
            _ => {
                // Unknown SCSI command — set CHECK CONDITION.
                if let Some(target) = &mut self.targets[target_id] {
                    target.set_illegal_request();
                }
                self.dma_buffer.clear();
                (wd_csr::XFER_DONE, true)
            }
        }
    }

    /// Execute a SCSI READ command: copy sectors from disk to DMA buffer.
    fn do_scsi_read(&mut self, target_id: usize, lba: u32, count: u32) -> (u8, bool) {
        self.dma_buffer.clear();
        let target = match &mut self.targets[target_id] {
            Some(t) => t,
            None => return (wd_csr::TIMEOUT, true),
        };
        if lba + count > target.total_sectors {
            target.set_lba_out_of_range();
            return (wd_csr::XFER_DONE, true);
        }
        let byte_offset = lba as usize * 512;
        let byte_len = count as usize * 512;
        self.dma_buffer
            .extend_from_slice(&target.disk_image[byte_offset..byte_offset + byte_len]);
        target.clear_sense();
        (wd_csr::XFER_DONE, true)
    }

    /// Prepare for a SCSI WRITE command: allocate the DMA buffer for
    /// incoming data. The actual write happens when `commit_write` is
    /// called after the DMA transfer completes.
    fn do_scsi_write_prepare(
        &mut self,
        target_id: usize,
        lba: u32,
        count: u32,
    ) -> (u8, bool) {
        let target = match &self.targets[target_id] {
            Some(t) => t,
            None => return (wd_csr::TIMEOUT, false),
        };
        if lba + count > target.total_sectors {
            if let Some(t) = &mut self.targets[target_id] {
                t.set_lba_out_of_range();
            }
            return (wd_csr::XFER_DONE, false);
        }
        let byte_len = count as usize * 512;
        self.dma_buffer.clear();
        self.dma_buffer.resize(byte_len, 0);
        // Store LBA in the register file for commit_write to use.
        self.regs[wd_reg::LOG_ADDR_HI as usize] = (lba >> 24) as u8;
        self.regs[wd_reg::LOG_ADDR_2 as usize] = (lba >> 16) as u8;
        self.regs[wd_reg::LOG_ADDR_3 as usize] = (lba >> 8) as u8;
        self.regs[wd_reg::LOG_ADDR_LO as usize] = lba as u8;
        (wd_csr::XFER_DONE, false) // false = initiator→target (write)
    }

    /// Commit buffered write data to the target's disk image.
    fn commit_write(&mut self, target_id: usize) {
        let lba = (u32::from(self.regs[wd_reg::LOG_ADDR_HI as usize]) << 24)
            | (u32::from(self.regs[wd_reg::LOG_ADDR_2 as usize]) << 16)
            | (u32::from(self.regs[wd_reg::LOG_ADDR_3 as usize]) << 8)
            | u32::from(self.regs[wd_reg::LOG_ADDR_LO as usize]);
        let byte_offset = lba as usize * 512;
        let byte_len = self.dma_buffer.len();
        if let Some(target) = &mut self.targets[target_id] {
            if byte_offset + byte_len <= target.disk_image.len() {
                target.disk_image[byte_offset..byte_offset + byte_len]
                    .copy_from_slice(&self.dma_buffer);
                target.clear_sense();
            }
        }
    }

    /// True when the WD33C93 INT pin is asserted (ASR bit 7).
    fn int_active(&self) -> bool {
        self.asr & wd_asr::INT != 0
    }
}

// ---------------------------------------------------------------------------
// SDMAC 390537
// ---------------------------------------------------------------------------

/// Commodore 390537 SDMAC state.
///
/// Provides the WD33C93 SCSI interface and DMA registers at
/// `$DD0000–$DDFFFF`. Supports disk targets for SCSI boot.
#[derive(Debug, Clone)]
pub struct Dmac390537 {
    wd: Wd33c93,
    /// CNTR — control register.
    cntr: u8,
    /// DAWR — DACK width register (write-only, 2 bits).
    dawr: u8,
    /// WTC — word transfer count (24-bit, stored as u32).
    wtc: u32,
    /// ACR — address counter register (32-bit).
    acr: u32,
    /// Latched interrupt flags (cleared by CINT strobe).
    istr_latched: u8,
    /// DMA active flag (set by ST_DMA strobe).
    dma_active: bool,
}

impl Dmac390537 {
    /// Create a new SDMAC in power-on state (no SCSI targets).
    #[must_use]
    pub fn new() -> Self {
        Self {
            wd: Wd33c93::new(),
            cntr: 0,
            dawr: 0,
            wtc: 0,
            acr: 0,
            istr_latched: 0,
            dma_active: false,
        }
    }

    /// Create a new SDMAC with a disk image attached at the given SCSI ID.
    #[must_use]
    pub fn with_disk(target_id: u8, image: Vec<u8>) -> Self {
        let mut dmac = Self::new();
        dmac.attach_disk(target_id, image);
        dmac
    }

    /// Attach a disk image to the given SCSI target ID (0–6).
    pub fn attach_disk(&mut self, target_id: u8, image: Vec<u8>) {
        if target_id < 7 {
            self.wd.targets[target_id as usize] = Some(ScsiTarget::new(image));
        }
    }

    /// True when a SCSI target is present at the given ID.
    #[must_use]
    pub fn target_present(&self, target_id: u8) -> bool {
        target_id < 7
            && self.wd.targets[target_id as usize].is_some()
    }

    /// Enable or disable WD33C93 command-level debug tracing.
    pub fn set_trace(&mut self, enabled: bool) {
        self.wd.trace = enabled;
    }

    /// Reset all state to power-on defaults.
    pub fn reset(&mut self) {
        self.wd.hardware_reset();
        self.cntr = 0;
        self.dawr = 0;
        self.wtc = 0;
        self.acr = 0;
        self.istr_latched = 0;
        self.dma_active = false;
    }

    /// Current CNTR register value.
    #[must_use]
    pub const fn cntr(&self) -> u8 {
        self.cntr
    }

    /// Current DAWR register value.
    #[must_use]
    pub const fn dawr(&self) -> u8 {
        self.dawr
    }

    /// Current WTC register value.
    #[must_use]
    pub const fn wtc(&self) -> u32 {
        self.wtc
    }

    /// Current ACR register value.
    #[must_use]
    pub const fn acr(&self) -> u32 {
        self.acr
    }

    /// Currently selected WD33C93 register.
    #[must_use]
    pub const fn wd_selected_reg(&self) -> u8 {
        self.wd.selected_reg
    }

    /// Current WD33C93 auxiliary-status register.
    #[must_use]
    pub fn wd_asr(&self) -> u8 {
        self.wd.read_asr()
    }

    /// Current WD33C93 SCSI-status register.
    #[must_use]
    pub fn wd_scsi_status(&self) -> u8 {
        self.wd.regs[wd_reg::SCSI_STATUS as usize]
    }

    /// True when DMA is active (ST_DMA strobe received, SP_DMA not yet).
    #[must_use]
    pub const fn dma_active(&self) -> bool {
        self.dma_active
    }

    /// True when a DMA transfer has pending data.
    #[must_use]
    pub fn dma_pending(&self) -> bool {
        self.wd.dma_pending
    }

    /// True when the pending DMA direction is read (target→initiator).
    #[must_use]
    pub fn dma_direction_read(&self) -> bool {
        self.wd.dma_direction_read
    }

    /// Bytes remaining in the DMA buffer.
    #[must_use]
    pub fn dma_bytes_remaining(&self) -> usize {
        if self.wd.dma_direction_read {
            self.wd.dma_buffer.len().saturating_sub(self.wd.dma_read_pos)
        } else {
            self.wd.dma_buffer.len().saturating_sub(self.wd.dma_write_pos)
        }
    }

    /// Read a byte from the DMA buffer (for target→initiator transfers).
    /// Returns 0 if the buffer is exhausted.
    #[must_use]
    pub fn dma_read_byte(&mut self) -> u8 {
        if self.wd.dma_read_pos < self.wd.dma_buffer.len() {
            let b = self.wd.dma_buffer[self.wd.dma_read_pos];
            self.wd.dma_read_pos += 1;
            if self.wd.dma_read_pos >= self.wd.dma_buffer.len() {
                self.wd.dma_pending = false;
            }
            b
        } else {
            0
        }
    }

    /// Write a byte to the DMA buffer (for initiator→target transfers).
    pub fn dma_write_byte(&mut self, val: u8) {
        if self.wd.dma_write_pos < self.wd.dma_buffer.len() {
            self.wd.dma_buffer[self.wd.dma_write_pos] = val;
            self.wd.dma_write_pos += 1;
            if self.wd.dma_write_pos >= self.wd.dma_buffer.len() {
                // Buffer full — commit the write to disk.
                let target_id =
                    (self.wd.regs[wd_reg::DESTINATION_ID as usize] & 0x07) as usize;
                if target_id < 7 {
                    self.wd.commit_write(target_id);
                }
                self.wd.dma_pending = false;
            }
        }
    }

    /// Compute the current ISTR value.
    ///
    /// ISTR is read-only and reflects live state plus latched flags.
    fn istr(&self) -> u8 {
        let mut val = self.istr_latched;

        // FE_FLG: FIFO empty when no DMA data pending.
        if !self.wd.dma_pending || self.wd.dma_read_pos >= self.wd.dma_buffer.len() {
            val |= istr_bits::FE_FLG;
        }

        // INTS follows the WD33C93 interrupt pin.
        if self.wd.int_active() {
            val |= istr_bits::INTS;
        }

        // INT_F: any interrupt source active.
        if val & (istr_bits::INTS | 0x20 | 0x08 | 0x04) != 0 {
            val |= istr_bits::INT_F;
        }

        // INT_P: only set when CNTR.INTEN is enabled.
        if val & istr_bits::INT_F != 0 && self.cntr & cntr_bits::INTEN != 0 {
            val |= istr_bits::INT_P;
        }

        val
    }

    /// Current ISTR value.
    #[must_use]
    pub fn current_istr(&self) -> u8 {
        self.istr()
    }

    /// True when the SDMAC interrupt output is pending and enabled.
    #[must_use]
    pub fn irq_pending(&self) -> bool {
        self.istr() & istr_bits::INT_P != 0
    }

    /// Read a word from an SDMAC register.
    ///
    /// `addr` is the full byte address in `$DD0000–$DDFFFF`. The bus
    /// wrapper calls this for word reads; byte reads extract the
    /// relevant byte from the returned word.
    #[must_use]
    pub fn read_word(&mut self, addr: u32) -> u16 {
        let offset = (addr & 0x00FF) as u8;
        match offset {
            REG_CNTR => u16::from(self.cntr),
            REG_ISTR => u16::from(self.istr()),
            REG_WTC_HI => (self.wtc >> 16) as u16,
            REG_WTC_LO => self.wtc as u16,
            REG_ACR_HI => (self.acr >> 16) as u16,
            REG_ACR_LO => self.acr as u16,
            REG_SASR | REG_SASR_ALT => {
                // Word read: high byte is 0, low byte is ASR.
                u16::from(self.wd.read_asr())
            }
            REG_SCMD | REG_SCMD_ALT => {
                // Word read: high byte is 0, low byte is register data.
                u16::from(self.wd.read_data())
            }
            _ => 0,
        }
    }

    /// Write a word to an SDMAC register.
    ///
    /// `addr` is the full byte address. `val` is the 16-bit data word.
    pub fn write_word(&mut self, addr: u32, val: u16) {
        let offset = (addr & 0x00FF) as u8;
        match offset {
            REG_DAWR => self.dawr = val as u8 & 0x03,
            REG_CNTR => {
                self.cntr = val as u8;
                // PREST: assert peripheral reset while set.
                if self.cntr & cntr_bits::PREST != 0 {
                    self.wd.hardware_reset();
                    self.istr_latched = 0;
                }
            }
            REG_WTC_HI => {
                self.wtc = (self.wtc & 0x0000_FFFF) | (u32::from(val) << 16);
            }
            REG_WTC_LO => {
                self.wtc = (self.wtc & 0xFFFF_0000) | u32::from(val);
            }
            REG_ACR_HI => {
                self.acr = (self.acr & 0x0000_FFFF) | (u32::from(val) << 16);
            }
            REG_ACR_LO => {
                self.acr = (self.acr & 0xFFFF_0000) | u32::from(val);
            }
            REG_ST_DMA => {
                self.dma_active = true;
            }
            REG_FLUSH => {
                // Flush FIFO — clear DMA buffer state.
                self.wd.dma_read_pos = 0;
            }
            REG_SP_DMA => {
                self.dma_active = false;
            }
            REG_CINT => {
                // Clear all latched interrupt flags.
                self.istr_latched = 0;
            }
            REG_SASR | REG_SASR_ALT => {
                // Write selects WD33C93 indirect register address.
                self.wd.selected_reg = val as u8 & 0x1F;
            }
            REG_SCMD | REG_SCMD_ALT => {
                // Write to WD33C93 register data port.
                self.wd.write_data(val as u8);
            }
            _ => {} // Unknown register — ignore.
        }
    }

    /// Read a single byte from an SDMAC address.
    ///
    /// Extracts the correct byte from a word read based on address
    /// alignment (even = high byte, odd = low byte).
    #[must_use]
    pub fn read_byte(&mut self, addr: u32) -> u8 {
        let word = self.read_word(addr & !1);
        if addr & 1 == 0 {
            (word >> 8) as u8
        } else {
            word as u8
        }
    }

    /// Write a single byte to an SDMAC address.
    pub fn write_byte(&mut self, addr: u32, val: u8) {
        // Most registers are in the low byte of the word. Route the
        // byte to a word write. For simplicity, put it in both halves.
        self.write_word(addr & !1, u16::from(val));
    }
}

impl Default for Dmac390537 {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn reg_addr(offset: u8) -> u32 {
        0xDD_0000 | u32::from(offset)
    }

    /// Helper: create a DMAC with a 1 MB disk at SCSI ID 0.
    fn dmac_with_disk() -> Dmac390537 {
        let mut image = vec![0u8; 1024 * 1024]; // 2048 sectors
        // Put a recognisable pattern in sector 0.
        image[0] = 0xDE;
        image[1] = 0xAD;
        image[510] = 0xBE;
        image[511] = 0xEF;
        Dmac390537::with_disk(0, image)
    }

    /// Helper: issue a SCSI command via SEL_ATN_XFER.
    fn issue_scsi_command(d: &mut Dmac390537, target_id: u8, cdb: &[u8]) {
        let sasr = reg_addr(REG_SASR);
        let scmd = reg_addr(REG_SCMD);

        // Set DESTINATION_ID.
        d.write_word(sasr, wd_reg::DESTINATION_ID as u16);
        d.write_word(scmd, u16::from(target_id));

        // Write CDB starting at TOTAL_SECTORS ($03).
        d.write_word(sasr, wd_reg::TOTAL_SECTORS as u16);
        for &b in cdb {
            d.write_word(scmd, u16::from(b));
        }

        // Issue SEL_ATN_XFER.
        d.write_word(sasr, wd_reg::COMMAND as u16);
        d.write_word(scmd, wd_cmd::SEL_ATN_XFER as u16);
    }

    // -- Original stub tests (preserved) ------------------------------------

    #[test]
    fn power_on_istr_fifo_empty() {
        let mut d = Dmac390537::new();
        let istr = d.read_word(reg_addr(REG_ISTR)) as u8;
        // FIFO is empty after power-on.
        assert_eq!(istr & istr_bits::FE_FLG, istr_bits::FE_FLG);
        // WD33C93 power-on reset generates an interrupt (INTS + INT_F).
        assert_eq!(istr & istr_bits::INTS, istr_bits::INTS);
        assert_eq!(istr & istr_bits::INT_F, istr_bits::INT_F);
        // INT_P requires CNTR.INTEN (which is 0 at power-on).
        assert_eq!(istr & istr_bits::INT_P, 0);
    }

    #[test]
    fn cntr_roundtrip() {
        let mut d = Dmac390537::new();
        let cntr_addr = reg_addr(REG_CNTR);
        d.write_word(cntr_addr, 0x04); // INTEN
        assert_eq!(d.read_word(cntr_addr) as u8, 0x04);
    }

    #[test]
    fn wd_reset_sets_int_and_status() {
        let mut d = Dmac390537::new();
        let sasr_addr = reg_addr(REG_SASR);
        let scmd_addr = reg_addr(REG_SCMD);

        // Write OWN_ID with EAF bit set.
        d.write_word(sasr_addr, wd_reg::OWN_ID as u16);
        d.write_word(scmd_addr, 0x0F); // SCSI ID 7 + EAF

        // Write COMMAND = RESET.
        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, wd_cmd::RESET as u16);

        // ASR should have INT set.
        let asr = d.read_word(sasr_addr) as u8;
        assert_ne!(asr & wd_asr::INT, 0, "ASR.INT should be set after reset");

        // SCSI_STATUS should be CSR_RESET_AF.
        d.write_word(sasr_addr, wd_reg::SCSI_STATUS as u16);
        let status = d.read_word(scmd_addr) as u8;
        assert_eq!(status, wd_csr::RESET_AF);

        // Reading SCSI_STATUS should clear ASR.INT.
        let asr_after = d.read_word(sasr_addr) as u8;
        assert_eq!(asr_after & wd_asr::INT, 0, "ASR.INT should be cleared");
    }

    #[test]
    fn wd_select_times_out() {
        let mut d = Dmac390537::new();
        let sasr_addr = reg_addr(REG_SASR);
        let scmd_addr = reg_addr(REG_SCMD);

        // Set DESTINATION_ID to target 0.
        d.write_word(sasr_addr, wd_reg::DESTINATION_ID as u16);
        d.write_word(scmd_addr, 0x00);

        // Issue SEL_ATN_XFER.
        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, wd_cmd::SEL_ATN_XFER as u16);

        // ASR.INT should be set.
        let asr = d.read_word(sasr_addr) as u8;
        assert_ne!(asr & wd_asr::INT, 0);

        // SCSI_STATUS should be CSR_TIMEOUT.
        d.write_word(sasr_addr, wd_reg::SCSI_STATUS as u16);
        let status = d.read_word(scmd_addr) as u8;
        assert_eq!(status, wd_csr::TIMEOUT);
    }

    #[test]
    fn cint_clears_latched_flags() {
        let mut d = Dmac390537::new();
        let cint_addr = reg_addr(REG_CINT);

        d.istr_latched = 0xFF;
        d.write_word(cint_addr, 0);
        assert_eq!(d.istr_latched, 0);
    }

    #[test]
    fn istr_reflects_live_wd_interrupt() {
        let mut d = Dmac390537::new();
        let sasr_addr = reg_addr(REG_SASR);
        let scmd_addr = reg_addr(REG_SCMD);
        let istr_addr = reg_addr(REG_ISTR);

        // Trigger a reset to set WD INT.
        d.write_word(sasr_addr, wd_reg::OWN_ID as u16);
        d.write_word(scmd_addr, 0x07); // SCSI ID 7, no EAF
        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, wd_cmd::RESET as u16);

        let istr = d.read_word(istr_addr) as u8;
        assert_ne!(istr & istr_bits::INTS, 0, "ISTR.INTS should reflect WD INT");
        assert_ne!(istr & istr_bits::INT_F, 0, "ISTR.INT_F should be set");
    }

    #[test]
    fn istr_int_p_requires_inten() {
        let mut d = Dmac390537::new();
        let sasr_addr = reg_addr(REG_SASR);
        let scmd_addr = reg_addr(REG_SCMD);
        let istr_addr = reg_addr(REG_ISTR);
        let cntr_addr = reg_addr(REG_CNTR);

        // Trigger WD INT.
        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, wd_cmd::RESET as u16);

        // INT_P should be 0 without INTEN.
        let istr = d.read_word(istr_addr) as u8;
        assert_eq!(istr & istr_bits::INT_P, 0);

        // Enable INTEN.
        d.write_word(cntr_addr, cntr_bits::INTEN as u16);
        let istr = d.read_word(istr_addr) as u8;
        assert_ne!(istr & istr_bits::INT_P, 0);
    }

    #[test]
    fn prest_resets_wd() {
        let mut d = Dmac390537::new();
        let cntr_addr = reg_addr(REG_CNTR);
        let sasr_addr = reg_addr(REG_SASR);

        // Set some WD state.
        d.wd.asr = 0xFF;
        d.wd.regs[0] = 0xAA;

        // Assert PREST.
        d.write_word(cntr_addr, cntr_bits::PREST as u16);

        // WD should be reset.
        let asr = d.read_word(sasr_addr) as u8;
        assert_eq!(asr, 0);
        assert_eq!(d.wd.regs[0], 0);
    }

    #[test]
    fn byte_read_odd_returns_low_byte() {
        let mut d = Dmac390537::new();
        let cntr_addr = reg_addr(REG_CNTR);
        d.write_word(cntr_addr, 0x07);

        // Odd byte address should return the low byte of the word.
        let val = d.read_byte(cntr_addr | 1);
        assert_eq!(val, 0x07);
    }

    #[test]
    fn all_seven_ids_timeout() {
        let mut d = Dmac390537::new();
        let sasr_addr = reg_addr(REG_SASR);
        let scmd_addr = reg_addr(REG_SCMD);

        for target_id in 0..7u8 {
            // Set DESTINATION_ID.
            d.write_word(sasr_addr, wd_reg::DESTINATION_ID as u16);
            d.write_word(scmd_addr, u16::from(target_id));

            // Issue SELECT.
            d.write_word(sasr_addr, wd_reg::COMMAND as u16);
            d.write_word(scmd_addr, wd_cmd::SEL_ATN as u16);

            // Verify timeout.
            d.write_word(sasr_addr, wd_reg::SCSI_STATUS as u16);
            let status = d.read_word(scmd_addr) as u8;
            assert_eq!(status, wd_csr::TIMEOUT, "target {target_id} should timeout");
        }
    }

    #[test]
    fn reset_restores_dma_register_defaults() {
        let mut d = Dmac390537::new();
        d.cntr = cntr_bits::INTEN;
        d.dawr = 0x03;
        d.wtc = 0x12_3456;
        d.acr = 0x89AB_CDEF;
        d.istr_latched = 0xA0;
        d.wd.selected_reg = 0x1F;
        d.wd.asr = wd_asr::INT;

        d.reset();

        assert_eq!(d.cntr, 0);
        assert_eq!(d.dawr, 0);
        assert_eq!(d.wtc, 0);
        assert_eq!(d.acr, 0);
        assert_eq!(d.istr_latched, 0);
        assert_eq!(d.wd.selected_reg, 0);
        assert_eq!(d.read_word(reg_addr(REG_ISTR)) as u8, istr_bits::FE_FLG);
    }

    #[test]
    fn dawr_masks_to_low_two_bits() {
        let mut d = Dmac390537::new();
        let dawr_addr = reg_addr(REG_DAWR);

        d.write_word(dawr_addr, 0x00FF);

        assert_eq!(d.dawr, 0x03);
    }

    #[test]
    fn wtc_and_acr_roundtrip() {
        let mut d = Dmac390537::new();
        let wtc_hi_addr = reg_addr(REG_WTC_HI);
        let wtc_lo_addr = reg_addr(REG_WTC_LO);
        let acr_hi_addr = reg_addr(REG_ACR_HI);
        let acr_lo_addr = reg_addr(REG_ACR_LO);

        d.write_word(wtc_hi_addr, 0x1234);
        d.write_word(wtc_lo_addr, 0x5678);
        d.write_word(acr_hi_addr, 0x89AB);
        d.write_word(acr_lo_addr, 0xCDEF);

        assert_eq!(d.read_word(wtc_hi_addr), 0x1234);
        assert_eq!(d.read_word(wtc_lo_addr), 0x5678);
        assert_eq!(d.read_word(acr_hi_addr), 0x89AB);
        assert_eq!(d.read_word(acr_lo_addr), 0xCDEF);
    }

    #[test]
    fn byte_access_uses_low_register_byte() {
        let mut d = Dmac390537::new();
        let cntr_addr = reg_addr(REG_CNTR);

        d.write_byte(cntr_addr + 1, cntr_bits::INTEN);

        assert_eq!(d.read_byte(cntr_addr), 0);
        assert_eq!(d.read_byte(cntr_addr + 1), cntr_bits::INTEN);
    }

    #[test]
    fn sasr_alt_port_mirrors_primary_and_auto_increments() {
        let mut d = Dmac390537::new();
        let sasr_alt_addr = reg_addr(REG_SASR_ALT);
        let scmd_alt_addr = reg_addr(REG_SCMD_ALT);
        let sasr_addr = reg_addr(REG_SASR);
        let scmd_addr = reg_addr(REG_SCMD);

        d.write_word(sasr_alt_addr, wd_reg::OWN_ID as u16);
        d.write_word(scmd_alt_addr, 0x12);
        d.write_word(scmd_alt_addr, 0x34);

        d.write_word(sasr_addr, wd_reg::OWN_ID as u16);
        assert_eq!(d.read_word(scmd_addr) as u8, 0x12);
        assert_eq!(d.read_word(scmd_addr) as u8, 0x34);
    }

    #[test]
    fn unknown_command_sets_lci_without_wd_interrupt() {
        let mut d = Dmac390537::new();
        let sasr_addr = reg_addr(REG_SASR);
        let scmd_addr = reg_addr(REG_SCMD);
        let istr_addr = reg_addr(REG_ISTR);

        // Clear the power-on RESET interrupt by reading SCSI_STATUS.
        d.write_word(sasr_addr, wd_reg::SCSI_STATUS as u16);
        let _ = d.read_word(scmd_addr);

        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, 0xFF);

        let asr = d.read_word(sasr_addr) as u8;
        assert_eq!(asr & 0x40, 0x40);
        assert_eq!(asr & wd_asr::INT, 0);
        assert_eq!(d.read_word(istr_addr) as u8 & istr_bits::INTS, 0);
    }

    #[test]
    fn wd_status_read_clears_live_dmac_interrupt_source() {
        let mut d = Dmac390537::new();
        let sasr_addr = reg_addr(REG_SASR);
        let scmd_addr = reg_addr(REG_SCMD);
        let istr_addr = reg_addr(REG_ISTR);

        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, wd_cmd::RESET as u16);

        d.write_word(sasr_addr, wd_reg::SCSI_STATUS as u16);
        assert_eq!(d.read_word(scmd_addr) as u8, wd_csr::RESET);

        let istr = d.read_word(istr_addr) as u8;
        assert_eq!(istr & istr_bits::INTS, 0);
        assert_eq!(istr & istr_bits::INT_F, 0);
        assert_eq!(d.wd_asr() & wd_asr::INT, 0);
    }

    #[test]
    fn cint_clears_latched_flags_but_not_live_wd_interrupt_source() {
        let mut d = Dmac390537::new();
        let sasr_addr = reg_addr(REG_SASR);
        let scmd_addr = reg_addr(REG_SCMD);
        let cint_addr = reg_addr(REG_CINT);
        let istr_addr = reg_addr(REG_ISTR);

        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, wd_cmd::RESET as u16);
        assert_ne!(d.read_word(istr_addr) as u8 & istr_bits::INTS, 0);

        d.istr_latched = 0x20;
        d.write_word(cint_addr, 0);

        let istr_after_cint = d.read_word(istr_addr) as u8;
        assert_eq!(d.wd_asr() & wd_asr::INT, wd_asr::INT);
        assert_eq!(istr_after_cint & 0x20, 0);
        assert_ne!(istr_after_cint & istr_bits::INTS, 0);
        assert_ne!(istr_after_cint & istr_bits::INT_F, 0);
    }

    #[test]
    fn public_state_accessors_reflect_register_values() {
        let mut d = Dmac390537::new();
        d.write_word(reg_addr(REG_DAWR), 0x0003);
        d.write_word(reg_addr(REG_WTC_HI), 0x0012);
        d.write_word(reg_addr(REG_WTC_LO), 0x3456);
        d.write_word(reg_addr(REG_ACR_HI), 0x89AB);
        d.write_word(reg_addr(REG_ACR_LO), 0xCDEF);
        d.write_word(reg_addr(REG_SASR), wd_reg::COMMAND as u16);

        assert_eq!(d.dawr(), 0x03);
        assert_eq!(d.wtc(), 0x0012_3456);
        assert_eq!(d.acr(), 0x89AB_CDEF);
        assert_eq!(d.wd_selected_reg(), wd_reg::COMMAND);
        assert_eq!(d.cntr(), 0x00);
        assert_eq!(d.current_istr() & istr_bits::FE_FLG, istr_bits::FE_FLG);
        assert!(!d.irq_pending());
    }

    #[test]
    fn real_rom_byte_offsets_hit_the_expected_registers() {
        let mut d = Dmac390537::new();

        d.write_byte(reg_addr(REG_SASR_ALT) + 1, wd_reg::DESTINATION_ID);
        assert_eq!(d.wd_selected_reg(), wd_reg::DESTINATION_ID);

        d.write_byte(reg_addr(REG_SCMD) + 1, 0x05);
        assert_eq!(d.wd.regs[wd_reg::DESTINATION_ID as usize], 0x05);

        d.write_byte(reg_addr(REG_CNTR) + 1, cntr_bits::INTEN);
        assert_eq!(d.cntr(), cntr_bits::INTEN);
    }

    #[test]
    fn irq_pending_tracks_int_p() {
        let mut d = Dmac390537::new();
        let sasr_addr = reg_addr(REG_SASR);
        let scmd_addr = reg_addr(REG_SCMD);
        let cntr_addr = reg_addr(REG_CNTR);

        d.write_word(sasr_addr, wd_reg::COMMAND as u16);
        d.write_word(scmd_addr, wd_cmd::RESET as u16);
        assert!(!d.irq_pending());

        d.write_word(cntr_addr, cntr_bits::INTEN as u16);
        assert!(d.irq_pending());
    }

    // -- New SCSI target tests ----------------------------------------------

    #[test]
    fn with_disk_makes_target_present() {
        let d = dmac_with_disk();
        assert!(d.target_present(0));
        assert!(!d.target_present(1));
        assert!(!d.target_present(6));
    }

    #[test]
    fn select_present_target_succeeds() {
        let mut d = dmac_with_disk();
        let sasr = reg_addr(REG_SASR);
        let scmd = reg_addr(REG_SCMD);

        d.write_word(sasr, wd_reg::DESTINATION_ID as u16);
        d.write_word(scmd, 0x00); // target 0

        d.write_word(sasr, wd_reg::COMMAND as u16);
        d.write_word(scmd, wd_cmd::SEL_ATN as u16);

        d.write_word(sasr, wd_reg::SCSI_STATUS as u16);
        let status = d.read_word(scmd) as u8;
        assert_eq!(status, wd_csr::SEL_COMPLETE);
    }

    #[test]
    fn select_absent_target_still_times_out() {
        let mut d = dmac_with_disk();
        let sasr = reg_addr(REG_SASR);
        let scmd = reg_addr(REG_SCMD);

        d.write_word(sasr, wd_reg::DESTINATION_ID as u16);
        d.write_word(scmd, 0x03); // target 3 (absent)

        d.write_word(sasr, wd_reg::COMMAND as u16);
        d.write_word(scmd, wd_cmd::SEL_ATN_XFER as u16);

        d.write_word(sasr, wd_reg::SCSI_STATUS as u16);
        let status = d.read_word(scmd) as u8;
        assert_eq!(status, wd_csr::TIMEOUT);
    }

    #[test]
    fn inquiry_returns_disk_type() {
        let mut d = dmac_with_disk();
        issue_scsi_command(&mut d, 0, &[scsi_cmd::INQUIRY, 0, 0, 0, 96, 0]);

        assert!(d.dma_pending());
        assert!(d.dma_direction_read());

        let peripheral = d.dma_read_byte();
        assert_eq!(peripheral & 0x1F, 0x00, "Should be direct-access device");

        // Read remaining bytes.
        let mut buf = vec![peripheral];
        for _ in 1..36 {
            buf.push(d.dma_read_byte());
        }
        // Vendor at bytes 8-15.
        let vendor = std::str::from_utf8(&buf[8..16]).unwrap_or("");
        assert_eq!(vendor, "EMU198X ");
    }

    #[test]
    fn read_capacity_returns_correct_size() {
        let mut d = dmac_with_disk();
        issue_scsi_command(&mut d, 0, &[scsi_cmd::READ_CAPACITY_10, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

        let mut buf = [0u8; 8];
        for b in &mut buf {
            *b = d.dma_read_byte();
        }
        let last_lba = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let block_size = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);

        // 1 MB = 2048 sectors, last LBA = 2047.
        assert_eq!(last_lba, 2047);
        assert_eq!(block_size, 512);
    }

    #[test]
    fn read_6_returns_disk_data() {
        let mut d = dmac_with_disk();
        // READ(6): LBA 0, count 1.
        issue_scsi_command(&mut d, 0, &[scsi_cmd::READ_6, 0, 0, 0, 1, 0]);

        assert!(d.dma_pending());
        assert_eq!(d.dma_bytes_remaining(), 512);

        let b0 = d.dma_read_byte();
        let b1 = d.dma_read_byte();
        assert_eq!(b0, 0xDE);
        assert_eq!(b1, 0xAD);

        // Skip to end.
        for _ in 2..510 {
            let _ = d.dma_read_byte();
        }
        let b510 = d.dma_read_byte();
        let b511 = d.dma_read_byte();
        assert_eq!(b510, 0xBE);
        assert_eq!(b511, 0xEF);
        assert!(!d.dma_pending());
    }

    #[test]
    fn read_10_multi_sector() {
        let mut d = dmac_with_disk();
        // READ(10): LBA 0, count 2.
        issue_scsi_command(
            &mut d, 0,
            &[scsi_cmd::READ_10, 0, 0, 0, 0, 0, 0, 0, 2, 0],
        );

        assert_eq!(d.dma_bytes_remaining(), 1024);
    }

    #[test]
    fn write_6_commits_to_image() {
        let mut d = dmac_with_disk();
        // WRITE(6): LBA 1, count 1.
        issue_scsi_command(&mut d, 0, &[scsi_cmd::WRITE_6, 0, 0, 1, 1, 0]);

        assert!(d.dma_pending());
        assert!(!d.dma_direction_read()); // write = initiator→target

        // Write a pattern.
        for i in 0..512u16 {
            d.dma_write_byte(i as u8);
        }
        assert!(!d.dma_pending());

        // Read back via READ(6).
        issue_scsi_command(&mut d, 0, &[scsi_cmd::READ_6, 0, 0, 1, 1, 0]);
        let b0 = d.dma_read_byte();
        let b1 = d.dma_read_byte();
        assert_eq!(b0, 0x00);
        assert_eq!(b1, 0x01);
    }

    #[test]
    fn test_unit_ready_clears_sense() {
        let mut d = dmac_with_disk();
        issue_scsi_command(&mut d, 0, &[scsi_cmd::TEST_UNIT_READY, 0, 0, 0, 0, 0]);

        // Request sense should return no error.
        issue_scsi_command(&mut d, 0, &[scsi_cmd::REQUEST_SENSE, 0, 0, 0, 18, 0]);
        let mut buf = [0u8; 18];
        for b in &mut buf {
            *b = d.dma_read_byte();
        }
        assert_eq!(buf[2] & 0x0F, 0, "Sense key should be 0 (no error)");
    }

    #[test]
    fn mode_sense_returns_header() {
        let mut d = dmac_with_disk();
        issue_scsi_command(&mut d, 0, &[scsi_cmd::MODE_SENSE_6, 0, 0, 0, 12, 0]);

        assert!(d.dma_pending());
        let mode_data_len = d.dma_read_byte();
        assert_eq!(mode_data_len, 11); // 12 - 1
    }

    #[test]
    fn unknown_scsi_command_sets_check_condition() {
        let mut d = dmac_with_disk();
        issue_scsi_command(&mut d, 0, &[0xFF, 0, 0, 0, 0, 0]); // Invalid opcode

        // Request sense should report ILLEGAL REQUEST.
        issue_scsi_command(&mut d, 0, &[scsi_cmd::REQUEST_SENSE, 0, 0, 0, 18, 0]);
        let mut buf = [0u8; 18];
        for b in &mut buf {
            *b = d.dma_read_byte();
        }
        assert_eq!(buf[2] & 0x0F, 0x05, "Sense key should be ILLEGAL REQUEST");
        assert_eq!(buf[12], 0x20, "ASC should be 0x20 (invalid command)");
    }

    #[test]
    fn read_past_end_sets_lba_out_of_range() {
        let mut d = dmac_with_disk();
        // 1 MB disk = 2048 sectors. Try to read sector 2048.
        issue_scsi_command(
            &mut d, 0,
            &[scsi_cmd::READ_10, 0, 0, 0, 0x08, 0x00, 0, 0, 1, 0], // LBA 2048
        );

        issue_scsi_command(&mut d, 0, &[scsi_cmd::REQUEST_SENSE, 0, 0, 0, 18, 0]);
        let mut buf = [0u8; 18];
        for b in &mut buf {
            *b = d.dma_read_byte();
        }
        assert_eq!(buf[2] & 0x0F, 0x05);
        assert_eq!(buf[12], 0x21, "ASC should be LBA out of range");
    }

    #[test]
    fn attach_disk_after_construction() {
        let mut d = Dmac390537::new();
        assert!(!d.target_present(2));

        d.attach_disk(2, vec![0u8; 512]);
        assert!(d.target_present(2));
    }

    #[test]
    fn st_dma_and_sp_dma_strobes() {
        let mut d = Dmac390537::new();
        assert!(!d.dma_active());

        d.write_word(reg_addr(REG_ST_DMA), 0);
        assert!(d.dma_active());

        d.write_word(reg_addr(REG_SP_DMA), 0);
        assert!(!d.dma_active());
    }

    #[test]
    fn all_seven_ids_timeout_no_targets() {
        // Verify original no-target behaviour is preserved.
        let mut d = Dmac390537::new();
        let sasr = reg_addr(REG_SASR);
        let scmd = reg_addr(REG_SCMD);

        for id in 0..7u8 {
            d.write_word(sasr, wd_reg::DESTINATION_ID as u16);
            d.write_word(scmd, u16::from(id));
            d.write_word(sasr, wd_reg::COMMAND as u16);
            d.write_word(scmd, wd_cmd::SEL_ATN_XFER as u16);

            d.write_word(sasr, wd_reg::SCSI_STATUS as u16);
            assert_eq!(d.read_word(scmd) as u8, wd_csr::TIMEOUT);
        }
    }

    #[test]
    fn mixed_targets_only_present_ones_respond() {
        let mut d = Dmac390537::new();
        d.attach_disk(0, vec![0u8; 512]);
        d.attach_disk(6, vec![0u8; 512]);

        let sasr = reg_addr(REG_SASR);
        let scmd = reg_addr(REG_SCMD);

        // ID 0 should succeed.
        d.write_word(sasr, wd_reg::DESTINATION_ID as u16);
        d.write_word(scmd, 0);
        d.write_word(sasr, wd_reg::COMMAND as u16);
        d.write_word(scmd, wd_cmd::SEL as u16);
        d.write_word(sasr, wd_reg::SCSI_STATUS as u16);
        assert_eq!(d.read_word(scmd) as u8, wd_csr::SEL_COMPLETE);

        // ID 3 should timeout.
        d.write_word(sasr, wd_reg::DESTINATION_ID as u16);
        d.write_word(scmd, 3);
        d.write_word(sasr, wd_reg::COMMAND as u16);
        d.write_word(scmd, wd_cmd::SEL as u16);
        d.write_word(sasr, wd_reg::SCSI_STATUS as u16);
        assert_eq!(d.read_word(scmd) as u8, wd_csr::TIMEOUT);

        // ID 6 should succeed.
        d.write_word(sasr, wd_reg::DESTINATION_ID as u16);
        d.write_word(scmd, 6);
        d.write_word(sasr, wd_reg::COMMAND as u16);
        d.write_word(scmd, wd_cmd::SEL as u16);
        d.write_word(sasr, wd_reg::SCSI_STATUS as u16);
        assert_eq!(d.read_word(scmd) as u8, wd_csr::SEL_COMPLETE);
    }
}
