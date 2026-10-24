//! Save-state snapshot enum and dispatch helper.
//!
//! Each mapper supplies a `snapshot()` method returning the variant
//! corresponding to its concrete type. [`mapper_from_snapshot`]
//! reverses the dispatch, rebuilding the trait object after a save
//! state has been deserialised.

use serde::{Deserialize, Serialize};

use crate::mapper::Mapper;
use crate::mappers::{
    action53::Action53, axrom::AxRom, bxrom::BxRom, camerica::Camerica, cnrom::CnRom,
    colordreams::ColorDreams, mmc1::Mmc1, mmc3::Mmc3, mmc5::Mmc5, nina001::Nina001, nrom::Nrom,
    sunsoft4::Sunsoft4, uxrom::UxRom, vrc2a::Vrc2a,
};

/// Serializable state for every mapper currently supported by this crate.
#[derive(Clone, Serialize, Deserialize)]
pub enum MapperSnapshot {
    /// Mapper 0.
    Nrom(Nrom),
    /// Mapper 1.
    Mmc1(Mmc1),
    /// Mapper 2.
    UxRom(UxRom),
    /// Mapper 3.
    CnRom(CnRom),
    /// Mapper 4.
    Mmc3(Mmc3),
    /// Mapper 5.
    Mmc5(Mmc5),
    /// Mapper 7.
    AxRom(AxRom),
    /// Mapper 11.
    ColorDreams(ColorDreams),
    /// Mapper 22.
    Vrc2a(Vrc2a),
    /// Mapper 28.
    Action53(Action53),
    /// Mapper 34, CHR-RAM variant.
    BxRom(BxRom),
    /// Mapper 34, NINA-001 variant.
    Nina001(Nina001),
    /// Mapper 68.
    Sunsoft4(Sunsoft4),
    /// Mapper 71.
    Camerica(Camerica),
}

/// Rebuild a boxed mapper from a previously exported mapper snapshot.
#[must_use]
pub fn mapper_from_snapshot(snapshot: MapperSnapshot) -> Box<dyn Mapper> {
    match snapshot {
        MapperSnapshot::Nrom(mapper) => Box::new(mapper),
        MapperSnapshot::Mmc1(mapper) => Box::new(mapper),
        MapperSnapshot::UxRom(mapper) => Box::new(mapper),
        MapperSnapshot::CnRom(mapper) => Box::new(mapper),
        MapperSnapshot::Mmc3(mapper) => Box::new(mapper),
        MapperSnapshot::Mmc5(mapper) => Box::new(mapper),
        MapperSnapshot::AxRom(mapper) => Box::new(mapper),
        MapperSnapshot::ColorDreams(mapper) => Box::new(mapper),
        MapperSnapshot::Vrc2a(mapper) => Box::new(mapper),
        MapperSnapshot::Action53(mapper) => Box::new(mapper),
        MapperSnapshot::BxRom(mapper) => Box::new(mapper),
        MapperSnapshot::Nina001(mapper) => Box::new(mapper),
        MapperSnapshot::Sunsoft4(mapper) => Box::new(mapper),
        MapperSnapshot::Camerica(mapper) => Box::new(mapper),
    }
}
