//! The cartridge as the 2C02 sees it: [`PpuCartridge`] and
//! [`Mirroring`].
//!
//! The PPU's address bus runs out to the cartridge connector. Pattern
//! tables (`$0000-$1FFF`) are CHR ROM or RAM on the cartridge, and the
//! cartridge decides which half of the console's 2 KiB of nametable
//! RAM (CIRAM) each logical nametable reaches, or supplies its own.
//! This trait is that connector, from the PPU's side. The PPU owns it
//! so it can be used without any particular cartridge crate: a
//! cartridge implements it, and the machine hands the cartridge to
//! [`Ppu::tick`](crate::Ppu::tick) and the register methods.
//!
//! [`Mirroring`] lives here for the same reason. The cartridge picks
//! the mode, and MMC1 and its kin change it at run time, but the
//! mode's meaning is how the PPU maps a nametable address onto CIRAM,
//! which is this crate's job.

use serde::{Deserialize, Serialize};

/// Nametable mirroring mode.
///
/// Determined by the cartridge, not the PPU. The PPU queries the
/// cartridge on every nametable access (`$2000-$2FFF`) to find which
/// physical nametable a given logical address should route to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mirroring {
    /// A-A / B-B — both horizontal strips share a nametable. Games
    /// with vertical scrolling (e.g. *Ice Climber*) use this.
    Horizontal,
    /// A-B / A-B — both vertical strips share a nametable. Games
    /// with horizontal scrolling (e.g. *Super Mario Bros.*) use this.
    Vertical,
    /// Four unique nametables — requires cartridge VRAM on top of
    /// the PPU's 2 KiB. Used by *Gauntlet* and a few others.
    FourScreen,
    /// All four logical nametables point at the lower physical
    /// bank. Set by MMC1 on power-up and via control register.
    SingleScreenLower,
    /// All four logical nametables point at the upper physical
    /// bank. MMC1 control register.
    SingleScreenUpper,
}

/// The cartridge connector, from the PPU's side: CHR access,
/// nametable mirroring, nametable overrides and the address-bus
/// notifications some mappers count.
///
/// ## Design notes
///
/// - `chr_read` takes `&mut self` because some mappers (MMC2, MMC4)
///   update internal latches when the PPU reads from pattern table
///   addresses. NROM ignores the `&mut` but the trait keeps the
///   method signature uniform across all cartridges.
///
/// - `notify_a12_rendering` is the MMC3 IRQ counter hook. Called
///   from inside the PPU tick when the PPU address bus transitions
///   A12 during background or sprite fetches, or when the CPU moves
///   A12 through `$2006`.
pub trait PpuCartridge {
    /// PPU-side bus read for pattern table addresses
    /// (`$0000-$1FFF`). `&mut self` is required for mappers with
    /// read-side-effect latches (MMC2, MMC4).
    fn chr_read(&mut self, addr: u16) -> u8;

    /// PPU-side bus write for pattern table addresses
    /// (`$0000-$1FFF`). Ignored for CHR ROM cartridges; writes CHR
    /// RAM on cartridges without CHR ROM.
    fn chr_write(&mut self, addr: u16, value: u8);

    /// Current nametable mirroring mode. Queried by the PPU on
    /// every nametable access — mappers may change this on the fly
    /// (MMC1, MMC3) but NROM does not.
    fn mirroring(&self) -> Mirroring;

    /// MMC3 IRQ counter hook. Called from inside the PPU tick on
    /// every PPU address-bus A12 edge — whether the PPU is rendering
    /// or the CPU is toggling A12 through `$2006` during forced
    /// blank. `ppu_cycle` is the PPU master-clock counter at the
    /// edge (4 units per dot); the mapper uses it to measure how long
    /// A12 stayed low and reject rises that arrive too soon after a
    /// fall — the hardware A12 filter (Mesen's `_a12LowClock`).
    ///
    /// Default: no-op. NROM has no IRQ counter.
    fn notify_a12_rendering(&mut self, _a12_high: bool, _ppu_cycle: u64) {}

    /// Notify the cartridge of one PPU read. MMC5 uses the sequence of
    /// nametable reads to detect scanlines; other mappers ignore it.
    fn notify_ppu_read(&mut self, _addr: u16, _rendering: bool) {}

    /// Cartridge-owned nametable read override for cartridges that map
    /// ROM/RAM into `$2000-$2FFF` instead of ordinary CIRAM.
    ///
    /// Default returns `None`, meaning the PPU should use its internal
    /// nametable RAM with the cartridge's [`Mirroring`] mode.
    fn nametable_read(&mut self, _addr: u16) -> Option<u8> {
        None
    }

    /// Cartridge-owned nametable write override. Returning `true` means
    /// the cartridge consumed the write and the PPU must not write CIRAM.
    fn nametable_write(&mut self, _addr: u16, _value: u8) -> bool {
        false
    }
}
