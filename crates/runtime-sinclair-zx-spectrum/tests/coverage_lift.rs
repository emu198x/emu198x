//! Coverage: the per-variant runtime constructors (`blank` /
//! `from_rom_bytes` / `from_firmware`) and the family-enum debug +
//! dispatch surface.
//!
//! The behavioural variant tests build runtimes only through the generic
//! `SpectrumRuntime::new(model, machine)` path, so the bespoke firmware
//! constructors on the exotic clones (Pentagon, Scorpion, Timex) and the
//! hand-written `DebugPrimitives` impl on the family enum were never
//! exercised. These tests drive each construction path (success +
//! wrong-size + missing-firmware) and the debug verbs, holding the
//! Spectrum coverage gate (SOLID criterion 11) above its floor.

use common_sinclair_zx_spectrum::audio::SpeakerChannel;
use emu198x_shell::{DebugPrimitives, FirmwareImage, FirmwareSet};
use machine_pentagon_128::Pentagon128;
use machine_scorpion_zs256::ScorpionZS256;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus2a::SpectrumPlus2A;
use machine_timex_tc2048::TimexTC2048;
use machine_timex_ts2068::{TimexModel, TimexTS2068};
use runtime_sinclair_zx_spectrum::{
    Model, Pentagon128Runtime, ScorpionZS256Runtime, Spectrum48kRuntime, Spectrum128kRuntime,
    SpectrumMachine, SpectrumPlus2ARuntime, SpectrumRuntimeKind, TimexTC2048Runtime,
    TimexTS2068Runtime,
};

const ROM16: usize = 16 * 1024;
const ROM8: usize = 8 * 1024;

// ── Pentagon 128 (two 16 KiB ROMs) ──────────────────────────────────

#[test]
fn pentagon_constructors_cover_every_path() {
    let rom = vec![0u8; ROM16];

    // blank() → new_pentagon([0;16K], [0;16K])
    let _ = Pentagon128Runtime::blank();

    // from_rom_bytes: Ok with correctly-sized slices, Err on wrong size.
    assert!(Pentagon128Runtime::from_rom_bytes(&rom, &rom).is_ok());
    assert!(Pentagon128Runtime::from_rom_bytes(&rom[..10], &rom).is_err());
    assert!(Pentagon128Runtime::from_rom_bytes(&rom, &rom[..10]).is_err());

    // from_firmware: missing → Err; complete (zeroed) → Ok.
    assert!(Pentagon128Runtime::from_firmware(&FirmwareSet::new()).is_err());
    let mut fw = FirmwareSet::new();
    fw.push(FirmwareImage::new("pentagon-rom-0", &rom));
    fw.push(FirmwareImage::new("pentagon-rom-1", &rom));
    assert!(Pentagon128Runtime::from_firmware(&fw).is_ok());
}

// ── Scorpion ZS-256 (four 16 KiB ROMs) ──────────────────────────────

#[test]
fn scorpion_constructors_cover_every_path() {
    let rom = vec![0u8; ROM16];

    let _ = ScorpionZS256Runtime::blank();

    assert!(ScorpionZS256Runtime::from_rom_bytes([&rom, &rom, &rom, &rom]).is_ok());
    assert!(ScorpionZS256Runtime::from_rom_bytes([&rom[..10], &rom, &rom, &rom]).is_err());

    assert!(ScorpionZS256Runtime::from_firmware(&FirmwareSet::new()).is_err());
    let mut fw = FirmwareSet::new();
    for id in [
        "scorpion-rom-0",
        "scorpion-rom-1",
        "scorpion-rom-2",
        "scorpion-rom-3",
    ] {
        fw.push(FirmwareImage::new(id, &rom));
    }
    assert!(ScorpionZS256Runtime::from_firmware(&fw).is_ok());
}

// ── Timex TC2048 (one 16 KiB ROM) ───────────────────────────────────

#[test]
fn timex_tc2048_constructors_cover_every_path() {
    let rom = vec![0u8; ROM16];

    let _ = TimexTC2048Runtime::blank();

    assert!(TimexTC2048Runtime::from_rom_bytes(&rom).is_ok());
    assert!(TimexTC2048Runtime::from_rom_bytes(&rom[..10]).is_err());

    assert!(TimexTC2048Runtime::from_firmware(&FirmwareSet::new()).is_err());
    let mut fw = FirmwareSet::new();
    fw.push(FirmwareImage::new("timex-tc2048-rom", &rom));
    assert!(TimexTC2048Runtime::from_firmware(&fw).is_ok());
}

// ── Timex TS2068 / TC2068 (16 KiB ROM + 8 KiB EXROM) ─────────────────

#[test]
fn timex_ts2068_constructors_cover_every_path() {
    let rom = vec![0u8; ROM16];
    let exrom = vec![0u8; ROM8];

    let _ = TimexTS2068Runtime::blank(Model::TimexTS2068);
    let _ = TimexTS2068Runtime::blank(Model::TimexTC2068);

    assert!(TimexTS2068Runtime::from_rom_bytes(Model::TimexTS2068, &rom, &exrom).is_ok());
    assert!(TimexTS2068Runtime::from_rom_bytes(Model::TimexTS2068, &rom[..10], &exrom).is_err());
    assert!(TimexTS2068Runtime::from_rom_bytes(Model::TimexTS2068, &rom, &exrom[..10]).is_err());

    assert!(TimexTS2068Runtime::from_firmware(Model::TimexTS2068, &FirmwareSet::new()).is_err());
    let mut fw = FirmwareSet::new();
    fw.push(FirmwareImage::new("timex-ts2068-rom-0", &rom));
    fw.push(FirmwareImage::new("timex-ts2068-rom-1", &exrom));
    assert!(TimexTS2068Runtime::from_firmware(Model::TimexTS2068, &fw).is_ok());
}

// ── Family enum: frame_halfcycles arms, as_48k_mut, debug surface ───

/// One representative `SpectrumRuntimeKind` per master-clock timing group
/// plus the exotic clones, so every `frame_halfcycles` match arm runs.
fn one_kind_per_timing_group() -> Vec<SpectrumRuntimeKind> {
    vec![
        SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::new(
            Model::Spectrum48KPal,
            Spectrum48k::new(),
        )),
        SpectrumRuntimeKind::Spectrum128K(Spectrum128kRuntime::new(
            Model::Spectrum128KPal,
            Spectrum128K::new(),
        )),
        SpectrumRuntimeKind::SpectrumPlus2A(SpectrumPlus2ARuntime::new(
            Model::SpectrumPlus2A,
            SpectrumPlus2A::new(),
        )),
        SpectrumRuntimeKind::Pentagon128(Pentagon128Runtime::blank()),
        SpectrumRuntimeKind::ScorpionZS256(ScorpionZS256Runtime::blank()),
        SpectrumRuntimeKind::TimexTC2048(TimexTC2048Runtime::blank()),
        SpectrumRuntimeKind::TimexTC2068(TimexTS2068Runtime::blank(Model::TimexTC2068)),
        SpectrumRuntimeKind::TimexTS2068(TimexTS2068Runtime::blank(Model::TimexTS2068)),
    ]
}

#[test]
fn family_kind_reports_a_nonzero_frame_length_for_every_variant() {
    for kind in one_kind_per_timing_group() {
        assert!(
            kind.frame_halfcycles() > 0,
            "every variant must report a positive frame length"
        );
    }
}

#[test]
fn as_48k_mut_is_some_only_for_the_48k_variant() {
    let mut forty_eight = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::new(
        Model::Spectrum48KPal,
        Spectrum48k::new(),
    ));
    assert!(forty_eight.as_48k_mut().is_some());

    let mut pentagon = SpectrumRuntimeKind::Pentagon128(Pentagon128Runtime::blank());
    assert!(pentagon.as_48k_mut().is_none());
}

/// Build a `SpectrumRuntimeKind` from `(id, bytes)` firmware pairs and
/// assert it succeeds. Validation is structural (IDs only), so zeroed
/// ROMs of the right size build any variant.
fn assert_builds(model: Model, reqs: &[(&'static str, &[u8])]) {
    let mut fw = FirmwareSet::new();
    for (id, bytes) in reqs {
        fw.push(FirmwareImage::new(*id, bytes));
    }
    assert!(
        SpectrumRuntimeKind::from_firmware(model, &fw).is_ok(),
        "from_firmware should build {model:?} from zeroed ROMs"
    );
}

#[test]
fn from_firmware_builds_every_model_from_zeroed_roms() {
    // Drives the whole `from_firmware` dispatch match plus each variant's
    // firmware constructor. All ROMs are 16 KiB except the TS2068/TC2068
    // EXROM (8 KiB).
    let rom16 = vec![0u8; ROM16];
    let rom8 = vec![0u8; ROM8];
    let r16: &[u8] = &rom16;
    let r8: &[u8] = &rom8;

    assert_builds(
        Model::Spectrum16KPal,
        &[("sinclair-zx-spectrum-48k-rom", r16)],
    );
    assert_builds(
        Model::Spectrum48KPal,
        &[("sinclair-zx-spectrum-48k-rom", r16)],
    );
    assert_builds(
        Model::SpectrumPlus,
        &[("sinclair-zx-spectrum-48k-rom", r16)],
    );
    assert_builds(
        Model::Spectrum128KPal,
        &[
            ("sinclair-zx-spectrum-128k-rom-0", r16),
            ("sinclair-zx-spectrum-128k-rom-1", r16),
        ],
    );
    assert_builds(
        Model::SpectrumPlus2,
        &[
            ("sinclair-zx-spectrum-plus2-rom-0", r16),
            ("sinclair-zx-spectrum-plus2-rom-1", r16),
        ],
    );
    let plus3: &[(&str, &[u8])] = &[
        ("sinclair-zx-spectrum-plus3-rom-0", r16),
        ("sinclair-zx-spectrum-plus3-rom-1", r16),
        ("sinclair-zx-spectrum-plus3-rom-2", r16),
        ("sinclair-zx-spectrum-plus3-rom-3", r16),
    ];
    assert_builds(Model::SpectrumPlus2A, plus3);
    assert_builds(Model::SpectrumPlus2B, plus3);
    assert_builds(Model::SpectrumPlus3, plus3);
    assert_builds(
        Model::Pentagon128,
        &[("pentagon-rom-0", r16), ("pentagon-rom-1", r16)],
    );
    assert_builds(
        Model::ScorpionZS256,
        &[
            ("scorpion-rom-0", r16),
            ("scorpion-rom-1", r16),
            ("scorpion-rom-2", r16),
            ("scorpion-rom-3", r16),
        ],
    );
    assert_builds(Model::TimexTC2048, &[("timex-tc2048-rom", r16)]);
    assert_builds(
        Model::TimexTC2068,
        &[("timex-ts2068-rom-0", r16), ("timex-ts2068-rom-1", r8)],
    );
    assert_builds(
        Model::TimexTS2068,
        &[("timex-ts2068-rom-0", r16), ("timex-ts2068-rom-1", r8)],
    );
}

#[test]
fn family_kind_debug_surface_round_trips() {
    let mut kind = SpectrumRuntimeKind::Spectrum48K(Spectrum48kRuntime::new(
        Model::Spectrum48KPal,
        Spectrum48k::new(),
    ));

    // PC + CPU-state JSON read back cleanly.
    let _pc = kind.dbg_pc();
    let state = kind.dbg_cpu_state();
    assert!(state.get("pc").is_some(), "cpu state carries pc: {state}");

    // Poke a RAM byte and read it back through the debug peek path.
    kind.dbg_poke(0x4000, 0xA5);
    assert_eq!(kind.dbg_peek(0x4000), 0xA5);

    // Disassemble + single-step both run.
    assert!(kind.dbg_disassemble(0x4000).is_some());
    let _ = kind.dbg_step();
}

// ── SpectrumMachine surface on the exotic clones ────────────────────

/// Drive the audio / tape / memory-watch verbs of the `SpectrumMachine`
/// impl. The behavioural tests only ran a frame + snapshot, leaving the
/// bulk of each `impl SpectrumMachine for <clone>` block in variants.rs
/// uncovered.
fn exercise_machine_surface<M: SpectrumMachine>(m: &mut M) {
    SpectrumMachine::run_frame(m);
    let _ = m.framebuffer();
    let _ = m.audio_frame();
    let controls = m.audio_controls();
    m.set_audio_controls(controls);
    m.set_audio_channel_enabled(SpeakerChannel::Speaker, false);
    m.set_audio_channel_enabled(SpeakerChannel::Speaker, true);
    m.set_audio_channel_gain(SpeakerChannel::Speaker, 0.5);
    m.set_keyboard_rows(&[0xFF; 8]);
    let _ = m.set_kempston_button(0, true);
    let _ = m.keyboard_rows();
    m.load_tape_blocks(Vec::new());
    m.tape_play();
    let _ = m.tape_is_playing();
    let _ = m.tape_is_loaded();
    m.tape_stop();
    let _ = m.recorded_tape_blocks();
    m.clear_tape_recording();
    let _ = m.read_byte(0x4000);
    m.write_byte(0x4000, 0x5A);
    let _ = m.half_cycle_in_frame();
    let _ = m.tstate_in_frame();
    let _ = m.start_memory_write_watch(0x4000, 16);
    let _ = m.memory_write_watch_range();
    let _ = m.memory_write_watch_records();
    m.clear_memory_write_watch_records();
    m.stop_memory_write_watch();
    m.reset_machine();
    m.after_restore();
}

#[test]
fn spectrum_machine_surface_runs_for_the_exotic_clones() {
    exercise_machine_surface(&mut Pentagon128::new());
    exercise_machine_surface(&mut ScorpionZS256::new());
    exercise_machine_surface(&mut TimexTC2048::new());
    exercise_machine_surface(&mut TimexTS2068::new(TimexModel::TS2068));
}
