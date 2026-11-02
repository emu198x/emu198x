//! Shared runtime dispatch never substitutes an ECS board or region.
use emu198x_shell::{MachineCore, ResetKind};
use runtime_commodore_amiga::{
    AmigaA500PlusNtscRuntime, AmigaA500PlusPalRuntime, AmigaA600NtscRuntime, AmigaA600PalRuntime,
    AmigaRuntimeKind, Model,
};
const ROM: &[u8] =
    include_bytes!("../../../test-data/commodore-amiga/ecs-v66/active-progressive.rom");
const BOARDS: [(Model, &str); 4] = [
    (Model::A500PlusEcsPal, "commodore-amiga-a500-plus-pal"),
    (Model::A500PlusEcsNtsc, "commodore-amiga-a500-plus-ntsc"),
    (Model::A600EcsPal, "commodore-amiga-a600-pal"),
    (Model::A600EcsNtsc, "commodore-amiga-a600-ntsc"),
];
#[test]
fn constructors_refuse_other_board_and_region_profiles() {
    for (model, _) in BOARDS {
        assert_eq!(
            AmigaA500PlusPalRuntime::new(model, ROM.to_vec()).is_ok(),
            model == Model::A500PlusEcsPal
        );
        assert_eq!(
            AmigaA500PlusNtscRuntime::new(model, ROM.to_vec()).is_ok(),
            model == Model::A500PlusEcsNtsc
        );
        assert_eq!(
            AmigaA600PalRuntime::new(model, ROM.to_vec()).is_ok(),
            model == Model::A600EcsPal
        );
        assert_eq!(
            AmigaA600NtscRuntime::new(model, ROM.to_vec()).is_ok(),
            model == Model::A600EcsNtsc
        );
    }
}
#[test]
fn dispatch_and_reset_keep_each_exact_owner() {
    for (model, id) in BOARDS {
        let mut runtime = AmigaRuntimeKind::blank(model);
        for _ in 0..2 {
            let owner = match &runtime {
                AmigaRuntimeKind::A500PlusPal(r) => r.machine().machine_id(),
                AmigaRuntimeKind::A500PlusNtsc(r) => r.machine().machine_id(),
                AmigaRuntimeKind::A600Pal(r) => r.machine().machine_id(),
                AmigaRuntimeKind::A600Ntsc(r) => r.machine().machine_id(),
                _ => panic!("wrong ECS dispatcher"),
            };
            assert_eq!(owner, id);
            assert_eq!(runtime.profile().machine_id.as_str(), id);
            runtime.reset(ResetKind::Hard);
        }
    }
}
#[test]
fn original_ids_and_aliases_stay_bound_to_their_owner() {
    for ((model, id), legacy_profile) in BOARDS.into_iter().zip([
        "commodore-amiga-a500-plus-ecs-pal",
        "commodore-amiga-a500-plus-ecs-ntsc",
        "commodore-amiga-a600-ecs-pal",
        "commodore-amiga-a600-ecs-ntsc",
    ]) {
        assert_eq!(Model::from_variant_id(id), Some(model));
        assert_eq!(
            AmigaRuntimeKind::blank(model).profile().profile_id.as_str(),
            legacy_profile
        );
    }
    for (alias, model) in [
        ("a500-plus", Model::A500PlusEcsPal),
        ("a500-plus-ntsc", Model::A500PlusEcsNtsc),
        ("a600", Model::A600EcsPal),
        ("a600-ntsc", Model::A600EcsNtsc),
    ] {
        assert_eq!(Model::from_variant_id(alias), Some(model));
    }
}
#[test]
fn foreign_profile_restores_are_atomic_for_every_pair() {
    for (target, _) in BOARDS {
        let mut runtime = AmigaRuntimeKind::blank(target);
        let before = runtime.snapshot().expect("before");
        for (source, _) in BOARDS {
            if source == target {
                continue;
            }
            let foreign = AmigaRuntimeKind::blank(source).snapshot().expect("foreign");
            assert!(runtime.restore(&foreign).is_err());
            assert!(runtime.snapshot().expect("after") == before);
        }
    }
}
