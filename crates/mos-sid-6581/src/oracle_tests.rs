//! reSID oracle regression: settled filter response vs the real reSID.
//!
//! `tests/fixtures/resid_filter_reference.csv` is emitted by the reSID
//! reference oracle (`~/.emu198x/tools/resid-oracle/`, built from the
//! vendored VICE 3.10 reSID with `NEW_8580_FILTER=1` — the same
//! `filter8580new` op-amp model this crate ports). Each row is one scenario:
//! a settled voice-1 sawtooth driven through the filter, with the
//! steady-state peak-to-peak and DC mean of the raw 1 MHz output stream.
//!
//! The comparison is steady-state response, not bit-identity: our oscillator
//! and envelope are not bit-identical to reSID's, our DAC tables keep more
//! precision than reSID's `u16` rounding, and the voice-input dither uses a
//! different (deterministic) PRNG. What must match is the *filter*: gain vs
//! cutoff/resonance/mode/model.

use crate::{Sid6581, SidModel};

const FIXTURE: &str = include_str!("../tests/fixtures/resid_filter_reference.csv");

/// Cycles to settle the tone, filter state, and output-coupling high-pass.
/// Matches the oracle's `SETTLE_CYCLES`.
const SETTLE_CYCLES: usize = 300_000;
/// Steady-state measurement window. Matches the oracle's `WINDOW_CYCLES`.
const WINDOW_CYCLES: usize = 100_000;

struct Reference {
    model: SidModel,
    mode_bits: u8,
    filt: u8,
    fc: u16,
    res: u8,
    pp: i32,
    mean: f64,
}

fn parse_fixture() -> Vec<Reference> {
    FIXTURE
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split(',').collect();
            assert_eq!(fields.len(), 6, "malformed fixture row: {line}");
            let model = match fields[0] {
                "6581" => SidModel::Mos6581,
                "8580" => SidModel::Mos8580,
                other => panic!("unknown model {other}"),
            };
            let (mode_bits, filt) = match fields[1] {
                "direct" => (0x00, 0x00),
                "lp" => (0x10, 0x01),
                "bp" => (0x20, 0x01),
                "hp" => (0x40, 0x01),
                other => panic!("unknown mode {other}"),
            };
            Reference {
                model,
                mode_bits,
                filt,
                fc: fields[2].parse().expect("fc"),
                res: fields[3].parse().expect("res"),
                pp: fields[4].parse().expect("pp"),
                mean: fields[5].parse().expect("mean"),
            }
        })
        .collect()
}

/// Replicate one oracle scenario against the raw per-tick output stream
/// (the oracle reads `sid.output()` after every `sid.clock()`).
fn run_scene(reference: &Reference) -> (i32, f64) {
    let mut sid = Sid6581::new_with_model(985_248, 48_000, reference.model);

    // Same register sequence as the oracle's `run_scene`, one clock after
    // every write (a real CPU always has cycles between writes; reSID's
    // 8580 SAMPLE_FAST path even pipelines writes by one cycle).
    let writes = [
        (0x00, 0x00), // freq lo
        (0x01, 0x20), // freq hi = 0x2000, ~481 Hz PAL
        (0x05, 0x00), // AD = 0
        (0x06, 0xF0), // sustain F, release 0
        (0x04, 0x21), // sawtooth + gate
        (0x15, (reference.fc & 0x07) as u8),
        (0x16, (reference.fc >> 3) as u8),
        (0x17, (reference.res << 4) | reference.filt),
        (0x18, reference.mode_bits | 0x0F), // mode + volume 15
    ];
    for (reg, value) in writes {
        sid.write(reg, value);
        sid.tick();
    }

    for _ in 0..SETTLE_CYCLES {
        sid.tick();
    }

    let mut min = i32::MAX;
    let mut max = i32::MIN;
    let mut sum = 0i64;
    for _ in 0..WINDOW_CYCLES {
        sid.tick();
        let sample = sid.ext_filter.output();
        min = min.min(sample);
        max = max.max(sample);
        sum += i64::from(sample);
    }
    (max - min, sum as f64 / WINDOW_CYCLES as f64)
}

/// Peak-to-peak must match reSID within 2% relative or 24 counts absolute
/// (the absolute floor covers closed-filter scenarios where pp is a handful
/// of dither counts); the DC mean within 24 counts. Measured worst case over
/// the full battery: 0.45% relative / 4 counts / 19.9 mean.
fn check_values(reference: &Reference, pp: i32, mean: f64) -> Result<(), String> {
    let pp_tolerance = (f64::from(reference.pp) * 0.02).max(24.0);
    let pp_err = f64::from(pp - reference.pp).abs();
    let mean_err = (mean - reference.mean).abs();
    if pp_err > pp_tolerance || mean_err > 24.0 {
        return Err(format!(
            "{:?} mode={:#04x} fc={} res={}: pp={} want {} (err {:.0}), mean={:.1} want {:.1}",
            reference.model,
            reference.mode_bits,
            reference.fc,
            reference.res,
            pp,
            reference.pp,
            pp_err,
            mean,
            reference.mean,
        ));
    }
    Ok(())
}

fn run_battery(references: &[&Reference]) {
    let mut worst_rel = 0.0f64;
    let mut worst_small_abs = 0.0f64;
    let mut worst_mean = 0.0f64;
    let mut failures = Vec::new();
    for reference in references {
        let (pp, mean) = run_scene(reference);
        let pp_err = f64::from(pp - reference.pp).abs();
        let mean_err = (mean - reference.mean).abs();
        if reference.pp > 100 {
            worst_rel = worst_rel.max(pp_err / f64::from(reference.pp));
        } else {
            worst_small_abs = worst_small_abs.max(pp_err);
        }
        worst_mean = worst_mean.max(mean_err);
        if let Err(message) = check_values(reference, pp, mean) {
            failures.push(message);
        }
    }
    println!(
        "worst deviation vs reSID: pp {:.2}% relative, {worst_small_abs:.0} counts \
         absolute (small-pp scenes), mean {worst_mean:.1} counts",
        worst_rel * 100.0
    );
    assert!(
        failures.is_empty(),
        "{} of {} scenarios out of tolerance:\n{}",
        failures.len(),
        references.len(),
        failures.join("\n")
    );
}

/// Fast subset run in the default suite: every mode and model at the cutoff
/// extremes and midpoint, resonance off/max, plus the direct references.
#[test]
fn resid_oracle_subset() {
    let references = parse_fixture();
    let subset: Vec<&Reference> = references
        .iter()
        .filter(|r| (r.filt == 0) || ([0, 1024, 2047].contains(&r.fc) && [0, 15].contains(&r.res)))
        .collect();
    assert!(subset.len() >= 20, "subset unexpectedly small");
    run_battery(&subset);
}

/// Full 410-scenario battery (~6 s optimized); run explicitly with
/// `cargo test -p mos-sid-6581 resid_oracle_full -- --ignored`.
#[test]
#[ignore = "the always-on subset covers every mode/model; run for full sweeps"]
fn resid_oracle_full() {
    let references = parse_fixture();
    let all: Vec<&Reference> = references.iter().collect();
    run_battery(&all);
}
