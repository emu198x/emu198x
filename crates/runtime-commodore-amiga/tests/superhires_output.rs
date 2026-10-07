//! Native sample transport and geometry for the fixed Lisa framebuffer.
use emu198x_shell::{FamilyRuntime, HeadlessSession, MachineCore};
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model};

#[test]
fn full_lisa_frame_and_clock_preserve_physical_picture_width() {
    let mut physical_widths = Vec::new();
    for (model, width, clock) in [
        (Model::A500OcsPal, 768, 14_187_580.0),
        (Model::A1200AgaPal, 1536, 28_375_160.0),
    ] {
        let runtime =
            AmigaRuntimeKind::new(model, vec![0; 512 * 1024]).expect("blank firmware runtime");
        assert_eq!(runtime.framebuffer_dims(), (width, 576));
        let aspect = runtime
            .display()
            .expect("display geometry")
            .pixel_aspect_ratio(width, 576);
        physical_widths.push(width as f32 * aspect);
        let ticks = runtime.native_frame_ticks();
        let mut session = HeadlessSession::new(runtime, ticks);
        session.run_frames(1).expect("emit native frame");
        let frame = session.latest_frame().expect("frame");
        assert_eq!((frame.width, frame.height), (width, 576));
        assert_eq!(frame.pixels.len(), (width * 576 * 4) as usize);
        let timing = frame.signal.as_ref().expect("RGB signal").timing;
        assert_eq!(timing.pixel_hz, clock);
        assert_eq!(timing.line_pixels, width);
    }
    assert!((physical_widths[0] - physical_widths[1]).abs() < 0.001);
}
