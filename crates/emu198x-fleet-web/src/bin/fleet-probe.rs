//! Native execution checkpoints for the browser parity gate (local files only).
use emu198x_fleet_web::Fleet;
use serde_json::{Value, json};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = std::env::args().nth(1).ok_or("pass a fixture manifest")?;
    let cases: Vec<Value> = serde_json::from_slice(&std::fs::read(manifest)?)?;
    let mut results = Vec::new();
    for case in cases {
        let run = || -> Result<Value, String> {
            let mut machine = Fleet::new(
                case["family"].as_str().ok_or("family")?,
                case["id"].as_str().ok_or("id")?,
            )?;
            for (id, path) in case["firmware"].as_object().ok_or("firmware")? {
                machine.firmware(
                    id,
                    &std::fs::read(path.as_str().ok_or("path")?).map_err(|e| e.to_string())?,
                )?;
            }
            machine.boot()?;
            if let Some(media) = case["media"].as_object() {
                machine.load(
                    media["slot"].as_str().ok_or("slot")?,
                    &std::fs::read(media["path"].as_str().ok_or("path")?)
                        .map_err(|e| e.to_string())?,
                )?;
            }
            machine.configure_audio(48000)?;
            let mut checkpoints = Vec::new();
            for pressed in [false, true, false] {
                machine.key("A", pressed)?;
                machine.button("fire1", pressed)?;
                let mut samples = 0;
                let mut energy = 0.0_f64;
                for _ in 0..6 {
                    machine.step()?;
                    let audio = machine.audio()?;
                    samples += audio.len();
                    for sample in audio {
                        energy += f64::from(sample.abs());
                    }
                }
                let hash = machine.pixels()?.iter().fold(2166136261_u32, |h, b| {
                    (h ^ u32::from(*b)).wrapping_mul(16777619)
                });
                checkpoints.push(json!({"hash":hash,"samples":samples,"energy":energy}));
            }
            Ok(
                json!({"frameMs":machine.frame_ms()?,"width":machine.width()?,"height":machine.height()?,"keymap":serde_json::from_str::<Value>(&machine.keymap()?).map_err(|e|e.to_string())?,"checkpoints":checkpoints}),
            )
        };
        results.push(match run() {
            Ok(mut result) => {
                result["family"] = case["family"].clone();
                result["id"] = case["id"].clone();
                result
            }
            Err(error) => json!({"family":case["family"],"id":case["id"],"error":error}),
        });
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
