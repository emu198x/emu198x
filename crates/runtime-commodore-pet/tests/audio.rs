//! CB2 piezo audio for the Commodore PET (#348).
//!
//! Boots a synthetic ROM set and programs the VIA's shift register the way PET
//! BASIC does for a tone, confirming the runtime pushes a real waveform into
//! the `AudioPacket` it plumbs instead of the empty buffer it used to send.

use emu198x_shell::{
    AudioCapture, CapturedAudio, HostIo, MachineCore, MachineTime, NullFrameSink, NullTraceSink,
};
use runtime_commodore_pet::{Model, PetRuntime};

/// 6502 program run from reset: point the VIA at a free-running shift-out tone
/// (ACR bits 4-2 = 100) at the T2 rate, then spin. This is the PET's BASIC
/// sound path — T2 sets the pitch, the shift-register byte the timbre.
fn tone_kernal() -> Vec<u8> {
    let mut kernal = vec![0xEAu8; 0x1000];
    let program: &[u8] = &[
        0xA9, 100, // LDA #100
        0x8D, 0x48, 0xE8, // STA $E848   ; T2L-L sets the shift rate
        0xA9, 0x10, // LDA #$10
        0x8D, 0x4B, 0xE8, // STA $E84B   ; ACR: free-running shift out
        0xA9, 0xF0, // LDA #$F0
        0x8D, 0x4A, 0xE8, // STA $E84A   ; SR pattern -> CB2
        0x4C, 0x0F, 0xF0, // JMP $F00F   ; spin
    ];
    kernal[0..program.len()].copy_from_slice(program);
    // Reset vector -> $F000.
    kernal[0x0FFC] = 0x00;
    kernal[0x0FFD] = 0xF0;
    kernal
}

fn silent_kernal() -> Vec<u8> {
    let mut kernal = vec![0xEAu8; 0x1000];
    kernal[0x0FFC] = 0x00;
    kernal[0x0FFD] = 0xF0;
    kernal
}

fn capture(kernal: Vec<u8>) -> CapturedAudio {
    let mut runtime = PetRuntime::new(
        Model::Pet40Col,
        kernal,
        vec![0u8; 0x2000],
        vec![0u8; 0x0800],
        vec![0u8; 0x1000],
    )
    .expect("synthetic ROM set builds");
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
fn a_shift_register_tone_produces_a_non_empty_waveform() {
    let captured = capture(tone_kernal());

    assert_eq!(captured.sample_rate, 48_000);
    assert_eq!(captured.channels, 1);
    assert!(
        !captured.samples.is_empty(),
        "the tone produced audio samples"
    );
    assert!(
        captured.samples.iter().any(|&s| s > 0.0),
        "piezo-high samples are present"
    );
    assert!(
        captured.samples.iter().any(|&s| s < 0.0),
        "piezo-low samples are present"
    );
}

#[test]
fn an_idle_machine_still_reports_the_declared_format() {
    let captured = capture(silent_kernal());

    assert_eq!(captured.sample_rate, 48_000);
    assert_eq!(captured.channels, 1);
    assert!(!captured.samples.is_empty(), "samples are still emitted");
    assert!(
        captured
            .samples
            .iter()
            .all(|&s| (s - captured.samples[0]).abs() < f32::EPSILON),
        "an untouched CB2 produces no waveform"
    );
}
