//! Deterministic input path through the title and a moving/jumping game scene.
use emu198x_nes_web::Nes;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("pass dash.nes")?;
    let mut nes = Nes::new(&std::fs::read(path)?)?;
    for (name, pressed, frames) in [
        ("start", false, 70),
        ("start", true, 4),
        ("start", false, 2),
        ("left", true, 24),
        ("left", false, 6),
        ("a", true, 4),
        ("a", false, 54),
        ("right", true, 36),
        ("a", true, 4),
        ("a", false, 26),
        ("right", false, 60),
    ] {
        nes.button(name, pressed)?;
        for _ in 0..frames {
            nes.step()?;
        }
        let hash = nes.pixels().iter().fold(2_166_136_261_u32, |h, b| {
            (h ^ u32::from(*b)).wrapping_mul(16_777_619)
        });
        println!("{hash:08x}");
    }
    assert!(nes.button("invalid", true).is_err());
    Ok(())
}
