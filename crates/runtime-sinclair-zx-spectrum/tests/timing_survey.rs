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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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

fn sha256_hex(bytes: &[u8]) -> String {
    // Small local SHA-256 so the survey can pin its fixture without the
    // crate taking a dependency for one hash.
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let mut msg = bytes.to_vec();
    let bit_len = (bytes.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d) = (h[0], h[1], h[2], h[3]);
        let (mut e, mut f, mut g, mut hh) = (h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(v);
        }
    }
    h.iter().map(|w| format!("{w:08x}")).collect()
}

fn revision() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |s| s.trim().to_owned())
}

fn set_key(rows: &mut [u8; 8], key: SpectrumKey, pressed: bool) {
    let (row, bit) = key.row_bit();
    let mask = 1u8 << bit;
    if pressed {
        rows[row] &= !mask;
    } else {
        rows[row] |= mask;
    }
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

/// One `{Contended}` or `{Uncontended}` case within a numbered test.
#[derive(Debug, Clone, serde::Serialize)]
struct CaseResult {
    test: usize,
    mode: String,
    description: String,
    verdict: String,
    measured: BTreeMap<String, i64>,
    expected: BTreeMap<String, i64>,
}

/// Pull `R=`, `loop=` and `sp=` style readings out of one decoded line.
fn parse_readings(line: &str) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    for token in line.split_whitespace() {
        if let Some((name, value)) = token.split_once('=')
            && let Ok(parsed) = value
                .trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse()
            && !name.is_empty()
        {
            out.insert(name.to_ascii_lowercase(), parsed);
        }
    }
    out
}

/// Scrape every case visible on the current screen.
///
/// The suite prints a running transcript, so a screen can hold several
/// cases at once. Each begins `Test N {Mode}`, carries an opcode
/// description, then a readings line ending `Pass` or `Fail`; a failing
/// case follows with `Expecting:` and a second readings line.
fn scrape_cases(lines: &[String]) -> Vec<CaseResult> {
    let mut cases = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        let Some(rest) = line.strip_prefix("Test ") else {
            i += 1;
            continue;
        };
        let Some((num, mode)) = rest.split_once('{') else {
            i += 1;
            continue;
        };
        let Ok(test) = num.trim().parse::<usize>() else {
            i += 1;
            continue;
        };
        let mode = mode.trim_end_matches('}').trim().to_owned();

        // Description and readings follow within the next few lines.
        let mut description = String::new();
        let mut verdict = String::new();
        let mut measured = BTreeMap::new();
        let mut expected = BTreeMap::new();
        let mut j = i + 1;
        while j < lines.len() && j < i + 8 {
            let l = lines[j].trim();
            if l.starts_with("Test ") {
                break;
            }
            if l.contains("Pass") || l.contains("Fail") {
                verdict = if l.contains("Fail") { "fail" } else { "pass" }.to_owned();
                measured = parse_readings(l);
            } else if l.starts_with("Expecting") {
                if let Some(next) = lines.get(j + 1) {
                    expected = parse_readings(next.trim());
                }
            } else if !l.is_empty() && !l.starts_with('-') && description.is_empty() {
                description = l.to_owned();
            }
            j += 1;
        }

        if !verdict.is_empty() {
            cases.push(CaseResult {
                test,
                mode,
                description,
                verdict,
                measured,
                expected,
            });
        }
        i = j;
    }
    cases
}

/// Merge freshly scraped cases into the accumulated set.
///
/// A case can be sampled while it is still printing, so a later sample
/// of the same `(test, mode)` may carry readings the earlier one lacked
/// — most often the `Expecting:` block, which prints after the verdict.
/// Keep whichever version knows more.
/// Keys spelling a test number at the suite's "choose test 1-35" prompt.
fn digit_keys(mut n: usize) -> Vec<SpectrumKey> {
    let digits = [
        SpectrumKey::Num0,
        SpectrumKey::Num1,
        SpectrumKey::Num2,
        SpectrumKey::Num3,
        SpectrumKey::Num4,
        SpectrumKey::Num5,
        SpectrumKey::Num6,
        SpectrumKey::Num7,
        SpectrumKey::Num8,
        SpectrumKey::Num9,
    ];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    if n == 0 {
        stack.push(0);
    }
    while n > 0 {
        stack.push(n % 10);
        n /= 10;
    }
    while let Some(d) = stack.pop() {
        out.push(digits[d]);
    }
    out
}

fn absorb(into: &mut Vec<CaseResult>, fresh: Vec<CaseResult>) {
    for case in fresh {
        if case.verdict.is_empty() {
            continue;
        }
        match into
            .iter_mut()
            .find(|c| c.test == case.test && c.mode == case.mode)
        {
            Some(existing) => {
                if case.expected.len() > existing.expected.len()
                    || case.measured.len() > existing.measured.len()
                {
                    *existing = case;
                }
            }
            None => into.push(case),
        }
    }
}

fn write_report(path: &Path, body: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("report directory");
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(body).expect("serialize report"),
    )
    .expect("write report");
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
    const RATCHET_FAILURES: usize = 36;
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
