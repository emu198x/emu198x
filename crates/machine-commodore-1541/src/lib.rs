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

use common_commodore_iec::IecBus;
use format_commodore_c64_d64::{D64FileType, D64ParseError, parse_directory};
use mos_6502::M6502;
use mos_via_6522::Via6522;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const RAM_SIZE: usize = 0x0800;
const ROM_SIZE: usize = 0x4000;
const DEFAULT_DEVICE_NUMBER: u8 = 8;
const INITIAL_HEAD_POSITION: u8 = 36;
const MAX_HEAD_POSITION: u8 = 84;

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
    device_number: u8,
    head_position: u8,
    stepper_phase: u8,
    motor_on: bool,
    activity_led: bool,
    density_code: u8,
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
    cycles: u64,
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
            device_number: DEFAULT_DEVICE_NUMBER,
            head_position: INITIAL_HEAD_POSITION,
            stepper_phase: 0x03,
            motor_on: false,
            activity_led: false,
            density_code: 0,
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
    pub const fn disk(&self) -> Option<&Drive1541Disk> {
        self.disk.as_ref()
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
        Ok(())
    }

    pub fn eject_disk(&mut self) {
        self.disk = None;
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
        self.device_number = snapshot.device_number;
        self.head_position = snapshot.head_position;
        self.stepper_phase = snapshot.stepper_phase;
        self.motor_on = snapshot.motor_on;
        self.activity_led = snapshot.activity_led;
        self.density_code = snapshot.density_code;
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
            device_number: snapshot.device_number,
            head_position: snapshot.head_position,
            stepper_phase: snapshot.stepper_phase,
            motor_on: snapshot.motor_on,
            activity_led: snapshot.activity_led,
            density_code: snapshot.density_code,
            cycles: snapshot.cycles,
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
            0x1800..=0x18FF => self.via1.write((addr & 0x0F) as u8, value),
            0x1C00..=0x1CFF => self.via2.write((addr & 0x0F) as u8, value),
            0xC000..=0xFFFF => {}
            _ => {}
        }
    }

    pub fn tick(&mut self) -> bool {
        self.apply_drive_inputs(None);
        self.cpu.irq = self.via1.irq || self.via2.irq;

        if self.cpu.rw {
            self.cpu.data_in = self.peek(self.cpu.addr);
        } else {
            self.poke(self.cpu.addr, self.cpu.data);
        }

        let completed = self.cpu.tick();
        self.apply_drive_inputs(None);
        self.via1.tick();
        self.via2.tick();
        self.refresh_drive_mechanics();
        self.apply_drive_inputs(None);
        self.cycles += 1;
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

        let completed = self.cpu.tick();
        self.apply_drive_inputs(Some(bus));
        self.via1.tick();
        self.via2.tick();
        self.refresh_drive_mechanics();
        self.apply_drive_inputs(Some(bus));
        self.cycles += 1;
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
                let value = self.via2_port_b_read();
                self.via2.read_port_b_with_value(value)
            }
            0x1C00..=0x1CFF if matches!(addr & 0x0F, 0x01 | 0x0F) => {
                let value = self.via2_port_a_read();
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
                self.drive_iec_outputs(bus);
            }
            0x1C00..=0x1CFF => self.via2.write((addr & 0x0F) as u8, value),
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
        self.via2.ca1 = self.byte_ready_not_asserted() || !self.so_enable();
    }

    fn refresh_drive_mechanics(&mut self) {
        let port_b = self.via2.port_b_drive_state();
        let phase = port_b & 0x03;
        let movement = phase.wrapping_sub(self.stepper_phase) & 0x03;

        self.motor_on = port_b & 0x04 != 0;
        self.activity_led = port_b & 0x08 != 0;
        self.density_code = (port_b >> 5) & 0x03;

        if self.motor_on && (movement & 0x01) != 0 {
            if (movement & 0x02) == 0 {
                self.head_position = self.head_position.saturating_add(1).min(MAX_HEAD_POSITION);
            } else {
                self.head_position = self.head_position.saturating_sub(1);
            }
        }

        self.stepper_phase = phase;
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
        0xFE | u8::from(self.head_position != 0)
    }

    fn via1_port_b_input(&self, bus: Option<&IecBus>) -> u8 {
        self.via1_bus_port(bus)
    }

    fn via2_port_a_input(&self) -> u8 {
        0xFF
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

    fn so_enable(&self) -> bool {
        if self.via2.ca2_drive {
            self.via2.ca2_out
        } else {
            true
        }
    }

    fn byte_ready_not_asserted(&self) -> bool {
        true
    }

    fn sync_not_detected(&self) -> bool {
        true
    }

    fn write_protect_not_asserted(&self) -> bool {
        self.disk
            .as_ref()
            .is_some_and(|disk| !disk.write_protected())
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
    use super::{Drive1541, Drive1541Config, Drive1541InitError, ROM_SIZE};
    use common_commodore_iec::IecBus;

    const D64_STANDARD_SIZE: usize = 174_848;
    const D64_SECTOR_SIZE: usize = 256;

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
    fn via_status_ports_reflect_track_zero_and_write_protect() {
        let rom = make_rom(&[(0xC000, &[0xEA])], 0xC000);
        let mut machine = Drive1541::new(Drive1541Config { dos_rom: &rom })
            .expect("1541 scaffold ROM should be valid");
        let bus = IecBus::new();
        machine.head_position = 0;
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
        assert_eq!(
            machine.peek_with_iec_bus(0x1C00, &bus) & 0x80,
            0x80,
            "sync should stay inactive until GCR/sector mechanics exist"
        );
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
