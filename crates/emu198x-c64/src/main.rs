//! `emu198x-c64` — Commodore 64 native binary.
//!
//! One binary, three modes: UI (default), headless script, and MCP.
//! `main.rs` is a tiny dispatcher; the modes live in `src/ui.rs`,
//! `src/script.rs`, and `src/mcp.rs`.
//!
//! # Cargo features
//!
//! - `ui` (default) — compiles in winit + wgpu for the interactive
//!   verifier window. Required for the default UI mode.
//! - Without `ui` (`--no-default-features`) — `--script` and `--mcp`
//!   still work; the default UI path errors at runtime with a
//!   "rebuild with `--features ui`" message. The headless
//!   boot/capture/trace pipeline uses this build to skip the heavy
//!   graphics stack.
//!
//! Mode selection: `--mcp` wins; otherwise a headless-only flag
//! (`--script`, `--headless`, `--frames`, `--screenshot`,
//! `--save-snapshot`, `--print-*`, `--trace-*`, `--wait-for-*`) selects
//! headless script mode; otherwise the interactive UI runs. Flags shared
//! with the UI (`--rom-dir`, `--kernal`/`--basic`/`--chargen`, `--disk`,
//! `--tape`, `--load`, `--autoload-*`, `--start-tape`, `--load-snapshot`,
//! `--model`, `--scale`, `--video`, `--turbo-tape`) do not force
//! headless, so `--rom-dir … --disk game.d64 --autoload-disk` opens the
//! window.

mod mcp;
mod mcp_tools;
mod script;

#[cfg(feature = "ui")]
mod ui;

use std::process;

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Ui,
    Script,
    Mcp,
}

fn detect_mode(args: &[String]) -> Mode {
    if args.iter().any(|arg| arg == "--mcp") {
        Mode::Mcp
    } else if args.iter().any(|arg| is_script_flag(arg)) {
        Mode::Script
    } else {
        Mode::Ui
    }
}

/// Flags only the headless runner understands. Flags shared with the UI
/// (`--rom-dir`, `--disk`, `--tape`, `--load`, `--autoload-*`, `--model`,
/// …) are intentionally absent so an interactive invocation opens the
/// window.
fn is_script_flag(arg: &str) -> bool {
    matches!(
        arg,
        "--script"
            | "--headless"
            | "--frames"
            | "--screenshot"
            | "--save-snapshot"
            | "--print-query"
            | "--print-screen-text"
            | "--trace-drive-rom"
            | "--trace-limit"
            | "--trace-vic-colours"
            | "--wait-for-boot"
            | "--wait-for-tape-stop"
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let result = match detect_mode(&args) {
        Mode::Ui => run_ui(args),
        Mode::Script => script::run(args),
        Mode::Mcp => mcp::run(),
    };

    if let Err(err) = result {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

#[cfg(feature = "ui")]
fn run_ui(args: Vec<String>) -> Result<(), String> {
    let cli = ui::parse_cli(args);
    ui::run(cli).map_err(|err| err.to_string())
}

#[cfg(not(feature = "ui"))]
fn run_ui(_args: Vec<String>) -> Result<(), String> {
    Err("this binary was built without the `ui` feature; rebuild with `--features ui` for interactive mode, or use --script / --mcp instead".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_mode_defaults_to_ui_with_no_args() {
        assert_eq!(detect_mode(&[]), Mode::Ui);
    }

    #[test]
    fn detect_mode_treats_interactive_media_flags_as_ui() {
        let args = vec![
            "--rom-dir".to_owned(),
            "roms".to_owned(),
            "--disk".to_owned(),
            "game.d64".to_owned(),
            "--autoload-disk".to_owned(),
        ];
        assert_eq!(detect_mode(&args), Mode::Ui);
    }

    #[test]
    fn detect_mode_recognises_script_via_automation_flags() {
        for flag in [
            "--script",
            "--headless",
            "--frames",
            "--screenshot",
            "--wait-for-boot",
            "--trace-vic-colours",
        ] {
            let args = vec!["--rom-dir".to_owned(), "roms".to_owned(), flag.to_owned()];
            assert_eq!(
                detect_mode(&args),
                Mode::Script,
                "flag {flag} should be script"
            );
        }
    }

    #[test]
    fn detect_mode_mcp_takes_precedence_over_script() {
        let args = vec![
            "--mcp".to_owned(),
            "--script".to_owned(),
            "steps.json".to_owned(),
        ];
        assert_eq!(detect_mode(&args), Mode::Mcp);
    }

    #[cfg(not(feature = "ui"))]
    #[test]
    fn run_ui_errors_when_feature_off() {
        assert!(run_ui(vec![]).is_err());
    }
}
