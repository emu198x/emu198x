//! Frozen before physical board extraction at 77440f443. These hashes cover full
//! native frames (timestamps, dimensions, palette, pixels and audio sample bits)
//! and complete saves. Synthetic firmware is DI; JP, with patterned display RAM.
//! Preserve the originals: a later identity migration needs explicit routing and
//! compatibility assertions, not regenerated expectations.
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
        let mut exercised = std::collections::BTreeSet::new();
        for case in baseline.cases {
            assert!(exercised.insert(case.id.clone()), "duplicate frozen case");
            let model = Model::ALL
                .into_iter()
                .find(|model| model.profile_id() == case.profile)
                .expect(
                    "every frozen profile must remain covered or gain an explicit board mapping",
                );
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
            assert_hash(&state, &case.state_sha256, &case.id, "initial save");
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
            assert_hash(&final_state, &case.final_sha256, &case.id, "final save");
            assert_eq!(
                restored.snapshot().expect("resumed final state"),
                final_state,
                "{}: resumed final state",
                case.id
            );
        }
        assert_eq!(exercised.len(), 192);
    });
}
