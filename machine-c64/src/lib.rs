//! Commodore 64/128 emulator.
//!
//! This crate provides complete C64 emulation with:
//! - 6510 CPU (via cpu-6502 crate)
//! - 64KB RAM with ROM banking
//! - VIC-II video (text and bitmap modes)
//! - SID audio (6581/8580)
//! - CIA1/CIA2 I/O chips
//! - Keyboard matrix input
//! - Joystick support
//!
//! # C128 Components
//!
//! This crate also provides C128-specific components:
//! - MMU (Memory Management Unit) - 128K banking
//! - VDC 8563 (80-column video chip)
//! - Z80 CPU support (via IoBus trait)
//!
//! # Supported Variants
//!
//! ## C64
//! - C64 "breadbin" (PAL/NTSC) - original 1982 with 6581 SID
//! - C64C (PAL/NTSC) - 1986 revision with 8580 SID
//! - SX-64 - portable with built-in drive
//! - C64 GS - cartridge-only game console
//!
//! ## C128 (via machine-c128 crate)
//! - C128 (PAL/NTSC) - with VDC 8563 (16K)
//! - C128DCR (PAL/NTSC) - with VDC 8568 (64K)
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
//! - `.d64` - D64 disk images (1541 single-sided format)
//! - `.d71` - D71 disk images (1571 double-sided format)
//! - `.tap` - TAP tape images (pulse data)
//! - `.t64` - T64 tape archives (instant load)

mod c64;
mod cartridge;
mod config;
mod disk;
mod input;
mod memory;
mod mmu;
mod palette;
mod reu;
mod sid;
mod snapshot;
mod tap;
mod vdc;
mod vic;

pub use c64::C64;
pub use cartridge::{Cartridge, CartridgeType};
pub use config::{MachineConfig, MachineVariant, SidRevision, TimingMode, VicRevision};
pub use disk::{Disk, DiskAudioEvent, DiskFormat};
pub use mmu::Mmu;
pub use palette::{Color, PALETTE_PEPTO, PALETTE_VICE, Palette, palette_for_revision};
pub use reu::{Reu, ReuModel};
pub use snapshot::Snapshot;
pub use tap::{T64Entry, Tape, TapeFormat};
pub use vdc::{Vdc, VdcRevision};
