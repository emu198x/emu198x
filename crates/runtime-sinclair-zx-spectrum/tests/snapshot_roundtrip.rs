//! Snapshot/restore round-trip coverage for the ZX Spectrum family runtime.
//!
//! Part of the best-in-class W1 floor: a generic run -> snapshot -> restore ->
//! run-forward-in-lockstep guard, giving CI a fleet-wide "snapshot regression
//! anywhere" tripwire. The Spectrum is a multi-variant family, so this covers
//! the structurally-distinct memory layouts — unexpanded 16K, classic 48K,
//! paged 128K, and the +3 with its disk system — through the shared
//! `MachineCore` surface, leaning only on the ROM-free `blank()` constructors
//! so it needs no firmware and runs on every push.

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_sinclair_zx_spectrum::{
    Spectrum16kRuntime, Spectrum48kRuntime, Spectrum128kRuntime, SpectrumPlus3Runtime,
};

const WARMUP: u64 = 50_000;
const LOCKSTEP: u64 = 100_000;

fn run_to(runtime: &mut impl MachineCore, target: u64) {
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
        .expect("a blank runtime should run without a host-sink error");
}

/// Runs `runtime` forward, snapshots it, restores into `restored`, then advances
/// both in lockstep and asserts they stay bit-identical — proving restore
/// captured the full live state, not just a re-serialisable subset.
fn assert_round_trip<R: MachineCore>(mut runtime: R, mut restored: R) {
    run_to(&mut runtime, WARMUP);

    let snap = runtime.snapshot().expect("blank runtime should snapshot");
    restored.restore(&snap).expect("snapshot should restore");

    assert_eq!(
        restored.time(),
        runtime.time(),
        "restore must preserve time"
    );
    assert_eq!(
        restored
            .snapshot()
            .expect("restored runtime should re-snapshot"),
        snap,
        "restore then re-snapshot must reproduce the bytes exactly",
    );

    run_to(&mut runtime, LOCKSTEP);
    run_to(&mut restored, LOCKSTEP);
    assert_eq!(
        restored
            .snapshot()
            .expect("restored runtime should re-snapshot"),
        runtime.snapshot().expect("runtime should re-snapshot"),
        "a restored machine must evolve identically to the original",
    );
}

#[test]
fn snapshot_round_trip_16k() {
    assert_round_trip(Spectrum16kRuntime::blank(), Spectrum16kRuntime::blank());
}

#[test]
fn snapshot_round_trip_48k() {
    assert_round_trip(Spectrum48kRuntime::blank(), Spectrum48kRuntime::blank());
}

#[test]
fn snapshot_round_trip_128k() {
    assert_round_trip(Spectrum128kRuntime::blank(), Spectrum128kRuntime::blank());
}

#[test]
fn snapshot_round_trip_plus3() {
    assert_round_trip(SpectrumPlus3Runtime::blank(), SpectrumPlus3Runtime::blank());
}
