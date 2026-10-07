use emu198x_spectrum_composite_experiment::{
    Connection, Experiment, HEIGHT, Receiver, WIDTH, colour, raw_rgb, rgb,
};
use std::{error::Error, fs, hint::black_box, path::Path, time::Instant};

fn export(path: &Path, colours: impl Iterator<Item = [f64; 3]>) -> Result<(), Box<dyn Error>> {
    let rgba: Vec<u8> = colours
        .flat_map(|c| {
            [
                (c[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                (c[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                (c[2].clamp(0.0, 1.0) * 255.0).round() as u8,
                255,
            ]
        })
        .collect();
    fs::write(path, rgba)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        return Err(
            "usage: composite-experiment INPUT.idx OUTPUT_DIR [samples=4] [phase=0] [field=0]"
                .into(),
        );
    }
    let receiver = Receiver {
        samples_per_pixel: args.get(3).map(|v| v.parse()).transpose()?.unwrap_or(4),
        phase_cycles: args.get(4).map(|v| v.parse()).transpose()?.unwrap_or(0.0),
        field: args.get(5).map(|v| v.parse()).transpose()?.unwrap_or(0),
        ..Receiver::default()
    };
    let experiment = Experiment::new(receiver)?;
    let frame = fs::read(&args[1])?;
    if frame.len() != WIDTH * HEIGHT || frame.iter().any(|i| *i > 15) {
        return Err("invalid index frame".into());
    }
    let dir = Path::new(&args[2]);
    fs::create_dir_all(dir)?;
    export(&dir.join("raw.rgba"), frame.iter().map(|i| raw_rgb(*i)))?;
    export(
        &dir.join("analogue.rgba"),
        frame.iter().map(|i| rgb(colour(*i))),
    )?;
    let separate = experiment.decode(&frame, Connection::Separated)?;
    export(
        &dir.join("separated.rgba"),
        separate.iter().map(|c| rgb(*c)),
    )?;
    // Warm processing path once; three measured iterations, excluding exports
    // and oscillator/filter construction. Allocations remain included.
    black_box(experiment.decode(&frame, Connection::Composite)?);
    let mut timings = Vec::new();
    let mut decoded = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        decoded = experiment.decode(black_box(&frame), Connection::Composite)?;
        black_box(&decoded);
        timings.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    export(&dir.join("composite.rgba"), decoded.iter().map(|c| rgb(*c)))?;
    let mut max_delta: f64 = 0.0;
    let mut mse = 0.0;
    let mut chroma = 0.0;
    let mut clipped = 0;
    for (c, s) in decoded.iter().zip(&separate) {
        chroma += c.u * c.u + c.v * c.v;
        for (a, b) in rgb(*c).into_iter().zip(rgb(*s)) {
            mse += (a - b) * (a - b);
            max_delta = max_delta.max((a - b).abs());
            clipped += usize::from(!(0.0..=1.0).contains(&a));
        }
    }
    timings.sort_by(f64::total_cmp);
    fs::write(
        dir.join("metrics.json"),
        format!(
            "{{\"width\":{WIDTH},\"height\":{HEIGHT},\"samples_per_pixel\":{},\"phase_cycles\":{},\"field\":{},\"median_decode_ms\":{:.4},\"composite_vs_separated_rgb_rmse\":{:.6},\"max_rgb_delta\":{max_delta:.6},\"mean_chroma_energy\":{:.6},\"clipped_rgb_channel_fraction\":{:.6}}}\n",
            receiver.samples_per_pixel,
            receiver.phase_cycles,
            receiver.field,
            timings[1],
            (mse / (decoded.len() * 3) as f64).sqrt(),
            chroma / decoded.len() as f64,
            clipped as f64 / (decoded.len() * 3) as f64
        ),
    )?;
    println!("{}: {:.2} ms/frame", args[1], timings[1]);
    Ok(())
}
