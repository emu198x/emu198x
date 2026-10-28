use emu198x_shell::{
    HostIo, MachineCore, MachineError, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink, ResetKind,
};
use runtime_sord_m5::{M5Runtime, Model};
const LIMIT: usize = 20480;
fn runtime(model: Model) -> M5Runtime {
    let mut bios = vec![0; 8192];
    bios[..3].copy_from_slice(&[0xC3, 0, 0x20]);
    M5Runtime::new(model, bios)
}
fn cart() -> Vec<u8> {
    let mut bytes = vec![0; LIMIT];
    // LD A,(last ROM byte); LD (RAM),A; LD A,$5A; LD (RAM+1),A; HALT.
    let last: u16 = 28671;
    let ram: u16 = 28672;
    let [lo, hi] = last.to_le_bytes();
    let [rlo, rhi] = ram.to_le_bytes();
    let [slo, shi] = (ram + 1).to_le_bytes();
    bytes[..12].copy_from_slice(&[
        0x3A, lo, hi, 0x32, rlo, rhi, 0x3E, 0x5A, 0x32, slo, shi, 0x76,
    ]);
    bytes[LIMIT - 1] = 0xA7;
    bytes
}
fn load(runtime: &mut M5Runtime, bytes: &[u8]) -> Result<(), MachineError> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, bytes));
    runtime.load_media(&media)
}
fn run(runtime: &mut M5Runtime) {
    runtime
        .run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut NullFrameSink,
            audio_sink: &mut NullAudioSink,
            trace_sink: &mut NullTraceSink,
        })
        .expect("field");
}
#[test]
fn maximum_linear_image_is_guest_visible_after_load_reset_and_restore() {
    for model in Model::ALL {
        let mut runtime = runtime(model);
        load(&mut runtime, &cart()).expect("boundary image");
        run(&mut runtime);
        assert_eq!(runtime.machine().expect("machine").peek(28672), 0xA7);
        assert_eq!(
            runtime.machine().expect("machine").peek(28672 + 1),
            0x5A,
            "CPU guest completes"
        );
        let saved = runtime.snapshot().expect("saved");
        let mut restored = M5Runtime::blank(model);
        restored.restore(&saved).expect("restore");
        run(&mut runtime);
        run(&mut restored);
        assert_eq!(
            runtime.snapshot().expect("continued"),
            restored.snapshot().expect("replayed")
        );
        restored.reset(ResetKind::Hard);
        run(&mut restored);
        assert_eq!(restored.machine().expect("machine").peek(28672), 0xA7);
        assert_eq!(restored.machine().expect("machine").peek(28672 + 1), 0x5A);
    }
}

#[test]
fn restoring_empty_state_clears_reset_images() {
    for model in Model::ALL {
        let mut populated = runtime(model);
        load(&mut populated, &cart()).expect("cart");
        let empty = M5Runtime::blank(model).snapshot().expect("empty snapshot");
        populated.restore(&empty).expect("restore empty");
        assert!(!populated.cartridge_loaded());
        populated.reset(ResetKind::Hard);
        assert!(
            populated.machine().is_none(),
            "reset must not resurrect pre-restore media"
        );
    }
}
#[test]
fn restoring_over_other_media_replaces_reset_images() {
    for model in Model::ALL {
        let mut source = runtime(model);
        load(&mut source, &cart()).expect("source");
        run(&mut source);
        let saved = source.snapshot().expect("saved");
        let mut target = runtime(model);
        let mut other = cart();
        other[LIMIT - 1] = 0x42;
        load(&mut target, &other).expect("other");
        run(&mut target);
        target.restore(&saved).expect("restore");
        assert!(target.cartridge_loaded());
        target.reset(ResetKind::Hard);
        run(&mut target);
        assert_eq!(target.machine().expect("reset machine").peek(28672), 0xA7);
        assert_eq!(
            target.machine().expect("reset machine").peek(28672 + 1),
            0x5A
        );
    }
}
