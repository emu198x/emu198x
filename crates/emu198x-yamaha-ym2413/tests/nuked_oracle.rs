//! The chip against Nuked-OPLL, clock by clock.
//!
//! Each script in `oracle/cases/` drives the chip through one area of its
//! behaviour, on each variant. `oracle/golden/` holds what Nuked-OPLL printed
//! for the same script: a hash of the melody and rhythm pins over every
//! block of 4096 internal clocks. This test runs the script on the crate and
//! demands the same output, so a single wrong pin on a single clock fails it
//! and the report names the block. `oracle/regenerate.sh` rebuilds the golden
//! files; see `oracle/make_cases.py` for the format.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use emu198x_yamaha_ym2413::{Variant, Ym2413};

const BLOCK: u64 = 4096;

struct Runner {
    chip: Ym2413,
    hash: u64,
    clocks: u64,
    block_index: u64,
    out: String,
}

impl Runner {
    fn new() -> Self {
        Self {
            chip: Ym2413::new(Variant::Ym2413),
            hash: 0xcbf2_9ce4_8422_2325,
            clocks: 0,
            block_index: 0,
            out: String::new(),
        }
    }

    fn fnv(&mut self, byte: u8) {
        self.hash ^= u64::from(byte);
        self.hash = self.hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn run(&mut self, n: u64) {
        for _ in 0..n {
            self.chip.clock();
            let [mo_lo, mo_hi] = self.chip.mo.to_le_bytes();
            let [ro_lo, ro_hi] = self.chip.ro.to_le_bytes();
            for byte in [mo_lo, mo_hi, ro_lo, ro_hi] {
                self.fnv(byte);
            }
            self.clocks += 1;
            if self.clocks.is_multiple_of(BLOCK) {
                let _ = writeln!(self.out, "b {} {:016x}", self.block_index, self.hash);
                self.block_index += 1;
                self.hash = 0xcbf2_9ce4_8422_2325;
            }
        }
    }

    fn execute(&mut self, line: &str) {
        let words: Vec<&str> = line.split_whitespace().collect();
        let num = |i: usize| -> u64 {
            let text = words[i];
            match text.strip_prefix("0x") {
                Some(hex) => u64::from_str_radix(hex, 16),
                None => text.parse(),
            }
            .unwrap_or_else(|_| panic!("bad number in `{line}`"))
        };
        match words.first().copied() {
            None => {}
            Some("variant") => {
                let variant = match words[1] {
                    "ym2413" => Variant::Ym2413,
                    "vrc7" => Variant::Vrc7,
                    other => panic!("unknown variant {other}"),
                };
                self.chip = Ym2413::new(variant);
            }
            Some("w") => self.chip.write(num(1) as u8, num(2) as u8),
            Some("c") => self.run(num(1)),
            Some("reg") => {
                self.chip.write(0, num(1) as u8);
                self.run(3);
                self.chip.write(1, num(2) as u8);
                self.run(21);
            }
            Some(other) => panic!("unknown command {other}"),
        }
    }

    fn finish(mut self) -> String {
        let _ = writeln!(self.out, "e {} {:016x}", self.clocks % BLOCK, self.hash);
        self.out
    }
}

fn oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle")
}

fn run_script(path: &Path) -> String {
    let script = std::fs::read_to_string(path).expect("script should read");
    let mut runner = Runner::new();
    for line in script.lines() {
        runner.execute(line);
    }
    runner.finish()
}

#[test]
fn every_script_matches_nuked_opll_clock_for_clock() {
    let mut scripts: Vec<PathBuf> = std::fs::read_dir(oracle_dir().join("cases"))
        .expect("cases should list")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "opll"))
        .collect();
    scripts.sort();
    assert_eq!(scripts.len(), 28, "fourteen cases on two variants");

    let mut failures = Vec::new();
    for script in &scripts {
        let name = script.file_stem().and_then(|s| s.to_str()).expect("name");
        let golden =
            std::fs::read_to_string(oracle_dir().join("golden").join(format!("{name}.txt")))
                .expect("golden file should read");
        let actual = run_script(script);
        if actual != golden {
            let first = golden
                .lines()
                .zip(actual.lines())
                .find(|(want, got)| want != got)
                .map_or_else(
                    || {
                        format!(
                            "line counts differ: {} vs {}",
                            golden.lines().count(),
                            actual.lines().count()
                        )
                    },
                    |(want, got)| format!("expected `{want}`, got `{got}`"),
                );
            failures.push(format!("{name}: {first}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} scripts diverge:\n{}",
        failures.len(),
        scripts.len(),
        failures.join("\n")
    );
}
