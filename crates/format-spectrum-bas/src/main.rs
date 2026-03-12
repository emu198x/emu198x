//! bas2tap — convert a text .bas file to a ZX Spectrum TAP file.
//!
//! Usage: bas2tap input.bas [-o output.tap] [--name NAME] [--autorun LINE]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use format_spectrum_bas::tokenise;
use format_spectrum_tap::{TapBlock, TapFile};

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {e}");
            eprintln!();
            print_usage();
            process::exit(1);
        }
    };

    let source = match fs::read_to_string(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {e}", args.input.display());
            process::exit(1);
        }
    };

    let program = match tokenise(&source) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error tokenising: {e}");
            process::exit(1);
        }
    };

    let data_len = program.bytes.len() as u16;
    let header = TapBlock::program_header(&args.name, data_len, args.autorun, data_len);
    let data = TapBlock::data(program.bytes);

    let mut tap = TapFile::new();
    tap.blocks.push(header);
    tap.blocks.push(data);

    let output = args.output.unwrap_or_else(|| args.input.with_extension("tap"));

    match fs::write(&output, tap.to_bytes()) {
        Ok(()) => {
            eprintln!(
                "{} -> {} ({data_len} bytes, {})",
                args.input.display(),
                output.display(),
                match args.autorun {
                    Some(line) => format!("autorun at line {line}"),
                    None => "no autorun".to_string(),
                }
            );
        }
        Err(e) => {
            eprintln!("Error writing {}: {e}", output.display());
            process::exit(1);
        }
    }
}

struct Args {
    input: PathBuf,
    output: Option<PathBuf>,
    name: String,
    autorun: Option<u16>,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        return Err("no input file specified".to_string());
    }

    let mut input = None;
    let mut output = None;
    let mut name = None;
    let mut autorun = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                output = Some(PathBuf::from(
                    args.get(i).ok_or("-o requires a filename")?,
                ));
            }
            "--name" | "-n" => {
                i += 1;
                name = Some(
                    args.get(i)
                        .ok_or("--name requires a value")?
                        .clone(),
                );
            }
            "--autorun" | "-a" => {
                i += 1;
                let line: u16 = args
                    .get(i)
                    .ok_or("--autorun requires a line number")?
                    .parse()
                    .map_err(|_| "--autorun must be a number (1–9999)")?;
                autorun = Some(line);
            }
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option: {other}"));
            }
            _ => {
                if input.is_some() {
                    return Err("only one input file allowed".to_string());
                }
                input = Some(PathBuf::from(&args[i]));
            }
        }
        i += 1;
    }

    let input = input.ok_or("no input file specified")?;

    // Default name from filename stem, uppercase, truncated to 10 chars
    let name = name.unwrap_or_else(|| {
        input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("PROGRAM")
            .to_uppercase()
            .chars()
            .take(10)
            .collect()
    });

    Ok(Args {
        input,
        output,
        name,
        autorun,
    })
}

fn print_usage() {
    eprintln!("Usage: bas2tap <input.bas> [options]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -o, --output FILE     Output TAP file (default: input with .tap extension)");
    eprintln!("  -n, --name NAME       Program name in TAP header (default: filename, max 10 chars)");
    eprintln!("  -a, --autorun LINE    Auto-run at this line number");
    eprintln!("  -h, --help            Show this help");
}
