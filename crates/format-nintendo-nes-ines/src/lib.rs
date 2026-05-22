//! iNES / NES 2.0 cartridge header parser and the [`Mapper`] trait.
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
//! **Mapper 22 (VRC2a)**, **Mapper 28 (Action 53)**,
//! **Mapper 5 (MMC5)**, **Mapper 34 (BxROM/BNROM and NINA-001)**,
//! **Mapper 68 (Sunsoft-4)**, and **Mapper 71 (Camerica/Codemasters)** —
//! the trait, the header parser, and the first mappers needed to boot
//! flat-layout test ROMs plus common PRG/CHR-bank-switched cartridges.
//!
//! The remaining mappers are archive-provenance (see
//! [archives-as-source.md](../../knowledge/decisions/archives-as-source.md))
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
//!   [`parse_ines`].
//! - [`mappers`] holds one module per supported mapper number.
//!
//! Public re-exports below preserve the flat
//! `format_nintendo_nes_ines::*` API used by downstream crates.

#![allow(clippy::cast_possible_truncation)]

pub mod format;
pub mod mapper;
pub mod mappers;
pub mod snapshot;

pub use format::{CartridgeHeader, ParsedCartridge, parse_ines};
pub use mapper::{Mapper, Mirroring};
pub use mappers::action53::Action53;
pub use mappers::axrom::AxRom;
pub use mappers::bxrom::BxRom;
pub use mappers::camerica::Camerica;
pub use mappers::cnrom::CnRom;
pub use mappers::colordreams::ColorDreams;
pub use mappers::mmc1::Mmc1;
pub use mappers::mmc3::Mmc3;
pub use mappers::mmc5::Mmc5;
pub use mappers::nina001::Nina001;
pub use mappers::nrom::Nrom;
pub use mappers::sunsoft4::Sunsoft4;
pub use mappers::uxrom::UxRom;
pub use mappers::vrc2a::Vrc2a;
pub use snapshot::{MapperSnapshot, mapper_from_snapshot};
