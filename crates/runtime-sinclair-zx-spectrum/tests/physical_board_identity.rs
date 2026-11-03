//! An envelope label must not be able to change the saved physical board.
use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MachineCore};
use runtime_sinclair_zx_spectrum::{Model, SpectrumLiveAccess, SpectrumRuntimeKind};

fn runtime(model: Model) -> SpectrumRuntimeKind {
    let mut rom = [0_u8; 16384];
    rom[..4].copy_from_slice(&[0xf3, 0xc3, 1, 0]);
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("sinclair-zx-spectrum-48k-rom", &rom));
    SpectrumRuntimeKind::from_firmware(model, &firmware).expect("synthetic firmware")
}

#[test]
fn relabelling_issue1_as_issue2_cannot_change_saved_board_wiring() {
    let source = Model::SpectrumIssue1Ram48kPal;
    let target = Model::SpectrumIssue2Ram48kPal;
    let mut bytes = runtime(source).snapshot().expect("source save");
    let from = source.profile_id().as_bytes();
    let to = target.profile_id().as_bytes();
    assert_eq!(from.len(), to.len());
    let offsets: Vec<_> = bytes
        .windows(from.len())
        .enumerate()
        .filter_map(|(offset, chunk)| (chunk == from).then_some(offset))
        .collect();
    assert_eq!(offsets.len(), 1, "replace exactly the envelope profile");
    bytes[offsets[0]..offsets[0] + from.len()].copy_from_slice(to);
    let mut machine = runtime(target);
    let before = machine.snapshot().expect("target before restore");
    machine
        .restore(&bytes)
        .expect_err("same ULA family does not establish the physical board");
    assert_eq!(machine.snapshot().expect("target after rejection"), before);
}

type Prefix = (u32, String, emu198x_shell::MachineTime, [u8; 8]);

#[test]
fn every_foreign_board_or_ram_population_is_rejected_atomically() {
    let models: Vec<_> = Model::ALL
        .into_iter()
        .filter(|m| m.physical_board().is_some())
        .collect();
    assert_eq!(models.len(), 18);
    let mut rejected = 0;
    let mut restored = 0;
    for (source_index, &source) in models.iter().enumerate() {
        let mut original = runtime(source);
        original.write_byte(0x4000, source_index as u8);
        original.profile_cycles(137).expect("partial native phase");
        let save = original.snapshot().expect("source save");
        let ((version, _, time, keys), body) =
            postcard::take_from_bytes::<Prefix>(&save).expect("prefix");
        assert_eq!(version, 18);
        for (target_index, &target) in models.iter().enumerate() {
            let mut forged = postcard::to_allocvec(&(version, target.profile_id(), time, keys))
                .expect("relabelled envelope");
            forged.extend_from_slice(body);
            let mut machine = runtime(target);
            machine.write_byte(0x4000, 255 - target_index as u8);
            machine.profile_cycles(17).expect("target phase");
            let before = machine.snapshot().expect("target before restore");
            if source == target {
                machine.restore(&forged).expect("matching board and RAM");
                assert_eq!(machine.snapshot().expect("exact matching save"), save);
                restored += 1;
            } else {
                machine
                    .restore(&forged)
                    .expect_err("a relabelled header cannot change board or fitted RAM");
                assert_eq!(machine.snapshot().expect("atomic rejection"), before);
                rejected += 1;
            }
        }
    }
    assert_eq!(restored, 18);
    assert_eq!(rejected, 306);
}

#[test]
fn all_original_v17_profiles_are_rejected_before_payload_decode() {
    use sha2::{Digest, Sha256};
    let originals: &[(&[u8], &str)] = &[
        (
            include_bytes!(
                "fixtures/physical_boards/sinclair-zx-spectrum-16k-pal-mode0-phase137.state"
            )
            .as_slice(),
            "b53e0d74a5afe20bac15e40953769f3d4bdd429d52427f1f1b256c7a7747896d",
        ),
        (
            include_bytes!(
                "fixtures/physical_boards/sinclair-zx-spectrum-48k-pal-mode0-phase137.state"
            )
            .as_slice(),
            "ba4d55487177f516e5b0cf7985e0c46c2f3faca7703573b69f620c92e6fb74b5",
        ),
        (
            include_bytes!(
                "fixtures/physical_boards/sinclair-zx-spectrum-48k-issue1-pal-mode0-phase137.state"
            )
            .as_slice(),
            "1d66b5b18d9975d0d561078ad4bc4ea06b46d033dffc6cd58ee917077458503e",
        ),
        (
            include_bytes!(
                "fixtures/physical_boards/sinclair-zx-spectrum-48k-issue2-pal-mode0-phase137.state"
            )
            .as_slice(),
            "12c5d5dc548daf56e80ec3888a26e6b7369227b3001c42940b57d222b2653ed7",
        ),
        (
            include_bytes!(
                "fixtures/physical_boards/sinclair-zx-spectrum-plus-pal-mode0-phase137.state"
            )
            .as_slice(),
            "6b4e418d38fc71aaf2afed044d7fddeab94586b7a235b0c9e9ca45e5fefab80d",
        ),
    ];
    assert_eq!(originals.len(), 5);
    let mut checked = 0;
    for model in Model::ALL
        .into_iter()
        .filter(|m| m.physical_board().is_some())
    {
        for &(bytes, expected) in originals {
            let actual: String = Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(actual, expected, "retained original must not change");
            let ((version, _, _, _), _) =
                postcard::take_from_bytes::<Prefix>(bytes).expect("original prefix");
            assert_eq!(version, 17);
            let mut machine = runtime(model);
            let before = machine.snapshot().expect("before legacy rejection");
            let error = machine
                .restore(bytes)
                .expect_err("v17 lacks independent physical identity");
            assert!(
                matches!(error, emu198x_shell::MachineError::InvalidSnapshot { ref reason }
                if reason == "unsupported snapshot version 17; expected 18"),
                "{error}"
            );
            assert_eq!(machine.snapshot().expect("unchanged"), before);
            // Even a version-only payload must reject through the same boundary.
            let short = postcard::to_allocvec(&17_u32).expect("version prefix");
            let short_error = machine
                .restore(&short)
                .expect_err("before decoding the absent body");
            assert!(
                matches!(short_error, emu198x_shell::MachineError::InvalidSnapshot { ref reason }
                if reason == "unsupported snapshot version 17; expected 18"),
                "{short_error}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 90);
}
