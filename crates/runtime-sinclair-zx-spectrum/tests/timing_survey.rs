//! ZXSpectrum4.net 35-test timing survey — the Spectrum's graded oracle.
//!
//! Every other declared Spectrum ULA gate is binary: eight tests that
//! pass or fail, and at the time of writing all eight pass. That
//! instrumentation cannot distinguish "the ULA is correct" from "the ULA
//! is wrong in ways no declared gate probes", so it can never say where
//! to look next. The Amiga and C64 campaigns both turned on a graded,
//! revision-keyed survey; this is the Spectrum's.
//!
//! The suite is Richard's ZXSpectrum4.net timing tests, derived from
//! Woody's code. Each of 35 tests times a group of opcodes through a
//! `JP (HL)` loop broken only by the frame interrupt, once through
//! uncontended memory and once through contended, and compares three
//! measured quantities against values recorded on real 48K hardware.
//! It reports `Pass` or `Fail` itself, and on failure prints the values
//! it expected — so this harness registers no reference images and
//! defines no comparison method of its own. It reads what the machine
//! reported.
//!
//! Because the suite grades itself, a result here is a *disagreement
//! with published real-hardware values*, not a disagreement with a
//! second emulator. That places it at rank 2 of
//! `knowledge/decisions/spectrum-test-oracle-priority.md`, above FUSE.
//!
//! Run:
//!
//! ```text
//! EMU198X_SPECTRUM_TIMING_SUITE=<dir> \
//!   cargo test --release -p runtime-sinclair-zx-spectrum \
//!   --test timing_survey -- --ignored --nocapture
//! ```
//!
//! Writes `target/accuracy/spectrum-timing-survey/<revision>/report.json`,
//! keyed by the full source revision, in the shape of the C64 VIC-II
//! survey report.

mod common;

use common::{
    CaseResult, absorb, digit_keys, parse_readings, revision, scrape_cases, set_key, sha256_hex,
    write_report,
};
use std::path::PathBuf;

use common_sinclair_zx_spectrum::MemoryBus;
use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use common_sinclair_zx_spectrum::screen_text::decode_screen_text;
use format_sinclair_zx_spectrum_sna::parse_sna;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;

/// Directory holding `timingTests48k.sna`.
const SUITE_DIR_ENV: &str = "EMU198X_SPECTRUM_TIMING_SUITE";
const SUITE_FILE: &str = "timingTests48k.sna";

/// Pinned identity of the suite image.
///
/// The survey's results are only comparable across revisions if the
/// program producing them is fixed. Provenance is still open: the image
/// was recovered from a local cache, and its upstream packaging at
/// `zxspectrum4.net/op_timing.php` has not yet been matched to it.
/// Pinning the bytes is what makes that resolvable later rather than
/// never.
const SUITE_SHA256: &str = "b30fa49bd85dc5cefaf014a3088c6568256796f1a8541fca6ed9edce46bce9cf";

/// Tests in the suite, per its own prompt ("choose test 1-35").
const TEST_COUNT: usize = 35;

/// Frames to settle after loading before the machine reaches its prompt.
const BOOT_FRAMES: usize = 200;
/// Frames to wait for one test to finish before giving up on it.
const TEST_BUDGET_FRAMES: usize = 4_000;
/// Frames between polls of the screen while a test runs.
const POLL_FRAMES: usize = 25;

/// The suite blocks here until a key is pressed.
const CONTINUE_PROMPT: &str = "Press any key for next test";

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME must be set"))
}

fn rom_path() -> PathBuf {
    home().join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom")
}

fn suite_path() -> PathBuf {
    std::env::var_os(SUITE_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("Projects/198x/emulators/zx-spectrum/Zen/Other Images"))
        .join(SUITE_FILE)
}

/// Press and release one key, giving the ROM time to see both edges.
fn tap_key(machine: &mut Spectrum48k, key: SpectrumKey) {
    let rows = machine.keyboard_mut().rows_mut();
    set_key(rows, key, true);
    for _ in 0..6 {
        machine.run_frame();
    }
    let rows = machine.keyboard_mut().rows_mut();
    set_key(rows, key, false);
    for _ in 0..6 {
        machine.run_frame();
    }
}

fn run_frames(machine: &mut Spectrum48k, frames: usize) {
    for _ in 0..frames {
        machine.run_frame();
    }
}

/// A 48K machine reads glyphs and display file through one flat map.
fn screen(machine: &Spectrum48k) -> Vec<String> {
    decode_screen_text(|addr| machine.read(addr), |addr| machine.read(addr))
}

/// Run all 35 tests and record every case, contended and uncontended.
///
/// Fails only on harness problems — a missing or altered fixture, a test
/// that never completes. A *disagreement* is data, not an error: the
/// point of a graded survey is to report where the machine differs, and
/// turning any single case into an assertion here would collapse it back
/// into the binary gate this exists to replace. Strict per-case
/// assertions come later, once a case has been closed and can be held.
#[test]
#[ignore = "requires the ZXSpectrum4.net suite and a 48K ROM; several minutes"]
fn timing_survey_records_every_case() {
    let rom_path = rom_path();
    let suite_path = suite_path();
    if !rom_path.is_file() {
        panic!("48K ROM not found at {}", rom_path.display());
    }
    if !suite_path.is_file() {
        panic!(
            "timing suite not found at {} — set {SUITE_DIR_ENV}",
            suite_path.display()
        );
    }

    let suite_bytes = std::fs::read(&suite_path).expect("read timing suite");
    let actual_sha = sha256_hex(&suite_bytes);
    assert_eq!(
        actual_sha, SUITE_SHA256,
        "timing suite bytes changed; results are not comparable across revisions \
         until the pin is updated deliberately"
    );

    let rom = std::fs::read(&rom_path).expect("read 48K ROM");
    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(&rom).expect("48K ROM should load");
    let snapshot = parse_sna(&suite_bytes).expect("parse timing suite .sna");
    machine.apply_snapshot(&snapshot);
    run_frames(&mut machine, BOOT_FRAMES);

    // The suite's own machine classification, printed before the prompt.
    let boot_screen = screen(&machine);
    let timing_type = boot_screen
        .iter()
        .find(|l| l.contains("timings detected"))
        .map(|l| l.trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned());

    // One fresh machine per test, selected by number at the prompt.
    //
    // Driving all 35 from a single session was tried first and is the
    // wrong shape. The transcript scrolls, so a completed test's header
    // leaves a 24-line screen while later output is still printing; the
    // verdict prints before input is armed, so a continue-key can be
    // swallowed; and any single lost keypress truncates the whole
    // survey from that point. Selecting a test by number costs one boot
    // each — trivial at release speed — and makes every test
    // independent: clean screen, full `Expecting:` block, and a stall
    // affects one test instead of every test after it.
    let mut cases: Vec<CaseResult> = Vec::new();
    let mut incomplete = Vec::new();

    for test_number in 1..=TEST_COUNT {
        let mut machine = Spectrum48k::new();
        machine.load_rom_bytes(&rom).expect("48K ROM should load");
        machine.apply_snapshot(&snapshot);
        run_frames(&mut machine, BOOT_FRAMES);

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
        "survey": "zxspectrum4.net-timing-tests-48k",
        "revision": revision,
        "machine": "sinclair-zx-spectrum-48k",
        "suite_sha256": actual_sha,
        "suite_file": SUITE_FILE,
        "timing_classification": timing_type,
        "tests_covered": TEST_COUNT,
        "cases_recorded": cases.len(),
        "cases_failing": failures.len(),
        "tests_incomplete": incomplete,
        "cases": cases,
    });

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/accuracy/spectrum-timing-survey")
        .join(&revision)
        .join("report.json");
    write_report(&path, &report);

    println!("\n=== ZXSpectrum4.net timing survey @ {revision} ===");
    println!("  classification: {timing_type}");
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
    println!("  report: {}", path.display());

    // The ratchet. This survey ran for weeks reporting 36 failures and
    // returning `ok`, so no contention change could ever be scored by it
    // automatically — which is how `ad0e8c53` moved the 128K floating bus
    // by a T-state unnoticed (#851).
    //
    // This is the real-software oracle: 70 graded cases from
    // ZXSpectrum4.net running actual Z80 code, so it catches "closer to
    // FUSE in the abstract, worse on programs people ran". Treat a rise
    // here as more serious than a rise in the frame-wide differential.
    //
    // A ceiling, not a target. Lower it in the same commit that earns the
    // improvement; never raise it silently.
    //
    // 13, from 36 at the start of the contention work and 29 before the
    // window's phase and edge were derived rather than written out. Six
    // of the thirteen have a wrong loop count; the rest fail on `R` or
    // `SP` readings alone.
    //
    // **8**, earned by #880 charging each port class the number of
    // contention lookups FUSE charges it. Five of the six loop-count cases
    // went green together: test 32 (`INI/INIR/IND/INDR`) and test 35
    // (`IN A,(n); OUT (n),A`) in *both* passes, and test 33
    // (`OUTI/OTIR/OUTD/OTDR`) in its Contended pass. That is the group
    // `spectrum-accuracy-what-is-left.md` predicted, and predicted for the
    // right reason: the suite drives those cases with `B` in the contended
    // range, so their **Uncontended** pass was never uncontended — it was
    // I/O contention on a contended-page port, mislabelled by the suite.
    //
    // What is left has changed character, which is the more useful half of
    // this number. Seven of the eight survivors disagree on `R` or `SP`
    // alone — tests 1, 9, 11, 15, 23, 24 and 34, each off by one `R` — and
    // only test 33's Uncontended pass still has a loop count. The board is
    // no longer contention-shaped, so the next contention change should not
    // expect to move it.
    const RATCHET_FAILURES: usize = 8;
    assert!(
        failures.len() <= RATCHET_FAILURES,
        "timing survey regressed: {} of {} cases failing, was {RATCHET_FAILURES}. \
         The failing cases are listed above. If this change is right and the \
         suite's expectations are wrong, say which cases and why, and move the \
         ratchet in the same commit.",
        failures.len(),
        cases.len(),
    );
    if failures.len() < RATCHET_FAILURES {
        println!(
            "  RATCHET: {} of {} failing — improved on {RATCHET_FAILURES}. \
             Lower the constant in this commit.",
            failures.len(),
            cases.len(),
        );
    }

    assert!(
        incomplete.is_empty(),
        "tests did not complete within budget: {incomplete:?}"
    );
    let tests_covered: std::collections::BTreeSet<usize> = cases.iter().map(|c| c.test).collect();
    assert_eq!(
        tests_covered.len(),
        TEST_COUNT,
        "survey covered {} of {TEST_COUNT} tests. Under-coverage must fail \
         loudly: a harness that reports success having measured a fraction is \
         indistinguishable from one that measured everything. See \
         knowledge/decisions/a-gate-nobody-runs-is-a-silent-gate.md",
        tests_covered.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn parses_readings_from_a_result_line() {
        let r = parse_readings("R=43  loop=1201  sp=56806   Pass");
        assert_eq!(r.get("r"), Some(&43));
        assert_eq!(r.get("loop"), Some(&1201));
        assert_eq!(r.get("sp"), Some(&56806));
    }

    /// The transcript shape observed on real output: a passing
    /// uncontended case followed by a failing contended one carrying an
    /// `Expecting:` block.
    #[test]
    fn scrapes_a_pass_and_a_fail_with_expectations() {
        let lines: Vec<String> = [
            "Test 1 {Uncontended}",
            "JR; INC BC; LD BC,(nn);LD (nn),BC",
            "R=43  loop=1201  sp=56806   Pass",
            "------------------------------",
            "Test 1 {Contended}",
            "JR; INC BC; LD BC,(nn);LD (nn),BC",
            "R=100 loop=987 sp=23296    Fail",
            "Expecting:",
            "R=74  loop=1014",
            "------------------------------",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();

        let cases = scrape_cases(&lines);
        assert_eq!(cases.len(), 2, "both cases should be scraped: {cases:?}");

        assert_eq!(cases[0].mode, "Uncontended");
        assert_eq!(cases[0].verdict, "pass");
        assert_eq!(cases[0].measured.get("loop"), Some(&1201));
        assert!(cases[0].expected.is_empty(), "a pass prints no expectation");

        assert_eq!(cases[1].mode, "Contended");
        assert_eq!(cases[1].verdict, "fail");
        assert_eq!(cases[1].measured.get("r"), Some(&100));
        assert_eq!(cases[1].expected.get("r"), Some(&74));
        assert_eq!(cases[1].expected.get("loop"), Some(&1014));
    }
}
