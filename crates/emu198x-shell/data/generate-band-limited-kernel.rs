//! Reproduce the original Blackman/sinc kernel using the host math library.
//! Run with the pinned toolchain: rustc generate-band-limited-kernel.rs -o /tmp/kernel-generator
//! Then: /tmp/kernel-generator NEW-OUTPUT.f64le
//! The output must not exist. Host sin/cos may produce different low bits;
//! compare with the canonical SHA-256 before considering any replacement.
use std::{f64::consts::PI, fs::OpenOptions, io::Write};
const SUPPORT: usize = 96;
const PHASES: usize = 256;
const DELAY: usize = SUPPORT / 2;
const CUTOFF: f64 = 22.0 / 48.0;
fn main() {
    let path = std::env::args_os().nth(1).expect("new output path");
    // Allocate on the heap, including during construction (also on Wasm).
    let mut integral = vec![0.0; SUPPORT * PHASES + 1];
    let mut previous = 0.0;
    let mut area = 0.0;
    for (i, sample) in integral.iter_mut().enumerate() {
        let t = i as f64 / PHASES as f64;
        let x = t - DELAY as f64;
        let window = 0.42 - 0.5 * (2.0 * PI * t / SUPPORT as f64).cos()
            + 0.08 * (4.0 * PI * t / SUPPORT as f64).cos();
        let sinc = if x == 0.0 {
            2.0 * CUTOFF
        } else {
            (2.0 * PI * CUTOFF * x).sin() / (PI * x)
        };
        let value = window * sinc;
        if i != 0 {
            area += (previous + value) / (2 * PHASES) as f64;
        }
        *sample = area;
        previous = value;
    }
    for sample in &mut integral {
        *sample /= area;
    }
    let mut rows = vec![[0.0; SUPPORT]; PHASES + 1].into_boxed_slice();
    for (phase, row) in rows.iter_mut().enumerate() {
        for (sample, correction) in row.iter_mut().enumerate() {
            *correction = integral[sample * PHASES + phase] - 1.0;
        }
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("new output only");
    for row in rows.iter() {
        for value in row {
            file.write_all(&value.to_le_bytes())
                .expect("coefficient bytes");
        }
    }
}
