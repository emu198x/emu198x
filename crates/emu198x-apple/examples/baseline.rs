//! Direct-`Fleet` checkpoints for the Swift package's parity test.
//!
//! Runs the synthetic beeper program (`tests/common/beeper.rs`) on a
//! Spectrum 48K through `Fleet` itself, not the C interface, and prints one
//! entry per frame as JSON: the picture hash, the sample count and the bits
//! of the audio energy. `apple/Emu198xKit` runs the same program through
//! Swift and the C interface and must produce the same entries.
//! `scripts/build-apple-xcframework.sh` writes the output to
//! `apple/build/baseline.json`.

use emu198x_fleet_web::Fleet;

#[path = "../tests/common/beeper.rs"]
mod beeper;

/// FNV-1a over the RGBA bytes; the Swift test computes the same.
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn main() -> Result<(), String> {
    let mut fleet = Fleet::new("sinclair-zx-spectrum", "spectrum_48k")?;
    fleet.firmware("sinclair-zx-spectrum-48k-rom", &beeper::beeper_rom())?;
    fleet.boot()?;
    fleet.configure_audio(48_000)?;
    let mut entries = Vec::new();
    for _ in 0..50 {
        fleet.step()?;
        let audio = fleet.audio()?;
        let energy: f64 = audio.iter().map(|sample| f64::from(sample.abs())).sum();
        let pixels = fleet.pixels()?;
        // 64-bit values are strings: JSON readers may round numbers above
        // 2^53.
        entries.push(format!(
            r#"{{"hash":"{}","samples":{},"energy":"{}"}}"#,
            hash(&pixels),
            audio.len(),
            energy.to_bits()
        ));
    }
    println!("[{}]", entries.join(","));
    Ok(())
}
