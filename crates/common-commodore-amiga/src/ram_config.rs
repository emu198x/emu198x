//! RAM layouts shared by the Amiga board implementations.

use crate::memory;

/// RAM layout for an Amiga instance.
///
/// Chip RAM lives at `$000000` and is required. Slow RAM is the A501-
/// style trapdoor expansion at `$C00000`. Fast RAM is a Zorro-II
/// autoconfig board; `fast_kb` keeps the runtime preset surface stable across
/// the autoconfig wiring.
///
/// Sizes are in kilobytes. `is_valid` checks the family-wide memory
/// representation; each machine constructor additionally enforces the
/// installed Agnus revision's chip-RAM ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RamConfig {
    /// Chip RAM in KiB. One of 256, 512, 1024, 2048.
    pub chip_kb: u32,
    /// Slow RAM in KiB. One of 0, 256, 512, 1024, 1536.
    pub slow_kb: u32,
    /// Fast RAM in KiB. Multiples of 64 up to 8192; autoconfig
    /// protocol supports sizes {64, 128, 256, 512, 1024, 2048, 4096,
    /// 8192}. Zero means "no board present".
    pub fast_kb: u32,
}

impl RamConfig {
    /// Stock A500: 512K chip, no expansion.
    #[must_use]
    pub const fn bare() -> Self {
        Self {
            chip_kb: 512,
            slow_kb: 0,
            fast_kb: 0,
        }
    }

    /// A500 with A501 trapdoor: 512K chip + 512K slow.
    #[must_use]
    pub const fn a501_trapdoor() -> Self {
        Self {
            chip_kb: 512,
            slow_kb: 512,
            fast_kb: 0,
        }
    }

    /// A500Plus-equivalent chip layout: 1M chip, no slow, no fast.
    #[must_use]
    pub const fn a500_plus() -> Self {
        Self {
            chip_kb: 1024,
            slow_kb: 0,
            fast_kb: 0,
        }
    }

    /// Maxed A500: 1M chip + 512K slow + 8M Zorro-II fast.
    #[must_use]
    pub const fn a500_maxed() -> Self {
        Self {
            chip_kb: 1024,
            slow_kb: 512,
            fast_kb: 8192,
        }
    }

    /// `true` if the sizes are all within the supported set.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        memory::is_valid_chip_ram_size(self.chip_kb as usize * 1024)
            && memory::is_valid_slow_ram_size(self.slow_kb as usize * 1024)
            && self.fast_kb <= 8192
            && self.fast_kb.is_multiple_of(64)
    }
}

impl Default for RamConfig {
    fn default() -> Self {
        Self::bare()
    }
}
