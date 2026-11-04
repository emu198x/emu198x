use super::*;
use emu198x_shell::{
    AudioPacket, AudioSink, FirmwareImage, FirmwareSet, HostIo, InputEvent, LatestFrameCapture,
    MachineError, NullAudioSink, NullFrameSink, NullTraceSink, ResetKind,
};

fn runtime() -> AppleIIRuntime {
    // Authored loop toggles the speaker; it never consumes the keyboard latch.
    runtime_with_program(&[0xa9, 42, 0x8d, 0, 4, 0xad, 0x30, 0xc0, 0x4c, 5, 0xe0])
}

fn runtime_with_program(program: &[u8]) -> AppleIIRuntime {
    let mut rom = [0xea; 8192];
    rom[..program.len()].copy_from_slice(program);
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
    deliver_events(runtime, &events);
}

fn deliver_events(runtime: &mut AppleIIRuntime, events: &[InputEvent]) {
    runtime
        .run_ticks(
            1000,
            &mut HostIo {
                input_events: events,
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("UI key delivery");
}

#[test]
fn releasing_target_ctrl_restores_plain_letters_after_rom_acknowledgement() {
    // Read strobed keys into consecutive RAM bytes, acknowledging each through
    // C010. The second key must travel through the real ROM-driven latch path.
    let mut target = runtime_with_program(&[
        0xa2, 0, 0xad, 0, 0xc0, 0x10, 0xfb, 0x9d, 0, 2, 0xe8, 0x2c, 0x10, 0xc0, 0x4c, 2, 0xe0,
    ]);
    deliver(&mut target, &["ctrl".into(), "b".into()]);
    deliver_events(
        &mut target,
        &[
            InputEvent::Key {
                name: "b".into(),
                pressed: false,
            },
            InputEvent::Key {
                name: "ctrl".into(),
                pressed: false,
            },
        ],
    );
    let plain_b = AppleIISystem.host_character_keys(&target, 'b').expect("B");
    deliver(&mut target, &plain_b);
    assert_eq!(target.machine().peek(0x200), 0x82, "Ctrl+B");
    assert_eq!(target.machine().peek(0x201), 0xc2, "plain uppercase B");
    assert!(
        !target.machine().key_pending(),
        "ROM acknowledged both bytes"
    );
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
fn speaker_delay_loop_produces_a_one_khz_waveform_at_the_native_sample_rate() {
    // LDA C030; LDY #100; DEY/BNE; JMP E000. The delay makes the tone audible,
    // unlike the earlier fast-toggle smoke test. C020 is the silent control.
    for (address, expected_tone) in [(0x30, true), (0x20, false)] {
        let mut target = runtime_with_program(&[
            0xad, address, 0xc0, 0xa0, 100, 0x88, 0xd0, 0xfd, 0x4c, 0, 0xe0,
        ]);
        let mut audio = Audio::default();
        target
            .run_ticks(
                profiles::FRAME_TICKS * 20,
                &mut HostIo {
                    input_events: &[],
                    frame_sink: &mut NullFrameSink,
                    audio_sink: &mut audio,
                    trace_sink: &mut NullTraceSink,
                },
            )
            .expect("twenty native frames");
        let clock = target.profile().clock.rate;
        assert_eq!(
            audio.0.len() as u64,
            target.time().get() * 48_000 / clock.numerator_hz
        );
        assert!(
            audio
                .0
                .iter()
                .all(|s| s.is_finite() && (-0.25..=0.25).contains(s))
        );
        let crossings = audio
            .0
            .windows(2)
            .filter(|s| s[0] <= 0.0 && s[1] > 0.0)
            .count();
        let frequency = crossings as f64 * 48_000.0 / audio.0.len() as f64;
        if expected_tone {
            assert!(
                (990.0..1020.0).contains(&frequency),
                "tone is {frequency} Hz"
            );
            assert!(audio.0.iter().any(|s| *s > 0.2));
            assert!(audio.0.iter().any(|s| *s < -0.2));
        } else {
            assert_eq!(crossings, 0, "unrelated I/O must not sound the speaker");
        }
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
