//! The Famicom Disk System's RAM adapter and drive, as a board in the
//! cartridge slot: 32 KiB of PRG RAM at `$6000–$DFFF`, the 8 KiB BIOS at
//! `$E000`, 8 KiB of CHR RAM, a timer IRQ, the disk interface and the
//! sound unit ([`super::fds_audio`]).
//!
//! # Firmware
//!
//! The BIOS is Nintendo's and is never committed. The board is built with
//! whatever 8 KiB image the caller supplies — the runtime takes it from
//! its `fds-bios` firmware slot — and tests use synthetic programs.
//!
//! # The disk
//!
//! A `.fds` file keeps only block contents. The drive reads more than
//! that, so each side is laid out on a track as the drive meets it: a
//! lead-in gap of 28,300 bits, then for each block a start mark (`$80`),
//! the block, its CRC-16 (polynomial `$8408`, reflected, over the mark
//! and the block, low byte first), and a 976-bit gap. Bytes after the
//! last block that the block types do not describe are kept after one
//! more start mark. The CRC is the real one, so the adapter's CRC check
//! passes on every block.
//!
//! The drive moves one byte every 150 CPU cycles once it is up to speed,
//! about 96.4 kbit/s; spinning up from the start of the track takes 50,000
//! cycles. Each byte read sets the transfer flag and, if enabled, raises
//! the disk IRQ, from the first non-zero byte after the gap (the start
//! mark) onward, while the adapter has the drive "ready". The head runs to
//! the end of the track, stops the motor and must be restarted.
//!
//! # Registers
//!
//! | Address | Write | Read |
//! |---|---|---|
//! | `$4020/1` | timer reload, low and high | |
//! | `$4022` | timer control: bit 0 repeat, bit 1 enable | |
//! | `$4023` | bit 0 disk registers, bit 1 sound registers | |
//! | `$4024` | data to write | |
//! | `$4025` | drive control | |
//! | `$4026` | expansion port | |
//! | `$4030` | | status: IRQs, transfer done, mirroring, CRC |
//! | `$4031` | | data read |
//! | `$4032` | | drive status |
//! | `$4033` | | expansion port; bit 7 is battery good |
//!
//! Facts from the NESdev wiki "Family Computer Disk System" and "FDS disk
//! format" pages and Mesen2's `Fds.cpp` and `FdsLoader.cpp` (GPL-3, read
//! for behaviour only). Mesen2's automatic disk insertion and fast-forward
//! are emulator conveniences and are not modelled.

use format_nintendo_famicom_fds::{SIDE_LEN, Side, parse, write};
use serde::{Deserialize, Serialize};

use super::fds_audio::FdsAudio;
use crate::mapper::{DiskDrive, Mapper, Mirroring, PpuCartridge};
use crate::snapshot::MapperSnapshot;

/// The BIOS's size.
pub const BIOS_LEN: usize = 8192;

/// The lead-in gap before the first block, in bytes (28,300 bits).
const LEAD_IN: usize = 28_300 / 8;

/// The gap after each block, in bytes (976 bits).
const BLOCK_GAP: usize = 976 / 8;

/// The mark that ends a gap and starts a block.
const START_MARK: u8 = 0x80;

/// CPU cycles from motor start to the first byte under the head.
const SPIN_UP_CYCLES: u32 = 50_000;

/// CPU cycles between bytes after the first. (Mesen2 waits 149 cycles
/// plus the cycle that moves the byte.)
const BYTE_CYCLES: u32 = 149;

/// Bits of a status read the adapter leaves to the bus, which after an
/// absolute read of `$40xx` holds `$40`.
const OPEN_BUS: u8 = 0x40;

/// The NESdev wiki puts the FDS wave at full volume near 2.4 times a
/// 2A03 pulse at full volume (the APU mix gives that pulse 0.1494).
const FULL_SCALE: f32 = 2.4 * 0.1494;

/// Advance a CRC-16/KERMIT-style accumulator (polynomial `$8408`,
/// reflected) by one byte, as the adapter does.
fn crc_step(crc: u16, byte: u8) -> u16 {
    let mut crc = crc ^ u16::from(byte);
    for _ in 0..8 {
        let carry = crc & 1 != 0;
        crc >>= 1;
        if carry {
            crc ^= 0x8408;
        }
    }
    crc
}

/// Lay a side's blocks out on a track as the drive reads them.
#[must_use]
pub fn side_to_track(side: &[u8]) -> Vec<u8> {
    let scan = Side { bytes: side }.scan();
    let mut track = vec![0; LEAD_IN];
    for block in &scan.blocks {
        let crc = block
            .bytes
            .iter()
            .fold(crc_step(0, START_MARK), |crc, &byte| crc_step(crc, byte));
        track.push(START_MARK);
        track.extend_from_slice(block.bytes);
        track.extend_from_slice(&crc.to_le_bytes());
        track.resize(track.len() + BLOCK_GAP, 0);
    }
    if scan.rest.iter().any(|&byte| byte != 0) {
        track.push(START_MARK);
        track.extend_from_slice(scan.rest);
    }
    if track.len() < SIDE_LEN {
        track.resize(SIDE_LEN, 0);
    }
    track
}

/// Read a track back into the `.fds` side layout: the blocks without
/// their marks, CRCs and gaps, padded to 65,500 bytes.
#[must_use]
pub fn track_to_side(track: &[u8]) -> Vec<u8> {
    let mut side = Vec::with_capacity(SIDE_LEN);
    let mut at = 0;
    let mut file_size = 0;
    while at < track.len() {
        // Skip the gap to the next start mark.
        let Some(mark) = track[at..].iter().position(|&byte| byte == START_MARK) else {
            break;
        };
        at += mark + 1;
        let len = match track.get(at) {
            Some(1) => 56,
            Some(2) => 2,
            Some(3) => 16,
            Some(4) => 1 + file_size,
            _ => {
                // Data the block types do not describe: keep it as is.
                side.extend_from_slice(&track[at..]);
                break;
            }
        };
        let Some(block) = track.get(at..at + len) else {
            break;
        };
        if block[0] == 3 {
            file_size = usize::from(u16::from_le_bytes([block[13], block[14]]));
        }
        side.extend_from_slice(block);
        at += len + 2;
    }
    side.resize(SIDE_LEN, 0);
    side
}

/// The FDS RAM adapter with its drive.
#[derive(Clone, Serialize, Deserialize)]
pub struct Fds {
    bios: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_ram: Vec<u8>,
    tracks: Vec<Vec<u8>>,
    side: Option<usize>,
    /// Whether the file had the fwNES header, so [`DiskDrive::image`]
    /// writes it back the same way.
    file_header: bool,

    timer_reload: u16,
    timer_counter: u16,
    timer_enabled: bool,
    timer_repeat: bool,
    timer_irq: bool,
    disk_registers: bool,
    sound_registers: bool,

    write_data: u8,
    motor_on: bool,
    transfer_reset: bool,
    read_mode: bool,
    horizontal: bool,
    crc_control: bool,
    drive_ready: bool,
    disk_irq_enabled: bool,
    expansion: u8,

    disk_irq: bool,
    transfer_complete: bool,
    read_data: u8,
    bad_crc: bool,
    end_of_head: bool,
    position: usize,
    delay: u32,
    crc: u16,
    previous_crc_control: bool,
    gap_ended: bool,
    scanning: bool,

    audio: FdsAudio,
}

impl Fds {
    /// Build the adapter around a BIOS image and the sides of a `.fds`
    /// file, in file order. The BIOS is padded or cut to 8 KiB. No side
    /// is inserted until [`DiskDrive::insert`] is called.
    #[must_use]
    pub fn new<S: AsRef<[u8]>>(bios: &[u8], sides: &[S]) -> Self {
        let mut bios = bios.to_vec();
        bios.resize(BIOS_LEN, 0);
        Self {
            bios,
            prg_ram: vec![0; 32 * 1024],
            chr_ram: vec![0; 8192],
            tracks: sides
                .iter()
                .map(|side| side_to_track(side.as_ref()))
                .collect(),
            side: None,
            file_header: false,
            timer_reload: 0,
            timer_counter: 0,
            timer_enabled: false,
            timer_repeat: false,
            timer_irq: false,
            disk_registers: true,
            sound_registers: true,
            write_data: 0,
            motor_on: false,
            transfer_reset: false,
            read_mode: false,
            horizontal: false,
            crc_control: false,
            drive_ready: false,
            disk_irq_enabled: false,
            expansion: 0,
            disk_irq: false,
            transfer_complete: false,
            read_data: 0,
            bad_crc: false,
            end_of_head: true,
            position: 0,
            delay: 0,
            crc: 0,
            previous_crc_control: false,
            gap_ended: true,
            scanning: false,
            audio: FdsAudio::default(),
        }
    }

    /// Build the adapter from a BIOS and a `.fds` file, with side 0
    /// (disk 1 side A) in the drive.
    ///
    /// # Errors
    ///
    /// The `.fds` layout's error, as text, when the file cannot be read.
    pub fn from_file(bios: &[u8], file: &[u8]) -> Result<Self, String> {
        let image = parse(file).map_err(|error| error.to_string())?;
        let sides: Vec<&[u8]> = image.sides.iter().map(|side| side.bytes).collect();
        let mut fds = Self::new(bios, &sides);
        fds.file_header = image.has_header;
        fds.side = Some(0);
        Ok(fds)
    }

    /// The sound unit's output level, 0 to 63.
    #[must_use]
    pub fn audio_level(&self) -> u8 {
        self.audio.output()
    }

    fn status(&self) -> u8 {
        (OPEN_BUS & 0x24)
            | u8::from(self.timer_irq)
            | (u8::from(self.transfer_complete) << 1)
            | (u8::from(self.horizontal) << 3)
            | (u8::from(self.bad_crc) << 4)
    }

    fn drive_status(&self) -> u8 {
        let empty = self.side.is_none();
        (OPEN_BUS & 0xF8)
            | u8::from(empty)
            | (u8::from(empty || !self.scanning) << 1)
            | (u8::from(empty) << 2)
    }

    /// A register read without side effects.
    fn peek_register(&self, addr: u16) -> Option<u8> {
        match addr {
            0x4040..=0x4097 if self.sound_registers => self.audio.read(addr),
            0x4030 if self.disk_registers => Some(self.status()),
            0x4031 if self.disk_registers => Some(self.read_data),
            0x4032 if self.disk_registers => Some(self.drive_status()),
            0x4033 if self.disk_registers => Some(self.expansion),
            _ => None,
        }
    }

    fn tick_timer(&mut self) {
        if !self.timer_enabled {
            return;
        }
        if self.timer_counter == 0 {
            self.timer_irq = true;
            self.timer_counter = self.timer_reload;
            if !self.timer_repeat {
                self.timer_enabled = false;
            }
        } else {
            self.timer_counter -= 1;
        }
    }

    fn tick_drive(&mut self) {
        let Some(side) = self.side else {
            self.end_of_head = true;
            self.scanning = false;
            return;
        };
        if !self.motor_on {
            self.end_of_head = true;
            self.scanning = false;
            return;
        }
        if self.transfer_reset && !self.scanning {
            return;
        }
        if self.end_of_head {
            self.delay = SPIN_UP_CYCLES;
            self.end_of_head = false;
            self.position = 0;
            self.gap_ended = false;
            return;
        }
        if self.delay > 0 {
            self.delay -= 1;
            return;
        }
        self.scanning = true;
        let mut raise = self.disk_irq_enabled;
        if self.read_mode {
            let byte = self.tracks[side][self.position];
            if !self.previous_crc_control {
                self.crc = crc_step(self.crc, byte);
            }
            if !self.drive_ready {
                self.gap_ended = false;
                self.crc = 0;
                self.bad_crc = false;
            } else if byte != 0 && !self.gap_ended {
                // The start mark: the next byte is the first of the block.
                self.gap_ended = true;
                raise = false;
            }
            if self.gap_ended {
                self.transfer_complete = true;
                self.read_data = byte;
                if raise {
                    self.disk_irq = true;
                }
            }
            if !self.previous_crc_control && self.crc_control {
                self.bad_crc = self.crc != 0;
            }
        } else {
            let mut byte = 0;
            if !self.crc_control {
                self.transfer_complete = true;
                byte = self.write_data;
                if raise {
                    self.disk_irq = true;
                }
            }
            if !self.drive_ready {
                byte = 0;
                self.crc = 0;
            }
            if self.crc_control {
                byte = (self.crc & 0xFF) as u8;
                self.crc >>= 8;
            } else {
                self.crc = crc_step(self.crc, byte);
            }
            self.tracks[side][self.position] = byte;
            self.gap_ended = false;
            self.bad_crc = false;
        }
        self.previous_crc_control = self.crc_control;
        self.position += 1;
        if self.position >= self.tracks[side].len() {
            self.motor_on = false;
            if self.disk_irq_enabled {
                self.disk_irq = true;
            }
        } else {
            self.delay = BYTE_CYCLES;
        }
    }
}

impl DiskDrive for Fds {
    fn side_count(&self) -> usize {
        self.tracks.len()
    }

    fn inserted(&self) -> Option<usize> {
        self.side
    }

    fn insert(&mut self, side: usize) {
        if side < self.tracks.len() {
            self.side = Some(side);
        }
    }

    fn eject(&mut self) {
        self.side = None;
    }

    fn image(&self) -> Vec<u8> {
        let sides: Vec<Vec<u8>> = self
            .tracks
            .iter()
            .map(|track| track_to_side(track))
            .collect();
        write(&sides, self.file_header)
    }
}

impl PpuCartridge for Fds {
    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr_ram[usize::from(addr & 0x1FFF)]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        self.chr_ram[usize::from(addr & 0x1FFF)] = value;
    }

    fn mirroring(&self) -> Mirroring {
        if self.horizontal {
            Mirroring::Horizontal
        } else {
            Mirroring::Vertical
        }
    }
}

impl Mapper for Fds {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x4020..=0x5FFF => self.peek_register(addr).unwrap_or(OPEN_BUS),
            0x6000..=0xDFFF => self.prg_ram[usize::from(addr - 0x6000)],
            0xE000..=0xFFFF => self.bios[usize::from(addr - 0xE000)],
            _ => 0,
        }
    }

    fn cpu_read_expansion(&mut self, addr: u16) -> Option<u8> {
        let value = self.peek_register(addr)?;
        match addr {
            0x4030 => {
                self.transfer_complete = false;
                self.timer_irq = false;
                self.disk_irq = false;
            }
            0x4031 => {
                self.transfer_complete = false;
                self.disk_irq = false;
            }
            _ => {}
        }
        Some(value)
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x4024..=0x4026 if !self.disk_registers => {}
            0x4040..=0x408A if !self.sound_registers => {}
            0x4020 => self.timer_reload = (self.timer_reload & 0xFF00) | u16::from(value),
            0x4021 => self.timer_reload = (self.timer_reload & 0x00FF) | (u16::from(value) << 8),
            0x4022 => {
                self.timer_repeat = value & 0x01 != 0;
                self.timer_enabled = value & 0x02 != 0 && self.disk_registers;
                if self.timer_enabled {
                    self.timer_counter = self.timer_reload;
                } else {
                    self.timer_irq = false;
                }
            }
            0x4023 => {
                self.disk_registers = value & 0x01 != 0;
                self.sound_registers = value & 0x02 != 0;
                if !self.disk_registers {
                    self.timer_enabled = false;
                    self.timer_irq = false;
                    self.disk_irq = false;
                }
            }
            0x4024 => {
                self.write_data = value;
                self.transfer_complete = false;
                self.disk_irq = false;
            }
            0x4025 => {
                self.motor_on = value & 0x01 != 0;
                self.transfer_reset = value & 0x02 != 0;
                self.read_mode = value & 0x04 != 0;
                self.horizontal = value & 0x08 != 0;
                self.crc_control = value & 0x10 != 0;
                self.drive_ready = value & 0x40 != 0;
                self.disk_irq_enabled = value & 0x80 != 0;
                self.disk_irq = false;
            }
            0x4026 => self.expansion = value,
            0x4040..=0x408A => self.audio.write(addr, value),
            0x6000..=0xDFFF => self.prg_ram[usize::from(addr - 0x6000)] = value,
            _ => {}
        }
    }

    fn irq_pending(&self) -> bool {
        self.timer_irq || self.disk_irq
    }

    fn cpu_tick(&mut self) {
        self.tick_timer();
        self.audio.tick();
        self.tick_drive();
    }

    fn expansion_audio_sample(&self) -> f32 {
        f32::from(self.audio.output()) / 63.0 * FULL_SCALE
    }

    fn disk_drive(&self) -> Option<&dyn DiskDrive> {
        Some(self)
    }

    fn disk_drive_mut(&mut self) -> Option<&mut dyn DiskDrive> {
        Some(self)
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Fds(Box::new(self.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use format_nintendo_famicom_fds::DISK_SIGNATURE;

    /// A side with a disk info block, a file amount, and one file.
    fn side(data: &[u8]) -> Vec<u8> {
        let mut side = vec![0x01];
        side.extend_from_slice(&DISK_SIGNATURE);
        side.resize(56, 0);
        side.extend_from_slice(&[0x02, 1]);
        side.extend_from_slice(&[0x03, 0, 0]);
        side.extend_from_slice(b"FILE    ");
        side.extend_from_slice(&0x6000u16.to_le_bytes());
        side.extend_from_slice(&(data.len() as u16).to_le_bytes());
        side.push(0);
        side.push(0x04);
        side.extend_from_slice(data);
        side.resize(SIDE_LEN, 0);
        side
    }

    #[test]
    fn a_track_carries_gaps_marks_and_crcs_that_check() {
        let track = side_to_track(&side(b"hi"));
        assert!(track[..LEAD_IN].iter().all(|&b| b == 0));
        assert_eq!(track[LEAD_IN], START_MARK);
        assert_eq!(track[LEAD_IN + 1], 0x01);
        // The CRC over the mark, block and CRC is zero.
        let block_end = LEAD_IN + 1 + 56 + 2;
        let crc = track[LEAD_IN..block_end]
            .iter()
            .fold(0, |crc, &byte| crc_step(crc, byte));
        assert_eq!(crc, 0);
        assert!(
            track[block_end..block_end + BLOCK_GAP]
                .iter()
                .all(|&b| b == 0)
        );
        assert_eq!(track[block_end + BLOCK_GAP], START_MARK);
        assert_eq!(track_to_side(&track), side(b"hi"));
    }

    #[test]
    fn crc_matches_the_kermit_check_value() {
        let crc = b"123456789"
            .iter()
            .fold(0, |crc, &byte| crc_step(crc, byte));
        assert_eq!(crc, 0x2189);
    }

    #[test]
    fn memory_map_and_registers() {
        let bios: Vec<u8> = (0..BIOS_LEN).map(|i| (i >> 8) as u8).collect();
        let mut fds = Fds::new(&bios, &[side(b"x")]);
        assert_eq!(fds.cpu_read(0xFFFF), 0x1F);
        fds.cpu_write(0x6000, 1);
        fds.cpu_write(0xDFFF, 2);
        assert_eq!(fds.cpu_read(0x6000), 1);
        assert_eq!(fds.cpu_read(0xDFFF), 2);
        fds.chr_write(0x1FFF, 3);
        assert_eq!(fds.chr_read(0x1FFF), 3);
        fds.cpu_write(0x4025, 0x08);
        assert_eq!(fds.mirroring(), Mirroring::Horizontal);
        assert_eq!(fds.cpu_read_expansion(0x4030).map(|v| v & 0x08), Some(0x08));
        // No disk: not inserted, not ready, protected.
        assert_eq!(fds.cpu_read_expansion(0x4032).map(|v| v & 0x07), Some(0x07));
        fds.insert(0);
        assert_eq!(fds.cpu_read_expansion(0x4032).map(|v| v & 0x07), Some(0x02));
        fds.cpu_write(0x4026, 0x80);
        assert_eq!(fds.cpu_read_expansion(0x4033), Some(0x80));
        assert_eq!(fds.cpu_read_expansion(0x4100), None);
        // Disabling the disk registers hides them.
        fds.cpu_write(0x4023, 0x02);
        assert_eq!(fds.cpu_read_expansion(0x4030), None);
    }

    #[test]
    fn the_timer_irq_counts_down_and_repeats() {
        let mut fds = Fds::new(&[], &[side(b"")]);
        fds.cpu_write(0x4023, 0x01);
        fds.cpu_write(0x4020, 3);
        fds.cpu_write(0x4021, 0);
        fds.cpu_write(0x4022, 0x03); // enable, repeat
        for _ in 0..3 {
            fds.cpu_tick();
        }
        assert!(!fds.irq_pending(), "3, 2, 1");
        fds.cpu_tick();
        assert!(fds.irq_pending(), "0 fires");
        assert_eq!(fds.cpu_read_expansion(0x4030).map(|v| v & 1), Some(1));
        assert!(!fds.irq_pending(), "reading $4030 acknowledges");
        for _ in 0..4 {
            fds.cpu_tick();
        }
        assert!(fds.irq_pending(), "repeat reloads");
        fds.cpu_write(0x4022, 0);
        assert!(!fds.irq_pending());
    }

    /// Run until the adapter has a byte, then take it as the BIOS would.
    fn next_byte(fds: &mut Fds) -> (u8, u32) {
        // The lead-in gap alone is 3,537 bytes, over half a million cycles.
        for cycles in 1..1_000_000 {
            fds.cpu_tick();
            if fds.irq_pending() {
                return (fds.cpu_read_expansion(0x4031).expect("data"), cycles);
            }
        }
        panic!("no byte");
    }

    #[test]
    fn reading_a_side_delivers_each_block_byte_with_a_disk_irq() {
        let mut fds = Fds::new(&[], &[side(b"hi")]);
        fds.insert(0);
        // Motor on, read mode, IRQ on; not yet ready, which skips the gap.
        fds.cpu_write(0x4025, 0x85);
        for _ in 0..SPIN_UP_CYCLES + 1000 {
            fds.cpu_tick();
        }
        assert!(!fds.irq_pending(), "no data while not ready");
        fds.cpu_write(0x4025, 0xC5); // ready: wait for the start mark
        let (first, _) = next_byte(&mut fds);
        assert_eq!(first, 0x01, "the disk info block's type byte");
        let mut block = vec![first];
        for _ in 1..56 {
            let (byte, cycles) = next_byte(&mut fds);
            assert_eq!(cycles, BYTE_CYCLES + 1);
            block.push(byte);
        }
        assert_eq!(&block[1..15], &DISK_SIGNATURE);
        // Two CRC bytes, then CRC control: the check passes.
        next_byte(&mut fds);
        next_byte(&mut fds);
        fds.cpu_write(0x4025, 0xD5);
        for _ in 0..=BYTE_CYCLES {
            fds.cpu_tick();
        }
        assert_eq!(fds.cpu_read(0x4030) & 0x10, 0, "CRC good");
        assert_eq!(fds.cpu_read(0x4032) & 0x02, 0, "ready while scanning");
    }

    #[test]
    fn writing_puts_bytes_and_their_crc_on_the_track() {
        let mut fds = Fds::new(&[], &[side(b"hi")]);
        fds.insert(0);
        fds.cpu_write(0x4025, 0x41); // motor on, write mode, ready
        for _ in 0..SPIN_UP_CYCLES + 1 {
            fds.cpu_tick();
        }
        let start = fds.position;
        for byte in [START_MARK, 0x02, 0x07] {
            fds.cpu_write(0x4024, byte);
            for _ in 0..=BYTE_CYCLES {
                fds.cpu_tick();
            }
        }
        fds.cpu_write(0x4025, 0x51); // CRC control: write the CRC
        for _ in 0..2 * (BYTE_CYCLES + 1) {
            fds.cpu_tick();
        }
        let written = &fds.tracks[0][start..start + 5];
        assert_eq!(&written[..3], &[START_MARK, 0x02, 0x07]);
        assert_eq!(written.iter().fold(0, |crc, &b| crc_step(crc, b)), 0);
    }

    #[test]
    fn the_drive_reports_its_sides_and_writes_them_back() {
        let sides = [side(b"a"), side(b"bc")];
        let mut fds = Fds::new(&[], &sides);
        assert_eq!(fds.side_count(), 2);
        assert_eq!(fds.inserted(), None);
        fds.insert(5);
        assert_eq!(fds.inserted(), None, "no side 5");
        fds.insert(1);
        assert_eq!(fds.inserted(), Some(1));
        fds.eject();
        assert_eq!(fds.inserted(), None);
        assert_eq!(fds.image(), sides.concat());
    }

    #[test]
    fn a_file_loads_with_side_a_inserted_and_keeps_its_header() {
        let file = write(&[side(b"a"), side(b"b")], true);
        let fds = Fds::from_file(&[], &file).expect("load");
        assert_eq!(fds.inserted(), Some(0));
        assert_eq!(fds.side_count(), 2);
        assert_eq!(fds.image(), file);
        assert!(Fds::from_file(&[], &[0; 16]).is_err());
    }

    #[test]
    fn sound_registers_reach_the_audio_unit() {
        let mut fds = Fds::new(&[], &[side(b"")]);
        fds.cpu_write(0x4080, 0x80 | 20);
        assert_eq!(fds.cpu_read_expansion(0x4090).map(|v| v & 0x3F), Some(20));
        fds.cpu_write(0x4023, 0x01); // sound registers off
        fds.cpu_write(0x4080, 0x80 | 5);
        assert_eq!(fds.cpu_read_expansion(0x4090), None);
        assert_eq!(fds.expansion_audio_sample(), 0.0);
    }
}
