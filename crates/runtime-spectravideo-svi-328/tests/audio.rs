//! SVI-328 PSG audio reaches the host-facing runtime packet (#254).

use emu198x_shell::{AudioCapture, HostIo, MachineCore, MachineTime, NullFrameSink, NullTraceSink};
use runtime_spectravideo_svi_328::{Model, Svi328Runtime};

#[test]
fn a_psg_tone_reaches_the_host_audio_sink() {
    let mut bios = vec![0; 32 * 1024];
    // LD A,register ; OUT ($88),A ; LD A,value ; OUT ($8C),A.
    // Configure tone A, then loop at $0020 while the PSG keeps ticking.
    let program = [
        0x3E, 0x00, 0xD3, 0x88, 0x3E, 100, 0xD3, 0x8C, // period low
        0x3E, 0x01, 0xD3, 0x88, 0x3E, 0, 0xD3, 0x8C, // period high
        0x3E, 0x07, 0xD3, 0x88, 0x3E, 0x3E, 0xD3, 0x8C, // tone A only
        0x3E, 0x08, 0xD3, 0x88, 0x3E, 15, 0xD3, 0x8C, // volume A
        0xC3, 0x20, 0x00, // JP $0020
    ];
    bios[..program.len()].copy_from_slice(&program);
    let mut runtime = Svi328Runtime::new(Model::Svi328Ntsc, bios).expect("valid synthetic BIOS");

    let mut audio = AudioCapture::default();
    runtime
        .run_until(
            MachineTime::new(60_000),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut audio,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("synthetic BIOS runs");

    let captured = audio.audio().expect("the runtime pushed an audio packet");
    let (minimum, maximum) = captured.samples.iter().copied().fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(minimum, maximum), sample| (minimum.min(sample), maximum.max(sample)),
    );
    assert_eq!(captured.sample_rate, 48_000);
    assert_eq!(captured.channels, 1);
    assert!(!captured.samples.is_empty());
    assert!(
        maximum - minimum > 0.01,
        "an enabled PSG tone should reach the host as a varying waveform"
    );
}
