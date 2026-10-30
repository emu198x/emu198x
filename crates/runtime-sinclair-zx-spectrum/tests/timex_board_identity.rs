//! Fixed owners must agree with profiles and reject conflicting saves atomically.
use emu198x_shell::{
    AudioCapture, ControlCommand, FamilyRuntime, FirmwareImage, FirmwareSet, HostIo, InputEvent,
    LatestFrameCapture, MachineCore, MachineError, MediaImage, MediaKind, MediaSet,
    MediaTransportAction, MediaTransportCommand, NullTraceSink, ResetKind,
};
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
    let exrom = [0; 8192];
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("timex-ts2068-rom-0", &rom));
    firmware.push(FirmwareImage::new("timex-ts2068-rom-1", &exrom));
    SpectrumRuntimeKind::from_firmware(model, &firmware).expect("fixed board")
}
fn frame(r: &mut SpectrumRuntimeKind, events: &[InputEvent]) -> Vec<u8> {
    let mut f = LatestFrameCapture::default();
    let mut a = AudioCapture::default();
    r.run_frame(&mut HostIo {
        input_events: events,
        frame_sink: &mut f,
        audio_sink: &mut a,
        trace_sink: &mut NullTraceSink,
    })
    .expect("native field");
    assert_eq!(f.frames_seen(), 1);
    let f = f.frame().expect("frame");
    let samples: Vec<_> = a
        .audio()
        .expect("audio")
        .samples
        .iter()
        .map(|s| s.to_bits())
        .collect();
    postcard::to_allocvec(&(
        f.timestamp.get(),
        f.width,
        f.height,
        &f.pixels,
        &f.palette,
        samples,
    ))
    .expect("complete output")
}
#[test]
fn profiles_match_the_fixed_hardware_and_its_clock() {
    on_stack(|| {
        for model in [Model::TimexTC2068, Model::TimexTS2068] {
            let r = runtime(model);
            let (id, hz) = match &r {
                SpectrumRuntimeKind::TimexTC2068(r) => {
                    (r.machine().model_id(), r.machine().timing().master_hz)
                }
                SpectrumRuntimeKind::TimexTS2068(r) => {
                    (r.machine().model_id(), r.machine().timing().master_hz)
                }
                _ => panic!("wrong family variant"),
            };
            assert_eq!(r.profile().machine_id.as_str(), id);
            assert_eq!(id, model.model_id());
            assert_eq!(r.profile().profile_id.as_str(), model.profile_id());
            assert_eq!(r.profile().clock.rate.numerator_hz, hz);
        }
    });
}
#[test]
fn other_board_payload_is_rejected_before_time_or_hardware_changes() {
    on_stack(|| {
        for (target, source) in [
            (Model::TimexTC2068, Model::TimexTS2068),
            (Model::TimexTS2068, Model::TimexTC2068),
        ] {
            for mode in [0, 6] {
                let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                    "../../test-data/timex-2068/board-split-v17/{}-mode{mode}.state",
                    source.model_id()
                ));
                let bytes = std::fs::read(path).expect("original complete v17 state");
                let (version, tail) = postcard::take_from_bytes::<u32>(&bytes).expect("version");
                assert_eq!(version, 17);
                let (profile, payload) =
                    postcard::take_from_bytes::<String>(tail).expect("profile");
                assert_eq!(profile, source.profile_id());
                // A matching envelope must not hide the other board in its payload.
                let mut forged =
                    postcard::to_allocvec(&(version, target.profile_id())).expect("header");
                forged.extend_from_slice(payload);
                let mut r = runtime(target);
                frame(&mut r, &[]);
                r.write_byte(0x9000, 0x5a);
                let before = r.snapshot().expect("before");
                assert!(
                    matches!(
                        r.restore(&forged),
                        Err(MachineError::InvalidSnapshot { .. })
                    ),
                    "{source:?} payload under {target:?} header"
                );
                assert!(r.snapshot().expect("after") == before, "atomic rejection");
            }
        }
    });
}
#[test]
fn active_tape_held_keys_and_firmware_survive_restore_and_reset() {
    on_stack(|| {
        for model in [Model::TimexTC2068, Model::TimexTS2068] {
            let mut r = runtime(model);
            let mut tap = vec![19, 0];
            tap.extend([0; 19]);
            let mut media = MediaSet::new();
            media.push(MediaImage::new("tape-1", MediaKind::Tape, &tap));
            r.load_media(&media).expect("tape");
            r.command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Start,
            )))
            .expect("play");
            frame(
                &mut r,
                &[
                    InputEvent::Key {
                        name: "a".into(),
                        pressed: true,
                    },
                    InputEvent::Key {
                        name: "s".into(),
                        pressed: true,
                    },
                ],
            );
            assert_eq!(r.profile_cycles(137).expect("partial field").ticks, 137);
            assert!(r.tape_is_playing());
            assert_eq!(r.port_read(0xfdfe) & 0x1f, 0x1c);
            let saved = r.snapshot().expect("active save");
            let mut restored = runtime(model);
            restored.restore(&saved).expect("restore");
            assert!(restored.snapshot().expect("reencoded") == saved);
            assert!(restored.tape_is_playing());
            let release = [InputEvent::Key {
                name: "a".into(),
                pressed: false,
            }];
            assert!(frame(&mut r, &release) == frame(&mut restored, &release));
            assert_eq!(
                restored.port_read(0xfdfe) & 0x1f,
                0x1d,
                "S remains held after releasing restored A"
            );
            assert_eq!(r.port_read(0xfdfe) & 0x1f, 0x1d);
            assert!(frame(&mut r, &[]) == frame(&mut restored, &[]));
            assert!(r.snapshot().expect("uninterrupted") == restored.snapshot().expect("resumed"));
            for r in [&mut r, &mut restored] {
                r.reset(ResetKind::Hard);
                assert_eq!(r.read_byte(0), 0xf3);
                assert!(r.tape_is_loaded());
                assert_eq!(r.profile().machine_id.as_str(), model.model_id());
            }
            assert!(
                r.snapshot().expect("reset original")
                    == restored.snapshot().expect("reset restored")
            );
        }
    });
}
#[test]
fn malformed_geometry_and_clock_phase_are_rejected_atomically() {
    use common_timex_2068::{SpectrumDriver, Timex2068, TimexBoard};
    use runtime_sinclair_zx_spectrum::SpectrumRuntime;
    fn check<B: TimexBoard>() {
        for case in 0..3 {
            let mut bad = SpectrumRuntime::<Timex2068<B>>::blank();
            match case {
                0 => bad.machine_mut().framebuffer.clear(),
                1 => bad.machine_mut().framebuffer.push(0),
                _ => {
                    let ticks = bad.machine().frame_timing().halfcycles_per_frame;
                    *bad.machine_mut().hc_mut() = ticks;
                }
            }
            let bytes = bad.snapshot().expect("malformed specimen");
            let mut target = SpectrumRuntime::<Timex2068<B>>::blank();
            let before = target.snapshot().expect("before");
            assert!(matches!(
                target.restore(&bytes),
                Err(MachineError::InvalidSnapshot { .. })
            ));
            assert!(target.snapshot().expect("after") == before);
        }
    }
    on_stack(|| {
        check::<machine_timex_tc2068::Board>();
        check::<machine_timex_ts2068::Board>();
    });
}
