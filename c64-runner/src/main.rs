//! Commodore 64 Emulator

use emu_core::Machine;
use machine_c64::C64;
use runner_lib::{RunnerConfig, run};
use std::fs;

fn main() {
    let mut machine = C64::new();

    // Load ROMs
    let basic = fs::read("roms/basic.bin")
        .or_else(|_| fs::read("roms/basic.rom"))
        .expect("Failed to load BASIC ROM (roms/basic.bin)");
    machine
        .load_file("basic.bin", &basic)
        .expect("Failed to load BASIC ROM");

    let kernal = fs::read("roms/kernal.bin")
        .or_else(|_| fs::read("roms/kernal.rom"))
        .expect("Failed to load KERNAL ROM (roms/kernal.bin)");
    machine
        .load_file("kernal.bin", &kernal)
        .expect("Failed to load KERNAL ROM");

    let chargen = fs::read("roms/chargen.bin")
        .or_else(|_| fs::read("roms/chargen.rom"))
        .expect("Failed to load Character ROM (roms/chargen.bin)");
    machine
        .load_file("chargen.bin", &chargen)
        .expect("Failed to load Character ROM");

    // Initialize the machine (reset to read vectors)
    machine.reset();

    // Load PRG file if provided
    if let Some(file_path) = std::env::args().nth(1) {
        let data = fs::read(&file_path).expect("Failed to load file");
        let lower = file_path.to_lowercase();

        if !lower.ends_with(".prg") {
            eprintln!("Unknown file type: {}", file_path);
            eprintln!("Supported formats: .prg");
            std::process::exit(1);
        }

        machine
            .load_file(&file_path, &data)
            .expect("Failed to load file");
        println!("Loaded: {}", file_path);
        println!("Type RUN and press Enter to start the program.");
    }

    run(
        machine,
        RunnerConfig {
            title: "Commodore 64".into(),
            scale: 3,
            crt_enabled: false,
        },
    );
}
