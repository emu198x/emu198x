//! Independent original v20 engine output, not regenerated expectations.
use emu198x_shell::{HostIo, InputEvent, MachineCore, MachineError, NullTraceSink};
use runtime_atari_5200::{Atari5200Runtime, Model};
#[derive(Default)]
struct Frames(Vec<u8>);
impl emu198x_shell::FrameSink for Frames {
    fn push_frame(
        &mut self,
        frame: emu198x_shell::FramePacket<'_>,
    ) -> Result<(), emu198x_shell::MachineError> {
        self.0.extend(frame.timestamp.get().to_le_bytes());
        self.0.extend(frame.width.to_le_bytes());
        self.0.extend(frame.height.to_le_bytes());
        self.0.extend(frame.pixels);
        Ok(())
    }
}
#[derive(Default)]
struct Audio(Vec<u8>);
impl emu198x_shell::AudioSink for Audio {
    fn push_audio(
        &mut self,
        packet: emu198x_shell::AudioPacket<'_>,
    ) -> Result<(), emu198x_shell::MachineError> {
        self.0.extend(packet.timestamp.get().to_le_bytes());
        self.0.extend(packet.sample_rate.to_le_bytes());
        self.0.push(packet.channels);
        for sample in packet.samples {
            self.0.extend(sample.to_bits().to_le_bytes());
        }
        Ok(())
    }
}

fn relabel(bytes: &[u8], model: Model) -> Vec<u8> {
    let (version, rest) = postcard::take_from_bytes::<u16>(bytes).expect("version");
    let (time, rest) = postcard::take_from_bytes::<u64>(rest).expect("time");
    let (old, rest) = postcard::take_from_bytes::<String>(rest).expect("model");
    assert_eq!(old, "atari-5200-ntsc");
    let mut result =
        postcard::to_allocvec(&(version, time, model.model_id())).expect("test envelope");
    result.extend(rest);
    result
}
fn fixture(selector: usize, kind: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/atari/5200-board-owners")
            .join(format!("selector-{selector}-{kind}")),
    )
    .expect("frozen original fixture")
}
#[test]
fn explicit_four_port_owners_retain_original_v20_continuations() {
    for model in [Model::Ca018087, Model::Ca020108] {
        for selector in 0..4 {
            let before = relabel(&fixture(selector, "before.state"), model);
            let after = relabel(&fixture(selector, "after.state"), model);
            let mut m = Atari5200Runtime::blank(model);
            m.restore(&before).expect("explicit board envelope");
            assert_eq!(m.snapshot().expect("roundtrip"), before);
            let (mut frames, mut audio) = (Frames::default(), Audio::default());
            let later = [
                InputEvent::Button {
                    port: 4,
                    name: "fire2".into(),
                    pressed: false,
                },
                InputEvent::Button {
                    port: 2,
                    name: "1".into(),
                    pressed: false,
                },
                InputEvent::Axis {
                    port: 1,
                    name: "x".into(),
                    value: 12345,
                },
            ];
            m.run_until(
                m.time().saturating_add(262 * 228 * 2 + 37),
                &mut HostIo {
                    input_events: &later,
                    frame_sink: &mut frames,
                    audio_sink: &mut audio,
                    trace_sink: &mut NullTraceSink,
                },
            )
            .expect("continuation");
            assert_eq!(
                frames.0,
                fixture(selector, "frames.bin"),
                "frame packets for {model:?}/{selector}"
            );
            assert_eq!(
                audio.0,
                fixture(selector, "audio.bin"),
                "float audio for {model:?}/{selector}"
            );
            assert_eq!(
                m.snapshot().expect("final"),
                after,
                "complete final state for {model:?}/{selector}"
            );
        }
    }
}
#[test]
fn retired_generic_snapshots_and_foreign_boards_reject_atomically() {
    assert!(Model::from_variant_id("atari-5200-ntsc").is_none());
    for model in Model::ALL {
        let mut m = Atari5200Runtime::blank(model);
        let before = m.snapshot().expect("before");
        let error = m
            .restore(&fixture(0, "before.state"))
            .expect_err("retired generic profile");
        assert!(
            matches!(error,MachineError::InvalidSnapshot{reason} if reason.contains("atari-5200-ntsc"))
        );
        assert_eq!(m.snapshot().expect("unchanged"), before);
        for other in Model::ALL {
            if other == model {
                continue;
            }
            let foreign = Atari5200Runtime::blank(other)
                .snapshot()
                .expect("foreign board");
            assert!(m.restore(&foreign).is_err());
            assert_eq!(m.snapshot().expect("unchanged"), before);
        }
    }
}

#[test]
fn each_profile_rebuilds_and_restores_its_advertised_fixed_owner() {
    for model in Model::ALL {
        let mut runtime = Atari5200Runtime::new(model, vec![0xea; 32768], vec![0x55; 2048])
            .expect("physical board");
        assert_eq!(
            runtime.machine().expect("loaded").board_id(),
            model.model_id()
        );
        let saved = runtime.snapshot().expect("save");
        runtime.reset(emu198x_shell::ResetKind::Hard);
        assert_eq!(
            runtime.machine().expect("reset").board_id(),
            model.model_id()
        );
        runtime.set_bios(vec![0x66; 2048]).expect("firmware");
        assert_eq!(
            runtime.machine().expect("firmware").board_id(),
            model.model_id()
        );
        runtime.restore(&saved).expect("matching explicit board");
        assert_eq!(
            runtime.machine().expect("restored").board_id(),
            model.model_id()
        );
        assert_eq!(runtime.snapshot().expect("exact save"), saved);
    }
}
#[test]
fn forged_two_port_envelope_cannot_restore_four_held_controllers() {
    let mut runtime = Atari5200Runtime::blank(Model::Ca021374);
    let before = runtime.snapshot().expect("before");
    let forged = relabel(&fixture(0, "before.state"), Model::Ca021374);
    assert!(runtime.restore(&forged).is_err());
    assert_eq!(runtime.snapshot().expect("unchanged"), before);
}
