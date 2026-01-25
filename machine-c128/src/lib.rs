//! Commodore 128 emulator.
//!
//! The C128 is a dual-CPU computer featuring:
//! - MOS 8502 (6510-compatible) running at 1/2 MHz
//! - Zilog Z80 running at 4 MHz (for CP/M compatibility)
//! - 128K RAM in two 64K banks
//! - VIC-II 40-column video (6569 PAL / 6567 NTSC)
//! - VDC 80-column video (8563/8568)
//! - Two SID chips (optional on C128D/DCR)
//! - Two CIA chips for I/O
//! - MMU for memory management
//!
//! # Operating Modes
//!
//! - **C128 Mode**: Native mode with 128K RAM, dual video output
//! - **C64 Mode**: Full compatibility with C64 software
//! - **CP/M Mode**: Z80-based CP/M operating system
//!
//! # Memory Map (C128 Mode)
//!
//! The MMU at $D500-$D50B controls memory banking:
//! - Bank 0: System RAM (lower 64K)
//! - Bank 1: Expansion RAM (upper 64K)
//! - ROM areas can be banked in/out via MMU CR
//!
//! # ROMs Required
//!
//! Place in `roms/` directory:
//! - `c128_basic_lo.bin` (16K) - C128 BASIC ROM low
//! - `c128_basic_hi.bin` (16K) - C128 BASIC ROM high
//! - `c128_kernal.bin` (16K) - C128 KERNAL ROM
//! - `c128_editor.bin` (4K) - Screen editor ROM
//! - `c128_chargen.bin` (8K) - Character generator ROM (both sets)
//! - `c64_basic.bin` (8K) - C64 BASIC ROM (for C64 mode)
//! - `c64_kernal.bin` (8K) - C64 KERNAL ROM (for C64 mode)

mod c128;
mod memory;

pub use c128::C128;
pub use memory::C128Memory;

// Re-export shared components from machine-c64
pub use machine_c64::{
    Cartridge, CartridgeType, MachineConfig, MachineVariant, Mmu, Palette, Reu, ReuModel,
    SidRevision, TimingMode, Vdc, VdcRevision, VicRevision,
};
