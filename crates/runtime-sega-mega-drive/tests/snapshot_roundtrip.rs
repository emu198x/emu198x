//! Snapshot/restore round-trip coverage for the Mega Drive runtime.
//!
//! The fleet-wide run -> snapshot -> restore -> run-forward-in-lockstep
//! guard. A blank console has no machine to snapshot, so this runs a small
//! synthetic cartridge whose 68000 takes V-ints and plays the PSG.

mod common;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink, ResetKind,
};
use runtime_sega_mega_drive::{MegaDriveRuntime, Model, blank, with_cartridge};

const MODEL: Model = Model::Genesis;
const WARMUP: u64 = 3 * 262 * 3420;
const LOCKSTEP: u64 = 6 * 262 * 3420;

fn run_to(runtime: &mut MegaDriveRuntime, target: u64) {
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
    let mut runtime = with_cartridge(MODEL, common::cartridge());
    run_to(&mut runtime, WARMUP);

    let snap = runtime.snapshot().expect("runtime should snapshot");

    let mut restored = blank(MODEL);
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
    let frames = common::ram_word(restored.machine().expect("machine").ram(), common::RAM + 2);
    assert!(frames >= 5, "the V-int handler ran: {frames}");
}

/// A snapshot restored into a runtime that never had a cartridge brings
/// its ROM with it, so a hard reset afterwards still has one to boot.
#[test]
fn a_restored_runtime_can_hard_reset() {
    let mut runtime = with_cartridge(MODEL, common::cartridge());
    run_to(&mut runtime, WARMUP);
    let mut restored = blank(MODEL);
    restored
        .restore(&runtime.snapshot().expect("snapshot"))
        .expect("restore");
    restored.reset(ResetKind::Hard);
    let machine = restored.machine().expect("the cartridge is still in");
    assert_eq!(machine.rom(), common::cartridge());
    assert_eq!(restored.time(), MachineTime::default());
}
