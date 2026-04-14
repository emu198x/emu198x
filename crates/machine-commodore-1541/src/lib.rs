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

use common_commodore_iec::IecBus;
use format_commodore_c64_d64::{
    D64FileType, D64ParseError, parse_directory, read_sector, sectors_in_track,
};
use mos_6502::{M6502, registers::FLAG_V};
use mos_via_6522::Via6522;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const RAM_SIZE: usize = 0x0800;
const ROM_SIZE: usize = 0x4000;
const DEFAULT_DEVICE_NUMBER: u8 = 8;
const INITIAL_HEAD_POSITION: u8 = 36;
const MAX_HEAD_POSITION: u8 = 84;
const GCR_CONVERSION_TABLE: [u8; 16] = [
    0x0A, 0x0B, 0x12, 0x13, 0x0E, 0x0F, 0x16, 0x17, 0x09, 0x19, 0x1A, 0x1B, 0x0D, 0x1D, 0x1E, 0x15,
];
const READ_BITS_PER_SECOND_BY_ZONE: [u64; 4] = [250_000, 266_667, 285_714, 307_692];
const RAW_TRACK_SIZE_BY_ZONE: [usize; 4] = [6_250, 6_666, 7_142, 7_692];
const GAP_SIZE_BY_ZONE: [usize; 4] = [9, 12, 17, 8];
const HEADER_GAP_SIZE: usize = 9;
const SYNC_SIZE: usize = 5;
const SECTOR_GCR_SIZE_WITH_HEADER: usize = 335;
const TRACK_SLOT_COUNT: usize = (MAX_HEAD_POSITION as usize) - 1;
const IO_TRACE_LIMIT: usize = 64;
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

#[derive(Clone, Serialize, Deserialize)]
pub struct Drive1541Disk {
    image_bytes: Vec<u8>,
    disk_name: String,
    disk_id: String,
    write_protected: bool,
    directory_entries: Vec<Drive1541DirectoryEntry>,
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

    /// Loads one decoded `D64` image into the drive.
    ///
    /// # Errors
    ///
    /// Returns an error if the `D64` image is malformed.
    pub fn load_d64_bytes(&mut self, bytes: &[u8]) -> Result<(), Drive1541MediaError> {
        let directory = parse_directory(bytes)?;
        self.track_data = Some(build_track_data(bytes)?);
        self.disk = Some(Drive1541Disk {
            image_bytes: bytes.to_vec(),
            disk_name: directory.disk_name,
            disk_id: directory.disk_id,
            write_protected: true,
            directory_entries: directory
                .entries
                .into_iter()
                .map(|entry| Drive1541DirectoryEntry {
                    name: entry.name,
                    file_type: d64_file_type_name(entry.file_type).to_owned(),
                    blocks: entry.blocks,
                })
                .collect(),
        });
        self.reset_rotation_state();
        Ok(())
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
                self.clear_byte_ready_level();
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
        bus.write_drive_port_b(self.device_number, self.via1.port_b_drive_state());
    }

    fn apply_drive_inputs(&mut self, bus: Option<&IecBus>) {
        self.via1.pa_in = self.via1_port_a_input();
        self.via1.pb_in = self.via1_port_b_input(bus);
        self.via1.ca1 = !self.bus_atn_high(bus);
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
                self.clear_byte_ready_level();
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
        0xFE | u8::from(self.head_position != 2)
    }

    fn via1_port_b_input(&self, bus: Option<&IecBus>) -> u8 {
        self.via1_bus_port(bus)
    }

    fn via2_port_a_input(&self) -> u8 {
        self.gcr_read
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
            0x0C => {
                if !self.is_read_mode() || !self.byte_ready_active() {
                    self.clear_byte_ready();
                }
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
        self.disk
            .as_ref()
            .is_some_and(|disk| !disk.write_protected())
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
        if ref_cycles == 0 || !self.motor_on || !self.is_read_mode() {
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
        let bit = self.current_track_bit(self.gcr_head_offset);

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
        self.track_data.as_ref()?.track_bytes(self.head_position)
    }

    fn is_read_mode(&self) -> bool {
        self.via2.peek(0x0C) & 0x20 != 0
    }

    fn byte_ready_active(&self) -> bool {
        self.via2.peek(0x0C) & 0x02 != 0
    }
}

impl Drive1541TrackData {
    fn track_bytes(&self, head_position: u8) -> Option<&[u8]> {
        let slot = track_slot_index(head_position)?;
        let track = self.tracks.get(slot)?;
        if track.is_empty() { None } else { Some(track) }
    }
}

fn rebuild_track_data(
    disk: Option<&Drive1541Disk>,
) -> Result<Option<Drive1541TrackData>, D64ParseError> {
    disk.map(|disk| build_track_data(disk.image_bytes()))
        .transpose()
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

#[derive(Clone, Copy)]
struct GcrHeader {
    sector: u8,
    track: u8,
    id2: u8,
    id1: u8,
}

fn encode_sector_to_gcr(source: &[u8], dest: &mut [u8], header: GcrHeader, gap_size: usize) {
    debug_assert_eq!(source.len(), 256);
    debug_assert_eq!(
        dest.len(),
        SECTOR_GCR_SIZE_WITH_HEADER + HEADER_GAP_SIZE + gap_size + (SYNC_SIZE * 2)
    );

    dest.fill(0x55);
    let mut offset = 0usize;
    dest[offset..offset + SYNC_SIZE].fill(0xFF);
    offset += SYNC_SIZE;

    let mut block = [0u8; 4];
    block[0] = 0x08;
    block[1] = header.sector ^ header.track ^ header.id2 ^ header.id1;
    block[2] = header.sector;
    block[3] = header.track;
    encode_4bytes_to_gcr(block, &mut dest[offset..offset + 5]);
    offset += 5;

    block = [header.id2, header.id1, 0x0F, 0x0F];
    encode_4bytes_to_gcr(block, &mut dest[offset..offset + 5]);
    offset += 5;

    offset += HEADER_GAP_SIZE;
    dest[offset..offset + SYNC_SIZE].fill(0xFF);
    offset += SYNC_SIZE;

    let mut checksum = source[0] ^ source[1] ^ source[2];
    block = [0x07, source[0], source[1], source[2]];
    encode_4bytes_to_gcr(block, &mut dest[offset..offset + 5]);
    offset += 5;

    let mut index = 3usize;
    for _ in 0..63 {
        block.copy_from_slice(&source[index..index + 4]);
        checksum ^= block[0] ^ block[1] ^ block[2] ^ block[3];
        encode_4bytes_to_gcr(block, &mut dest[offset..offset + 5]);
        offset += 5;
        index += 4;
    }

    block = [source[255], checksum ^ source[255], 0, 0];
    encode_4bytes_to_gcr(block, &mut dest[offset..offset + 5]);
}

fn encode_4bytes_to_gcr(source: [u8; 4], dest: &mut [u8]) {
    let mut encoded = 0u64;
    for byte in source {
        encoded = (encoded << 5) | u64::from(GCR_CONVERSION_TABLE[usize::from(byte >> 4)]);
        encoded = (encoded << 5) | u64::from(GCR_CONVERSION_TABLE[usize::from(byte & 0x0F)]);
    }

    dest.copy_from_slice(&encoded.to_be_bytes()[3..]);
}

const fn speed_zone_for_track(track: u8) -> u8 {
    (track < 31) as u8 + (track < 25) as u8 + (track < 18) as u8
}

fn track_slot_index(head_position: u8) -> Option<usize> {
    if (2..TRACK_SLOT_COUNT as u8 + 2).contains(&head_position) {
        Some(usize::from(head_position - 2))
    } else {
        None
    }
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
        Drive1541, Drive1541Config, Drive1541InitError, Drive1541TrackData, ROM_SIZE,
        TRACK_SLOT_COUNT, build_track_data, track_slot_index,
    };
    use format_commodore_c64_d64::read_sector;
    use common_commodore_iec::IecBus;

    const D64_STANDARD_SIZE: usize = 174_848;
    const D64_SECTOR_SIZE: usize = 256;
    const FROM_GCR_CONVERSION_TABLE: [u8; 32] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 0, 1, 0, 12, 4, 5, 0, 0, 2, 3, 0, 15, 6, 7, 0, 9, 10, 11,
        0, 13, 14, 0,
    ];

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
        assert!(!machine.tick());
        assert!(machine.tick());
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

    fn gcr_find_sync(raw: &[u8], mut bit_offset: usize, mut remaining_bits: usize) -> Option<usize> {
        if raw.is_empty() {
            return None;
        }

        let total_bits = raw.len() * 8;
        let mut window = 0u16;
        let mut byte = raw[bit_offset >> 3] << (bit_offset & 0x07);

        while remaining_bits > 0 {
            if byte & 0x80 != 0 {
                window = (window << 1) | 1;
            } else if window & 0x03FF != 0x03FF {
                window <<= 1;
            } else {
                return Some(bit_offset);
            }

            if (bit_offset & 0x07) != 0x07 {
                bit_offset += 1;
                byte <<= 1;
            } else {
                bit_offset += 1;
                if bit_offset >= total_bits {
                    bit_offset = 0;
                }
                byte = raw[bit_offset >> 3];
            }

            remaining_bits -= 1;
        }

        None
    }

    fn gcr_decode_4bytes(source: &[u8]) -> [u8; 4] {
        let mut expanded = u32::from(source[0]) << 13;
        let mut dest = [0u8; 4];

        for (i, byte) in dest.iter_mut().enumerate() {
            expanded |= u32::from(source[i + 1]) << (5 + (i as u32 * 2));
            *byte = FROM_GCR_CONVERSION_TABLE[((expanded >> 16) & 0x1F) as usize] << 4;
            expanded <<= 5;
            *byte |= FROM_GCR_CONVERSION_TABLE[((expanded >> 16) & 0x1F) as usize];
            expanded <<= 5;
        }

        dest
    }

    fn gcr_decode_block(raw: &[u8], bit_offset: usize, blocks: usize) -> Vec<u8> {
        let shift = bit_offset & 0x07;
        let mut byte_offset = bit_offset >> 3;
        let mut carry = raw[byte_offset] << shift;
        let mut decoded = Vec::with_capacity(blocks * 4);

        for _ in 0..blocks {
            let mut gcr = [0u8; 5];
            for item in &mut gcr {
                byte_offset += 1;
                if byte_offset >= raw.len() {
                    byte_offset = 0;
                }
                if shift == 0 {
                    *item = carry;
                    carry = raw[byte_offset];
                } else {
                    *item = carry | (((u16::from(raw[byte_offset]) << shift) >> 8) as u8);
                    carry = raw[byte_offset] << shift;
                }
            }
            decoded.extend_from_slice(&gcr_decode_4bytes(&gcr));
        }

        decoded
    }

    fn gcr_read_sector_from_raw_track(raw: &[u8], sector: u8) -> Option<[u8; 256]> {
        let total_bits = raw.len() * 8;
        let mut search = 0usize;
        let mut first_sync = None;

        loop {
            let sync = gcr_find_sync(raw, search, total_bits)?;
            if first_sync == Some(sync) {
                return None;
            }
            first_sync.get_or_insert(sync);

            let header = gcr_decode_block(raw, sync, 1);
            if header[0] == 0x08 && header[2] == sector {
                let data_sync = gcr_find_sync(raw, sync, 500 * 8)?;
                let decoded = gcr_decode_block(raw, data_sync, 65);
                if decoded[0] != 0x07 {
                    return None;
                }

                let mut sector_data = [0u8; 256];
                sector_data.copy_from_slice(&decoded[1..257]);
                return Some(sector_data);
            }

            search = sync.wrapping_add(1) % total_bits;
        }
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
    fn via_status_ports_reflect_track_zero_and_write_protect() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let bus = IecBus::new();
        machine.head_position = 2;
        machine
            .load_d64_bytes(&synthetic_d64())
            .expect("synthetic D64 should mount");

        assert_eq!(
            machine.peek_with_iec_bus(0x1801, &bus) & 0x01,
            0x00,
            "track-zero sense should pull VIA1 PA0 low"
        );
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
    fn mounted_d64_gcr_track_round_trips_sector_zero() {
        let bytes = synthetic_d64();
        let track_data = build_track_data(&bytes).expect("synthetic D64 should build GCR data");
        let raw_track = track_data.track_bytes(2).expect("track 1 should be present");
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
}
