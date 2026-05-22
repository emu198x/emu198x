//! [`Mapper`] trait and the [`Mirroring`] enum shared by every mapper
//! implementation.
//!
//! # Mirroring
//!
//! [`Mirroring`] is defined here rather than re-exported from a PPU
//! crate because the PPU is not yet ported. When `ricoh-ppu-2c02` is
//! rewritten in the dot-driven architecture, the canonical
//! `Mirroring` will live in that crate and this one will re-export
//! it — matching the archive's shape. The enum is small enough
//! (five variants) that re-defining it here is cheap and the future
//! reconciliation will be a one-line re-export change.
//!
//! # Scope of the `Mapper` trait
//!
//! The trait defined here is intentionally **leaner** than the
//! archive version. It carries the CPU/CHR bus methods, mirroring,
//! IRQ pending, and the MMC3 A12 notifier — everything the
//! [nes-clock-topology.md](../../knowledge/decisions/nes-clock-topology.md)
//! decision record says the machine layer and the (future) PPU need
//! to call. It drops the archive's save-state, peek-chr, expansion
//! audio, and PRG-RAM accessor methods; those are features of
//! higher-layer mappers (MMC3, Sunsoft 5B, VRC6) and have no callers
//! yet. They will land back in the trait as default methods when the
//! mappers that need them get ported.

use serde::{Deserialize, Serialize};

use crate::snapshot::MapperSnapshot;

// ─── Mirroring ─────────────────────────────────────────────────────

/// Nametable mirroring mode.
///
/// Determined by the cartridge, not the PPU. The PPU queries the
/// mapper on every nametable access (`$2000-$2FFF`) to find which
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

// ─── Mapper trait ──────────────────────────────────────────────────

/// Cartridge mapper: translates CPU addresses in `$4020-$FFFF` and
/// PPU addresses in `$0000-$1FFF` to ROM, RAM, or bank-switched
/// memory on the cartridge.
///
/// Implementations are per-mapper-number. The parser in
/// [`parse_ines`](crate::parse_ines) inspects the iNES header's mapper
/// field and constructs the right concrete type. This port carries
/// [`Nrom`](crate::Nrom) (mapper 0), [`Mmc1`](crate::Mmc1) (mapper 1),
/// [`UxRom`](crate::UxRom) (mapper 2), [`CnRom`](crate::CnRom)
/// (mapper 3), [`Mmc3`](crate::Mmc3) (mapper 4),
/// [`AxRom`](crate::AxRom) (mapper 7),
/// [`ColorDreams`](crate::ColorDreams) (mapper 11),
/// [`Vrc2a`](crate::Vrc2a) (mapper 22),
/// [`Action53`](crate::Action53) (mapper 28), [`Mmc5`](crate::Mmc5)
/// (mapper 5), [`BxRom`](crate::BxRom) / [`Nina001`](crate::Nina001)
/// (mapper 34), [`Sunsoft4`](crate::Sunsoft4) (mapper 68), and
/// [`Camerica`](crate::Camerica) (mapper 71).
///
/// ## Design notes
///
/// - `chr_read` takes `&mut self` because some mappers (MMC2, MMC4)
///   update internal latches when the PPU reads from pattern table
///   addresses. NROM ignores the `&mut` but the trait keeps the
///   method signature uniform across all mappers.
///
/// - `irq_pending()` is the mapper's IRQ output pin, polled by the
///   machine layer once per CPU cycle and OR'd into the CPU's
///   `irq` input. Default returns `false` — most mappers don't do
///   IRQ.
///
/// - `notify_a12_rendering` is the MMC3 IRQ counter hook. Called
///   from inside the PPU tick when the PPU address bus transitions
///   A12 during background or sprite fetches. See
///   [nes-clock-topology.md](../../knowledge/decisions/nes-clock-topology.md#pin-contracts)
///   for the rationale.
pub trait Mapper: Send {
    /// CPU-side bus read. Called by the machine layer's `cpu_read`
    /// for addresses in `$4020-$FFFF`. Returns the byte the
    /// cartridge would drive onto the CPU data bus.
    fn cpu_read(&self, addr: u16) -> u8;

    /// CPU-side bus read with mapper-visible side effects.
    ///
    /// Most mappers are pure on reads, so the default delegates to
    /// [`Self::cpu_read`]. Mappers with readable status registers or
    /// read-triggered audio modes override this method.
    fn cpu_read_side_effect(&mut self, addr: u16) -> u8 {
        self.cpu_read(addr)
    }

    /// CPU-side bus write. Called by the machine layer's
    /// `cpu_write` for addresses in `$4020-$FFFF`. The mapper
    /// decides whether to latch the value (bank switching), write
    /// to PRG RAM, or ignore.
    fn cpu_write(&mut self, addr: u16, value: u8);

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

    /// Level-triggered IRQ output. Default: never asserted.
    ///
    /// The machine layer ORs this with other IRQ sources (e.g. APU
    /// frame IRQ, DMC IRQ) and drives the CPU's `irq` input.
    fn irq_pending(&self) -> bool {
        false
    }

    /// MMC3 IRQ counter hook. Called from inside the PPU tick when
    /// the PPU address bus A12 line changes during background or
    /// sprite fetches. The mapper applies its own debounce filter
    /// (MMC3 ignores transitions < 15 dots apart).
    ///
    /// Default: no-op. NROM has no IRQ counter.
    fn notify_a12_rendering(&mut self, _a12_high: bool) {}

    /// Notify the mapper of one PPU read. MMC5 uses the sequence of
    /// nametable reads to detect scanlines; other mappers ignore it.
    fn notify_ppu_read(&mut self, _addr: u16, _rendering: bool) {}

    /// Advance mapper-local CPU-cycle state such as expansion audio or
    /// no-PPU-read timers.
    fn cpu_tick(&mut self) {}

    /// Current expansion-audio contribution, mixed additively by the APU.
    fn expansion_audio_sample(&self) -> f32 {
        0.0
    }

    /// Mapper-owned nametable read override for cartridges that map
    /// ROM/RAM into `$2000-$2FFF` instead of ordinary CIRAM.
    ///
    /// Default returns `None`, meaning the PPU should use its internal
    /// nametable RAM with the mapper's [`Mirroring`] mode.
    fn nametable_read(&mut self, _addr: u16) -> Option<u8> {
        None
    }

    /// Mapper-owned nametable write override. Returning `true` means
    /// the mapper consumed the write and the PPU must not write CIRAM.
    fn nametable_write(&mut self, _addr: u16, _value: u8) -> bool {
        false
    }

    /// Capture concrete mapper state for save-state export.
    fn snapshot(&self) -> MapperSnapshot;
}
