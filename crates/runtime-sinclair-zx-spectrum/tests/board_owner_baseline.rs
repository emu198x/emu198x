//! Original native output from 77440f443 and independent snow-fixed saves from
//! ffe704759, both captured before physical owner extraction. Full frames include
//! timestamps, dimensions, palette, pixels and audio sample bits. Synthetic
//! firmware is DI; JP with patterned display RAM. Preserve both immutable sets;
//! only envelope identity/version and the new board tag are normalized.
use emu198x_shell::{
    AudioCapture, FamilyRuntime, FirmwareImage, FirmwareSet, HostIo, LatestFrameCapture,
    MachineCore, NullTraceSink,
};
use runtime_sinclair_zx_spectrum::{Model, SpectrumLiveAccess, SpectrumRuntimeKind, profile_for};

fn runtime(model: Model) -> SpectrumRuntimeKind {
    let mut rom = [0; 16384];
    // DI; jump to self. Rendering and sound remain live while the CPU runs.
    rom[..4].copy_from_slice(&[0xf3, 0xc3, 1, 0]);
    let exrom = [0; 8192];
    let profile = profile_for(model);
    let mut firmware = FirmwareSet::new();
    for requirement in &profile.firmware {
        let id = requirement.id.as_ref();
        firmware.push(FirmwareImage::new(
            requirement.id.clone(),
            if id == "timex-ts2068-rom-1" {
                &exrom
            } else {
                &rom
            },
        ));
    }
    SpectrumRuntimeKind::from_firmware(model, &firmware).expect("synthetic firmware")
}
type Output = (u64, u32, u32, Vec<u8>, Option<Vec<u32>>, Vec<u32>);
fn frame(r: &mut SpectrumRuntimeKind) -> Output {
    let mut pixels = LatestFrameCapture::default();
    let mut audio = AudioCapture::default();
    r.run_frame(&mut HostIo {
        input_events: &[],
        frame_sink: &mut pixels,
        audio_sink: &mut audio,
        trace_sink: &mut NullTraceSink,
    })
    .expect("native boundary");
    assert_eq!(pixels.frames_seen(), 1);
    let f = pixels.frame().expect("frame");
    (
        f.timestamp.get(),
        f.width,
        f.height,
        f.pixels.clone(),
        f.palette.clone(),
        audio
            .audio()
            .expect("audio")
            .samples
            .iter()
            .map(|s| s.to_bits())
            .collect(),
    )
}
fn run_on_test_stack(test: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(test)
        .expect("test thread")
        .join()
        .expect("test result");
}

#[derive(serde::Deserialize)]
struct Baseline {
    source_commit: String,
    cases: Vec<Case>,
}

#[derive(serde::Deserialize)]
struct Case {
    id: String,
    profile: String,
    mode: u8,
    phase: u32,
    state_sha256: String,
    frames_sha256: String,
    final_sha256: String,
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn assert_hash(bytes: &[u8], expected: &str, id: &str, field: &str) {
    assert_eq!(sha256(bytes), expected, "{id}: {field}");
}

// v20 adds a physical-board enum immediately before the old machine body.
// The only normalisation is the approved envelope identity/version and that tag;
// every other saved byte, including clocks, RAM, ULA latches and audio, is pinned.
fn legacy_metadata(bytes: &[u8], model: Model, old_profile: &str) -> Vec<u8> {
    if model.physical_board().is_none() {
        return bytes.to_vec();
    }
    type Prefix = (u32, String, emu198x_shell::MachineTime, [u8; 8]);
    let ((version, profile, time, keys), body) =
        postcard::take_from_bytes::<Prefix>(bytes).expect("current envelope prefix");
    assert_eq!(version, 20);
    assert_eq!(profile, model.profile_id());
    let (board, old_body) =
        postcard::take_from_bytes::<common_sinclair_zx_spectrum_48k_class::PhysicalBoard>(body)
            .expect("independent physical board tag");
    assert_eq!(Some(board), model.physical_board());
    let mut legacy =
        postcard::to_allocvec(&(19_u32, old_profile, time, keys)).expect("old metadata");
    legacy.extend_from_slice(old_body);
    legacy
}

#[test]
fn original_profiles_keep_frozen_native_output_and_exact_continuation() {
    run_on_test_stack(|| {
        let baseline: Baseline = serde_json::from_str(include_str!(
            "fixtures/physical_boards/pre_split_77440f443.json"
        ))
        .expect("frozen pre-split manifest");
        assert_eq!(
            baseline.source_commit,
            "77440f4435ffd4700696a0e56f2050d71827a39b"
        );
        assert_eq!(baseline.cases.len(), 192);
        let snow_baseline: Baseline = serde_json::from_str(include_str!(
            "fixtures/physical_boards/pre_split_ffe704759.json"
        ))
        .expect("independently captured snow-fixed baseline");
        assert_eq!(
            snow_baseline.source_commit,
            "ffe704759803b7dddd0d82f022cf727c68b465ef"
        );
        assert_eq!(snow_baseline.cases.len(), 192);
        for (old, new) in baseline.cases.iter().zip(&snow_baseline.cases) {
            assert_eq!(
                (&old.id, &old.profile, old.mode, old.phase),
                (&new.id, &new.profile, new.mode, new.phase)
            );
            assert_eq!(
                old.frames_sha256, new.frames_sha256,
                "{}: preserve original native output",
                old.id
            );
        }
        let baseline = snow_baseline;

        let mut exercised = std::collections::BTreeSet::new();
        let mut routed = std::collections::BTreeSet::new();
        let mut comparisons = 0;

        for case in baseline.cases {
            assert!(exercised.insert(case.id.clone()), "duplicate frozen case");
            let models: Vec<_> = Model::ALL
                .into_iter()
                .filter(|model| match case.profile.as_str() {
                    "sinclair-zx-spectrum-16k-pal" => {
                        model.physical_board().is_some_and(|b| {
                            b.ula_family()
                                == common_sinclair_zx_spectrum_48k_class::UlaRevision::Ferranti6C
                        }) && model.ram_kib() == Some(16)
                    }
                    "sinclair-zx-spectrum-48k-pal" | "sinclair-zx-spectrum-plus-pal" => {
                        model.physical_board().is_some_and(|b| {
                            b.ula_family()
                                == common_sinclair_zx_spectrum_48k_class::UlaRevision::Ferranti6C
                        }) && model.ram_kib() == Some(48)
                    }
                    "sinclair-zx-spectrum-48k-issue1-pal" => {
                        model.physical_board()
                            == Some(common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue1)
                    }
                    "sinclair-zx-spectrum-48k-issue2-pal" => {
                        model.physical_board()
                            == Some(common_sinclair_zx_spectrum_48k_class::PhysicalBoard::Issue2)
                    }
                    profile => model.profile_id() == profile,
                })
                .collect();
            assert!(
                !models.is_empty(),
                "{}: every original case needs an explicit route",
                case.id
            );
            for model in models {
                comparisons += 1;
                routed.insert((model.profile_id(), case.mode, case.phase));
                let mut r = runtime(model);
                for addr in 0x4000_u16..0x8000 {
                    r.write_byte(addr, (addr as u8).wrapping_mul(17) ^ 0x5a);
                }
                r.port_write(0xff, case.mode | 0x28);
                for (reg, value) in [(0, 24), (1, 0), (7, 0x3e), (8, 15)] {
                    r.port_write(0xf5, reg);
                    r.port_write(0xf6, value);
                    r.port_write(0xfffd, reg);
                    r.port_write(0xbffd, value);
                }
                r.port_write(0xfe, 0x12);
                frame(&mut r);
                frame(&mut r);
                if case.phase > 0 {
                    assert_eq!(
                        r.profile_cycles(case.phase).expect("partial phase").ticks,
                        u64::from(case.phase)
                    );
                }
                let state = r.snapshot().expect("initial state");
                let matching_old_population = !(model.ram_kib() == Some(16)
                    && model.physical_board().is_some_and(|b| {
                        b.ula_family()
                            == common_sinclair_zx_spectrum_48k_class::UlaRevision::Ferranti5C
                    }));
                if matching_old_population {
                    assert_hash(
                        &legacy_metadata(&state, model, &case.profile),
                        &case.state_sha256,
                        &case.id,
                        "initial save body with original metadata",
                    );
                }
                let mut restored = runtime(model);
                restored
                    .restore(&state)
                    .expect("restore frozen-equivalent state");
                assert_eq!(
                    restored.snapshot().expect("restored bytes"),
                    state,
                    "{}: exact round trip",
                    case.id
                );
                let frames = [frame(&mut r), frame(&mut r)];
                let resumed = [frame(&mut restored), frame(&mut restored)];
                assert_eq!(resumed, frames, "{}: first resumed native output", case.id);
                assert_hash(
                    &postcard::to_allocvec(&frames).expect("native outputs"),
                    &case.frames_sha256,
                    &case.id,
                    "complete native output",
                );
                let final_state = r.snapshot().expect("final state");
                if matching_old_population {
                    assert_hash(
                        &legacy_metadata(&final_state, model, &case.profile),
                        &case.final_sha256,
                        &case.id,
                        "final save body with original metadata",
                    );
                }
                assert_eq!(
                    restored.snapshot().expect("resumed final state"),
                    final_state,
                    "{}: resumed final state",
                    case.id
                );
            }
        }
        assert_eq!(exercised.len(), 192);
        assert_eq!(comparisons, 352);
        assert_eq!(
            routed.len(),
            296,
            "every current board/RAM/mode/phase input"
        );
    });
}
