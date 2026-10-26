//! [`Mapper`] trait shared by every mapper implementation.
//!
//! # The PPU side, and Mirroring
//!
//! A mapper's PPU-facing half — CHR reads and writes, nametable
//! [`Mirroring`], nametable overrides and the A12 and PPU-read
//! notifications — is the [`PpuCartridge`] trait, which the PPU crate
//! `emu198x-ricoh-ppu-2c02` owns. [`Mapper`] requires it, so every
//! mapper implements both, and a `&mut dyn Mapper` coerces to the
//! `&mut dyn PpuCartridge` the PPU takes. [`Mirroring`] comes from the
//! same crate and is re-exported here. The cartridge still decides
//! mirroring — MMC1 and its kin change the answer at run time — but
//! the PPU owns the interface it is asked through, so the PPU stands
//! on its own and the cartridge depends on it, never the other way
//! round.
//!
//! # Scope of the `Mapper` trait
//!
//! The trait defined here is intentionally **leaner** than the
//! archive version. It carries the CPU/CHR bus methods, mirroring,
//! IRQ pending, and the MMC3 A12 notifier — everything the
//! [nes-clock-topology.md](../../../knowledge/decisions/nes-clock-topology.md)
//! decision record says the machine layer and the (future) PPU need
//! to call. It drops the archive's save-state, peek-chr, expansion
//! audio, and PRG-RAM accessor methods; those are features of
//! higher-layer mappers (MMC3, Sunsoft 5B, VRC6) and have no callers
//! yet. They will land back in the trait as default methods when the
//! mappers that need them get ported.

pub use emu198x_ricoh_ppu_2c02::{Mirroring, PpuCartridge};

use crate::snapshot::MapperSnapshot;

// ─── Mapper trait ──────────────────────────────────────────────────

/// Cartridge mapper: translates CPU addresses in `$4020-$FFFF` and
/// PPU addresses in `$0000-$1FFF` to ROM, RAM, or bank-switched
/// memory on the cartridge. The PPU half is the [`PpuCartridge`]
/// supertrait, which the PPU crate owns.
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
/// - `chr_read` (on [`PpuCartridge`]) takes `&mut self` because some
///   mappers (MMC2, MMC4) update internal latches when the PPU reads
///   from pattern table addresses.
///
/// - `irq_pending()` is the mapper's IRQ output pin, polled by the
///   machine layer once per CPU cycle and OR'd into the CPU's
///   `irq` input. Default returns `false` — most mappers don't do
///   IRQ.
///
/// - `notify_a12_rendering` (on [`PpuCartridge`]) is the MMC3 IRQ
///   counter hook. Called from inside the PPU tick when the PPU
///   address bus transitions A12 during background or sprite
///   fetches. See
///   [nes-clock-topology.md](../../../knowledge/decisions/nes-clock-topology.md#pin-contracts)
///   for the rationale.
pub trait Mapper: Send + PpuCartridge {
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

    /// CPU-side read of the cartridge expansion area (`$4020-$5FFF`).
    ///
    /// Returns `Some(byte)` when the cartridge drives the data bus and
    /// `None` (the default) when it leaves the bus floating, so the
    /// machine layer can supply open-bus. Almost every mapper leaves
    /// `$4020-$5FFF` unclaimed; MMC5 is the exception, putting its
    /// IRQ-status, 8×8 multiplier and ExRAM registers here. May carry
    /// read side effects — reading MMC5's `$5204` clears the in-frame
    /// IRQ and `$5010` clears the PCM IRQ — so it takes `&mut self`.
    fn cpu_read_expansion(&mut self, _addr: u16) -> Option<u8> {
        None
    }

    /// CPU-side bus write. Called by the machine layer's
    /// `cpu_write` for addresses in `$4020-$FFFF`. The mapper
    /// decides whether to latch the value (bank switching), write
    /// to PRG RAM, or ignore.
    fn cpu_write(&mut self, addr: u16, value: u8);

    /// Level-triggered IRQ output. Default: never asserted.
    ///
    /// The machine layer ORs this with other IRQ sources (e.g. APU
    /// frame IRQ, DMC IRQ) and drives the CPU's `irq` input.
    fn irq_pending(&self) -> bool {
        false
    }

    /// Advance mapper-local CPU-cycle state such as expansion audio or
    /// no-PPU-read timers.
    fn cpu_tick(&mut self) {}

    /// Current expansion-audio contribution, mixed additively by the APU.
    fn expansion_audio_sample(&self) -> f32 {
        0.0
    }

    /// Side-effect-free view of [`PpuCartridge::nametable_read`], for tooling.
    ///
    /// ⚠ Exists because `nametable_read` takes `&mut self` — MMC5 uses
    /// nametable fetches to drive its scanline detector, so a debugger
    /// or test harness calling it would corrupt the very timing it is
    /// trying to observe. Anything that wants to *look* at the screen
    /// must come through here.
    ///
    /// A mapper that overrides `nametable_read` should override this
    /// too, or screen-reading tools will silently see empty CIRAM
    /// instead of what the PPU actually fetches.
    fn nametable_peek(&self, _addr: u16) -> Option<u8> {
        None
    }

    /// Capture concrete mapper state for save-state export.
    /// The disk drive, for a board that has one (the Famicom Disk
    /// System's RAM adapter). Cartridges have none.
    fn disk_drive(&self) -> Option<&dyn DiskDrive> {
        None
    }

    /// The disk drive, mutably, to insert or eject a side.
    fn disk_drive_mut(&mut self) -> Option<&mut dyn DiskDrive> {
        None
    }

    fn snapshot(&self) -> MapperSnapshot;

    /// Cartridge PRG-RAM ($6000-$7FFF work/save RAM), for battery `.sav`
    /// persistence. Empty by default; mappers with PRG-RAM expose it
    /// here. The runtime gates persistence on the cartridge's battery
    /// flag, so a non-battery work-RAM mapper may expose RAM here without
    /// it being written to a `.sav`.
    fn save_ram(&self) -> &[u8] {
        &[]
    }

    /// Restore a loaded `.sav` into PRG-RAM. No-op by default; mappers
    /// with PRG-RAM copy in as many bytes as fit (a short or oversized
    /// sidecar is truncated, never panics).
    fn restore_save_ram(&mut self, _bytes: &[u8]) {}
}

/// A disk drive behind the cartridge slot, holding one side of a disk
/// set at a time.
pub trait DiskDrive {
    /// How many sides the loaded disk set has.
    fn side_count(&self) -> usize;

    /// The side in the drive, if any.
    fn inserted(&self) -> Option<usize>;

    /// Put a side in the drive. A side past the set's end is ignored.
    fn insert(&mut self, side: usize);

    /// Take the side out.
    fn eject(&mut self);

    /// The disk set as it stands now, writes included, in the file
    /// layout it was loaded from.
    fn image(&self) -> Vec<u8>;
}
