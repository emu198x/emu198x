//! Nintendo Entertainment System (NES) emulator.
//!
//! This crate provides complete NES/Famicom emulation:
//! - 2A03 CPU (6502 without decimal mode)
//! - PPU 2C02 (picture processing unit)
//! - APU (audio processing unit)
//! - Controller input
//! - Mapper support for cartridge banking
//!
//! # Supported Mappers
//!
//! - Mapper 0 (NROM) - No banking, covers ~10% of games
//! - Mapper 1 (MMC1) - Nintendo's first mapper, ~28% of games
//! - Mapper 2 (UxROM) - Simple PRG banking, ~10% of games
//! - Mapper 3 (CNROM) - Simple CHR banking, ~6% of games
//! - Mapper 4 (MMC3) - Scanline counter, ~24% of games
//! - Mapper 7 (AxROM) - Single-screen mirroring, ~3% of games
//!
//! # ROMs
//!
//! Load iNES format (.nes) ROM files.

mod apu;
mod cartridge;
mod controller;
mod mapper;
mod memory;
mod nes;
mod ppu;

pub use cartridge::{Cartridge, Mirroring};
pub use controller::Controller;
pub use nes::Nes;
pub use ppu::Ppu;
