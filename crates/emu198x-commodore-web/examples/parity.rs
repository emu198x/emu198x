//! Native checkpoints for scripts/check-parity.mjs. Firmware remains local.
use emu198x_commodore_web::Commodore;
use serde_json::json;

fn hash(bytes: &[u8]) -> String {
    format!(
        "{:08x}",
        bytes.iter().fold(2_166_136_261_u32, |h, b| {
            (h ^ u32::from(*b)).wrapping_mul(16_777_619)
        })
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let kind = args
        .first()
        .ok_or("pass c64 KERNAL BASIC CHARGEN [1541] or amiga KICKSTART")?;
    let mut machine = match kind.as_str() {
        "c64" if args.len() == 4 || args.len() == 5 => Commodore::c64(
            &std::fs::read(&args[1])?,
            &std::fs::read(&args[2])?,
            &std::fs::read(&args[3])?,
            &if args.len() == 5 {
                std::fs::read(&args[4])?
            } else {
                vec![]
            },
        )?,
        "amiga" if args.len() == 2 => Commodore::amiga(&std::fs::read(&args[1])?)?,
        _ => return Err("pass c64 KERNAL BASIC CHARGEN [1541] or amiga KICKSTART".into()),
    };
    machine.configure_audio(48_000)?;
    let mut checkpoints = Vec::new();
    let mut samples = 0usize;
    let mut energy = 0.0_f64;
    for frame in 1..=360 {
        if frame == 301 {
            machine.key("a", true);
            machine.joystick("fire", true)?;
            machine.mouse_move(20, -10);
            machine.mouse_button("left", true);
        }
        if frame == 311 {
            machine.key("a", false);
            machine.joystick("fire", false)?;
            machine.mouse_button("left", false);
        }
        machine.step()?;
        let audio = machine.audio();
        assert!(audio.iter().all(|v| v.is_finite()));
        samples += audio.len();
        energy += audio.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
        if [1, 100, 300, 310, 360].contains(&frame) {
            checkpoints.push(json!({"frame":frame,"hash":hash(&machine.pixels()),
                "width":machine.width(),"height":machine.height(),"samples":samples,"energy":energy}));
        }
    }
    println!("{}", json!({"kind":kind,"checkpoints":checkpoints}));
    Ok(())
}
