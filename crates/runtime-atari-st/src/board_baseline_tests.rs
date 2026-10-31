//! Complete original v1 states and first native continuations before extraction.
use crate::{AtariStRuntime, Model, Monitor};
use emu198x_shell::{
    AudioCapture, CapturedFrame, HostIo, LatestFrameCapture, MachineCore, MachineTime,
    NullTraceSink, ResetKind, StopReason,
};
use std::path::{Path, PathBuf};
#[path = "../tests/common/mod.rs"]
mod common;
fn frame(r: &mut AtariStRuntime) -> (CapturedFrame, Vec<u32>) {
    let mut pictures = LatestFrameCapture::default();
    let mut sound = AudioCapture::default();
    let result = r
        .run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut pictures,
            audio_sink: &mut sound,
            trace_sink: &mut NullTraceSink,
        })
        .expect("native frame");
    assert_eq!(result.stop_reason, StopReason::ReachedTarget);
    assert_eq!(pictures.frames_seen(), 1);
    (
        pictures.frame().expect("frame").clone(),
        sound
            .audio()
            .expect("sound")
            .samples
            .iter()
            .map(|x| x.to_bits())
            .collect(),
    )
}
fn specimen(model: Model, monitor: Monitor) -> AtariStRuntime {
    let mut r = AtariStRuntime::new(model, common::tos()).expect("project TOS");
    r.set_monitor(monitor);
    let m = r.machine_mut().expect("machine");
    m.glue.set_config(m.glue.matching_config());
    m.glue.write(
        0x20a,
        if model.standard() == machine_atari_st::VideoStandard::Pal {
            2
        } else {
            0
        },
    );
    m.glue.write(0x201, 2);
    m.glue.write(0x203, 0);
    m.shifter
        .write_resolution(if monitor == Monitor::Mono { 2 } else { 0 });
    m.glue
        .set_resolution(if monitor == Monitor::Mono { 2 } else { 0 });
    for i in 0..16 {
        m.shifter.write_palette(i, if i == 0 { 0 } else { 0x777 });
    }
    for addr in 0x20000..0x28000 {
        m.poke(addr, if addr & 1 == 0 { 0x5a } else { 0xa5 });
    }
    for _ in 0..3 {
        frame(&mut r);
    }
    let m = r.machine_mut().expect("machine");
    for (reg, val) in [(0, 40), (1, 0), (7, 0x3e), (8, 15)] {
        m.psg.select_register(reg);
        m.psg.write_data(val);
    }
    m.key(0x1e, true);
    m.mouse_move(13, -7);
    m.mouse_buttons(true, false);
    m.joystick(
        1,
        machine_atari_st::Stick(machine_atari_st::Stick::RIGHT | machine_atari_st::Stick::FIRE),
    );
    let disk = machine_atari_st::disk_from_st(&vec![0xa5; 80 * 9 * 512]).expect("disk");
    m.insert_disk(0, disk.clone());
    m.insert_disk(1, disk.write_protected(true));
    for _ in 0..137 {
        m.tick();
    }
    m.take_audio();
    let now = m.cycles();
    r.set_time(MachineTime::new(now));
    r
}
fn continuation(r: &mut AtariStRuntime) -> Vec<u8> {
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
            "active video"
        );
        assert!(
            sound.iter().any(|x| f32::from_bits(*x).abs() > 0.0001),
            "active sound"
        );
        fields.push((r.time().get() - before, picture, sound));
    }
    postcard::to_allocvec(&(fields, r.snapshot().expect("final state"))).expect("continuation")
}
#[test]
fn original_v1_states_and_first_three_native_fields_remain_exact() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/atari-st/board-split-v1");
    for model in Model::ALL {
        for monitor in [Monitor::Colour, Monitor::Mono] {
            let name = format!(
                "{}-{}",
                model.model_id(),
                if monitor == Monitor::Mono {
                    "mono"
                } else {
                    "colour"
                }
            );
            let mut live = specimen(model, monitor);
            let saved = live.snapshot().expect("save");
            let output = continuation(&mut live);
            if let Some(dir) = std::env::var_os("EMU198X_ST_CAPTURE") {
                use std::io::Write;
                let dir = PathBuf::from(dir);
                std::fs::create_dir_all(&dir).expect("capture directory");
                for (suffix, bytes) in [("state", &saved), ("output", &output)] {
                    let path = dir.join(format!("{name}.{suffix}"));
                    if path.exists() {
                        assert!(
                            std::fs::read(&path).expect("existing capture") == *bytes,
                            "existing capture must be identical"
                        );
                        continue;
                    }
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
            let original =
                std::fs::read(root.join(format!("{name}.state"))).expect("original state");
            let expected =
                std::fs::read(root.join(format!("{name}.output"))).expect("original continuation");
            assert!(saved == original, "{name}: original v1 bytes");
            assert!(output == expected, "{name}: direct continuation");
            let mut restored = AtariStRuntime::new(model, common::tos()).expect("restoring TOS");
            restored.restore(&original).expect("restore");
            assert!(
                restored.snapshot().expect("reencoded") == original,
                "{name}: reencoded state"
            );
            assert!(
                continuation(&mut restored) == expected,
                "{name}: first-frame pixels, sample bits, clocks and final state"
            );
        }
    }
}
#[test]
fn restored_disks_and_monitor_survive_reset_and_monitor_replacement() {
    for model in Model::ALL {
        let source = specimen(model, Monitor::Mono);
        let saved = source.snapshot().expect("save");
        let mut r = AtariStRuntime::new(model, common::tos()).expect("TOS");
        r.restore(&saved).expect("restore");
        assert_eq!(r.monitor(), Monitor::Mono);
        r.reset(ResetKind::Hard);
        for monitor in [Monitor::Mono, Monitor::Colour] {
            r.set_monitor(monitor);
            let m = r.machine().expect("machine");
            assert_eq!(m.monitor(), monitor);
            assert_eq!(m.rom(), common::tos());
            assert!(
                m.disk(0)
                    .expect("modified disk")
                    .data()
                    .iter()
                    .all(|x| *x == 0xa5)
            );
            assert!(m.disk(1).expect("protected disk").is_write_protected());
        }
    }
}
