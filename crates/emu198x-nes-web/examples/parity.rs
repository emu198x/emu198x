//! Print framebuffer hashes for the same checkpoints used by the WASM check.
use emu198x_nes_web::Nes;
fn hash(bytes: &[u8]) -> u32 {
    bytes.iter().fold(2_166_136_261_u32, |h, b| {
        (h ^ u32::from(*b)).wrapping_mul(16_777_619)
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let mut nes = Nes::new(&bytes)?;
        for pressed in [false, true, false] {
            nes.button_a(pressed);
            for _ in 0..10 {
                nes.step()?;
            }
            println!("{path} A={pressed} {:08x}", hash(&nes.pixels()));
        }
    }
    assert!(Nes::new(b"not a cartridge").is_err());
    Ok(())
}
