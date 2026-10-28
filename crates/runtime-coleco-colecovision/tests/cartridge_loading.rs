use emu198x_shell::{
    HostIo, MachineCore, MachineError, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink,
};
use runtime_coleco_colecovision::{CvRuntime, Model};
const LIMIT: usize = 32768;
fn runtime(model: Model) -> CvRuntime {
    let mut bios = vec![0; 8192];
    bios[..3].copy_from_slice(&[0xC3, 0x24, 0x80]);
    CvRuntime::new(model, bios).expect("BIOS")
}
fn cart() -> Vec<u8> {
    let mut bytes = vec![0; LIMIT];
    // LD A,(last ROM byte); LD (RAM),A; LD A,$5A; LD (RAM+1),A; HALT.
    let last: u16 = 65535;
    let ram: u16 = 24576;
    let [lo, hi] = last.to_le_bytes();
    let [rlo, rhi] = ram.to_le_bytes();
    let [slo, shi] = (ram + 1).to_le_bytes();
    bytes[..2].copy_from_slice(&[0xAA, 0x55]);
    bytes[0x24..0x30].copy_from_slice(&[
        0x3A, lo, hi, 0x32, rlo, rhi, 0x3E, 0x5A, 0x32, slo, shi, 0x76,
    ]);
    bytes[LIMIT - 1] = 0xA7;
    bytes
}
fn load(runtime: &mut CvRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, bytes));
    runtime.load_media(&media)
}
fn run(runtime: &mut CvRuntime) {
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
fn inaccessible_images_reject_without_changing_live_or_blank_state() {
    for model in Model::ALL {
        let mut runtime = runtime(model);
        load(&mut runtime, &cart()).expect("valid");
        run(&mut runtime);
        let before = runtime.snapshot().expect("snapshot");
        let time = runtime.time();
        for size in [LIMIT + 1, LIMIT * 2] {
            let mut bytes = cart();
            bytes.resize(size, 0);
            assert!(
                matches!(
                    load(&mut runtime, &bytes),
                    Err(MachineError::InvalidMedia { .. })
                ),
                "unmapped cartridge bytes must reject"
            );
            assert_eq!(runtime.snapshot().expect("unchanged"), before);
            assert_eq!(runtime.time(), time);
            let mut blank = CvRuntime::blank(model);
            let before = blank.snapshot().expect("blank");
            assert!(load(&mut blank, &bytes).is_err());
            assert!(
                !blank.cartridge_loaded(),
                "rejection must not stage a cartridge"
            );
            assert_eq!(blank.snapshot().expect("blank unchanged"), before);
        }
    }
}
#[test]
fn later_invalid_image_cannot_replace_a_running_cartridge() {
    for model in Model::ALL {
        let mut runtime = runtime(model);
        load(&mut runtime, &cart()).expect("valid");
        run(&mut runtime);
        let before = runtime.snapshot().expect("snapshot");
        for (slot, kind) in [
            ("unknown", MediaKind::Cartridge),
            ("cartridge-1", MediaKind::Disk),
        ] {
            let mut replacement = cart();
            replacement[LIMIT - 1] = 0x42;
            let mut media = MediaSet::new();
            media.push(MediaImage::new(
                "cartridge-1",
                MediaKind::Cartridge,
                &replacement,
            ));
            media.push(MediaImage::new(slot, kind, &replacement));
            assert!(runtime.load_media(&media).is_err());
            assert_eq!(
                runtime.snapshot().expect("unchanged"),
                before,
                "a rejected request must not replace or reset live state"
            );
        }
    }
}

#[test]
fn old_unchecked_snapshot_images_remain_restorable() {
    for model in Model::ALL {
        let mut source = runtime(model);
        let mut image = cart();
        image.push(0x42);
        // Previous/direct construction can retain bytes the current mapper cannot
        // address. Admission tightening must not make an existing save unreadable.
        source.insert_cartridge(image);
        run(&mut source);
        let state = source.snapshot().expect("legacy state");
        let mut restored = CvRuntime::blank(model);
        restored.restore(&state).expect("legacy restore");
        assert_eq!(restored.snapshot().expect("unchanged legacy state"), state);
    }
}
