//! Minimal snapshot envelope for the MSX1 runtime.
//!
//! `machine-msx` does not expose a deep chip-level snapshot today,
//! so this implementation captures the bootstrap state (BIOS bytes,
//! cartridges, region/model, time) — enough to replay from a known
//! starting point. A full live snapshot (CPU/VDP/PSG/PPI/RAM) lands
//! as a follow-up once `machine-msx` grows a `MsxSnapshot`.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_msx::MapperType;
use serde::{Deserialize, Serialize};

use crate::runtime::MsxRuntime;

const SNAPSHOT_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
struct MsxRuntimeSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    bios_bytes: Option<Vec<u8>>,
    cart1_bytes: Option<Vec<u8>>,
    cart1_mapper: SerdeMapperType,
    cart2_bytes: Option<Vec<u8>>,
    cart2_mapper: SerdeMapperType,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
enum SerdeMapperType {
    Plain,
    Konami,
    KonamiScc,
    Ascii8,
    Ascii16,
}

impl From<MapperType> for SerdeMapperType {
    fn from(value: MapperType) -> Self {
        match value {
            MapperType::Plain => Self::Plain,
            MapperType::Konami => Self::Konami,
            MapperType::KonamiScc => Self::KonamiScc,
            MapperType::Ascii8 => Self::Ascii8,
            MapperType::Ascii16 => Self::Ascii16,
        }
    }
}

impl From<SerdeMapperType> for MapperType {
    fn from(value: SerdeMapperType) -> Self {
        match value {
            SerdeMapperType::Plain => Self::Plain,
            SerdeMapperType::Konami => Self::Konami,
            SerdeMapperType::KonamiScc => Self::KonamiScc,
            SerdeMapperType::Ascii8 => Self::Ascii8,
            SerdeMapperType::Ascii16 => Self::Ascii16,
        }
    }
}

pub(crate) fn encode(runtime: &MsxRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = MsxRuntimeSnapshotV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model().model_id().to_owned(),
        bios_bytes: runtime.bios_bytes().map(<[u8]>::to_vec),
        cart1_bytes: runtime.cart1_bytes().map(<[u8]>::to_vec),
        cart1_mapper: runtime.cart1_mapper().into(),
        cart2_bytes: runtime.cart2_bytes().map(<[u8]>::to_vec),
        cart2_mapper: runtime.cart2_mapper().into(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut MsxRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let snapshot: MsxRuntimeSnapshotV1 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    if snapshot.version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported MSX snapshot version {}", snapshot.version),
        });
    }
    runtime.set_time(MachineTime::new(snapshot.time));
    runtime.set_bios_bytes(snapshot.bios_bytes);
    runtime.set_cart1(snapshot.cart1_bytes, snapshot.cart1_mapper.into());
    runtime.set_cart2(snapshot.cart2_bytes, snapshot.cart2_mapper.into());
    runtime.rebuild_after_restore();
    Ok(())
}
