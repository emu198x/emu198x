//! Postcard-encoded snapshot envelope for the Neo Geo.
//!
//! Serialises the live machine (68000, Z80, LSPC2 with video and palette
//! RAM, YM2610, controllers, both RAMs and the memory card) so a restore
//! resumes exactly. The BIOS and cartridge are not in it: they are media,
//! and a restore takes them from the runtime it lands in.

use emu198x_shell::{MachineCore, MachineError, MachineTime};
use machine_snk_neo_geo::NeoGeo;
use serde::{Deserialize, Serialize};

use crate::runtime::NeoGeoRuntime;

/// The machine serialises positionally, so any change to its layout needs
/// a bump here.
const SNAPSHOT_VERSION: u16 = 1;

/// Borrowing envelope used during encode — avoids cloning the live machine.
#[derive(Serialize)]
struct NeoGeoSnapshotRefV1<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a NeoGeo>,
}

/// Owning envelope used during decode.
#[derive(Deserialize)]
struct NeoGeoSnapshotV1 {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<NeoGeo>,
}

pub(crate) fn encode(runtime: &NeoGeoRuntime) -> Result<Vec<u8>, MachineError> {
    let snapshot = NeoGeoSnapshotRefV1 {
        version: SNAPSHOT_VERSION,
        time: runtime.time().get(),
        model_id: runtime.model_id(),
        machine: runtime.machine(),
    };
    postcard::to_allocvec(&snapshot).map_err(|reason| MachineError::InvalidSnapshot {
        reason: format!("encode failed: {reason}"),
    })
}

pub(crate) fn decode(runtime: &mut NeoGeoRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let (version, _) = postcard::take_from_bytes::<u16>(bytes).map_err(|reason| {
        MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        }
    })?;
    if version != SNAPSHOT_VERSION {
        return Err(MachineError::InvalidSnapshot {
            reason: format!("unsupported snapshot version {version}; expected {SNAPSHOT_VERSION}"),
        });
    }
    let snapshot: NeoGeoSnapshotV1 =
        postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("decode failed: {reason}"),
        })?;
    debug_assert_eq!(snapshot.version, SNAPSHOT_VERSION);
    if snapshot.model_id != runtime.model_id() {
        return Err(MachineError::InvalidSnapshot {
            reason: format!(
                "snapshot model {} does not match runtime model {}",
                snapshot.model_id,
                runtime.model_id()
            ),
        });
    }
    runtime.set_machine(snapshot.machine)?;
    runtime.set_time(MachineTime::new(snapshot.time));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SNAPSHOT_VERSION, decode};
    use crate::{Model, blank};
    use emu198x_shell::MachineError;

    #[test]
    fn decode_rejects_future_version_before_payload_decode() {
        let mut runtime = blank(Model::Aes);
        let future_version = SNAPSHOT_VERSION + 1;
        let bytes = postcard::to_allocvec(&future_version).expect("future version should encode");
        let err = decode(&mut runtime, &bytes).expect_err("future version should reject");
        assert!(
            matches!(err, MachineError::InvalidSnapshot { ref reason }
                if reason.contains(&format!("unsupported snapshot version {future_version}"))),
            "{err:?}"
        );
        assert!(decode(&mut runtime, &[]).is_err());
    }
}
