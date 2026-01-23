//! ZX Spectrum 48K Emulator

use emu_core::Machine;
use machine_spectrum::Spectrum48K;
use runner_lib::{RunnerConfig, run};
use std::fs;

fn main() {
    let mut machine = Spectrum48K::new();

    // Load the ROM
    let rom = fs::read("roms/48.rom").expect("Failed to load ROM");
    machine
        .load_file("48.rom", &rom)
        .expect("Failed to load ROM");

    // Load file if provided (supports .tap and .sna)
    if let Some(file_path) = std::env::args().nth(1) {
        let data = fs::read(&file_path).expect("Failed to load file");
        let lower = file_path.to_lowercase();

        if !lower.ends_with(".sna") && !lower.ends_with(".tap") {
            eprintln!("Unknown file type: {}", file_path);
            eprintln!("Supported formats: .tap, .sna");
            std::process::exit(1);
        }

        machine
            .load_file(&file_path, &data)
            .expect("Failed to load file");
        println!("Loaded: {}", file_path);
    }

    run(
        machine,
        RunnerConfig {
            title: "ZX Spectrum 48K".into(),
            scale: 3,
            crt_enabled: false,
        },
    );
}
