//! The ADPCM units against ymfm, sample by sample.
//!
//! Each script in `oracle/cases/` drives ADPCM-A and ADPCM-B through one
//! area of their behaviour. `oracle/golden/` holds what ymfm's own engines
//! printed for the same script: a hash of the mixed output, ADPCM-A's end
//! bits and ADPCM-B's status over every block of 1024 samples. This test
//! runs the script on the crate and demands the same output, so one wrong
//! value on one sample fails it and the report names the block.
//! `oracle/regenerate.sh` rebuilds the golden files; see
//! `oracle/make_cases.py` for the format.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use emu198x_yamaha_ym2610::adpcm::{AdpcmA, AdpcmB};

const BLOCK: u64 = 1024;
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

struct Runner {
    rom: Vec<u8>,
    a: AdpcmA,
    b: AdpcmB,
    phase: u8,
    hash: u64,
    samples: u64,
    block_index: u64,
    out: String,
}

impl Runner {
    fn new() -> Self {
        Self {
            rom: Vec::new(),
            a: AdpcmA::new(),
            b: AdpcmB::new(),
            phase: 0,
            hash: FNV_OFFSET,
            samples: 0,
            block_index: 0,
            out: String::new(),
        }
    }

    fn fnv(&mut self, byte: u8) {
        self.hash ^= u64::from(byte);
        self.hash = self.hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn step(&mut self) {
        self.phase += 1;
        let ended = if self.phase == 3 {
            self.phase = 0;
            self.a.clock(&self.rom)
        } else {
            0
        };
        self.b.clock(&self.rom);
        let a = self.a.output();
        let b = self.b.output();
        for side in 0..2 {
            for byte in (a[side] + b[side]).to_le_bytes() {
                self.fnv(byte);
            }
        }
        self.fnv(ended);
        self.fnv(self.b.status());
        self.samples += 1;
        if self.samples.is_multiple_of(BLOCK) {
            let _ = writeln!(self.out, "b {} {:016x}", self.block_index, self.hash);
            self.block_index += 1;
            self.hash = FNV_OFFSET;
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
            Some("rom") => {
                let mut state = num(1) as u32;
                self.rom = (0..num(2))
                    .map(|_| {
                        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                        (state >> 16) as u8
                    })
                    .collect();
            }
            Some("a") => self.a.write(num(1) as u8, num(2) as u8),
            Some("b") => self.b.write(num(1) as u8, num(2) as u8),
            Some("s") => {
                for _ in 0..num(1) {
                    self.step();
                }
            }
            Some(other) => panic!("unknown command {other}"),
        }
    }

    fn finish(mut self) -> String {
        let _ = writeln!(self.out, "e {} {:016x}", self.samples % BLOCK, self.hash);
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
fn every_script_matches_ymfm_sample_for_sample() {
    let mut scripts: Vec<PathBuf> = std::fs::read_dir(oracle_dir().join("cases"))
        .expect("cases should list")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "adpcm"))
        .collect();
    scripts.sort();
    assert_eq!(scripts.len(), 9, "nine cases");

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
