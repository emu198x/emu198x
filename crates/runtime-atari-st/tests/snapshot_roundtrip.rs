//! Snapshot/restore round-trip coverage for the Atari ST runtime.
//!
//! The fleet-wide run -> snapshot -> restore -> run-forward-in-lockstep
//! guard, on a synthetic TOS whose 68000 takes VBLs and plays the YM2149.

mod common;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink, ResetKind,
};
use runtime_atari_st::{AtariStRuntime, Model, Monitor};

const MODEL: Model = Model::St520Pal;
const WARMUP: u64 = 3 * 313 * 512;
const LOCKSTEP: u64 = 6 * 313 * 512;

fn run_to(runtime: &mut AtariStRuntime, target: u64) {
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

fn fresh() -> AtariStRuntime {
    AtariStRuntime::new(MODEL, common::tos()).expect("192 KB TOS")
}

#[test]
fn snapshot_round_trip_preserves_live_state() {
    let mut runtime = fresh();
    runtime.insert_disk(0, &[0u8; 737_280]).expect("720 KB");
    run_to(&mut runtime, WARMUP);

    let snap = runtime.snapshot().expect("runtime should snapshot");

    let mut restored = fresh();
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
    let machine = restored.machine().expect("machine");
    let frames = common::word(machine, common::COUNTER + 2);
    assert!(frames >= 5, "the VBL handler ran: {frames}");
    assert!(machine.disk(0).is_some(), "the disk came with the snapshot");
}

/// A snapshot does not carry TOS, so it restores only into a runtime that
/// has one.
#[test]
fn a_snapshot_restores_only_into_a_runtime_with_tos() {
    let mut runtime = fresh();
    run_to(&mut runtime, WARMUP);
    let snap = runtime.snapshot().expect("snapshot");
    let mut blank = runtime_atari_st::blank(MODEL);
    assert!(blank.restore(&snap).is_err());
}

/// The monitor a snapshot was taken on comes back with it, and a hard reset
/// afterwards boots the same TOS on it.
#[test]
fn a_restored_runtime_keeps_its_monitor_and_can_hard_reset() {
    let mut runtime = fresh();
    runtime.set_monitor(Monitor::Mono);
    run_to(&mut runtime, WARMUP);
    let mut restored = AtariStRuntime::new(MODEL, common::tos()).expect("TOS");
    restored
        .restore(&runtime.snapshot().expect("snapshot"))
        .expect("restore");
    assert_eq!(restored.monitor(), Monitor::Mono);
    restored.reset(ResetKind::Hard);
    let machine = restored.machine().expect("TOS is still in");
    assert_eq!(machine.rom(), common::tos());
    assert_eq!(machine.monitor(), Monitor::Mono);
    assert_eq!(restored.time(), MachineTime::default());
}
