//! The S-DSP against ares's, step for step.
//!
//! Each case in `oracle/cases/` sets up audio RAM and writes registers at
//! exact steps. The golden files hold what ares's DSP, compiled from its own
//! source, produced: a running FNV-1a over all 128 registers after every
//! step and each sample as it appears, every 2048 steps, then a hash of the
//! audio RAM. `oracle/regenerate.sh` rebuilds them.

use std::path::Path;

use emu198x_sony_s_dsp::SDsp;

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn byte(&mut self, b: u8) {
        self.0 ^= u64::from(b);
        self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
    }
}

struct Script {
    ram: Vec<(u16, Vec<u8>)>,
    events: Vec<(u64, u8, u8)>,
    end: u64,
}

fn parse(text: &str) -> Script {
    let mut script = Script {
        ram: Vec::new(),
        events: Vec::new(),
        end: 0,
    };
    let mut at = 0;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut words = line.split_whitespace();
        let Some(op) = words.next() else { continue };
        let hex = |s: &str| u32::from_str_radix(s, 16).expect("hex");
        match op {
            "ram" => {
                let addr = hex(words.next().expect("address")) as u16;
                script
                    .ram
                    .push((addr, words.map(|b| hex(b) as u8).collect()));
            }
            "at" => at = words.next().expect("step").parse().expect("step"),
            "w" => {
                let reg = hex(words.next().expect("reg")) as u8;
                let data = hex(words.next().expect("data")) as u8;
                script.events.push((at, reg, data));
            }
            "end" => script.end = words.next().expect("steps").parse().expect("steps"),
            other => panic!("unknown command {other}"),
        }
    }
    script
}

/// Run a case; the lines the harness would print, and the largest sample.
fn run(script: &Script) -> (Vec<String>, i32) {
    let mut dsp = SDsp::new();
    for (addr, bytes) in &script.ram {
        for (i, b) in bytes.iter().enumerate() {
            dsp.ram_mut()[usize::from(addr.wrapping_add(i as u16))] = *b;
        }
    }
    let mut events = script.events.iter().peekable();
    let mut hash = Fnv::new();
    let mut lines = Vec::new();
    let mut loudest = 0i32;
    let mut steps = 0u64;
    loop {
        while let Some((_, reg, data)) = events.next_if(|e| e.0 == steps) {
            dsp.write(*reg, *data);
        }
        dsp.tick();
        for addr in 0..128u8 {
            hash.byte(dsp.read(addr));
        }
        for (l, r) in dsp.take_samples() {
            loudest = loudest.max(i32::from(l).abs()).max(i32::from(r).abs());
            for b in l.to_le_bytes().into_iter().chain(r.to_le_bytes()) {
                hash.byte(b);
            }
        }
        steps += 1;
        if steps.is_multiple_of(2048) {
            lines.push(format!("{steps} {:016x}", hash.0));
        }
        if steps == script.end {
            break;
        }
    }
    let mut ram = Fnv::new();
    for b in dsp.ram() {
        ram.byte(*b);
    }
    lines.push(format!("ram {:016x}", ram.0));
    (lines, loudest)
}

#[test]
fn every_case_matches_ares() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle");
    let mut names: Vec<_> = std::fs::read_dir(dir.join("cases"))
        .expect("cases")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "dsp"))
        .collect();
    names.sort();
    assert!(names.len() >= 10, "the oracle cases are present");
    let mut failures = Vec::new();
    for case in &names {
        let name = case
            .file_stem()
            .expect("stem")
            .to_string_lossy()
            .into_owned();
        let script = parse(&std::fs::read_to_string(case).expect("case"));
        let golden = std::fs::read_to_string(dir.join("golden").join(format!("{name}.txt")))
            .expect("golden file");
        let (lines, loudest) = run(&script);
        let want: Vec<&str> = golden.lines().collect();
        if let Some(i) = (0..want.len().max(lines.len()))
            .find(|&i| want.get(i).copied() != lines.get(i).map(String::as_str))
        {
            failures.push(format!(
                "{name}: line {i}: ares {:?}, ours {:?}",
                want.get(i),
                lines.get(i)
            ));
        }
        assert!(loudest > 256, "{name} makes sound (loudest {loudest})");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
