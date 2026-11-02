//! Fixed owners retain legacy profiles, lifecycle and atomic restore.
use emu198x_shell::{MachineCore, ResetKind};
use runtime_commodore_amiga::{AmigaRuntimeKind, Model};
const KICK: &[u8] = include_bytes!(
    "../../../test-data/commodore-amiga/ocs-v67/firmware/kickstart-active-progressive.rom"
);
const BOOT: &[u8] = include_bytes!(
    "../../../test-data/commodore-amiga/ocs-v67/firmware/handoff-active-progressive.rom"
);
const PROFILES: [(Model, &str); 12] = [
    (Model::A1000OcsPal, "commodore-amiga-a1000-pal"),
    (Model::A1000OcsNtsc, "commodore-amiga-a1000-ntsc"),
    (Model::A500OcsPal, "commodore-amiga-a500-rev5-pal"),
    (Model::A500OcsPalA501, "commodore-amiga-a500-rev5-pal"),
    (Model::A500OcsPalGvpA530, "commodore-amiga-a500-rev5-pal"),
    (Model::A500OcsNtsc, "commodore-amiga-a500-rev5-ntsc"),
    (Model::A500OcsNtscA501, "commodore-amiga-a500-rev5-ntsc"),
    (Model::A500OcsNtscGvpA530, "commodore-amiga-a500-rev5-ntsc"),
    (Model::A500OcsPalMaxed, "commodore-amiga-a500-rev6a-pal"),
    (Model::A500OcsNtscMaxed, "commodore-amiga-a500-rev6a-ntsc"),
    (Model::A2000OcsPal, "commodore-amiga-a2000cr-rev6-pal"),
    (Model::A2000OcsNtsc, "commodore-amiga-a2000cr-rev6-ntsc"),
];
fn firmware(model: Model) -> Vec<u8> {
    if model.is_a1000() {
        BOOT.to_vec()
    } else {
        KICK.to_vec()
    }
}
#[test]
fn constructors_accept_only_their_board_and_region_profiles() {
    for model in Model::VARIANTS {
        assert_eq!(
            runtime_commodore_amiga::AmigaA1000PalRuntime::new(model, firmware(model)).is_ok(),
            matches!(model, Model::A1000OcsPal)
        );
        assert_eq!(
            runtime_commodore_amiga::AmigaA1000NtscRuntime::new(model, firmware(model)).is_ok(),
            matches!(model, Model::A1000OcsNtsc)
        );
        assert_eq!(
            runtime_commodore_amiga::AmigaA500Rev5PalRuntime::new(model, firmware(model)).is_ok(),
            matches!(
                model,
                Model::A500OcsPal | Model::A500OcsPalA501 | Model::A500OcsPalGvpA530
            )
        );
        assert_eq!(
            runtime_commodore_amiga::AmigaA500Rev5NtscRuntime::new(model, firmware(model)).is_ok(),
            matches!(
                model,
                Model::A500OcsNtsc | Model::A500OcsNtscA501 | Model::A500OcsNtscGvpA530
            )
        );
        assert_eq!(
            runtime_commodore_amiga::AmigaA500Rev6APalRuntime::new(model, firmware(model)).is_ok(),
            matches!(model, Model::A500OcsPalMaxed)
        );
        assert_eq!(
            runtime_commodore_amiga::AmigaA500Rev6ANtscRuntime::new(model, firmware(model)).is_ok(),
            matches!(model, Model::A500OcsNtscMaxed)
        );
        assert_eq!(
            runtime_commodore_amiga::AmigaA2000CrRev6PalRuntime::new(model, firmware(model))
                .is_ok(),
            matches!(model, Model::A2000OcsPal)
        );
        assert_eq!(
            runtime_commodore_amiga::AmigaA2000CrRev6NtscRuntime::new(model, firmware(model))
                .is_ok(),
            matches!(model, Model::A2000OcsNtsc)
        );
    }
}
#[test]
fn all_profiles_dispatch_reset_and_keep_their_ids() {
    for (model, id) in PROFILES {
        let mut runtime = AmigaRuntimeKind::blank(model);
        for _ in 0..2 {
            let owner = match &runtime {
                AmigaRuntimeKind::A1000Pal(r) => r.machine().machine_id(),
                AmigaRuntimeKind::A1000Ntsc(r) => r.machine().machine_id(),
                AmigaRuntimeKind::A500Rev5Pal(r) => r.machine().machine_id(),
                AmigaRuntimeKind::A500Rev5Ntsc(r) => r.machine().machine_id(),
                AmigaRuntimeKind::A500Rev6APal(r) => r.machine().machine_id(),
                AmigaRuntimeKind::A500Rev6ANtsc(r) => r.machine().machine_id(),
                AmigaRuntimeKind::A2000CrRev6Pal(r) => r.machine().machine_id(),
                AmigaRuntimeKind::A2000CrRev6Ntsc(r) => r.machine().machine_id(),
                _ => panic!("wrong remaining-Amiga dispatcher"),
            };
            assert_eq!(owner, id);
            assert_eq!(runtime.profile().machine_id.as_str(), id);
            assert_eq!(runtime.profile().profile_id.as_str(), model.profile_id());
            assert_eq!(Model::from_variant_id(model.variant_id()), Some(model));
            runtime.reset(ResetKind::Hard);
        }
    }
}
#[test]
fn every_foreign_family_profile_is_rejected_without_mutation() {
    for (target, _) in PROFILES {
        let mut runtime = AmigaRuntimeKind::blank(target);
        let before = runtime.snapshot().expect("before");
        for source in Model::VARIANTS {
            if source == target {
                continue;
            }
            let foreign = AmigaRuntimeKind::blank(source).snapshot().expect("foreign");
            assert!(
                runtime.restore(&foreign).is_err(),
                "{source:?} into {target:?}"
            );
            assert!(
                runtime.snapshot().expect("after") == before,
                "{source:?} into {target:?} mutated target"
            );
        }
    }
}
#[test]
fn owner_ids_resolve_to_the_existing_canonical_profiles() {
    assert_eq!(
        Model::from_variant_id("commodore-amiga-a1000-pal"),
        Some(Model::A1000OcsPal)
    );
    assert_eq!(
        Model::from_variant_id("commodore-amiga-a1000-ntsc"),
        Some(Model::A1000OcsNtsc)
    );
    assert_eq!(
        Model::from_variant_id("commodore-amiga-a500-rev5-pal"),
        Some(Model::A500OcsPal)
    );
    assert_eq!(
        Model::from_variant_id("commodore-amiga-a500-rev5-ntsc"),
        Some(Model::A500OcsNtsc)
    );
    assert_eq!(
        Model::from_variant_id("commodore-amiga-a500-rev6a-pal"),
        Some(Model::A500OcsPalMaxed)
    );
    assert_eq!(
        Model::from_variant_id("commodore-amiga-a500-rev6a-ntsc"),
        Some(Model::A500OcsNtscMaxed)
    );
    assert_eq!(
        Model::from_variant_id("commodore-amiga-a2000cr-rev6-pal"),
        Some(Model::A2000OcsPal)
    );
    assert_eq!(
        Model::from_variant_id("commodore-amiga-a2000cr-rev6-ntsc"),
        Some(Model::A2000OcsNtsc)
    );
}
