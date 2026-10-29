//! Real, model-matched CoCo boot and shared typed-symbol regression (#2138/#2135).
use super::*;
use emu198x_shell::{HeadlessScript, HeadlessSession, ScriptStep};
use std::path::PathBuf;
fn run(model: Model, name: &str, expected_crc: u32, hardware: common_dragon::DragonHardware) {
    let dir = PathBuf::from(
        std::env::var_os("EMU198X_COCO_BOARD_ROM_DIR")
            .expect("directory with bas12.rom, bas13.rom and extbas11.rom"),
    );
    let basic = std::fs::read(dir.join(name)).expect("identified Color BASIC");
    let extended =
        std::fs::read(dir.join("extbas11.rom")).expect("identified Extended Color BASIC");
    assert_eq!(basic.len(), 8192);
    assert_eq!(extended.len(), 8192);
    assert_eq!(
        crc32(&basic),
        expected_crc,
        "MAME model-matched Color BASIC identity"
    );
    assert_eq!(
        crc32(&extended),
        0xA82A_6254,
        "MAME Extended Color BASIC 1.1 identity"
    );
    let mut firmware = FirmwareSet::new();
    firmware.push(emu198x_shell::FirmwareImage::new(
        crate::COCO_COLOR_BASIC_ROM_ID,
        &basic,
    ));
    firmware.push(emu198x_shell::FirmwareImage::new(
        crate::COCO_EXTENDED_BASIC_ROM_ID,
        &extended,
    ));
    let runtime = DragonRuntime::from_firmware(model, &firmware).expect("matched original ROMs");
    assert_eq!(runtime.machine().hardware(), hardware);
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        model.frame_ticks(),
        DragonSessionQueryProvider,
    );
    session
        .wait_for_boot(100)
        .expect("real firmware reaches OK");
    // As in the existing Dragon ROM tests: the visible prompt precedes keyboard readiness.
    session
        .run_frames(30)
        .expect("firmware settles at its input loop");
    HeadlessScript {
        steps: vec![ScriptStep::TypeString {
            text: "PRINT 7*6\nPRINT 7+6\n".into(),
            hold_frames: Some(8),
            settle_frames: Some(60),
        }],
    }
    .execute(&mut session)
    .expect("shared keyboard typing");
    let lines = session
        .query("screen.text.lines")
        .expect("text lines")
        .value;
    let text: Vec<_> = lines
        .as_array()
        .expect("lines")
        .iter()
        .map(|v| v.as_str().expect("line").trim())
        .collect();
    assert!(
        text.contains(&"42") && text.contains(&"13"),
        "real BASIC must evaluate both typed symbols: {text:?}"
    );
    assert!(!text.iter().any(|s| s.contains("ERROR")), "{text:?}");
}
#[test]
#[ignore = "FIXTURE: needs Color BASIC 1.2 and Extended Color BASIC 1.1 in EMU198X_COCO_BOARD_ROM_DIR"]
fn coco1_real_basic_boots_and_evaluates_typed_print() {
    run(
        Model::CoCo1Ntsc,
        "bas12.rom",
        0x5436_8805,
        common_dragon::DragonHardware::CoCo1,
    );
}
#[test]
#[ignore = "FIXTURE: needs Color BASIC 1.3 and Extended Color BASIC 1.1 in EMU198X_COCO_BOARD_ROM_DIR"]
fn coco2_real_basic_boots_and_evaluates_typed_print() {
    run(
        Model::CoCo2Ntsc,
        "bas13.rom",
        0xD8F4_D15E,
        common_dragon::DragonHardware::CoCo2,
    );
}
