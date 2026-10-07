//! VICE CIA test programs — behavioural oracles for the 6526 serial port and
//! CNT input.
//!
//! The programs come from VICE's `testprogs/CIA/` tree (external, env-gated,
//! staged under `~/.emu198x/test-suites/c64-cia/` or
//! `EMU198X_C64_CIA_TESTPROGS_DIR`). Each one boots, drives a CIA and leaves
//! its verdict in the border colour: light green (5) for pass, anything else
//! for fail. Their reference data was captured on real 6526 and 6526A chips.
//!
//! Staging: `shiftregister/` and `ciavarious/` (plus `ciavarious/src/` for
//! the CIA9 and CIA14 sources), fetched from
//! `https://svn.code.sf.net/p/vice-emu/code/testprogs/CIA/` at revision
//! 46281 for #797, and Lorenz's `cntdef.prg` and `cnto2.prg` from
//! `testprogs/general/Lorenz-2.15/src/` at the same revision, under
//! `lorenz/`. The staging directory's `SOURCE.txt` records the same.

mod common;

use std::path::PathBuf;

use common::{local_rom_dir, local_rom_firmware};
use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::HeadlessSession;
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_KEY_HOLD_FRAMES, DEFAULT_TYPE_SETTLE_FRAMES,
    Model, type_string,
};

/// Border colour most of the programs leave on success (green).
const BORDER_PASS: u8 = 5;
/// Border colour the `cia-sdr-*` programs leave on success (light green).
const BORDER_PASS_SDR: u8 = 13;

/// The explicitly configured CIA testprog directory, or the conventional
/// per-user staging directory when no explicit path is supplied.
fn testprogs_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("EMU198X_C64_CIA_TESTPROGS_DIR") {
        let path = PathBuf::from(path);
        return path.exists().then_some(path);
    }
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/test-suites/c64-cia");
    path.exists().then_some(path)
}

fn roms_present() -> bool {
    let dir = local_rom_dir();
    ["kernal.rom", "basic.rom", "chargen.rom"]
        .iter()
        .all(|name| dir.join(name).is_file())
}

fn staged() -> bool {
    roms_present() && testprogs_dir().is_some()
}

type Session = HeadlessSession<C64Runtime, C64SessionQueryProvider>;

/// Boot real ROMs on `model`, load a testprog `.prg` (relative to the testprog
/// dir), RUN it and let it run for `frames` frames.
fn run_testprog(rel_prg: &str, model: Model, frames: u32) -> Session {
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

fn border(session: &mut Session) -> u8 {
    session.machine_mut().machine_mut().cpu_read(0xD020) & 0x0F
}

/// The first `rows` lines of the text screen, screen codes mapped to ASCII.
fn screen_text(session: &mut Session, rows: u16) -> String {
    let machine = session.machine_mut().machine_mut();
    let mut text = String::new();
    for row in 0..rows {
        for col in 0..40 {
            let code = machine.peek(0x0400 + row * 40 + col) & 0x7F;
            text.push(char::from(if code < 0x20 { code + 0x40 } else { code }));
        }
        text.push('\n');
    }
    text
}

/// Run each `(prg, model)` and collect the ones that do not finish on
/// `pass`.
fn failures(cases: &[(&str, Model)], frames: u32, pass: u8) -> Vec<String> {
    let mut failures = Vec::new();
    for &(prg, model) in cases {
        let colour = border(&mut run_testprog(prg, model, frames));
        if colour != pass {
            failures.push(format!("{prg} on {model:?}: border {colour}"));
        }
    }
    failures
}

const FIXTURE: &str =
    "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-cia";

/// `shiftregister/cia-sp-test-*`: serial-port output interrupts as Arkanoid's
/// protection uses them. Timer A at 1 drives the shift register; the program
/// logs every ICR read until the SDR interrupt arrives and compares the log
/// with captures from real chips.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-cia"]
fn cia_sp_test_matches_real_chips() {
    if !staged() {
        emu198x_test_skip::skip!("{FIXTURE}");
    }
    let failures = failures(
        &[
            (
                "shiftregister/cia-sp-test-oneshot-old.prg",
                Model::C64PalBreadbin,
            ),
            (
                "shiftregister/cia-sp-test-continues-old.prg",
                Model::C64PalBreadbin,
            ),
            ("shiftregister/cia-sp-test-oneshot-new.prg", Model::C64cPal),
            (
                "shiftregister/cia-sp-test-continues-new.prg",
                Model::C64cPal,
            ),
        ],
        30,
        BORDER_PASS,
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// `shiftregister/cia-icr-test*`: ICR reads around serial-port and timer
/// interrupts, compared with captures from real chips.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-cia"]
fn cia_icr_test_matches_real_chips() {
    if !staged() {
        emu198x_test_skip::skip!("{FIXTURE}");
    }
    let failures = failures(
        &[
            (
                "shiftregister/cia-icr-test-oneshot-old.prg",
                Model::C64PalBreadbin,
            ),
            (
                "shiftregister/cia-icr-test-continues-old.prg",
                Model::C64PalBreadbin,
            ),
            ("shiftregister/cia-icr-test-oneshot-new.prg", Model::C64cPal),
            (
                "shiftregister/cia-icr-test-continues-new.prg",
                Model::C64cPal,
            ),
            ("shiftregister/cia-icr-test2-oneshot.prg", Model::C64cPal),
            ("shiftregister/cia-icr-test2-continues.prg", Model::C64cPal),
        ],
        30,
        BORDER_PASS,
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// `shiftregister/cia-sdr-{init,load,delay}` (VICE bug #1219): when a byte
/// written to the SDR reaches the shift register, and when the SDR
/// interrupt follows, timed by Timer B against values read on a real C64C.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-cia"]
fn cia_sdr_load_timing_matches_a_real_c64() {
    if !staged() {
        emu198x_test_skip::skip!("{FIXTURE}");
    }
    let failures = failures(
        &[
            ("shiftregister/cia-sdr-init.prg", Model::C64cPal),
            ("shiftregister/cia-sdr-load.prg", Model::C64cPal),
            ("shiftregister/cia-sdr-delay.prg", Model::C64cPal),
        ],
        150,
        BORDER_PASS_SDR,
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// `ciavarious/cia9`: Timer A switched between counting φ2 and counting CNT
/// on every pass. With the user port empty CNT sits high, so a CNT-counting
/// timer must stand still; the reads are compared with a real C64.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-cia"]
fn ciavarious_cnt_input_mode_matches_a_real_c64() {
    if !staged() {
        emu198x_test_skip::skip!("{FIXTURE}");
    }
    let failures = failures(
        &[("ciavarious/cia9.prg", Model::C64PalBreadbin)],
        100,
        BORDER_PASS,
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Lorenz `cntdef` and `cnto2`: CNT idles high with nothing on the user
/// port (Timer B counting Timer A underflows "while CNT is high" counts),
/// and switching Timer A between CNT and φ2 costs two cycles either way.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-cia"]
fn lorenz_cnt_cases_pass() {
    if !staged() {
        emu198x_test_skip::skip!("{FIXTURE}");
    }
    for (prg, name) in [
        ("lorenz/cntdef.prg", "CNTDEF"),
        ("lorenz/cnto2.prg", "CNTO2"),
    ] {
        // Each case prints its name and " - OK", then tries to LOAD the next
        // one from a disk that is not there.
        let mut session = run_testprog(prg, Model::C64PalBreadbin, 100);
        let text = screen_text(&mut session, 12);
        assert!(text.contains(&format!("{name} - OK")), "{prg}:\n{text}");
    }
}

/// The rest of `ciavarious` (timers, cascade, PB6/PB7): a regression net
/// around the CIA changes. The `new` builds carry 6526A reference data.
/// CIA15 (TOD) needs minutes of emulated time and is left out.
#[test]
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-cia"]
fn ciavarious_matches_a_real_c64() {
    if !staged() {
        emu198x_test_skip::skip!("{FIXTURE}");
    }
    let cases: Vec<(String, Model)> = [
        ("cia1", Model::C64PalBreadbin),
        ("cia2", Model::C64PalBreadbin),
        ("cia3", Model::C64PalBreadbin),
        ("cia3new", Model::C64cPal),
        ("cia3a", Model::C64PalBreadbin),
        ("cia3anew", Model::C64cPal),
        ("cia4", Model::C64PalBreadbin),
        ("cia4new", Model::C64cPal),
        ("cia5", Model::C64PalBreadbin),
        ("cia6", Model::C64PalBreadbin),
        ("cia7", Model::C64PalBreadbin),
        ("cia8", Model::C64PalBreadbin),
        ("cia8new", Model::C64cPal),
        ("cia10", Model::C64PalBreadbin),
        ("cia11", Model::C64PalBreadbin),
        ("cia12", Model::C64PalBreadbin),
        ("cia13", Model::C64PalBreadbin),
        ("cia14", Model::C64PalBreadbin),
    ]
    .into_iter()
    .map(|(name, model)| (format!("ciavarious/{name}.prg"), model))
    .collect();
    let cases: Vec<(&str, Model)> = cases.iter().map(|(p, m)| (p.as_str(), *m)).collect();
    let failures = failures(&cases, 300, BORDER_PASS);
    assert!(failures.is_empty(), "{failures:#?}");
}
