//! Immutable v5 evidence and exact v6 native continuations.
use crate::{Model, PetRuntime};
use emu198x_shell::{
    AudioCapture, HostIo, LatestFrameCapture, MachineCore, MachineTime, MediaImage, MediaKind,
    MediaSet, NullTraceSink, StopReason,
};
use std::path::Path;
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
fn current_v6_states_resume_every_native_pixel_sample_clock_and_state() {
    for model in Model::ALL {
        let mut live = specimen(model);
        let saved = live.snapshot().expect("current v6");
        let (version, _) = postcard::take_from_bytes::<u16>(&saved).expect("header");
        assert_eq!(version, 6);
        let expected = continuation(&mut live);
        let mut restored = runtime(model);
        restored.restore(&saved).expect("current restore");
        assert_eq!(restored.snapshot().expect("round trip"), saved);
        assert_eq!(
            continuation(&mut restored),
            expected,
            "{}",
            model.model_id()
        );
    }
}
#[test]
fn original_v5_specimens_stay_intact_and_reject_before_payload_decoding() {
    use sha2::{Digest, Sha256};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/commodore-pet/board-v5");
    let records = [
        (
            Model::Pet2001n32k,
            "output",
            "7fca4e9bfa109372d7fce835d94d8179835abcf7951bba9323a47580df6975a9",
        ),
        (
            Model::Pet2001n32k,
            "state",
            "35e470f1ba5320f6abfc20631cc2cc7ed30afe40c1393ce71d3fa2fca0cb08b5",
        ),
        (
            Model::Pet40Col,
            "output",
            "6f8f14b4575e442a48d5e50de59e4892ec2b0c7c22c253da519ab62d504d0035",
        ),
        (
            Model::Pet40Col,
            "state",
            "aa1978f884cee04b277c21dfc2560dffe61909ca1af78352af035fc1382e942d",
        ),
        (
            Model::Pet80Col,
            "output",
            "70ca7b5fa7c2e18945a32861000919b235773bd89314ce28aec90448b9bbfc27",
        ),
        (
            Model::Pet80Col,
            "state",
            "a8b2fd0da8e4529d55d183db3c75f9cefc15d8ab5dd1052bcd749f3760aa4477",
        ),
    ];
    for (model, suffix, expected) in records {
        let old = std::fs::read(root.join(format!("{}.{suffix}", model.model_id())))
            .expect("original specimen");
        let hash = Sha256::digest(&old)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(hash, expected, "immutable {}.{suffix}", model.model_id());
        if suffix == "state" {
            let mut r = specimen(model);
            let before = r.snapshot().expect("occupied receiver");
            for bytes in [&old[..], &[5u8][..]] {
                assert!(
                    matches!(r.restore(bytes),Err(emu198x_shell::MachineError::InvalidSnapshot{reason})
                    if reason=="unsupported snapshot version 5; expected 6")
                );
                assert_eq!(r.snapshot().expect("atomic rejection"), before);
            }
        }
    }
}
#[test]
fn existing_three_boards_keep_their_frozen_native_picture_audio_and_clocks() {
    type Fields = Vec<(u64, emu198x_shell::CapturedFrame, Vec<u32>)>;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/commodore-pet/board-v5");
    for model in [Model::Pet2001n32k, Model::Pet40Col, Model::Pet80Col] {
        let old =
            std::fs::read(root.join(format!("{}.output", model.model_id()))).expect("old output");
        let (expected, _): (Fields, Vec<u8>) =
            postcard::from_bytes(&old).expect("output wrapper only, not legacy state");
        let mut r = specimen(model);
        let actual = continuation(&mut r);
        let (actual, _): (Fields, Vec<u8>) = postcard::from_bytes(&actual).expect("current output");
        assert_eq!(actual, expected, "{} native output", model.model_id());
    }
}
