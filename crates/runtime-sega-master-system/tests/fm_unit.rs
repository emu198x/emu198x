//! The YM2413 FM hardware through the runtime (#213): the Japanese model
//! has it built in, and any model can be fitted with an FM Sound Unit that
//! survives cartridge changes and model swaps.

use emu198x_shell::{FamilyRuntime, FirmwareSet, SessionQueryProvider};
use runtime_sega_master_system::{
    FmUnit, Model, SmsRuntime, SmsSessionQueryProvider, blank, with_cartridge,
};

fn query(rt: &SmsRuntime, path: &str) -> serde_json::Value {
    SmsSessionQueryProvider
        .query(rt, path)
        .expect("query does not error")
        .unwrap_or_else(|| panic!("path `{path}` resolves"))
        .value
}

#[test]
fn only_the_japanese_master_system_has_fm_built_in() {
    for model in Model::ALL {
        let rt = with_cartridge(model, vec![0u8; 0x8000]);
        let expected = if model == Model::SmsJapanNtsc {
            FmUnit::BuiltIn
        } else {
            FmUnit::None
        };
        assert_eq!(rt.fm_unit(), expected, "{model:?}");
        assert_eq!(query(&rt, "fm.unit"), format!("{expected:?}"));
    }
}

#[test]
fn a_fitted_sound_unit_outlives_cartridge_changes_and_model_swaps() {
    let mut rt = with_cartridge(Model::SmsNtsc, vec![0u8; 0x8000]);
    rt.set_fm_unit(FmUnit::SoundUnit);
    assert_eq!(rt.fm_unit(), FmUnit::SoundUnit);

    rt.insert_cartridge(vec![0u8; 0x8000]);
    assert_eq!(
        rt.fm_unit(),
        FmUnit::SoundUnit,
        "kept across a new cartridge"
    );

    let swapped = rt
        .replacement(Model::SmsPal, &FirmwareSet::new())
        .expect("model swap");
    assert_eq!(
        swapped.fm_unit(),
        FmUnit::SoundUnit,
        "kept across a model swap"
    );
}

#[test]
fn the_unit_can_be_removed_from_a_japanese_machine() {
    let mut rt = with_cartridge(Model::SmsJapanNtsc, vec![0u8; 0x8000]);
    rt.set_fm_unit(FmUnit::None);
    assert_eq!(rt.fm_unit(), FmUnit::None);
    assert!(rt.machine().expect("loaded").ym2413().is_none());
}

#[test]
fn a_blank_runtime_reports_the_unit_it_will_fit() {
    let mut rt = blank(Model::SmsNtsc);
    assert_eq!(rt.fm_unit(), FmUnit::None);
    rt.set_fm_unit(FmUnit::SoundUnit);
    assert_eq!(rt.fm_unit(), FmUnit::SoundUnit);
    rt.insert_cartridge(vec![0u8; 0x8000]);
    assert_eq!(rt.machine().expect("loaded").fm_unit(), FmUnit::SoundUnit);
}
