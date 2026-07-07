//! Board-level Commodore 1541 substrate.
//!
//! This crate deliberately stops before GCR and sector mechanics. It owns the
//! durable drive-side board behavior that those later layers will need:
//! - 6502 CPU bus loop
//! - 2KB RAM with mirroring
//! - 16KB DOS ROM mapping
//! - VIA1 and VIA2 register decode at `$1800`/`$1C00`
//! - IEC-visible VIA1 wiring
//! - first-pass drive-side status/mechanics signals
//!
//! It now also includes the first read-only D64-backed GCR/rotation layer the
//! DOS ROM needs for honest disk reads.

use common_commodore_drive_gcr::{
    GcrHeader, HEADER_GAP_SIZE, MAX_HEAD_POSITION, SECTOR_GCR_SIZE_WITH_HEADER, SYNC_SIZE,
    TRACK_SLOT_COUNT, encode_sector_to_gcr, gcr_read_sector_from_raw_track, speed_zone_for_track,
    track_slot_index,
};
use common_commodore_iec::IecBus;
use format_commodore_c64_d64::{
    D64FileType, D64ParseError, parse_directory, read_sector, sectors_in_track, write_sector,
};
use format_commodore_c64_g64::{G64Image, G64ParseError};
use mos_6502::{M6502, registers::FLAG_V};
use mos_via_6522::Via6522;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const RAM_SIZE: usize = 0x0800;
const ROM_SIZE: usize = 0x4000;
const DEFAULT_DEVICE_NUMBER: u8 = 8;
const INITIAL_HEAD_POSITION: u8 = 36;
const READ_BITS_PER_SECOND_BY_ZONE: [u64; 4] = [250_000, 266_667, 285_714, 307_692];
const RAW_TRACK_SIZE_BY_ZONE: [usize; 4] = [6_250, 6_666, 7_142, 7_692];
const GAP_SIZE_BY_ZONE: [usize; 4] = [9, 12, 17, 8];
/// Non-zero seed for the weak-bit LFSR so it never starts in a degenerate state.
const WEAK_BIT_SEED: u32 = 0x2545_F491;
const IO_TRACE_LIMIT: usize = 2048;
const ROTATION_REF_CYCLES_PER_CPU_CYCLE: u64 = 16;
const BUS_READ_DELAY_REF_CYCLES: u64 = 14;

/// Nominal 1541 6502 clock used for first-pass combined C64/drive scheduling.
pub const DRIVE1541_CPU_HZ: u64 = 1_000_000;

#[derive(Clone)]
pub struct Drive1541 {
    cpu: M6502,
    via1: Via6522,
    via2: Via6522,
    ram: [u8; RAM_SIZE],
    rom: [u8; ROM_SIZE],
    disk: Option<Drive1541Disk>,
    track_data: Option<Drive1541TrackData>,
    device_number: u8,
    head_position: u8,
    stepper_phase: u8,
    motor_on: bool,
    activity_led: bool,
    density_code: u8,
    gcr_read: u8,
    gcr_write_value: u8,
    gcr_head_offset: usize,
    last_read_data: u16,
    bit_counter: u8,
    /// LFSR/LCG state feeding weak-bit reads: over a `0x00` (no-flux) GCR byte
    /// the head picks up random flux, so each revolution reads differently.
    /// A G64 copy-protection check reads such an area twice and requires the
    /// bytes to differ. Advances only while reading a weak byte.
    weak_bit_lfsr: u32,
    /// Which bit of the write serialiser the head emits next, MSB first.
    /// Transient write-mode state; not snapshotted.
    write_bit_index: u8,
    /// The write serialiser: emits its MSB onto the surface and shifts left
    /// each bit, reloading from the `gcr_write_value` port latch only at the
    /// byte boundary. Modelling it as a latch-fed shift register (not a live
    /// index into `gcr_write_value`) means a mid-byte store by the ROM's write
    /// loop — which runs one byte ahead in the pipeline — cannot corrupt the
    /// byte already on its way to the surface. Mirrors VICE `rotation.c`.
    /// Transient write-mode state; not snapshotted.
    write_shift: u8,
    sync_active: bool,
    byte_ready_level: bool,
    byte_ready_edge: bool,
    byte_ready_delay_ref_cycles: u8,
    sync_event_count: u64,
    byte_ready_event_count: u64,
    rotation_accum: u64,
    rotation_ref_phase: u8,
    recent_io_writes: Vec<Drive1541IoWriteEvent>,
    cycles: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Drive1541Snapshot {
    cpu: M6502,
    via1: Via6522,
    via2: Via6522,
    ram: Vec<u8>,
    rom: Vec<u8>,
    disk: Option<Drive1541Disk>,
    device_number: u8,
    head_position: u8,
    stepper_phase: u8,
    motor_on: bool,
    activity_led: bool,
    density_code: u8,
    gcr_read: u8,
    gcr_write_value: u8,
    gcr_head_offset: usize,
    last_read_data: u16,
    bit_counter: u8,
    #[serde(default)]
    weak_bit_lfsr: u32,
    sync_active: bool,
    byte_ready_level: bool,
    byte_ready_edge: bool,
    byte_ready_delay_ref_cycles: u8,
    sync_event_count: u64,
    byte_ready_event_count: u64,
    rotation_accum: u64,
    rotation_ref_phase: u8,
    cycles: u64,
}

#[derive(Clone, Default)]
struct Drive1541TrackData {
    tracks: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Drive1541IoWriteEvent {
    pub cycle: u64,
    pub pc: u16,
    pub addr: u16,
    pub value: u8,
}

/// How a mounted disk's `image_bytes` are encoded — decoded sectors (`D64`) or
/// a raw-GCR track image (`G64`). Governs how the live track surface is (re)built
/// and whether it can be flushed back to a host image.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Drive1541ImageFormat {
    /// A decoded `D64` sector image (the surface is GCR-encoded on mount).
    #[default]
    D64,
    /// A raw-GCR `G64` image (the surface is the file's bytes verbatim,
    /// preserving copy protection). Read-only in v1.
    G64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Drive1541Disk {
    image_bytes: Vec<u8>,
    disk_name: String,
    disk_id: String,
    write_protected: bool,
    directory_entries: Vec<Drive1541DirectoryEntry>,
    /// Encoding of `image_bytes`. Defaults to `D64` so pre-G64 snapshots restore
    /// unchanged.
    #[serde(default)]
    image_format: Drive1541ImageFormat,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Drive1541DirectoryEntry {
    pub name: String,
    pub file_type: String,
    pub blocks: u16,
}

impl Drive1541Disk {
    #[must_use]
    pub fn image_bytes(&self) -> &[u8] {
        &self.image_bytes
    }

    #[must_use]
    pub fn disk_name(&self) -> &str {
        &self.disk_name
    }

    #[must_use]
    pub fn disk_id(&self) -> &str {
        &self.disk_id
    }

    #[must_use]
    pub const fn write_protected(&self) -> bool {
        self.write_protected
    }

    #[must_use]
    pub fn directory_entries(&self) -> &[Drive1541DirectoryEntry] {
        &self.directory_entries
    }
}

#[derive(Clone, Copy)]
pub struct Drive1541Config<'a> {
    pub dos_rom: &'a [u8],
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Drive1541InitError {
    #[error("expected 1541 DOS ROM of {expected:#06X} bytes, got {actual:#06X}")]
    InvalidRomSize { expected: usize, actual: usize },
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Drive1541MediaError {
    #[error("invalid D64 media: {0}")]
    InvalidD64(#[from] D64ParseError),
    #[error("invalid G64 media: {0}")]
    InvalidG64(#[from] G64ParseError),
}

impl Drive1541 {
    /// Constructs a new 1541 board from one 16KB DOS ROM image.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM size is not exactly 16KB.
    pub fn new(config: Drive1541Config<'_>) -> Result<Self, Drive1541InitError> {
        if config.dos_rom.len() != ROM_SIZE {
            return Err(Drive1541InitError::InvalidRomSize {
                expected: ROM_SIZE,
                actual: config.dos_rom.len(),
            });
        }

        let mut rom = [0u8; ROM_SIZE];
        rom.copy_from_slice(config.dos_rom);

        let mut cpu = M6502::new();
        cpu.reset();

        Ok(Self {
            cpu,
            via1: Via6522::new(),
            via2: Via6522::new(),
            ram: [0; RAM_SIZE],
            rom,
            disk: None,
            track_data: None,
            device_number: DEFAULT_DEVICE_NUMBER,
            head_position: INITIAL_HEAD_POSITION,
            stepper_phase: 0x03,
            motor_on: false,
            activity_led: false,
            density_code: 0,
            gcr_read: 0x11,
            gcr_write_value: 0,
            gcr_head_offset: 0,
            last_read_data: 0,
            bit_counter: 0,
            weak_bit_lfsr: WEAK_BIT_SEED,
            write_bit_index: 0,
            write_shift: 0,
            sync_active: false,
            byte_ready_level: false,
            byte_ready_edge: false,
            byte_ready_delay_ref_cycles: 0,
            sync_event_count: 0,
            byte_ready_event_count: 0,
            rotation_accum: 0,
            rotation_ref_phase: 0,
            recent_io_writes: Vec::new(),
            cycles: 0,
        })
    }

    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    #[must_use]
    pub const fn via1(&self) -> &Via6522 {
        &self.via1
    }

    #[must_use]
    pub const fn via2(&self) -> &Via6522 {
        &self.via2
    }

    #[must_use]
    pub const fn cycles(&self) -> u64 {
        self.cycles
    }

    #[must_use]
    pub const fn device_number(&self) -> u8 {
        self.device_number
    }

    /// Sets the IEC device number (8-11). The drive derives its bus address
    /// from this on every tick, so it takes effect immediately.
    pub const fn set_device_number(&mut self, device_number: u8) {
        self.device_number = device_number;
    }

    #[must_use]
    pub const fn head_position(&self) -> u8 {
        self.head_position
    }

    #[must_use]
    pub const fn motor_on(&self) -> bool {
        self.motor_on
    }

    #[must_use]
    pub const fn activity_led(&self) -> bool {
        self.activity_led
    }

    #[must_use]
    pub const fn density_code(&self) -> u8 {
        self.density_code
    }

    #[must_use]
    pub const fn gcr_read(&self) -> u8 {
        self.gcr_read
    }

    #[must_use]
    pub const fn byte_ready(&self) -> bool {
        self.byte_ready_level
    }

    #[must_use]
    pub fn sync_detected(&self) -> bool {
        !self.sync_not_detected()
    }

    #[must_use]
    pub const fn sync_event_count(&self) -> u64 {
        self.sync_event_count
    }

    #[must_use]
    pub const fn byte_ready_event_count(&self) -> u64 {
        self.byte_ready_event_count
    }

    #[must_use]
    pub const fn disk(&self) -> Option<&Drive1541Disk> {
        self.disk.as_ref()
    }

    #[must_use]
    pub fn recent_io_writes(&self) -> &[Drive1541IoWriteEvent] {
        &self.recent_io_writes
    }

    #[must_use]
    pub const fn disk_inserted(&self) -> bool {
        self.disk.is_some()
    }

    /// Loads one decoded `D64` image into the drive, **write-protected**.
    ///
    /// This is the default for archive media: a SAVE to a disk mounted this way
    /// yields the authentic `?WRITE PROTECT ERROR`, and the host image is never
    /// altered. Mount writable with [`load_d64_bytes_writable`] for a work disk.
    /// See `knowledge/decisions/disk-save-write-back.md`.
    ///
    /// # Errors
    ///
    /// Returns an error if the `D64` image is malformed.
    pub fn load_d64_bytes(&mut self, bytes: &[u8]) -> Result<(), Drive1541MediaError> {
        self.load_d64_bytes_writable(bytes, false)
    }

    /// Loads one decoded `D64` image, choosing whether the drive may write to it.
    ///
    /// `writable == false` clears the write-protect tab's *protection*: the ROM
    /// sees a protected disk and refuses to write. `writable == true` lets a
    /// SAVE lay GCR onto the surface; [`flush_image`](Self::flush_image) then
    /// decodes it back to D64 bytes for the host to persist.
    ///
    /// # Errors
    ///
    /// Returns an error if the `D64` image is malformed.
    pub fn load_d64_bytes_writable(
        &mut self,
        bytes: &[u8],
        writable: bool,
    ) -> Result<(), Drive1541MediaError> {
        let directory = parse_directory(bytes)?;
        self.track_data = Some(build_track_data(bytes)?);
        self.disk = Some(Drive1541Disk {
            image_bytes: bytes.to_vec(),
            disk_name: directory.disk_name,
            disk_id: directory.disk_id,
            write_protected: !writable,
            directory_entries: directory
                .entries
                .into_iter()
                .map(|entry| Drive1541DirectoryEntry {
                    name: entry.name,
                    file_type: d64_file_type_name(entry.file_type).to_owned(),
                    blocks: entry.blocks,
                })
                .collect(),
            image_format: Drive1541ImageFormat::D64,
        });
        self.reset_rotation_state();
        Ok(())
    }

    /// Loads a raw-GCR `G64` image read-only — the surface the drive head reads
    /// is the file's bytes verbatim, so copy-protection tricks the `D64` layer
    /// cannot represent (custom sync, non-standard sectors, fat/half-tracks,
    /// extra tracks, density, weak bits) survive. Mount writable with
    /// [`load_g64_bytes_writable`](Self::load_g64_bytes_writable) for a work disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the `G64` image is malformed.
    pub fn load_g64_bytes(&mut self, bytes: &[u8]) -> Result<(), Drive1541MediaError> {
        self.load_g64_bytes_writable(bytes, false)
    }

    /// Loads a raw-GCR `G64` image, choosing whether the drive may write to it.
    /// `writable == true` clears the write-protect tab so a fastloader/formatter
    /// SAVE can lay new GCR on the surface; [`flush_image`](Self::flush_image)
    /// then re-serialises the modified surface back to `G64` bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the `G64` image is malformed.
    pub fn load_g64_bytes_writable(
        &mut self,
        bytes: &[u8],
        writable: bool,
    ) -> Result<(), Drive1541MediaError> {
        let image = format_commodore_c64_g64::parse(bytes)?;
        self.track_data = Some(build_track_data_from_g64(&image));
        self.disk = Some(Drive1541Disk {
            image_bytes: bytes.to_vec(),
            // A G64 carries no decoded directory; the on-disk one is reached by
            // the DOS reading track 18 like any other. Leave the metadata blank.
            disk_name: String::new(),
            disk_id: String::new(),
            write_protected: !writable,
            directory_entries: Vec::new(),
            image_format: Drive1541ImageFormat::G64,
        });
        self.reset_rotation_state();
        Ok(())
    }

    /// Decodes the live GCR track surface back into a `D64` image.
    ///
    /// A SAVE lands GCR on the rotating surface in write mode; this turns the
    /// whole surface back into 256-byte sectors so the host can persist it.
    /// Unwritten tracks round-trip to their original bytes, so decoding the
    /// entire disk is safe; a track that fails to decode keeps its prior bytes.
    /// Returns `None` when no disk is mounted.
    #[must_use]
    pub fn flush_image(&self) -> Option<Vec<u8>> {
        let disk = self.disk.as_ref()?;
        // A G64 has no sector layout to decode back to; re-serialise the live raw
        // GCR surface (with the mounted image's speed zones) straight to G64.
        if disk.image_format == Drive1541ImageFormat::G64 {
            return self.flush_g64_image(disk);
        }
        let track_data = self.track_data.as_ref()?;
        let mut image = disk.image_bytes.clone();

        for track in 1..=35u8 {
            let Some(raw) = track_data.track_bytes(track * 2) else {
                continue;
            };
            let Ok(sectors) = sectors_in_track(track) else {
                continue;
            };
            for sector in 0..sectors {
                if let Some(data) = gcr_read_sector_from_raw_track(raw, sector) {
                    let _ = write_sector(&mut image, track, sector, &data);
                }
            }
        }

        Some(image)
    }

    /// Re-serialises the live raw-GCR surface of a mounted `G64` back to `G64`
    /// bytes: the mounted image supplies version/speed/geometry, and each
    /// present slot's GCR is replaced with the live surface (a written track
    /// diverges from the original; unwritten ones reproduce it).
    fn flush_g64_image(&self, disk: &Drive1541Disk) -> Option<Vec<u8>> {
        // Only a writable work disk persists; a read-only original (the common
        // protected case) has nothing to write back.
        if disk.write_protected {
            return None;
        }
        let track_data = self.track_data.as_ref()?;
        let mut image = format_commodore_c64_g64::parse(&disk.image_bytes).ok()?;
        for (slot, half_track) in image.half_tracks.iter_mut().enumerate() {
            if let (Some(half_track), Some(live)) = (half_track, track_data.tracks.get(slot))
                && !live.is_empty()
            {
                half_track.gcr.clone_from(live);
            }
        }
        // A track that can't be represented (over-long / too many half-tracks)
        // yields no write-back rather than a corrupt image — unreachable for a
        // valid C64 surface.
        format_commodore_c64_g64::write(&image).ok()
    }

    pub fn eject_disk(&mut self) {
        self.disk = None;
        self.track_data = None;
        self.reset_rotation_state();
    }

    #[must_use]
    pub fn snapshot_state(&self) -> Drive1541Snapshot {
        Drive1541Snapshot {
            cpu: self.cpu.clone(),
            via1: self.via1.clone(),
            via2: self.via2.clone(),
            ram: self.ram.to_vec(),
            rom: self.rom.to_vec(),
            disk: self.disk.clone(),
            device_number: self.device_number,
            head_position: self.head_position,
            stepper_phase: self.stepper_phase,
            motor_on: self.motor_on,
            activity_led: self.activity_led,
            density_code: self.density_code,
            gcr_read: self.gcr_read,
            gcr_write_value: self.gcr_write_value,
            gcr_head_offset: self.gcr_head_offset,
            last_read_data: self.last_read_data,
            bit_counter: self.bit_counter,
            weak_bit_lfsr: self.weak_bit_lfsr,
            sync_active: self.sync_active,
            byte_ready_level: self.byte_ready_level,
            byte_ready_edge: self.byte_ready_edge,
            byte_ready_delay_ref_cycles: self.byte_ready_delay_ref_cycles,
            sync_event_count: self.sync_event_count,
            byte_ready_event_count: self.byte_ready_event_count,
            rotation_accum: self.rotation_accum,
            rotation_ref_phase: self.rotation_ref_phase,
            cycles: self.cycles,
        }
    }

    /// Restores a board from a serialized snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the snapshot contains the wrong RAM or ROM sizes.
    pub fn restore_snapshot_state(&mut self, snapshot: Drive1541Snapshot) -> Result<(), String> {
        if snapshot.ram.len() != RAM_SIZE {
            return Err(format!(
                "1541 snapshot RAM size mismatch: expected {RAM_SIZE:#06X} bytes, got {:#06X}",
                snapshot.ram.len()
            ));
        }

        if snapshot.rom.len() != ROM_SIZE {
            return Err(format!(
                "1541 snapshot ROM size mismatch: expected {ROM_SIZE:#06X} bytes, got {:#06X}",
                snapshot.rom.len()
            ));
        }

        self.cpu = snapshot.cpu;
        self.via1 = snapshot.via1;
        self.via2 = snapshot.via2;
        self.ram.copy_from_slice(&snapshot.ram);
        self.rom.copy_from_slice(&snapshot.rom);
        self.disk = snapshot.disk;
        self.track_data = rebuild_track_data(self.disk.as_ref())
            .map_err(|err| format!("1541 snapshot disk rebuild failed: {err}"))?;
        self.device_number = snapshot.device_number;
        self.head_position = snapshot.head_position;
        self.stepper_phase = snapshot.stepper_phase;
        self.motor_on = snapshot.motor_on;
        self.activity_led = snapshot.activity_led;
        self.density_code = snapshot.density_code;
        self.gcr_read = snapshot.gcr_read;
        self.gcr_write_value = snapshot.gcr_write_value;
        self.gcr_head_offset = snapshot.gcr_head_offset;
        self.last_read_data = snapshot.last_read_data;
        self.bit_counter = snapshot.bit_counter;
        self.weak_bit_lfsr = snapshot.weak_bit_lfsr;
        self.sync_active = snapshot.sync_active;
        self.byte_ready_level = snapshot.byte_ready_level;
        self.byte_ready_edge = snapshot.byte_ready_edge;
        self.byte_ready_delay_ref_cycles = snapshot.byte_ready_delay_ref_cycles;
        self.sync_event_count = snapshot.sync_event_count;
        self.byte_ready_event_count = snapshot.byte_ready_event_count;
        self.rotation_accum = snapshot.rotation_accum;
        self.rotation_ref_phase = snapshot.rotation_ref_phase;
        self.recent_io_writes.clear();
        self.normalize_head_offset();
        self.cycles = snapshot.cycles;
        Ok(())
    }

    /// Rebuilds a 1541 board from a serialized snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the snapshot contains the wrong RAM or ROM sizes.
    pub fn from_snapshot(snapshot: Drive1541Snapshot) -> Result<Self, String> {
        if snapshot.ram.len() != RAM_SIZE {
            return Err(format!(
                "1541 snapshot RAM size mismatch: expected {RAM_SIZE:#06X} bytes, got {:#06X}",
                snapshot.ram.len()
            ));
        }

        if snapshot.rom.len() != ROM_SIZE {
            return Err(format!(
                "1541 snapshot ROM size mismatch: expected {ROM_SIZE:#06X} bytes, got {:#06X}",
                snapshot.rom.len()
            ));
        }

        let mut ram = [0u8; RAM_SIZE];
        ram.copy_from_slice(&snapshot.ram);

        let mut rom = [0u8; ROM_SIZE];
        rom.copy_from_slice(&snapshot.rom);

        Ok(Self {
            cpu: snapshot.cpu,
            via1: snapshot.via1,
            via2: snapshot.via2,
            ram,
            rom,
            disk: snapshot.disk,
            track_data: None,
            device_number: snapshot.device_number,
            head_position: snapshot.head_position,
            stepper_phase: snapshot.stepper_phase,
            motor_on: snapshot.motor_on,
            activity_led: snapshot.activity_led,
            density_code: snapshot.density_code,
            gcr_read: snapshot.gcr_read,
            gcr_write_value: snapshot.gcr_write_value,
            gcr_head_offset: snapshot.gcr_head_offset,
            last_read_data: snapshot.last_read_data,
            bit_counter: snapshot.bit_counter,
            weak_bit_lfsr: snapshot.weak_bit_lfsr,
            write_bit_index: 0,
            write_shift: 0,
            sync_active: snapshot.sync_active,
            byte_ready_level: snapshot.byte_ready_level,
            byte_ready_edge: snapshot.byte_ready_edge,
            byte_ready_delay_ref_cycles: snapshot.byte_ready_delay_ref_cycles,
            sync_event_count: snapshot.sync_event_count,
            byte_ready_event_count: snapshot.byte_ready_event_count,
            rotation_accum: snapshot.rotation_accum,
            rotation_ref_phase: snapshot.rotation_ref_phase,
            recent_io_writes: Vec::new(),
            cycles: snapshot.cycles,
        })
        .and_then(|mut machine| {
            machine.track_data = rebuild_track_data(machine.disk.as_ref())
                .map_err(|err| format!("1541 snapshot disk rebuild failed: {err}"))?;
            machine.normalize_head_offset();
            Ok(machine)
        })
    }

    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x17FF => self.ram[usize::from(addr & 0x07FF)],
            0x1800..=0x18FF if (addr & 0x0F) == 0x00 => self.via1_port_b_read(None),
            0x1800..=0x18FF if matches!(addr & 0x0F, 0x01 | 0x0F) => self.via1_port_a_read(),
            0x1800..=0x18FF => self.via1.peek((addr & 0x0F) as u8),
            0x1C00..=0x1CFF if (addr & 0x0F) == 0x00 => self.via2_port_b_read(),
            0x1C00..=0x1CFF if matches!(addr & 0x0F, 0x01 | 0x0F) => self.via2_port_a_read(),
            0x1C00..=0x1CFF => self.via2.peek((addr & 0x0F) as u8),
            0xC000..=0xFFFF => self.rom[usize::from(addr - 0xC000)],
            _ => 0xFF,
        }
    }

    pub fn poke(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x17FF => self.ram[usize::from(addr & 0x07FF)] = value,
            0x1800..=0x18FF => {
                self.via1.write((addr & 0x0F) as u8, value);
                self.record_io_write(addr, value);
            }
            0x1C00..=0x1CFF => {
                let reg = (addr & 0x0F) as u8;
                self.via2.write(reg, value);
                self.after_via2_write(reg, value);
                self.record_io_write(addr, value);
            }
            0xC000..=0xFFFF => {}
            _ => {}
        }
    }

    pub fn tick(&mut self) -> bool {
        self.apply_drive_inputs(None);
        self.cpu.irq = self.via1.irq || self.via2.irq;

        if self.cpu.rw {
            self.cpu.data_in = self.read_without_iec_bus(self.cpu.addr);
        } else {
            self.write_without_iec_bus(self.cpu.addr, self.cpu.data);
        }

        self.apply_byte_ready_overflow();
        self.cpu.so = true;
        let completed = self.cpu.tick();
        self.cpu.so = true;
        self.apply_drive_inputs(None);
        self.via1.tick();
        self.via2.tick();
        self.refresh_drive_mechanics();
        self.cycles += 1;
        self.finish_cycle_rotation();
        self.apply_drive_inputs(None);
        completed
    }

    pub fn tick_with_iec_bus(&mut self, bus: &mut IecBus) -> bool {
        self.apply_drive_inputs(Some(bus));
        self.cpu.irq = self.via1.irq || self.via2.irq;

        if self.cpu.rw {
            self.cpu.data_in = self.read_with_iec_bus(self.cpu.addr, bus);
        } else {
            self.write_with_iec_bus(self.cpu.addr, self.cpu.data, bus);
        }

        self.apply_byte_ready_overflow();
        self.cpu.so = true;
        let completed = self.cpu.tick();
        self.cpu.so = true;
        self.apply_drive_inputs(Some(bus));
        self.via1.tick();
        self.via2.tick();
        self.refresh_drive_mechanics();
        self.cycles += 1;
        self.finish_cycle_rotation();
        self.apply_drive_inputs(Some(bus));
        completed
    }

    #[must_use]
    pub fn peek_with_iec_bus(&self, addr: u16, bus: &IecBus) -> u8 {
        match addr {
            0x0000..=0x17FF => self.ram[usize::from(addr & 0x07FF)],
            0x1800..=0x18FF if (addr & 0x0F) == 0x00 => self.via1_port_b_read(Some(bus)),
            0x1800..=0x18FF if matches!(addr & 0x0F, 0x01 | 0x0F) => self.via1_port_a_read(),
            0x1800..=0x18FF => self.via1.peek((addr & 0x0F) as u8),
            0x1C00..=0x1CFF if (addr & 0x0F) == 0x00 => self.via2_port_b_read(),
            0x1C00..=0x1CFF if matches!(addr & 0x0F, 0x01 | 0x0F) => self.via2_port_a_read(),
            0x1C00..=0x1CFF => self.via2.peek((addr & 0x0F) as u8),
            0xC000..=0xFFFF => self.rom[usize::from(addr - 0xC000)],
            _ => 0xFF,
        }
    }

    pub fn read_with_iec_bus(&mut self, addr: u16, bus: &IecBus) -> u8 {
        match addr {
            0x0000..=0x17FF => self.ram[usize::from(addr & 0x07FF)],
            0x1800..=0x18FF if (addr & 0x0F) == 0x00 => {
                let value = self.via1_port_b_read(Some(bus));
                self.via1.read_port_b_with_value(value)
            }
            0x1800..=0x18FF if matches!(addr & 0x0F, 0x01 | 0x0F) => {
                let value = self.via1_port_a_read();
                self.via1.read_port_a_with_value(value)
            }
            0x1800..=0x18FF => self.via1.read((addr & 0x0F) as u8),
            0x1C00..=0x1CFF if (addr & 0x0F) == 0x00 => {
                self.rotate_disk_bus_read();
                let value = self.via2_port_b_read();
                self.via2.read_port_b_with_value(value)
            }
            0x1C00..=0x1CFF if matches!(addr & 0x0F, 0x01 | 0x0F) => {
                self.rotate_disk_bus_read();
                let value = self.via2_port_a_read();
                self.clear_byte_ready_level();
                self.via2.read_port_a_with_value(value)
            }
            0x1C00..=0x1CFF => self.via2.read((addr & 0x0F) as u8),
            0xC000..=0xFFFF => self.rom[usize::from(addr - 0xC000)],
            _ => 0xFF,
        }
    }

    pub fn write_with_iec_bus(&mut self, addr: u16, value: u8, bus: &mut IecBus) {
        match addr {
            0x0000..=0x17FF => self.ram[usize::from(addr & 0x07FF)] = value,
            0x1800..=0x18FF => {
                self.via1.write((addr & 0x0F) as u8, value);
                self.record_io_write(addr, value);
                self.drive_iec_outputs(bus);
            }
            0x1C00..=0x1CFF => {
                let reg = (addr & 0x0F) as u8;
                self.via2.write(reg, value);
                self.after_via2_write(reg, value);
                self.record_io_write(addr, value);
            }
            0xC000..=0xFFFF => {}
            _ => {}
        }
    }

    pub fn sync_iec_bus(&mut self, bus: &mut IecBus) {
        self.apply_drive_inputs(Some(bus));
    }

    fn drive_iec_outputs(&self, bus: &mut IecBus) {
        // VICE stores the 1541 IEC contribution from the VIA1 Port B mixed
        // output state (`PRB | ~DDRB`), so input-configured bits release the
        // open-collector lines high immediately when DDR changes.
        bus.write_drive_port_b(self.device_number, self.via1.port_b_drive_state());
    }

    fn apply_drive_inputs(&mut self, bus: Option<&IecBus>) {
        self.via1.pa_in = self.via1_port_a_input();
        self.via1.pb_in = self.via1_port_b_input(bus);
        // The 1541 serial glue presents IEC ATN to VIA1 CA1 inverted: ATN low
        // becomes a CA1 rising edge, matching VICE's `viacore_signal(...,
        // VIA_SIG_CA1, VIA_SIG_RISE)` path for 1541-style drives.
        self.via1.set_ca1_level(!self.bus_atn_high(bus));
        self.via2.pa_in = self.via2_port_a_input();
        self.via2.pb_in = self.via2_port_b_input();
        self.via2.ca1 = self.byte_ready_not_asserted();
    }

    fn refresh_drive_mechanics(&mut self) {
        let was_motor_on = self.motor_on;
        let port_b = self.via2.port_b_drive_state();
        let new_stepper_position = port_b & 0x03;
        let old_stepper_position = self.head_position.saturating_sub(2) & 0x03;
        let step_count = new_stepper_position.wrapping_sub(old_stepper_position) & 0x03;

        self.motor_on = port_b & 0x04 != 0;
        self.activity_led = port_b & 0x08 != 0;
        self.density_code = (port_b >> 5) & 0x03;

        if self.motor_on {
            match step_count {
                1 => {
                    self.head_position =
                        self.head_position.saturating_add(1).min(MAX_HEAD_POSITION);
                }
                3 => {
                    self.head_position = self.head_position.saturating_sub(1);
                }
                _ => {}
            }
        }

        if !self.motor_on && was_motor_on {
            self.clear_byte_ready();
            self.last_read_data = 0;
            self.bit_counter = 0;
            self.sync_active = false;
            self.rotation_accum = 0;
            self.rotation_ref_phase = 0;
        } else if self.motor_on && !was_motor_on {
            self.rotation_accum = 0;
            self.rotation_ref_phase = 0;
        }

        self.normalize_head_offset();
        self.stepper_phase = new_stepper_position;
    }

    fn read_without_iec_bus(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x17FF => self.ram[usize::from(addr & 0x07FF)],
            0x1800..=0x18FF if (addr & 0x0F) == 0x00 => {
                let value = self.via1_port_b_read(None);
                self.via1.read_port_b_with_value(value)
            }
            0x1800..=0x18FF if matches!(addr & 0x0F, 0x01 | 0x0F) => {
                let value = self.via1_port_a_read();
                self.via1.read_port_a_with_value(value)
            }
            0x1800..=0x18FF => self.via1.read((addr & 0x0F) as u8),
            0x1C00..=0x1CFF if (addr & 0x0F) == 0x00 => {
                self.rotate_disk_bus_read();
                let value = self.via2_port_b_read();
                self.via2.read_port_b_with_value(value)
            }
            0x1C00..=0x1CFF if matches!(addr & 0x0F, 0x01 | 0x0F) => {
                self.rotate_disk_bus_read();
                let value = self.via2_port_a_read();
                self.clear_byte_ready_level();
                self.via2.read_port_a_with_value(value)
            }
            0x1C00..=0x1CFF => self.via2.read((addr & 0x0F) as u8),
            0xC000..=0xFFFF => self.rom[usize::from(addr - 0xC000)],
            _ => 0xFF,
        }
    }

    fn write_without_iec_bus(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x17FF => self.ram[usize::from(addr & 0x07FF)] = value,
            0x1800..=0x18FF => {
                self.via1.write((addr & 0x0F) as u8, value);
                self.record_io_write(addr, value);
            }
            0x1C00..=0x1CFF => {
                let reg = (addr & 0x0F) as u8;
                self.via2.write(reg, value);
                self.after_via2_write(reg, value);
                self.record_io_write(addr, value);
            }
            0xC000..=0xFFFF => {}
            _ => {}
        }
    }

    fn via1_port_a_read(&self) -> u8 {
        self.via1.compose_port_a_read(self.via1_port_a_input())
    }

    fn via1_port_b_read(&self, bus: Option<&IecBus>) -> u8 {
        (((self.via1.orb() & 0x1A) | self.via1_bus_port(bus)) ^ 0x85)
            | (self.device_select_bits() << 5)
    }

    fn via2_port_a_read(&self) -> u8 {
        self.via2.compose_port_a_read(self.via2_port_a_input())
    }

    fn via2_port_b_read(&self) -> u8 {
        self.via2.compose_port_b_read(self.via2_port_b_input())
    }

    fn via1_port_a_input(&self) -> u8 {
        // On a plain 1541, VIA1 Port A is not the track-zero/byte-ready status
        // port used by the dual-drive DOS heritage. VICE models it as pulled
        // high unless parallel-cable hardware is active, which we do not yet
        // emulate here.
        0xFF
    }

    fn via1_port_b_input(&self, bus: Option<&IecBus>) -> u8 {
        self.via1_bus_port(bus)
    }

    fn via2_port_a_input(&self) -> u8 {
        if self.selected_internal_drive_present() {
            self.gcr_read
        } else {
            0
        }
    }

    fn via2_port_b_input(&self) -> u8 {
        let mut value = 0x6F;
        if self.sync_not_detected() {
            value |= 0x80;
        }
        if self.write_protect_not_asserted() {
            value |= 0x10;
        }
        value
    }

    fn device_select_bits(&self) -> u8 {
        self.device_number.saturating_sub(DEFAULT_DEVICE_NUMBER) & 0x03
    }

    fn via1_bus_port(&self, bus: Option<&IecBus>) -> u8 {
        let mut value = 0;
        if self.bus_data_high(bus) {
            value |= 0x01;
        }
        if self.bus_clock_high(bus) {
            value |= 0x04;
        }
        if self.bus_atn_high(bus) {
            value |= 0x80;
        }
        value
    }

    fn bus_atn_high(&self, bus: Option<&IecBus>) -> bool {
        bus.is_none_or(IecBus::drive_atn_high)
    }

    fn bus_clock_high(&self, bus: Option<&IecBus>) -> bool {
        bus.is_none_or(|bus| bus.drive_port() & 0x04 != 0)
    }

    fn bus_data_high(&self, bus: Option<&IecBus>) -> bool {
        bus.is_none_or(|bus| bus.drive_port() & 0x01 != 0)
    }

    fn after_via2_write(&mut self, reg: u8, value: u8) {
        match reg & 0x0F {
            0x00 => {
                self.refresh_drive_mechanics();
                self.clear_byte_ready_level();
            }
            0x01 | 0x0F => {
                self.gcr_write_value = value;
                self.clear_byte_ready_level();
            }
            0x02 => self.refresh_drive_mechanics(),
            0x0C if !self.is_read_mode() || !self.byte_ready_active() => {
                self.clear_byte_ready();
            }
            _ => {}
        }
    }

    fn byte_ready_not_asserted(&self) -> bool {
        !(self.byte_ready_active() && (self.byte_ready_level || self.byte_ready_edge))
    }

    fn apply_byte_ready_overflow(&mut self) {
        if self.byte_ready_edge && self.byte_ready_active() {
            self.cpu.regs.set_flag(FLAG_V, true);
            self.byte_ready_edge = false;
        }
    }

    fn sync_not_detected(&self) -> bool {
        !self.is_read_mode() || !self.sync_active
    }

    fn write_protect_not_asserted(&self) -> bool {
        // The write-protect photocell reads "not protected" whenever light
        // reaches it: through a writable disk's notch, AND through an empty
        // drive (no media to block the beam). Only a write-protect tab — a
        // mounted, protected disk — asserts the line. Reporting an empty drive
        // as *protected* would make mounting a writable disk a phantom WP
        // transition, which the DOS reads as a disk change and uses to slam
        // every open channel shut (ROM $F9AD sets $1C → $EC54 JSR $D313) —
        // breaking a SAVE onto a disk inserted after power-up. Matches VICE
        // drive-writeprotect.c ("No disk in drive, write protection is off").
        if !self.selected_internal_drive_present() {
            return true;
        }
        self.disk
            .as_ref()
            .is_none_or(|disk| !disk.write_protected())
    }

    fn clear_byte_ready(&mut self) {
        self.byte_ready_level = false;
        self.byte_ready_edge = false;
        self.byte_ready_delay_ref_cycles = 0;
    }

    fn clear_byte_ready_level(&mut self) {
        self.byte_ready_level = false;
    }

    fn reset_rotation_state(&mut self) {
        self.gcr_read = 0x11;
        self.gcr_write_value = 0;
        self.gcr_head_offset = 0;
        self.last_read_data = 0;
        self.bit_counter = 0;
        self.write_bit_index = 0;
        self.write_shift = 0;
        self.sync_active = false;
        self.byte_ready_level = false;
        self.byte_ready_edge = false;
        self.byte_ready_delay_ref_cycles = 0;
        self.sync_event_count = 0;
        self.byte_ready_event_count = 0;
        self.rotation_accum = 0;
        self.rotation_ref_phase = 0;
        self.recent_io_writes.clear();
    }

    fn record_io_write(&mut self, addr: u16, value: u8) {
        if self.recent_io_writes.len() == IO_TRACE_LIMIT {
            self.recent_io_writes.remove(0);
        }
        self.recent_io_writes.push(Drive1541IoWriteEvent {
            cycle: self.cycles,
            pc: self.cpu.regs.pc,
            addr,
            value,
        });
    }

    fn normalize_head_offset(&mut self) {
        let total_bits = self.current_track_bit_len();
        if total_bits == 0 {
            self.gcr_head_offset = 0;
        } else {
            self.gcr_head_offset %= total_bits;
        }
    }

    fn rotate_disk_bus_read(&mut self) {
        // The 1541 read path pays this bus delay in addition to the normal
        // CPU-cycle rotation budget, not instead of it.
        self.advance_rotation_ref_cycles(BUS_READ_DELAY_REF_CYCLES);
    }

    fn finish_cycle_rotation(&mut self) {
        self.advance_rotation_ref_cycles(ROTATION_REF_CYCLES_PER_CPU_CYCLE);
        self.rotation_ref_phase = 0;
    }

    fn advance_rotation_ref_cycles(&mut self, ref_cycles: u64) {
        // The disk spins whenever the motor is on, in read *or* write mode.
        // Read mode assembles bytes off the surface; write mode lays the write
        // latch onto it. (Previously rotation was gated to read mode only, so
        // SAVE never reached the surface.)
        if ref_cycles == 0 || !self.motor_on {
            return;
        }

        let bits_per_second = READ_BITS_PER_SECOND_BY_ZONE[usize::from(self.density_code)];
        let ref_hz = DRIVE1541_CPU_HZ * ROTATION_REF_CYCLES_PER_CPU_CYCLE;
        let mut remaining = ref_cycles;

        while remaining > 0 {
            let to_next_bit = self.ref_cycles_until_next_bit(bits_per_second, ref_hz);
            let to_byte_ready = if self.byte_ready_delay_ref_cycles == 0 {
                u64::MAX
            } else {
                u64::from(self.byte_ready_delay_ref_cycles)
            };
            let step = remaining.min(to_next_bit.min(to_byte_ready));
            debug_assert!(step > 0);

            self.rotation_accum = self
                .rotation_accum
                .saturating_add(bits_per_second.saturating_mul(step));
            self.rotation_ref_phase = self
                .rotation_ref_phase
                .saturating_add(u8::try_from(step).unwrap_or(u8::MAX));
            self.advance_byte_ready_delay_ref_cycles(step);
            remaining -= step;

            if self.rotation_accum >= ref_hz {
                self.rotation_accum -= ref_hz;
                self.rotate_one_track_bit();
            }
        }
    }

    fn ref_cycles_until_next_bit(&self, bits_per_second: u64, ref_hz: u64) -> u64 {
        let remaining = ref_hz.saturating_sub(self.rotation_accum);
        remaining.div_ceil(bits_per_second).max(1)
    }

    fn advance_byte_ready_delay_ref_cycles(&mut self, ref_cycles: u64) {
        if self.byte_ready_delay_ref_cycles == 0 {
            return;
        }

        if ref_cycles >= u64::from(self.byte_ready_delay_ref_cycles) {
            self.byte_ready_delay_ref_cycles = 0;
            self.byte_ready_level = true;
            self.byte_ready_edge = true;
            self.byte_ready_event_count += 1;
        } else {
            self.byte_ready_delay_ref_cycles -= ref_cycles as u8;
        }
    }

    fn schedule_byte_ready(&mut self, edge_phase: u8) {
        if !self.byte_ready_active() {
            return;
        }
        let _ = edge_phase;
        self.byte_ready_delay_ref_cycles = 0;
        self.byte_ready_level = true;
        self.byte_ready_edge = true;
        self.byte_ready_event_count += 1;
    }

    fn rotate_one_track_bit(&mut self) {
        let total_bits = self.current_track_bit_len();
        if total_bits == 0 {
            return;
        }

        self.gcr_head_offset += 1;
        if self.gcr_head_offset >= total_bits {
            self.gcr_head_offset = 0;
        }

        if !self.is_read_mode() {
            self.write_one_track_bit();
            return;
        }

        // Reading holds the write serialiser at bit 0 and keeps it pre-loaded
        // with the current latch, so the next write phase starts on a byte
        // boundary aligned with the ROM's first latched byte.
        self.write_bit_index = 0;
        self.write_shift = self.gcr_write_value;

        let bit = self.next_read_bit(self.gcr_head_offset);

        self.last_read_data = ((self.last_read_data << 1) | u16::from(bit)) & 0x03FF;
        let sync_now = self.last_read_data == 0x03FF;
        if sync_now {
            if !self.sync_active {
                self.sync_event_count += 1;
            }
            self.sync_active = true;
            self.bit_counter = 0;
            return;
        }

        self.sync_active = false;
        self.bit_counter = self.bit_counter.wrapping_add(1);
        if self.bit_counter == 8 {
            self.bit_counter = 0;
            self.gcr_read = self.last_read_data as u8;
            self.schedule_byte_ready(self.rotation_ref_phase.saturating_sub(1));
        }
    }

    /// Lays one bit of the write serialiser onto the surface at the head
    /// position, MSB first, then shifts the serialiser left. After eight bits a
    /// byte has been written, so the serialiser reloads from the `gcr_write_value`
    /// port latch and byte-ready pulses to make the ROM's write loop feed the
    /// next byte — the write-mode mirror of the read path's byte assembly.
    ///
    /// The serialiser is a latch-fed shift register, not a live index into
    /// `gcr_write_value`: the ROM's write loop runs one byte ahead, storing the
    /// next byte into the latch partway through the current byte's emission.
    /// Reading the latch live would splice that next byte into the current one
    /// and fail the drive's read-after-write verify; loading it only at the byte
    /// boundary keeps each byte intact. Writes are dropped on a protected disk.
    fn write_one_track_bit(&mut self) {
        let writable = self
            .disk
            .as_ref()
            .is_some_and(|disk| !disk.write_protected());
        if writable {
            let bit = (self.write_shift >> 7) & 0x01;
            let offset = self.gcr_head_offset;
            let head = self.head_position;
            if let Some(track) = self
                .track_data
                .as_mut()
                .and_then(|data| data.track_bytes_mut(head))
            {
                let byte_index = offset / 8;
                let bit_index = 7 - (offset & 0x07);
                if bit != 0 {
                    track[byte_index] |= 1 << bit_index;
                } else {
                    track[byte_index] &= !(1 << bit_index);
                }
            }
        }

        self.write_shift <<= 1;
        self.write_bit_index = (self.write_bit_index + 1) & 0x07;
        if self.write_bit_index == 0 {
            self.write_shift = self.gcr_write_value;
            self.schedule_byte_ready(self.rotation_ref_phase.saturating_sub(1));
        }
    }

    /// Reads the next surface bit, substituting random flux over a weak byte.
    ///
    /// A `0x00` GCR byte cannot occur in valid GCR (no code has eight zero bits),
    /// so on a real disk it marks an unformatted/no-flux area. The read head
    /// picks up noise there, differing every revolution — which is exactly what a
    /// copy-protection weak-bit check reads twice and requires to differ. The
    /// LFSR advances per weak bit, so consecutive passes read differently, while
    /// ordinary (non-zero) GCR reads back bit-exact.
    fn next_read_bit(&mut self, bit_offset: usize) -> u8 {
        if self.current_track_byte(bit_offset) == Some(0) {
            self.weak_bit_lfsr = self
                .weak_bit_lfsr
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            return (self.weak_bit_lfsr >> 31) as u8;
        }
        self.current_track_bit(bit_offset)
    }

    fn current_track_byte(&self, bit_offset: usize) -> Option<u8> {
        self.current_track_bytes()
            .and_then(|track| track.get(bit_offset / 8).copied())
    }

    fn current_track_bit(&self, bit_offset: usize) -> u8 {
        let Some(track) = self.current_track_bytes() else {
            return 0;
        };

        let byte_index = bit_offset / 8;
        let bit_index = 7 - (bit_offset & 0x07);
        u8::from(track[byte_index] & (1 << bit_index) != 0)
    }

    fn current_track_bit_len(&self) -> usize {
        self.current_track_bytes()
            .map_or(0, |track| track.len() * 8)
    }

    fn current_track_bytes(&self) -> Option<&[u8]> {
        if !self.selected_internal_drive_present() {
            return None;
        }
        self.track_data.as_ref()?.track_bytes(self.head_position)
    }

    fn selected_internal_drive(&self) -> u8 {
        self.ram[0x007F] & 0x01
    }

    fn selected_internal_drive_present(&self) -> bool {
        self.selected_internal_drive() == 0
    }

    fn is_read_mode(&self) -> bool {
        self.cb2_line_high()
    }

    fn byte_ready_active(&self) -> bool {
        self.ca2_line_high()
    }

    fn ca2_line_high(&self) -> bool {
        if self.via2.ca2_drive {
            self.via2.ca2_out
        } else {
            self.via2.peek(0x0C) & 0x02 != 0
        }
    }

    fn cb2_line_high(&self) -> bool {
        if self.via2.cb2_drive {
            self.via2.cb2_out
        } else {
            self.via2.peek(0x0C) & 0x20 != 0
        }
    }
}

impl Drive1541TrackData {
    fn track_bytes(&self, head_position: u8) -> Option<&[u8]> {
        let slot = track_slot_index(head_position)?;
        let track = self.tracks.get(slot)?;
        if track.is_empty() { None } else { Some(track) }
    }

    fn track_bytes_mut(&mut self, head_position: u8) -> Option<&mut [u8]> {
        let slot = track_slot_index(head_position)?;
        let track = self.tracks.get_mut(slot)?;
        if track.is_empty() {
            None
        } else {
            Some(track.as_mut_slice())
        }
    }
}

fn rebuild_track_data(
    disk: Option<&Drive1541Disk>,
) -> Result<Option<Drive1541TrackData>, Drive1541MediaError> {
    disk.map(|disk| match disk.image_format {
        Drive1541ImageFormat::D64 => Ok(build_track_data(disk.image_bytes())?),
        Drive1541ImageFormat::G64 => {
            let image = format_commodore_c64_g64::parse(disk.image_bytes())?;
            Ok(build_track_data_from_g64(&image))
        }
    })
    .transpose()
}

/// Builds the live track surface directly from a parsed `G64`: each half-track's
/// raw GCR drops into the matching slot (the G64 half-track index is the drive's
/// own slot index). Half-tracks beyond the drive's reachable range are dropped.
fn build_track_data_from_g64(image: &G64Image) -> Drive1541TrackData {
    let mut tracks = vec![Vec::new(); TRACK_SLOT_COUNT];
    for (slot, track) in image.half_tracks.iter().enumerate().take(TRACK_SLOT_COUNT) {
        if let Some(track) = track {
            tracks[slot] = track.gcr.clone();
        }
    }
    Drive1541TrackData { tracks }
}

fn build_track_data(bytes: &[u8]) -> Result<Drive1541TrackData, D64ParseError> {
    let bam = read_sector(bytes, 18, 0)?;
    let id1 = bam[0xA2];
    let id2 = bam[0xA3];
    let mut tracks = vec![Vec::new(); TRACK_SLOT_COUNT];
    let mut track_offset = 0usize;

    for track in 1..=35u8 {
        let zone = speed_zone_for_track(track);
        let track_size = RAW_TRACK_SIZE_BY_ZONE[usize::from(zone)];
        let sectors = usize::from(sectors_in_track(track)?);
        let sector_size = SECTOR_GCR_SIZE_WITH_HEADER
            + HEADER_GAP_SIZE
            + GAP_SIZE_BY_ZONE[usize::from(zone)]
            + (SYNC_SIZE * 2);
        let mut temp = vec![0x55; track_size];
        let gap_size = GAP_SIZE_BY_ZONE[usize::from(zone)];

        for sector in 0..sectors {
            let offset = sector * sector_size;
            encode_sector_to_gcr(
                read_sector(bytes, track, sector as u8)?,
                &mut temp[offset..offset + sector_size],
                GcrHeader {
                    sector: sector as u8,
                    track,
                    id2,
                    id1,
                },
                gap_size,
            );
        }

        track_offset += (sectors * sector_size).saturating_sub(gap_size);
        track_offset += (track_size * 100) / 270;
        track_offset %= track_size;

        let mut raw = vec![0x55; track_size];
        raw[track_offset..].copy_from_slice(&temp[..track_size - track_offset]);
        raw[..track_offset].copy_from_slice(&temp[track_size - track_offset..]);

        let slot = usize::from((track * 2) - 2);
        tracks[slot] = raw;
    }

    Ok(Drive1541TrackData { tracks })
}

const fn d64_file_type_name(kind: D64FileType) -> &'static str {
    match kind {
        D64FileType::Del => "DEL",
        D64FileType::Seq => "SEQ",
        D64FileType::Prg => "PRG",
        D64FileType::Usr => "USR",
        D64FileType::Rel => "REL",
        D64FileType::Unknown(_) => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_DEVICE_NUMBER, Drive1541, Drive1541Config, Drive1541InitError, Drive1541TrackData,
        IO_TRACE_LIMIT, MAX_HEAD_POSITION, RAM_SIZE, ROM_SIZE, TRACK_SLOT_COUNT, build_track_data,
        d64_file_type_name, gcr_read_sector_from_raw_track, track_slot_index,
    };
    use common_commodore_iec::IecBus;
    use format_commodore_c64_d64::{D64FileType, read_sector};

    const D64_STANDARD_SIZE: usize = 174_848;
    const D64_SECTOR_SIZE: usize = 256;

    /// Builds a minimal valid G64 (84 half-tracks) with raw GCR on the given
    /// slots. `slot` is the drive half-track index (0 = track 1, head pos 2).
    fn minimal_g64(slot_tracks: &[(usize, Vec<u8>)]) -> Vec<u8> {
        let num_half = 84usize;
        let max_len = 7928u16;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GCR-1541");
        buf.push(0);
        buf.push(num_half as u8);
        buf.extend_from_slice(&max_len.to_le_bytes());
        let data_base = 12 + num_half * 4 + num_half * 4;
        let mut offsets = vec![0u32; num_half];
        let mut speeds = vec![0u32; num_half];
        let mut data = Vec::new();
        for (slot, gcr) in slot_tracks {
            offsets[*slot] = (data_base + data.len()) as u32;
            speeds[*slot] = 3;
            data.extend_from_slice(&(gcr.len() as u16).to_le_bytes());
            data.extend_from_slice(gcr);
        }
        for o in &offsets {
            buf.extend_from_slice(&o.to_le_bytes());
        }
        for s in &speeds {
            buf.extend_from_slice(&s.to_le_bytes());
        }
        buf.extend_from_slice(&data);
        buf
    }

    #[test]
    fn load_g64_fills_slots_read_only_and_survives_snapshot() {
        // A distinctive non-standard-length GCR stream on track 1 (slot 0).
        let pattern: Vec<u8> = (0..300u16).map(|i| (i % 256) as u8).collect();
        let g64 = minimal_g64(&[(0, pattern.clone())]);

        let rom = make_rom(&[], 0xEB22);
        let mut drive = Drive1541::new(Drive1541Config { dos_rom: &rom[..] }).expect("valid ROM");
        drive.load_g64_bytes(&g64).expect("valid G64 mounts");

        assert!(drive.disk_inserted(), "G64 disk should be inserted");
        assert!(
            drive.flush_image().is_none(),
            "a read-only G64 mount has nothing to flush"
        );
        // The raw GCR lands verbatim in slot 0 (head position 2), at its exact
        // non-standard length — the head wraps at gcr.len().
        let track = drive
            .track_data
            .as_ref()
            .expect("track data")
            .track_bytes(2);
        assert_eq!(track, Some(&pattern[..]));

        // A snapshot re-parses the G64 on restore (no D64 to rebuild from), so
        // the raw surface is identical.
        let restored = Drive1541::from_snapshot(drive.snapshot_state()).expect("snapshot restores");
        assert_eq!(
            restored
                .track_data
                .as_ref()
                .expect("track data")
                .track_bytes(2),
            Some(&pattern[..])
        );
    }

    #[test]
    fn weak_zero_bytes_read_as_random_flux() {
        // A track that is all 0x00 GCR — a fully weak (no-flux) region. Valid
        // GCR never contains 0x00, so this is unambiguously a weak marker.
        let g64 = minimal_g64(&[(0, vec![0x00u8; 16])]);
        let rom = make_rom(&[], 0xEB22);
        let mut drive = Drive1541::new(Drive1541Config { dos_rom: &rom[..] }).expect("valid ROM");
        drive.load_g64_bytes(&g64).expect("valid G64 mounts");
        drive.head_position = 2; // track 1 (slot 0)

        // Read 128 bits over the weak region. A plain 0x00 read would be all
        // zeros; weak flux gives a 0/1 mix, so both values must appear — this is
        // what makes a two-read copy-protection weak-bit check see differing data.
        let ones: u32 = (0..128usize)
            .map(|offset| u32::from(drive.next_read_bit(offset)))
            .sum();
        assert!(
            (20..108).contains(&ones),
            "weak reads should be a 0/1 mix, got {ones}/128 ones"
        );
    }

    #[test]
    fn writable_g64_flushes_the_modified_surface_back_to_g64() {
        use format_commodore_c64_g64::parse as parse_g64;

        let original: Vec<u8> = vec![0x55u8; 200];
        let g64 = minimal_g64(&[(0, original.clone())]);
        let rom = make_rom(&[], 0xEB22);
        let mut drive = Drive1541::new(Drive1541Config { dos_rom: &rom[..] }).expect("valid ROM");
        drive
            .load_g64_bytes_writable(&g64, true)
            .expect("writable G64 mounts");

        // Overwrite track 1's live surface (as a fastloader SAVE would).
        let written: Vec<u8> = (0..200u16).map(|i| (i % 256) as u8).collect();
        drive.track_data.as_mut().expect("track data").tracks[0].copy_from_slice(&written);

        // Flush re-serialises the modified surface; re-parsing recovers it.
        let flushed = drive.flush_image().expect("writable G64 flushes");
        let reparsed = parse_g64(&flushed).expect("flushed G64 re-parses");
        assert_eq!(
            reparsed.half_tracks[0].as_ref().expect("track 1").gcr,
            written
        );
    }

    fn make_rom(program: &[(u16, &[u8])], reset_vector: u16) -> [u8; ROM_SIZE] {
        let mut rom = [0xEA; ROM_SIZE];
        for (addr, bytes) in program {
            let start = usize::from(*addr - 0xC000);
            rom[start..start + bytes.len()].copy_from_slice(bytes);
        }
        let vector = 0xFFFCusize - 0xC000usize;
        rom[vector] = reset_vector as u8;
        rom[vector + 1] = (reset_vector >> 8) as u8;
        rom
    }

    fn boot(machine: &mut Drive1541) {
        // 6502 reset is a 7-cycle sequence: step until the CPU is
        // instruction-complete and sync-pin is asserted.
        for _ in 0..7 {
            if machine.tick() && machine.cpu().instruction_complete() {
                break;
            }
        }
        assert!(machine.cpu().instruction_complete());
        assert!(machine.cpu().sync);
    }

    fn run_one(machine: &mut Drive1541) -> u64 {
        let before = machine.cycles();
        loop {
            let completed = machine.tick();
            if completed && machine.cpu().instruction_complete() {
                break;
            }
        }
        machine.cycles() - before
    }

    fn d64_linear_sector_index(track: u8, sector_num: u8) -> usize {
        const TRACK_SECTOR_COUNTS: [u8; 35] = [
            21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 19, 19, 19, 19, 19,
            19, 19, 18, 18, 18, 18, 18, 18, 17, 17, 17, 17, 17,
        ];
        TRACK_SECTOR_COUNTS[..usize::from(track - 1)]
            .iter()
            .map(|&count| usize::from(count))
            .sum::<usize>()
            + usize::from(sector_num)
    }

    fn write_d64_sector(
        bytes: &mut [u8],
        track: u8,
        sector_num: u8,
        sector: &[u8; D64_SECTOR_SIZE],
    ) {
        let offset = d64_linear_sector_index(track, sector_num) * D64_SECTOR_SIZE;
        bytes[offset..offset + D64_SECTOR_SIZE].copy_from_slice(sector);
    }

    fn synthetic_d64() -> Vec<u8> {
        let mut bytes = vec![0u8; D64_STANDARD_SIZE];

        let mut bam = [0u8; D64_SECTOR_SIZE];
        bam[0] = 18;
        bam[1] = 1;
        bam[0x90..0x98].copy_from_slice(b"DEMO DIS");
        bam[0x98] = b'K';
        bam[0xA2..0xA4].copy_from_slice(b"42");
        write_d64_sector(&mut bytes, 18, 0, &bam);

        let mut directory = [0u8; D64_SECTOR_SIZE];
        directory[2] = 0x82;
        directory[3] = 1;
        directory[4] = 0;
        directory[5..10].copy_from_slice(b"HELLO");
        directory[30..32].copy_from_slice(&(1u16).to_le_bytes());
        write_d64_sector(&mut bytes, 18, 1, &directory);

        let mut file_sector = [0u8; D64_SECTOR_SIZE];
        file_sector[0] = 0;
        file_sector[1] = 6;
        file_sector[2..7].copy_from_slice(&[0x01, 0x08, 0x11, 0x22, 0x33]);
        write_d64_sector(&mut bytes, 1, 0, &file_sector);

        bytes
    }

    #[test]
    fn rejects_wrong_rom_size() {
        let err = match Drive1541::new(Drive1541Config { dos_rom: &[0; 1] }) {
            Ok(_) => panic!("unexpected success"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            Drive1541InitError::InvalidRomSize {
                expected: ROM_SIZE,
                actual: 1
            }
        );
    }

    #[test]
    fn reset_vector_boots_from_rom() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        boot(&mut machine);

        assert_eq!(machine.cpu().regs.pc, 0xC000);
        assert_eq!(run_one(&mut machine), 2);
        assert_eq!(machine.cpu().regs.pc, 0xC001);
    }

    #[test]
    fn ram_is_mirrored_through_low_8k_window() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        machine.poke(0x0002, 0x5A);

        assert_eq!(machine.peek(0x0802), 0x5A);
        assert_eq!(machine.peek(0x1002), 0x5A);
    }

    #[test]
    fn via_registers_are_decoded_and_mirrored() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        machine.poke(0x1802, 0xAA);
        machine.poke(0x1C03, 0x55);

        assert_eq!(machine.peek(0x1802), 0xAA);
        assert_eq!(machine.peek(0x18F2), 0xAA);
        assert_eq!(machine.peek(0x1C03), 0x55);
        assert_eq!(machine.peek(0x1CF3), 0x55);
    }

    #[test]
    fn cpu_can_write_through_board_to_via_space() {
        let rom = make_rom(&[(0xC000, &[0xA9, 0xFF, 0x8D, 0x02, 0x18, 0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        boot(&mut machine);
        assert_eq!(run_one(&mut machine), 2);
        assert_eq!(run_one(&mut machine), 4);

        assert_eq!(machine.peek(0x1802), 0xFF);
    }

    #[test]
    fn via1_port_b_read_reflects_iec_lines() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let mut bus = IecBus::new();

        machine.poke(0x1800, 0x1A);
        machine.sync_iec_bus(&mut bus);

        assert_eq!(machine.peek_with_iec_bus(0x1800, &bus), 0x1A);
    }

    #[test]
    fn via1_port_b_output_pulls_cpu_data_line_low() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let mut bus = IecBus::new();

        machine.write_with_iec_bus(0x1802, 0xFF, &mut bus);
        machine.write_with_iec_bus(0x1800, 0xF7, &mut bus);

        assert_eq!(bus.cpu_port() & 0x80, 0x00);
    }

    #[test]
    fn via1_port_b_input_bits_do_not_drive_iec_bus_low() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let mut bus = IecBus::new();

        machine.write_with_iec_bus(0x1802, 0x1A, &mut bus);
        machine.write_with_iec_bus(0x1800, 0x01, &mut bus);

        assert_eq!(bus.cpu_port() & 0xC0, 0xC0);
    }

    #[test]
    fn atn_falling_edge_reaches_via1_ca1_as_a_rising_edge() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let mut bus = IecBus::new();

        machine.poke(0x180C, 0x01);
        machine.sync_iec_bus(&mut bus);
        assert!(
            !machine.via1().ca1,
            "idle IEC ATN high should present CA1 low"
        );
        assert_eq!(machine.via1().peek(0x0D) & 0x02, 0x00);

        bus.write_cpu_port_a(0xF7);
        machine.sync_iec_bus(&mut bus);

        assert!(machine.via1().ca1, "C64 ATN low should present CA1 high");
        assert_eq!(
            machine.via1().peek(0x0D) & 0x02,
            0x02,
            "ATN low should reach VIA1 CA1 as the configured rising-edge interrupt"
        );
    }

    #[test]
    fn via1_port_a_reads_high_on_plain_1541() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let bus = IecBus::new();
        machine.head_position = 2;

        assert_eq!(
            machine.peek_with_iec_bus(0x1801, &bus),
            0xFF,
            "plain 1541 VIA1 Port A should read high without parallel hardware"
        );
    }

    #[test]
    fn via2_status_port_reflects_write_protect_and_sync() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let bus = IecBus::new();
        machine.head_position = 2;
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");

        assert_eq!(
            machine.peek_with_iec_bus(0x1C00, &bus) & 0x10,
            0x00,
            "write-protect sense should pull VIA2 PB4 low for mounted read-only media"
        );
        assert!(
            machine.peek_with_iec_bus(0x1C00, &bus) & 0x80 != 0,
            "sync should stay inactive while the spindle is stopped"
        );
    }

    #[test]
    fn halftrack_positions_map_back_to_whole_d64_tracks() {
        assert_eq!(track_slot_index(0), None);
        assert_eq!(track_slot_index(1), None);
        assert_eq!(track_slot_index(2), Some(0));
        assert_eq!(track_slot_index(3), Some(1));
        assert_eq!(track_slot_index(34), Some(32));
        assert_eq!(track_slot_index(35), Some(33));
        assert_eq!(track_slot_index(36), Some(34));
        assert_eq!(track_slot_index(70), Some(68));
        assert_eq!(track_slot_index(71), Some(69));
        assert_eq!(track_slot_index(72), Some(70));
        assert_eq!(track_slot_index(84), Some(82));
        assert_eq!(track_slot_index(85), None);
    }

    #[test]
    fn via2_outputs_drive_motor_led_and_head_motion() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        machine.head_position = 10;
        machine.stepper_phase = 0;

        machine.poke(0x1C02, 0x7F);
        machine.poke(0x1C00, 0x0C);
        machine.tick();
        machine.poke(0x1C00, 0x0D);
        machine.tick();

        assert!(machine.motor_on());
        assert!(machine.activity_led());
        assert_eq!(machine.density_code(), 0);
        assert_eq!(machine.head_position(), 11);
    }

    #[test]
    fn via2_pcr_controls_read_mode_and_byte_ready_enable() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        machine.poke(0x1C0C, 0x22);
        assert!(machine.is_read_mode());
        assert!(machine.byte_ready_active());

        machine.poke(0x1C0C, 0xEC);
        assert!(machine.is_read_mode());
        assert!(
            !machine.byte_ready_active(),
            "PCR bit 1 disables byte-ready even when CA2 output state is high"
        );
    }

    #[test]
    fn drive_can_mount_d64_and_report_directory() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");

        let disk = machine.disk().expect("disk should be inserted");
        assert_eq!(disk.disk_name, "DEMO DISK");
        assert_eq!(disk.disk_id, "42");
        assert!(disk.write_protected());
        assert_eq!(disk.directory_entries.len(), 1);
        assert_eq!(disk.directory_entries[0].name, "HELLO");
        assert_eq!(disk.directory_entries[0].file_type, "PRG");
        assert_eq!(disk.directory_entries[0].blocks, 1);
    }

    #[test]
    fn mounted_d64_produces_byte_ready_and_gcr_data_in_read_mode() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");
        machine.head_position = 2;
        machine.poke(0x1C02, 0x7F);
        machine.poke(0x1C00, 0x04);
        machine.poke(0x1C0C, 0x22);

        let mut saw_byte_ready = false;
        for _ in 0..512 {
            machine.tick();
            saw_byte_ready |= machine.byte_ready();
            if saw_byte_ready {
                break;
            }
        }

        assert!(
            saw_byte_ready,
            "mounted D64 should eventually latch a byte-ready edge in read mode"
        );
        assert_ne!(machine.gcr_read(), 0x11);
    }

    #[test]
    fn mounted_d64_leaves_odd_halftracks_unformatted() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");
        machine.head_position = 2;
        assert!(machine.current_track_bytes().is_some());
        machine.head_position = 3;
        assert!(
            machine.current_track_bytes().is_none(),
            "odd halftracks should stay unformatted for mounted D64 media"
        );
    }

    #[test]
    fn write_mode_lays_the_latch_byte_onto_the_surface() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        machine
            .load_d64_bytes_writable(&synthetic_d64(), true)
            .expect("writable mount should succeed");
        machine.head_position = 2;

        // Serialise 0xAB MSB-first across the eight bit cells of track byte 1.
        // The serialiser is loaded from the port latch at a byte boundary, so
        // seed it here as that boundary load would.
        machine.gcr_write_value = 0xAB;
        machine.write_shift = 0xAB;
        machine.write_bit_index = 0;
        for offset in 8..16usize {
            machine.gcr_head_offset = offset;
            machine.write_one_track_bit();
        }

        let raw = machine
            .track_data
            .as_ref()
            .expect("track data present")
            .track_bytes(2)
            .expect("track 1 present");
        assert_eq!(raw[1], 0xAB, "the latch byte should land on the surface");
    }

    #[test]
    fn protected_disk_drops_writes() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("default (protected) mount should succeed");
        machine.head_position = 2;

        let before = machine
            .track_data
            .as_ref()
            .expect("track data present")
            .track_bytes(2)
            .expect("track 1 present")
            .to_vec();

        machine.gcr_write_value = 0xFF;
        machine.write_bit_index = 0;
        for offset in 8..16usize {
            machine.gcr_head_offset = offset;
            machine.write_one_track_bit();
        }

        let after = machine
            .track_data
            .as_ref()
            .expect("track data present")
            .track_bytes(2)
            .expect("track 1 present");
        assert_eq!(after, before.as_slice(), "a protected disk must not change");
    }

    #[test]
    fn flush_image_round_trips_an_unwritten_disk() {
        // Decoding the whole GCR surface back to D64 must reproduce the mounted
        // image byte-for-byte — the property that makes whole-disk flush safe.
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let bytes = synthetic_d64();
        machine
            .load_d64_bytes_writable(&bytes, true)
            .expect("synthetic D64 should mount writable");

        let flushed = machine.flush_image().expect("a mounted disk should flush");
        assert_eq!(flushed, bytes, "unwritten disk must round-trip exactly");
    }

    #[test]
    fn writable_mount_clears_write_protect() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("default mount should succeed");
        assert!(
            machine
                .disk
                .as_ref()
                .expect("disk mounted")
                .write_protected(),
            "default mount is write-protected (archive-safe)"
        );

        machine
            .load_d64_bytes_writable(&synthetic_d64(), true)
            .expect("writable mount should succeed");
        assert!(
            !machine
                .disk
                .as_ref()
                .expect("disk mounted")
                .write_protected(),
            "writable mount drops write protection"
        );
    }

    #[test]
    fn mounted_d64_gcr_track_round_trips_sector_zero() {
        let bytes = synthetic_d64();
        let track_data = build_track_data(&bytes).expect("synthetic D64 should build GCR data");
        let raw_track = track_data
            .track_bytes(2)
            .expect("track 1 should be present");
        let sector = gcr_read_sector_from_raw_track(raw_track, 0)
            .expect("raw GCR track should decode sector 0");
        let expected = read_sector(&bytes, 1, 0).expect("synthetic D64 sector should exist");

        assert_eq!(sector.as_slice(), expected);
    }

    #[test]
    fn live_read_path_produces_varying_gcr_bytes() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let bus = IecBus::new();
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");
        machine.head_position = 2;
        machine.poke(0x1C02, 0x7F);
        machine.poke(0x1C00, 0x04);
        machine.poke(0x1C0C, 0x22);

        let mut seen = std::collections::BTreeSet::new();
        let mut reads = 0usize;
        for _ in 0..20_000 {
            machine.tick();
            if machine.byte_ready() {
                seen.insert(machine.read_with_iec_bus(0x1C01, &bus));
                reads += 1;
                if reads >= 32 {
                    break;
                }
            }
        }

        assert_eq!(reads, 32, "drive should produce enough byte-ready reads");
        assert!(
            seen.len() > 1,
            "live 1541 read path should not collapse to one repeated GCR byte"
        );
    }

    #[test]
    fn reading_via2_port_a_clears_byte_ready() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let bus = IecBus::new();
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");
        machine.head_position = 2;
        machine.poke(0x1C02, 0x7F);
        machine.poke(0x1C00, 0x04);
        machine.poke(0x1C0C, 0x22);

        for _ in 0..512 {
            machine.tick();
            if machine.byte_ready() {
                break;
            }
        }

        assert!(
            machine.byte_ready(),
            "track should eventually assert byte ready"
        );
        let _ = machine.read_with_iec_bus(0x1C01, &bus);
        assert!(
            !machine.byte_ready(),
            "reading VIA2 Port A should clear byte ready"
        );
    }

    #[test]
    fn reading_via2_port_b_does_not_clear_byte_ready() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let bus = IecBus::new();
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");
        machine.head_position = 2;
        machine.poke(0x1C02, 0x7F);
        machine.poke(0x1C00, 0x04);
        machine.poke(0x1C0C, 0x22);

        for _ in 0..512 {
            machine.tick();
            if machine.byte_ready() {
                break;
            }
        }

        assert!(
            machine.byte_ready(),
            "track should eventually assert byte ready"
        );
        let _ = machine.read_with_iec_bus(0x1C00, &bus);
        assert!(
            machine.byte_ready(),
            "reading VIA2 Port B should not clear byte ready"
        );
    }

    #[test]
    fn byte_ready_asserts_only_after_scheduled_delay() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        machine.byte_ready_delay_ref_cycles = 11;
        machine.advance_byte_ready_delay_ref_cycles(10);
        assert!(!machine.byte_ready_level);
        assert!(!machine.byte_ready_edge);
        assert_eq!(machine.byte_ready_event_count, 0);

        machine.advance_byte_ready_delay_ref_cycles(1);
        assert!(machine.byte_ready_level);
        assert!(machine.byte_ready_edge);
        assert_eq!(machine.byte_ready_event_count, 1);
    }

    #[test]
    fn pending_byte_ready_edge_sets_cpu_overflow_when_enabled() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        machine.poke(0x1C0C, 0x22);
        machine
            .cpu
            .regs
            .set_flag(mos_6502::registers::FLAG_V, false);
        machine.byte_ready_edge = true;
        machine.apply_byte_ready_overflow();

        assert!(machine.cpu.regs.overflow());
        assert!(!machine.byte_ready_edge);
    }

    #[test]
    fn sync_from_decoder_shift_resets_bit_counter_without_clearing_byte_ready() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        machine.poke(0x1C0C, 0x22); // PCR: CB2 high = read mode
        machine.track_data = Some(Drive1541TrackData {
            tracks: vec![vec![0xFF]; TRACK_SLOT_COUNT],
        });
        machine.head_position = 2;
        machine.last_read_data = 0x01FF;
        machine.bit_counter = 7;
        machine.byte_ready_level = true;
        machine.byte_ready_edge = true;
        machine.byte_ready_delay_ref_cycles = 7;

        machine.rotate_one_track_bit();

        assert!(machine.sync_active);
        assert_eq!(machine.bit_counter, 0);
        assert!(machine.byte_ready_level);
        assert!(machine.byte_ready_edge);
        assert_eq!(machine.byte_ready_delay_ref_cycles, 7);
    }

    #[test]
    fn decoder_consumes_the_next_bit_after_head_advance() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        machine.track_data = Some(Drive1541TrackData {
            tracks: vec![vec![0x80]; TRACK_SLOT_COUNT],
        });
        machine.head_position = 2;
        machine.gcr_head_offset = 0;
        machine.last_read_data = 0;

        machine.rotate_one_track_bit();

        assert_eq!(
            machine.last_read_data, 0,
            "the first rotated bit should come from the next on-disk bit position"
        );
        assert_eq!(machine.gcr_head_offset, 1);
    }

    #[test]
    fn snapshot_round_trip_preserves_drive_state() {
        let rom = make_rom(&[(0xC000, &[0xA9, 0x34, 0x8D, 0x00, 0x04])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        boot(&mut machine);
        assert_eq!(run_one(&mut machine), 2);
        assert_eq!(run_one(&mut machine), 4);
        machine.write_with_iec_bus(0x1802, 0xFF, &mut IecBus::new());
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");

        let snapshot = machine.snapshot_state();
        let restored = Drive1541::from_snapshot(snapshot).expect("1541 snapshot should round-trip");

        assert_eq!(restored.cpu().regs, machine.cpu().regs);
        assert_eq!(restored.cpu().addr, machine.cpu().addr);
        assert_eq!(restored.cpu().rw, machine.cpu().rw);
        assert_eq!(restored.cpu().sync, machine.cpu().sync);
        assert_eq!(restored.via1().pa, machine.via1().pa);
        assert_eq!(restored.via1().pb, machine.via1().pb);
        assert_eq!(restored.via2().pa, machine.via2().pa);
        assert_eq!(restored.via2().pb, machine.via2().pb);
        assert_eq!(restored.peek(0x0400), machine.peek(0x0400));
        assert_eq!(restored.cycles(), machine.cycles());
        assert_eq!(restored.device_number(), machine.device_number());
        assert_eq!(restored.head_position(), machine.head_position());
        assert_eq!(restored.motor_on(), machine.motor_on());
        assert_eq!(restored.activity_led(), machine.activity_led());
        assert_eq!(restored.density_code(), machine.density_code());
        assert!(restored.disk_inserted());
        assert_eq!(
            restored.disk().expect("disk should be restored").disk_name,
            "DEMO DISK"
        );
    }

    // ----- accessors and small helpers -----

    #[test]
    fn drive1541_disk_accessor_methods_expose_inner_state() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let bytes = synthetic_d64();
        machine
            .load_d64_bytes(&bytes)
            .expect("synthetic D64 should mount");

        let disk = machine.disk().expect("disk should be inserted");
        assert_eq!(disk.disk_name(), "DEMO DISK");
        assert_eq!(disk.disk_id(), "42");
        assert_eq!(disk.image_bytes().len(), bytes.len());
        let entries = disk.directory_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "HELLO");
    }

    #[test]
    fn sync_detected_and_event_count_accessors_initial_state() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        // Drive idle: no sync detected, no events recorded yet.
        assert!(!machine.sync_detected());
        assert_eq!(machine.sync_event_count(), 0);
        assert_eq!(machine.byte_ready_event_count(), 0);
        assert!(machine.recent_io_writes().is_empty());
    }

    #[test]
    fn d64_file_type_name_covers_every_arm() {
        assert_eq!(d64_file_type_name(D64FileType::Del), "DEL");
        assert_eq!(d64_file_type_name(D64FileType::Seq), "SEQ");
        assert_eq!(d64_file_type_name(D64FileType::Prg), "PRG");
        assert_eq!(d64_file_type_name(D64FileType::Usr), "USR");
        assert_eq!(d64_file_type_name(D64FileType::Rel), "REL");
        assert_eq!(d64_file_type_name(D64FileType::Unknown(7)), "UNKNOWN");
    }

    #[test]
    fn track_slot_index_returns_none_outside_supported_range() {
        // Below the lower bound and above the upper bound both return `None`.
        assert!(track_slot_index(1).is_none());
        assert!(track_slot_index(MAX_HEAD_POSITION + 1).is_none());
    }

    // ----- eject + reset/rotation state -----

    #[test]
    fn eject_disk_drops_image_and_resets_rotation_state() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");
        assert!(machine.disk_inserted());

        machine.gcr_head_offset = 100;
        machine.byte_ready_level = true;

        machine.eject_disk();

        assert!(!machine.disk_inserted());
        assert!(machine.disk().is_none());
        assert!(machine.track_data.is_none());
        assert_eq!(machine.gcr_head_offset, 0);
        assert!(!machine.byte_ready_level);
    }

    // ----- peek/poke fall-through ranges -----

    #[test]
    fn peek_returns_rom_byte_and_open_bus_for_unmapped_addresses() {
        let mut rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        rom[0x1234] = 0xAB; // ROM offset 0x1234 -> address 0xD234
        let machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        assert_eq!(machine.peek(0xD234), 0xAB);
        // Anything outside the mapped windows reads as open bus (0xFF).
        assert_eq!(machine.peek(0x2000), 0xFF);
        assert_eq!(machine.peek(0xBFFF), 0xFF);
    }

    #[test]
    fn poke_to_rom_and_unmapped_ranges_is_ignored() {
        let mut rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        rom[0x1234] = 0xAB;
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        machine.poke(0xD234, 0x55);
        machine.poke(0x2000, 0x77);
        machine.poke(0xBFFF, 0x99);

        assert_eq!(machine.peek(0xD234), 0xAB);
    }

    // ----- read_without_iec_bus + write_without_iec_bus full coverage -----

    #[test]
    fn cpu_read_through_via1_register_paths_via_tick() {
        // Program: LDA $1800; LDA $1801; LDA $1802; LDA $1900; NOP loop (read VIA1 PB,
        // PA, the DDRB register, and the mirrored Port-B address through the CPU bus).
        let rom = make_rom(
            &[(
                0xC000,
                &[
                    0xAD, 0x00, 0x18, // LDA $1800 (port B with via1 read path)
                    0xAD, 0x01, 0x18, // LDA $1801 (port A with via1 read path)
                    0xAD, 0x02, 0x18, // LDA $1802 (other VIA1 register)
                    0xAD, 0x00, 0x19, // LDA $1900 (mirror of $1800 via decode)
                    0xEA,
                ],
            )],
            0xC000,
        );
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        boot(&mut machine);
        for _ in 0..4 {
            run_one(&mut machine);
        }

        // We don't assert on the exact value — we just need each VIA1 read branch
        // of `read_without_iec_bus` to execute. Make sure execution made progress.
        assert!(machine.cpu().regs.pc >= 0xC00C);
    }

    #[test]
    fn cpu_read_through_via2_register_paths_via_tick() {
        // LDA $1C00; LDA $1C01; LDA $1C02 — covers the VIA2 PB, PA, and other-reg
        // arms of `read_without_iec_bus`.
        let rom = make_rom(
            &[(
                0xC000,
                &[
                    0xAD, 0x00, 0x1C, // LDA $1C00
                    0xAD, 0x01, 0x1C, // LDA $1C01
                    0xAD, 0x02, 0x1C, // LDA $1C02 (DDRB)
                    0xEA,
                ],
            )],
            0xC000,
        );
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        boot(&mut machine);
        for _ in 0..3 {
            run_one(&mut machine);
        }

        assert!(machine.cpu().regs.pc >= 0xC009);
    }

    #[test]
    fn cpu_write_through_board_to_via2_space_uses_after_write_hook() {
        // STA $1C00 — ends up in the VIA2-port-b branch of write_without_iec_bus,
        // which calls after_via2_write and refresh_drive_mechanics.
        let rom = make_rom(
            &[(
                0xC000,
                &[
                    0xA9, 0x04, // LDA #$04 (motor on bit)
                    0x8D, 0x00, 0x1C, // STA $1C00
                    0xEA,
                ],
            )],
            0xC000,
        );
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        boot(&mut machine);
        run_one(&mut machine); // LDA #$04
        run_one(&mut machine); // STA $1C00

        assert!(machine.motor_on(), "writing motor bit should engage motor");
        // Recording also captured the I/O write.
        let writes = machine.recent_io_writes();
        assert!(writes.iter().any(|ev| ev.addr == 0x1C00));
    }

    #[test]
    fn cpu_write_to_rom_window_is_ignored_during_tick() {
        // STA $C500 — exercise the ROM-write fall-through branch in
        // write_without_iec_bus.
        let rom = make_rom(
            &[(
                0xC000,
                &[
                    0xA9, 0x55, // LDA #$55
                    0x8D, 0x00, 0xC5, // STA $C500
                    0xEA,
                ],
            )],
            0xC000,
        );
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        boot(&mut machine);
        run_one(&mut machine); // LDA
        run_one(&mut machine); // STA into ROM (ignored)

        assert_eq!(machine.peek(0xC500), 0xEA, "ROM should remain unchanged");
    }

    // ----- IEC-bus read/write/peek paths -----

    #[test]
    fn read_with_iec_bus_returns_ram_via1_via2_and_rom_bytes() {
        let mut rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        rom[0x2000] = 0x77; // ROM offset 0x2000 -> address 0xE000
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let bus = IecBus::new();

        machine.poke(0x0010, 0xAB);
        machine.poke(0x1802, 0xCD); // VIA1 DDRB
        machine.poke(0x1C03, 0xEF); // VIA2 DDRA

        // RAM
        assert_eq!(machine.read_with_iec_bus(0x0010, &bus), 0xAB);
        // VIA1 Port B (decode + iec bus path)
        let _ = machine.read_with_iec_bus(0x1800, &bus);
        // VIA1 Port A
        let _ = machine.read_with_iec_bus(0x1801, &bus);
        // VIA1 other register (DDRB)
        assert_eq!(machine.read_with_iec_bus(0x1802, &bus), 0xCD);
        // VIA2 other register (DDRA)
        assert_eq!(machine.read_with_iec_bus(0x1C03, &bus), 0xEF);
        // ROM
        assert_eq!(machine.read_with_iec_bus(0xE000, &bus), 0x77);
        // Open bus fall-through for unmapped address space.
        assert_eq!(machine.read_with_iec_bus(0x2000, &bus), 0xFF);
    }

    #[test]
    fn write_with_iec_bus_targets_ram_via2_rom_and_ignored_ranges() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let mut bus = IecBus::new();

        // RAM: should be writable through the IEC-aware write path.
        machine.write_with_iec_bus(0x0040, 0x12, &mut bus);
        assert_eq!(machine.peek(0x0040), 0x12);

        // VIA2 IO: writes go through after_via2_write.
        machine.write_with_iec_bus(0x1C02, 0x7F, &mut bus); // DDRB
        machine.write_with_iec_bus(0x1C00, 0x04, &mut bus); // motor on
        assert!(machine.motor_on());

        // ROM and unmapped: silently ignored.
        machine.write_with_iec_bus(0xC100, 0xFF, &mut bus);
        machine.write_with_iec_bus(0x2000, 0xFF, &mut bus);
        assert_eq!(machine.peek(0xC100), 0xEA);
    }

    #[test]
    fn peek_with_iec_bus_covers_via1_pa_via2_pa_and_other_paths() {
        let mut rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        rom[0x3000] = 0x42; // 0xF000
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        machine.head_position = 2;
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");
        let bus = IecBus::new();

        // VIA1 Port A (peek path with $1801).
        let _ = machine.peek_with_iec_bus(0x1801, &bus);
        // VIA1 other register (peek path).
        let _ = machine.peek_with_iec_bus(0x1802, &bus);
        // VIA2 Port A (peek path with $1C01).
        let _ = machine.peek_with_iec_bus(0x1C01, &bus);
        // VIA2 other register peek path ($1C03).
        let _ = machine.peek_with_iec_bus(0x1C03, &bus);
        // ROM
        assert_eq!(machine.peek_with_iec_bus(0xF000, &bus), 0x42);
        // Open bus
        assert_eq!(machine.peek_with_iec_bus(0x2000, &bus), 0xFF);
    }

    #[test]
    fn tick_with_iec_bus_advances_cpu_and_drive_state() {
        let rom = make_rom(&[(0xC000, &[0xEA, 0xEA, 0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let mut bus = IecBus::new();

        // Boot via the IEC-aware tick path so tick_with_iec_bus is exercised end-to-end.
        for _ in 0..7 {
            if machine.tick_with_iec_bus(&mut bus) && machine.cpu().instruction_complete() {
                break;
            }
        }
        let cycles_before = machine.cycles();

        // Run one NOP via the IEC-aware path.
        loop {
            let completed = machine.tick_with_iec_bus(&mut bus);
            if completed && machine.cpu().instruction_complete() {
                break;
            }
        }

        assert_eq!(machine.cycles() - cycles_before, 2);
    }

    #[test]
    fn tick_with_iec_bus_writes_through_to_via1_when_cpu_stores_to_iec_register() {
        // STA $1800 with IEC-aware tick — exercises the write branch and
        // drive_iec_outputs.
        let rom = make_rom(
            &[(
                0xC000,
                &[
                    0xA9, 0x00, // LDA #$00 (pull data line low when DDR enabled)
                    0x8D, 0x02, 0x18, // STA $1802 -> set DDRB to 0
                    0xEA,
                ],
            )],
            0xC000,
        );
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let mut bus = IecBus::new();

        for _ in 0..7 {
            if machine.tick_with_iec_bus(&mut bus) && machine.cpu().instruction_complete() {
                break;
            }
        }
        // Run LDA + STA via the IEC path.
        for _ in 0..2 {
            loop {
                let completed = machine.tick_with_iec_bus(&mut bus);
                if completed && machine.cpu().instruction_complete() {
                    break;
                }
            }
        }

        // The drive should have published an IEC contribution onto the bus.
        assert!(bus.drive_data(DEFAULT_DEVICE_NUMBER).is_some());
    }

    // ----- mechanics: motor off transition, head step backwards, density -----

    #[test]
    fn motor_off_transition_clears_byte_ready_and_rotation_state() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        machine.head_position = 10;

        // Engage motor first.
        machine.poke(0x1C02, 0x7F);
        machine.poke(0x1C00, 0x04);
        machine.tick();
        assert!(machine.motor_on());

        // Force some byte-ready / rotation state, then drop the motor bit.
        machine.byte_ready_level = true;
        machine.byte_ready_edge = true;
        machine.byte_ready_delay_ref_cycles = 5;
        machine.last_read_data = 0x1234;
        machine.bit_counter = 4;
        machine.sync_active = true;
        machine.rotation_accum = 1234;
        machine.rotation_ref_phase = 7;

        machine.poke(0x1C00, 0x00);
        machine.tick();

        assert!(!machine.motor_on());
        assert!(!machine.byte_ready_level);
        assert!(!machine.byte_ready_edge);
        assert_eq!(machine.byte_ready_delay_ref_cycles, 0);
        assert_eq!(machine.last_read_data, 0);
        assert_eq!(machine.bit_counter, 0);
        assert!(!machine.sync_active);
        assert_eq!(machine.rotation_accum, 0);
        assert_eq!(machine.rotation_ref_phase, 0);
    }

    #[test]
    fn step_phase_three_decrements_head_position() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        // Engage motor with stepper phase 0 (head_position 36 -> phase ((36-2)&3)=2,
        // so we land in a sensible neighbourhood). Then advance the head into a
        // place where we can step backwards.
        machine.head_position = 10;
        machine.stepper_phase = 0;
        machine.poke(0x1C02, 0x7F);
        machine.poke(0x1C00, 0x0C); // motor + new stepper phase 0 -> step_count 0
        machine.tick();
        machine.poke(0x1C00, 0x0D); // motor + stepper phase 1 -> +1
        machine.tick();
        assert_eq!(machine.head_position(), 11);

        // Now walk backwards: from stepper position 1, going to 0 gives step_count 3.
        machine.poke(0x1C00, 0x0C); // motor + stepper phase 0 -> step_count 3 (back)
        machine.tick();
        assert_eq!(
            machine.head_position(),
            10,
            "stepper phase delta of 3 should retract the head"
        );
    }

    #[test]
    fn density_code_field_reflects_via2_port_b_bits() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        machine.head_position = 10;

        machine.poke(0x1C02, 0xFF); // all VIA2 PB bits as outputs
        machine.poke(0x1C00, 0x60 | 0x04); // density bits (0b11), motor on
        machine.tick();

        assert_eq!(machine.density_code(), 0b11);
    }

    // ----- after_via2_write: $1C01/$1C0F path -----

    #[test]
    fn writing_via2_port_a_register_buffers_gcr_write_value() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        // Pre-charge byte-ready level so we can verify it is cleared.
        machine.byte_ready_level = true;
        machine.poke(0x1C01, 0xA5);

        assert_eq!(machine.gcr_write_value, 0xA5);
        assert!(!machine.byte_ready_level);

        // The mirrored register $1C0F also buffers the GCR value.
        machine.byte_ready_level = true;
        machine.poke(0x1C0F, 0x5A);
        assert_eq!(machine.gcr_write_value, 0x5A);
        assert!(!machine.byte_ready_level);
    }

    // ----- via2_port_a_input falls back when drive 1 is selected -----

    #[test]
    fn via2_port_a_input_returns_zero_when_other_drive_selected() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        // The 1541 ROM uses the LSB of $7F to pick between drive 0 and drive 1
        // in dual-drive heritage; selecting drive 1 makes the GCR data port read 0.
        machine.poke(0x007F, 0x01);
        machine.gcr_read = 0xAA;

        assert_eq!(machine.via2_port_a_input(), 0);
    }

    // ----- record_io_write trace ring buffer cap -----

    #[test]
    fn record_io_write_caps_at_io_trace_limit() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        // Write `IO_TRACE_LIMIT + 4` times into the same VIA register; the buffer
        // should grow until the cap and then start dropping the oldest entries.
        for value in 0..=(IO_TRACE_LIMIT as u16 + 4) {
            machine.poke(0x1802, value as u8);
        }

        assert_eq!(machine.recent_io_writes().len(), IO_TRACE_LIMIT);
        // The first surviving record should be later than the very first write.
        let first = &machine.recent_io_writes()[0];
        assert_eq!(first.addr, 0x1802);
        assert_ne!(first.value, 0);
    }

    // ----- byte ready / sync helpers under explicit pre-conditions -----

    #[test]
    fn schedule_byte_ready_no_op_when_byte_ready_disabled() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        // Disable byte-ready (PCR bit 1 = 0 keeps CA2 low through `ca2_line_high`).
        machine.poke(0x1C0C, 0x20); // CB2=high (read mode), CA2 manual low
        let count_before = machine.byte_ready_event_count();
        machine.byte_ready_level = false;
        machine.byte_ready_edge = false;

        machine.schedule_byte_ready(0);

        assert!(!machine.byte_ready_level);
        assert!(!machine.byte_ready_edge);
        assert_eq!(machine.byte_ready_event_count(), count_before);
    }

    #[test]
    fn rotate_one_track_bit_is_noop_when_no_track_bytes() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        // No track data inserted; total_bits == 0 => early return.
        machine.gcr_head_offset = 0;
        machine.last_read_data = 0;
        machine.rotate_one_track_bit();

        assert_eq!(machine.gcr_head_offset, 0);
        assert_eq!(machine.last_read_data, 0);
    }

    #[test]
    fn rotate_one_track_bit_wraps_head_offset_at_track_end() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        // Single-byte track: 8 bits total, so wrapping happens predictably.
        machine.track_data = Some(Drive1541TrackData {
            tracks: vec![vec![0x00]; TRACK_SLOT_COUNT],
        });
        machine.head_position = 2;
        // Place head at the last bit of the track; the next rotation must wrap to 0.
        machine.gcr_head_offset = 7;

        machine.rotate_one_track_bit();

        assert_eq!(machine.gcr_head_offset, 0);
    }

    // ----- selected_internal_drive_present: track selection blocked -----

    #[test]
    fn selecting_external_drive_hides_track_data_from_decoder() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");
        machine.head_position = 2;
        // Route the GCR head to the alternate drive selection path.
        machine.poke(0x007F, 0x01);

        assert!(machine.current_track_bytes().is_none());
        assert_eq!(machine.current_track_bit_len(), 0);
    }

    #[test]
    fn track_data_track_bytes_returns_none_for_empty_or_invalid_slot() {
        let bytes = synthetic_d64();
        let track_data = build_track_data(&bytes).expect("synthetic D64 should build GCR data");

        // Out of range head position -> None.
        assert!(track_data.track_bytes(1).is_none());

        // Even with a built dataset, an odd halftrack slot stays empty -> None.
        assert!(track_data.track_bytes(3).is_none());
    }

    // ----- snapshot/restore covering restore_snapshot_state -----

    #[test]
    fn restore_snapshot_state_repopulates_drive_in_place() {
        let rom = make_rom(&[(0xC000, &[0xA9, 0x12, 0x8D, 0x00, 0x04, 0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        boot(&mut machine);
        run_one(&mut machine); // LDA #$12
        run_one(&mut machine); // STA $0400
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");
        machine.head_position = 4;

        let snapshot = machine.snapshot_state();
        let snapshot_pc = machine.cpu().regs.pc;
        let snapshot_cycles = machine.cycles();

        // Disturb the live machine, then restore in place.
        let mut other = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        other
            .restore_snapshot_state(snapshot)
            .expect("snapshot should restore in place");

        assert_eq!(other.cpu().regs.pc, snapshot_pc);
        assert_eq!(other.cycles(), snapshot_cycles);
        assert!(other.disk_inserted());
        assert_eq!(other.head_position(), 4);
        assert_eq!(other.peek(0x0400), 0x12);
        // The recent IO trace is intentionally cleared on restore.
        assert!(other.recent_io_writes().is_empty());
    }

    #[test]
    fn restore_snapshot_state_rejects_wrong_ram_size() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let mut snapshot = machine.snapshot_state();
        snapshot.ram = vec![0u8; RAM_SIZE - 1];

        let err = machine
            .restore_snapshot_state(snapshot)
            .expect_err("undersized RAM should be rejected");
        assert!(err.contains("snapshot RAM size mismatch"));
    }

    #[test]
    fn restore_snapshot_state_rejects_wrong_rom_size() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let mut snapshot = machine.snapshot_state();
        snapshot.rom = vec![0u8; ROM_SIZE - 1];

        let err = machine
            .restore_snapshot_state(snapshot)
            .expect_err("undersized ROM should be rejected");
        assert!(err.contains("snapshot ROM size mismatch"));
    }

    #[test]
    fn write_to_unmapped_address_is_silently_dropped_during_tick() {
        // STA $1900 falls between the VIA1 mirror window ($1800-$18FF) and the
        // VIA2 window ($1C00-$1CFF), exercising the catch-all match arm in
        // `write_without_iec_bus`.
        let rom = make_rom(
            &[(
                0xC000,
                &[
                    0xA9, 0x55, // LDA #$55
                    0x8D, 0x00, 0x19, // STA $1900
                    0xEA,
                ],
            )],
            0xC000,
        );
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        boot(&mut machine);
        run_one(&mut machine); // LDA #$55
        run_one(&mut machine); // STA $1900 (no-op)

        // Open-bus read confirms the address was not captured anywhere.
        assert_eq!(machine.peek(0x1900), 0xFF);
    }

    #[test]
    fn via2_port_b_reflects_unprotected_disk_with_write_protect_low() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let bus = IecBus::new();
        machine.head_position = 2;
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");

        // Override write-protect so the OR-in branch fires.
        if let Some(disk) = machine.disk.as_mut() {
            disk.write_protected = false;
        }

        let port_b = machine.peek_with_iec_bus(0x1C00, &bus);
        assert!(
            port_b & 0x10 != 0,
            "an unprotected disk should pull VIA2 PB4 high through write_protect_not_asserted"
        );
    }

    #[test]
    fn via2_port_b_reads_not_protected_with_no_disk() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let bus = IecBus::new();

        // An empty drive lets the write-protect photocell see light, so it reads
        // "not protected" (PB4 high) — the same level a writable disk presents.
        // If it read protected instead, mounting a writable disk later would be a
        // phantom WP transition that the DOS treats as a disk change and uses to
        // force every open channel shut (breaking SAVE onto a freshly inserted
        // disk).
        assert!(machine.disk.is_none(), "fixture starts with no disk");
        let port_b = machine.peek_with_iec_bus(0x1C00, &bus);
        assert!(
            port_b & 0x10 != 0,
            "an empty drive should read VIA2 PB4 high (not write-protected)"
        );
    }

    #[test]
    fn advance_rotation_ref_cycles_consumes_delay_in_smaller_step() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");
        machine.head_position = 2;

        // Engage motor + read mode through the public IO path so flags align.
        machine.poke(0x1C02, 0x7F);
        machine.poke(0x1C00, 0x04);
        machine.poke(0x1C0C, 0x22);
        machine.tick();

        // Pre-load a small byte-ready delay; then drive rotation forward enough
        // to make `to_byte_ready` the limiting factor inside the inner loop.
        machine.byte_ready_delay_ref_cycles = 3;
        machine.advance_rotation_ref_cycles(2);

        assert_eq!(machine.byte_ready_delay_ref_cycles, 1);
    }

    #[test]
    fn current_track_bit_returns_zero_when_no_track_data() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        // No D64 mounted, so current_track_bytes() returns None and the bit read
        // collapses to a zero.
        assert_eq!(machine.current_track_bit(0), 0);
    }

    /// Drives the write serialiser one bit at a time exactly as
    /// `rotate_one_track_bit`'s write branch does, capturing the surface offset
    /// each bit lands on.
    fn write_one_byte_with_mid_store(machine: &mut Drive1541, first_byte: u8, mid_store: u8) -> u8 {
        // Latch the byte, then cross a byte boundary so the serialiser is loaded
        // with it (the 1541 write port is a latch consumed at the boundary).
        machine.gcr_write_value = first_byte;
        let mut guard = 0;
        loop {
            machine.rotate_one_track_bit();
            guard += 1;
            assert!(guard < 64, "should reach a byte boundary");
            if machine.write_bit_index == 0 {
                break;
            }
        }

        // Emit the eight bits of `first_byte`. After the first bit, the ROM's
        // write loop stores the *next* byte into the latch mid-serialisation —
        // a latched serialiser must ignore it until the next boundary.
        let mut offsets = [0usize; 8];
        for (index, slot) in offsets.iter_mut().enumerate() {
            machine.rotate_one_track_bit();
            *slot = machine.gcr_head_offset;
            if index == 0 {
                machine.gcr_write_value = mid_store;
            }
        }

        offsets.iter().fold(0u8, |acc, &offset| {
            (acc << 1) | machine.current_track_bit(offset)
        })
    }

    #[test]
    fn write_latch_ignores_mid_byte_store() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        machine
            .load_d64_bytes_writable(&synthetic_d64(), true)
            .expect("synthetic disk should mount writable");

        // Write mode: PCR drives CB2 as a manual output held low.
        machine.poke(0x1C0C, 0xC0);
        assert!(!machine.is_read_mode(), "PCR $C0 should select write mode");

        // The byte serialised onto the surface must be the value latched at the
        // boundary, not whatever the ROM stored partway through. The live-index
        // write path corrupts the tail of the byte; the latched serialiser does
        // not. Mirrors VICE `rotation.c` (separate write shift register reloaded
        // from `GCR_write_value` only at the byte boundary).
        let written = write_one_byte_with_mid_store(&mut machine, 0xA5, 0x00);
        assert_eq!(
            written, 0xA5,
            "a mid-byte store to the write latch must not corrupt the byte on the surface"
        );
    }

    #[test]
    fn from_snapshot_rejects_wrong_ram_and_rom_sizes() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");

        let mut snapshot = machine.snapshot_state();
        snapshot.ram = vec![0u8; RAM_SIZE - 1];
        let err = match Drive1541::from_snapshot(snapshot) {
            Ok(_) => panic!("undersized RAM should be rejected"),
            Err(err) => err,
        };
        assert!(err.contains("snapshot RAM size mismatch"));

        let mut snapshot = machine.snapshot_state();
        snapshot.rom = vec![0u8; ROM_SIZE - 1];
        let err = match Drive1541::from_snapshot(snapshot) {
            Ok(_) => panic!("undersized ROM should be rejected"),
            Err(err) => err,
        };
        assert!(err.contains("snapshot ROM size mismatch"));
    }
}
