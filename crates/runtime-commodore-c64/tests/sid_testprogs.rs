//! VICE SID test programs — behavioural oracles for the SID register surface.
//!
//! The programs come from VICE's `testprogs/SID/` tree (external,
//! env-gated, staged under `~/.emu198x/test-suites/c64-sid/` or
//! `EMU198X_C64_SID_TESTPROGS_DIR`). Each one boots, pokes the SID, and leaves
//! its readings in screen RAM and its verdict in the border colour: light
//! green (5) for pass, red (2) for fail. Their expected values were measured
//! on real 6581 and 8580 chips; see each program's `readme.txt`.

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
