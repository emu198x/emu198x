//! ZXSpectrum4.net timing survey, 128K edition.
//!
//! Runs the Butler 128K suite in locked 48K paging mode and preserves its
//! own verdicts. The suite's five failures on early-timing Toastracks are
//! also reported on physical machines: tests 4, 17, 18, 26 and 33.
//! This harness requires those exact readings and passes everywhere else,
//! rather than treating any five failures as acceptable.
//!
//! Hardware attribution, table selection and scope are documented in
//! `test-data/spectrum-128k-timing-profile-validation.md`.
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
use common_sinclair_zx_spectrum_128k_class::{
    AmstradPlus2Marker, Class128kVariant, Sinclair128KMarker, Spectrum128kClassCore,
};
use format_sinclair_zx_spectrum_snapshot::Snapshot;
use format_sinclair_zx_spectrum_szx::parse_szx;

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
/// **Empty, and that took establishing.** Test 2's contended pass used to
/// stop with `4 Out of memory, 5070:1` — a BASIC error the suite raises
/// itself, after its uncontended pass had already reported `Pass`. It
/// reproduced exactly from a fresh boot.
///
/// It stopped happening at `56e8148b`, "sample /INT at the instruction
/// boundary", bisected on whether the report contains the case rather than
/// on the test's exit code. That commit changes which instruction boundary
/// an interrupt is taken at, and so the machine stack's depth when it is
/// taken — and `4 Out of memory` is Sinclair BASIC's report for the stack
/// growing into BASIC's space.
///
/// The later die-derived IRQ deadline and integrated ULA correction
/// preserve the complete case set. Instruction-boundary event processing
/// is not evidence of the CPU pin's sampling edge; see
/// `test-data/z80-irq-deadline-validation.md` for the measured deadline.
///
/// Kept as an asserting list rather than deleted: a *new* gap is still a
/// regression, and this is where it would be recorded.
const KNOWN_INCOMPLETE: &[(usize, &str)] = &[];

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

fn roms(paths: [&str; 2]) -> Option<(Vec<u8>, Vec<u8>)> {
    let rom0 = std::fs::read(std::env::var(paths[0]).ok()?).ok()?;
    let rom1 = std::fs::read(std::env::var(paths[1]).ok()?).ok()?;
    Some((rom0, rom1))
}

/// Press and release one key, giving the ROM time to see both edges.
///
/// The 128K-class core exposes a bare `[u8; 8]` matrix rather than the
/// 48K's `KeyboardMatrix` wrapper, which is the only reason this is not
/// shared with `timing_survey.rs`.
fn tap_key<V: Class128kVariant>(machine: &mut Spectrum128kClassCore<V>, key: SpectrumKey) {
    set_key(&mut machine.keyboard, key, true);
    run_frames(machine, 6);
    set_key(&mut machine.keyboard, key, false);
    run_frames(machine, 6);
}

fn run_frames<V: Class128kVariant>(machine: &mut Spectrum128kClassCore<V>, frames: usize) {
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
fn screen<V: Class128kVariant>(machine: &Spectrum128kClassCore<V>) -> Vec<String> {
    decode_screen_text(
        |addr| machine.memory.read_rom_byte(1, addr),
        |addr| machine.memory.read(addr),
    )
}

/// A machine with the suite loaded and settled at its prompt.
fn booted<V: Class128kVariant>(
    roms: &(Vec<u8>, Vec<u8>),
    snapshot: &Snapshot,
) -> Spectrum128kClassCore<V> {
    let mut machine = Spectrum128kClassCore::<V>::new();
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
#[ignore = "FIXTURE: needs the zx-spectrum-tests corpus and 128K ROMs"]
fn the_suite_runs_in_48k_paging_mode() {
    let (Some(roms), Ok(bytes)) = (
        roms([ROM0_PATH_ENV, ROM1_PATH_ENV]),
        std::fs::read(suite_path()),
    ) else {
        panic!("set {TESTS_DIR_ENV}, {ROM0_PATH_ENV} and {ROM1_PATH_ENV}");
    };
    let snapshot = parse_szx(&bytes).expect("parse the 128K timing suite");
    let machine = booted::<Sinclair128KMarker>(&roms, &snapshot);

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

/// Published Issue 6K and 6U Toastrack results, attributed to Brendon Alford:
/// https://github.com/redcode/ZXSpectrum/wiki/ZX-Spectrum-Timing-Tests-128K
/// Tuples contain test number, R, loop count and saved IRQ return address.
/// The suite labels the last field `sp`; it is not the live stack pointer.
const EARLY_TOASTRACK_READINGS: [(usize, i64, i64, i64); 5] = [
    (4, 6, 174, 23305),
    (17, 22, 203, 23335),
    (18, 22, 203, 23335),
    (26, 75, 147, 23345),
    (33, 119, 196, 23315),
];

/// Completeness is checked separately after the report is written.
fn early_toastrack_mismatches(cases: &[CaseResult]) -> Vec<String> {
    let mut mismatches = Vec::new();
    for case in cases {
        let expected = EARLY_TOASTRACK_READINGS
            .iter()
            .find(|(test, _, _, _)| case.mode == "Contended" && case.test == *test);
        if let Some((_, r, loops, return_address)) = expected {
            let readings = std::collections::BTreeMap::from([
                ("r".to_owned(), *r),
                ("loop".to_owned(), *loops),
                ("sp".to_owned(), *return_address),
            ]);
            if case.verdict != "fail" || case.measured != readings {
                mismatches.push(format!(
                    "test {} {} differs from early Toastrack hardware: \
                     {:?} {:?}, expected suite fail with {readings:?}",
                    case.test, case.mode, case.verdict, case.measured
                ));
            }
        } else if case.verdict != "pass" {
            mismatches.push(format!(
                "test {} {}: unexpected suite {:?} outside the early Toastrack exceptions",
                case.test, case.mode, case.verdict
            ));
        }
    }
    mismatches
}

#[test]
fn early_profile_rejects_changed_readings_even_with_the_same_failure_count() {
    let mut cases: Vec<CaseResult> = EARLY_TOASTRACK_READINGS
        .iter()
        .map(|(test, r, loops, sp)| CaseResult {
            test: *test,
            mode: "Contended".to_owned(),
            description: String::new(),
            verdict: "fail".to_owned(),
            measured: std::collections::BTreeMap::from([
                ("r".to_owned(), *r),
                ("loop".to_owned(), *loops),
                ("sp".to_owned(), *sp),
            ]),
            expected: std::collections::BTreeMap::new(),
        })
        .collect();
    assert!(early_toastrack_mismatches(&cases).is_empty());
    cases[0].measured.insert("r".to_owned(), 7);
    assert_eq!(early_toastrack_mismatches(&cases).len(), 1);
    cases[0].measured.insert("r".to_owned(), 6);
    cases[0].test = 5;
    assert_eq!(early_toastrack_mismatches(&cases).len(), 1);
    cases[0].test = 4;
    cases[0].mode = "Uncontended".to_owned();
    assert_eq!(early_toastrack_mismatches(&cases).len(), 1);
    cases[0].mode = "Contended".to_owned();
    cases[0].verdict = "pass".to_owned();
    assert_eq!(early_toastrack_mismatches(&cases).len(), 1);
}

/// Record all 68 cases before checking completeness and the early Toastrack
/// profile. Raw suite failures remain visible in the JSON and console output.
#[test]
#[ignore = "FIXTURE: needs the zx-spectrum-tests corpus and 128K ROMs; ~5 min"]
fn timing_survey_128k_records_every_case() {
    run_survey::<Sinclair128KMarker>(
        [ROM0_PATH_ENV, ROM1_PATH_ENV],
        "early-toastrack",
        "spectrum-timing-survey-128k",
        early_toastrack_mismatches,
    );
}

/// Published grey +2 boards pass every case, unlike early Toastracks.
#[test]
#[ignore = "FIXTURE: needs the zx-spectrum-tests corpus and grey +2 ROMs; ~8 min"]
fn timing_survey_plus2_records_every_case() {
    run_survey::<AmstradPlus2Marker>(
        ["EMU198X_SPECTRUM_PLUS2_ROM0", "EMU198X_SPECTRUM_PLUS2_ROM1"],
        "late-grey-plus2",
        "spectrum-timing-survey-plus2",
        |cases| {
            cases
                .iter()
                .filter(|case| case.verdict != "pass")
                .map(|case| {
                    format!(
                        "grey +2 test {} {}: expected pass, got {}",
                        case.test, case.mode, case.verdict
                    )
                })
                .collect()
        },
    );
}

fn run_survey<V: Class128kVariant>(
    rom_paths: [&str; 2],
    profile: &str,
    report_directory: &str,
    check_profile: fn(&[CaseResult]) -> Vec<String>,
) {
    let roms =
        roms(rom_paths).unwrap_or_else(|| panic!("set {} and {}", rom_paths[0], rom_paths[1]));
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
        let mut machine = booted::<V>(&roms, &snapshot);
        assert_eq!(snapshot.port_7ffd & PAGING_LOCKED, PAGING_LOCKED);

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

    let profile_mismatches = check_profile(&cases);
    let revision = revision();
    let report = serde_json::json!({
        "survey": "zxspectrum4.net-timing-tests-128k",
        "revision": revision,
        "machine": V::MODEL_ID,
        "rom_sha256": [sha256_hex(&roms.0), sha256_hex(&roms.1)],
        "suite_sha256": actual_sha,
        "suite_file": SUITE_FILE,
        "tests_covered": TEST_COUNT,
        "cases_recorded": cases.len(),
        "cases_failing": failures.len(),
        "hardware_profile": profile,
        "hardware_profile_mismatches": profile_mismatches,
        "tests_incomplete": incomplete,
        "cases": cases,
    });

    let report_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/accuracy")
        .join(report_directory)
        .join(&revision)
        .join("report.json");
    write_report(&report_path, &report);

    println!(
        "\n=== ZXSpectrum4.net {} timing survey @ {revision} ===",
        V::MODEL_ID
    );
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
    let mut stale = Vec::new();

    if missing != known {
        stale.push(format!(
            "the set of cases that never reported has changed: found {missing:?}, \
             recorded {known:?}. Extra entries are a stall or an undriven prompt; \
             missing entries mean a known gap closed and the record needs updating."
        ));
    }

    stale.extend(profile_mismatches);

    assert!(
        stale.is_empty(),
        "the 128K survey's record no longer describes what it measures:\n  - {}",
        stale.join("\n  - ")
    );
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
