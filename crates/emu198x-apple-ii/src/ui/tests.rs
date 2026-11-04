use super::*;
use emu198x_shell::{
    AudioPacket, AudioSink, FirmwareImage, FirmwareSet, HostIo, InputEvent, LatestFrameCapture,
    MachineError, NullAudioSink, NullFrameSink, NullTraceSink, ResetKind,
};

fn runtime() -> AppleIIRuntime {
    // Authored loop toggles the speaker; it never consumes the keyboard latch.
    let mut rom = [0xea; 8192];
    rom[..11].copy_from_slice(&[0xa9, 42, 0x8d, 0, 4, 0xad, 0x30, 0xc0, 0x4c, 5, 0xe0]);
    rom[0x1ffc..0x1ffe].copy_from_slice(&[0, 0xe0]);
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(profiles::FIRMWARE[0], &rom));
    firmware.push(FirmwareImage::new(profiles::FIRMWARE[1], &[0; 512]));
    AppleIIRuntime::from_firmware(&firmware).expect("authored firmware")
}

fn deliver(runtime: &mut AppleIIRuntime, names: &[String]) {
    let events: Vec<_> = names
        .iter()
        .map(|name| InputEvent::Key {
            name: name.clone().into(),
            pressed: true,
        })
        .collect();
    runtime
        .run_ticks(
            1000,
            &mut HostIo {
                input_events: &events,
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("UI key delivery");
}

#[test]
fn host_layout_characters_reach_the_encoded_keyboard_latch() {
    for (ch, byte) in [
        ('a', b'A'),
        ('"', b'"'),
        (':', b':'),
        ('*', b'*'),
        ('?', b'?'),
    ] {
        let mut runtime = runtime();
        let names = AppleIISystem
            .host_character_keys(&runtime, ch)
            .expect("ASCII");
        assert_eq!(
            names,
            runtime
                .keyboard_target()
                .expect("keyboard")
                .keys_for_char(ch)
                .expect("shared map")
        );
        deliver(&mut runtime, &names);
        assert_eq!(runtime.machine().peek(0xc000), byte | 128);
    }
    assert!(AppleIISystem.host_character_keys(&runtime(), 'é').is_none());
}

#[test]
fn deliberate_control_chord_and_named_keys_use_supported_target_contacts() {
    let system = AppleIISystem;
    let modifier = system.host_keyword_modifier().expect("target Ctrl");
    assert_eq!(modifier.key, KeyCode::AltLeft);
    let mut names: Vec<String> = modifier.keys.iter().map(|s| (*s).into()).collect();
    names.extend(
        system
            .map_keys(KeyCode::KeyB)
            .expect("B")
            .iter()
            .map(|s| (*s).into()),
    );
    let mut target = runtime();
    deliver(&mut target, &names);
    assert_eq!(target.machine().peek(0xc000), 0x82);
    for (key, byte) in [
        (KeyCode::Enter, 13),
        (KeyCode::Escape, 27),
        (KeyCode::Backspace, 8),
        (KeyCode::ArrowLeft, 8),
        (KeyCode::ArrowRight, 21),
    ] {
        let mut target = runtime();
        let names = system.host_control_keys(key).expect("control");
        deliver(
            &mut target,
            &names.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
        );
        assert_eq!(target.machine().peek(0xc000), byte | 128);
    }
    assert!(system.map_keys(KeyCode::ShiftLeft).is_none());
    assert!(system.map_keys(KeyCode::ArrowUp).is_none());
}

#[derive(Default)]
struct Audio(Vec<f32>);
impl AudioSink for Audio {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        assert_eq!((packet.sample_rate, packet.channels), (48000, 1));
        self.0.extend_from_slice(packet.samples);
        Ok(())
    }
}

#[test]
fn native_budget_delivers_colour_and_speaker_audio_and_reset_preserves_ram() {
    let mut runtime = runtime();
    let mut system = AppleIISystem;
    let mut video = LatestFrameCapture::default();
    let mut audio = Audio::default();
    runtime
        .run_ticks(
            system.frame_ticks(&runtime),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut video,
                audio_sink: &mut audio,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("native frame");
    let rate = runtime.profile().clock.rate;
    assert_eq!(
        system.frame_duration(&runtime),
        Duration::from_secs_f64(runtime.time().get() as f64 / rate.numerator_hz as f64)
    );
    let frame = video.frame().expect("colour frame");
    assert_eq!(
        (frame.width, frame.height),
        system.framebuffer_size(&runtime)
    );
    assert_eq!(frame.pixels.len(), 560 * 192 * 4);
    assert!(!audio.0.is_empty());
    assert!(
        audio.0.windows(2).any(|s| s[0] != s[1]),
        "speaker must vary"
    );
    assert!(runtime.display().is_some());
    assert_eq!(system.display(&runtime), runtime.display());
    let before = runtime.time();
    let ram = runtime.machine().peek(0x400);
    assert_eq!(ram, 42);
    runtime.reset(ResetKind::Soft);
    system.after_reset(&mut runtime).expect("shared reset hook");
    assert_eq!(runtime.time(), before);
    assert_eq!(runtime.machine().peek(0x400), ram);
}
