use super::Sg1000RuntimeSnapshotRefV3;
use crate::{Model, Sg1000Runtime};
use emu198x_shell::MachineCore;
#[test]
fn malformed_board_and_phase_fail_before_mutating_time_or_cartridge() {
    let mut live = Sg1000Runtime::new(Model::Sg1000Ntsc, vec![0x76; 8192]);
    let before = live.snapshot().expect("before");
    let original = serde_json::to_value(live.machine().expect("loaded")).expect("raw");
    for fault in ["region", "tstates_per_frame", "vdp_phase", "framebuffer"] {
        let mut value = original.clone();
        match fault {
            "region" => value["region"] = serde_json::json!("Pal"),
            "tstates_per_frame" => value[fault] = serde_json::json!(0),
            "vdp_phase" => value[fault] = serde_json::json!(u32::MAX),
            _ => {
                value["vdp"]["framebuffer"]
                    .as_array_mut()
                    .expect("pixels")
                    .pop();
            }
        }
        let machine: machine_sega_sg_1000::Sg1000 =
            serde_json::from_value(value).expect("forged state");
        let bytes = postcard::to_allocvec(&Sg1000RuntimeSnapshotRefV3 {
            version: 6,
            time: 999,
            model_id: Model::Sg1000Ntsc.model_id(),
            machine: Some(&machine),
        })
        .expect("forge");
        assert!(live.restore(&bytes).is_err(), "{fault}");
        assert!(live.snapshot().expect("unchanged") == before);
    }
}
#[test]
fn blank_v6_round_trips_for_every_profile_and_legacy_pal_rejects_before_decode() {
    for model in Model::ALL {
        let bytes = Sg1000Runtime::blank(model).snapshot().expect("blank");
        let mut live = Sg1000Runtime::new(model, vec![0x76; 8192]);
        live.restore(&bytes).expect("blank restore");
        assert!(live.machine().is_none());
        assert!(live.snapshot().expect("same") == bytes);
    }
    let mut pal = Sg1000Runtime::blank(Model::Sg1000Pal);
    let err = pal
        .restore(&[5])
        .expect_err("old PAL refused before machine decode");
    assert!(err.to_string().contains("v5 PAL"));
    let mut ntsc = Sg1000Runtime::blank(Model::Sg1000Ntsc);
    let legacy = postcard::to_allocvec(&(5_u16, 0_u64, Model::Sg1000Ntsc.model_id(), None::<u8>))
        .expect("blank v5");
    ntsc.restore(&legacy).expect("NTSC legacy blank");
    assert!(ntsc.machine().is_none());
}
