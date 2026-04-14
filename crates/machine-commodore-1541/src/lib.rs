//! Board-level Commodore 1541 substrate.
//!
//! This crate deliberately stops before IEC, GCR, and mechanics. It owns the
//! durable drive-side board behavior that those later layers will need:
//! - 6502 CPU bus loop
//! - 2KB RAM with mirroring
//! - 16KB DOS ROM mapping
//! - VIA1 and VIA2 register decode at `$1800`/`$1C00`

use common_commodore_iec::IecBus;
use mos_6502::M6502;
use mos_via_6522::Via6522;
use thiserror::Error;

const RAM_SIZE: usize = 0x0800;
const ROM_SIZE: usize = 0x4000;
const DEFAULT_DEVICE_NUMBER: u8 = 8;

#[derive(Clone)]
pub struct Drive1541 {
    cpu: M6502,
    via1: Via6522,
    via2: Via6522,
    ram: [u8; RAM_SIZE],
    rom: [u8; ROM_SIZE],
    device_number: u8,
    cycles: u64,
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
            device_number: DEFAULT_DEVICE_NUMBER,
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
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x17FF => self.ram[usize::from(addr & 0x07FF)],
            0x1800..=0x18FF => self.via1.peek((addr & 0x0F) as u8),
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
        self.cpu.irq = self.via1.irq || self.via2.irq;

        if self.cpu.rw {
            self.cpu.data_in = self.peek(self.cpu.addr);
        } else {
            self.poke(self.cpu.addr, self.cpu.data);
        }

        let completed = self.cpu.tick();
        self.via1.tick();
        self.via2.tick();
        self.cycles += 1;
        completed
    }

    pub fn tick_with_iec_bus(&mut self, bus: &mut IecBus) -> bool {
        self.apply_iec_inputs(bus);
        self.cpu.irq = self.via1.irq || self.via2.irq;

        if self.cpu.rw {
            self.cpu.data_in = self.read_with_iec_bus(self.cpu.addr, bus);
        } else {
            self.write_with_iec_bus(self.cpu.addr, self.cpu.data, bus);
        }

        let completed = self.cpu.tick();
        self.apply_iec_inputs(bus);
        self.via1.tick();
        self.via2.tick();
        self.cycles += 1;
        completed
    }

    #[must_use]
    pub fn peek_with_iec_bus(&self, addr: u16, bus: &IecBus) -> u8 {
        match addr {
            0x0000..=0x17FF => self.ram[usize::from(addr & 0x07FF)],
            0x1800..=0x18FF if (addr & 0x0F) == 0x00 => self.via1_port_b_read(bus),
            0x1800..=0x18FF => self.via1.peek((addr & 0x0F) as u8),
            0x1C00..=0x1CFF => self.via2.peek((addr & 0x0F) as u8),
            0xC000..=0xFFFF => self.rom[usize::from(addr - 0xC000)],
            _ => 0xFF,
        }
    }

    pub fn read_with_iec_bus(&mut self, addr: u16, bus: &IecBus) -> u8 {
        match addr {
            0x0000..=0x17FF => self.ram[usize::from(addr & 0x07FF)],
            0x1800..=0x18FF if (addr & 0x0F) == 0x00 => {
                let value = self.via1_port_b_read(bus);
                self.via1.read_port_b_with_value(value)
            }
            0x1800..=0x18FF => self.via1.read((addr & 0x0F) as u8),
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
        self.apply_iec_inputs(bus);
    }

    fn drive_iec_outputs(&self, bus: &mut IecBus) {
        bus.write_drive_port_b(self.device_number, self.via1.pb);
    }

    fn apply_iec_inputs(&mut self, bus: &IecBus) {
        self.via1.ca1 = bus.drive_atn_high();
    }

    fn via1_port_b_read(&self, bus: &IecBus) -> u8 {
        ((self.via1.orb() & 0x1A) | bus.drive_port()) ^ 0x85
    }
}

#[cfg(test)]
mod tests {
    use super::{Drive1541, Drive1541Config, Drive1541InitError, ROM_SIZE};
    use common_commodore_iec::IecBus;

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
}
