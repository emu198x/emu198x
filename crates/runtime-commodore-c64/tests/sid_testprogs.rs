//! VICE SID test programs — behavioural oracles for the SID register surface.
//!
//! The programs come from VICE's `testprogs/SID/` tree (external,
//! env-gated, staged under `~/.emu198x/test-suites/c64-sid/` or
//! `EMU198X_C64_SID_TESTPROGS_DIR`). Each one boots, pokes the SID, and leaves
//! its readings in screen RAM and, mostly, its verdict in the border colour:
//! light green (5) for pass, red (2) for fail. Their expected values were
//! measured on real 6581 and 8580 chips; see each program's `readme.txt`.
//!
//! Staging: the subset in use, fetched from
//! `https://svn.code.sf.net/p/vice-emu/code/testprogs/SID/` at revision
//! 46281, is `bitfade`, `busvalue`, `ringmod`, `osc3-wave0`, `testwave00`
//! and `noiselfsrinit` (#777), plus `noisewriteback`, `wb_testsuite`,
//! `wf12nsr`, `waveforms` and the `noise-reset_*.asm` includes (#769), and
//! `oscinit`, `osc_topbit`, `writedelay` and `detect` (#1606). The staging
//! directory's `SOURCE.txt` records the same.

mod common;

use std::path::PathBuf;

use common::{local_rom_dir, local_rom_firmware};
use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::HeadlessSession;
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_KEY_HOLD_FRAMES, DEFAULT_TYPE_SETTLE_FRAMES,
    Model, type_string,
};

/// Border colour the programs leave on success (light green).
const BORDER_PASS: u8 = 5;

/// The explicitly configured SID testprog directory, or the conventional
/// per-user staging directory when no explicit path is supplied.
fn testprogs_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("EMU198X_C64_SID_TESTPROGS_DIR") {
        let path = PathBuf::from(path);
        return path.exists().then_some(path);
    }
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/test-suites/c64-sid");
    path.exists().then_some(path)
}

fn roms_present() -> bool {
    let dir = local_rom_dir();
    ["kernal.rom", "basic.rom", "chargen.rom"]
        .iter()
        .all(|name| dir.join(name).is_file())
}

/// Boot real ROMs on `model`, load a testprog `.prg` (relative to the testprog
/// dir), RUN it and let it run for `frames` frames.
fn run_testprog(
    rel_prg: &str,
    model: Model,
    frames: u32,
) -> HeadlessSession<C64Runtime, C64SessionQueryProvider> {
    let dir = testprogs_dir().expect("testprog dir checked by caller");
    let prg = std::fs::read(dir.join(rel_prg)).expect("testprog .prg should read");
    // Both PAL models (6569 breadbin, 8565 C64C) run 312 lines of 63 cycles.
    let cycles_per_frame = TIMING_PAL_BREADBIN.cycles_per_frame;

    let firmware = local_rom_firmware();
    let runtime = C64Runtime::from_firmware(model, &firmware)
        .expect("real C64 firmware should construct a runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(cycles_per_frame),
        C64SessionQueryProvider,
    );
    session.run_frames(150).expect("boot should run");

    let load_addr = session
        .machine_mut()
        .load_prg_bytes(&prg)
        .expect("testprog .prg should load");
    let end = load_addr + (prg.len() as u16 - 2);
    {
        let machine = session.machine_mut().machine_mut();
        machine.cpu_write(0x2D, (end & 0xFF) as u8);
        machine.cpu_write(0x2E, (end >> 8) as u8);
    }
    type_string(
        &mut session,
        "RUN\n",
        DEFAULT_KEY_HOLD_FRAMES,
        DEFAULT_TYPE_SETTLE_FRAMES,
    )
    .expect("typing RUN should succeed");
    session.run_frames(frames).expect("testprog should run");
    session
}

fn screen(session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>, offset: u16) -> u8 {
    session.machine_mut().machine_mut().peek(0x0400 + offset)
}

fn border(session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>) -> u8 {
    session.machine_mut().machine_mut().cpu_read(0xD020) & 0x0F
}

fn staged() -> bool {
    roms_present() && testprogs_dir().is_some()
}

/// `oscinit/allinit`: straight after power-up, OSC3 reads `$00` with no
/// waveform, `$FE` for noise (reset leaves the register at `0x7FFFFE`), `$FF`
/// for pulse at PW 0, and `$55`/`$AA` for sawtooth/triangle from the
/// accumulator's power-on `0x555555`. Measured on real chips; the program
/// stores the five readings at `$0400`.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn oscinit_reads_the_power_on_oscillator_and_noise_register() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    for model in [Model::C64PalBreadbin, Model::C64cPal] {
        let mut session = run_testprog("oscinit/allinit.prg", model, 30);
        let reads: Vec<u8> = (0..5).map(|offset| screen(&mut session, offset)).collect();
        assert_eq!(reads, [0x00, 0xFE, 0xFF, 0x55, 0xAA], "OSC3 on {model:?}");
        assert_eq!(border(&mut session), BORDER_PASS, "verdict on {model:?}");
    }
}

/// `osc3-wave0`: pulse with PW $FFF reads OSC3 $00, PW $000 reads $FF (the
/// comparator drives the pulse high once the accumulator reaches PW).
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn osc3_wave0_pulse_width_extremes() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    let mut session = run_testprog("osc3-wave0/osc3-wave0.prg", Model::C64PalBreadbin, 30);
    assert_eq!(screen(&mut session, 0), 0x00, "OSC3 with PW $FFF");
    assert_eq!(screen(&mut session, 1), 0xFF, "OSC3 with PW $000");
}

/// `ringmod`: voices 2 and 3 stopped at zero, voice 3 a ring-modulated
/// triangle. The MSB is substituted with `MSB EOR NOT source-MSB`, so with
/// both MSBs clear the triangle is inverted and OSC3 reads $FF.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn ringmod_inverts_the_triangle_on_a_clear_source_msb() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    for model in [Model::C64PalBreadbin, Model::C64cPal] {
        let mut session = run_testprog("ringmod/ringmodtest.prg", model, 30);
        assert_eq!(screen(&mut session, 0), 0xFF, "OSC3 on {model:?}");
        assert_eq!(border(&mut session), BORDER_PASS, "verdict on {model:?}");
    }
}

/// `busvalue`: write-only and undecoded registers read the last byte on the
/// SID's data bus, refreshed by the OSC3 read just before.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn busvalue_write_only_reads_return_the_bus_value() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    for model in [Model::C64PalBreadbin, Model::C64cPal] {
        let mut session = run_testprog("busvalue/busvalue.prg", model, 30);
        assert_eq!(
            screen(&mut session, 0x18),
            0xA5,
            "write-only read after a write on {model:?}"
        );
        assert_eq!(border(&mut session), BORDER_PASS, "verdict on {model:?}");
    }
}

/// `osc3-wave0`: after deselecting the waveform OSC3 keeps reading $FF from
/// the floating DAC input, then fades to $00. The `-new` build waits longer
/// for the 8580's slower fade.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn osc3_wave0_floating_dac_holds_then_fades() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    for (prg, model, frames) in [
        ("osc3-wave0/osc3-wave0.prg", Model::C64PalBreadbin, 120),
        ("osc3-wave0/osc3-wave0-new.prg", Model::C64cPal, 600),
    ] {
        let mut session = run_testprog(prg, model, frames);
        assert_eq!(
            screen(&mut session, 2),
            0xFF,
            "OSC3 just after wave 0 on {model:?}"
        );
        assert_eq!(border(&mut session), BORDER_PASS, "verdict on {model:?}");
    }
}

/// `osc_topbit`: run sawtooth until the accumulator's MSB rises, switch
/// briefly to sawtooth combined with triangle, pulse or noise (each driving
/// DAC bit 11 low), back to sawtooth, and read OSC3 bit 7. On a real 6581
/// the combination pulls the MSB itself low (`_old` builds expect it clear);
/// the 8580 buffers it (`_new` builds expect it set). With FREQ 1 the MSB
/// takes 2^23 cycles, about 430 frames, to rise.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn osc_topbit_combined_waveforms_pull_the_6581_msb_low() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    let (count, failures) = failing_border_verdicts("osc_topbit", "osc_topbit_test_", 700);
    assert_eq!(count, 6, "osc_topbit programs staged");
    assert!(failures.is_empty(), "osc_topbit failures: {failures:?}");
}

/// `writedelay`: FREQ `$1000`, PW `$003`, TEST, then pulse, and OSC3 read
/// four cycles after the write reads `$FF`. The readme: "Register writes are
/// not delayed one cycle on the 8580, from circuit analysis the control
/// logic looks identical on both chips". With the pulse comparator's
/// one-cycle delay, a delayed write would read `$00`. reSID delays 8580
/// writes only in its non-cycle-exact `SAMPLE_FAST` mode, as a stand-in for
/// the OSC3 delay; reSIDfp never does.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn writedelay_register_writes_are_not_delayed() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    for model in [Model::C64PalBreadbin, Model::C64cPal] {
        let mut session = run_testprog("writedelay/writedelay.prg", model, 30);
        assert_eq!(screen(&mut session, 0), 0xFF, "OSC3 on {model:?}");
        assert_eq!(border(&mut session), BORDER_PASS, "verdict on {model:?}");
    }
}

/// Border colour `noiselfsrinit` leaves on success (light green is 13
/// here, not 5).
const NOISELFSRINIT_PASS: u8 = 13;

/// `noiselfsrinit` (VICE bug #1920): clear the noise register with
/// noise+pulse+sawtooth+triangle under TEST, shift a set pattern in with
/// TEST pulses, then run the oscillator to a phase fixed by cycle-counted
/// code and read noise from OSC3. `simple` prints `7F`; `scan` repeats at
/// every `$1000`th accumulator value and compares the half period with
/// `reference.bin`. The readme gives both as consistent across ten real
/// 8580s. They depend on the TEST-release write-back from all four
/// waveforms into noise alone and on the noise taps; the readings are
/// taken with the oscillator stopped, so the shift timing does not show.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn noiselfsrinit_matches_real_8580s() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    let mut session = run_testprog("noiselfsrinit/simple.prg", Model::C64cPal, 120);
    let printed = [screen(&mut session, 0), screen(&mut session, 1)];
    assert_eq!(printed, [0x37, 0x06], "simple prints 7F");
    assert_eq!(border(&mut session), NOISELFSRINIT_PASS, "simple verdict");

    let dir = testprogs_dir().expect("testprog dir checked by caller");
    let reference =
        std::fs::read(dir.join("noiselfsrinit/reference.bin")).expect("reference.bin should read");
    let mut session = run_testprog("noiselfsrinit/scan.prg", Model::C64cPal, 600);
    let machine = session.machine_mut().machine_mut();
    let scanned: Vec<u8> = (0..0x7F0_u16).map(|i| machine.peek(0x2000 + i)).collect();
    assert_eq!(scanned, reference[..0x7F0], "scan against reference.bin");
    assert_eq!(border(&mut session), NOISELFSRINIT_PASS, "scan verdict");
}

/// `detect`: two SID-model detection routines, each built for the 6581
/// (`-old`) and the 8580 (`-new`). `detect-1` looks for the 8580's
/// triangle+sawtooth combination reaching OSC3 bit 7; `detect-2` starts the
/// oscillator at FREQ `$FFFF` and reads OSC3 four cycles later, `3` on a
/// 6581 and `2` on an 8580, whose OSC3 reads triangle and sawtooth a cycle
/// late.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn detect_tells_the_models_apart() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    for prg in [
        "detect-1-old.prg",
        "detect-1-new.prg",
        "detect-2-old.prg",
        "detect-2-new.prg",
    ] {
        let mut session = run_testprog(&format!("detect/{prg}"), model_for(prg), 60);
        assert_eq!(border(&mut session), BORDER_PASS, "{prg} verdict");
    }
}

/// Read the 8-digit hex delay the `bitfade/delay*.prg` programs print at
/// `$0428` (screen codes: digits `$30`-`$39`, letters A-F `$01`-`$06`).
fn printed_delay(session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>) -> u32 {
    (0x28..0x30).fold(0, |value, offset| {
        let code = screen(session, offset);
        let digit = match code {
            0x30..=0x39 => u32::from(code - 0x30),
            0x01..=0x06 => u32::from(code) + 9,
            other => panic!("unexpected screen code ${other:02X} in the delay readout"),
        };
        value << 4 | digit
    })
}

/// `bitfade/delayfrq0` and `delaynoise`: CIA-timed hold times of the SID data
/// bus after a write, and of the noise register's drift to all ones while
/// TEST is held. Real chips: about $1D00 for the 6581 bus and $7A000-$108000
/// for the 8580's (readme.txt); the noise drift follows reSID's per-model
/// start delay plus one step per missing bit.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn bitfade_delays_match_the_model_hold_times() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    let mut session = run_testprog("bitfade/delayfrq0.prg", Model::C64PalBreadbin, 30);
    let delay = printed_delay(&mut session);
    assert!(
        (0x1C00..=0x1E00).contains(&delay),
        "6581 bus hold ${delay:X}"
    );

    let mut session = run_testprog("bitfade/delayfrq0.prg", Model::C64cPal, 120);
    let delay = printed_delay(&mut session);
    assert!(
        (0xA1000..=0xA3000).contains(&delay),
        "8580 bus hold ${delay:X}"
    );

    let mut session = run_testprog("bitfade/delaynoise.prg", Model::C64PalBreadbin, 60);
    let delay = printed_delay(&mut session);
    assert!(
        (35_000..=56_000).contains(&delay),
        "6581 noise drift {delay}"
    );

    let mut session = run_testprog("bitfade/delaynoise.prg", Model::C64cPal, 900);
    let delay = printed_delay(&mut session);
    assert!(
        (2_519_864..=2_519_864 + 21 * 315_000).contains(&delay),
        "8580 noise drift {delay}"
    );
}

/// The SID model a testprog build targets: `_old`/`-old` builds carry 6581
/// reference values (C64 breadbin), `_new`/`-new` and `-8580` builds 8580
/// values (C64C).
fn model_for(prg: &str) -> Model {
    if prg.contains("new") || prg.contains("8580") {
        Model::C64cPal
    } else {
        Model::C64PalBreadbin
    }
}

/// Run every `.prg` in `dir` (relative to the testprog dir) whose name
/// starts with `prefix`, on the model its name targets, and return the
/// names whose border verdict is not a pass.
fn failing_border_verdicts(dir: &str, prefix: &str, frames: u32) -> (usize, Vec<String>) {
    let root = testprogs_dir().expect("testprog dir checked by caller");
    let mut names: Vec<String> = std::fs::read_dir(root.join(dir))
        .expect("testprog subdirectory should list")
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.starts_with(prefix) && name.ends_with(".prg"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no {prefix}*.prg in {dir}");
    let failures = names
        .iter()
        .filter_map(|name| {
            let mut session = run_testprog(&format!("{dir}/{name}"), model_for(name), frames);
            (border(&mut session) != BORDER_PASS).then(|| name.clone())
        })
        .collect();
    (names.len(), failures)
}

/// `wb_testsuite`: reset the noise register to all ones, select waveform X
/// with TEST, release TEST into waveform Y (both including noise), then
/// shift with TEST pulses and compare OSC3 with readings from real 6581s
/// (`_old`) and 8580s (`_new`). Exercises the combined-waveform write-back
/// into the noise register, both while a combination is selected and on
/// the TEST release, and the noise taps.
///
/// 100 of the 110 programs pass. The ten that fail are a strict residual;
/// VICE 3.10's reSID fails all ten too (and nine more). Seven are 6581
/// releases into noise+pulse (`C`): reSIDfp notes that skipping the
/// write-back while noise+pulse is selected fixes four of them but breaks
/// `wf12nsr`, which this emulation passes on the 6581. `F_to_8_old`
/// is the one transition reSIDfp's rule set gives up to pass the
/// `noiselfsrinit` programs. `D_to_E_old` and `C_to_F_new` are unexplained
/// in both references.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn wb_testsuite_noise_write_back_matches_real_chips() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    let (count, failures) = failing_border_verdicts("wb_testsuite", "noise_writeback_check_", 60);
    assert_eq!(count, 110, "wb_testsuite programs staged");
    assert_eq!(
        failures,
        [
            "noise_writeback_check_8_to_C_old.prg",
            "noise_writeback_check_9_to_C_old.prg",
            "noise_writeback_check_A_to_C_old.prg",
            "noise_writeback_check_C_to_C_old.prg",
            "noise_writeback_check_C_to_F_new.prg",
            "noise_writeback_check_D_to_C_old.prg",
            "noise_writeback_check_D_to_E_old.prg",
            "noise_writeback_check_E_to_C_old.prg",
            "noise_writeback_check_F_to_8_old.prg",
            "noise_writeback_check_F_to_C_old.prg",
        ],
        "wb_testsuite residual"
    );
}

/// `noisewriteback/noise_writeback_test1` and `test2` (6581 `-old` and 8580
/// `-new` builds): with the noise register full, release TEST from
/// noise+triangle into noise alone and read OSC3 `$FE` twice (no write-back
/// on that release); and release it from no waveform into noise+triangle,
/// read `$00` (the write-back follows the shift), then start the oscillator
/// and read the shifted-in ones. The directory's older
/// `noise_writeback_check_*` programs are superseded by `wb_testsuite`.
///
/// `test2`'s second reading lands on the cycle the shift completes, two
/// cycles after bit 19 rises, before the selector can write zeros back:
/// the chips read the new ones ANDed with the triangle, `$14` on the 6581
/// and `$12` on the 8580, whose OSC3 sees last cycle's triangle.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn noisewriteback_tests_match_real_chips() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    for prg in [
        "noise_writeback_test1-old.prg",
        "noise_writeback_test1-new.prg",
    ] {
        let mut session = run_testprog(&format!("noisewriteback/{prg}"), model_for(prg), 900);
        assert_eq!(screen(&mut session, 0), 0xFE, "{prg} first read");
        assert_eq!(screen(&mut session, 1), 0xFE, "{prg} second read");
        assert_eq!(border(&mut session), BORDER_PASS, "{prg} verdict");
    }
    for prg in [
        "noise_writeback_test2-old.prg",
        "noise_writeback_test2-new.prg",
    ] {
        let mut session = run_testprog(&format!("noisewriteback/{prg}"), model_for(prg), 900);
        assert_eq!(screen(&mut session, 0), 0x00, "{prg} first read");
        let expected = if prg.contains("new") { 0x12 } else { 0x14 };
        assert_eq!(screen(&mut session, 1), expected, "{prg} second read");
        assert_eq!(border(&mut session), BORDER_PASS, "{prg} verdict");
    }
}

/// `wf12nsr`: for each waveform with noise, reset the register, read OSC3
/// with TEST, without it and with it again, then read noise alone as it
/// shifts, against readings from a real 6581 (`wf12nsr.prg`) and 8580
/// (`wf12nsr-8580.prg`). It checks the noise+pulse pull-down, which reads
/// `$FC` (252) with TEST held, and the write-back each combination leaves.
///
/// The 6581 build matches all 1000 cells. The 8580 build differs in two:
/// noise+pulse over a full register reads `$FC` where the chip reads `$F8`.
/// That is reSID's `noise_pulse8580` value; VICE's reSID fails the same
/// build.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn wf12nsr_noise_combinations_match_real_chips() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    // Each reset waits for the TEST drift to refill the register, which
    // takes about 9.1 million cycles on the 8580.
    let mut session = run_testprog("wf12nsr/wf12nsr.prg", Model::C64PalBreadbin, 2000);
    assert_eq!(border(&mut session), BORDER_PASS, "6581 verdict");

    let mut session = run_testprog("wf12nsr/wf12nsr-8580.prg", Model::C64cPal, 8000);
    let machine = session.machine_mut().machine_mut();
    // The program colours each cell red ($2) where it differs from the
    // reference.
    let colours: Vec<u8> = (0..1000_u16)
        .map(|cell| machine.cpu_read(0xD800 + cell))
        .collect();
    let mismatches: Vec<(u16, u8)> = (0..1000_u16)
        .filter(|&cell| colours[usize::from(cell)] & 0x0F == 2)
        .map(|cell| (cell, machine.peek(0x0400 + cell)))
        .collect();
    assert_eq!(
        mismatches,
        [(12 * 40 + 1, 0xFC), (12 * 40 + 2, 0xFC)],
        "8580: only noise+pulse over a full register differs ($F8 on the chip)"
    );
}

/// Agreement, out of 256 OSC3 samples one cycle apart, between this
/// emulation and the readings `waveforms.asm` recorded from a real 6581 and
/// 8580 (gpz's C64 and C64C), for each waveform 0-7. The interactive build
/// samples every waveform before it waits for a key, storing its readings
/// at `$5000 + 256 * waveform` beside the reference at `$4000 + 256 *
/// waveform`.
fn waveform_agreement(prg: &str, model: Model) -> [usize; 8] {
    let mut session = run_testprog(prg, model, 50);
    // `currtest` ($FC) counts up through the waveforms; stop once 0-7 are in.
    let mut frames = 0;
    while session.machine_mut().machine_mut().peek(0xFC) < 8 {
        session.run_frames(50).expect("testprog should run");
        frames += 50;
        assert!(frames < 2000, "waveforms 0-7 never finished on {model:?}");
    }
    let machine = session.machine_mut().machine_mut();
    std::array::from_fn(|wave| {
        let base = wave as u16 * 256;
        (0..256)
            .filter(|&i| machine.peek(0x5000 + base + i) == machine.peek(0x4000 + base + i))
            .count()
    })
}

/// `waveforms`: combined waveforms against OSC3 readings from real chips.
/// These are different chips from the ones reSID's tables were sampled on,
/// and the readme warns combined waveforms vary between chips and drift, so
/// the counts are a strict record of agreement, not a pass mark.
///
/// Both models match triangle, sawtooth and pulse exactly; on the 8580
/// that depends on OSC3 reading triangle and sawtooth a cycle late, and on
/// both on the pulse comparator's one-cycle delay. The 6581's pulse+saw and
/// pulse+saw+triangle match exactly too, because the combination pulls the
/// accumulator's MSB low halfway through the cycle.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-sid"]
fn waveforms_combined_agree_with_real_chips() {
    if !staged() {
        emu198x_test_skip::skip!("C64 ROMs or VICE SID testprogs not staged");
    }
    let agreement = waveform_agreement("waveforms/waveforms-6581.prg", Model::C64PalBreadbin);
    assert_eq!(
        agreement,
        [256, 256, 256, 242, 256, 246, 256, 256],
        "6581 agreement per waveform 0-7"
    );
    let agreement = waveform_agreement("waveforms/waveforms-8580.prg", Model::C64cPal);
    assert_eq!(
        agreement,
        [256, 256, 256, 216, 256, 252, 183, 243],
        "8580 agreement per waveform 0-7"
    );
}
