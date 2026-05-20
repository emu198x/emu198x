//! Standing boot-invariant suite for the Spectrum 48K runtime.
//!
//! Each test asserts a known-good waypoint that the project has reached
//! and depends on. The file is the canonical regression gate for
//! Spectrum-shaped breakage — when a refactor touches the Z80 core,
//! the ULA, the contention model, or the runtime envelope, these are
//! the tests that should stay green.
//!
//! Hermetic invariants run on every `cargo test --workspace`. ROM-
//! backed invariants are `#[ignore]`'d and resolve assets from
//! `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`.
//!
//! Promoted from existing waypoints per A.2 of
//! `docs/plans/2026-04-28-october-runup-plan.md`.

use std::borrow::Cow;
use std::error::Error;
use std::path::PathBuf;

use common_sinclair_zx_spectrum::ula::Ula;
use emu198x_shell::{
    HostIo, InputEvent, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
};
use machine_pentagon_128::Pentagon128;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus3::SpectrumPlus3;
use runtime_sinclair_zx_spectrum::{
    Model, Pentagon128Runtime, Spectrum128kRuntime, Spectrum48kRuntime, SpectrumMachine,
    SpectrumPlus3Runtime,
};

fn null_host() -> HostIo<'static> {
    HostIo {
        input_events: &[],
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    }
}

fn home_rom_48k() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    if path.exists() { Some(path) } else { None }
}

// ─────────────────────────────────────────────────────────────────────
// Hermetic — run on every cargo test
// ─────────────────────────────────────────────────────────────────────

/// Waypoint: dummy 16 KiB ROM constructs a runtime cleanly.
///
/// Catches regression: any change to `Spectrum48kRuntime::from_rom_bytes`
/// shape — a moving validation envelope or a stricter size check would
/// fail this immediately.
#[test]
fn dummy_rom_constructs_runtime() {
    let runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])
        .expect("dummy 16 KiB ROM should construct cleanly");
    assert_eq!(runtime.time(), MachineTime::default());
}

/// Waypoint: blank runtime advances time when run forward.
///
/// Catches regression: any infinite-loop / hang in the master-clock
/// run loop, the half-cycle Z80 dispatch, or the frame-emission path.
#[test]
fn run_until_advances_past_first_frame() -> Result<(), Box<dyn Error>> {
    let mut runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])?;
    let mut host = null_host();
    let target = MachineTime::new(80_000);
    runtime.run_until(target, &mut host)?;
    let now = runtime.time();
    assert!(
        now.get() >= 50_000,
        "runtime should have advanced at least one frame, got {now:?}"
    );
    Ok(())
}

/// Waypoint: snapshot → restore → snapshot is a fixed point on a
/// dummy-ROM runtime that has been ticked far enough to have non-
/// trivial state.
///
/// Catches regression: any chip-state field that fails to round-trip.
#[test]
fn snapshot_round_trip_is_fixed_point_after_warmup() -> Result<(), Box<dyn Error>> {
    let mut original = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])?;
    let mut host = null_host();
    original.run_until(MachineTime::new(50_000), &mut host)?;
    let bytes_a = original.snapshot()?;
    let mut restored = Spectrum48kRuntime::blank();
    restored.restore(&bytes_a)?;
    let bytes_b = restored.snapshot()?;
    assert_eq!(bytes_a, bytes_b, "snapshot drift after restore");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// Seam 5 — canonical waypoints from the architecture review
// ─────────────────────────────────────────────────────────────────────
// Five per-variant invariants the catalogue cannot catch by
// construction. Each waypoint targets one specific load-bearing
// timing or routing fact; together they form a second line of
// defence behind the catalogue's end-state hashes. See
// `knowledge/decisions/spectrum-architecture-review.md` Seam 5.

/// **Seam 5 waypoint #1:** Interrupt asserts at the canonical T-state.
///
/// On the 48K, /INT goes active at scan 248, pixel 1 (T-state
/// 55552 from frame start: 248 lines × 224 T-states = 55552, plus
/// half a T-state into pixel 1) and stays active for 32 T-states
/// until pixel 65 (T-state ≈ 55584). Outside that window the ULA's
/// `int_active` is false and `z80.irq` follows.
///
/// Catches regression: any change to `int_start_pixel`,
/// `int_end_pixel`, or the half-cycle dispatch order that breaks
/// the canonical INT timing.
#[test]
fn int_asserts_at_canonical_t_state_48k() -> Result<(), Box<dyn Error>> {
    let mut runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])?;

    // Step one frame forward, then position just before the INT
    // window. After this advance, scan should be at 248 and pixel
    // at ~0 — INT hasn't fired yet on this scan.
    runtime
        .machine_mut()
        .advance_tstates(248 * 224 - 1);
    assert!(
        !runtime.machine().z80().irq,
        "INT must not be asserted before the canonical T-state"
    );

    // Step two more T-states to cross into the INT-active window
    // (scan 248, pixel ≥ 1).
    runtime.machine_mut().advance_tstates(2);
    assert!(
        runtime.machine().z80().irq,
        "INT must be asserted at scan 248, pixel ≥ 1"
    );

    // Step past pixel 65 (32 T-states active) — INT should fall.
    runtime.machine_mut().advance_tstates(40);
    assert!(
        !runtime.machine().z80().irq,
        "INT must deassert after the 32-T-state active window"
    );
    Ok(())
}

/// **Seam 5 waypoint #1b:** Interrupt asserts at the canonical
/// half-cycle on the 128K.
///
/// The Sinclair 128K shares `int_scan = 248` with the 48K but runs at
/// the 17.7 MHz master clock (`cpu_divisor = 5`, not 4), so a T-state
/// is 5 half-cycles long and does not divide evenly into the ULA's
/// 2-half-cycles-per-pixel tick rate. Asserting in half-cycles is
/// the only fraction-free way to pin the INT-fires-here position:
/// `(248 × 456 + 1) × 2 = 226 178` half-cycles from frame start
/// (`pixels_per_line = 456`, `int_start_pixel = 1`, two half-cycles
/// per ULA pixel).
///
/// Catches regression: any drift in `int_scan`, `int_start_pixel`,
/// `int_end_pixel`, or `pixels_per_line` for `CONFIG_128K`.
#[test]
fn int_asserts_at_canonical_t_state_128k() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());

    // After 226 176 half-cycles the ULA has ticked 113 088 times
    // (one tick per even-`hc` iteration). 113 088 = 248 × 456, so the
    // counter sits at (scan = 248, pixel = 0) — one ULA tick shy of
    // int_start_pixel = 1.
    runtime.machine_mut().advance_halfcycles(226_176);
    assert!(
        !runtime.machine().z80.irq,
        "INT must not be asserted before scan 248, pixel 1 on 128K"
    );

    // +2 half-cycles fires one more ULA tick (113 089), incrementing
    // the pixel counter to 1 and latching int_active = true. The
    // engine's `feed_irq` runs in the same half-cycle and propagates
    // the flag to `z80.irq`.
    runtime.machine_mut().advance_halfcycles(2);
    assert!(
        runtime.machine().z80.irq,
        "INT must be asserted at scan 248, pixel ≥ 1 on 128K"
    );

    // +130 half-cycles = 65 more ULA ticks, pushing the pixel
    // counter past int_end_pixel = 65.
    runtime.machine_mut().advance_halfcycles(130);
    assert!(
        !runtime.machine().z80.irq,
        "INT must deassert after the 64-pixel active window on 128K"
    );
}

/// **Seam 5 waypoint #1c:** Interrupt asserts at the canonical T-state
/// on the Pentagon.
///
/// The Pentagon is the load-bearing exception in the family: 320 lines
/// (extra VBlank) and `int_scan = 256` instead of 248 — INT fires
/// eight scans later than on a Sinclair 128K. 448 pixels / line = 224
/// T-states / line, so the canonical INT T-state is 256 × 224 = 57 344
/// from frame start. Catches regression in `CONFIG_PENTAGON.int_scan`
/// — easy to break by copying from `CONFIG_128K` and forgetting the
/// Pentagon-specific override.
#[test]
fn int_asserts_at_canonical_t_state_pentagon() {
    let mut runtime = Pentagon128Runtime::new(Model::Pentagon128, Pentagon128::new());

    runtime.machine_mut().advance_tstates(256 * 224 - 1);
    assert!(
        !runtime.machine().z80.irq,
        "INT must not be asserted before scan 256 on Pentagon"
    );

    runtime.machine_mut().advance_tstates(2);
    assert!(
        runtime.machine().z80.irq,
        "INT must be asserted at scan 256, pixel ≥ 1 on Pentagon"
    );

    runtime.machine_mut().advance_tstates(40);
    assert!(
        !runtime.machine().z80.irq,
        "INT must deassert after the 32-T-state active window on Pentagon"
    );
}

/// **Seam 5 waypoint #2:** First display fetch lands on the data bus
/// at T-state 14338 (48K), the canonical Float48K sample point.
///
/// This is the Seam 1 fix made testable. Pre-Seam-1 the first fetch
/// was at T-14342 (4 T-states late); post-Seam-1 it's at T-14338.
/// The Float48K probe is the gold-standard third-party verifier and
/// is now un-gated (no env-var) in
/// `crates/machine-sinclair-zx-spectrum-48k/tests/float_bus.rs` —
/// `#[ignore]`'d only because it needs the local 48K ROM and the
/// `Float48k.tap` fixture. Run via
/// `cargo test --release -p machine-sinclair-zx-spectrum-48k \
///   --test float_bus -- --ignored`.
/// This waypoint asserts the same invariant via the engine's own
/// `MEM_TABLE` and `fetch_start` constants — a hermetic structural
/// check that runs on every `cargo test` and doesn't depend on
/// driving real BASIC code through the probe.
///
/// Catches regression: any reshuffle of `MEM_TABLE` /
/// `IDLE_TABLE` / `fetch_start` that re-introduces the
/// pre-Seam-1 +4 T-state offset.
#[test]
fn first_display_fetch_phase_matches_seam_1_landed_state() {
    use common_sinclair_zx_spectrum::ula_engine::{CONFIG_48K, MEM_TABLE};

    // Seam 1 landed `fetch_start: 4` — first VRAM fetch happens at
    // pixel 4 of scan 0, which is T-state 14338 from frame INT (the
    // canonical Float48K sample point per Smith Chapter 21 p. 227).
    assert_eq!(
        CONFIG_48K.fetch_start, 4,
        "Seam 1: CONFIG_48K.fetch_start must be 4 (pre-Seam-1 was 8); \
         see knowledge/decisions/spectrum-architecture-review.md"
    );
    assert_eq!(
        CONFIG_48K.fetch_end, 260,
        "Seam 1: CONFIG_48K.fetch_end must be 260 (pre-Seam-1 was 264)"
    );

    // MEM_TABLE: fetches at phases 4, 6, 8, 10 (false = fetch active).
    let fetch_phases: Vec<usize> = (0..16).filter(|&i| !MEM_TABLE[i]).collect();
    assert_eq!(
        fetch_phases,
        vec![4, 6, 8, 10],
        "Seam 1: MEM_TABLE fetches must align at phases 4/6/8/10 — \
         pre-Seam-1 was 8/10/12/14"
    );
}

/// **Seam 5 waypoint #3:** Floating bus floats outside the active
/// fetch window.
///
/// `IN A,($FF)` returns the most-recently-latched display byte
/// during the ULA's active fetch period and `0xFF` (idle / pull-ups)
/// outside it. Catches regression: any reshuffle that lets the
/// floating bus leak fetched data into the border / vblank window.
#[test]
fn floating_bus_idles_outside_active_fetch_window() -> Result<(), Box<dyn Error>> {
    let mut runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])?;

    // Run to a point firmly in vertical blank (scan ≥ 312 wraps to
    // 0, so use a small T-state count just past frame start). At
    // scan 0, pixel 0 the ULA is idle for the prefetch slots.
    runtime.machine_mut().advance_tstates(10);
    let floating = runtime.machine().ula().floating_bus();
    assert_eq!(
        floating, 0xFF,
        "floating bus must idle (0xFF) outside active fetch — \
         got {floating:#04x}",
    );
    Ok(())
}

/// **Seam 5 waypoint #4:** Kempston attaches on first gamepad event.
///
/// Until the user touches the gamepad, `KempstonJoystick::attached`
/// is false and the peripheral declines port `$1F` — matching real
/// hardware where a disconnected interface reads floating bus.
/// On the first `InputEvent::Button { port: 0, … }` or
/// `InputEvent::Axis { port: 0, … }` the runtime flips
/// `attached = true` so software probing `$1F` for Kempston
/// detection starts seeing the state byte.
///
/// Catches regression: any change to the runtime input layer or
/// `SpectrumMachine::set_kempston_button` that breaks the
/// "implicit attach on first event" contract. Seam 2 follow-up.
#[test]
fn kempston_attaches_on_first_gamepad_event_48k() -> Result<(), Box<dyn Error>> {
    let mut runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])?;
    // Fresh machine: Kempston unattached, state byte clear.
    assert!(
        !runtime.machine().kempston.attached,
        "Kempston must default to unattached"
    );
    assert_eq!(runtime.machine().kempston.state, 0);

    let press = InputEvent::Button {
        port: 0,
        name: Cow::Borrowed("fire"),
        pressed: true,
    };
    let events = [press];
    let mut host = HostIo {
        input_events: &events,
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    };
    // run_until is the canonical event-drain path — the runtime
    // applies queued input events before stepping the frame.
    runtime.run_until(MachineTime::new(2_000), &mut host)?;

    assert!(
        runtime.machine().kempston.attached,
        "first Kempston event must attach the interface"
    );
    assert_eq!(
        runtime.machine().kempston.state,
        0b0001_0000,
        "fire bit (bit 4) must be set after the press event"
    );
    Ok(())
}

/// **Seam 5 waypoint #5:** Snapshot version is locked at v2.
///
/// The runtime envelope was bumped 1 → 2 in Seam 3 to carry the
/// disk-image cache. Any further breaking change to the envelope
/// must update [`SNAPSHOT_VERSION`] and document the upgrade path
/// in `crates/runtime-sinclair-zx-spectrum/src/snapshot.rs`. This
/// waypoint locks the current version so a drive-by bump fails the
/// test and forces the author to justify the change.
///
/// Catches regression: silent envelope drift that would break
/// previously-saved snapshots.
#[test]
fn snapshot_envelope_version_is_v2() -> Result<(), Box<dyn Error>> {
    // The envelope embeds the version as a varint at byte offset 0
    // (postcard encodes the leading u32 directly with no length
    // prefix). For small u32s the varint occupies exactly 1 byte
    // and matches the value, so byte 0 of the snapshot bytes == 2.
    let runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])?;
    let bytes = runtime.snapshot()?;
    assert!(!bytes.is_empty(), "snapshot must produce non-empty bytes");
    assert_eq!(
        bytes[0], 2,
        "snapshot envelope must be at version 2 (Seam 3 disk-image cache); \
         see knowledge/decisions/spectrum-architecture-review.md Seam 3"
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// 128K-family waypoints
// ─────────────────────────────────────────────────────────────────────
// The 128K (Sinclair 7K010E ULA) and the +3 (Amstrad 40077 gate
// array) share a paging-lock invariant the 48K class does not have:
// writing bit 5 to port $7FFD permanently disables further paging
// writes until the next *hard* reset. A soft reset preserves the
// lock — that's how 48K BASIC stays mapped after the 128K's autoboot
// drops to 48 BASIC mode and then RAMTOP/reset cycles.

/// **Seam 5 waypoint #6:** Paging lock persists across soft reset
/// on the 128K.
///
/// Set bit 5 of port `$7FFD`, soft-reset the machine, verify the
/// lock survives. Catches regressions in the 128K paging-lock
/// state machine — `Memory128K::write_7ffd` returns early when
/// `locked` is true, and `SpectrumMachineCore::reset_machine`
/// must not clear that flag.
#[test]
fn paging_lock_persists_across_soft_reset_128k() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    assert!(
        !runtime.machine().memory.is_paging_locked(),
        "fresh 128K must not have paging locked"
    );

    // Write bit 5 (paging disable) via the memory's $7FFD port path.
    runtime.machine_mut().memory.write_7ffd(0x20);
    assert!(
        runtime.machine().memory.is_paging_locked(),
        "writing bit 5 to $7FFD must lock paging"
    );

    runtime.machine_mut().reset();
    assert!(
        runtime.machine().memory.is_paging_locked(),
        "soft reset must preserve the paging lock"
    );

    // Subsequent write must be a no-op — the paging byte stays where
    // it was when the lock latched ($20). Try to clear it.
    runtime.machine_mut().memory.write_7ffd(0x00);
    assert!(
        runtime.machine().memory.is_paging_locked(),
        "lock must remain set across subsequent attempts"
    );
}

/// **Seam 5 waypoint #7:** Paging lock persists across soft reset on
/// the +3 (Amstrad-class).
///
/// Mirrors waypoint #6 against the Amstrad-class paging path. The
/// +3 has both `$7FFD` (128K-style) and `$1FFD` (Amstrad-specific)
/// paging ports; the lock affects both. Catches regressions in
/// `MemoryPlus::write_7ffd` / `write_1ffd` or
/// `SpectrumAmstradClassCore::reset`.
#[test]
fn paging_lock_persists_across_soft_reset_plus3() {
    let mut runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());
    assert!(
        !runtime.machine().memory.is_paging_locked(),
        "fresh +3 must not have paging locked"
    );

    runtime.machine_mut().memory.write_7ffd(0x20);
    assert!(
        runtime.machine().memory.is_paging_locked(),
        "writing bit 5 to $7FFD must lock paging on +3"
    );

    runtime.machine_mut().reset();
    assert!(
        runtime.machine().memory.is_paging_locked(),
        "+3 soft reset must preserve the paging lock"
    );

    // Both port paths must respect the lock.
    runtime.machine_mut().memory.write_7ffd(0x00);
    runtime.machine_mut().memory.write_1ffd(0x00);
    assert!(
        runtime.machine().memory.is_paging_locked(),
        "lock must remain set across both port paths"
    );
}

/// **Seam 5 waypoint #8:** Kempston attaches on first gamepad event
/// on the 128K.
///
/// Same invariant as the 48K equivalent (waypoint #4), exercised
/// through the 128K runtime to prove the override on Spectrum128K's
/// `set_kempston_button` impl is wired correctly.
#[test]
fn kempston_attaches_on_first_gamepad_event_128k() -> Result<(), Box<dyn Error>> {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    assert!(!runtime.machine().kempston.attached);

    let press = InputEvent::Button {
        port: 0,
        name: Cow::Borrowed("fire"),
        pressed: true,
    };
    let events = [press];
    let mut host = HostIo {
        input_events: &events,
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    };
    runtime.run_until(MachineTime::new(2_000), &mut host)?;

    assert!(
        runtime.machine().kempston.attached,
        "128K Kempston must attach on first event"
    );
    assert_eq!(
        runtime.machine().kempston.state,
        0b0001_0000,
        "fire bit must be set"
    );
    Ok(())
}

/// **Seam 5 waypoint #9:** Contention delay tables match the canonical
/// 16-entry pixel masks for 48K-class and Amstrad-class.
///
/// `DELAY_TABLE_48K` and `DELAY_TABLE_PLUS2A` are the source-of-truth
/// pixel masks that the four ULA implementations (Ferranti 6C001E,
/// Sinclair 7K010E, Amstrad 40077, Timex SCLD) all index when deciding
/// whether to withhold the CPU clock at each 16-pixel phase. The
/// canonical T-state delay sequences documented in
/// `knowledge/systems/spectrum/contention.md` — `[6,5,4,3,2,1,0,0]` for
/// 48K/128K and `[1,0,7,6,5,4,3,2]` for +2A/+3 — fall out of these
/// masks once sampled at the two-pixel-per-T-state rate. If either
/// table shifts, the contention window over T=14335..14400 shifts with
/// it and every catalogue end-state hash that depends on the exact
/// fetch/contention timing drifts silently.
///
/// Catches regression: any reshuffle of the contention masks. Pure
/// structural check — no machine stepping required.
#[test]
fn contention_table_matches_canonical_for_known_window() {
    use common_sinclair_zx_spectrum::ula_engine::{DELAY_TABLE_48K, DELAY_TABLE_PLUS2A};

    // 48K/128K: contention active during the 12 fetch-window phases
    // (indices 3-14), idle on the 4 phases that bracket the cycle
    // (0, 1, 2, 15). Produces `[6, 5, 4, 3, 2, 1, 0, 0]` once sampled
    // at one entry per T-state — see contention.md §"48K".
    let expected_48k: [bool; 16] = [
        false, false, false, true, true, true, true, true, true, true, true, true, true, true,
        true, false,
    ];
    assert_eq!(
        DELAY_TABLE_48K, expected_48k,
        "DELAY_TABLE_48K must match the canonical 16-entry mask — \
         see knowledge/systems/spectrum/contention.md and \
         knowledge/decisions/spectrum-architecture-review.md Seam 5"
    );

    // +2A/+3: completely different shape — contention only at the
    // three phases bracketing the cycle (0, 14, 15). Produces
    // `[1, 0, 7, 6, 5, 4, 3, 2]` once sampled per T-state — see
    // contention.md §"+2A / +3".
    let expected_plus2a: [bool; 16] = [
        true, false, false, false, false, false, false, false, false, false, false, false, false,
        false, true, true,
    ];
    assert_eq!(
        DELAY_TABLE_PLUS2A, expected_plus2a,
        "DELAY_TABLE_PLUS2A must match the canonical Amstrad 16-entry mask"
    );
}

/// **Seam 5 waypoint #10:** Amstrad class declines Kempston events.
///
/// The +2A / +2B / +3 broke the rear-connector pinout in 1987 so a
/// real Kempston interface cannot physically attach. The architecture
/// review requires this be enforced — `SpectrumMachine::set_kempston_
/// button` returns `false` from the no-op default on Amstrad-class
/// variants. Catches regression where the override accidentally
/// leaks across class boundaries.
#[test]
fn amstrad_class_declines_kempston_events_plus3() -> Result<(), Box<dyn Error>> {
    let mut runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());

    let press = InputEvent::Button {
        port: 0,
        name: Cow::Borrowed("fire"),
        pressed: true,
    };
    let events = [press];
    let mut host = HostIo {
        input_events: &events,
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    };
    runtime.run_until(MachineTime::new(2_000), &mut host)?;

    // The Amstrad-class SpectrumMachine impl inherits the default
    // no-op `set_kempston_button`. The +3 has no `kempston` field,
    // so we can't assert against it directly — we assert the
    // negative via the return value contract: the default returns
    // false, signalling the event was not applied.
    let accepted = runtime.machine_mut().set_kempston_button(4, true);
    assert!(
        !accepted,
        "+3 must decline Kempston events — \
         see knowledge/decisions/spectrum-architecture-review.md Seam 2"
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// ROM-backed — `#[ignore]`'d; resolve assets under ~/.emu198x/
// ─────────────────────────────────────────────────────────────────────

/// Waypoint: real 48K ROM advances multiple frames without panicking.
/// Catches regression: any change that breaks the Z80 / ULA / contention
/// chain when running real Sinclair ROM code.
#[test]
#[ignore = "requires ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
fn real_48k_rom_runs_thirty_frames() -> Result<(), Box<dyn Error>> {
    let Some(rom_path) = home_rom_48k() else {
        eprintln!("skip: 48.rom missing");
        return Ok(());
    };
    let rom = std::fs::read(&rom_path)?;
    let mut runtime = Spectrum48kRuntime::from_rom_bytes(&rom)?;
    let mut host = null_host();
    // 30 frames at 69_888 t-states/frame, half-cycle units.
    let target = MachineTime::new(30 * 2 * 69_888);
    runtime.run_until(target, &mut host)?;
    assert!(runtime.time().get() >= target.get() / 2);
    Ok(())
}
