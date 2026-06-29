//! 1-bit speaker audio for the Acorn Atom (#368).
//!
//! Loads a synthetic `.atm` that bit-bangs the speaker (8255 PC2) and confirms
//! the runtime pushes a non-empty waveform into the `AudioPacket` it plumbs.

use emu198x_shell::{
    AudioCapture, HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullFrameSink,
    NullTraceSink,
};
use runtime_acorn_atom::{AtomRuntime, Model};

/// Build a `.atm`: 16-byte (blank) name, LE load/exec/length, then the body.
fn atm(load: u16, exec: u16, body: &[u8]) -> Vec<u8> {
    let mut image = vec![0u8; 16];
    image.extend_from_slice(&load.to_le_bytes());
    image.extend_from_slice(&exec.to_le_bytes());
    image.extend_from_slice(&(body.len() as u16).to_le_bytes());
    image.extend_from_slice(body);
    image
}

#[test]
fn a_toggler_program_produces_a_non_empty_waveform() {
    let mut rt = AtomRuntime::new(Model::AtomFull, vec![0u8; 24 * 1024])
        .expect("synthetic ROM builds the machine");

    // LDA $B002 ; EOR #$04 ; STA $B002 ; JMP $0200 — the Atom beeper loop (toggle
    // PC2), at $0200; auto-runs because the exec address is in low RAM.
    let toggler = [
        0xAD, 0x02, 0xB0, // LDA $B002
        0x49, 0x04, // EOR #$04
        0x8D, 0x02, 0xB0, // STA $B002
        0x4C, 0x00, 0x02, // JMP $0200
    ];
    let image = atm(0x0200, 0x0200, &toggler);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("program-1", MediaKind::Program, &image));
    rt.load_media(&media).expect(".atm loads");

    let mut frames = NullFrameSink;
    let mut audio = AudioCapture::default();
    let mut trace = NullTraceSink;
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frames,
        audio_sink: &mut audio,
        trace_sink: &mut trace,
    };
    // ~5 PAL fields (each ≈ 20 000 master ticks at 1 MHz).
    rt.run_until(MachineTime::new(100_000), &mut host)
        .expect("runs");

    let captured = audio.audio().expect("audio was captured");
    assert_eq!(captured.sample_rate, 48_000);
    assert_eq!(captured.channels, 1);
    assert!(
        !captured.samples.is_empty(),
        "the toggler produced audio samples"
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
