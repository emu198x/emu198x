//! Frozen complete original v2 saves and continued field output.
use crate::{MegaDriveRuntime, Model};
use emu198x_shell::{AudioCapture, HostIo, MachineCore, MachineTime, NullFrameSink, NullTraceSink};
use std::path::{Path, PathBuf};

fn native_cart() -> Vec<u8> {
    let mut rom = vec![0; 0x1000];
    rom[..4].copy_from_slice(&0x00fffe00u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x200u32.to_be_bytes());
    rom[0x78..0x7c].copy_from_slice(&0x400u32.to_be_bytes());
    let mut p = Vec::<u16>::new();
    fn long(p: &mut Vec<u16>, v: u32) {
        p.extend([(v >> 16) as u16, v as u16]);
    }
    for (value, addr) in [
        (0x8164u16, 0xc00004),
        (0x8c81, 0xc00004),
        (0x8701, 0xc00004),
    ] {
        p.extend([0x33fc, value]);
        long(&mut p, addr);
    }
    p.push(0x23fc);
    long(&mut p, 0xc0020000);
    long(&mut p, 0xc00004);
    p.extend([0x33fc, 0x000e]);
    long(&mut p, 0xc00000);
    for b in [0x80, 0x01, 0x90] {
        p.extend([0x13fc, b]);
        long(&mut p, 0xc00011);
    }
    p.extend([0x46fc, 0x2000]);
    let loop_pc = 0x200 + 2 * p.len() as u32;
    p.push(0x13f9);
    long(&mut p, 0xa10001);
    long(&mut p, 0xff0010);
    p.push(0x13f9);
    long(&mut p, 0xa10003);
    long(&mut p, 0xff0011);
    p.push(0x5279);
    long(&mut p, 0xff0020);
    p.push(0x4ef9);
    long(&mut p, loop_pc);
    for (i, w) in p.iter().enumerate() {
        rom[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&w.to_be_bytes());
    }
    let handler = [0x5279u16, 0x00ff, 0x0022, 0x4e73];
    for (i, w) in handler.iter().enumerate() {
        rom[0x400 + i * 2..0x402 + i * 2].copy_from_slice(&w.to_be_bytes());
    }
    rom
}
fn sms_cart() -> Vec<u8> {
    let mut rom = vec![0xff; 0x10000];
    // Mode 4, PSG tone, select bank 3 in slot 1, keep a marker and count VBlank.
    let code = [
        0xf3, 0x3e, 4, 0xd3, 0xbf, 0x3e, 0x80, 0xd3, 0xbf, 0x3e, 0x40, 0xd3, 0xbf, 0x3e, 0x81,
        0xd3, 0xbf, 0x3e, 0x80, 0xd3, 0x7f, 0x3e, 1, 0xd3, 0x7f, 0x3e, 0x90, 0xd3, 0x7f, 0x3e, 3,
        0x32, 0xfe, 0xff, 0x3a, 0, 0x40, 0x32, 1, 0xc0, 0xdb, 0xbf, 0x07, 0x30, 0xfb, 0x21, 0,
        0xc0, 0x34, 0x18, 0xf5,
    ];
    rom[..code.len()].copy_from_slice(&code);
    rom[0x7ff0..0x7ff8].copy_from_slice(b"TMR SEGA");
    rom[0xc000] = 0x53;
    rom
}
pub(super) fn specimen(model: Model, pbc: bool) -> MegaDriveRuntime {
    let mut r = MegaDriveRuntime::blank(model);
    if pbc {
        r.insert_master_system_cartridge(sms_cart());
    } else {
        r.insert_mega_drive_cartridge(native_cart());
    }
    for _ in 0..3 {
        frame(&mut r);
    }
    let m = r.machine_mut().expect("machine");
    m.ports[0].device = crate::Device::SixButton;
    m.ports[0].write_control(0x40);
    for value in [0x40, 0, 0x40, 0] {
        m.ports[0].write_data(value);
    }
    m.set_buttons(
        0,
        crate::Buttons {
            up: true,
            a: true,
            ..crate::Buttons::default()
        },
    );
    m.poke(0xff0100, 0x5a);
    for _ in 0..137 {
        m.tick();
    }
    m.take_audio();
    r.time = MachineTime::new(m.mclk());
    r
}
fn frame(r: &mut MegaDriveRuntime) -> Vec<u32> {
    let mut audio = AudioCapture::default();
    r.run_frame(&mut HostIo {
        input_events: &[],
        frame_sink: &mut NullFrameSink,
        audio_sink: &mut audio,
        trace_sink: &mut NullTraceSink,
    })
    .expect("native field");
    audio
        .audio()
        .expect("audio packet")
        .samples
        .iter()
        .map(|s| s.to_bits())
        .collect()
}
fn output(r: &mut MegaDriveRuntime) -> Vec<u8> {
    let mut fields = Vec::new();
    for _ in 0..3 {
        let start = r.time().get();
        let samples = frame(r);
        let m = r.machine().expect("machine");
        fields.push((
            r.time().get() - start,
            r.time().get(),
            m.frame_width(),
            m.frame_height(),
            m.framebuffer().to_vec(),
            samples,
        ));
    }
    postcard::to_allocvec(&(fields, r.snapshot().expect("final state"))).expect("output")
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/sega-mega-drive/board-split-v2")
}
#[test]
fn original_market_boards_keep_v2_bytes_pixels_clocks_and_stereo_samples() {
    if let Some(dir) = std::env::var_os("EMU198X_MEGA_DRIVE_CAPTURE") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("directory");
        for (name, rom) in [
            ("native.bin", native_cart()),
            ("power-base.sms", sms_cart()),
        ] {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(dir.join(name))
                .expect("new cartridge capture")
                .write_all(&rom)
                .expect("cartridge");
        }
    }

    for model in Model::ALL {
        for pbc in [false, true] {
            let name = format!(
                "{}-{}",
                model.variant_id(),
                if pbc { "pbc" } else { "native" }
            );
            let mut r = specimen(model, pbc);
            let state = r.snapshot().expect("snapshot");
            let out = output(&mut r);
            if let Some(dir) = std::env::var_os("EMU198X_MEGA_DRIVE_CAPTURE") {
                let dir = PathBuf::from(dir);
                std::fs::create_dir_all(&dir).expect("directory");
                for (suffix, bytes) in [("state", &state), ("output", &out)] {
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
            let saved =
                std::fs::read(root().join(format!("{name}.state"))).expect("original state");
            assert!(state == saved, "{name}: full original v2 envelope");
            let expected =
                std::fs::read(root().join(format!("{name}.output"))).expect("original output");
            assert!(
                out == expected,
                "{name}: pixels, clocks, stereo samples and final state"
            );
            let mut restored = MegaDriveRuntime::blank(model);
            restored.restore(&saved).expect("original save");
            assert!(
                restored.snapshot().expect("reencoded") == saved,
                "{name}: reencoded save"
            );
            assert!(
                output(&mut restored) == expected,
                "{name}: restored continued output"
            );
        }
    }
}
