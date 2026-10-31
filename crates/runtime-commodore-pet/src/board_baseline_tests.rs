//! Corrected v5 states and three native continuations, frozen before release.
use crate::{Model, PetRuntime};
use emu198x_shell::{
    AudioCapture, HostIo, LatestFrameCapture, MachineCore, MachineTime, MediaImage, MediaKind,
    MediaSet, NullTraceSink, StopReason,
};
use std::path::{Path, PathBuf};
fn runtime(model: Model) -> PetRuntime {
    let mut k = vec![0xea; 4096];
    k[..3].copy_from_slice(&[0x4c, 0, 0xf0]);
    k[0xffc..0xffe].copy_from_slice(&[0, 0xf0]);
    let mut c = vec![0; 2048];
    for bank in [0, 1024] {
        c[bank + 8..bank + 16].fill(0x81);
    }
    PetRuntime::new(model, k, vec![0; model.basic_size()], vec![0; 2048], c)
        .expect("project firmware")
}
fn frame(r: &mut PetRuntime) -> (emu198x_shell::CapturedFrame, Vec<u32>) {
    let mut picture = LatestFrameCapture::default();
    let mut sound = AudioCapture::default();
    let outcome = r
        .run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut picture,
            audio_sink: &mut sound,
            trace_sink: &mut NullTraceSink,
        })
        .expect("field");
    assert_eq!(outcome.stop_reason, StopReason::ReachedTarget);
    assert_eq!(picture.frames_seen(), 1);
    (
        picture.frame().expect("picture").clone(),
        sound
            .audio()
            .expect("audio")
            .samples
            .iter()
            .map(|v| v.to_bits())
            .collect(),
    )
}
fn specimen(model: Model) -> PetRuntime {
    let mut r = runtime(model);
    for _ in 0..118 {
        frame(&mut r);
    }
    let m = r.machine_mut().expect("machine");
    m.poke(0x8000, 1);
    m.poke(0x8001, 0x81);
    m.press_key(common_commodore_pet::PetKey::Plus);
    for (a, v) in [(0xe848, 100), (0xe84b, 0x10), (0xe84a, 0xf0)] {
        m.poke(a, v);
    }
    for _ in 0..137 {
        m.step_instruction();
    }
    let now = m.master_clock();
    r.set_time(MachineTime::new(now));
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "program-1",
        MediaKind::Program,
        &[0, 2, 0x42],
    ));
    r.load_media(&media).expect("queue");
    r
}
fn continuation(r: &mut PetRuntime) -> Vec<u8> {
    let mut fields = Vec::new();
    for _ in 0..3 {
        let before = r.time().get();
        let (picture, sound) = frame(r);
        assert!(
            picture
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p.as_slice() != &picture.pixels[..4]),
            "active raster"
        );
        assert!(!sound.is_empty(), "samples");
        fields.push((r.time().get() - before, picture, sound));
    }
    assert_eq!(
        r.machine().expect("machine").peek(0x200),
        0x42,
        "queued load applied"
    );
    postcard::to_allocvec(&(fields, r.snapshot().expect("final state"))).expect("output")
}
#[test]
fn corrected_v5_states_and_first_three_fields_remain_exact() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/commodore-pet/board-v5");
    for model in Model::ALL {
        let mut live = specimen(model);
        let saved = live.snapshot().expect("save");
        let output = continuation(&mut live);
        if let Some(dir) = std::env::var_os("EMU198X_PET_CAPTURE") {
            use std::io::Write;
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).expect("directory");
            for (suffix, bytes) in [("state", &saved), ("output", &output)] {
                std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(dir.join(format!("{}.{suffix}", model.model_id())))
                    .expect("new fixture only")
                    .write_all(bytes)
                    .expect("capture");
            }
            continue;
        }
        let original =
            std::fs::read(root.join(format!("{}.state", model.model_id()))).expect("frozen state");
        let expected = std::fs::read(root.join(format!("{}.output", model.model_id())))
            .expect("frozen output");
        assert!(saved == original, "{}: v5 state changed", model.model_id());
        assert!(
            output == expected,
            "{}: direct continuation changed",
            model.model_id()
        );
        let mut restored = runtime(model);
        restored.restore(&original).expect("restore");
        assert!(
            restored.snapshot().expect("reencode") == original,
            "reencode"
        );
        assert!(
            continuation(&mut restored) == expected,
            "{}: first pixels/sample bits/clocks/final state",
            model.model_id()
        );
    }
}
