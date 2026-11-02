//! Original A2000 history survives the explicit v66/v67 clock migration.
mod legacy_v67;
use std::path::Path;

use emu198x_shell::MachineCore;
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model};

fn original_a2000(model: Model) -> Vec<u8> {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/commodore-amiga/ocs-v67");
    legacy_v67::Originals::open(&root).read(&format!(
        "{}-kickstart-progressive-debug-cut.state",
        model.model_id()
    ))
}

#[test]
fn original_a2000_payloads_keep_every_field_apart_from_header_and_clock_decode() {
    for model in [Model::A2000OcsPal, Model::A2000OcsNtsc] {
        let original = original_a2000(model);
        for version in [66, 67] {
            // The original is v67. Explicit header translation also exercises
            // the identical v66 payload layout; it is not a new v66 capture.
            let mut legacy = original.clone();
            legacy[0] = version;
            let mut runtime = AmigaRuntimeKind::blank(model);
            runtime.restore(&legacy).expect("explicit legacy reader");
            let current = runtime.snapshot().expect("v68 save");
            legacy_v67::assert_migrated_save(&original, &current, true);
            assert!(runtime.gary_diagnostic_snapshot().rtc_present);
            let clock = runtime.rtc_diagnostic_snapshot();
            assert_eq!(
                runtime.read_word(0xDC0000) & 15,
                u16::from(clock.second % 10)
            );
            let mut restored = AmigaRuntimeKind::blank(model);
            restored.restore(&current).expect("v68 reader");
            assert_eq!(restored.rtc_diagnostic_snapshot(), clock);
            assert!(restored.snapshot().expect("retained history") == current);
        }
    }
}

#[test]
fn v68_cannot_remove_the_fitted_a2000_clock_and_failure_is_atomic() {
    for model in [Model::A2000OcsPal, Model::A2000OcsNtsc] {
        let original = original_a2000(model);
        let mut runtime = AmigaRuntimeKind::blank(model);
        runtime.restore(&original).expect("original v67");
        let current = runtime.snapshot().expect("correct v68");
        legacy_v67::assert_migrated_save(&original, &current, true);
        let flag = original
            .iter()
            .zip(&current)
            .enumerate()
            .find(|(index, (old, new))| *index != 0 && old != new)
            .map(|(index, _)| index)
            .expect("the one approved RTC-presence bit");
        let mut malformed = current.clone();
        malformed[flag] = 0;
        let error = runtime
            .restore(&malformed)
            .expect_err("unfitted clock is invalid v68");
        assert!(error.to_string().contains("onboard RTC must be present"));
        assert!(runtime.snapshot().expect("after rejected restore") == current);
    }
}

#[test]
fn already_fitted_v67_states_restore_for_every_existing_profile() {
    for model in Model::VARIANTS {
        let source = AmigaRuntimeKind::blank(model);
        let current = source.snapshot().expect("v68 save");
        assert_eq!(current[0], 68);
        let mut legacy = current.clone();
        legacy[0] = 67;
        let mut restored = AmigaRuntimeKind::blank(model);
        restored.restore(&legacy).expect("v67 reader");
        assert!(
            restored.snapshot().expect("current save") == current,
            "{model:?}: every payload byte"
        );
    }
}
