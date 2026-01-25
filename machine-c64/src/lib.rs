//! Commodore 64 emulator.
//!
//! This crate provides a complete C64 emulation with:
//! - 6510 CPU (via cpu-6502 crate)
//! - 64KB RAM with ROM banking
//! - VIC-II video (text and bitmap modes)
//! - CIA1/CIA2 I/O chips
//! - Keyboard matrix input
//! - Joystick support
//!
//! # ROMs Required
//!
//! Place in `roms/` directory:
//! - `basic.bin` (8KB) - C64 BASIC ROM
//! - `kernal.bin` (8KB) - C64 KERNAL ROM
//! - `chargen.bin` (4KB) - Character generator ROM
//!
//! # File Formats
//!
//! - `.prg` - PRG files (2-byte load address + program data)
//! - `.d64` - D64 disk images (1541 format)

mod c64;
mod disk;
mod input;
mod memory;
mod sid;
mod snapshot;
mod vic;

pub use c64::C64;
pub use disk::Disk;
pub use snapshot::Snapshot;
