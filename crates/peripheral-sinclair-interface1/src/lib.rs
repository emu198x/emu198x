//! Sinclair's ZX Interface 1 and up to eight ZX Microdrives.
//!
//! The Interface 1 adds an 8 KB shadow ROM, a Microdrive controller, an
//! RS-232 port and the ZX Net. Its ROM extends BASIC by catching the
//! errors the Spectrum ROM raises for the new syntax.
//!
//! The ROM is Sinclair's firmware, now owned by Amstrad. It is never
//! part of this repository: [`Interface1::attach`] takes the image from
//! the user.
//!
//! # Shadow ROM paging
//!
//! The interface pages its ROM over the bottom 16 KB, mirrored at
//! `$0000` and `$2000`, when the CPU fetches an opcode from `$0008` (the
//! `RST 8` error restart) or `$1708` (`CLOSE #`). The fetch itself
//! already comes from the shadow ROM. It pages out after the fetch from
//! `$0700`, so that opcode still comes from the shadow ROM and the next
//! comes from the Spectrum's.
//!
//! # Ports
//!
//! The interface decodes A3 and A4 only.
//!
//! | Port | A4 A3 | Read | Write |
//! |---|---|---|---|
//! | `$E7` | 0 0 | Microdrive data | Microdrive data |
//! | `$EF` | 0 1 | status: bit 0 write protect, 1 sync, 2 gap, 3 DTR, 4 busy, all active low | control: bit 0 comms data, 1 comms clock, 2 read/write, 3 erase, 4 CTS, 5 wait |
//! | `$F7` | 1 0 | bit 0 ZX Net, bit 7 RS-232 TxD | bit 0 ZX Net / RS-232 RxD |
//!
//! Drives are selected by shifting a bit along the daisy chain: each
//! falling edge of the comms clock moves every drive's motor state one
//! drive along and sets drive 1 from the inverted comms data bit.
//!
//! # The Microdrive model
//!
//! The tape loop is modelled as Fuse models it, by position rather than
//! by time. Each data access moves the head one byte round the loop,
//! which wraps at the end of the cartridge. Each status read counts
//! down the gap and then the sync before a block, and any other access
//! moves the head to the start of the next header or data block. A
//! header block carries 15 bytes and a data block 528, after the
//! 12-byte preamble the ROM writes and the drive synthesises on read.
//! The real tape runs at a fixed speed, so a program that times its
//! polling would see a difference; the Interface 1 ROM does not.
//!
//! RS-232 and the ZX Net are modelled at register level only: the
//! control bits are kept, the output line is latched, and the input
//! lines read whatever the host last set, idle by default.
//!
//! # Sources
//!
//! Fuse's `peripherals/if1.c` and `z80/z80_ops.c` (GPL-2, used for facts
//! only) for the port decoding, the status and control bits, the drive
//! chain and the block model; 198x's `interface1-reference.md` for the
//! trap addresses and register map. The status bits are active low, as
//! Fuse has them and the Interface 1 ROM expects.

use common_sinclair_zx_spectrum::peripheral::{MemoryIntercept, Peripheral};
use format_sinclair_zx_spectrum_mdr::{HEADER_LEN, MdrImage, SECTOR_LEN};

/// The size of the shadow ROM.
pub const INTERFACE1_ROM_BYTES: usize = 0x2000;

/// The number of Microdrives the chain can address.
pub const MICRODRIVE_COUNT: usize = 8;

/// Bytes the drive delivers from a data block: its record header, data
/// and checksum.
const DATA_BLOCK_BYTES: usize = SECTOR_LEN - HEADER_LEN;

/// A block's preamble has been written and read back intact.
const SYNC_OK: u8 = 0xFF;

/// Status reads of gap, and then of sync, before a block.
const GAP_SYNC_POLLS: u8 = 15;

/// A ROM image that is not 8 KB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interface1RomError {
    /// The length that was supplied.
    pub actual: usize,
}

impl std::fmt::Display for Interface1RomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "expected an {INTERFACE1_ROM_BYTES}-byte Interface 1 ROM image, got {} bytes",
            self.actual
        )
    }
}

impl std::error::Error for Interface1RomError {}

/// A cartridge in a drive.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Cartridge {
    data: Vec<u8>,
    write_protected: bool,
    /// Per block, whether its preamble is good: header blocks first, then
    /// data blocks from index 256, as Fuse keeps them.
    preamble: Vec<u8>,
    modified: bool,
}

impl Cartridge {
    fn from_image(image: &MdrImage, formatted: bool) -> Self {
        let state = if formatted { SYNC_OK } else { 0 };
        let mut preamble = vec![0; 512];
        for sector in 0..image.sector_count() {
            preamble[sector] = state;
            preamble[256 + sector] = state;
        }
        Self {
            data: image.data().to_vec(),
            write_protected: image.write_protected,
            preamble,
            modified: !formatted,
        }
    }

    fn to_image(&self) -> MdrImage {
        let mut bytes = self.data.clone();
        bytes.push(u8::from(self.write_protected));
        // The data came from a valid image and never changes length.
        MdrImage::parse(&bytes).unwrap_or_else(|_| MdrImage::blank(self.data.len() / SECTOR_LEN))
    }
}

/// One drive on the chain.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Microdrive {
    cartridge: Option<Cartridge>,
    motor_on: bool,
    head_pos: usize,
    transferred: usize,
    max_bytes: usize,
    last: u8,
    gap: u8,
    sync: u8,
}

impl Microdrive {
    fn reset(&mut self) {
        self.head_pos = 0;
        self.motor_on = false;
        self.gap = GAP_SYNC_POLLS;
        self.sync = GAP_SYNC_POLLS;
        self.transferred = 0;
    }

    fn running(&mut self) -> Option<&mut Cartridge> {
        if self.motor_on {
            self.cartridge.as_mut()
        } else {
            None
        }
    }

    /// The preamble slot of the block under the head.
    fn block(&self) -> usize {
        self.head_pos / SECTOR_LEN + if self.max_bytes == HEADER_LEN { 0 } else { 256 }
    }

    fn advance_head(&mut self) {
        let len = self.cartridge.as_ref().map_or(0, |c| c.data.len());
        self.head_pos += 1;
        if self.head_pos >= len {
            self.head_pos = 0;
        }
    }

    /// Moves the head to the start of the next header or data block and
    /// starts counting bytes afresh.
    fn restart(&mut self) {
        if self.cartridge.is_some() {
            while !self.head_pos.is_multiple_of(SECTOR_LEN)
                && self.head_pos % SECTOR_LEN != HEADER_LEN
            {
                self.advance_head();
            }
        }
        self.transferred = 0;
        self.max_bytes = if self.head_pos.is_multiple_of(SECTOR_LEN) {
            HEADER_LEN
        } else {
            DATA_BLOCK_BYTES
        };
    }

    fn read_data(&mut self) -> Option<u8> {
        let head = self.head_pos;
        let fresh = self.transferred < self.max_bytes;
        let cartridge = self.running()?;
        let byte = cartridge.data[head];
        if fresh {
            self.last = byte;
            self.advance_head();
        }
        self.transferred += 1;
        Some(self.last)
    }

    fn write_data(&mut self, value: u8) {
        let block = self.block();
        let head = self.head_pos;
        let transferred = self.transferred;
        let max_bytes = self.max_bytes;
        let Some(cartridge) = self.running() else {
            return;
        };
        // The 12-byte preamble: ten $00, then two $FF.
        let slot = &mut cartridge.preamble[block];
        if transferred == 0 && value == 0x00 {
            *slot = 1;
        } else if (1..10).contains(&transferred) && value == 0x00
            || (10..12).contains(&transferred) && value == 0xFF
        {
            *slot = slot.wrapping_add(1);
        } else if transferred == 12 && *slot == 12 {
            *slot = SYNC_OK;
        }
        let in_block = transferred > 11 && transferred < max_bytes + 12;
        if in_block {
            cartridge.data[head] = value;
            cartridge.modified = true;
        }
        if in_block {
            self.advance_head();
        }
        self.transferred += 1;
    }

    /// Clears the status bits this drive drives low.
    fn status(&mut self, mut status: u8) -> u8 {
        let block = self.block();
        let Some(cartridge) = self.running() else {
            return status;
        };
        let formatted = cartridge.preamble[block] == SYNC_OK;
        let protected = cartridge.write_protected;
        if formatted {
            if self.gap > 0 {
                self.gap -= 1;
            } else {
                status &= 0xF9;
                if self.sync > 0 {
                    self.sync -= 1;
                } else {
                    self.gap = GAP_SYNC_POLLS;
                    self.sync = GAP_SYNC_POLLS;
                }
            }
        }
        if protected {
            status &= 0xFE;
        }
        status
    }
}

/// Which register a port selects.
enum Register {
    Microdrive,
    Control,
    Comms,
}

/// The interface's state.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Interface1 {
    attached: bool,
    rom: Vec<u8>,
    paged: bool,
    drives: [Microdrive; MICRODRIVE_COUNT],
    comms_clk: bool,
    comms_data: bool,
    cts: bool,
    wait: bool,
    /// The level last written to the ZX Net / RS-232 output.
    net_out: bool,
    /// The ZX Net input line.
    pub net_in: bool,
    /// The RS-232 TxD input line, from the attached device.
    pub tx: bool,
    /// The RS-232 DTR input line, from the attached device.
    pub dtr: bool,
    /// The busy line, which the Interface 1 ROM never reads.
    pub busy: bool,
}

impl Default for Interface1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Interface1 {
    /// An unplugged interface with eight empty drives.
    #[must_use]
    pub fn new() -> Self {
        let mut interface = Self {
            attached: false,
            rom: vec![0; INTERFACE1_ROM_BYTES],
            paged: false,
            drives: Default::default(),
            comms_clk: false,
            comms_data: false,
            cts: false,
            wait: false,
            net_out: false,
            net_in: false,
            tx: false,
            dtr: false,
            busy: false,
        };
        interface.reset();
        interface
    }

    /// Plugs the interface in with its ROM image.
    ///
    /// # Errors
    ///
    /// Returns an error, and leaves the interface as it was, if the image
    /// is not 8 KB.
    pub fn attach(&mut self, rom: &[u8]) -> Result<(), Interface1RomError> {
        if rom.len() != INTERFACE1_ROM_BYTES {
            return Err(Interface1RomError { actual: rom.len() });
        }
        self.rom.copy_from_slice(rom);
        self.attached = true;
        self.reset();
        Ok(())
    }

    /// Whether the interface is plugged in.
    #[must_use]
    pub const fn is_attached(&self) -> bool {
        self.attached
    }

    /// Whether the shadow ROM is paged in.
    #[must_use]
    pub const fn is_paged(&self) -> bool {
        self.paged
    }

    /// Whether drive `drive` (0 to 7) has its motor running.
    #[must_use]
    pub fn motor_on(&self, drive: usize) -> bool {
        self.drives.get(drive).is_some_and(|d| d.motor_on)
    }

    /// The CTS line, as last written to the control register.
    #[must_use]
    pub const fn cts(&self) -> bool {
        self.cts
    }

    /// The ZX Net / RS-232 output line, as last written.
    #[must_use]
    pub const fn net_out(&self) -> bool {
        self.net_out
    }

    /// Returns the interface to its power-on state: paged out, every
    /// motor stopped and every head at the start of its tape. Cartridges
    /// stay in their drives.
    pub fn reset(&mut self) {
        self.paged = false;
        self.comms_clk = false;
        self.comms_data = false;
        self.net_out = false;
        for drive in &mut self.drives {
            drive.reset();
        }
    }

    /// Puts a cartridge in drive `drive` (0 to 7), replacing any there.
    /// An image read from a file is taken to be formatted. Returns
    /// `false` if there is no such drive.
    pub fn insert(&mut self, drive: usize, image: &MdrImage) -> bool {
        self.insert_cartridge(drive, Cartridge::from_image(image, true))
    }

    /// Puts an unformatted cartridge of `sectors` sectors in drive
    /// `drive`. Returns `false` if there is no such drive.
    pub fn insert_blank(&mut self, drive: usize, sectors: usize) -> bool {
        self.insert_cartridge(
            drive,
            Cartridge::from_image(&MdrImage::blank(sectors), false),
        )
    }

    fn insert_cartridge(&mut self, drive: usize, cartridge: Cartridge) -> bool {
        let Some(slot) = self.drives.get_mut(drive) else {
            return false;
        };
        slot.cartridge = Some(cartridge);
        slot.head_pos = 0;
        slot.restart();
        true
    }

    /// Takes the cartridge out of drive `drive`, returning its image.
    pub fn eject(&mut self, drive: usize) -> Option<MdrImage> {
        let slot = self.drives.get_mut(drive)?;
        let image = slot.cartridge.take()?.to_image();
        slot.head_pos = 0;
        Some(image)
    }

    /// The image of the cartridge in drive `drive`, as it is now.
    #[must_use]
    pub fn cartridge(&self, drive: usize) -> Option<MdrImage> {
        self.drives
            .get(drive)?
            .cartridge
            .as_ref()
            .map(Cartridge::to_image)
    }

    /// Whether the cartridge in drive `drive` has been written since it
    /// was inserted.
    #[must_use]
    pub fn cartridge_modified(&self, drive: usize) -> bool {
        self.drives
            .get(drive)
            .and_then(|d| d.cartridge.as_ref())
            .is_some_and(|c| c.modified)
    }

    fn restart_drives(&mut self) {
        for drive in &mut self.drives {
            drive.restart();
        }
    }

    const fn register(port: u16) -> Option<Register> {
        match port & 0x0018 {
            0x0000 => Some(Register::Microdrive),
            0x0008 => Some(Register::Control),
            0x0010 => Some(Register::Comms),
            _ => None,
        }
    }

    fn write_control(&mut self, value: u8) {
        let clk = value & 0x02 != 0;
        if self.comms_clk && !clk {
            for d in (1..MICRODRIVE_COUNT).rev() {
                self.drives[d].motor_on = self.drives[d - 1].motor_on;
            }
            self.drives[0].motor_on = value & 0x01 == 0;
        }
        self.wait = value & 0x20 != 0;
        self.comms_data = value & 0x01 != 0;
        self.comms_clk = clk;
        self.cts = value & 0x10 != 0;
        self.restart_drives();
    }

    fn read_status(&mut self) -> u8 {
        let mut status = 0xFF;
        for drive in &mut self.drives {
            status = drive.status(status);
        }
        if !self.dtr {
            status &= 0xF7;
        }
        if !self.busy {
            status &= 0xEF;
        }
        self.restart_drives();
        status
    }

    fn read_comms(&mut self) -> u8 {
        let mut value = 0xFF;
        if !self.tx {
            value &= 0x7F;
        }
        if !self.net_in {
            value &= 0xFE;
        }
        self.restart_drives();
        value
    }
}

impl Peripheral for Interface1 {
    fn claims_port(&self, port: u16) -> bool {
        self.attached && Self::register(port).is_some()
    }

    fn read(&mut self, port: u16) -> u8 {
        match Self::register(port) {
            Some(Register::Microdrive) => self
                .drives
                .iter_mut()
                .filter_map(Microdrive::read_data)
                .fold(0xFF, |acc, byte| acc & byte),
            Some(Register::Control) => self.read_status(),
            Some(Register::Comms) => self.read_comms(),
            None => 0xFF,
        }
    }

    fn write(&mut self, port: u16, value: u8) {
        match Self::register(port) {
            Some(Register::Microdrive) => {
                for drive in &mut self.drives {
                    drive.write_data(value);
                }
            }
            Some(Register::Control) => self.write_control(value),
            // Nothing is connected to the outputs, so as in Fuse the drives
            // keep their place.
            Some(Register::Comms) => self.net_out = value & 0x01 != 0,
            None => {}
        }
    }

    fn on_m1(&mut self, addr: u16) {
        if self.attached && (addr == 0x0008 || addr == 0x1708) {
            self.paged = true;
        }
    }
}

impl MemoryIntercept for Interface1 {
    fn read_memory(&self, addr: u16) -> Option<u8> {
        if self.paged && addr < 0x4000 {
            Some(self.rom[usize::from(addr) & (INTERFACE1_ROM_BYTES - 1)])
        } else {
            None
        }
    }

    fn write_memory(&mut self, addr: u16, _val: u8) -> bool {
        self.paged && addr < 0x4000
    }

    fn after_m1(&mut self, addr: u16) {
        if addr == 0x0700 {
            self.paged = false;
        }
    }
}

#[cfg(test)]
mod tests;
