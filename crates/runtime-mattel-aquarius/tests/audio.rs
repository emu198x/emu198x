//! 1-bit speaker audio for the Mattel Aquarius (#309).
//!
//! Boots a synthetic ROM whose reset vector bit-bangs the speaker latch on port
//! `$FF`, and confirms the runtime pushes a real waveform into the
//! `AudioPacket` it plumbs instead of the empty buffer it used to send.

use emu198x_shell::{
    AudioCapture, CapturedAudio, HostIo, MachineCore, MachineTime, NullFrameSink, NullTraceSink,
};
use runtime_mattel_aquarius::{AquariusRuntime, Model};

/// 8 KB BASIC-ROM stand-in that runs the canonical beeper loop from reset:
/// write bit 0 of port `$FF`, wait out a `DJNZ` delay, flip the bit, repeat.
fn beeper_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x2000];
    rom[0x0000..0x000B].copy_from_slice(&[
        0xAF, // XOR A          ; speaker level starts low
        0xD3, 0xFF, // OUT ($FF),A   ; drive the speaker latch
        0x06, 0x00, // LD B,0        ; 256 delay iterations
        0x10, 0xFE, // DJNZ -2
        0xEE, 0x01, // XOR $01       ; flip the level
        0x18, 0xF6, // JR -10        ; back to the OUT
    ]);
    rom
}

fn capture(rom: Vec<u8>) -> CapturedAudio {
    let mut runtime = AquariusRuntime::new(Model::Aquarius, rom).expect("synthetic ROM builds");
    let mut frames = NullFrameSink;
    let mut audio = AudioCapture::default();
    let mut trace = NullTraceSink;
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frames,
        audio_sink: &mut audio,
        trace_sink: &mut trace,
    };
    runtime
        .run_until(MachineTime::new(200_000), &mut host)
        .expect("runs");
    audio.audio().expect("audio was captured").clone()
}

#[test]
fn a_beeper_program_produces_a_non_empty_waveform() {
    let captured = capture(beeper_rom());

    assert_eq!(captured.sample_rate, 48_000);
    assert_eq!(captured.channels, 1);
    assert!(
        !captured.samples.is_empty(),
        "the beeper loop produced audio samples"
    );
    assert!(
        captured.samples.iter().any(|&s| s > 0.0),
        "speaker-high samples are present"
    );
    assert!(
        captured.samples.iter().any(|&s| s < 0.0),
        "speaker-low samples are present"
    );
}

#[test]
fn an_idle_machine_still_reports_the_declared_format() {
    // A ROM that halts at reset never touches $FF, so the packet carries a flat
    // level rather than nothing at all — silence is a waveform, not an absence.
    let mut rom = vec![0u8; 0x2000];
    rom[0x0000] = 0x18; // JR -2
    rom[0x0001] = 0xFE;
    let captured = capture(rom);

    assert_eq!(captured.sample_rate, 48_000);
    assert_eq!(captured.channels, 1);
    assert!(!captured.samples.is_empty(), "samples are still emitted");
    assert!(
        captured
            .samples
            .iter()
            .all(|&s| (s - captured.samples[0]).abs() < f32::EPSILON),
        "an idle speaker produces no waveform"
    );
}
