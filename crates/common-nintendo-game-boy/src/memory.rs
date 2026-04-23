//! `MemoryBus` — the SM83's view of the Game Boy memory map.
//!
//! The DMG memory map is heavily cartridge-driven, so unlike the
//! Spectrum's `Spectrum48kMemory` there's no concrete struct here —
//! just the trait every machine implements over its own composition
//! of cartridge ROM/RAM, work RAM, video RAM, OAM, HRAM, and the
//! IO registers.
//!
//! ```text
//! $0000-$3FFF — Cartridge ROM bank 0          (MBC-mapped)
//! $4000-$7FFF — Cartridge ROM bank 1..N       (MBC-mapped)
//! $8000-$9FFF — Video RAM                     (8 KiB DMG / 16 KiB CGB banked)
//! $A000-$BFFF — Cartridge RAM                 (MBC-mapped, optional)
//! $C000-$CFFF — Work RAM bank 0
//! $D000-$DFFF — Work RAM bank 1..7            (CGB only — bank 1 on DMG)
//! $E000-$FDFF — Echo of $C000-$DDFF           (mirrored hardware)
//! $FE00-$FE9F — OAM (sprite attribute table)
//! $FEA0-$FEFF — Unusable region
//! $FF00-$FF7F — IO registers
//! $FF80-$FFFE — High RAM (HRAM)
//! $FFFF       — Interrupt enable register (IE)
//! ```

/// SM83-side memory interface.
///
/// Every machine in the family implements this trait over its own
/// memory composition. The CPU consumes it indirectly: the machine
/// reads from this bus when populating `data_in` between CPU ticks,
/// and writes to it when the CPU's `wr` strobe asserts.
///
/// Reads must be side-effect-free where possible (some IO registers
/// have read side-effects on real hardware — STAT mode bits, joypad,
/// etc. — and those are the implementor's responsibility to model).
pub trait MemoryBus {
    /// Reads one byte from the CPU-visible address space.
    fn read(&mut self, addr: u16) -> u8;

    /// Writes one byte to the CPU-visible address space. Writes to
    /// ROM ranges are typically interpreted as MBC bank-switch
    /// commands rather than ignored.
    fn write(&mut self, addr: u16, value: u8);
}
