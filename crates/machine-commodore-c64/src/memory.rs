//! C64 memory subsystem with 6510-controlled banking.

use format_commodore_c64_prg::RamAccess;
use mos_vic_ii::VicMemory;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const BASIC_ROM_SIZE: usize = 0x2000;
const KERNAL_ROM_SIZE: usize = 0x2000;
const CHARACTER_ROM_SIZE: usize = 0x1000;
const RAM_SIZE: usize = 0x10000;
const COLOUR_RAM_SIZE: usize = 0x0400;
const PORT_PULLUPS: u8 = 0x37;

/// Memory-construction errors.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum MemoryInitError {
    /// One ROM image had the wrong size.
    #[error("{which} ROM has {actual} bytes; expected exactly {expected}")]
    WrongRomSize {
        which: &'static str,
        expected: usize,
        actual: usize,
    },
}

/// C64 memory subsystem.
#[derive(Clone)]
pub struct C64Memory {
    ram: Box<[u8; RAM_SIZE]>,
    basic_rom: Box<[u8; BASIC_ROM_SIZE]>,
    kernal_rom: Box<[u8; KERNAL_ROM_SIZE]>,
    character_rom: Box<[u8; CHARACTER_ROM_SIZE]>,
    colour_ram: [u8; COLOUR_RAM_SIZE],
    port_ddr: u8,
    port_data: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct C64MemorySnapshot {
    ram: Vec<u8>,
    basic_rom: Vec<u8>,
    kernal_rom: Vec<u8>,
    character_rom: Vec<u8>,
    colour_ram: Vec<u8>,
    port_ddr: u8,
    port_data: u8,
}

impl C64Memory {
    /// Constructs the memory subsystem from ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if any ROM size is incorrect.
    pub fn new(
        kernal_rom: &[u8],
        basic_rom: &[u8],
        character_rom: &[u8],
    ) -> Result<Self, MemoryInitError> {
        Ok(Self {
            ram: Box::new([0; RAM_SIZE]),
            basic_rom: boxed_array_from_slice("BASIC", basic_rom)?,
            kernal_rom: boxed_array_from_slice("KERNAL", kernal_rom)?,
            character_rom: boxed_array_from_slice("character", character_rom)?,
            colour_ram: [0; COLOUR_RAM_SIZE],
            port_ddr: 0x2F,
            port_data: 0x37,
        })
    }

    /// Rebuilds one memory subsystem from a previously captured snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if any stored array has the wrong size.
    pub(crate) fn from_snapshot(snapshot: C64MemorySnapshot) -> Result<Self, String> {
        if snapshot.ram.len() != RAM_SIZE {
            return Err(format!(
                "snapshot RAM has {} bytes, expected {}",
                snapshot.ram.len(),
                RAM_SIZE
            ));
        }

        if snapshot.colour_ram.len() != COLOUR_RAM_SIZE {
            return Err(format!(
                "snapshot colour RAM has {} bytes, expected {}",
                snapshot.colour_ram.len(),
                COLOUR_RAM_SIZE
            ));
        }

        let mut memory = Self::new(
            &snapshot.kernal_rom,
            &snapshot.basic_rom,
            &snapshot.character_rom,
        )
        .map_err(|reason| reason.to_string())?;
        memory.ram.copy_from_slice(&snapshot.ram);
        memory.colour_ram.copy_from_slice(&snapshot.colour_ram);
        memory.port_ddr = snapshot.port_ddr;
        memory.port_data = snapshot.port_data;
        Ok(memory)
    }

    /// Captures the full memory state for runtime snapshot serialization.
    #[must_use]
    pub(crate) fn snapshot_state(&self) -> C64MemorySnapshot {
        C64MemorySnapshot {
            ram: self.ram.as_slice().to_vec(),
            basic_rom: self.basic_rom.as_slice().to_vec(),
            kernal_rom: self.kernal_rom.as_slice().to_vec(),
            character_rom: self.character_rom.as_slice().to_vec(),
            colour_ram: self.colour_ram.to_vec(),
            port_ddr: self.port_ddr,
            port_data: self.port_data,
        }
    }

    /// Current 6510 port DDR value at `$0000`.
    #[must_use]
    pub const fn port_ddr(&self) -> u8 {
        self.port_ddr
    }

    /// Current 6510 port data value at `$0001`.
    #[must_use]
    pub const fn port_data(&self) -> u8 {
        self.port_data
    }

    /// Effective 6510 port value after applying DDR outputs and pull-ups.
    #[must_use]
    pub const fn effective_port(&self) -> u8 {
        (self.port_data & self.port_ddr) | (PORT_PULLUPS & !self.port_ddr)
    }

    /// Returns `true` when BASIC ROM is visible at `$A000-$BFFF`.
    #[must_use]
    pub const fn basic_visible(&self) -> bool {
        self.hiram() && self.loram()
    }

    /// Returns `true` when KERNAL ROM is visible at `$E000-$FFFF`.
    #[must_use]
    pub const fn kernal_visible(&self) -> bool {
        self.hiram()
    }

    /// Returns `true` when I/O is visible at `$D000-$DFFF`.
    #[must_use]
    pub const fn is_io_visible(&self) -> bool {
        self.charen() && (self.hiram() || self.loram())
    }

    /// Returns `true` when character ROM is visible to the CPU.
    #[must_use]
    pub const fn is_character_rom_visible_to_cpu(&self) -> bool {
        !self.charen() && self.hiram() && self.loram()
    }

    /// CPU-visible read with ROM overlays applied.
    #[must_use]
    pub fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000 => self.port_ddr,
            0x0001 => self.effective_port(),
            0xA000..=0xBFFF if self.basic_visible() => self.basic_rom[usize::from(addr - 0xA000)],
            0xD000..=0xDFFF if self.is_character_rom_visible_to_cpu() => {
                self.character_rom[usize::from(addr - 0xD000)]
            }
            0xE000..=0xFFFF if self.kernal_visible() => self.kernal_rom[usize::from(addr - 0xE000)],
            _ => self.ram[usize::from(addr)],
        }
    }

    /// CPU-visible write. ROM areas still write through to underlying RAM,
    /// matching real hardware.
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000 => self.port_ddr = value,
            0x0001 => self.port_data = value,
            _ => self.ram[usize::from(addr)] = value,
        }
    }

    /// Direct RAM read bypassing overlays.
    #[must_use]
    pub fn ram_read(&self, addr: u16) -> u8 {
        self.ram[usize::from(addr)]
    }

    /// Borrows the full underlying RAM image.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        self.ram.as_slice()
    }

    /// Direct RAM write bypassing overlays.
    pub fn ram_write(&mut self, addr: u16, value: u8) {
        self.ram[usize::from(addr)] = value;
    }

    /// Reads the current VIC-visible byte from one 16 KiB bank-local offset.
    #[must_use]
    pub fn vic_read(&self, bank: u8, offset: u16) -> u8 {
        let bank = usize::from(bank & 0x03);
        let offset = usize::from(offset & 0x3FFF);
        if (bank == 0 || bank == 2) && (0x1000..0x2000).contains(&offset) {
            return self.character_rom[offset - 0x1000];
        }

        self.ram[(bank * 0x4000) + offset]
    }

    /// Reads one colour RAM nibble.
    #[must_use]
    pub fn colour_ram_read(&self, offset: u16) -> u8 {
        self.colour_ram
            .get(usize::from(offset))
            .copied()
            .map_or(0, |value| value & 0x0F)
    }

    /// Writes one colour RAM nibble.
    pub fn colour_ram_write(&mut self, offset: u16, value: u8) {
        if let Some(slot) = self.colour_ram.get_mut(usize::from(offset)) {
            *slot = value & 0x0F;
        }
    }

    /// Borrows the full underlying colour RAM image.
    #[must_use]
    pub fn colour_ram(&self) -> &[u8] {
        &self.colour_ram
    }

    const fn hiram(&self) -> bool {
        self.effective_port() & 0x04 != 0
    }

    const fn loram(&self) -> bool {
        self.effective_port() & 0x02 != 0
    }

    const fn charen(&self) -> bool {
        self.effective_port() & 0x01 != 0
    }
}

fn boxed_array_from_slice<const N: usize>(
    which: &'static str,
    bytes: &[u8],
) -> Result<Box<[u8; N]>, MemoryInitError> {
    if bytes.len() != N {
        return Err(MemoryInitError::WrongRomSize {
            which,
            expected: N,
            actual: bytes.len(),
        });
    }

    let mut array = Box::new([0; N]);
    array.copy_from_slice(bytes);
    Ok(array)
}

impl VicMemory for C64Memory {
    fn read_vram(&self, addr: u16) -> u8 {
        self.vic_read((addr >> 14) as u8, addr & 0x3FFF)
    }

    fn read_colour(&self, offset: u16) -> u8 {
        self.colour_ram_read(offset)
    }
}

impl RamAccess for C64Memory {
    fn ram_read(&self, addr: u16) -> u8 {
        self.ram_read(addr)
    }

    fn ram_write(&mut self, addr: u16, val: u8) {
        self.ram_write(addr, val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_memory() -> C64Memory {
        C64Memory::new(
            &[0xEE; KERNAL_ROM_SIZE],
            &[0xBB; BASIC_ROM_SIZE],
            &[0xCC; CHARACTER_ROM_SIZE],
        )
        .expect("stub ROM sizes should be valid")
    }

    #[test]
    fn default_banking_shows_basic_and_kernal() {
        let memory = make_memory();
        assert_eq!(memory.cpu_read(0xA000), 0xBB);
        assert_eq!(memory.cpu_read(0xE000), 0xEE);
        assert!(memory.is_io_visible());
    }

    #[test]
    fn writes_land_in_ram_under_roms() {
        let mut memory = make_memory();
        memory.cpu_write(0xA000, 0x42);
        memory.cpu_write(0xE000, 0x24);
        assert_eq!(memory.cpu_read(0xA000), 0xBB);
        assert_eq!(memory.cpu_read(0xE000), 0xEE);
        assert_eq!(memory.ram_read(0xA000), 0x42);
        assert_eq!(memory.ram_read(0xE000), 0x24);
    }

    #[test]
    fn all_ram_banking_hides_roms_and_io() {
        let mut memory = make_memory();
        memory.cpu_write(0x0000, 0xFF);
        memory.cpu_write(0x0001, 0x00);
        memory.ram_write(0xA000, 0x42);
        memory.ram_write(0xD000, 0x43);
        memory.ram_write(0xE000, 0x44);
        assert_eq!(memory.cpu_read(0xA000), 0x42);
        assert_eq!(memory.cpu_read(0xD000), 0x43);
        assert_eq!(memory.cpu_read(0xE000), 0x44);
        assert!(!memory.is_io_visible());
    }

    #[test]
    fn character_rom_appears_when_charen_is_clear() {
        let mut memory = make_memory();
        memory.cpu_write(0x0000, 0xFF);
        memory.cpu_write(0x0001, 0x36);
        assert_eq!(memory.cpu_read(0xD000), 0xCC);
        assert!(memory.is_character_rom_visible_to_cpu());
    }

    #[test]
    fn port_inputs_float_high_when_ddr_is_clear() {
        let mut memory = make_memory();
        memory.cpu_write(0x0000, 0x00);
        memory.cpu_write(0x0001, 0x00);
        assert_eq!(memory.cpu_read(0x0001), PORT_PULLUPS);
    }

    #[test]
    fn vic_reads_character_rom_in_banks_zero_and_two() {
        let mut memory = make_memory();
        memory.ram_write(0x5000, 0xAA);
        memory.ram_write(0xD000, 0xBB);

        assert_eq!(memory.vic_read(0, 0x1000), 0xCC);
        assert_eq!(memory.vic_read(2, 0x1000), 0xCC);
        assert_eq!(memory.vic_read(1, 0x1000), 0xAA);
        assert_eq!(memory.vic_read(3, 0x1000), 0xBB);
    }

    #[test]
    fn colour_ram_stores_low_nibble_only() {
        let mut memory = make_memory();
        memory.colour_ram_write(0, 0x0F);
        memory.colour_ram_write(1, 0xFF);
        assert_eq!(memory.colour_ram_read(0), 0x0F);
        assert_eq!(memory.colour_ram_read(1), 0x0F);
    }

    #[test]
    fn wrong_rom_sizes_are_rejected() {
        let err = match C64Memory::new(&[0; 1], &[0; BASIC_ROM_SIZE], &[0; CHARACTER_ROM_SIZE]) {
            Ok(_) => panic!("wrong KERNAL size must fail"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            MemoryInitError::WrongRomSize {
                which: "KERNAL",
                expected: KERNAL_ROM_SIZE,
                actual: 1,
            }
        );
    }
}
