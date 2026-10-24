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
    Model, Pentagon128Runtime, Spectrum48kRuntime, Spectrum128kRuntime, SpectrumMachine,
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

// `EMU198X_SPECTRUM_48K_ROM` first, so CI can provision one copy and
// point every Spectrum test at it; the home directory is the developer
// fallback. Matches `z80test.rs` and `float_bus_oracle.rs`.
fn home_rom_48k() -> Option<PathBuf> {
    let path = match std::env::var_os("EMU198X_SPECTRUM_48K_ROM") {
        Some(path) => PathBuf::from(path),
        None => {
            let home = std::env::var("HOME").ok()?;
            PathBuf::from(home).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom")
        }
    };
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
    runtime.machine_mut().advance_tstates(248 * 224 - 1);
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
/// master-clock position on the 128K.
///
/// The Sinclair 128K shares `int_scan = 248` with the 48K but runs at
/// the 17.7 MHz master clock (`cpu_divisor = 5`, not 4). The shared
/// scheduler therefore emits two ULA/Z80 half-cycle edges at phases
/// 0 and 2 of each five-tick T-state. Pinning the interrupt in master-
/// clock units verifies both the ULA geometry and that divide-by-five
/// cadence.
///
/// Catches regression: any drift in `int_scan`, `int_start_pixel`,
/// `int_end_pixel`, or `pixels_per_line` for `CONFIG_128K`.
#[test]
fn int_asserts_at_canonical_t_state_128k() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());

    // Counter pixel 5 is evaluated before advance, on master tick 282733.
    // This pins the physical IRQ edge independently of FUSE read timestamps.
    runtime.machine_mut().advance_halfcycles(282_732);
    assert!(!runtime.machine().z80.irq);
    runtime.machine_mut().advance_halfcycles(1);
    assert!(runtime.machine().z80.irq);

    // Sixty-four further ULA edges reach pixel 69. The Sinclair 128K
    // pulse is 36 T-states wide, so it remains asserted here; this is
    // where the Amstrad-class 32-T-state pulse deasserts.
    runtime.machine_mut().advance_halfcycles(160);
    assert!(
        runtime.machine().z80.irq,
        "Sinclair 128K INT must remain active for 36 T-states"
    );

    // Eight more ULA edges reach int_end_pixel = 77. At two edges per
    // five master ticks, those final four T-states consume 20 ticks.
    runtime.machine_mut().advance_halfcycles(20);
    assert!(
        !runtime.machine().z80.irq,
        "Sinclair 128K INT must deassert after its 36-T-state window"
    );
}

/// **Seam 5 waypoint #1c:** The Amstrad ASIC retains its shorter
/// 32-T-state interrupt pulse.
#[test]
fn int_pulse_is_32_tstates_on_amstrad_class() {
    let mut runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());

    runtime.machine_mut().advance_halfcycles(282_720);
    assert!(!runtime.machine().z80.irq);
    runtime.machine_mut().advance_halfcycles(1);
    assert!(runtime.machine().z80.irq);
    runtime.machine_mut().advance_halfcycles(160);
    assert!(
        !runtime.machine().z80.irq,
        "Amstrad-class INT must deassert after its 32-T-state window"
    );
}

/// **Seam 5 waypoint #1d:** Interrupt asserts at the canonical T-state
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

/// Smith's C3-gated fetches and the independent SpecIde memory table
/// place Sinclair VRAM accesses at counter phases 8/10/12/14.
/// Hardware tape probes separately validate the instruction-visible timing.
#[test]
fn display_fetches_match_the_physical_counter() {
    use common_sinclair_zx_spectrum::ula_engine::{CONFIG_48K, MEM_TABLE};
    assert_eq!(CONFIG_48K.fetch_start, 8);
    assert_eq!(CONFIG_48K.fetch_end, 264);
    let fetch_phases: Vec<usize> = (0..16).filter(|&i| !MEM_TABLE[i]).collect();
    assert_eq!(fetch_phases, vec![8, 10, 12, 14]);
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

/// An Interface 1 takes Microdrive cartridges in the `microdrive-N`
/// slots, gives them back as `.mdr` bytes, and keeps them in a snapshot.
/// The ROM is a synthetic stand-in.
#[test]
fn microdrive_cartridges_load_export_and_survive_a_snapshot_48k() -> Result<(), Box<dyn Error>> {
    use emu198x_shell::{MediaImage, MediaKind, MediaSet};

    let mut mdr: Vec<u8> = (0..10 * 543).map(|i| i as u8).collect();
    mdr.push(1);
    let load = |runtime: &mut Spectrum48kRuntime, slot: &'static str| {
        let mut media = MediaSet::new();
        media.push(MediaImage::new(slot, MediaKind::Cartridge, &mdr));
        runtime.load_media(&media)
    };

    let mut runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])?;
    assert!(
        load(&mut runtime, "microdrive-1").is_err(),
        "no Interface 1, no Microdrives"
    );
    runtime.attach_interface1(&[0; 0x2000])?;
    load(&mut runtime, "microdrive-1")?;
    assert!(load(&mut runtime, "microdrive-9").is_err());
    assert_eq!(runtime.export_media("microdrive-1", "mdr")?, mdr);
    assert!(runtime.export_media("microdrive-2", "mdr").is_err());

    let mut restored = Spectrum48kRuntime::blank();
    restored.restore(&runtime.snapshot()?)?;
    assert_eq!(restored.export_media("microdrive-1", "mdr")?, mdr);

    runtime.eject_media("microdrive-1")?;
    assert!(runtime.export_media("microdrive-1", "mdr").is_err());
    Ok(())
}

/// **Seam 5 waypoint #5:** Snapshot version is locked at v14.
///
/// The runtime envelope was bumped 1 → 2 in Seam 3 to carry the
/// disk-image cache, 2 → 3 when accepted Z80 interrupt responses
/// gained a serialisable sequence identity, 3 → 4 for tape stop-release
/// state, 4 → 5 for live writable disk media (#1764), and 5 → 6 for
/// per-sector FDC read progress (#1833, explicitly approved). Any further breaking
/// change to the envelope must update [`SNAPSHOT_VERSION`] and
/// document the upgrade path in
/// `crates/runtime-sinclair-zx-spectrum/src/snapshot.rs`. This
/// waypoint locks the current version so a drive-by bump fails.
///
/// Catches regression: silent envelope drift that would break
/// previously-saved snapshots.
#[test]
fn snapshot_envelope_version_is_v14() -> Result<(), Box<dyn Error>> {
    // The envelope embeds the version as a varint at byte offset 0
    // (postcard encodes the leading u32 directly with no length
    // prefix). For small u32s the varint occupies exactly 1 byte
    // and matches the value, so byte 0 of the snapshot bytes == 14.
    let runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])?;
    let bytes = runtime.snapshot()?;
    assert!(!bytes.is_empty(), "snapshot must produce non-empty bytes");
    assert_eq!(
        bytes[0], 14,
        "snapshot envelope must be at version 14 (the Interface 1); \
         see crates/runtime-sinclair-zx-spectrum/src/snapshot.rs"
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// 128K-family waypoints
// ─────────────────────────────────────────────────────────────────────
/// Reset must clear the 128K paging latch, retain all RAM/ROM, and allow
/// every bank to be selected again. Both script reset kinds reach this path.
/// Sinclair 128 Service Manual §1.3; Fuse `spec128_common_reset`.
#[test]
fn reset_unlocks_128k_paging_and_preserves_memory() {
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use emu198x_shell::{HeadlessSession, ResetKind, ScriptStep};

    for kind in [ResetKind::Soft, ResetKind::Hard] {
        let mut machine = Spectrum128K::new();
        machine.memory.load_roms(&[0xC3], &[0x76]);
        for bank in 0..8_u8 {
            machine.memory.write_7ffd(bank);
            machine.memory.write(0xFB00, 0xA0 + bank);
        }
        machine.memory.write_7ffd(0x3B); // ROM 1, shadow screen, bank 3, locked.
        machine.memory.write_7ffd(0x00);
        assert!(machine.memory.is_paging_locked());
        assert_eq!(machine.memory.read(0xFB00), 0xA3);
        assert_eq!(machine.memory.read(0), 0x76);
        machine.z80.regs.pc = 0x8123;
        let runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, machine);
        let mut session = HeadlessSession::new(runtime, 283_632);
        ScriptStep::Reset { kind }
            .execute_collect(&mut session)
            .expect("script reset succeeds");
        let machine = session.machine_mut().machine_mut();
        assert_eq!(machine.z80.regs.pc, 0);
        assert!(
            !machine.memory.is_paging_locked(),
            "{kind:?} unlocks paging"
        );
        assert_eq!(machine.memory.current_bank(), 0);
        assert_eq!(machine.memory.current_rom(), 0);
        assert_eq!(machine.memory.screen_bank(), 5);
        assert_eq!(machine.memory.read(0), 0xC3);
        for bank in 0..8_u8 {
            machine.memory.write_7ffd(0x10 | bank);
            assert_eq!(machine.memory.read(0xFB00), 0xA0 + bank);
            assert_eq!(machine.memory.read(0), 0x76);
        }
    }
}

/// Reset exits locked normal and all-RAM mappings, retains every bank, and
/// allows both paging ports to operate again. Amstrad +3 manual pp. 189–191;
/// Fuse 1.7.0 `specplus3_plus2a_common_reset`.
#[test]
fn reset_unlocks_plus3_paging_and_preserves_memory() {
    use common_sinclair_zx_spectrum::memory::MemoryBus;
    use emu198x_shell::{HeadlessSession, ResetKind, ScriptStep};

    for kind in [ResetKind::Soft, ResetKind::Hard] {
        for secondary in [0x04, 0x01, 0x03, 0x05, 0x07] {
            let mut machine = SpectrumPlus3::new();
            machine.memory.load_roms(&[0xC0], &[0xC1], &[0xC2], &[0xC3]);
            for bank in 0..8_u8 {
                machine.memory.write_7ffd(bank);
                machine.memory.write(0xFB00, 0xA0 + bank);
            }
            machine.memory.write_1ffd(secondary);
            machine.memory.write_7ffd(0x3B); // ROM low bit, shadow screen, bank 3, lock.
            machine.memory.write_7ffd(0);
            machine.memory.write_1ffd(0);
            assert!(machine.memory.is_paging_locked());
            assert_eq!(machine.memory.screen_bank(), 7);
            let expected_bank = if secondary == 3 { 7 } else { 3 };
            assert_eq!(machine.memory.read(0xFB00), 0xA0 + expected_bank);
            machine.z80.regs.pc = 0x8123;
            let runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, machine);
            let mut session = HeadlessSession::new(runtime, 354_540);
            ScriptStep::Reset { kind }
                .execute_collect(&mut session)
                .expect("script reset succeeds");
            let machine = session.machine_mut().machine_mut();
            assert_eq!(machine.z80.regs.pc, 0);
            assert!(
                !machine.memory.is_paging_locked(),
                "{kind:?} unlocks paging"
            );
            assert_eq!(machine.memory.screen_bank(), 5);
            assert_eq!(machine.memory.read(0), 0xC0);
            for (address, bank) in [(0x7B00, 5), (0xBB00, 2), (0xFB00, 0)] {
                assert_eq!(machine.memory.read(address), 0xA0 + bank);
            }
            for bank in 0..8_u8 {
                machine.memory.write_7ffd(bank);
                assert_eq!(machine.memory.read(0xFB00), 0xA0 + bank);
            }
            for rom in 0..4_u8 {
                machine.memory.write_7ffd((rom & 1) << 4);
                machine.memory.write_1ffd((rom & 2) << 1);
                assert_eq!(machine.memory.read(0), 0xC0 + rom);
            }
            machine.memory.write_1ffd(0x03); // Both ports work after reset.
            assert_eq!(machine.memory.read(0x3B00), 0xA4);
            assert_eq!(machine.memory.read(0xBB00), 0xA6);
        }
    }
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

    // Raw counter mask: C2|C3. Wrappers account for the control-edge phase.
    let expected_48k: [bool; 16] = [
        false, false, false, false, true, true, true, true, true, true, true, true, true, true,
        true, true,
    ];
    assert_eq!(
        DELAY_TABLE_48K, expected_48k,
        "DELAY_TABLE_48K must match the canonical 16-entry mask — \
         see knowledge/systems/spectrum/contention.md and \
         knowledge/decisions/spectrum-architecture-review.md Seam 5"
    );

    // +2A/+3: eleven contiguous asserted half-cycles from pixel 4,
    // free over 15 and 0-3. Measured against FUSE by
    // `machine-sinclair-zx-spectrum-plus2a`'s arrival-resolved
    // differential, not derived — see the constant's own comment for
    // the sweep that isolates it and why FUSE's per-T-state
    // `[1, 0, 7, 6, 5, 4, 3, 2]` does not map onto it by a simple count.
    let expected_plus2a: [bool; 16] = [
        false, false, false, false, true, true, true, true, true, true, true, true, true, true,
        true, false,
    ];
    assert_eq!(
        DELAY_TABLE_PLUS2A, expected_plus2a,
        "DELAY_TABLE_PLUS2A must match the canonical Amstrad 16-entry mask"
    );
}

/// **Seam 5 waypoint #10a:** Sinclair Interface 2 port 1 closes
/// keyboard-matrix contacts on +3 (Amstrad-class).
///
/// IF2 routing is universal across the family — it's a keyboard-matrix
/// translation, not a peripheral. The +3 has *no* Kempston field (per
/// the 1987 rear-connector pinout change documented in
/// `knowledge/decisions/spectrum-joystick-architecture.md`) so an
/// `InputEvent::Button { port: 0, … }` is silently dropped — see
/// waypoint #10. An `InputEvent::Button { port: 1, … }` is the IF2
/// path, which routes through the keyboard matrix instead and is
/// therefore available on every variant, +3 included.
///
/// Per Grussu's table: port 1 fire closes the `0` key on the matrix
/// (row 4, bit 0 — `keyboard_rows()[4] & 0x01 == 0` when pressed).
///
/// Catches regression: any change to `apply_input_event`'s IF2 routing,
/// `if2_button_to_key`, or the SpectrumMachine trait's keyboard cache
/// that breaks the "joystick event = keyboard row update" contract.
#[test]
fn if2_port1_fire_closes_keyboard_zero_on_plus3() -> Result<(), Box<dyn Error>> {
    let mut runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());

    // Fresh machine: every keyboard row reads 0xFF (all keys released).
    assert_eq!(
        runtime.machine().keyboard_rows(),
        &[0xFF; 8],
        "fresh +3 keyboard matrix must read all-released"
    );

    let press = InputEvent::Button {
        port: 1,
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

    // `0` lives at (row 4, bit 0). Bit clear = key pressed (active low).
    let rows = runtime.machine().keyboard_rows();
    assert_eq!(
        rows[4] & 0x01,
        0,
        "IF2 port-1 fire must close the `0` key — row 4 bit 0 must be 0, got row=0x{:02X}",
        rows[4]
    );
    // No other row should be touched.
    for (i, byte) in rows.iter().enumerate() {
        let expected = if i == 4 { 0xFE } else { 0xFF };
        assert_eq!(
            *byte, expected,
            "row {i}: only row 4 bit 0 should be cleared, got 0x{byte:02X}",
        );
    }
    Ok(())
}

/// **Seam 5 waypoint #10b:** Sinclair Interface 2 port 2 closes the
/// row-3 keys on the 48K, and does not touch Kempston state.
///
/// IF2 port 2 fire closes the `5` key (row 3, bit 4). Same routing
/// path as port 1 but different keys, against a 48K runtime to prove
/// the routing isn't accidentally Amstrad-class-specific. Also
/// asserts the Kempston peripheral was not attached as a side-effect
/// — IF2 events route through the keyboard matrix only.
#[test]
fn if2_port2_fire_closes_keyboard_five_on_48k() -> Result<(), Box<dyn Error>> {
    let mut runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])?;

    let press = InputEvent::Button {
        port: 2,
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

    // `5` lives at (row 3, bit 4).
    let rows = runtime.machine().keyboard_rows();
    assert_eq!(
        rows[3] & 0x10,
        0,
        "IF2 port-2 fire must close the `5` key — row 3 bit 4 must be 0, got row=0x{:02X}",
        rows[3]
    );
    // Also: Kempston must NOT have attached — IF2 events don't touch port-0 state.
    assert!(
        !runtime.machine().kempston.attached,
        "IF2 events on port 2 must not flip Kempston attached flag"
    );
    Ok(())
}

/// **Seam 5 waypoint #11:** Amstrad class declines Kempston events.
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
#[ignore = "FIXTURE: requires ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
fn real_48k_rom_runs_thirty_frames() -> Result<(), Box<dyn Error>> {
    let Some(rom_path) = home_rom_48k() else {
        emu198x_test_skip::record("skip: 48.rom missing");
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
