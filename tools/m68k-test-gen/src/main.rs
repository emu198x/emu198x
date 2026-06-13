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
use std::path::{Path, PathBuf};
use testcase::TestFile;

#[derive(Debug, PartialEq, Eq)]
struct CliArgs {
    cpu_name: String,
    cpu_type: u32,
    count: usize,
    instruction_name: Option<String>,
    all: bool,
}

fn print_usage() {
    eprintln!("Usage: m68k-test-gen --cpu <CPU> --instruction <NAME> --count <N>");
    eprintln!("       m68k-test-gen --cpu <CPU> --all --count <N>");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let cli = match parse_args_from(&args) {
        Ok(Some(cli)) => cli,
        Ok(None) => {
            print_usage();
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("{e}");
            print_usage();
            std::process::exit(1);
        }
    };

    let output_dir = output_dir_for_cpu(&cli.cpu_name);
    fs::create_dir_all(&output_dir).expect("failed to create output directory");

    if cli.all {
        let defs = instructions::catalogue(cli.cpu_type);
        if defs.is_empty() {
            eprintln!("No instructions defined for CPU {}", cli.cpu_name);
            std::process::exit(1);
        }
        println!(
            "Generating {} tests for {} instructions (CPU {})",
            cli.count,
            defs.len(),
            cli.cpu_name
        );
        for def in &defs {
            generate_and_write(def, cli.cpu_type, &cli.cpu_name, cli.count, &output_dir);
        }
    } else if let Some(name) = &cli.instruction_name {
        let def = instructions::find(cli.cpu_type, name).unwrap_or_else(|| {
            eprintln!("Unknown instruction: {name}");
            let defs = instructions::catalogue(cli.cpu_type);
            eprintln!("Available:");
            for d in &defs {
                eprintln!("  {}", d.name);
            }
            std::process::exit(1);
        });
        generate_and_write(&def, cli.cpu_type, &cli.cpu_name, cli.count, &output_dir);
    } else {
        print_usage();
        std::process::exit(1);
    }
}

fn generate_and_write(
    def: &instructions::InstructionDef,
    cpu_type: u32,
    cpu_name: &str,
    count: usize,
    output_dir: &Path,
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
    println!("{} tests, {} bytes", file.tests.len(), data.len());
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
        "68010" => base.join("test-data/m68010/v1"),
        "68EC020" | "68ec020" => base.join("test-data/m68ec020/v1"),
        "68020" => base.join("test-data/m68020/v1"),
        "68EC030" | "68ec030" => base.join("test-data/m68ec030/v1"),
        "68030" => base.join("test-data/m68030/v1"),
        "68EC040" | "68ec040" => base.join("test-data/m68ec040/v1"),
        // 68LC040 omitted: Musashi's implementation is broken for this variant
        "68040" => base.join("test-data/m68040/v1"),
        other => base.join(format!("test-data/m68k-{other}/v1")),
    }
}

fn parse_cpu_type(cpu_name: &str) -> Result<u32, String> {
    match cpu_name {
        "68000" => Ok(musashi::M68K_CPU_TYPE_68000),
        "68010" => Ok(musashi::M68K_CPU_TYPE_68010),
        "68EC020" | "68ec020" => Ok(musashi::M68K_CPU_TYPE_68EC020),
        "68020" => Ok(musashi::M68K_CPU_TYPE_68020),
        "68EC030" | "68ec030" => Ok(musashi::M68K_CPU_TYPE_68EC030),
        "68030" => Ok(musashi::M68K_CPU_TYPE_68030),
        "68EC040" | "68ec040" => Ok(musashi::M68K_CPU_TYPE_68EC040),
        // 68LC040 intentionally omitted: Musashi's implementation is broken.
        "68040" => Ok(musashi::M68K_CPU_TYPE_68040),
        other => Err(format!(
            "Unknown CPU type: {other}\nSupported: 68000, 68010, 68EC020, 68020, 68EC030, 68030, 68EC040, 68040"
        )),
    }
}

fn parse_args_from(args: &[String]) -> Result<Option<CliArgs>, String> {
    let mut cli = CliArgs {
        cpu_name: "68000".to_string(),
        cpu_type: musashi::M68K_CPU_TYPE_68000,
        count: 2500,
        instruction_name: None,
        all: false,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--cpu" => {
                i += 1;
                let value = args
                    .get(i)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| "--cpu requires a value".to_string())?;
                cli.cpu_name = value.clone();
            }
            "--count" => {
                i += 1;
                let value = args
                    .get(i)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| "--count requires a value".to_string())?;
                cli.count = value
                    .parse()
                    .map_err(|_| format!("Invalid value for --count: {value}"))?;
            }
            "--instruction" => {
                i += 1;
                let value = args
                    .get(i)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| "--instruction requires a value".to_string())?;
                cli.instruction_name = Some(value.clone());
            }
            "--all" => {
                cli.all = true;
            }
            "--help" | "-h" => return Ok(None),
            other => return Err(format!("Unknown argument: {other}")),
        }
        i += 1;
    }

    if cli.all == cli.instruction_name.is_some() {
        return Err("Specify exactly one of --all or --instruction <NAME>".to_string());
    }

    cli.cpu_type = parse_cpu_type(&cli.cpu_name)?;
    Ok(Some(cli))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_dir_for_cpu_maps_known_and_unknown_variants() {
        assert!(output_dir_for_cpu("68020").ends_with("test-data/m68020/v1"));
        assert!(output_dir_for_cpu("68EC020").ends_with("test-data/m68ec020/v1"));
        assert!(output_dir_for_cpu("custom").ends_with("test-data/m68k-custom/v1"));
    }

    #[test]
    fn cli_parser_reads_instruction_mode_and_defaults() {
        let args = vec![
            "m68k-test-gen".to_string(),
            "--cpu".to_string(),
            "68020".to_string(),
            "--instruction".to_string(),
            "NOP".to_string(),
        ];
        let cli = parse_args_from(&args)
            .expect("parse should succeed")
            .expect("help was not requested");

        assert_eq!(cli.cpu_name, "68020");
        assert_eq!(cli.cpu_type, musashi::M68K_CPU_TYPE_68020);
        assert_eq!(cli.count, 2500);
        assert_eq!(cli.instruction_name.as_deref(), Some("NOP"));
        assert!(!cli.all);
    }

    #[test]
    fn cli_parser_reads_all_mode_with_custom_count() {
        let args = vec![
            "m68k-test-gen".to_string(),
            "--cpu".to_string(),
            "68ec020".to_string(),
            "--all".to_string(),
            "--count".to_string(),
            "5".to_string(),
        ];
        let cli = parse_args_from(&args)
            .expect("parse should succeed")
            .expect("help was not requested");

        assert_eq!(cli.cpu_name, "68ec020");
        assert_eq!(cli.cpu_type, musashi::M68K_CPU_TYPE_68EC020);
        assert_eq!(cli.count, 5);
        assert_eq!(cli.instruction_name, None);
        assert!(cli.all);
    }

    #[test]
    fn cli_parser_requires_exactly_one_generation_mode() {
        let neither =
            parse_args_from(&["m68k-test-gen".to_string()]).expect_err("missing mode should fail");
        assert!(neither.contains("Specify exactly one of --all or --instruction"));

        let both = parse_args_from(&[
            "m68k-test-gen".to_string(),
            "--all".to_string(),
            "--instruction".to_string(),
            "NOP".to_string(),
        ])
        .expect_err("conflicting modes should fail");
        assert!(both.contains("Specify exactly one of --all or --instruction"));
    }

    #[test]
    fn cli_parser_rejects_missing_or_invalid_values() {
        let missing = parse_args_from(&[
            "m68k-test-gen".to_string(),
            "--cpu".to_string(),
            "--all".to_string(),
        ])
        .expect_err("missing cpu value should fail");
        assert!(missing.contains("--cpu requires a value"));

        let invalid_count = parse_args_from(&[
            "m68k-test-gen".to_string(),
            "--all".to_string(),
            "--count".to_string(),
            "abc".to_string(),
        ])
        .expect_err("invalid count should fail");
        assert!(invalid_count.contains("Invalid value for --count"));

        let invalid_cpu = parse_args_from(&[
            "m68k-test-gen".to_string(),
            "--all".to_string(),
            "--cpu".to_string(),
            "68060".to_string(),
        ])
        .expect_err("invalid cpu should fail");
        assert!(invalid_cpu.contains("Unknown CPU type: 68060"));
    }

    #[test]
    fn cli_parser_reports_help_and_unknown_args() {
        assert!(matches!(
            parse_args_from(&["m68k-test-gen".to_string(), "--help".to_string()])
                .expect("help parse should succeed"),
            None
        ));

        let result = parse_args_from(&[
            "m68k-test-gen".to_string(),
            "--all".to_string(),
            "--bogus".to_string(),
        ]);
        assert!(result.is_err());
        assert!(
            result
                .expect_err("unknown args should fail")
                .contains("Unknown argument: --bogus")
        );
    }
}
