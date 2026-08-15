//! Snapshot/restore round-trip coverage for the Amstrad CPC runtime.
//!
//! Part of the fleet-wide W1 floor: run -> snapshot -> restore -> run forward
//! in lockstep, so a snapshot regression anywhere trips on every push.
//!
//! Unlike the runtimes that test this through `blank()`, this one builds a
//! synthetic 32 KB firmware first. A blank runtime holds no machine at all, so
//! `run_until` returns immediately and the lockstep comparison is two empty
//! snapshots against each other — it would pass with the whole snapshot path
//! deleted. Sixteen KB of `NOP` under sixteen of `RET` costs nothing, needs no
//! copyrighted ROM, and gives the CPU, Gate Array, CRTC and PSG real state to
//! preserve.

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_amstrad_cpc::{AmstradCpcRuntime, Model};

const MODEL: Model = Model::Cpc464;
const WARMUP: u64 = 50_000;
const LOCKSTEP: u64 = 150_000;

/// 16 KB of `NOP` for the OS half, 16 KB of `RET` for BASIC — enough for the
/// Z80 to run and the video and interrupt logic to advance.
fn test_firmware() -> Vec<u8> {
    let mut rom = vec![0x00u8; 0x8000];
    rom[0x4000..].fill(0xC9);
    rom
}

fn runtime() -> AmstradCpcRuntime {
    AmstradCpcRuntime::new(MODEL, test_firmware()).expect("32 KB firmware is accepted")
}

fn run_to(runtime: &mut AmstradCpcRuntime, target: u64) {
    runtime
        .run_until(
            MachineTime::new(target),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("the runtime should run without a host-sink error");
}

#[test]
fn snapshot_round_trip_preserves_live_state() {
    let mut original = runtime();
    run_to(&mut original, WARMUP);
    assert!(original.time().get() >= WARMUP, "the machine actually ran");

    let snap = original.snapshot().expect("runtime should snapshot");

    let mut restored = runtime();
    restored.restore(&snap).expect("snapshot should restore");

    assert_eq!(restored.time(), original.time(), "restore preserves time");
    assert_eq!(
        restored.snapshot().expect("restored should re-snapshot"),
        snap,
        "restore then re-snapshot must reproduce the bytes exactly",
    );

    // The strong check: run both forward the same amount. If restore captured
    // the full *live* state rather than a re-serialisable subset, the two stay
    // bit-identical — including the Z80's rehydrated micro-op walker.
    run_to(&mut original, LOCKSTEP);
    run_to(&mut restored, LOCKSTEP);
    assert_eq!(
        restored.snapshot().expect("restored should re-snapshot"),
        original.snapshot().expect("original should re-snapshot"),
        "a restored machine must evolve identically to the original",
    );
}

#[test]
fn a_snapshot_from_another_model_is_refused() {
    // One model today, but the check is what keeps a 6128 snapshot from being
    // restored into a 464 the day there are two.
    let mut runtime = runtime();
    run_to(&mut runtime, WARMUP);
    let snap = runtime.snapshot().expect("runtime should snapshot");
    // Corrupt the model string in place rather than fabricate an envelope, so
    // the test exercises the real decode path.
    let model = Model::Cpc464.model_id().as_bytes();
    let at = snap
        .windows(model.len())
        .position(|w| w == model)
        .expect("the model id is in the encoded snapshot");
    let mut tampered = snap.clone();
    tampered[at] = b'X';

    let err = runtime
        .restore(&tampered)
        .expect_err("a foreign model should be refused");
    assert!(
        format!("{err}").contains("does not match"),
        "unexpected error: {err}"
    );
}

#[test]
fn a_runtime_with_no_firmware_runs_without_advancing() {
    // `blank()` is the pre-firmware state the host sits in before a ROM is
    // chosen. It must not panic, and it must not pretend to have run.
    let mut runtime = AmstradCpcRuntime::blank(MODEL);
    run_to(&mut runtime, WARMUP);
    assert_eq!(runtime.time().get(), 0);
    assert!(runtime.machine().is_none());
}
