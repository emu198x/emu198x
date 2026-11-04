//! The 48K's Issue 1 and Issue 2 boards, beside the Issue 3 `spectrum_48k`:
//! each is its own variant, saves and restores as itself, and won't take
//! another ULA family's saved state.
use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore, MachineError};
use runtime_sinclair_zx_spectrum::{Model, SpectrumLiveAccess, SpectrumRuntimeKind};

fn on_stack(test: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(test)
        .expect("test thread")
        .join()
        .expect("test result");
}

fn runtime(model: Model) -> SpectrumRuntimeKind {
    let mut rom = [0; 16384];
    rom[..4].copy_from_slice(&[0xf3, 0xc3, 1, 0]);
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("sinclair-zx-spectrum-48k-rom", &rom));
    SpectrumRuntimeKind::from_firmware(model, &firmware).expect("48K board")
}

const BOARDS: [(Model, &str, &str); 3] = [
    (
        Model::SpectrumIssue1Ram48kPal,
        "spectrum_48k_issue1",
        "sinclair-zx-spectrum-48k-issue1-pal",
    ),
    (
        Model::SpectrumIssue2Ram48kPal,
        "spectrum_48k_issue2",
        "sinclair-zx-spectrum-48k-issue2-pal",
    ),
    (
        Model::SpectrumIssue3Ram48kPal,
        "spectrum_issue3_48k",
        "sinclair-zx-spectrum-issue3-48k-pal",
    ),
];

#[test]
fn each_board_is_its_own_variant_and_profile() {
    on_stack(|| {
        for (model, variant, profile) in BOARDS {
            assert_eq!(Model::from_variant_id(variant), Some(model), "{variant}");
            let r = runtime(model);
            assert_eq!(r.profile().profile_id.as_str(), profile);
            let kind = match &r {
                SpectrumRuntimeKind::Issue1Ram48k(_) => Model::SpectrumIssue1Ram48kPal,
                SpectrumRuntimeKind::Issue2Ram48k(_) => Model::SpectrumIssue2Ram48kPal,
                SpectrumRuntimeKind::Issue3Ram48k(_) => Model::SpectrumIssue3Ram48kPal,
                _ => panic!("{variant} built another family member"),
            };
            assert_eq!(kind, model);
        }
    });
}

#[test]
fn each_board_saves_and_restores_as_itself() {
    on_stack(|| {
        for (model, variant, _) in BOARDS {
            let mut r = runtime(model);
            r.write_byte(0x9000, 0x5a);
            let saved = r.snapshot().expect("save");
            let mut restored = runtime(model);
            restored.restore(&saved).expect(variant);
            assert!(
                restored.snapshot().expect("resave") == saved,
                "{variant} round trip"
            );
        }
    });
}

/// A save names its profile in its header; a 48K board's payload names its
/// ULA family. A payload from the other family is refused, even under this
/// board's header, and nothing changes.
#[test]
fn another_ula_familys_state_is_refused_under_a_matching_header() {
    on_stack(|| {
        for (target, source) in [
            (
                Model::SpectrumIssue3Ram48kPal,
                Model::SpectrumIssue1Ram48kPal,
            ),
            (
                Model::SpectrumIssue3Ram48kPal,
                Model::SpectrumIssue2Ram48kPal,
            ),
            (
                Model::SpectrumIssue1Ram48kPal,
                Model::SpectrumIssue3Ram48kPal,
            ),
            (
                Model::SpectrumIssue2Ram48kPal,
                Model::SpectrumIssue3Ram48kPal,
            ),
        ] {
            let bytes = runtime(source).snapshot().expect("source save");
            let (version, tail) = postcard::take_from_bytes::<u32>(&bytes).expect("version");
            let (profile, payload) = postcard::take_from_bytes::<String>(tail).expect("profile");
            assert_eq!(profile, source.profile_id());
            let mut forged =
                postcard::to_allocvec(&(version, target.profile_id())).expect("header");
            forged.extend_from_slice(payload);
            let mut r = runtime(target);
            r.write_byte(0x9000, 0x5a);
            let before = r.snapshot().expect("before");
            assert!(
                matches!(
                    r.restore(&forged),
                    Err(MachineError::InvalidSnapshot { .. })
                ),
                "{source:?} state under {target:?} header"
            );
            assert!(
                r.snapshot().expect("after") == before,
                "refused without change"
            );
        }
    });
}
