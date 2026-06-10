//! `emu198x-colecovision` — ColecoVision native binary.
//!
//! Two modes: headless script (the default — `--bios`/`--cart`/
//! `--frames`/`--screenshot`/`--audio-capture`/`--script`) and MCP
//! (`--mcp` / `--mcp-stdio`). Mirrors the MSX binary; UI mode is a
//! follow-up.

mod mcp;
mod script;

use std::process;

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Script,
    Mcp,
}

fn detect_mode(args: &[String]) -> Mode {
    if args.iter().any(|a| a == "--mcp" || a == "--mcp-stdio") {
        Mode::Mcp
    } else {
        Mode::Script
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match detect_mode(&args) {
        Mode::Script => script::run(args),
        Mode::Mcp => mcp::run(),
    };
    if let Err(err) = result {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_mode_defaults_to_script() {
        assert_eq!(detect_mode(&[]), Mode::Script);
    }

    #[test]
    fn detect_mode_recognises_mcp() {
        let args = vec!["--mcp".to_owned()];
        assert_eq!(detect_mode(&args), Mode::Mcp);
    }

    #[test]
    fn detect_mode_recognises_mcp_stdio() {
        let args = vec!["--mcp-stdio".to_owned()];
        assert_eq!(detect_mode(&args), Mode::Mcp);
    }
}
