//! Commodore Gayle gate array — IDE interface, PCMCIA slot, and address
//! decoding for the Amiga 600 and Amiga 1200.
//!
//! Gayle sits between the CPU and the $D80000-$DFFFFF address range,
//! providing IDE task-file registers and four control/status registers.
//! It also manages the PCMCIA slot, routing common memory ($600000-$9FFFFF),
//! attribute/IO ($A00000-$A5FFFF), and card reset ($A40000-$A5FFFF)
//! to the inserted card.
//!
//! Without a drive attached, IDE STATUS reads $7F ("no drive") and other
//! task-file registers read $FF — matching WinUAE behaviour.

pub mod ne2000;

// ---------------------------------------------------------------------------
// IDE ATA constants
// ---------------------------------------------------------------------------

/// IDE STATUS register bits.
const _STATUS_BSY: u8 = 0x80;
const STATUS_DRDY: u8 = 0x40;
const STATUS_DRQ: u8 = 0x08;
const STATUS_ERR: u8 = 0x01;

/// IDE ERROR register bits.
const ERROR_ABRT: u8 = 0x04;

/// IDE ERROR register bits.
const ERROR_IDNF: u8 = 0x10;

/// IDE commands.
const CMD_DEVICE_RESET: u8 = 0x08;
const CMD_READ_SECTORS: u8 = 0x20;
const CMD_WRITE_SECTORS: u8 = 0x30;
const CMD_READ_MULTIPLE: u8 = 0xC4;
const CMD_WRITE_MULTIPLE: u8 = 0xC5;
const CMD_SET_MULTIPLE_MODE: u8 = 0xC6;
const CMD_EXECUTE_DIAGNOSTIC: u8 = 0x90;
const CMD_INIT_DEVICE_PARAMS: u8 = 0x91;
const CMD_READ_VERIFY: u8 = 0x40;
const CMD_SEEK: u8 = 0x70;
const CMD_IDENTIFY_DEVICE: u8 = 0xEC;
const CMD_SET_FEATURES: u8 = 0xEF;

// ---------------------------------------------------------------------------
// Gayle Card Status register bit definitions
// ---------------------------------------------------------------------------

/// IDE interrupt active.
const GAYLE_CS_IDE: u8 = 0x80;
/// Card detect — card is inserted.
const GAYLE_CS_CCDET: u8 = 0x40;
/// Battery voltage detect 1.
const _GAYLE_CS_BVD1: u8 = 0x20;
/// Battery voltage detect 2.
const _GAYLE_CS_BVD2: u8 = 0x10;
/// Write protect — card is writable when set.
const GAYLE_CS_WR: u8 = 0x08;
/// Busy / IRQ — PCMCIA card interrupt pending.
const GAYLE_CS_BSY: u8 = 0x04;
/// Data acknowledge enable.
const _GAYLE_CS_DAEN: u8 = 0x02;
/// Disable — when set, PCMCIA slot is disabled.
const GAYLE_CS_DIS: u8 = 0x01;

// ---------------------------------------------------------------------------
// Disk geometry
// ---------------------------------------------------------------------------

/// CHS geometry for an IDE disk image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskGeometry {
    pub cylinders: u16,
    pub heads: u8,
    pub sectors_per_track: u8,
}

impl DiskGeometry {
    /// Total sectors in the geometry.
    #[must_use]
    pub const fn total_sectors(&self) -> u32 {
        self.cylinders as u32 * self.heads as u32 * self.sectors_per_track as u32
    }

    /// Derive a reasonable geometry from a disk image size.
    /// Uses 16 heads and 63 sectors/track (standard LBA-assist geometry).
    #[must_use]
    pub fn from_image_size(size: usize) -> Self {
        let total_sectors = (size / 512) as u32;
        let heads: u8 = 16;
        let spt: u8 = 63;
        let cylinders = total_sectors / (heads as u32 * spt as u32);
        Self {
            cylinders: cylinders.min(65535) as u16,
            heads,
            sectors_per_track: spt,
        }
    }
}

// ---------------------------------------------------------------------------
// IDE drive state
// ---------------------------------------------------------------------------

/// State of the IDE data transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdeState {
    /// Idle — no pending data transfer.
    Idle,
    /// Data transfer to host (e.g. after IDENTIFY or READ).
    DataIn,
    /// Data transfer from host (e.g. WRITE).
    DataOut,
}

/// Internal state for an attached IDE drive.
#[derive(Debug, Clone)]
struct IdeDrive {
    // Task-file registers
    error: u8,
    sector_count: u8,
    sector_number: u8,
    cylinder_lo: u8,
    cylinder_hi: u8,
    dev_head: u8,
    status: u8,

    // Data transfer state
    state: IdeState,
    data_buffer: Vec<u8>,
    data_pos: usize,
    data_len: usize,

    // Multi-sector transfer state
    sectors_remaining: u16,
    sectors_in_block: u16,
    sectors_per_irq: u16,

    // Configuration
    multiple_count: u8,
    logical_heads: u8,
    logical_spt: u8,

    // Disk image
    disk_image: Vec<u8>,
    geometry: DiskGeometry,

    // IRQ
    irq_pending: bool,
    /// NIEN (No Interrupt Enable) — when set, suppress IRQ assertion.
    nien: bool,
}

impl IdeDrive {
    fn new(image: Vec<u8>, geometry: DiskGeometry) -> Self {
        Self {
            error: 0x01, // diagnostic: no error
            sector_count: 0x01,
            sector_number: 0x01,
            cylinder_lo: 0,
            cylinder_hi: 0,
            dev_head: 0,
            status: STATUS_DRDY,
            state: IdeState::Idle,
            data_buffer: vec![0u8; 512],
            data_pos: 0,
            data_len: 0,
            sectors_remaining: 0,
            sectors_in_block: 0,
            sectors_per_irq: 1,
            multiple_count: 0,
            logical_heads: geometry.heads,
            logical_spt: geometry.sectors_per_track,
            disk_image: image,
            geometry,
            irq_pending: false,
            nien: false,
        }
    }

    fn reset(&mut self) {
        self.error = 0x01;
        self.sector_count = 0x01;
        self.sector_number = 0x01;
        self.cylinder_lo = 0;
        self.cylinder_hi = 0;
        self.dev_head = 0;
        self.status = STATUS_DRDY;
        self.state = IdeState::Idle;
        self.data_pos = 0;
        self.data_len = 0;
        self.sectors_remaining = 0;
        self.sectors_in_block = 0;
        self.sectors_per_irq = 1;
        self.irq_pending = false;
        self.nien = false;
    }

    /// Read the 16-bit DATA register (pulls two bytes from the transfer buffer).
    fn read_data_word(&mut self) -> u16 {
        if self.state != IdeState::DataIn || self.data_pos >= self.data_len {
            return 0;
        }
        let hi = self.data_buffer[self.data_pos];
        let lo = self.data_buffer[self.data_pos + 1];
        self.data_pos += 2;

        // If the buffer is exhausted, load next block or go idle.
        if self.data_pos >= self.data_len {
            if self.sectors_remaining > 0 {
                self.load_sector_block();
            } else {
                self.state = IdeState::Idle;
                self.status = STATUS_DRDY;
            }
        }
        u16::from(hi) << 8 | u16::from(lo)
    }

    /// Write the 16-bit DATA register (pushes two bytes into the transfer buffer).
    fn write_data_word(&mut self, val: u16) {
        if self.state != IdeState::DataOut || self.data_pos >= self.data_len {
            return;
        }
        self.data_buffer[self.data_pos] = (val >> 8) as u8;
        self.data_buffer[self.data_pos + 1] = val as u8;
        self.data_pos += 2;

        // If the buffer is full, commit to disk.
        if self.data_pos >= self.data_len {
            self.commit_write_block();
            if self.sectors_remaining > 0 {
                self.prepare_write_block();
            } else {
                self.state = IdeState::Idle;
                self.status = STATUS_DRDY;
            }
            self.irq_pending = !self.nien;
        }
    }

    /// Convert CHS or LBA from task-file registers to a byte offset.
    fn lba_offset(&self) -> Option<u64> {
        let lba_mode = self.dev_head & 0x40 != 0;
        let lba = if lba_mode {
            u32::from(self.dev_head & 0x0F) << 24
                | u32::from(self.cylinder_hi) << 16
                | u32::from(self.cylinder_lo) << 8
                | u32::from(self.sector_number)
        } else {
            // CHS: sector numbers are 1-based
            let c = u32::from(self.cylinder_hi) << 8 | u32::from(self.cylinder_lo);
            let h = u32::from(self.dev_head & 0x0F);
            let s = u32::from(self.sector_number);
            if s == 0 {
                return None;
            }
            (c * u32::from(self.logical_heads) + h) * u32::from(self.logical_spt)
                + (s - 1)
        };
        Some(u64::from(lba) * 512)
    }

    /// Execute a command written to the COMMAND register.
    fn execute_command(&mut self, cmd: u8) {
        match cmd {
            CMD_IDENTIFY_DEVICE => self.cmd_identify(),
            CMD_READ_SECTORS => self.cmd_read_sectors(),
            CMD_WRITE_SECTORS => self.cmd_write_sectors(),
            CMD_READ_MULTIPLE => self.cmd_read_multiple(),
            CMD_WRITE_MULTIPLE => self.cmd_write_multiple(),
            CMD_SET_MULTIPLE_MODE => self.cmd_set_multiple_mode(),
            CMD_INIT_DEVICE_PARAMS => self.cmd_init_device_params(),
            CMD_READ_VERIFY => self.cmd_read_verify(),
            CMD_SEEK => self.cmd_seek(),
            CMD_SET_FEATURES => self.cmd_set_features(),
            CMD_DEVICE_RESET => self.reset(),
            CMD_EXECUTE_DIAGNOSTIC => {
                self.error = 0x01; // no error
                self.status = STATUS_DRDY;
            }
            _ => {
                // Unknown command: set ABRT
                self.error = ERROR_ABRT;
                self.status = STATUS_DRDY | STATUS_ERR;
                self.irq_pending = !self.nien;
            }
        }
    }

    fn cmd_identify(&mut self) {
        // Fill 256 words (512 bytes) of IDENTIFY data.
        self.data_buffer.resize(512, 0);
        self.data_buffer.fill(0);

        let buf = &mut self.data_buffer;
        let g = &self.geometry;

        // Word 0: General configuration — fixed disk
        set_word(buf, 0, 0x0040);
        // Word 1: Number of cylinders
        set_word(buf, 1, g.cylinders);
        // Word 3: Number of heads
        set_word(buf, 3, u16::from(g.heads));
        // Word 6: Sectors per track
        set_word(buf, 6, u16::from(g.sectors_per_track));
        // Words 27-46: Model number (padded with spaces, big-endian pairs)
        let model = b"Emu198x IDE Disk            ";
        set_string(buf, 27, model);
        // Word 47: Max sectors per multiple R/W (16, valid flag set)
        set_word(buf, 47, 0x8010);
        // Word 49: Capabilities — LBA supported
        set_word(buf, 49, 0x0200);
        // Word 53: Field validity — words 54-58 valid
        set_word(buf, 53, 0x0001);
        // Word 54: Current cylinders
        set_word(buf, 54, g.cylinders);
        // Word 55: Current heads
        set_word(buf, 55, u16::from(g.heads));
        // Word 56: Current sectors per track
        set_word(buf, 56, u16::from(g.sectors_per_track));
        // Words 57-58: Current capacity in sectors (32-bit)
        let total = g.total_sectors();
        set_word(buf, 57, total as u16);
        set_word(buf, 58, (total >> 16) as u16);
        // Words 60-61: Total addressable LBA sectors (same as current)
        set_word(buf, 60, total as u16);
        set_word(buf, 61, (total >> 16) as u16);
        // Word 59: Current multiple sector setting
        if self.multiple_count > 0 {
            set_word(buf, 59, 0x0100 | u16::from(self.multiple_count));
        }

        self.data_pos = 0;
        self.data_len = 512;
        self.state = IdeState::DataIn;
        self.status = STATUS_DRDY | STATUS_DRQ;
        self.irq_pending = !self.nien;
    }

    // -- Helpers ------------------------------------------------------------

    fn effective_sector_count(&self) -> u16 {
        if self.sector_count == 0 {
            256
        } else {
            u16::from(self.sector_count)
        }
    }

    fn abort(&mut self, error_bits: u8) {
        self.error = error_bits;
        self.status = STATUS_DRDY | STATUS_ERR;
        self.state = IdeState::Idle;
        self.irq_pending = !self.nien;
    }

    /// Advance CHS/LBA address by one sector and decrement sector_count.
    fn advance_address(&mut self) {
        if self.dev_head & 0x40 != 0 {
            // LBA mode: increment 28-bit LBA.
            let mut lba = u32::from(self.dev_head & 0x0F) << 24
                | u32::from(self.cylinder_hi) << 16
                | u32::from(self.cylinder_lo) << 8
                | u32::from(self.sector_number);
            lba = lba.wrapping_add(1);
            self.sector_number = lba as u8;
            self.cylinder_lo = (lba >> 8) as u8;
            self.cylinder_hi = (lba >> 16) as u8;
            self.dev_head = (self.dev_head & 0xF0) | ((lba >> 24) as u8 & 0x0F);
        } else {
            // CHS mode: increment sector (1-based), overflow to head, overflow to cylinder.
            let spt = self.logical_spt;
            let heads = self.logical_heads;
            let mut s = self.sector_number;
            let mut h = self.dev_head & 0x0F;
            let mut c = u16::from(self.cylinder_hi) << 8 | u16::from(self.cylinder_lo);
            s += 1;
            if s > spt {
                s = 1;
                h += 1;
                if h >= heads {
                    h = 0;
                    c = c.wrapping_add(1);
                }
            }
            self.sector_number = s;
            self.dev_head = (self.dev_head & 0xF0) | (h & 0x0F);
            self.cylinder_lo = c as u8;
            self.cylinder_hi = (c >> 8) as u8;
        }
        self.sector_count = self.sector_count.wrapping_sub(1);
    }

    /// Load the next block of sectors into the read buffer.
    /// Block size is min(sectors_per_irq, sectors_remaining).
    fn load_sector_block(&mut self) {
        let block_size = self.sectors_remaining.min(self.sectors_per_irq) as usize;
        let total_bytes = block_size * 512;
        self.data_buffer.resize(total_bytes, 0);

        for i in 0..block_size {
            let Some(offset) = self.lba_offset() else {
                self.abort(ERROR_IDNF);
                return;
            };
            let offset = offset as usize;
            if offset + 512 > self.disk_image.len() {
                self.abort(ERROR_IDNF);
                return;
            }
            let buf_start = i * 512;
            self.data_buffer[buf_start..buf_start + 512]
                .copy_from_slice(&self.disk_image[offset..offset + 512]);
            self.advance_address();
            self.sectors_remaining -= 1;
        }

        self.sectors_in_block = block_size as u16;
        self.data_pos = 0;
        self.data_len = total_bytes;
        self.state = IdeState::DataIn;
        self.status = STATUS_DRDY | STATUS_DRQ;
        self.irq_pending = !self.nien;
    }

    /// Prepare the write buffer for the next block of sectors.
    fn prepare_write_block(&mut self) {
        let block_size = self.sectors_remaining.min(self.sectors_per_irq) as usize;
        let total_bytes = block_size * 512;
        self.data_buffer.resize(total_bytes, 0);
        self.data_buffer.fill(0);
        self.sectors_in_block = block_size as u16;
        self.data_pos = 0;
        self.data_len = total_bytes;
        self.state = IdeState::DataOut;
        self.status = STATUS_DRQ;
    }

    /// Commit the current write buffer to the disk image.
    fn commit_write_block(&mut self) {
        let block_size = self.sectors_in_block as usize;
        for i in 0..block_size {
            let Some(offset) = self.lba_offset() else {
                self.abort(ERROR_IDNF);
                return;
            };
            let offset = offset as usize;
            if offset + 512 > self.disk_image.len() {
                self.abort(ERROR_IDNF);
                return;
            }
            let buf_start = i * 512;
            self.disk_image[offset..offset + 512]
                .copy_from_slice(&self.data_buffer[buf_start..buf_start + 512]);
            self.advance_address();
            self.sectors_remaining -= 1;
        }
    }

    // -- Command implementations --------------------------------------------

    fn cmd_read_sectors(&mut self) {
        self.sectors_remaining = self.effective_sector_count();
        self.sectors_per_irq = 1;
        self.load_sector_block();
    }

    fn cmd_write_sectors(&mut self) {
        self.sectors_remaining = self.effective_sector_count();
        self.sectors_per_irq = 1;
        self.prepare_write_block();
    }

    fn cmd_read_multiple(&mut self) {
        if self.multiple_count == 0 {
            self.abort(ERROR_ABRT);
            return;
        }
        self.sectors_remaining = self.effective_sector_count();
        self.sectors_per_irq = u16::from(self.multiple_count);
        self.load_sector_block();
    }

    fn cmd_write_multiple(&mut self) {
        if self.multiple_count == 0 {
            self.abort(ERROR_ABRT);
            return;
        }
        self.sectors_remaining = self.effective_sector_count();
        self.sectors_per_irq = u16::from(self.multiple_count);
        self.prepare_write_block();
    }

    fn cmd_set_multiple_mode(&mut self) {
        let count = self.sector_count;
        if count == 0 || count > 16 {
            self.abort(ERROR_ABRT);
            return;
        }
        self.multiple_count = count;
        self.status = STATUS_DRDY;
        self.irq_pending = !self.nien;
    }

    fn cmd_init_device_params(&mut self) {
        // sector_count = sectors per track, dev_head & 0x0F = max head number
        let spt = self.sector_count;
        let max_head = self.dev_head & 0x0F;
        if spt == 0 {
            self.abort(ERROR_ABRT);
            return;
        }
        self.logical_spt = spt;
        self.logical_heads = max_head + 1; // max head number → head count
        self.status = STATUS_DRDY;
        self.irq_pending = !self.nien;
    }

    fn cmd_read_verify(&mut self) {
        // Verify sectors are readable without transferring data.
        let count = self.effective_sector_count();
        for _ in 0..count {
            let Some(offset) = self.lba_offset() else {
                self.abort(ERROR_IDNF);
                return;
            };
            let offset = offset as usize;
            if offset + 512 > self.disk_image.len() {
                self.abort(ERROR_IDNF);
                return;
            }
            self.advance_address();
        }
        self.status = STATUS_DRDY;
        self.irq_pending = !self.nien;
    }

    fn cmd_seek(&mut self) {
        // Validate the address is reachable.
        if self.lba_offset().is_none() {
            self.abort(ERROR_IDNF);
            return;
        }
        self.status = STATUS_DRDY;
        self.irq_pending = !self.nien;
    }

    fn cmd_set_features(&mut self) {
        // Accept but no-op — we don't model transfer modes.
        self.status = STATUS_DRDY;
        self.irq_pending = !self.nien;
    }
}

/// Set a 16-bit word in an IDENTIFY buffer at the given word index.
fn set_word(buf: &mut [u8], word_idx: usize, val: u16) {
    let byte_idx = word_idx * 2;
    buf[byte_idx] = (val >> 8) as u8;
    buf[byte_idx + 1] = val as u8;
}

/// Set a string in an IDENTIFY buffer at the given word index.
/// ATA strings are stored as big-endian character pairs, with each pair
/// byte-swapped (first char in even byte, second char in odd byte).
fn set_string(buf: &mut [u8], start_word: usize, s: &[u8]) {
    let byte_idx = start_word * 2;
    for (i, &ch) in s.iter().enumerate() {
        // ATA swaps character pairs: even index → high byte, odd → low byte
        let dest = byte_idx + (i ^ 1);
        if dest < buf.len() {
            buf[dest] = ch;
        }
    }
}

// ---------------------------------------------------------------------------
// PCMCIA card types
// ---------------------------------------------------------------------------

use ne2000::Ne2000State;

/// PCMCIA card inserted into the Gayle slot.
///
/// Variants are not constructible externally — use the `Gayle::with_pcmcia_*`
/// constructors instead.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub(crate) enum PcmciaCard {
    /// SRAM memory card — direct byte-addressable storage.
    Sram {
        common: Vec<u8>,
        cis: Vec<u8>,
        readonly: bool,
    },
    /// CompactFlash card — ATA via PCMCIA I/O space.
    CompactFlash {
        drive: IdeDrive,
        cis: Vec<u8>,
        configured: i8,
    },
    /// NE2000 PCMCIA Ethernet adapter (DP8390-based).
    Ne2000 {
        nic: Ne2000State,
        cis: Vec<u8>,
        configured: i8,
    },
}

// ---------------------------------------------------------------------------
// CIS tuple generation
// ---------------------------------------------------------------------------

// PCMCIA CIS tuple codes.
const CISTPL_DEVICE: u8 = 0x01;
const CISTPL_VERS_1: u8 = 0x15;
const CISTPL_CONFIG: u8 = 0x1A;
const CISTPL_CFTABLE_ENTRY: u8 = 0x1B;
const CISTPL_DEVICE_GEO: u8 = 0x1E;
const CISTPL_MANFID: u8 = 0x20;
const CISTPL_FUNCID: u8 = 0x21;
const CISTPL_END: u8 = 0xFF;

/// Build CIS tuples for an SRAM card.
fn build_sram_cis(size: usize) -> Vec<u8> {
    let mut cis = Vec::with_capacity(64);

    // CISTPL_DEVICE: SRAM, 100ns, size.
    cis.push(CISTPL_DEVICE);
    let size_code = match size {
        s if s <= 8 * 1024 => 0x01,        // 8 KB
        s if s <= 32 * 1024 => 0x02,        // 32 KB
        s if s <= 128 * 1024 => 0x03,       // 128 KB
        s if s <= 512 * 1024 => 0x04,       // 512 KB
        s if s <= 2 * 1024 * 1024 => 0x05,  // 2 MB
        _ => 0x06,                           // 4+ MB
    };
    cis.push(2); // tuple length
    cis.push(0x46); // SRAM device type (0x40) | 100ns speed (0x06)
    cis.push(size_code);

    // CISTPL_DEVICEGEO: 16-bit bus width.
    cis.push(CISTPL_DEVICE_GEO);
    cis.push(6); // tuple length
    cis.push(2); // bus width: 2 bytes (16-bit)
    cis.push(1); // erase block size (not applicable, 1)
    cis.push(1); // read block size
    cis.push(1); // write block size
    cis.push(1); // partition subdivision
    cis.push(1); // interleave

    // CISTPL_VERS_1: "Emu198x", "SRAM Card".
    cis.push(CISTPL_VERS_1);
    let s1 = b"Emu198x";
    let s2 = b"SRAM Card";
    let ver_len = 2 + s1.len() + 1 + s2.len() + 1 + 1; // major, minor, str1\0, str2\0, \xFF
    cis.push(ver_len as u8);
    cis.push(0x04); // major version 4
    cis.push(0x01); // minor version 1
    cis.extend_from_slice(s1);
    cis.push(0x00);
    cis.extend_from_slice(s2);
    cis.push(0x00);
    cis.push(0xFF);

    // CISTPL_FUNCID: Memory Card (ID=1).
    cis.push(CISTPL_FUNCID);
    cis.push(2);
    cis.push(0x01); // Memory
    cis.push(0x00); // system init

    // CISTPL_MANFID: generic.
    cis.push(CISTPL_MANFID);
    cis.push(4);
    cis.push(0xFF);
    cis.push(0xFF);
    cis.push(0x00);
    cis.push(0x00);

    // CISTPL_END.
    cis.push(CISTPL_END);

    cis
}

/// Build CIS tuples for a CompactFlash card.
fn build_cf_cis() -> Vec<u8> {
    let mut cis = Vec::with_capacity(96);

    // CISTPL_DEVICE: Fixed disk.
    cis.push(CISTPL_DEVICE);
    cis.push(2);
    cis.push(0xD6); // Fixed, 100ns (type=0xD0 | speed=0x06)
    cis.push(0x00); // No size info

    // CISTPL_VERS_1: "Emu198x", "CompactFlash".
    cis.push(CISTPL_VERS_1);
    let s1 = b"Emu198x";
    let s2 = b"CompactFlash";
    let ver_len = 2 + s1.len() + 1 + s2.len() + 1 + 1;
    cis.push(ver_len as u8);
    cis.push(0x04);
    cis.push(0x01);
    cis.extend_from_slice(s1);
    cis.push(0x00);
    cis.extend_from_slice(s2);
    cis.push(0x00);
    cis.push(0xFF);

    // CISTPL_FUNCID: Fixed Disk (ID=4).
    cis.push(CISTPL_FUNCID);
    cis.push(2);
    cis.push(0x04); // Fixed disk
    cis.push(0x00);

    // CISTPL_CONFIG: config register at attribute address $200.
    cis.push(CISTPL_CONFIG);
    cis.push(5);
    cis.push(0x01); // Field sizes: 1 byte TPCC_RASZ, 1 byte TPCC_RMSZ
    cis.push(0x01); // Last index
    cis.push(0x00); // Config register base lo
    cis.push(0x02); // Config register base hi ($200)
    cis.push(0x03); // Register present mask

    // CISTPL_CFTABLE_ENTRY: I/O at $1F0 (8 regs) + $3F6 (1 reg).
    cis.push(CISTPL_CFTABLE_ENTRY);
    cis.push(10);
    cis.push(0xC0 | 0x01); // Default + index 1
    cis.push(0x01); // feature selection: I/O space
    cis.push(0x08 | 0x01); // I/O: 8-bit, range present
    cis.push(0x48); // 2 ranges, 16-bit address, 8-bit size
    cis.push(0xF0); // Range 1 addr lo
    cis.push(0x01); // Range 1 addr hi ($1F0)
    cis.push(0x07); // Range 1 size - 1 (8 bytes)
    cis.push(0xF6); // Range 2 addr lo
    cis.push(0x03); // Range 2 addr hi ($3F6)
    cis.push(0x00); // Range 2 size - 1 (1 byte)

    // CISTPL_MANFID.
    cis.push(CISTPL_MANFID);
    cis.push(4);
    cis.push(0xFF);
    cis.push(0xFF);
    cis.push(0x00);
    cis.push(0x00);

    // CISTPL_END.
    cis.push(CISTPL_END);

    cis
}

/// NE2000 CIS data — matches WinUAE's `ne2000pcmcia[]` (CNET CN40BC card).
///
/// Config register at attribute address $3F8.
fn build_ne2000_cis() -> Vec<u8> {
    vec![
        // CISTPL_DEVICE
        0x01, 0x03, 0x00, 0x00, 0xFF,
        // CISTPL_VERS_1
        0x15, 0x21,
        0x04, 0x01,  // Version 4.1
        b'C', b'N', b'E', b'T', b' ', b'T', b'e', b'c', b'h', b'n', b'o', b'l',
        b'o', b'g', b'y', b',', b' ', b'I', b'n', b'c', b'.', 0x00,
        b'C', b'N', b'4', b'0', b'B', b'C', 0x00,
        0xFF,
        // CISTPL_CONFIG
        0x1A, 0x05,
        0x01,       // Field sizes
        0x20,       // Last index
        0xF8, 0x03, // Config reg base = $3F8
        0x03,       // Register present mask
        // CISTPL_CFTABLE_ENTRY: default config
        0x1B, 0x08,
        0xC0 | 0x20, // Default + index 0x20
        0x08,        // Feature: IRQ
        0x08 | 0x01, // I/O 8-bit, range
        0x20,        // 1 range, 16-bit addr, 8-bit size
        0x00, 0x03,  // I/O base $0300
        0x1F,        // Size: 32 bytes
        0x20,        // IRQ: level-mode, mask follows
        // CISTPL_FUNCID: Network Adapter (ID=6)
        0x21, 0x02, 0x06, 0x00,
        // CISTPL_MANFID
        0x20, 0x04, 0x01, 0x49, 0x00, 0x00,
        // CISTPL_END
        0xFF,
    ]
}

// ---------------------------------------------------------------------------
// Gayle
// ---------------------------------------------------------------------------

/// Gayle gate array state.
///
/// Handles IDE task-file registers ($DA0000+), four Gayle control
/// registers ($DA8000-$DABFFF), and the PCMCIA slot (common, attribute,
/// and I/O memory regions routed via Gary chip selects).
#[derive(Debug, Clone)]
pub struct Gayle {
    /// Card Status register ($DA8000).
    gayle_cs: u8,
    /// Interrupt Request register ($DA9000). Bits 2-7 are write-to-clear;
    /// bits 0-1 (RESET/BERR) are written directly.
    gayle_irq: u8,
    /// Interrupt Enable register ($DAA000).
    gayle_int: u8,
    /// Configuration register ($DAB000). Only low 4 bits are significant.
    gayle_cfg: u8,
    /// Attached IDE drive (None = no drive).
    drive: Option<IdeDrive>,
    /// PCMCIA card slot (None = empty).
    pcmcia_card: Option<PcmciaCard>,
}

impl Gayle {
    /// Create a new Gayle with no IDE drive and no PCMCIA card.
    #[must_use]
    pub fn new() -> Self {
        Self {
            gayle_cs: 0,
            gayle_irq: 0,
            gayle_int: 0,
            gayle_cfg: 0,
            drive: None,
            pcmcia_card: None,
        }
    }

    /// Create a Gayle with an IDE disk attached.
    #[must_use]
    pub fn with_disk(image: Vec<u8>, geometry: DiskGeometry) -> Self {
        Self {
            gayle_cs: 0,
            gayle_irq: 0,
            gayle_int: 0,
            gayle_cfg: 0,
            drive: Some(IdeDrive::new(image, geometry)),
            pcmcia_card: None,
        }
    }

    /// Create a Gayle with a PCMCIA SRAM card inserted.
    #[must_use]
    pub fn with_pcmcia_sram(image: Vec<u8>, readonly: bool) -> Self {
        let cis = build_sram_cis(image.len());
        let mut cs = GAYLE_CS_CCDET;
        if !readonly {
            cs |= GAYLE_CS_WR;
        }
        Self {
            gayle_cs: cs,
            gayle_irq: 0,
            gayle_int: 0,
            gayle_cfg: 0x08, // 100ns access time
            drive: None,
            pcmcia_card: Some(PcmciaCard::Sram {
                common: image,
                cis,
                readonly,
            }),
        }
    }

    /// Create a Gayle with a PCMCIA CompactFlash card inserted.
    #[must_use]
    pub fn with_pcmcia_cf(image: Vec<u8>, geometry: DiskGeometry) -> Self {
        let cis = build_cf_cis();
        Self {
            gayle_cs: GAYLE_CS_CCDET | GAYLE_CS_WR,
            gayle_irq: 0,
            gayle_int: 0,
            gayle_cfg: 0x08,
            drive: None,
            pcmcia_card: Some(PcmciaCard::CompactFlash {
                drive: IdeDrive::new(image, geometry),
                cis,
                configured: -1,
            }),
        }
    }

    /// Create a Gayle with a PCMCIA NE2000 Ethernet card inserted.
    #[must_use]
    pub fn with_pcmcia_ne2000(mac: [u8; 6]) -> Self {
        let cis = build_ne2000_cis();
        Self {
            gayle_cs: GAYLE_CS_CCDET,
            gayle_irq: 0,
            gayle_int: 0,
            gayle_cfg: 0x08,
            drive: None,
            pcmcia_card: Some(PcmciaCard::Ne2000 {
                nic: Ne2000State::new(mac),
                cis,
                configured: -1,
            }),
        }
    }

    /// Attach a disk image to the IDE controller.
    pub fn attach_disk(&mut self, image: Vec<u8>, geometry: DiskGeometry) {
        self.drive = Some(IdeDrive::new(image, geometry));
    }

    /// Reset all registers to power-on defaults.
    pub fn reset(&mut self) {
        // Preserve PCMCIA card-detect and write-protect bits through reset.
        let preserved_cs = self.gayle_cs & (GAYLE_CS_CCDET | GAYLE_CS_WR);
        self.gayle_cs = preserved_cs;
        self.gayle_irq = 0;
        self.gayle_int = 0;
        self.gayle_cfg = if self.pcmcia_card.is_some() {
            0x08
        } else {
            0
        };
        if let Some(drive) = &mut self.drive {
            drive.reset();
        }
        match &mut self.pcmcia_card {
            Some(PcmciaCard::CompactFlash { drive, configured, .. }) => {
                drive.reset();
                *configured = -1;
            }
            Some(PcmciaCard::Ne2000 { nic, configured, .. }) => {
                nic.reset();
                *configured = -1;
            }
            _ => {}
        }
    }

    // -- Accessors ----------------------------------------------------------

    /// Current card-status register value.
    #[must_use]
    pub const fn cs(&self) -> u8 {
        self.gayle_cs
    }

    /// Current interrupt-request register value.
    #[must_use]
    pub const fn irq(&self) -> u8 {
        self.gayle_irq
    }

    /// Current interrupt-enable register value.
    #[must_use]
    pub const fn int_enable(&self) -> u8 {
        self.gayle_int
    }

    /// Current configuration register value.
    #[must_use]
    pub const fn cfg(&self) -> u8 {
        self.gayle_cfg
    }

    /// Current IDE status register value.
    #[must_use]
    pub fn ide_status(&self) -> u8 {
        self.drive.as_ref().map_or(0x7F, |d| d.status)
    }

    /// True when an IDE drive is attached.
    #[must_use]
    pub fn drive_present(&self) -> bool {
        self.drive.is_some()
    }

    /// True when a PCMCIA card is inserted.
    #[must_use]
    pub fn pcmcia_present(&self) -> bool {
        self.pcmcia_card.is_some()
    }

    /// True when a PCMCIA card interrupt is pending and enabled.
    ///
    /// NE2000 ISR & IMR → gayle_cs BSY bit → gayle_int bit 2 check.
    #[must_use]
    pub fn pcmcia_irq_pending(&self) -> bool {
        match &self.pcmcia_card {
            Some(PcmciaCard::Ne2000 { nic, configured, .. }) => {
                *configured >= 0 && nic.irq_pending() && (self.gayle_int & GAYLE_CS_BSY) != 0
            }
            Some(PcmciaCard::CompactFlash {
                drive, configured, ..
            }) => {
                *configured >= 0
                    && drive.irq_pending
                    && !drive.nien
                    && (self.gayle_int & GAYLE_CS_BSY) != 0
            }
            _ => false,
        }
    }

    /// Access the NE2000 state for packet I/O (push_rx_packet / pop_tx_packet).
    #[must_use]
    pub fn ne2000(&self) -> Option<&Ne2000State> {
        match &self.pcmcia_card {
            Some(PcmciaCard::Ne2000 { nic, .. }) => Some(nic),
            _ => None,
        }
    }

    /// Mutable access to the NE2000 state for packet I/O.
    pub fn ne2000_mut(&mut self) -> Option<&mut Ne2000State> {
        match &mut self.pcmcia_card {
            Some(PcmciaCard::Ne2000 { nic, .. }) => Some(nic),
            _ => None,
        }
    }

    // -- PCMCIA bus I/O -----------------------------------------------------

    /// Read a byte from PCMCIA common memory ($600000-$9FFFFF).
    #[must_use]
    pub fn read_pcmcia_common(&self, addr: u32) -> u8 {
        if self.gayle_cs & GAYLE_CS_DIS != 0 {
            return 0;
        }
        let offset = (addr - 0x60_0000) as usize;
        match &self.pcmcia_card {
            Some(PcmciaCard::Sram { common, .. }) => {
                if offset < common.len() {
                    common[offset]
                } else {
                    0
                }
            }
            // CF and NE2000 don't use common memory.
            _ => 0,
        }
    }

    /// Write a byte to PCMCIA common memory ($600000-$9FFFFF).
    pub fn write_pcmcia_common(&mut self, addr: u32, val: u8) {
        if self.gayle_cs & GAYLE_CS_DIS != 0 {
            return;
        }
        let offset = (addr - 0x60_0000) as usize;
        match &mut self.pcmcia_card {
            Some(PcmciaCard::Sram {
                common, readonly, ..
            }) => {
                if !*readonly && offset < common.len() {
                    common[offset] = val;
                }
            }
            _ => {}
        }
    }

    /// Read a byte from PCMCIA attribute/IO/reset space ($A00000-$A5FFFF).
    ///
    /// Sub-ranges:
    /// - $A00000-$A1FFFF: Attribute memory (CIS tuples, config register)
    /// - $A20000-$A3FFFF: I/O space (CF ATA, NE2000 registers)
    /// - $A40000-$A5FFFF: Card reset
    #[must_use]
    pub fn read_pcmcia_attr(&mut self, addr: u32) -> u8 {
        if self.gayle_cs & GAYLE_CS_DIS != 0 {
            return 0;
        }
        let local = addr - 0xA0_0000;
        if local < 0x2_0000 {
            // Attribute memory: even-byte addressing (addr / 2 = CIS index).
            self.read_pcmcia_attribute(local)
        } else if local < 0x4_0000 {
            // I/O space.
            self.read_pcmcia_io(local - 0x2_0000)
        } else {
            // Reset space — read returns 0.
            0
        }
    }

    /// Write a byte to PCMCIA attribute/IO/reset space ($A00000-$A5FFFF).
    pub fn write_pcmcia_attr(&mut self, addr: u32, val: u8) {
        if self.gayle_cs & GAYLE_CS_DIS != 0 {
            return;
        }
        let local = addr - 0xA0_0000;
        if local < 0x2_0000 {
            self.write_pcmcia_attribute(local, val);
        } else if local < 0x4_0000 {
            self.write_pcmcia_io(local - 0x2_0000, val);
        } else {
            // Reset space — trigger card reset on write.
            self.reset_pcmcia_card();
        }
    }

    /// Read a 16-bit word from PCMCIA attribute/IO space.
    ///
    /// Word accesses to NE2000 I/O registers use byte-swapped 16-bit mode
    /// (big-endian Amiga bus ↔ little-endian DP8390).
    #[must_use]
    pub fn read_pcmcia_attr_word(&mut self, addr: u32) -> u16 {
        if self.gayle_cs & GAYLE_CS_DIS != 0 {
            return 0;
        }
        let local = addr - 0xA0_0000;
        if local >= 0x2_0000 && local < 0x4_0000 {
            // I/O space word read — NE2000 data port needs 16-bit access.
            let io_offset = local - 0x2_0000;
            return self.read_pcmcia_io_word(io_offset);
        }
        // Attribute and reset: byte access only.
        u16::from(self.read_pcmcia_attr(addr))
    }

    /// Write a 16-bit word to PCMCIA attribute/IO space.
    pub fn write_pcmcia_attr_word(&mut self, addr: u32, val: u16) {
        if self.gayle_cs & GAYLE_CS_DIS != 0 {
            return;
        }
        let local = addr - 0xA0_0000;
        if local >= 0x2_0000 && local < 0x4_0000 {
            let io_offset = local - 0x2_0000;
            self.write_pcmcia_io_word(io_offset, val);
            return;
        }
        // Attribute and reset: byte only.
        self.write_pcmcia_attr(addr, val as u8);
    }

    // -- PCMCIA internal routing --------------------------------------------

    fn read_pcmcia_attribute(&self, offset: u32) -> u8 {
        // Even-byte addressing: CIS index = offset / 2.
        let cis_idx = (offset / 2) as usize;

        match &self.pcmcia_card {
            Some(PcmciaCard::Sram { cis, .. }) => {
                // Config register at $200 (CIS index $100).
                if cis_idx == 0x100 {
                    return 0; // No configurable I/O for SRAM.
                }
                cis.get(cis_idx).copied().unwrap_or(0)
            }
            Some(PcmciaCard::CompactFlash {
                cis, configured, ..
            }) => {
                // Config register at attribute address $200 (CIS index $100).
                if cis_idx == 0x100 {
                    return if *configured >= 0 {
                        *configured as u8
                    } else {
                        0
                    };
                }
                cis.get(cis_idx).copied().unwrap_or(0)
            }
            Some(PcmciaCard::Ne2000 {
                cis, configured, ..
            }) => {
                // Config register at attribute address $3F8 (CIS index $1FC).
                if cis_idx == 0x1FC {
                    return if *configured >= 0 {
                        *configured as u8
                    } else {
                        0
                    };
                }
                cis.get(cis_idx).copied().unwrap_or(0)
            }
            None => 0,
        }
    }

    fn write_pcmcia_attribute(&mut self, offset: u32, val: u8) {
        let cis_idx = (offset / 2) as usize;

        match &mut self.pcmcia_card {
            Some(PcmciaCard::CompactFlash { configured, .. }) => {
                if cis_idx == 0x100 {
                    *configured = val as i8;
                }
            }
            Some(PcmciaCard::Ne2000 { configured, .. }) => {
                if cis_idx == 0x1FC {
                    *configured = val as i8;
                }
            }
            _ => {}
        }
    }

    fn read_pcmcia_io(&mut self, io_offset: u32) -> u8 {
        match &mut self.pcmcia_card {
            Some(PcmciaCard::CompactFlash {
                drive, configured, ..
            }) => {
                if *configured < 0 {
                    return 0xFF;
                }
                // CF ATA at I/O $1F0-$1F7, alternate status at $3F6.
                match io_offset {
                    0x1F0..=0x1F7 => {
                        let reg = io_offset - 0x1F0;
                        if reg == 0 {
                            0 // DATA byte — use word access
                        } else {
                            match reg {
                                1 => drive.error,
                                2 => drive.sector_count,
                                3 => drive.sector_number,
                                4 => drive.cylinder_lo,
                                5 => drive.cylinder_hi,
                                6 => drive.dev_head,
                                7 => drive.status,
                                _ => 0xFF,
                            }
                        }
                    }
                    0x3F6 => {
                        // Alternate status (same as status, no IRQ clear).
                        drive.status
                    }
                    _ => 0xFF,
                }
            }
            Some(PcmciaCard::Ne2000 {
                nic, configured, ..
            }) => {
                if *configured < 0 {
                    return 0xFF;
                }
                // NE2000 at I/O $300-$31F.
                if io_offset >= 0x300 && io_offset < 0x320 {
                    let reg = (io_offset - 0x300) as u8;
                    nic.read_reg(reg)
                } else {
                    0xFF
                }
            }
            _ => 0,
        }
    }

    fn write_pcmcia_io(&mut self, io_offset: u32, val: u8) {
        let mut set_bsy = false;
        match &mut self.pcmcia_card {
            Some(PcmciaCard::CompactFlash {
                drive, configured, ..
            }) => {
                if *configured < 0 {
                    return;
                }
                match io_offset {
                    0x1F0..=0x1F7 => {
                        let reg = (io_offset - 0x1F0) as u8;
                        if reg != 0 {
                            Self::write_cf_register_on(drive, reg, val);
                        }
                    }
                    0x3F6 => {
                        drive.nien = val & 0x02 != 0;
                    }
                    _ => {}
                }
                set_bsy = drive.irq_pending && !drive.nien;
            }
            Some(PcmciaCard::Ne2000 {
                nic, configured, ..
            }) => {
                if *configured < 0 {
                    return;
                }
                if io_offset >= 0x300 && io_offset < 0x320 {
                    let reg = (io_offset - 0x300) as u8;
                    nic.write_reg(reg, val);
                    set_bsy = nic.irq_pending();
                }
            }
            _ => {}
        }
        if set_bsy {
            self.gayle_cs |= GAYLE_CS_BSY;
        }
    }

    fn read_pcmcia_io_word(&mut self, io_offset: u32) -> u16 {
        match &mut self.pcmcia_card {
            Some(PcmciaCard::CompactFlash {
                drive, configured, ..
            }) => {
                if *configured < 0 {
                    return 0xFFFF;
                }
                if io_offset == 0x1F0 {
                    drive.read_data_word()
                } else {
                    // Fall back to byte read for non-data registers.
                    match io_offset {
                        0x1F1..=0x1F7 => {
                            let reg = io_offset - 0x1F0;
                            u16::from(match reg {
                                1 => drive.error,
                                2 => drive.sector_count,
                                3 => drive.sector_number,
                                4 => drive.cylinder_lo,
                                5 => drive.cylinder_hi,
                                6 => drive.dev_head,
                                7 => drive.status,
                                _ => 0xFF,
                            })
                        }
                        0x3F6 => u16::from(drive.status),
                        _ => 0xFFFF,
                    }
                }
            }
            Some(PcmciaCard::Ne2000 {
                nic, configured, ..
            }) => {
                if *configured < 0 {
                    return 0xFFFF;
                }
                // NE2000 data port at $310 (offset 0x10 from base $300).
                if io_offset == 0x310 {
                    // Byte-swap: NE2000 is little-endian, Amiga is big-endian.
                    let val = nic.read_data_port();
                    (val >> 8) | (val << 8)
                } else if io_offset >= 0x300 && io_offset < 0x320 {
                    let reg = (io_offset - 0x300) as u8;
                    u16::from(nic.read_reg(reg))
                } else {
                    0xFFFF
                }
            }
            _ => 0,
        }
    }

    fn write_pcmcia_io_word(&mut self, io_offset: u32, val: u16) {
        let mut set_bsy = false;
        match &mut self.pcmcia_card {
            Some(PcmciaCard::CompactFlash {
                drive, configured, ..
            }) => {
                if *configured < 0 {
                    return;
                }
                if io_offset == 0x1F0 {
                    drive.write_data_word(val);
                    set_bsy = drive.irq_pending && !drive.nien;
                } else {
                    let reg = if io_offset >= 0x1F0 && io_offset <= 0x1F7 {
                        Some((io_offset - 0x1F0) as u8)
                    } else {
                        None
                    };
                    if let Some(reg) = reg {
                        if reg != 0 {
                            Self::write_cf_register_on(drive, reg, val as u8);
                            set_bsy = drive.irq_pending && !drive.nien;
                        }
                    } else if io_offset == 0x3F6 {
                        drive.nien = val as u8 & 0x02 != 0;
                    }
                }
            }
            Some(PcmciaCard::Ne2000 {
                nic, configured, ..
            }) => {
                if *configured < 0 {
                    return;
                }
                // NE2000 data port at $310.
                if io_offset == 0x310 {
                    // Byte-swap for NE2000 little-endian.
                    let swapped = (val >> 8) | (val << 8);
                    nic.write_data_port(swapped);
                    set_bsy = nic.irq_pending();
                } else if io_offset >= 0x300 && io_offset < 0x320 {
                    let reg = (io_offset - 0x300) as u8;
                    nic.write_reg(reg, val as u8);
                    set_bsy = nic.irq_pending();
                }
            }
            _ => {}
        }
        if set_bsy {
            self.gayle_cs |= GAYLE_CS_BSY;
        }
    }

    /// CF register write helper (takes direct drive reference to avoid borrow issues).
    fn write_cf_register_on(drive: &mut IdeDrive, reg: u8, val: u8) {
        match reg {
            1 => drive.error = val,
            2 => drive.sector_count = val,
            3 => drive.sector_number = val,
            4 => drive.cylinder_lo = val,
            5 => drive.cylinder_hi = val,
            6 => drive.dev_head = val,
            7 => {
                drive.irq_pending = false;
                drive.execute_command(val);
            }
            _ => {}
        }
    }

    fn reset_pcmcia_card(&mut self) {
        match &mut self.pcmcia_card {
            Some(PcmciaCard::CompactFlash { drive, configured, .. }) => {
                drive.reset();
                *configured = -1;
            }
            Some(PcmciaCard::Ne2000 { nic, configured, .. }) => {
                nic.reset();
                *configured = -1;
            }
            _ => {}
        }
    }

    // -- Gayle register / IDE Bus I/O ---------------------------------------

    /// Read a byte from a Gayle-decoded address.
    ///
    /// The caller should only invoke this for addresses in $D80000-$DFFFFF.
    /// Addresses that don't match the Gayle filter return 0.
    #[must_use]
    pub fn read(&self, addr: u32) -> u8 {
        let local = addr & 0x0F_FFFF;

        // A1200 address filter: only respond when bits 17 and 19 are both set.
        if local & 0xA_0000 != 0xA_0000 {
            return 0;
        }

        // Gayle control registers ($DA8000-$DABFFF): bit 15 set.
        if local & 0x8000 != 0 {
            return match (local >> 12) & 0x03 {
                0 => self.gayle_cs,
                1 => self.gayle_irq,
                2 => self.gayle_int,
                3 => self.gayle_cfg & 0x0F,
                _ => unreachable!(),
            };
        }

        // IDE task-file registers ($DA0000-$DA3FFF).
        self.read_ide_byte(local)
    }

    /// Read a 16-bit word from a Gayle-decoded address.
    /// Used for the IDE DATA register which transfers 16 bits at a time.
    #[must_use]
    pub fn read_word(&mut self, addr: u32) -> u16 {
        let local = addr & 0x0F_FFFF;
        if local & 0xA_0000 != 0xA_0000 {
            return 0;
        }
        if local & 0x8000 != 0 {
            return u16::from(self.read(addr));
        }
        let reg = ide_reg_index(local);
        if reg == 0 && let Some(drive) = &mut self.drive {
            return drive.read_data_word();
        }
        u16::from(self.read_ide_byte(local))
    }

    /// Write a byte to a Gayle-decoded address.
    ///
    /// The caller should only invoke this for addresses in $D80000-$DFFFFF.
    pub fn write(&mut self, addr: u32, val: u8) {
        let local = addr & 0x0F_FFFF;

        // A1200 address filter.
        if local & 0xA_0000 != 0xA_0000 {
            return;
        }

        // Gayle control registers.
        if local & 0x8000 != 0 {
            match (local >> 12) & 0x03 {
                0 => self.gayle_cs = val,
                1 => {
                    // Bits 2-7: writing 0 clears the corresponding flag.
                    // Bits 0-1 (RESET/BERR): written directly.
                    self.gayle_irq = (self.gayle_irq & val) | (val & 0x03);
                }
                2 => self.gayle_int = val,
                3 => self.gayle_cfg = val & 0x0F,
                _ => unreachable!(),
            }
            return;
        }

        // IDE task-file writes.
        self.write_ide_byte(local, val);
    }

    /// Write a 16-bit word to a Gayle-decoded address.
    /// Used for the IDE DATA register.
    pub fn write_word(&mut self, addr: u32, val: u16) {
        let local = addr & 0x0F_FFFF;
        if local & 0xA_0000 != 0xA_0000 {
            return;
        }
        if local & 0x8000 != 0 {
            self.write(addr, val as u8);
            return;
        }
        let reg = ide_reg_index(local);
        if reg == 0 {
            if let Some(drive) = &mut self.drive {
                drive.write_data_word(val);
            }
            return;
        }
        self.write_ide_byte(local, val as u8);
    }

    /// True when the IDE interrupt line is asserted and enabled.
    #[must_use]
    pub fn ide_irq_pending(&self) -> bool {
        let hw_irq = self
            .drive
            .as_ref()
            .is_some_and(|d| d.irq_pending && !d.nien);
        if hw_irq {
            // The IRQ line raises bit 7 (IDE_IRQ) in the Gayle IRQ register.
            (self.gayle_int & GAYLE_CS_IDE) != 0
        } else {
            false
        }
    }

    // -- IDE register I/O ---------------------------------------------------

    fn read_ide_byte(&self, local: u32) -> u8 {
        let reg = ide_reg_index(local);

        let Some(drive) = &self.drive else {
            // No drive: STATUS = $7F, all others = $FF.
            return if reg == 7 { 0x7F } else { 0xFF };
        };

        match reg {
            0 => 0, // DATA byte read — use read_word for 16-bit access
            1 => drive.error,
            2 => drive.sector_count,
            3 => drive.sector_number,
            4 => drive.cylinder_lo,
            5 => drive.cylinder_hi,
            6 => drive.dev_head,
            7 => {
                // Reading STATUS clears the IRQ.
                // (We can't clear it here because self is &self, but
                // the caller should handle IRQ clearing.)
                drive.status
            }
            _ => 0xFF,
        }
    }

    fn write_ide_byte(&mut self, local: u32, val: u8) {
        let reg = ide_reg_index(local);

        let Some(drive) = &mut self.drive else {
            return;
        };

        match reg {
            0 => {} // DATA byte write — use write_word for 16-bit access
            1 => drive.error = val, // FEATURES on write, ERROR on read
            2 => drive.sector_count = val,
            3 => drive.sector_number = val,
            4 => drive.cylinder_lo = val,
            5 => drive.cylinder_hi = val,
            6 => drive.dev_head = val,
            7 => {
                // COMMAND register: execute the command.
                drive.irq_pending = false;
                drive.execute_command(val);
                // Raise Gayle IRQ bit if the drive signalled.
                if drive.irq_pending {
                    self.gayle_irq |= 0x80;
                }
            }
            _ => {}
        }
    }
}

/// Decode an IDE task-file register index from a Gayle local address.
/// Strips bits 13 and 5, then shifts right 2 to get register index 0-7.
fn ide_reg_index(local: u32) -> u32 {
    let stripped = local & !0x2020;
    (stripped >> 2) & 0x07
}

impl Default for Gayle {
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

    fn test_disk(sectors: usize) -> (Vec<u8>, DiskGeometry) {
        let size = sectors * 512;
        let image = vec![0u8; size];
        let geometry = DiskGeometry::from_image_size(size);
        (image, geometry)
    }

    // -- No-drive behaviour (unchanged from stub) ---------------------------

    #[test]
    fn no_drive_status_returns_7f() {
        let g = Gayle::new();
        assert_eq!(g.read(0xDA_001C), 0x7F);
    }

    #[test]
    fn no_drive_other_returns_ff() {
        let g = Gayle::new();
        assert_eq!(g.read(0xDA_0004), 0xFF);
    }

    #[test]
    fn gayle_cs_roundtrip() {
        let mut g = Gayle::new();
        g.write(0xDA_8000, 0xA5);
        assert_eq!(g.read(0xDA_8000), 0xA5);
    }

    #[test]
    fn gayle_irq_write_to_clear() {
        let mut g = Gayle::new();
        g.gayle_irq = 0xFC;
        g.write(0xDA_9000, 0x0C);
        assert_eq!(g.read(0xDA_9000), 0x0C);
    }

    #[test]
    fn gayle_cfg_4_bits() {
        let mut g = Gayle::new();
        g.write(0xDA_B000, 0xFF);
        assert_eq!(g.read(0xDA_B000), 0x0F);
    }

    #[test]
    fn ide_address_decode() {
        let g = Gayle::new();
        assert_eq!(g.read(0xDA_0000), 0xFF);
        assert_eq!(g.read(0xDA_001C), 0x7F);
        assert_eq!(g.read(0xDA_0018), 0xFF);
    }

    #[test]
    fn address_filter_rejects_low_range() {
        let g = Gayle::new();
        assert_eq!(g.read(0xD8_0000), 0);
        assert_eq!(g.read(0xD9_0000), 0);
    }

    #[test]
    fn reset_restores_power_on_defaults() {
        let mut g = Gayle::new();
        g.gayle_cs = 0xA5;
        g.gayle_irq = 0x80;
        g.gayle_int = 0x80;
        g.gayle_cfg = 0x0F;

        g.reset();

        assert_eq!(g.read(0xDA_8000), 0);
        assert_eq!(g.read(0xDA_9000), 0);
        assert_eq!(g.read(0xDA_A000), 0);
        assert_eq!(g.read(0xDA_B000), 0);
        assert_eq!(g.read(0xDA_001C), 0x7F);
        assert!(!g.ide_irq_pending());
    }

    #[test]
    fn ide_irq_pending_requires_enable_and_irq_flag() {
        let mut g = Gayle::new();
        g.gayle_irq = 0x80;
        assert!(!g.ide_irq_pending());

        g.write(0xDA_A000, 0x80);
        // Still false: no drive to assert IRQ.
        assert!(!g.ide_irq_pending());
    }

    #[test]
    fn irq_write_sets_low_control_bits_directly() {
        let mut g = Gayle::new();
        g.gayle_irq = 0xFC;
        g.write(0xDA_9000, 0x03);
        assert_eq!(g.read(0xDA_9000), 0x03);
    }

    #[test]
    fn ide_decode_ignores_bits_13_and_5() {
        let g = Gayle::new();
        assert_eq!(g.read(0xDA_001C), 0x7F);
        assert_eq!(g.read(0xDA_003C), 0x7F);
        assert_eq!(g.read(0xDA_201C), 0x7F);
        assert_eq!(g.read(0xDA_203C), 0x7F);
    }

    #[test]
    fn invalid_writes_do_not_modify_decoded_registers() {
        let mut g = Gayle::new();
        g.write(0xDA_8000, 0xA5);
        g.write(0xD8_8000, 0x5A);
        g.write(0xD9_001C, 0x00);
        assert_eq!(g.read(0xDA_8000), 0xA5);
        assert_eq!(g.read(0xDA_001C), 0x7F);
    }

    #[test]
    fn public_state_accessors_reflect_registers() {
        let mut g = Gayle::new();
        g.write(0xDA_8000, 0x11);
        g.write(0xDA_9000, 0x02);
        g.write(0xDA_A000, 0x80);
        g.write(0xDA_B000, 0x0E);

        assert_eq!(g.cs(), 0x11);
        assert_eq!(g.irq(), 0x02);
        assert_eq!(g.int_enable(), 0x80);
        assert_eq!(g.cfg(), 0x0E);
        assert_eq!(g.ide_status(), 0x7F);
        assert!(!g.drive_present());
    }

    // -- Drive present: task-file registers ---------------------------------

    #[test]
    fn drive_present_status_returns_drdy() {
        let (image, geom) = test_disk(1008); // 16 heads × 63 spt × 1 cyl
        let g = Gayle::with_disk(image, geom);
        assert!(g.drive_present());
        assert_eq!(g.ide_status(), STATUS_DRDY);
    }

    #[test]
    fn task_file_registers_roundtrip() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);

        // Write task-file registers (reg 1-6)
        g.write(0xDA_0004, 0x11); // reg 1: error/features
        g.write(0xDA_0008, 0x22); // reg 2: sector count
        g.write(0xDA_000C, 0x33); // reg 3: sector number
        g.write(0xDA_0010, 0x44); // reg 4: cylinder lo
        g.write(0xDA_0014, 0x55); // reg 5: cylinder hi
        g.write(0xDA_0018, 0x66); // reg 6: dev/head

        assert_eq!(g.read(0xDA_0004), 0x11);
        assert_eq!(g.read(0xDA_0008), 0x22);
        assert_eq!(g.read(0xDA_000C), 0x33);
        assert_eq!(g.read(0xDA_0010), 0x44);
        assert_eq!(g.read(0xDA_0014), 0x55);
        assert_eq!(g.read(0xDA_0018), 0x66);
    }

    // -- IDENTIFY DEVICE ($EC) ----------------------------------------------

    #[test]
    fn identify_device_returns_512_bytes() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);

        // Issue IDENTIFY DEVICE command
        g.write(0xDA_001C, CMD_IDENTIFY_DEVICE);

        // Status should have DRQ set
        assert_eq!(g.ide_status() & STATUS_DRQ, STATUS_DRQ);

        // Read 256 words (512 bytes) from DATA register
        let mut identify = vec![0u16; 256];
        for word in &mut identify {
            *word = g.read_word(0xDA_0000);
        }

        // After reading all data, DRQ should be clear
        assert_eq!(g.ide_status() & STATUS_DRQ, 0);

        // Check geometry fields
        assert_eq!(identify[1], geom.cylinders);
        assert_eq!(identify[3], u16::from(geom.heads));
        assert_eq!(identify[6], u16::from(geom.sectors_per_track));

        // Check LBA capability (word 49, bit 9)
        assert_eq!(identify[49] & 0x0200, 0x0200);
    }

    // -- READ SECTOR(S) ($20) -----------------------------------------------

    #[test]
    fn read_sector_returns_disk_data() {
        let (mut image, geom) = test_disk(1008);
        // Write a known pattern to sector 0
        for i in 0..512 {
            image[i] = (i & 0xFF) as u8;
        }
        let mut g = Gayle::with_disk(image, geom);

        // Set up for LBA sector 0
        g.write(0xDA_0018, 0xE0); // dev/head: LBA mode, head 0
        g.write(0xDA_0008, 0x01); // sector count: 1
        g.write(0xDA_000C, 0x00); // sector number (LBA 7:0)
        g.write(0xDA_0010, 0x00); // cylinder lo (LBA 15:8)
        g.write(0xDA_0014, 0x00); // cylinder hi (LBA 23:16)

        // Issue READ SECTORS
        g.write(0xDA_001C, CMD_READ_SECTORS);
        assert_eq!(g.ide_status() & STATUS_DRQ, STATUS_DRQ);

        // Read back the sector
        let mut sector = vec![0u16; 256];
        for word in &mut sector {
            *word = g.read_word(0xDA_0000);
        }

        // Verify the known pattern
        assert_eq!(sector[0], 0x0001); // bytes 0,1
        assert_eq!(sector[1], 0x0203); // bytes 2,3
    }

    // -- WRITE SECTOR(S) ($30) ----------------------------------------------

    #[test]
    fn write_sector_commits_to_image() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);

        // Set up for LBA sector 0
        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_0008, 0x01);
        g.write(0xDA_000C, 0x00);
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);

        // Issue WRITE SECTORS
        g.write(0xDA_001C, CMD_WRITE_SECTORS);
        assert_eq!(g.ide_status() & STATUS_DRQ, STATUS_DRQ);

        // Write 256 words of known data
        for i in 0..256u16 {
            g.write_word(0xDA_0000, i);
        }

        // After writing, DRQ should be clear
        assert_eq!(g.ide_status() & STATUS_DRQ, 0);

        // Read back to verify
        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_0008, 0x01);
        g.write(0xDA_000C, 0x00);
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_READ_SECTORS);

        let mut readback = vec![0u16; 256];
        for word in &mut readback {
            *word = g.read_word(0xDA_0000);
        }

        for (i, &word) in readback.iter().enumerate() {
            assert_eq!(word, i as u16, "mismatch at word {i}");
        }
    }

    // -- SET FEATURES ($EF) -------------------------------------------------

    #[test]
    fn set_features_accepted() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);
        g.write(0xDA_001C, CMD_SET_FEATURES);
        assert_eq!(g.ide_status(), STATUS_DRDY);
    }

    // -- EXECUTE DEVICE DIAGNOSTIC ($90) ------------------------------------

    #[test]
    fn execute_diagnostic_sets_no_error() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);
        g.write(0xDA_001C, CMD_EXECUTE_DIAGNOSTIC);
        assert_eq!(g.read(0xDA_0004), 0x01); // error = 01 = no error
        assert_eq!(g.ide_status(), STATUS_DRDY);
    }

    // -- Unknown command aborts ---------------------------------------------

    #[test]
    fn unknown_command_sets_abrt() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);
        g.write(0xDA_001C, 0xFF); // bogus command
        assert_eq!(g.ide_status() & STATUS_ERR, STATUS_ERR);
        assert_eq!(g.read(0xDA_0004) & ERROR_ABRT, ERROR_ABRT);
    }

    // -- CHS addressing -----------------------------------------------------

    #[test]
    fn read_sector_chs_mode() {
        let (mut image, geom) = test_disk(1008);
        // Write pattern to CHS sector (C=0, H=0, S=1) = LBA 0
        image[0] = 0xDE;
        image[1] = 0xAD;
        let mut g = Gayle::with_disk(image, geom);

        // CHS mode: bit 6 clear in dev/head
        g.write(0xDA_0018, 0xA0); // dev/head: CHS, head 0
        g.write(0xDA_0008, 0x01); // sector count
        g.write(0xDA_000C, 0x01); // sector number (1-based)
        g.write(0xDA_0010, 0x00); // cylinder lo
        g.write(0xDA_0014, 0x00); // cylinder hi

        g.write(0xDA_001C, CMD_READ_SECTORS);
        let word = g.read_word(0xDA_0000);
        assert_eq!(word, 0xDEAD);
    }

    // -- DiskGeometry -------------------------------------------------------

    #[test]
    fn geometry_from_image_size() {
        let g = DiskGeometry::from_image_size(20 * 1024 * 1024); // 20 MB
        assert_eq!(g.heads, 16);
        assert_eq!(g.sectors_per_track, 63);
        assert!(g.cylinders > 0);
        assert_eq!(g.total_sectors(), g.cylinders as u32 * 16 * 63);
    }

    // -- Attach disk after construction -------------------------------------

    #[test]
    fn attach_disk_enables_drive() {
        let mut g = Gayle::new();
        assert!(!g.drive_present());
        assert_eq!(g.ide_status(), 0x7F);

        let (image, geom) = test_disk(1008);
        g.attach_disk(image, geom);
        assert!(g.drive_present());
        assert_eq!(g.ide_status(), STATUS_DRDY);
    }

    // -- IRQ flow -----------------------------------------------------------

    #[test]
    fn identify_raises_gayle_irq_bit() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);

        // Enable IDE interrupt
        g.write(0xDA_A000, 0x80);

        // Issue IDENTIFY
        g.write(0xDA_001C, CMD_IDENTIFY_DEVICE);

        // Gayle IRQ register should have bit 7 set
        assert_eq!(g.irq() & 0x80, 0x80);
        assert!(g.ide_irq_pending());
    }

    // -- Multi-sector READ SECTORS ------------------------------------------

    #[test]
    fn read_multi_sector_reads_consecutive_sectors() {
        let (mut image, geom) = test_disk(1008);
        // Write distinct patterns to sectors 0, 1, 2
        for sector in 0..3usize {
            let base = sector * 512;
            for i in 0..512 {
                image[base + i] = (sector as u8).wrapping_add(i as u8);
            }
        }
        let mut g = Gayle::with_disk(image, geom);

        // LBA sector 0, count 3
        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_0008, 0x03); // 3 sectors
        g.write(0xDA_000C, 0x00);
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_READ_SECTORS);

        for sector in 0..3u8 {
            assert_eq!(
                g.ide_status() & STATUS_DRQ,
                STATUS_DRQ,
                "sector {sector}: DRQ not set"
            );
            let mut buf = vec![0u16; 256];
            for word in &mut buf {
                *word = g.read_word(0xDA_0000);
            }
            // Verify first word of each sector
            let expected_hi = sector.wrapping_add(0);
            let expected_lo = sector.wrapping_add(1);
            assert_eq!(
                buf[0],
                u16::from(expected_hi) << 8 | u16::from(expected_lo),
                "sector {sector}: data mismatch"
            );
        }
        // After all sectors, DRQ should be clear
        assert_eq!(g.ide_status() & STATUS_DRQ, 0);
    }

    // -- Multi-sector WRITE SECTORS -----------------------------------------

    #[test]
    fn write_multi_sector_commits_all() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);

        // LBA sector 0, write 3 sectors
        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_0008, 0x03);
        g.write(0xDA_000C, 0x00);
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_WRITE_SECTORS);

        for sector in 0..3u16 {
            assert_eq!(
                g.ide_status() & STATUS_DRQ,
                STATUS_DRQ,
                "sector {sector}: DRQ not set"
            );
            for word in 0..256u16 {
                g.write_word(0xDA_0000, sector * 256 + word);
            }
        }
        assert_eq!(g.ide_status() & STATUS_DRQ, 0);

        // Read back all 3 sectors and verify
        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_0008, 0x03);
        g.write(0xDA_000C, 0x00);
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_READ_SECTORS);

        for sector in 0..3u16 {
            for word in 0..256u16 {
                let val = g.read_word(0xDA_0000);
                assert_eq!(
                    val,
                    sector * 256 + word,
                    "sector {sector} word {word}: expected {}, got {val}",
                    sector * 256 + word
                );
            }
        }
    }

    // -- Sector count 0 = 256 -----------------------------------------------

    #[test]
    fn sector_count_zero_means_256() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);

        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_0008, 0x00); // 0 = 256 sectors
        g.write(0xDA_000C, 0x00);
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_READ_SECTORS);

        // Read 256 sectors (each 256 words)
        for _ in 0..256 {
            assert_eq!(g.ide_status() & STATUS_DRQ, STATUS_DRQ);
            for _ in 0..256 {
                let _ = g.read_word(0xDA_0000);
            }
        }
        assert_eq!(g.ide_status() & STATUS_DRQ, 0);
    }

    // -- SET MULTIPLE MODE ($C6) --------------------------------------------

    #[test]
    fn set_multiple_mode_accepts_valid_count() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);
        g.write(0xDA_0008, 0x04); // sector count = 4
        g.write(0xDA_001C, CMD_SET_MULTIPLE_MODE);
        assert_eq!(g.ide_status(), STATUS_DRDY);
    }

    #[test]
    fn set_multiple_mode_rejects_zero() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);
        g.write(0xDA_0008, 0x00); // 0 is invalid
        g.write(0xDA_001C, CMD_SET_MULTIPLE_MODE);
        assert_eq!(g.ide_status() & STATUS_ERR, STATUS_ERR);
    }

    #[test]
    fn set_multiple_mode_rejects_over_16() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);
        g.write(0xDA_0008, 32);
        g.write(0xDA_001C, CMD_SET_MULTIPLE_MODE);
        assert_eq!(g.ide_status() & STATUS_ERR, STATUS_ERR);
    }

    // -- READ MULTIPLE ($C4) ------------------------------------------------

    #[test]
    fn read_multiple_transfers_block() {
        let (mut image, geom) = test_disk(1008);
        for sector in 0..4usize {
            let base = sector * 512;
            for i in 0..512 {
                image[base + i] = sector as u8;
            }
        }
        let mut g = Gayle::with_disk(image, geom);

        // Set multiple mode to 2 sectors per interrupt
        g.write(0xDA_0008, 0x02);
        g.write(0xDA_001C, CMD_SET_MULTIPLE_MODE);
        assert_eq!(g.ide_status(), STATUS_DRDY);

        // Read 4 sectors using READ MULTIPLE — should be 2 blocks of 2
        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_0008, 0x04);
        g.write(0xDA_000C, 0x00);
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_READ_MULTIPLE);

        // First block: 2 sectors (512 words)
        assert_eq!(g.ide_status() & STATUS_DRQ, STATUS_DRQ);
        for i in 0..512u16 {
            let val = g.read_word(0xDA_0000);
            let expected_sector = if i < 256 { 0u8 } else { 1 };
            let expected = u16::from(expected_sector) << 8 | u16::from(expected_sector);
            assert_eq!(val, expected, "block 0 word {i}");
        }

        // Second block: 2 sectors (512 words)
        assert_eq!(g.ide_status() & STATUS_DRQ, STATUS_DRQ);
        for i in 0..512u16 {
            let val = g.read_word(0xDA_0000);
            let expected_sector = if i < 256 { 2u8 } else { 3 };
            let expected = u16::from(expected_sector) << 8 | u16::from(expected_sector);
            assert_eq!(val, expected, "block 1 word {i}");
        }

        assert_eq!(g.ide_status() & STATUS_DRQ, 0);
    }

    #[test]
    fn read_multiple_without_set_multiple_aborts() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);
        // Don't set multiple mode first
        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_0008, 0x01);
        g.write(0xDA_000C, 0x00);
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_READ_MULTIPLE);
        assert_eq!(g.ide_status() & STATUS_ERR, STATUS_ERR);
    }

    // -- WRITE MULTIPLE ($C5) -----------------------------------------------

    #[test]
    fn write_multiple_commits_block() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);

        // Set multiple mode to 2
        g.write(0xDA_0008, 0x02);
        g.write(0xDA_001C, CMD_SET_MULTIPLE_MODE);

        // Write 4 sectors using WRITE MULTIPLE
        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_0008, 0x04);
        g.write(0xDA_000C, 0x00);
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_WRITE_MULTIPLE);

        // First block: 2 sectors
        assert_eq!(g.ide_status() & STATUS_DRQ, STATUS_DRQ);
        for i in 0..512u16 {
            g.write_word(0xDA_0000, i);
        }
        // Second block: 2 sectors
        assert_eq!(g.ide_status() & STATUS_DRQ, STATUS_DRQ);
        for i in 0..512u16 {
            g.write_word(0xDA_0000, 512 + i);
        }
        assert_eq!(g.ide_status() & STATUS_DRQ, 0);

        // Read back with READ SECTORS and verify
        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_0008, 0x04);
        g.write(0xDA_000C, 0x00);
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_READ_SECTORS);
        for i in 0..1024u16 {
            let val = g.read_word(0xDA_0000);
            assert_eq!(val, i, "word {i}");
        }
    }

    // -- INITIALIZE DEVICE PARAMETERS ($91) ---------------------------------

    #[test]
    fn init_device_params_updates_geometry() {
        let (mut image, geom) = test_disk(1008);
        // Write distinct data to what would be CHS (C=0, H=1, S=1) with
        // original geometry (16 heads, 63 spt) = LBA 63
        let lba = 63usize;
        image[lba * 512] = 0xCA;
        image[lba * 512 + 1] = 0xFE;
        let mut g = Gayle::with_disk(image, geom);

        // Change logical geometry to 4 heads, 32 spt
        g.write(0xDA_0008, 32); // sectors per track
        g.write(0xDA_0018, 0xA3); // CHS mode, max head = 3 → 4 heads
        g.write(0xDA_001C, CMD_INIT_DEVICE_PARAMS);
        assert_eq!(g.ide_status(), STATUS_DRDY);

        // Now read CHS (C=0, H=1, S=1) with new geometry = LBA 32
        g.write(0xDA_0018, 0xA1); // head 1
        g.write(0xDA_0008, 0x01);
        g.write(0xDA_000C, 0x01); // sector 1 (1-based)
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_READ_SECTORS);
        let word = g.read_word(0xDA_0000);
        // With 4 heads × 32 spt, (C=0,H=1,S=1) → LBA = 0*4+1)*32 + 0 = 32
        // Should NOT be 0xCAFE (that's at LBA 63)
        assert_ne!(word, 0xCAFE);
    }

    // -- READ VERIFY ($40) --------------------------------------------------

    #[test]
    fn read_verify_succeeds_for_valid_sectors() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);
        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_0008, 0x04);
        g.write(0xDA_000C, 0x00);
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_READ_VERIFY);
        assert_eq!(g.ide_status(), STATUS_DRDY);
        // No DRQ — verify doesn't transfer data
        assert_eq!(g.ide_status() & STATUS_DRQ, 0);
    }

    // -- SEEK ($70) ---------------------------------------------------------

    #[test]
    fn seek_succeeds_for_valid_address() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);
        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_000C, 0x00);
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_SEEK);
        assert_eq!(g.ide_status(), STATUS_DRDY);
    }

    #[test]
    fn seek_fails_for_chs_sector_zero() {
        let (image, geom) = test_disk(1008);
        let mut g = Gayle::with_disk(image, geom);
        g.write(0xDA_0018, 0xA0); // CHS mode
        g.write(0xDA_000C, 0x00); // sector 0 is invalid in CHS
        g.write(0xDA_001C, CMD_SEEK);
        assert_eq!(g.ide_status() & STATUS_ERR, STATUS_ERR);
    }

    // -- LBA address advances correctly -------------------------------------

    #[test]
    fn lba_address_advances_across_sectors() {
        let (mut image, geom) = test_disk(1008);
        // Write sector number as first byte of each sector
        for s in 0..10usize {
            image[s * 512] = s as u8;
        }
        let mut g = Gayle::with_disk(image, geom);

        // Read 5 sectors starting from LBA 3
        g.write(0xDA_0018, 0xE0);
        g.write(0xDA_0008, 0x05);
        g.write(0xDA_000C, 0x03); // LBA 3
        g.write(0xDA_0010, 0x00);
        g.write(0xDA_0014, 0x00);
        g.write(0xDA_001C, CMD_READ_SECTORS);

        for expected in 3..8u8 {
            let first_word = g.read_word(0xDA_0000);
            assert_eq!(
                (first_word >> 8) as u8, expected,
                "sector starting at LBA {expected}"
            );
            // Skip remaining 255 words
            for _ in 1..256 {
                let _ = g.read_word(0xDA_0000);
            }
        }
    }
}
