use super::{SNAPSHOT_VERSION, decode};
use crate::{Model, blank};
use emu198x_shell::{MachineCore, MachineError};

#[test]
fn decode_rejects_a_future_version_before_the_payload() {
    let mut runtime = blank(Model::SnesNtsc);
    let bytes = postcard::to_allocvec(&(SNAPSHOT_VERSION + 1)).expect("encodes");
    let err = decode(&mut runtime, &bytes).expect_err("rejected");
    assert!(
        matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("unsupported snapshot version")),
        "{err:?}"
    );
    assert!(decode(&mut runtime, &[]).is_err());
}

#[test]
fn decode_rejects_another_models_snapshot() {
    let snapshot = blank(Model::SnesNtsc).snapshot().expect("encode");
    let mut runtime = blank(Model::SnesPal);
    let err = runtime.restore(&snapshot).expect_err("model mismatch");
    assert!(
        matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("does not match")),
        "{err:?}"
    );
}
#[test]
fn forged_region_or_short_ipl_is_rejected_without_mutating_the_runtime() {
    use super::SnesSnapshotRefV2;
    use crate::with_cartridge;
    use common_nintendo_snes::{Cartridge, Map, Region, Snes};
    let mut runtime = with_cartridge(Model::SnesNtsc, crate::test_cartridge::cartridge());
    assert!(runtime.machine().is_some());
    let before = runtime.snapshot().expect("before");
    for region in [Region::Ntsc, Region::Pal] {
        let mut raw = Snes::new(
            Cartridge::from_rom(vec![0xea; 32768], Map::LoRom, 0),
            region,
            None,
        );
        if region == Region::Ntsc {
            raw.apu.ipl.pop();
        }
        let bad = postcard::to_allocvec(&SnesSnapshotRefV2 {
            version: SNAPSHOT_VERSION,
            time: 999,
            ipl: None,
            model_id: Model::SnesNtsc.model_id(),
            machine: Some(&raw),
        })
        .expect("forged");
        assert!(runtime.restore(&bad).is_err());
        assert!(runtime.snapshot().expect("unchanged") == before);
    }
}

#[test]
fn blank_v1_and_inconsistent_v2_fail_atomically() {
    use super::SnesSnapshotRefV2;
    use crate::with_cartridge;
    let mut runtime = with_cartridge(Model::SnesNtsc, crate::test_cartridge::cartridge());
    assert!(runtime.machine().is_some());
    let before = runtime.snapshot().expect("save");
    let legacy_blank =
        postcard::to_allocvec(&(1_u16, 42_u64, runtime.model_id(), None::<u8>)).expect("v1");
    let err = runtime
        .restore(&legacy_blank)
        .expect_err("blank v1 rejected");
    assert!(err.to_string().contains("blank v1"));
    assert_eq!(runtime.snapshot().expect("unchanged"), before);
    for firmware in [vec![0; 63], vec![0; 65], vec![0; 64]] {
        let bytes = postcard::to_allocvec(&SnesSnapshotRefV2 {
            version: SNAPSHOT_VERSION,
            time: 42,
            model_id: runtime.model_id(),
            ipl: Some(&firmware),
            machine: runtime.machine().map(|m| m.state()),
        })
        .expect("v2");
        assert!(runtime.restore(&bytes).is_err());
        assert_eq!(runtime.snapshot().expect("unchanged"), before);
    }
    let truncated = &before[..before.len() / 2];
    assert!(runtime.restore(truncated).is_err());
    assert_eq!(runtime.snapshot().expect("unchanged"), before);
}
