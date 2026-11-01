//! Original v1 wire bytes and native output captured before extraction.
use crate::test_cartridge as common;
use crate::{Model, SnesRuntime};
use emu198x_shell::{
    AudioCapture, HostIo, LatestFrameCapture, MachineCore, MachineTime, NullTraceSink, StopReason,
};
use std::path::{Path, PathBuf};
fn runtime(model: Model, interlace: bool) -> SnesRuntime {
    let mut rom = common::headed_cartridge(if model == Model::SuperFamicom {
        0
    } else if model == Model::SnesPal {
        2
    } else {
        1
    });
    rom[0x7fd6] = 2;
    rom[0x7fd8] = 1;
    let mut code = Vec::new();
    fn write(code: &mut Vec<u8>, address: u16, value: u8) {
        code.extend([0xa9, value, 0x8d, address as u8, (address >> 8) as u8]);
    }
    for (a, v) in [
        (0x2100, 15),
        (0x2121, 0),
        (0x2122, 0xe0),
        (0x2122, 3),
        (0x2133, u8::from(interlace)),
        (0x4200, 0x81),
    ] {
        write(&mut code, a, v);
    }
    let entry = code.len();
    code.extend([0xee, 0x10, 0, 0xad, 0x20, 0]);
    let rel = entry as i32 - (code.len() as i32 + 2);
    code.extend([0xf0, rel as i8 as u8, 0x9c, 0x20, 0]);
    for (a, v) in [
        (0x2115, 0x80),
        (0x2116, 0),
        (0x2117, 0),
        (0x4300, 1),
        (0x4301, 0x18),
        (0x4302, 0),
        (0x4303, 0x10),
        (0x4304, 0x7e),
        (0x4305, 0),
        (0x4306, 8),
        (0x21, 1),
        (0x420b, 1),
    ] {
        write(&mut code, a, v);
    }
    let target = 0x8000 + entry as u16;
    code.extend([0x4c, target as u8, (target >> 8) as u8]);
    assert!(code.len() < 256);
    rom[..code.len()].copy_from_slice(&code);
    SnesRuntime::new(model, rom)
}
fn field(r: &mut SnesRuntime) -> (emu198x_shell::CapturedFrame, Vec<u32>) {
    let mut p = LatestFrameCapture::default();
    let mut a = AudioCapture::default();
    let out = r
        .run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut p,
            audio_sink: &mut a,
            trace_sink: &mut NullTraceSink,
        })
        .expect("field");
    assert_eq!(out.stop_reason, StopReason::ReachedTarget);
    assert_eq!(p.frames_seen(), 1);
    (
        p.frame().expect("picture").clone(),
        a.audio()
            .expect("audio")
            .samples
            .iter()
            .map(|v| v.to_bits())
            .collect(),
    )
}
fn specimen(model: Model, interlace: bool) -> SnesRuntime {
    let mut r = runtime(model, interlace);
    let m = r.machine_mut().expect("machine");

    for (a, v) in [
        (0x0c, 127),
        (0x1c, 127),
        (0, 127),
        (1, 127),
        (5, 0),
        (7, 127),
        (0x6c, 0x1f),
        (0x3d, 1),
        (0x5c, 0),
        (0x4c, 1),
    ] {
        m.apu_mut().dsp.write(a, v);
    }
    for _ in 0..3 {
        field(&mut r);
    }
    let m = r.machine_mut().expect("machine");
    m.poke(0x700000, 0xa5);
    let mut buttons = m.pads[0].buttons;
    buttons.b = true;
    buttons.r = true;
    m.set_buttons(0, buttons);
    m.pads_mut()[0].latch(true);
    m.pads_mut()[0].latch(false);
    m.pads_mut()[0].data();
    for offset in 0..2048 {
        m.poke(0x7e1000 + offset, (offset as u8).wrapping_mul(17));
    }
    m.poke(0x20, 1);
    for _ in 0..20000 {
        if m.peek(0x21) == 1 {
            break;
        }
        m.tick();
    }
    assert_eq!(m.peek(0x21), 1, "guest armed DMA");
    for _ in 0..137 {
        m.tick();
    }
    let now = m.mclk();
    r.set_time(MachineTime::new(now));
    r
}
// Encode the original envelope solely to compare immutable pre-split bytes.
// Production writes v2; this is not a second shipping encoder.
fn legacy_snapshot(r: &SnesRuntime) -> Vec<u8> {
    postcard::to_allocvec(&(
        1_u16,
        r.time().get(),
        r.model_id(),
        r.machine().map(|m| m.state()),
    ))
    .expect("legacy comparison")
}
fn continuation(r: &mut SnesRuntime) -> Vec<u8> {
    let mut fields = Vec::new();
    for _ in 0..3 {
        let before = r.time().get();
        let (p, a) = field(r);
        assert!(
            p.pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|px| px.as_slice() != &p.pixels[..4]),
            "active picture"
        );
        assert!(
            a.iter().any(|v| f32::from_bits(*v).abs() > 0.00001),
            "active sound"
        );
        fields.push((r.time().get() - before, p, a));
    }
    postcard::to_allocvec(&(fields, legacy_snapshot(r))).expect("output")
}
#[test]
fn original_v1_states_and_three_native_fields_remain_exact() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/nintendo-snes/board-v1");
    for model in Model::ALL {
        for interlace in [false, true] {
            let name = format!(
                "{}-{}",
                model.model_id(),
                if interlace {
                    "interlaced"
                } else {
                    "progressive"
                }
            );
            let mut live = specimen(model, interlace);
            if !interlace && let Some(dir) = std::env::var_os("EMU198X_SNES_EXPORT_CARTS") {
                use std::io::Write;
                let dir = PathBuf::from(dir);
                std::fs::create_dir_all(&dir).expect("directory");
                let mut rom = live.machine().expect("loaded").cart.rom().to_vec();
                let sum = format_nintendo_snes_sfc::compute_checksum(&rom);
                rom[0x7fdc..0x7fde].copy_from_slice(&(!sum).to_le_bytes());
                rom[0x7fde..0x7fe0].copy_from_slice(&sum.to_le_bytes());
                std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(dir.join(format!("{}.sfc", model.model_id())))
                    .expect("new cart")
                    .write_all(&rom)
                    .expect("cart");
            }
            let saved = legacy_snapshot(&live);
            let output = continuation(&mut live);
            if let Some(dir) = std::env::var_os("EMU198X_SNES_CAPTURE") {
                use std::io::Write;
                let dir = PathBuf::from(dir);
                std::fs::create_dir_all(&dir).expect("directory");
                for (suffix, bytes) in [("state", &saved), ("output", &output)] {
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
                std::fs::read(root.join(format!("{name}.output"))).expect("original output");
            assert!(saved == original, "{name}: state bytes");
            assert!(output == expected, "{name}: direct output");
            let mut restored = SnesRuntime::blank(model);
            restored.restore(&original).expect("restore");
            assert!(legacy_snapshot(&restored) == original, "reencoded");
            assert!(
                continuation(&mut restored) == expected,
                "{name}: first pixels/audio/clocks/final state"
            );
        }
    }
}
