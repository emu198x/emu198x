//! blargg's `dmc_tests`, gated on the ROM's own **audible** result code.
//!
//! ⚠ These four were the last ROMs in the corpus with no gate of any
//! kind, and the campaign record named the only remaining option as a
//! DMC state trace compared against Mesen2 — which would have made
//! Mesen2, rather than the test's author, the definition of correct.
//! That is not necessary. blargg's shell reports its result code
//! audibly, and the encoding is published (`ppu_open_bus/readme.txt`):
//!
//! > A byte is reported as a series of tones. The code is in binary,
//! > with a low tone for 0 and a high tone for 1, and with leading
//! > zeroes skipped. The first tone is always a zero. A final code of 0
//! > means passed, 1 means failure, and 2 or higher indicates a
//! > specific reason.
//!
//! | Tones | Binary | Code |
//! |---|---|---|
//! | low | `0` | 0 — passed |
//! | low high | `01` | 1 — failed |
//! | low high low | `010` | 2 |
//! | low high high | `011` | 3 |
//!
//! ⚠ **This gate reads the code's LENGTH, not its value**, and that is
//! deliberate. Length alone separates pass from fail: code 0 is one
//! tone and every non-zero code is two or more, because the encoding
//! skips leading zeroes and always starts with one. So "exactly one
//! tone" is exactly "passed", which is the verdict a gate needs.
//!
//! Decoding the *value* would need low/high classification, and that was
//! attempted and abandoned. Within one ROM the two tones are an octave
//! apart (`6-MMC3_alt` beeps 222/444/222 Hz for code 2 — textbook
//! `010`), but the absolute pitches differ between suites:
//! `mmc3_irq_tests` beeps around 440 Hz where `mmc3_test_2` beeps around
//! 222. Worse, autocorrelation on the APU's mix ties across harmonics —
//! 440, 221 and 147 Hz score identically on the same burst — so a
//! classifier picks a sub-harmonic as often as a fundamental. Zero
//! crossings are worse still: DC offset made one low tone read anywhere
//! from 25 to 400 Hz. `probe_tone_shape` keeps the measurements for
//! whoever wants the value as well as the verdict.
//!
//! ⚠ The readme attributes the tones to NSF builds and these four are
//! `.nes`. They emit them anyway — measured, not assumed. What they do
//! NOT have is a `$6000` protocol: no `DE B0 61` signature and all
//! zeroes at `$6000-$6007` after 900M ticks. That much of the old note
//! was right, and is now measured rather than inferred, which is the
//! mistake `test_ppu_read_buffer` cost.
//!
//! ⚠⚠ **The counter is itself under test.** A tone counter that always
//! returned 1 would pass all four gates while proving nothing — the
//! failure mode this campaign keeps meeting. So three ROMs with codes
//! established through a completely separate channel (`$6000`, read by
//! the sweep) are counted by the same function the gates use:
//! `1.Clocking` passes, and `5.MMC3_rev_A` and `6-MMC3_alt` fail by
//! design with codes `$03` and `$02`. One tone against three.
//!
//! ⚠ Honest limit of that control: it shows the counter discriminates on
//! blargg shell ROMs, not on `dmc_tests` specifically. No failing build
//! of these four exists to point it at.
//!
//! Run with:
//! ```sh
//! cargo test --release -p machine-nintendo-nes --test dmc_tests_audio -- --ignored
//! ```

use std::path::PathBuf;

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;

/// `ricoh-apu-2a03` resamples to this rate.
const RATE: f32 = 48_000.0;

/// Frames to run. The shell beeps its result once the tests finish;
/// 900 frames is ~15 s emulated, well past all seven ROMs here.
const FRAMES: u64 = 900;

fn roms_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

fn capture_audio(rel: &str) -> Option<Vec<f32>> {
    let path = roms_root()?.join(rel);
    let bytes = std::fs::read(path).ok()?;
    let parsed = parse_ines(&bytes).ok()?;
    let mut nes = Nes::new(parsed.mapper);
    let mut audio = Vec::new();
    for _ in 0..FRAMES {
        nes.run_frame();
        audio.extend(nes.take_audio_buffer());
    }
    Some(audio)
}

/// Sample range of one detected tone.
type Tone = (usize, usize);

/// Split the buffer into tone bursts.
///
/// ⚠ Hysteresis and gap-merging are not polish. A plain
/// threshold-per-window split reported six bursts for a three-tone code,
/// because a tone's decay dips below any single threshold and the tail
/// then counts as a separate burst. That made code 3 read as six tones.
fn find_tones(audio: &[f32]) -> Vec<Tone> {
    let hop = (RATE * 0.005) as usize; // 5 ms
    let env: Vec<f32> = audio
        .chunks(hop)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt())
        .collect();
    let peak = env.iter().copied().fold(0.0f32, f32::max);
    if peak <= 0.0 {
        return Vec::new();
    }
    let (on, off) = (peak * 0.35, peak * 0.15);

    let mut spans: Vec<Tone> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &e) in env.iter().enumerate() {
        match start {
            None if e > on => start = Some(i),
            Some(s) if e < off => {
                spans.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        spans.push((s, env.len()));
    }

    // Merge spans separated by less than 40 ms — a decay dip inside one
    // tone, not a gap between two. The shell leaves 100 ms or more
    // between consecutive tones, so this cannot merge real neighbours.
    let max_gap = (0.040 / 0.005) as usize;
    let mut merged: Vec<Tone> = Vec::new();
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.0 - last.1 < max_gap => last.1 = span.1,
            _ => merged.push(span),
        }
    }

    // Drop anything under 40 ms. The shell's tones run 100-180 ms, and
    // the readme says outright that clicks are not part of the result:
    // "Some tests might need to ... cause slight audio clicks. This does
    // not indicate failure".
    let min_len = (0.040 / 0.005) as usize;
    merged
        .into_iter()
        .filter(|(s, e)| e - s >= min_len)
        .map(|(s, e)| (s * hop, (e * hop).min(audio.len())))
        .collect()
}

/// Count the tones in a ROM's result beep, printing what was found.
fn tone_count(rel: &str) -> Option<usize> {
    let audio = capture_audio(rel)?;
    let tones = find_tones(&audio);
    let summary: Vec<String> = tones
        .iter()
        .map(|(s, e)| format!("{}ms", (e - s) * 1000 / RATE as usize))
        .collect();
    println!("{rel:<40} {} tone(s) [{}]", tones.len(), summary.join(" "));
    Some(tones.len())
}

/// ⚠⚠ **The control, and it gates.** Three ROMs whose result codes are
/// known from the `$6000` protocol — an independent channel the
/// `dmc_tests` do not have — counted by the same function the gates
/// below use. A counter that could not tell one tone from three would
/// make every `dmc_tests` gate vacuous.
#[test]
#[ignore = "ROM run — requires test-suites/nes-test-roms"]
fn counter_separates_passing_from_failing_roms() {
    if roms_root().is_none() {
        eprintln!("nes-test-roms not found; skipping");
        return;
    }
    for (rel, expected, why) in [
        (
            "mmc3_irq_tests/1.Clocking.nes",
            1usize,
            "$6000 = $00, code 0",
        ),
        (
            "mmc3_irq_tests/5.MMC3_rev_A.nes",
            3,
            "$6000 = $03, code 3 = 011",
        ),
        (
            "mmc3_test_2/rom_singles/6-MMC3_alt.nes",
            3,
            "$6000 = $02, code 2 = 010",
        ),
    ] {
        let Some(count) = tone_count(rel) else {
            eprintln!("ROM missing; skipping {rel}");
            continue;
        };
        assert_eq!(count, expected, "{rel}: {why}");
    }
}

macro_rules! dmc_gate {
    ($name:ident, $rel:expr) => {
        /// Passes iff the ROM beeps a single tone — blargg code 0.
        #[test]
        #[ignore = "ROM run — requires test-suites/nes-test-roms"]
        fn $name() {
            if roms_root().is_none() {
                emu198x_test_skip::skip!(
                    "nes-test-roms corpus not staged (test-suites/nes-test-roms)"
                );
            }
            let Some(count) = tone_count($rel) else {
                emu198x_test_skip::skip!("nes-test-roms ROM not present: {}", $rel);
            };
            assert_eq!(
                count, 1,
                "{}: one tone is blargg code 0 (passed); {count} tones means a \
                 non-zero code",
                $rel
            );
        }
    };
}

dmc_gate!(buffer_retained, "dmc_tests/buffer_retained.nes");
dmc_gate!(latency, "dmc_tests/latency.nes");
dmc_gate!(status, "dmc_tests/status.nes");
dmc_gate!(status_irq, "dmc_tests/status_irq.nes");

/// Diagnostic kept for whoever wants the code's *value* and not only
/// its length: per-tone duration, gap and the top autocorrelation peaks,
/// which is where the low/high classifier came unstuck.
#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn probe_tone_shape() {
    if roms_root().is_none() {
        eprintln!("nes-test-roms not found; skipping");
        return;
    }
    for rel in [
        "mmc3_irq_tests/1.Clocking.nes",
        "mmc3_irq_tests/5.MMC3_rev_A.nes",
        "mmc3_test_2/rom_singles/6-MMC3_alt.nes",
        "dmc_tests/latency.nes",
    ] {
        let Some(audio) = capture_audio(rel) else {
            continue;
        };
        let tones = find_tones(&audio);
        println!("--- {rel}  ({} tones)", tones.len());
        let mut prev_end = 0usize;
        for &(start, end) in &tones {
            let seg = &audio[start..end];
            let skip = seg.len() / 5;
            let mid = &seg[skip..seg.len() - skip];
            let mean = mid.iter().sum::<f32>() / mid.len() as f32;
            let x: Vec<f32> = mid.iter().map(|s| s - mean).collect();
            let n = x.len().min(4096);
            let mut scored: Vec<(usize, f32)> = ((RATE / 2000.0) as usize
                ..((RATE / 60.0) as usize).min(n / 2))
                .map(|lag| {
                    (
                        lag,
                        (0..n - lag).map(|i| x[i] * x[i + lag]).sum::<f32>() / (n - lag) as f32,
                    )
                })
                .collect();
            // NaN cannot arise here (finite f32 sums), and a probe that
            // mis-orders its own diagnostic output is not worth a panic.
            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
            let top: Vec<String> = scored
                .iter()
                .take(3)
                .map(|(lag, v)| format!("{:.0}Hz({v:.4})", RATE / *lag as f32))
                .collect();
            println!(
                "  gap={:>4}ms len={:>4}ms peaks=[{}]",
                (start - prev_end) * 1000 / RATE as usize,
                (end - start) * 1000 / RATE as usize,
                top.join(" ")
            );
            prev_end = end;
        }
    }
}
