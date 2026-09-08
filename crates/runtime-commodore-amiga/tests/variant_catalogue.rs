//! Presets remain selectable without conflating base models, expansions or region.

use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore, Region};
use runtime_commodore_amiga::{AmigaRuntimeKind, Model, profiles};

#[test]
fn every_profile_has_a_distinct_selectable_id_and_builds_with_its_region() {
    let profiles = profiles();
    let mut built_profiles = Vec::new();
    for model in Model::VARIANTS {
        assert_eq!(Model::from_variant_id(model.variant_id()), Some(model));
        let rom = vec![
            0;
            if model.is_a1000() {
                64 * 1024
            } else {
                512 * 1024
            }
        ];
        let mut firmware = FirmwareSet::new();
        let sources = model.firmware_sources();
        firmware.push(FirmwareImage::new(sources[0].id, &rom));
        let runtime = AmigaRuntimeKind::from_firmware(model, &firmware).expect("construct preset");
        assert_eq!(runtime.model(), model);
        assert_eq!(
            runtime.profile().region,
            if model.is_ntsc() {
                Region::Ntsc
            } else {
                Region::Pal
            }
        );
        assert_eq!(runtime.native_frame_ticks(), model.frame_ticks());
        built_profiles.push(runtime.profile().profile_id.as_str().to_owned());
    }
    built_profiles.sort();
    built_profiles.dedup();
    let mut expected: Vec<_> = profiles
        .iter()
        .map(|p| p.profile_id.as_str().to_owned())
        .collect();
    expected.sort();
    assert_eq!(
        built_profiles, expected,
        "no runtime profile can disappear from selection"
    );
}

#[test]
fn legacy_ids_keep_pal_and_expansions_share_their_base_model() {
    for id in [
        "a1000",
        "a500",
        "a500-a501",
        "a500-maxed",
        "a500-gvp-a530",
        "a500-plus",
        "a600",
        "a1200",
        "a2000",
    ] {
        let pal = Model::from_variant_id(id).expect("legacy id");
        let ntsc = Model::from_variant_id(&format!("{id}-ntsc")).expect("NTSC counterpart");
        assert!(!pal.is_ntsc());
        assert!(ntsc.is_ntsc());
        assert_eq!(pal.base_model_label(), ntsc.base_model_label());
        assert_eq!(pal.configuration_label(), ntsc.configuration_label());
        assert_eq!(pal.ram_config(), ntsc.ram_config());
    }
    for model in [
        Model::A500OcsPal,
        Model::A500OcsPalA501,
        Model::A500OcsPalMaxed,
        Model::A500OcsPalGvpA530,
    ] {
        assert_eq!(model.base_model_label(), "Amiga 500");
    }
    assert_ne!(
        Model::A500PlusEcsPal.base_model_label(),
        Model::A500OcsPal.base_model_label()
    );
    assert!(
        Model::A500OcsPalGvpA530
            .configuration_label()
            .contains("research")
    );
}
