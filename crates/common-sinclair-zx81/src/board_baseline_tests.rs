//! Original engine capture and replay at partial CPU/video/tape phases.
use super::*;
use std::path::Path;
#[derive(Serialize)]
struct EnvelopeRef<'a> {
    version: u16,
    time: u64,
    model_id: &'a str,
    machine: Option<&'a Zx81State>,
}
#[derive(Deserialize)]
struct Envelope {
    version: u16,
    time: u64,
    model_id: String,
    machine: Option<Zx81State>,
}
fn snapshot(machine: &Zx81State) -> Vec<u8> {
    postcard::to_allocvec(&EnvelopeRef {
        version: 5,
        time: machine.master_clock(),
        model_id: "sinclair-zx81",
        machine: Some(machine),
    })
    .expect("legacy envelope")
}
fn continued(machine: &mut Zx81State) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut frames = Vec::new();
    let mut audio = Vec::new();
    for _ in 0..3 {
        machine.run_frame();
        frames.push((
            machine.master_clock(),
            machine.framebuffer_width(),
            machine.framebuffer_height(),
            machine.framebuffer().to_vec(),
        ));
        audio.push((
            machine.master_clock(),
            machine
                .take_audio_buffer()
                .iter()
                .map(|s| s.to_bits())
                .collect::<Vec<_>>(),
        ));
    }
    (
        snapshot(machine),
        postcard::to_allocvec(&frames).expect("frames"),
        postcard::to_allocvec(&audio).expect("audio"),
    )
}
fn prepared(ram: usize, standard: TelevisionStandard) -> Zx81State {
    let rom = include_bytes!("../../../test-data/synthetic-firmware/sinclair-zx81.rom").to_vec();
    let mut m = Zx81State::new(rom, ram).expect("synthetic firmware");
    m.set_television_standard(standard);
    for _ in 0..20 {
        m.run_frame();
    }
    m.press_key(Zx81Key::A);
    m.poke(0x4100, 0xA5);
    m.insert_tape(&[700, 701, 5000, 15000, 99999, 180001]);
    m.start_tape_recording();
    for _ in 0..37 {
        m.tick_tstate();
    }
    m.take_audio_buffer();
    assert_eq!(m.peek(0x4100), 0xA5);
    assert!(m.tape_remaining() > 0);
    m
}
fn all(root: &Path, capture: bool) {
    for (id, ram, standard) in [
        ("sinclair-zx81", 1024, TelevisionStandard::FiftyHz),
        ("sinclair-zx81-16k", 16384, TelevisionStandard::FiftyHz),
        ("timex-ts1000", 2048, TelevisionStandard::SixtyHz),
    ] {
        let mut m = prepared(ram, standard);
        let initial = snapshot(&m);
        let (after, frames, audio) = continued(&mut m);
        for (suffix, value) in ["state", "after.state", "frames", "audio"]
            .into_iter()
            .zip([initial.clone(), after, frames, audio])
        {
            let file = root.join(format!("{id}.{suffix}"));
            if capture {
                std::fs::write(file, value).expect("write");
            } else {
                assert!(
                    value == std::fs::read(file).expect("frozen original"),
                    "original {id}.{suffix}"
                );
            }
        }
        let envelope: Envelope = postcard::from_bytes(&initial).expect("decode original");
        assert_eq!(envelope.version, 5);
        assert_eq!(envelope.model_id, "sinclair-zx81");
        let mut restored = envelope.machine.expect("hardware");
        assert_eq!(envelope.time, restored.master_clock());
        restored.cpu_mut().rehydrate_walker_sequence();
        assert_eq!(snapshot(&restored), initial);
        let (after, frames, audio) = continued(&mut restored);
        for (suffix, value) in ["after.state", "frames", "audio"]
            .into_iter()
            .zip([after, frames, audio])
        {
            assert!(
                value == std::fs::read(root.join(format!("{id}.{suffix}"))).expect("original"),
                "restored {id}.{suffix}"
            );
        }
    }
}
#[test]
fn original_states_and_output_continue_identically() {
    all(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/zx81/board-split-v5"),
        false,
    );
}
#[test]
#[ignore = "DIAGNOSTIC: capture original hardware only into a new explicit directory"]
fn capture_original_board_states() {
    let dir = std::env::var_os("EMU198X_ZX_BOARD_CAPTURE").expect("output directory");
    let dir = Path::new(&dir);
    std::fs::create_dir(dir).expect("new directory");
    all(dir, true);
    all(dir, false);
}
