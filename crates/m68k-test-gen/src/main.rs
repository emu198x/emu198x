//! m68k-test-gen: Single-step test vector generator for Motorola 680x0 CPUs.
//!
//! Uses Musashi as reference emulator. Produces MessagePack test files
//! compatible with the motorola-68000 crate's test runner.
//!
//! Usage:
//!   m68k-test-gen --cpu 68000 --instruction NOP --count 2500
//!   m68k-test-gen --cpu 68000 --all --count 2500
//!   m68k-test-gen --cpu 68020 --all --count 2500

mod generator;
mod instructions;
mod memory;
mod musashi;
mod testcase;

use std::fs;
use std::path::PathBuf;
use testcase::TestFile;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let cpu_name = find_arg(&args, "--cpu").unwrap_or_else(|| "68000".to_string());
    let count: usize = find_arg(&args, "--count")
        .and_then(|s| s.parse().ok())
        .unwrap_or(2500);
    let instruction_name = find_arg(&args, "--instruction");
    let all = args.iter().any(|a| a == "--all");

    let cpu_type = match cpu_name.as_str() {
        "68000" => musashi::M68K_CPU_TYPE_68000,
        "68010" => musashi::M68K_CPU_TYPE_68010,
        "68EC020" | "68ec020" => musashi::M68K_CPU_TYPE_68EC020,
        "68020" => musashi::M68K_CPU_TYPE_68020,
        other => {
            eprintln!("Unknown CPU type: {other}");
            eprintln!("Supported: 68000, 68010, 68EC020, 68020");
            std::process::exit(1);
        }
    };

    let output_dir = output_dir_for_cpu(&cpu_name);
    fs::create_dir_all(&output_dir).expect("failed to create output directory");

    if all {
        let defs = instructions::catalogue(cpu_type);
        if defs.is_empty() {
            eprintln!("No instructions defined for CPU {cpu_name}");
            std::process::exit(1);
        }
        println!(
            "Generating {count} tests for {} instructions (CPU {cpu_name})",
            defs.len()
        );
        for def in &defs {
            generate_and_write(def, cpu_type, &cpu_name, count, &output_dir);
        }
    } else if let Some(name) = instruction_name {
        let def = instructions::find(cpu_type, &name).unwrap_or_else(|| {
            eprintln!("Unknown instruction: {name}");
            let defs = instructions::catalogue(cpu_type);
            eprintln!("Available:");
            for d in &defs {
                eprintln!("  {}", d.name);
            }
            std::process::exit(1);
        });
        generate_and_write(&def, cpu_type, &cpu_name, count, &output_dir);
    } else {
        eprintln!("Usage: m68k-test-gen --cpu <CPU> --instruction <NAME> --count <N>");
        eprintln!("       m68k-test-gen --cpu <CPU> --all --count <N>");
        std::process::exit(1);
    }
}

fn generate_and_write(
    def: &instructions::InstructionDef,
    cpu_type: u32,
    cpu_name: &str,
    count: usize,
    output_dir: &PathBuf,
) {
    print!("  {} ... ", def.name);
    let tests = generator::generate(def, cpu_type, count);
    let file = TestFile {
        cpu: cpu_name.to_string(),
        instruction: def.name.to_string(),
        tests,
    };

    let path = output_dir.join(format!("{}.msgpack", def.name));
    let data = rmp_serde::to_vec(&file).expect("failed to serialise");
    fs::write(&path, &data).expect("failed to write file");
    println!(
        "{} tests, {} bytes",
        file.tests.len(),
        data.len()
    );
}

fn output_dir_for_cpu(cpu_name: &str) -> PathBuf {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .parent()
        .expect("workspace root")
        .to_owned();

    match cpu_name {
        "68000" => base.join("test-data/m68000-musashi/v1"),
        "68020" | "68EC020" | "68ec020" => base.join("test-data/m68020/v1"),
        other => base.join(format!("test-data/m68k-{other}/v1")),
    }
}

fn find_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}
