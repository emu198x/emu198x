//! NES cartridge boards: the [`Mapper`] trait, nametable [`Mirroring`],
//! the supported boards, and loading one from an iNES / NES 2.0 file.
//!
//! The file layout itself is read by `format-nintendo-nes-ines`; this crate
//! is the hardware the file describes.
//!
//! # Scope of this port
//!
//! The archive crate
//! (`Emu198x-archive/crates/format-nintendo-nes-ines`) implemented 48
//! mapper variants covering virtually every licensed NES/Famicom
//! game. This port currently carries **Mapper 0 (NROM)**,
//! **Mapper 1 (MMC1)**, **Mapper 2 (UxROM)**,
//! **Mapper 3 (CNROM)**, **Mapper 4 (MMC3)**, and
//! **Mapper 7 (AxROM)**, **Mapper 11 (Color Dreams)**,
//! **Mapper 22 (VRC2a)**, **Mappers 21, 23 and 25 (VRC2b/c and VRC4)**,
//! **Mapper 28 (Action 53)**,
//! **Mapper 5 (MMC5)**, **Mapper 34 (BxROM/BNROM and NINA-001)**,
//! **Mapper 66 (GxROM)**, **Mapper 68 (Sunsoft-4)**, and **Mapper 71 (Camerica/Codemasters)** —
//! the trait, the header parser, and the first mappers needed to boot
//! flat-layout test ROMs plus common PRG/CHR-bank-switched cartridges.
//!
//! The boards with expansion audio follow, each feeding the cartridge
//! audio pin through [`Mapper::expansion_audio_sample`]: **Mapper 19
//! (Namco 163)**, **Mappers 24 and 26 (VRC6)**, **Mapper 69 (Sunsoft
//! FME-7 / 5B)** and **Mapper 85 (VRC7)**.
//!
//! The remaining mappers are archive-provenance (see
//! [archives-as-source.md](../../../knowledge/decisions/archives-as-source.md))
//! and will be lifted one at a time *once the PPU crate is back
//! online*, because there is no point porting address-translation
//! logic with no bus for the translated addresses to serve.
//!
//! # Module layout
//!
//! - [`mapper`] holds the [`Mapper`] trait and the [`Mirroring`] enum
//!   shared by every concrete mapper.
//! - [`snapshot`] holds [`MapperSnapshot`] and
//!   [`mapper_from_snapshot`].
//! - [`format`] holds [`CartridgeHeader`], [`ParsedCartridge`], and
//!   [`parse_ines`], which builds a board from a parsed file.
//! - [`mappers`] holds one module per supported mapper number, and
//!   [`Fds`], the Famicom Disk System's RAM adapter and drive, which
//!   plugs into the cartridge slot but loads from a `.fds` disk image and
//!   an 8 KiB BIOS rather than an iNES file.
//!
//! Public re-exports below preserve the flat
//! `nintendo_nes_cartridge::*` API used by downstream crates.

#![allow(clippy::cast_possible_truncation)]

pub mod format;
pub mod mapper;
pub mod mappers;
pub mod snapshot;

pub use format::{CartridgeHeader, ParsedCartridge, parse_ines};
pub use mapper::{DiskDrive, Mapper, Mirroring, PpuCartridge};
pub use mappers::action53::Action53;
pub use mappers::axrom::AxRom;
pub use mappers::bxrom::BxRom;
pub use mappers::camerica::Camerica;
pub use mappers::cnrom::CnRom;
pub use mappers::colordreams::ColorDreams;
pub use mappers::fds::Fds;
pub use mappers::fme7::Fme7;
pub use mappers::gxrom::GxRom;
pub use mappers::mmc1::Mmc1;
pub use mappers::mmc3::Mmc3;
pub use mappers::mmc5::Mmc5;
pub use mappers::namco163::Namco163;
pub use mappers::nina001::Nina001;
pub use mappers::nrom::Nrom;
pub use mappers::sunsoft4::Sunsoft4;
pub use mappers::uxrom::UxRom;
pub use mappers::vrc2a::Vrc2a;
pub use mappers::vrc4::Vrc4;
pub use mappers::vrc6::Vrc6;
pub use mappers::vrc7::Vrc7;
pub use snapshot::{MapperSnapshot, mapper_from_snapshot};
