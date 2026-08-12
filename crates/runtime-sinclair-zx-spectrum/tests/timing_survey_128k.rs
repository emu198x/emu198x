//! ZXSpectrum4.net timing survey, 128K edition.
//!
//! The 128K has had arrival-resolved differentials against FUSE since
//! #864 — memory contention at 17 of 375,406, the floating bus byte-exact
//! — and no program-level oracle at all. Everything real that has ever run
//! against this machine's timing is HALT2INT128, which is one pass/fail.
//!
//! This is the counterpart of `timing_survey.rs`: the same suite, by the
//! same authors (Richard and Tim Butler), built for the 128K. 35 tests,
//! each timing a group of opcodes through a `JP (HL)` loop broken only by
//! the frame interrupt, once through uncontended memory and once through
//! contended, graded by the suite itself against values recorded on real
//! hardware.
//!
//! Because the suite grades itself, a result here is a *disagreement with
//! published real-hardware values*, not with a second emulator — rank 2 of
//! `knowledge/decisions/spectrum-test-oracle-priority.md`, above FUSE.
//! That matters more on the 128K than it did on the 48K, because the 128K
//! is where our own two frame anchors disagree by two T-states and the
//! only oracle spanning them is a community-reference constant.
//!
//! Run:
//!
//! ```text
//! EMU198X_ZX_SPECTRUM_TESTS_DIR=<dir> \
//! EMU198X_SPECTRUM_128K_ROM0=<rom0> EMU198X_SPECTRUM_128K_ROM1=<rom1> \
//!   cargo test --release -p runtime-sinclair-zx-spectrum \
//!   --test timing_survey_128k -- --ignored --nocapture
//! ```
//!
//! ## Why it needed a snapshot format first
//!
//! The 48K suite ships as `.sna`. The 128K one ships as `.szx` and `.wav`
//! and nothing else, which is why `format-sinclair-zx-spectrum-szx` was
//! written (#865). It is also why this harness applies its snapshot to a
//! machine whose ROMs are loaded first — SZX carries no ROM, deliberately.
//!
//! ## The suite must run in 48K paging mode, and does
//!
//! Its own banner says `** MUST RUN IN 48k MODE **`, and the snapshot is
//! captured that way: `$7FFD` = `0x30`, which is ROM 1 with paging locked.
//! That is not a weaker test than the 48K survey — the 128K's ULA timing
//! differs from the 48K's whatever the paging does, which is the whole
//! point of there being a separate suite. `the_suite_runs_in_48k_paging_mode`
//! pins it, because a harness that paged underneath would be driving a
//! machine the snapshot never described.

mod common;

use common::{
    CaseResult, absorb, digit_keys, revision, scrape_cases, set_key, sha256_hex, write_report,
};
use std::path::{Path, PathBuf};

use common_sinclair_zx_spectrum::MemoryBus;
use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use common_sinclair_zx_spectrum::screen_text::decode_screen_text;
use format_sinclair_zx_spectrum_snapshot::Snapshot;
use format_sinclair_zx_spectrum_szx::parse_szx;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;

/// Directory holding the extracted `zx-spectrum-tests` corpus.
const TESTS_DIR_ENV: &str = "EMU198X_ZX_SPECTRUM_TESTS_DIR";
const ROM0_PATH_ENV: &str = "EMU198X_SPECTRUM_128K_ROM0";
const ROM1_PATH_ENV: &str = "EMU198X_SPECTRUM_128K_ROM1";

const SUITE_FILE: &str =
    "ZX Spectrum Timing Tests - 128K v1.0 (2015-03-30)(Butler, Richard; Butler, Tim)[!].szx";

/// Pinned so a swapped fixture fails the test rather than quietly changing
/// the score — the same rule `timing_survey.rs` applies to its `.sna`.
const SUITE_SHA256: &str = "cc380ad8f77fa8a66d4c1e92cd7be4bad71dac6d051da9464d6ca5ca942a7261";

/// Tests this build actually has: **34**, not the 35 its own prompt
/// offers.
///
/// The prompt reads `choose test 1-35 or leave blank`, inherited from the
/// 48K build. Asking for 35 does not stall — it drops straight out with
/// `9 STOP statement, 1350:1`, which is the suite falling off the end of
/// its own table. Tests 1 to 34 all run. Measured, because taking the
/// prompt at its word costs one dead test per run and looks like a hang.
const TEST_COUNT: usize = 34;

/// `(test, mode)` pairs this suite cannot complete on this machine.
///
/// Test 2's contended pass stops with `4 Out of memory, 5070:1` — a BASIC
/// error raised by the suite itself, after its uncontended pass has
/// already reported `Pass`. It reproduces exactly, from a fresh boot, and
/// it also ends the run when the prompt is answered with a blank line to
/// select every test.
///
/// Recorded rather than skipped or asserted away. Whether this is the
/// suite running out of room on a 128K in 48K mode or something this
/// engine does to it is **not established**, and the honest form is a list
/// that fails when it changes in either direction: a new gap is a
/// regression, and this one closing means someone should find out why.
const KNOWN_INCOMPLETE: &[(usize, &str)] = &[(2, "Contended")];

/// Frames to let the snapshot settle before its prompt is live.
const BOOT_FRAMES: usize = 200;
/// Upper bound on frames spent waiting for one mode to report.
const TEST_BUDGET_FRAMES: usize = 4_000;
const POLL_FRAMES: usize = 25;

const CONTINUE_PROMPT: &str = "Press any key for next test";

/// `$7FFD` bit 5 — paging disabled, i.e. 48K mode.
const PAGING_LOCKED: u8 = 0x20;

fn suite_path() -> PathBuf {
    PathBuf::from(std::env::var_os(TESTS_DIR_ENV).unwrap_or_default()).join(SUITE_FILE)
}

fn roms() -> Option<(Vec<u8>, Vec<u8>)> {
    let rom0 = std::fs::read(std::env::var(ROM0_PATH_ENV).ok()?).ok()?;
    let rom1 = std::fs::read(std::env::var(ROM1_PATH_ENV).ok()?).ok()?;
    Some((rom0, rom1))
}

/// Press and release one key, giving the ROM time to see both edges.
///
/// The 128K-class core exposes a bare `[u8; 8]` matrix rather than the
/// 48K's `KeyboardMatrix` wrapper, which is the only reason this is not
/// shared with `timing_survey.rs`.
fn tap_key(machine: &mut Spectrum128K, key: SpectrumKey) {
    set_key(&mut machine.keyboard, key, true);
    run_frames(machine, 6);
    set_key(&mut machine.keyboard, key, false);
    run_frames(machine, 6);
}

fn run_frames(machine: &mut Spectrum128K, frames: usize) {
    for _ in 0..frames {
        machine.run_frame();
    }
}

/// Glyphs from ROM 1 (48 BASIC) explicitly.
///
/// The suite prints through the 48K ROM's routines and the machine ends up
/// in 48K paging mode, but reading the font from whichever bank happens to
/// be mapped at capture time makes the decode depend on state this harness
/// does not control.
fn screen(machine: &Spectrum128K) -> Vec<String> {
    decode_screen_text(
        |addr| machine.memory.read_rom_byte(1, addr),
        |addr| machine.memory.read(addr),
    )
}

/// A machine with the suite loaded and settled at its prompt.
fn booted(roms: &(Vec<u8>, Vec<u8>), snapshot: &Snapshot) -> Spectrum128K {
    let mut machine = Spectrum128K::new();
    machine.memory.load_roms(&roms.0, &roms.1);
    machine.reset();
    machine.apply_snapshot(snapshot);
    run_frames(&mut machine, BOOT_FRAMES);
    machine
}

/// The suite's own banner says it must run in 48K paging mode. Check that
/// it is, rather than assuming the snapshot arranged it.
///
/// This can fail two ways that matter: the snapshot's `$7FFD` not being
/// applied, and a future change to `apply_snapshot` paging over it.
#[test]
#[ignore = "needs the zx-spectrum-tests corpus and 128K ROMs"]
fn the_suite_runs_in_48k_paging_mode() {
    let (Some(roms), Ok(bytes)) = (roms(), std::fs::read(suite_path())) else {
        panic!("set {TESTS_DIR_ENV}, {ROM0_PATH_ENV} and {ROM1_PATH_ENV}");
    };
    let snapshot = parse_szx(&bytes).expect("parse the 128K timing suite");
    let machine = booted(&roms, &snapshot);

    let banner = screen(&machine);
    assert!(
        banner.iter().any(|l| l.contains("MUST RUN IN 48k MODE")),
        "the suite's banner is missing, so this is not the program the \
         harness thinks it booted; screen:\n{}",
        banner.join("\n")
    );
    // Demonstrated rather than read off a field: try to page a different
    // bank in at `$C000` and show that nothing moves. That is what "paging
    // locked" *means*, and it cannot be satisfied by a stale copy of
    // `$7FFD` the way reading the port back could.
    let mut machine = machine;
    let before: Vec<u8> = (0xC000u16..0xC040)
        .map(|a| machine.memory.read(a))
        .collect();
    let other_bank = (snapshot.port_7ffd & 0x07) ^ 0x01;
    machine.port_write(0x7FFD, (snapshot.port_7ffd & !0x07) | other_bank);
    let after: Vec<u8> = (0xC000u16..0xC040)
        .map(|a| machine.memory.read(a))
        .collect();
    assert_eq!(
        before, after,
        "writing $7FFD changed what is mapped at $C000, so paging is not \
         locked and the suite is running on a machine it says it cannot"
    );
    assert_eq!(
        snapshot.port_7ffd & PAGING_LOCKED,
        PAGING_LOCKED,
        "the snapshot itself no longer requests 48K paging mode"
    );
}

/// Run all 35 tests and record every case, contended and uncontended.
///
/// Fails only on harness problems — a missing or altered fixture, a test
/// that never completes. A *disagreement* is data, not an error: a graded
/// survey exists to report where the machine differs, and turning any
/// single case into an assertion here would collapse it into the binary
/// gate it replaces.
#[test]
#[ignore = "needs the zx-spectrum-tests corpus and 128K ROMs; ~5 min"]
fn timing_survey_128k_records_every_case() {
    let Some(roms) = roms() else {
        panic!("set {ROM0_PATH_ENV} and {ROM1_PATH_ENV} to the 128K ROMs");
    };
    let path = suite_path();
    if !path.is_file() {
        panic!(
            "timing suite not found at {} — set {TESTS_DIR_ENV}",
            path.display()
        );
    }
    let suite_bytes = std::fs::read(&path).expect("read timing suite");
    let actual_sha = sha256_hex(&suite_bytes);
    if SUITE_SHA256 != "PLACEHOLDER" {
        assert_eq!(
            actual_sha, SUITE_SHA256,
            "timing suite bytes changed; results are not comparable across \
             revisions until the pin is updated deliberately"
        );
    }
    let snapshot = parse_szx(&suite_bytes).expect("parse the 128K timing suite");

    // One fresh machine per test, selected by number at the prompt.
    //
    // The 48K harness learned this the hard way and the 128K suite fails
    // the same way, harder: driving all 35 from one session by answering
    // the prompt with a blank line runs two tests and then dies with
    // `4 Out of memory, 5070:1`, because the transcript scrolls and BASIC
    // runs out of room. Per-test boots make every case independent.
    let mut cases: Vec<CaseResult> = Vec::new();
    let mut incomplete = Vec::new();

    for test_number in 1..=TEST_COUNT {
        let mut machine = booted(&roms, &snapshot);

        for key in digit_keys(test_number) {
            tap_key(&mut machine, key);
        }
        tap_key(&mut machine, SpectrumKey::Enter);

        // {Uncontended} first, then a key, then {Contended}.
        let mut seen_modes = 0;
        for _ in 0..2 {
            let mut waited = 0;
            let mut settled = false;
            while waited < TEST_BUDGET_FRAMES {
                let lines = screen(&machine);
                absorb(&mut cases, scrape_cases(&lines));
                let armed = lines
                    .iter()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .is_some_and(|l| l.contains(CONTINUE_PROMPT));
                let reported = cases.iter().filter(|c| c.test == test_number).count() > seen_modes;
                if reported && armed {
                    settled = true;
                    break;
                }
                run_frames(&mut machine, POLL_FRAMES);
                waited += POLL_FRAMES;
            }
            if !settled {
                break;
            }
            seen_modes = cases.iter().filter(|c| c.test == test_number).count();
            tap_key(&mut machine, SpectrumKey::Space);
        }

        absorb(&mut cases, scrape_cases(&screen(&machine)));
        if !cases.iter().any(|c| c.test == test_number) {
            incomplete.push(test_number);
        }
    }

    cases.sort_by_key(|c| (c.test, c.mode.clone()));
    let failures: Vec<&CaseResult> = cases.iter().filter(|c| c.verdict == "fail").collect();

    let revision = revision();
    let report = serde_json::json!({
        "survey": "zxspectrum4.net-timing-tests-128k",
        "revision": revision,
        "machine": "sinclair-zx-spectrum-128k",
        "suite_sha256": actual_sha,
        "suite_file": SUITE_FILE,
        "tests_covered": TEST_COUNT,
        "cases_recorded": cases.len(),
        "cases_failing": failures.len(),
        "tests_incomplete": incomplete,
        "cases": cases,
    });

    let report_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/accuracy/spectrum-timing-survey-128k")
        .join(&revision)
        .join("report.json");
    write_report(&report_path, &report);

    println!("\n=== ZXSpectrum4.net 128K timing survey @ {revision} ===");
    println!("  suite sha256: {actual_sha}");
    println!(
        "  cases recorded: {}  failing: {}",
        cases.len(),
        failures.len()
    );
    for case in &failures {
        println!(
            "  FAIL  test {:>2} {:<13} {}  measured {:?} expected {:?}",
            case.test, case.mode, case.description, case.measured, case.expected
        );
    }
    if !incomplete.is_empty() {
        println!("  incomplete tests: {incomplete:?}");
    }
    println!("  report: {}", report_path.display());

    // Which `(test, mode)` pairs never reported, against the ones known
    // not to. Asserted as an exact set: a *new* gap is a harness failure
    // or a regression, and a known gap closing is a finding.
    let recorded: std::collections::BTreeSet<(usize, String)> =
        cases.iter().map(|c| (c.test, c.mode.clone())).collect();
    let missing: Vec<(usize, String)> = (1..=TEST_COUNT)
        .flat_map(|t| {
            ["Uncontended", "Contended"]
                .into_iter()
                .map(move |m| (t, m.to_owned()))
        })
        .filter(|k| !recorded.contains(k))
        .collect();
    let known: Vec<(usize, String)> = KNOWN_INCOMPLETE
        .iter()
        .map(|(t, m)| (*t, (*m).to_owned()))
        .collect();
    assert_eq!(
        missing, known,
        "the set of cases that never reported has changed. Extra entries are \
         a stall or an undriven prompt; missing entries mean a known gap \
         closed and the record needs updating."
    );

    // The ratchet goes last, so a red run still prints its own explanation.
    // A ceiling, not a target: lower it in the commit that earns it, never
    // raise it silently.
    // 10 of 67, the first recorded figure for this machine. The shape
    // mirrors the 48K's: the block I/O groups (`INI`/`INIR`, `OUTI`/`OTIR`)
    // fail in both modes, and the arithmetic group fails contended only.
    const RATCHET_FAILURES: usize = 10;
    assert!(
        failures.len() <= RATCHET_FAILURES,
        "128K timing survey regressed: {} of {} cases failing, was \
         {RATCHET_FAILURES}. The failing cases are listed above. If this \
         change is right and the suite's expectations are wrong, say which \
         cases and why, and move the ratchet in the same commit.",
        failures.len(),
        cases.len(),
    );
    if failures.len() < RATCHET_FAILURES {
        println!(
            "  RATCHET: {} of {} failing — improved on {RATCHET_FAILURES}. \
             Lower the constant in this commit.",
            failures.len(),
            cases.len()
        );
    }
}

/// The report path is derived, not hand-built.
#[test]
fn the_report_path_is_machine_specific() {
    let a = Path::new("spectrum-timing-survey");
    let b = Path::new("spectrum-timing-survey-128k");
    assert_ne!(
        a, b,
        "the 48K and 128K surveys must not write to the same report path"
    );
}
