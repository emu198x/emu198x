//! This owner fixes decode and video while preserving native frame boundaries.
use common_dragon::{DragonHardware, DragonState};
use machine_tandy_coco_2::{
    Dragon32, DragonHardwareModel, DragonKeyboard, ROM_SIZE, VideoStandard,
};
fn rom() -> [u8; ROM_SIZE] {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/synthetic-firmware/dragon-32-control.rom"),
    )
    .expect("own firmware")
    .try_into()
    .expect("ROM size")
}
#[test]
fn fixed_construction_and_legacy_binding_reject_other_boards() {
    let m = Dragon32::new(&rom());
    assert_eq!(m.hardware(), DragonHardware::CoCo2);
    assert_eq!(m.hardware_model(), DragonHardwareModel::CoCo64k);
    assert_eq!(m.video_standard(), VideoStandard::Ntsc);
    for model in [
        DragonHardwareModel::Dragon32,
        DragonHardwareModel::Dragon64Compat,
        DragonHardwareModel::Dragon64Mode,
        DragonHardwareModel::CoCo,
        DragonHardwareModel::CoCo64k,
    ] {
        for video in [VideoStandard::Pal, VideoStandard::Ntsc] {
            let raw =
                DragonState::new_with_keyboard_and_model(&rom(), DragonKeyboard::new(), model)
                    .with_video_standard(video);
            assert_eq!(
                Dragon32::from_state(raw).is_ok(),
                (model == DragonHardwareModel::CoCo64k) && video == VideoStandard::Ntsc
            );
        }
    }
}
#[test]
fn partial_cpu_stepping_preserves_the_next_native_boundary() {
    let mut m = Dragon32::new(&rom());
    assert!(m.run_frame(0).1);
    let before = m.master_ticks();
    for _ in 0..37 {
        m.step_cycle();
    }
    assert!(m.run_frame(0).1);
    assert_eq!(m.master_ticks() - before, 262 * 912);
}
