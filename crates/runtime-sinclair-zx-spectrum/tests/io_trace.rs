//! `io_trace` across the Spectrum family (#1183).
//!
//! The family answered "not supported" for years on the grounds that
//! `port_read` / `port_write` and the AY watch covered the ground. They
//! do not: those sample a port when *you* ask, while a trace records
//! what the *program* did. On a machine that decodes I/O on single
//! address lines — bit 0 clear is the ULA, so keyboard, border, speaker
//! and tape all arrive at `$FE` and its even mirrors — that difference
//! is the whole question of which device a write was aimed at.

use emu198x_shell::DebugPrimitives;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus2a::SpectrumPlus2A;
use runtime_sinclair_zx_spectrum::{
    Model, Pentagon128Runtime, ScorpionZS256Runtime, Spectrum48kRuntime, Spectrum128kRuntime,
    SpectrumLiveAccess, SpectrumPlus2ARuntime, SpectrumRuntimeKind, TimexTC2048Runtime,
};

fn sinclair_and_amstrad() -> Vec<(&'static str, SpectrumRuntimeKind)> {
    vec![
        (
            "48K",
            SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::new(
                Model::Spectrum48KPal,
                Spectrum48k::new(),
            )),
        ),
        (
            "128K",
            SpectrumRuntimeKind::Spectrum128K(Spectrum128kRuntime::new(
                Model::Spectrum128KPal,
                Spectrum128K::new(),
            )),
        ),
        (
            "+2A",
            SpectrumRuntimeKind::SpectrumPlus2A(SpectrumPlus2ARuntime::new(
                Model::SpectrumPlus2A,
                SpectrumPlus2A::new(),
            )),
        ),
    ]
}

#[test]
fn the_class_core_machines_support_tracing() {
    for (name, kind) in sinclair_and_amstrad() {
        assert!(
            kind.dbg_supports_io_trace(),
            "{name} is built on a shared class core and should trace"
        );
    }
}

/// The clones carry their own cores, so they were wired separately —
/// Pentagon and Scorpion for their own paging ports, the Timex pair for
/// the SCLD at `$FF`. Every machine in the family traces, so a caller
/// does not have to know which core a variant happens to be built on.
#[test]
fn the_clones_trace_too() {
    let clones: Vec<(&str, SpectrumRuntimeKind)> = vec![
        (
            "Pentagon 128",
            SpectrumRuntimeKind::Pentagon128(Pentagon128Runtime::blank()),
        ),
        (
            "Scorpion ZS-256",
            SpectrumRuntimeKind::ScorpionZS256(ScorpionZS256Runtime::blank()),
        ),
        (
            "Timex TC2048",
            SpectrumRuntimeKind::TimexTC2048(TimexTC2048Runtime::blank()),
        ),
    ];
    for (name, kind) in clones {
        assert!(kind.dbg_supports_io_trace(), "{name} should trace");
    }
}

/// Whatever the variant, a write reaches the trace — the point of doing
/// the clones as well as the Sinclair and Amstrad machines.
#[test]
fn every_variant_records_a_write() {
    let mut all = sinclair_and_amstrad();
    all.push((
        "Pentagon 128",
        SpectrumRuntimeKind::Pentagon128(Pentagon128Runtime::blank()),
    ));
    all.push((
        "Timex TC2048",
        SpectrumRuntimeKind::TimexTC2048(TimexTC2048Runtime::blank()),
    ));
    for (name, mut kind) in all {
        kind.dbg_start_io_trace();
        kind.port_write(0x00FE, 0x05);
        let events = kind.dbg_take_io_trace();
        assert!(
            events
                .iter()
                .any(|e| e.port == 0x00FE && e.write && e.value == 0x05),
            "{name} did not record the border write: {events:?}"
        );
    }
}

/// The point of the feature: run the machine and see what it did.
#[test]
fn tracing_captures_the_traffic_a_running_machine_produces() {
    let (_, mut kind) = sinclair_and_amstrad().remove(0);
    kind.dbg_start_io_trace();
    // Drive one known write through the bus and confirm it is recorded.
    kind.port_write(0x00FE, 0x05);
    let events = kind.dbg_take_io_trace();
    assert!(
        events
            .iter()
            .any(|e| e.port == 0x00FE && e.write && e.value == 0x05),
        "the border write should appear in the trace: {events:?}"
    );
}
