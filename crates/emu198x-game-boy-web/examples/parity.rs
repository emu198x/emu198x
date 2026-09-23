//! Native checkpoints consumed by the shared player's WASM regression.
use emu198x_game_boy_web::GameBoy;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(std::env::args().nth(1).ok_or("pass a DMG cartridge")?)?;
    let mut machine = GameBoy::new("dmg", &bytes)?;
    machine.configure_audio(48_000)?;
    for pressed in [false, true, false] {
        machine.button("a", pressed)?;
        let mut count = 0;
        let mut energy = 0.0_f64;
        for _ in 0..20 {
            machine.step()?;
            let samples = machine.audio();
            count += samples.len();
            energy += samples.iter().map(|v| f64::from(*v).abs()).sum::<f64>();
        }
        let hash = machine.pixels().iter().fold(2_166_136_261_u32, |h, b| {
            (h ^ u32::from(*b)).wrapping_mul(16_777_619)
        });
        println!("{hash} {count} {energy:.9}");
    }
    Ok(())
}
