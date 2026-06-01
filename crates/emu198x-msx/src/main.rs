//! `emu198x-msx` — MSX1 native binary.
//!
//! Two modes: headless script (the default for this minimal port,
//! covering the legacy `--bios`/`--cart`/`--frames`/`--screenshot`/
//! `--audio-capture` surface plus the shared `--script` runner) and
//! MCP (`--mcp` / `--mcp-stdio`). UI mode is a follow-up — the
//! current MSX binary is headless-first by design.
//!
//! Mode selection: `--mcp` / `--mcp-stdio` win; everything else is
//! script mode.

mod mcp;
mod mcp_tools;
mod script;

use std::process;

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Script,
    Mcp,
}

fn detect_mode(args: &[String]) -> Mode {
    if args.iter().any(|arg| arg == "--mcp" || arg == "--mcp-stdio") {
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

    #[test]
    fn detect_mode_mcp_wins_over_script_args() {
        let args = vec!["--mcp".to_owned(), "--frames".to_owned(), "60".to_owned()];
        assert_eq!(detect_mode(&args), Mode::Mcp);
    }
}
