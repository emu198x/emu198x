//! Host display metadata must describe the same clock as the emitted picture.
use emu198x_shell::display::{Display, NTSC_ACTIVE_LINES, PAL_ACTIVE_LINES};
use emu198x_shell::machine::Region;
use emu198x_shell::{HostIo, LatestFrameCapture, MachineCore, NullAudioSink, NullTraceSink};
use runtime_commodore_amiga::{AmigaRuntimeKind, Model};

#[test]
fn a1200_display_standard_and_pixel_clock_agree_with_native_signal() {
    for (model, expected_region, active_lines) in [
        (Model::A500OcsPal, Region::Pal, PAL_ACTIVE_LINES),
        (Model::A500OcsNtsc, Region::Ntsc, NTSC_ACTIVE_LINES),
        (Model::A500PlusEcsPal, Region::Pal, PAL_ACTIVE_LINES),
        (Model::A500PlusEcsNtsc, Region::Ntsc, NTSC_ACTIVE_LINES),
        (Model::A1200AgaPal, Region::Pal, PAL_ACTIVE_LINES),
        (Model::A1200AgaNtsc, Region::Ntsc, NTSC_ACTIVE_LINES),
    ] {
        let mut firmware = vec![0; 512 * 1024];
        firmware[..4].copy_from_slice(&0x001F_F000u32.to_be_bytes());
        firmware[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
        firmware[8..10].copy_from_slice(&[0x60, 0xFE]);
        let mut runtime = AmigaRuntimeKind::new(model, firmware).expect("A1200");
        let mut capture = LatestFrameCapture::default();
        runtime
            .run_frame(&mut HostIo {
                input_events: &[],
                frame_sink: &mut capture,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            })
            .expect("one native field");
        let signal = capture
            .frame()
            .expect("frame")
            .signal
            .as_ref()
            .expect("RGB timing");
        let Some(Display::Television {
            region,
            pixel_clock_hz,
            lines_per_tv_height,
        }) = runtime.display()
        else {
            panic!("television descriptor")
        };
        assert_eq!(
            region, expected_region,
            "{model:?}: selected board standard"
        );
        assert_eq!(
            pixel_clock_hz, signal.timing.pixel_hz,
            "{model:?}: declared and emitted clocks"
        );
        assert_eq!(
            lines_per_tv_height,
            active_lines * 2.0,
            "both interlaced framebuffer fields"
        );
    }
}
