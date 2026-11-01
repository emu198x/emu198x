//! Fixed construction and restore identities under the shared family runtime.
use emu198x_shell::{MachineCore, ResetKind};
use runtime_commodore_amiga::{
    AmigaA1200NtscRuntime, AmigaA1200PalRuntime, AmigaRuntimeKind, Model,
};

const FIRMWARE: &[u8] =
    include_bytes!("../../../test-data/commodore-amiga/a1200-v66/active-progressive.rom");

#[test]
fn typed_construction_refuses_the_other_region() {
    assert!(AmigaA1200PalRuntime::new(Model::A1200AgaNtsc, FIRMWARE.to_vec()).is_err());
    assert!(AmigaA1200NtscRuntime::new(Model::A1200AgaPal, FIRMWARE.to_vec()).is_err());
    let pal = AmigaA1200PalRuntime::blank();
    let ntsc = AmigaA1200NtscRuntime::blank();
    assert_eq!(pal.model(), Model::A1200AgaPal);
    assert_eq!(ntsc.model(), Model::A1200AgaNtsc);
    assert_eq!(
        pal.profile().machine_id.as_str(),
        pal.machine().machine_id()
    );
    assert_eq!(
        ntsc.profile().machine_id.as_str(),
        ntsc.machine().machine_id()
    );
    assert_eq!(pal.profile().clock.rate.numerator_hz, 7_093_790);
    assert_eq!(ntsc.profile().clock.rate.numerator_hz, 7_159_090);
}

#[test]
fn family_dispatch_and_reset_retain_the_fixed_owner() {
    for model in Model::VARIANTS {
        let mut runtime = AmigaRuntimeKind::blank(model);
        runtime.reset(ResetKind::Hard);
        match runtime {
            AmigaRuntimeKind::AgaPal(inner) => {
                assert_eq!(model, Model::A1200AgaPal);
                assert_eq!(inner.machine().machine_id(), "commodore-amiga-a1200-pal");
                assert_eq!(
                    inner.profile().machine_id.as_str(),
                    inner.machine().machine_id()
                );
            }
            AmigaRuntimeKind::AgaNtsc(inner) => {
                assert_eq!(model, Model::A1200AgaNtsc);
                assert_eq!(inner.machine().machine_id(), "commodore-amiga-a1200-ntsc");
                assert_eq!(
                    inner.profile().machine_id.as_str(),
                    inner.machine().machine_id()
                );
            }
            AmigaRuntimeKind::Ocs(_) | AmigaRuntimeKind::Ecs(_) => assert!(!model.is_aga()),
        }
    }
}

#[test]
fn legacy_profiles_and_short_aliases_remain_available() {
    for (model, id, alias, owner) in [
        (
            Model::A1200AgaPal,
            "commodore-amiga-a1200-aga-pal",
            "a1200",
            "commodore-amiga-a1200-pal",
        ),
        (
            Model::A1200AgaNtsc,
            "commodore-amiga-a1200-aga-ntsc",
            "a1200-ntsc",
            "commodore-amiga-a1200-ntsc",
        ),
    ] {
        assert_eq!(model.profile_id(), id);
        assert_eq!(Model::from_variant_id(alias), Some(model));
        assert_eq!(Model::from_variant_id(owner), Some(model));
    }
}

#[test]
fn restoring_a_foreign_profile_does_not_mutate_the_live_runtime() {
    let mut ntsc =
        AmigaA1200NtscRuntime::new(Model::A1200AgaNtsc, FIRMWARE.to_vec()).expect("NTSC");
    let before = ntsc.snapshot().expect("before");
    let pal = include_bytes!(
        "../../../test-data/commodore-amiga/a1200-v66/commodore-amiga-a1200-aga-pal-progressive.state"
    );
    assert!(ntsc.restore(pal).is_err());
    assert!(
        ntsc.snapshot().expect("after") == before,
        "rejection is atomic"
    );
}
