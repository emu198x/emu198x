//! Original whole-runtime bytes and native output before fixed-owner extraction.
use crate::{C64FamilyMachine, C64FamilyRuntime, C64Runtime, C128Model, C128Runtime, Model};
use emu198x_shell::{
    AudioCapture, CapturedFrame, FamilyRuntime, FirmwareImage, FirmwareSet, HostIo,
    LatestFrameCapture, MachineCore, MachineTime, NullTraceSink, ResetKind, StopReason,
};
use std::path::Path;
fn program() -> Vec<u8> {
    let mut p = vec![0x78]; // SEI; configure real I/O through the guest CPU.
    for (addr, value) in [
        (0xd011u16, 0x1b),
        (0xd016, 0xc8),
        (0xd018, 0x14),
        (0xd020, 6),
        (0xd021, 0),
        (0xdd02, 3),
        (0xdd00, 3),
        (0xd400, 0x80),
        (0xd401, 0x20),
        (0xd405, 0),
        (0xd406, 0xf0),
        (0xd404, 0x21),
        (0xd418, 15),
    ] {
        p.extend([0xa9, value, 0x8d, addr as u8, (addr >> 8) as u8]);
    }
    p.extend([0xa2, 0, 0xa9, 1]);
    for hi in 0xd8..=0xdb {
        p.extend([0x9d, 0, hi]);
    }
    p.extend([0xe8, 0xd0, 0xf1]); // Four stores + INX + BNE = 15 bytes.
    let end = 0xe000 + p.len() as u16;
    p.extend([0x4c, end as u8, (end >> 8) as u8]);
    p
}
fn c64(model: Model) -> C64Runtime {
    let mut kernal = vec![0xea; 0x2000];
    let p = program();
    kernal[..p.len()].copy_from_slice(&p);
    kernal[0x1ffc..0x1ffe].copy_from_slice(&[0, 0xe0]);
    let mut r = C64Runtime::new(model, kernal, vec![0; 0x2000], vec![0xaa; 0x1000], None)
        .expect("synthetic ROMs");
    for a in 0x400..0x800 {
        r.machine_mut().poke(a, 0x41);
    }
    r.machine_mut().keyboard_mut().set_key(1, 2, true);
    r.machine_mut().set_joystick_control(2, "FIRE", true);
    r
}
fn c128(model: C128Model) -> C128Runtime {
    let mut kernal = vec![0xea; 0x4000];
    let p = program();
    kernal[0x2000..0x2000 + p.len()].copy_from_slice(&p);
    kernal[0x1000..0x1009].copy_from_slice(&[1, 5, 0xd5, 0x3e, 1, 0xed, 0x79, 0x18, 0xfe]);
    kernal[0x3ffc..0x3ffe].copy_from_slice(&[0, 0xe0]);
    let mut f = FirmwareSet::new();
    let lo = vec![0; 0x4000];
    let hi = lo.clone();
    let chars = vec![0xaa; 0x2000];
    for (id, data) in [
        ("commodore-c128-kernal-rom", kernal.as_slice()),
        ("commodore-c128-basic-lo-rom", &lo),
        ("commodore-c128-basic-hi-rom", &hi),
        ("commodore-c128-character-rom", &chars),
    ] {
        f.push(FirmwareImage::new(id, data));
    }
    let mut r = C128Runtime::from_firmware(model, &f).expect("synthetic ROMs");
    for a in 0x400..0x800 {
        r.machine_mut().poke(a, 0x41);
    }
    r.machine_mut().keyboard_mut().set_key(1, 2, true);
    r
}
fn field<M: C64FamilyMachine>(r: &mut C64FamilyRuntime<M>) -> (CapturedFrame, Vec<u32>)
where
    C64FamilyRuntime<M>: MachineCore,
{
    let mut pictures = LatestFrameCapture::default();
    let mut sound = AudioCapture::default();
    let result = r
        .run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut pictures,
            audio_sink: &mut sound,
            trace_sink: &mut NullTraceSink,
        })
        .expect("frame");
    assert_eq!(result.stop_reason, StopReason::ReachedTarget);
    assert_eq!(pictures.frames_seen(), 1);
    (
        pictures.frame().expect("picture").clone(),
        sound
            .audio()
            .expect("audio")
            .samples
            .iter()
            .map(|x| x.to_bits())
            .collect(),
    )
}
fn continuation<M: C64FamilyMachine>(r: &mut C64FamilyRuntime<M>) -> Vec<u8>
where
    C64FamilyRuntime<M>: MachineCore,
{
    let mut fields = Vec::new();
    for _ in 0..3 {
        let before = r.time();
        let (picture, sound) = field(r);
        assert!(
            picture
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p.as_slice() != &picture.pixels[..4]),
            "active picture"
        );
        assert!(
            sound.iter().any(|x| f32::from_bits(*x).abs() > 0.0001),
            "active sound"
        );
        fields.push((r.time().get() - before.get(), picture, sound));
    }
    postcard::to_allocvec(&(fields, r.snapshot().expect("final state"))).expect("output")
}
fn check<M: C64FamilyMachine>(
    name: &str,
    mut live: C64FamilyRuntime<M>,
    mut restored: C64FamilyRuntime<M>,
) where
    C64FamilyRuntime<M>: MachineCore,
{
    for _ in 0..3 {
        field(&mut live);
    }
    let now = live.time().get();
    let mut picture = LatestFrameCapture::default();
    let mut sound = AudioCapture::default();
    live.run_until(
        MachineTime::new(now + 137),
        &mut HostIo {
            input_events: &[],
            frame_sink: &mut picture,
            audio_sink: &mut sound,
            trace_sink: &mut NullTraceSink,
        },
    )
    .expect("partial field");
    let saved = live.snapshot().expect("save");
    let output = continuation(&mut live);
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/commodore/board-split-v22");
    if std::env::var_os("EMU198X_COMMODORE_CAPTURE").is_some() {
        std::fs::create_dir_all(&root).expect("root");
        for (suffix, data) in [("state", &saved), ("output", &output)] {
            let p = root.join(format!("{name}.{suffix}"));
            if p.exists() {
                assert!(std::fs::read(p).expect("existing") == *data);
            } else {
                use std::io::Write;
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(p)
                    .expect("new fixture")
                    .write_all(data)
                    .expect("write");
            }
        }
    }
    let old = std::fs::read(root.join(format!("{name}.state"))).expect("original state");
    let expected = std::fs::read(root.join(format!("{name}.output"))).expect("original output");
    assert!(saved == old, "{name}: original state");
    assert!(output == expected, "{name}: direct continuation");
    restored.restore(&old).expect("restore");
    assert!(
        restored.snapshot().expect("reencode") == old,
        "{name}: exact reencode"
    );
    assert!(
        continuation(&mut restored) == expected,
        "{name}: restored continuation"
    );
    live.reset(ResetKind::Soft);
    restored.reset(ResetKind::Soft);
    assert!(
        live.snapshot().expect("reset") == restored.snapshot().expect("restored reset"),
        "{name}: reset"
    );
}
#[test]
fn original_v22_states_and_first_three_native_fields_remain_exact() {
    for m in Model::ALL {
        check(m.model_id(), c64(m), c64(m));
    }
    for m in C128Model::ALL {
        check(m.model_id(), c128(m), c128(m));
    }
}
