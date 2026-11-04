//! Physical WAIT must survive a native save without losing either clock half.
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum_amstrad_class::{
    AmstradVariant, Plus2AMarker, Plus2BMarker, Plus3Marker, SpectrumAmstradClassCore,
};
use emu198x_shell::{AudioCapture, HostIo, LatestFrameCapture, MachineCore, NullTraceSink};
use runtime_sinclair_zx_spectrum::{Model, SpectrumMachine, SpectrumRuntime};
use sha2::{Digest, Sha256};

fn frame(runtime: &mut impl MachineCore) -> (Vec<u8>, Vec<u32>, Vec<u8>) {
    let mut frames = LatestFrameCapture::default();
    let mut audio = AudioCapture::default();
    runtime
        .run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut frames,
            audio_sink: &mut audio,
            trace_sink: &mut NullTraceSink,
        })
        .expect("native frame");
    assert_eq!(frames.frames_seen(), 1);
    let pixels = frames.frame().expect("frame").pixels.clone();
    let samples = audio
        .audio()
        .expect("audio")
        .samples
        .iter()
        .map(|s| s.to_bits())
        .collect::<Vec<_>>();
    assert!(
        samples.iter().any(|s| f32::from_bits(*s).abs() > 0.001),
        "active AY"
    );
    (pixels, samples, runtime.snapshot().expect("native save"))
}

fn qualify<V: AmstradVariant>(model: Model, original: &[u8], sha: &str)
where
    SpectrumAmstradClassCore<V>: SpectrumMachine,
{
    assert_eq!(
        Sha256::digest(original)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        sha,
        "immutable v19"
    );
    for offset in 0..8 {
        let mut machine = SpectrumAmstradClassCore::<V>::new();
        machine.memory.write_1ffd(5);
        // Uncontended code reads and writes contended bank 5. The active
        // screen and PSG make continuation compare real video and audio.
        machine.memory.ram_bank_mut(3)[0x3f00..0x3f09]
            .copy_from_slice(&[0x21, 0, 0x40, 0x7e, 0x3c, 0x77, 0xc3, 3, 0xff]);
        machine.z80.regs.pc = 0xff00;
        for (reg, value) in [(0, 40), (1, 0), (7, 0x3e), (8, 15)] {
            machine.ay.select_register(reg);
            machine.ay.write_data(value);
        }
        let mut found = false;
        for _ in 0..710_000 {
            machine.tick_one_halfcycle();
            if machine.z80.wait
                && matches!(
                    machine.z80.phase,
                    emu198x_zilog_z80::z80::Phase::MemRead(
                        emu198x_zilog_z80::z80::MemPhase::T2Rise
                    ) | emu198x_zilog_z80::z80::Phase::MemWrite(
                        emu198x_zilog_z80::z80::MemPhase::T2Rise
                    )
                )
            {
                found = true;
                break;
            }
        }
        assert!(found, "must capture an accepted memory WAIT");
        for _ in 0..offset {
            machine.tick_one_halfcycle();
        }
        let mut live = SpectrumRuntime::new(model, machine);
        let saved = live.snapshot().expect("paused WAIT");
        assert_eq!(saved[0], 20);
        let mut restored = SpectrumRuntime::new(model, SpectrumAmstradClassCore::<V>::new());
        restored.restore(&saved).expect("restore WAIT phase");
        assert_eq!(restored.snapshot().expect("exact round trip"), saved);
        for _ in 0..3 {
            assert_eq!(
                frame(&mut restored),
                frame(&mut live),
                "{model:?}, offset {offset}"
            );
            assert_eq!(restored.time(), live.time(), "native frame clocks");
        }
        // Real old payload, truncated old prefix and forged old-version
        // current payload must all reject before decoding and atomically.
        let before = restored.snapshot().expect("target before rejection");
        let mut forged = saved.clone();
        forged[0] = 19;
        for rejected in [original, &[19], forged.as_slice()] {
            let error = restored.restore(rejected).expect_err("v19 must reject");
            assert!(
                error
                    .to_string()
                    .contains("unsupported snapshot version 19; expected 20"),
                "{error}"
            );
            assert_eq!(restored.snapshot().expect("atomic rejection"), before);
        }
    }
}

#[test]
fn all_amstrad_boards_resume_wait_and_reject_original_v19() {
    qualify::<Plus2AMarker>(
        Model::SpectrumPlus2A,
        include_bytes!("fixtures/amstrad_wait_v19/plus2a.state"),
        "9731e180ff82b8ab26c49f82085ad29f80b421ff99651d7804a34816b659c28a",
    );
    qualify::<Plus2BMarker>(
        Model::SpectrumPlus2B,
        include_bytes!("fixtures/amstrad_wait_v19/plus2b.state"),
        "0d83f0ccaf6d5eb1d2676174dd81c6c86119f0701582da142e77b0895d89accb",
    );
    qualify::<Plus3Marker>(
        Model::SpectrumPlus3,
        include_bytes!("fixtures/amstrad_wait_v19/plus3.state"),
        "47f266a1aeff0a5173161308974f0b182289471b4d97f892b3525afe3042d1af",
    );
}
