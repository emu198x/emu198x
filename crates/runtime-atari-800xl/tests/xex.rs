//! Atari segmented executable loading through the runtime media surface.

use emu198x_shell::{
    HostIo, MachineCore, MachineError, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink,
};
use runtime_atari_800xl::{Atari800xlRuntime, Model};

fn segment(image: &mut Vec<u8>, start: u16, bytes: &[u8]) {
    let end = start + u16::try_from(bytes.len()).expect("small test segment") - 1;
    image.extend_from_slice(&start.to_le_bytes());
    image.extend_from_slice(&end.to_le_bytes());
    image.extend_from_slice(bytes);
}

fn load(runtime: &mut Atari800xlRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("program-1", MediaKind::Program, bytes));
    runtime.load_media(&media)
}

fn run_past_autoload(runtime: &mut Atari800xlRuntime) {
    let clocks_per_frame = runtime.machine().expect("machine").clocks_per_frame();
    runtime
        .run_until(
            MachineTime::new(clocks_per_frame * 152),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("XEX should run");
}

#[test]
fn multi_segment_xex_runs_init_then_run() {
    let mut image = vec![0xFF, 0xFF];
    // INIT: LDA #$A5 ; STA $0600 ; RTS
    segment(&mut image, 0x2000, &[0xA9, 0xA5, 0x8D, 0x00, 0x06, 0x60]);
    segment(&mut image, 0x02E2, &[0x00, 0x20]);
    // RUN: LDA #$5A ; STA $0601 ; JMP $2105
    segment(
        &mut image,
        0x2100,
        &[0xA9, 0x5A, 0x8D, 0x01, 0x06, 0x4C, 0x05, 0x21],
    );
    segment(&mut image, 0x02E0, &[0x00, 0x21]);

    let mut runtime = Atari800xlRuntime::new(
        Model::A800xlNtsc,
        Some(vec![0; 16 * 1024]),
        None,
        None,
        false,
    )
    .expect("synthetic OS builds machine");
    load(&mut runtime, &image).expect("valid XEX loads");
    run_past_autoload(&mut runtime);

    let machine = runtime.machine().expect("machine");
    assert_eq!(machine.peek(0x0600), 0xA5, "INIT ran before later segments");
    assert_eq!(machine.peek(0x0601), 0x5A, "RUNAD was entered");
}

#[test]
fn malformed_xex_is_rejected_at_mount_time() {
    let mut runtime = Atari800xlRuntime::blank(Model::A800xlNtsc);
    match load(&mut runtime, &[0xFF, 0xFF, 0x00]) {
        Err(MachineError::InvalidMedia { slot, .. }) => assert_eq!(slot, "program-1"),
        other => panic!("expected InvalidMedia, got {other:?}"),
    }
}

/// Exercise a real XEX supplied by the caller. This is intentionally ignored
/// in CI because the repository does not redistribute Atari software.
#[test]
#[ignore = "FIXTURE: set EMU198X_ATARI_XEX and EMU198X_ATARI_OS"]
fn real_xex_mounts_and_reaches_run() {
    let path = std::env::var("EMU198X_ATARI_XEX").expect("set EMU198X_ATARI_XEX");
    let bytes = std::fs::read(path).expect("read XEX");
    let os_path = std::env::var("EMU198X_ATARI_OS").expect("set EMU198X_ATARI_OS");
    let os = std::fs::read(os_path).expect("read Atari OS ROM");
    let mut runtime = Atari800xlRuntime::new(Model::A800xlNtsc, Some(os), None, None, false)
        .expect("synthetic OS builds machine");
    load(&mut runtime, &bytes).expect("real XEX mounts");
    run_past_autoload(&mut runtime);
}
