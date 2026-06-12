//! Chipset-agnostic board-level glue shared by every Amiga machine crate.
//!
//! These types and constants are byte-identical across the OCS / ECS /
//! AGA machine crates. They were relocated here from the three crates
//! (#34, unified-driver replatform) so the shared per-CCK driver can
//! reference them once instead of three times. None of them depend on
//! the chipset variant: the blitter-bus adaptor sees only the shared
//! chip-RAM `Memory`, the CPU bus-transaction value types are plain
//! data, and the master-clock divisors are the same on every Amiga.

use crate::memory::Memory;

/// Ticks per Agnus colour clock. A CCK (HRM beam-coordinate unit) is
/// two master/4 ticks — one tick per lores pixel.
pub const TICKS_PER_CCK: u64 = 2;

/// CIA E-clock divider: real CIA E-clock runs at master/40 = 0.71 MHz.
/// Our primary tick unit is master/4 (= 68000 CPU clock = lores pixel
/// rate), so CIAs fire once every 10 ticks. Confirmed by HRM register
/// map: "CIAA timer A (.709379 MHz PAL)" = master/40 exactly.
pub const CIA_E_CLOCK_DIVISOR: u64 = 10;

/// `BlitterBus` adaptor over chip RAM. The blitter sees chip RAM only,
/// via Agnus DMA, and addresses wrap at the 2 MiB chip-RAM boundary.
pub struct ChipRamBus<'a>(pub &'a mut Memory);

impl commodore_agnus_ocs::BlitterBus for ChipRamBus<'_> {
    fn read_word(&mut self, addr: u32) -> u16 {
        self.0.read_chip_ram_word(addr)
    }
    fn write_word(&mut self, addr: u32, val: u16) {
        self.0.write_word(addr & 0x001F_FFFE, val);
    }
}

/// Snapshotted out of `cpu.state.BusCycle` once per servicing pass so
/// chip-select handlers can operate on plain values instead of holding
/// a borrow on `&mut self.cpu`. `data` is `0` for reads.
#[derive(Clone, Copy)]
pub struct BusTransaction {
    pub addr: u32,
    pub is_read: bool,
    pub is_word: bool,
    pub data: u16,
}

/// What a chip-select arm produced for one [`BusTransaction`].
///
/// `Byte` and `Word` describe what the chip drove on the data lines;
/// the dispatcher applies the byte-lane extraction rule once.
/// `WriteAck` is the write-side equivalent — the chip absorbed the
/// write and the dispatcher returns `Ready(0)`.
///
/// Every reachable cycle ultimately gets handled (Memory's fallback
/// always claims the cycle, returning chip RAM, slow RAM, ROM, or
/// floating-bus from `last_bus_value`), so a "no chip drove anything"
/// variant is unreachable in this model.
#[derive(Clone, Copy)]
pub enum BusResponse {
    /// Chip drove an 8-bit value. Always returned in the low 8 bits.
    Byte(u8),
    /// Chip drove a 16-bit value. For byte reads the dispatcher
    /// extracts the byte lane: even address (UDS) → high byte, odd
    /// (LDS) → low byte, both delivered in the low 8 bits.
    Word(u16),
    /// Write completed; bus_status becomes `Ready(0)`.
    WriteAck,
}
