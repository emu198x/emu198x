//! Original complete v4 saves and native continuation before board extraction.
use crate::{AmstradCpcRuntime, Model};
use emu198x_shell::{
    AudioCapture, HostIo, LatestFrameCapture, MachineCore, MachineTime, NullTraceSink, ResetKind,
    StopReason,
};
use emu198x_tape_playback::TapeSpan;
use std::path::{Path, PathBuf};

fn firmware() -> Vec<u8> {
    include_bytes!("../../../test-data/synthetic-firmware/amstrad-cpc.rom").to_vec()
}
fn frame(r: &mut AmstradCpcRuntime) -> (emu198x_shell::CapturedFrame, Vec<u32>) {
    let mut frames = LatestFrameCapture::default();
    let mut audio = AudioCapture::default();
    let result = r
        .run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut frames,
            audio_sink: &mut audio,
            trace_sink: &mut NullTraceSink,
        })
        .expect("native field");
    assert_eq!(result.stop_reason, StopReason::ReachedTarget);
    assert_eq!(frames.frames_seen(), 1);
    let samples = audio
        .audio()
        .expect("audio")
        .samples
        .iter()
        .map(|s| s.to_bits())
        .collect();
    (frames.frame().expect("frame").clone(), samples)
}
fn specimen(model: Model) -> AmstradCpcRuntime {
    let mut r = AmstradCpcRuntime::new(model, firmware()).expect("our firmware");
    if model.has_disc() {
        r.set_amsdos(vec![0x7a; 0x4000]).expect("our upper ROM");
    }
    for _ in 0..4 {
        frame(&mut r);
    }
    let m = r.machine_mut().expect("machine");
    m.out(0xf782, 0x82);
    for (reg, val) in [(0, 40), (1, 0), (7, 0x3e), (8, 15)] {
        m.out(0xf400, reg);
        m.out(0xf600, 0xd0);
        m.out(0xf600, 0x10);
        m.out(0xf400, val);
        m.out(0xf600, 0x90);
        m.out(0xf600, 0x10);
    }
    m.out(0x7f00, 0x81);
    m.out(0x7f00, 3);
    m.out(0x7f00, 0x40 | 26);
    m.out(0x7f00, 16);
    m.out(0x7f00, 0x40 | 4);
    for addr in 0xc000..=0xffff {
        m.poke(addr, 0xff);
    }
    assert!(m.press_char('a'));
    assert!(m.press_char('s'));
    m.insert_tape(vec![
        TapeSpan::Level {
            duration: 4_000_000,
            level: true,
        },
        TapeSpan::Pulse(1000),
    ]);
    m.out(0xf600, 0x10);
    if model.has_disc() {
        use emu198x_nec_upd765a::{DiskImage, DiskSector, DiskTrack};
        let image = DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![DiskTrack {
                sectors: vec![DiskSector {
                    c: 0,
                    h: 0,
                    id: 0xc1,
                    n: 2,
                    st1: 0,
                    st2: 0,
                    data: vec![0xa5; 512],
                }],
            }]],
        };
        m.insert_disk(0, image.clone(), true);
        m.insert_disk(1, image, false);
        m.out(0xfa7e, 1);
    }
    if model == Model::Cpc6128 {
        m.out(0x7f00, 0xc4);
        m.poke(0x4125, 0x5a);
    }
    m.advance_tstates(137);
    m.take_audio_buffer();
    let now = m.cpu_tstates();
    r.set_time(MachineTime::new(now));
    r
}
fn output(r: &mut AmstradCpcRuntime) -> Vec<u8> {
    let mut fields = Vec::new();
    for _ in 0..3 {
        let start = r.time().get();
        let (f, audio) = frame(r);
        assert!(
            audio.iter().any(|b| f32::from_bits(*b).abs() > 0.0001),
            "active PSG"
        );
        assert!(
            f.pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p.as_slice() != &f.pixels[..4]),
            "live raster must have different pens"
        );
        fields.push((r.time().get() - start, f, audio));
    }
    postcard::to_allocvec(&(fields, r.snapshot().expect("final native save"))).expect("output")
}
#[test]
fn original_v4_boards_keep_exact_saves_and_first_three_resumed_fields() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/amstrad-cpc/board-split-v4");
    for model in [Model::Cpc464, Model::Cpc664, Model::Cpc6128] {
        let name = model.model_id();
        let mut live = specimen(model);
        let saved = live.snapshot().expect("save");
        let expected = output(&mut live);
        if let Some(dir) = std::env::var_os("EMU198X_CPC_CAPTURE") {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).expect("capture dir");
            for (suffix, bytes) in [("state", &saved), ("output", &expected)] {
                use std::io::Write;
                std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(dir.join(format!("{name}.{suffix}")))
                    .expect("new capture only")
                    .write_all(bytes)
                    .expect("capture");
            }
            continue;
        }
        let original = std::fs::read(root.join(format!("{name}.state"))).expect("original save");
        assert!(saved == original, "{name}: full original v4 state");
        let reference =
            std::fs::read(root.join(format!("{name}.output"))).expect("original output");
        assert!(expected == reference, "{name}: direct output");
        let mut restored = AmstradCpcRuntime::blank(model);
        restored.restore(&original).expect("restore original");
        assert!(
            restored.snapshot().expect("reencoded") == original,
            "{name}: reencoded save"
        );
        assert!(
            output(&mut restored) == reference,
            "{name}: first-frame pixels, samples, clocks and final state"
        );
    }
}
#[test]
fn a_restored_board_keeps_firmware_and_media_when_reset_without_host_caches() {
    for model in [Model::Cpc464, Model::Cpc664, Model::Cpc6128] {
        let mut original = specimen(model);
        original
            .machine_mut()
            .expect("machine")
            .fit_upper_rom(3, &vec![0x53; 0x4000])
            .expect("expansion");
        let saved = original.snapshot().expect("save");
        let mut restored = AmstradCpcRuntime::blank(model);
        restored.restore(&saved).expect("restore");
        restored.reset(ResetKind::Hard);
        let m = restored
            .machine()
            .expect("restored firmware must survive reset");
        assert_eq!(m.firmware(), firmware());
        assert_eq!(
            m.upper_roms().get(&3).expect("expansion ROM"),
            &vec![0x53; 0x4000]
        );
        assert_eq!(
            m.tape().spans(),
            original.machine().expect("original").tape().spans()
        );
        if model.has_disc() {
            assert_eq!(m.upper_roms().get(&7).expect("AMSDOS"), &vec![0x7a; 0x4000]);
        }
        if model.has_disc() {
            assert_eq!(
                m.disk(0)
                    .expect("modified disc")
                    .sector(0, 0, 0xc1)
                    .expect("sector")
                    .data,
                vec![0xa5; 512]
            );
            assert!(m.fdc().is_write_protected(1));
        }
    }
}
