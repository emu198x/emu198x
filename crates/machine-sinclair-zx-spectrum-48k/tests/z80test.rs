//! Patrik Rak's `z80test` exerciser, run on a real 48K Spectrum.
//!
//! For each TAP we parse out the CODE block, inject it into RAM at the address
//! recorded in the TAP CODE header (always `$8000` for z80test), then jump to
//! it from the booted-to-READY ROM environment. The exerciser uses the
//! Spectrum ROM's `PRINT-A-1` routine at `$0010` (RST 16) to print results;
//! we trap PC entries at `$0010` to capture the printed characters into a
//! transcript, and assert that the final `Result:` line says
//! `all tests passed.`
//!
//! Upstream: <https://github.com/raxoft/z80test> (MIT, Patrik Rak).
//! Reference catalogue: `_organised/by-topic/testing-suites/spectrum-test-roms.md`
//! in `~/Projects/Emu198x-Reference`.
//!
//! The seven TAPs (`z80full`, `z80doc`, `z80flags`, `z80docflags`, `z80ccf`,
//! `z80memptr`, plus the visual-only `z80ccfscr`) are expected at:
//!
//! 1. `$EMU198X_Z80TEST_DIR/<name>.tap` if the env var is set, otherwise
//! 2. `~/.emu198x/test-data/z80test/<name>.tap` if present, otherwise
//! 3. `~/Projects/Emu198x-Unclean/Zen/Other Images/<name>.tap` as a fallback.
//!
//! Each test is `#[ignore]`d by default because it requires both the ROM and
//! the TAP corpus. Run with:
//!
//! ```sh
//! cargo test --release -p machine-sinclair-zx-spectrum-48k --test z80test -- --ignored --nocapture
//! ```

use common_sinclair_zx_spectrum::memory::MemoryBus;
use format_sinclair_zx_spectrum_tap::parse_tap;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use std::path::PathBuf;

const Z80TEST_DIR_ENV: &str = "EMU198X_Z80TEST_DIR";
const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";
const BOOT_FRAMES: usize = 200;

/// Per-test t-state budget. The longest exerciser (`z80full`) emits ~80
/// individual test cases each doing substantial computation; on a real
/// Spectrum at 3.5 MHz a full run takes several minutes. 2 B t-states ≈ 570 s
/// emulated, comfortably over the worst case.
const MAX_TSTATES: u64 = 2_000_000_000;

/// Step granularity inside the run loop. Each RST 16 instruction is at least
/// 4 t-states (M1 fetch), so a step of 4 t-states is guaranteed to land on
/// PC = `RST10_ADDR` at most one iteration after the RST executes.
const STEP_TSTATES: u32 = 4;

const RST10_ADDR: u16 = 0x0010;
const SENTINEL_RET_ADDR: u16 = 0xFFFE;

/// Spectrum system variable `SCR_CT` — the scroll-prompt counter. The ROM
/// decrements it on each line break and prints "scroll?" (waiting for a key)
/// when it hits 1. The test prints ~40 lines, well past one screenful, so we
/// have to keep this high or the harness hangs forever.
const SCR_CT_ADDR: u16 = 0x5C8C;

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME must be set for test fixture lookup"))
}

fn rom_path() -> PathBuf {
    std::env::var_os(ROM_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom"))
}

fn z80test_tap_path(name: &str) -> Option<PathBuf> {
    let filename = format!("{name}.tap");

    let mut candidates = Vec::new();
    if let Some(dir) = std::env::var_os(Z80TEST_DIR_ENV) {
        candidates.push(PathBuf::from(dir).join(&filename));
    }
    candidates.push(home().join(".emu198x/test-data/z80test").join(&filename));
    candidates.push(
        home()
            .join("Projects/Emu198x-Unclean/Zen/Other Images")
            .join(&filename),
    );

    candidates.into_iter().find(|path| path.is_file())
}

/// Parses a z80test TAP into (`load_address`, `code_bytes`).
///
/// z80test TAPs are four-block: BASIC header, BASIC loader, CODE header, CODE
/// data. A Spectrum header is a 17-byte payload (flag and checksum already
/// stripped by `parse_tap`): one type byte (`0x03` = CODE), ten filename
/// bytes, two length bytes, and two pairs of parameters. For CODE the first
/// parameter (bytes 13–14, little-endian) is the load address.
fn extract_code_block(tap_bytes: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let blocks = parse_tap(tap_bytes).map_err(|e| format!("TAP parse: {e}"))?;

    let mut load_addr: Option<u16> = None;
    let mut code: Option<Vec<u8>> = None;

    for block in &blocks {
        if block.is_header() && block.data.len() >= 17 && block.data[0] == 0x03 {
            load_addr = Some(u16::from_le_bytes([block.data[13], block.data[14]]));
        } else if !block.is_header() && load_addr.is_some() && code.is_none() {
            code = Some(block.data.clone());
        }
    }

    match (load_addr, code) {
        (Some(addr), Some(bytes)) => Ok((addr, bytes)),
        _ => Err("TAP did not contain a CODE header followed by a CODE data block".to_string()),
    }
}

/// Outcome of capturing the exerciser's screen output up to its `Result:` line.
#[derive(Debug)]
struct TestOutcome {
    transcript: String,
    tstates: u64,
}

impl TestOutcome {
    /// True when the transcript ends in the canonical success sentence.
    fn passed(&self) -> bool {
        self.transcript.contains("all tests passed.")
    }
}

/// Runs one TAP. Returns `None` when the ROM or TAP fixture isn't available
/// (the test silently skips). Returns `Some(outcome)` once the exerciser
/// either prints its final `Result:` line, returns to the sentinel, or
/// exhausts `MAX_TSTATES`.
fn run_one(name: &str) -> Option<TestOutcome> {
    let rom_path = rom_path();
    if !rom_path.is_file() {
        emu198x_test_skip::record(&format!(
            "48K ROM not found at {} — skipping {name}",
            rom_path.display()
        ));
        return None;
    }

    let Some(tap_path) = z80test_tap_path(name) else {
        emu198x_test_skip::record(&format!(
            "{name}.tap not found (set {Z80TEST_DIR_ENV} or place under ~/.emu198x/test-data/z80test/) — skipping"
        ));
        return None;
    };

    let rom = std::fs::read(&rom_path).expect("48K ROM should read");
    let tap = std::fs::read(&tap_path).expect("z80test TAP should read");
    let (load_addr, code) = extract_code_block(&tap).expect("z80test TAP CODE block");

    let mut machine = Spectrum48k::new();
    machine
        .load_rom_bytes(&rom)
        .expect("48K ROM image should load");
    machine.reset();

    // Boot the ROM to the READY prompt so CHANS / sys vars are initialised —
    // z80test's printinit calls CHAN-OPEN ($1601) which needs that state.
    for _ in 0..BOOT_FRAMES {
        machine.run_frame();
    }

    // Inject the test binary at its requested load address.
    for (offset, &byte) in code.iter().enumerate() {
        let addr = load_addr.wrapping_add(offset as u16);
        machine.write(addr, byte);
    }

    // Land on an instruction boundary before hijacking PC.
    //
    // `run_frame` stops when the frame's half-cycle budget runs out, which
    // bears no relationship to where the CPU is inside an instruction. If we
    // write `regs.pc` while an instruction is still in flight, the core
    // finishes that instruction against the *new* PC: its remaining operand
    // reads come out of the freshly injected binary, and it lands wherever
    // those bytes send it.
    //
    // That is exactly what happened at `5bbea2f2`. The boot ended
    // mid-instruction, the in-flight instruction swallowed z80test's first
    // eight bytes (`di`, `push iy`, `exx`, `push hl`, `call printinit`) and
    // execution resumed at $8008 — so `printinit` never ran, `CHAN-OPEN`
    // never selected the upper screen, and every subsequent scroll grew the
    // *lower* screen until the ROM ran out of room and sat in `WAIT-KEY`
    // forever. The test reported "did not produce a Result line", which
    // looked like a CPU timing regression and was not one. See #943.
    //
    // Nothing about that was new in `5bbea2f2`; it only moved the frame
    // boundary relative to instruction boundaries, turning a latent coin
    // flip from heads to tails. Waiting for the start of an opcode fetch
    // (`m1` going low to high) removes the coin flip.
    {
        let mut prev_m1 = machine.z80().m1;
        let mut guard = 0;
        loop {
            machine.advance_tstates(1);
            let m1 = machine.z80().m1;
            if m1 && !prev_m1 {
                break;
            }
            prev_m1 = m1;
            guard += 1;
            assert!(
                guard < 256,
                "no opcode fetch began within 256 t-states of the boot ending;                  the CPU is not running"
            );
        }
    }

    // Set up an entry that looks like a RANDOMIZE USR call: PC = load address,
    // a sentinel return address on top of stack so the test's final RET lands
    // somewhere we can trap. Disable interrupts in case one fires before the
    // test's own `di` instruction executes.
    {
        let sp = machine.z80().regs.sp.wrapping_sub(2);
        machine.write(sp, (SENTINEL_RET_ADDR & 0xFF) as u8);
        machine.write(sp.wrapping_add(1), (SENTINEL_RET_ADDR >> 8) as u8);

        let z80 = machine.z80_mut();
        z80.regs.sp = sp;
        z80.regs.pc = load_addr;
        z80.regs.iff1 = false;
        z80.regs.iff2 = false;
    }

    let mut transcript = String::new();
    let mut tstates: u64 = 0;
    let mut prev_pc_at_rst10 = machine.z80().regs.pc == RST10_ADDR;

    while tstates < MAX_TSTATES {
        machine.advance_tstates(STEP_TSTATES);
        tstates += u64::from(STEP_TSTATES);

        let z80 = machine.z80();

        if z80.regs.pc == SENTINEL_RET_ADDR {
            break;
        }

        // Detect entry into PRINT-A-1 at $0010. PC sits at $0010 for the
        // duration of the M1 fetch (at least one iteration of this loop),
        // so checking it after each step is enough — we just need to debounce
        // so a multi-iteration stay at $0010 doesn't double-count.
        let at_rst10 = z80.regs.pc == RST10_ADDR;
        if at_rst10 && !prev_pc_at_rst10 {
            let ch = z80.regs.a();
            // Filter to printable ASCII + CR; the Spectrum charset is ASCII for
            // 0x20..=0x7F, with 0x0D as ENTER (line break).
            match ch {
                0x0D => {
                    eprintln!();
                    transcript.push('\n');
                }
                0x20..=0x7E => {
                    eprint!("{}", ch as char);
                    transcript.push(ch as char);
                }
                _ => {
                    // Skip Spectrum control codes (AT, INK, PAPER, etc.).
                }
            }

            // Suppress the "scroll?" prompt. The ROM decrements SCR_CT on
            // each line break and waits on a keypress when it reaches 1; we
            // never feed keys so the suite would hang. Setting it back to
            // 0xFF after every printed char is the cheapest reliable fix.
            machine.write(SCR_CT_ADDR, 0xFF);

            // Stop as soon as we've seen a complete Result: line.
            if transcript.contains("Result: ")
                && (transcript.contains("all tests passed.")
                    || transcript.contains("tests failed."))
            {
                // Read out the rest of the result line for the transcript.
                break;
            }
        }
        prev_pc_at_rst10 = at_rst10;
    }

    Some(TestOutcome {
        transcript,
        tstates,
    })
}

fn assert_passed(name: &str, outcome: &TestOutcome) {
    assert_passed_with_allowlist(name, outcome, &[]);
}

/// Assert with an allowlist of expected per-test failures (matching by
/// substring on the printed test name). Used for [`z80memptr`] which has two
/// permanently-failing cases that mirror the FUSE INIR/INDR disagreements
/// already documented in `knowledge/tests/spectrum.md` — those disagreements come
/// from a long-standing dispute in Z80 emulation references about MEMPTR
/// behaviour after block I/O. Tom Harte agrees with our current behaviour;
/// FUSE and Patrik Rak's z80memptr disagree on the same cases. Until the
/// underlying behaviour question is resolved against silicon evidence, this
/// test treats the named failures as acknowledged and fails loudly on any
/// other shape of disagreement.
fn assert_passed_with_allowlist(name: &str, outcome: &TestOutcome, allowed_failures: &[&str]) {
    assert!(
        outcome.transcript.contains("Result: "),
        "{name}: did not produce a Result line within {} t-states\n--- transcript ---\n{}",
        outcome.tstates,
        outcome.transcript,
    );

    if outcome.passed() {
        eprintln!(
            "\n{name}: PASS (after {} t-states, ~{:.1}s emulated)",
            outcome.tstates,
            outcome.tstates as f64 / 3_500_000.0,
        );
        return;
    }

    // Collect every per-test FAILED line so we can compare against the allowlist.
    let observed_failures: Vec<String> = outcome
        .transcript
        .lines()
        .filter(|line| line.contains("FAILED"))
        .map(|line| line.trim().to_string())
        .collect();

    let allowed_observed: Vec<&String> = observed_failures
        .iter()
        .filter(|line| {
            allowed_failures
                .iter()
                .any(|allowed| line.contains(allowed))
        })
        .collect();
    let unexpected: Vec<&String> = observed_failures
        .iter()
        .filter(|line| {
            !allowed_failures
                .iter()
                .any(|allowed| line.contains(allowed))
        })
        .collect();

    let expected_present: Vec<&str> = allowed_failures
        .iter()
        .copied()
        .filter(|allowed| observed_failures.iter().any(|line| line.contains(allowed)))
        .collect();
    let expected_missing: Vec<&str> = allowed_failures
        .iter()
        .copied()
        .filter(|allowed| !observed_failures.iter().any(|line| line.contains(allowed)))
        .collect();

    assert!(
        unexpected.is_empty(),
        "{name}: unexpected failures (not on allowlist): {:?}\nallowlist matched: {:?}\n--- transcript ---\n{}",
        unexpected,
        allowed_observed,
        outcome.transcript,
    );

    assert!(
        expected_missing.is_empty(),
        "{name}: allowlisted failures no longer present — investigate whether the underlying behaviour changed. Missing: {:?}, still failing: {:?}\n--- transcript ---\n{}",
        expected_missing,
        expected_present,
        outcome.transcript,
    );

    eprintln!(
        "\n{name}: PASS with {} allowlisted failure(s) (after {} t-states, ~{:.1}s emulated)",
        allowed_observed.len(),
        outcome.tstates,
        outcome.tstates as f64 / 3_500_000.0,
    );
}

// ---------------------------------------------------------------------------
// The seven exercisers. `z80ccfscr` is visual-only (no pass/fail) and not
// included as a #[test].
// ---------------------------------------------------------------------------

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and the z80test corpus; runs for ~minute"]
fn z80doc() {
    let Some(outcome) = run_one("z80doc") else {
        emu198x_test_skip::skip!("z80test corpus not staged");
    };
    assert_passed("z80doc", &outcome);
}

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and the z80test corpus; runs for ~minute"]
fn z80docflags() {
    let Some(outcome) = run_one("z80docflags") else {
        emu198x_test_skip::skip!("z80test corpus not staged");
    };
    assert_passed("z80docflags", &outcome);
}

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and the z80test corpus; runs for ~minute"]
fn z80flags() {
    let Some(outcome) = run_one("z80flags") else {
        emu198x_test_skip::skip!("z80test corpus not staged");
    };
    assert_passed("z80flags", &outcome);
}

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and the z80test corpus; runs for ~minute"]
fn z80full() {
    let Some(outcome) = run_one("z80full") else {
        emu198x_test_skip::skip!("z80test corpus not staged");
    };
    assert_passed("z80full", &outcome);
}

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and the z80test corpus; runs for ~minute"]
fn z80ccf() {
    let Some(outcome) = run_one("z80ccf") else {
        emu198x_test_skip::skip!("z80test corpus not staged");
    };
    assert_passed("z80ccf", &outcome);
}

/// Allowlist for `z80memptr` failures. The 2026-05-31 fix to stop
/// the INIR/INDR/OTIR/OTDR repeat path from clobbering WZ closed
/// the `102 INIR->NOP'` and `103 INDR->NOP'` cases — they now pass
/// cleanly along with the rest of the suite. Empty slice retained
/// so the regression contract still flows through the assert.
const Z80MEMPTR_ALLOWLIST: &[&str] = &[];

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and the z80test corpus; runs for ~minute"]
fn z80memptr() {
    let Some(outcome) = run_one("z80memptr") else {
        emu198x_test_skip::skip!("z80test corpus not staged");
    };
    assert_passed_with_allowlist("z80memptr", &outcome, Z80MEMPTR_ALLOWLIST);
}

// ---------------------------------------------------------------------------
// Unit tests for the TAP parser invariants — these run on every cargo test
// without fixtures.
// ---------------------------------------------------------------------------

#[test]
fn extract_code_block_skips_basic_header_and_loader() {
    // Synthetic four-block TAP: BASIC header, BASIC data, CODE header, CODE data.
    // Spectrum header payload is 17 bytes: type(1) + name(10) + length(2) + 2 + 2.
    let mut tap = Vec::new();

    let mut basic_hdr = vec![0u8; 17];
    basic_hdr[0] = 0x00; // BASIC type
    push_block(&mut tap, 0x00, &basic_hdr);

    push_block(&mut tap, 0xFF, &[0u8; 16]);

    let mut code_hdr = vec![0u8; 17];
    code_hdr[0] = 0x03; // CODE type
    code_hdr[13] = 0x00; // load addr lo
    code_hdr[14] = 0x90; // load addr hi → 0x9000
    push_block(&mut tap, 0x00, &code_hdr);

    let payload: Vec<u8> = (0..32).collect();
    push_block(&mut tap, 0xFF, &payload);

    let (load_addr, code) = extract_code_block(&tap).expect("synthetic TAP should parse");
    assert_eq!(load_addr, 0x9000);
    assert_eq!(code, payload);
}

#[test]
fn extract_code_block_errors_without_code_header() {
    let mut tap = Vec::new();
    push_block(&mut tap, 0xFF, &[1, 2, 3]);
    let err = extract_code_block(&tap).expect_err("TAP without CODE header should error");
    assert!(err.contains("CODE"), "error mentions CODE: {err}");
}

fn push_block(tap: &mut Vec<u8>, flag: u8, body: &[u8]) {
    // Length prefix counts the flag byte + body + checksum byte.
    let len = (1 + body.len() + 1) as u16;
    tap.extend_from_slice(&len.to_le_bytes());
    tap.push(flag);
    tap.extend_from_slice(body);
    // Checksum: XOR of flag and body. The parse_tap reader strips it.
    let mut sum = flag;
    for &b in body {
        sum ^= b;
    }
    tap.push(sum);
}
