//! Nintendo Entertainment System Emulator

use emu_core::Machine;
use machine_nes::Nes;
use runner_lib::{run, RunnerConfig};
use std::fs;

fn main() {
    let mut machine = Nes::new();

    // Load ROM file if provided
    if let Some(file_path) = std::env::args().nth(1) {
        let data = fs::read(&file_path).expect("Failed to load file");
        let lower = file_path.to_lowercase();

        if !lower.ends_with(".nes") {
            eprintln!("Unknown file type: {}", file_path);
            eprintln!("Supported formats: .nes (iNES)");
            std::process::exit(1);
        }

        machine
            .load_file(&file_path, &data)
            .expect("Failed to load ROM");
        println!("Loaded: {}", file_path);
    } else {
        eprintln!("Usage: nes-runner <rom.nes>");
        eprintln!("No ROM provided - starting with empty machine");
    }

    run(
        machine,
        RunnerConfig {
            title: "Nintendo Entertainment System".into(),
            scale: 3,
            crt_enabled: false,
        },
    );
}
