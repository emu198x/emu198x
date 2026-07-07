//! Snapshot/restore round-trip coverage for the Mattel Aquarius runtime.
//!
//! Part of the best-in-class W1 floor: a generic run -> snapshot -> restore ->
//! run-forward-in-lockstep guard, giving CI a fleet-wide "snapshot regression
//! anywhere" tripwire. It leans only on the ROM-free `blank()` constructor and
//! the shared `MachineCore` surface, so it needs no firmware and runs on every
//! push.

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_mattel_aquarius::{AquariusRuntime, Model};

const MODEL: Model = Model::Aquarius;
const WARMUP: u64 = 50_000;
const LOCKSTEP: u64 = 100_000;

fn run_to(runtime: &mut AquariusRuntime, target: u64) {
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

#[test]
fn snapshot_round_trip_preserves_live_state() {
    let mut runtime = AquariusRuntime::blank(MODEL);
    run_to(&mut runtime, WARMUP);

    let snap = runtime.snapshot().expect("blank runtime should snapshot");

    let mut restored = AquariusRuntime::blank(MODEL);
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

    // The strong check: run the original and the restored machine forward the
    // same amount. If restore captured the full *live* state (not just a
    // re-serialisable subset), the two stay bit-identical.
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
