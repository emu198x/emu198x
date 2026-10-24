//! The chip against Nuked-OPN2, clock by clock.
//!
//! Each script in `oracle/cases/` drives the chip through one area of its
//! behaviour. `oracle/golden/` holds what Nuked-OPN2 printed for the same
//! script: a hash of the MOL, MOR and IRQ pins over every block of 4096
//! internal clocks, and every value read. This test runs the script on the
//! crate and demands the same output, so a single wrong pin on a single
//! clock fails it and the report names the block. `oracle/regenerate.sh`
//! rebuilds the golden files; see `oracle/make_cases.py` for the format.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use emu198x_yamaha_ym2612::{Variant, Ym2612};

const BLOCK: u64 = 4096;

struct Runner {
    chip: Ym2612,
    hash: u64,
    clocks: u64,
    block_index: u64,
    reads: u64,
    out: String,
}

impl Runner {
    fn new() -> Self {
        Self {
            chip: Ym2612::new(Variant::Ym3438),
            hash: 0xcbf2_9ce4_8422_2325,
            clocks: 0,
            block_index: 0,
            reads: 0,
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
            let [mol_lo, mol_hi] = self.chip.mol.to_le_bytes();
            let [mor_lo, mor_hi] = self.chip.mor.to_le_bytes();
            for byte in [mol_lo, mol_hi, mor_lo, mor_hi, u8::from(self.chip.irq())] {
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
                    "ym2612" => Variant::Ym2612,
                    "ym3438" => Variant::Ym3438,
                    other => panic!("unknown variant {other}"),
                };
                self.chip = Ym2612::new(variant);
            }
            Some("readmode") => self.chip.status_on_all_ports = words[1] == "all",
            Some("w") => self.chip.write(num(1) as u8, num(2) as u8),
            Some("c") => self.run(num(1)),
            Some("r") => {
                let value = self.chip.read(num(1) as u8);
                let _ = writeln!(self.out, "r {} {value}", self.reads);
                self.reads += 1;
            }
            Some("pin") => self.chip.set_test_pin(num(1) != 0),
            Some("reg") => {
                let part = num(1) as u8;
                self.chip.write(part * 2, num(2) as u8);
                self.run(1);
                self.chip.write(part * 2 + 1, num(3) as u8);
                self.run(32);
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
fn every_script_matches_nuked_opn2_clock_for_clock() {
    let mut scripts: Vec<PathBuf> = std::fs::read_dir(oracle_dir().join("cases"))
        .expect("cases should list")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ym"))
        .collect();
    scripts.sort();
    assert_eq!(scripts.len(), 24, "twelve cases on two variants");

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
