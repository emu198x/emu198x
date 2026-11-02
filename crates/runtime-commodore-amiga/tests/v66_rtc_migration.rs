//! The v66 reader preserves history; v67 cannot remove fitted A500+ hardware.
use emu198x_shell::MachineCore;
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model};

#[test]
fn already_valid_v66_payloads_restore_for_every_existing_profile() {
    for model in Model::VARIANTS {
        let source = AmigaRuntimeKind::blank(model);
        let current = source.snapshot().expect("v67 state");
        assert_eq!(current[0], 67);
        let mut old = current.clone();
        old[0] = 66;
        let mut restored = AmigaRuntimeKind::blank(model);
        restored.restore(&old).expect("explicit v66 reader");
        assert!(
            restored.snapshot().expect("resaved v67") == current,
            "{model:?}: every payload byte"
        );
    }
}

#[test]
fn v67_refuses_legacy_missing_onboard_rtc_atomically() {
    let mut bytes = include_bytes!("../../../test-data/commodore-amiga/ecs-v66/commodore-amiga-a500-plus-ecs-pal-progressive.state").to_vec();
    assert_eq!(bytes[0], 66);
    bytes[0] = 67;
    let mut runtime = AmigaRuntimeKind::blank(Model::A500PlusEcsPal);
    let before = runtime.snapshot().expect("before");
    assert!(runtime.restore(&bytes).is_err());
    assert!(runtime.snapshot().expect("after") == before);
}

#[test]
fn migrated_clock_is_guest_visible_and_keeps_its_calendar() {
    let bytes = include_bytes!(
        "../../../test-data/commodore-amiga/ecs-v66/commodore-amiga-a500-plus-ecs-pal-progressive.state"
    );
    let mut runtime = AmigaRuntimeKind::blank(Model::A500PlusEcsPal);
    runtime.restore(bytes).expect("legacy clock migration");
    let clock = runtime.rtc_diagnostic_snapshot();
    assert!(runtime.gary_diagnostic_snapshot().rtc_present);
    assert_eq!(
        runtime.read_word(0xDC0000) & 15,
        u16::from(clock.second % 10)
    );
    let saved = runtime.snapshot().expect("migrated save");
    let mut restored = AmigaRuntimeKind::blank(Model::A500PlusEcsPal);
    restored.restore(&saved).expect("current save");
    assert_eq!(restored.rtc_diagnostic_snapshot(), clock);
    assert!(restored.snapshot().expect("identical current save") == saved);
}
