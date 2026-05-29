//! `emu198x-dragon` — Dragon 32/64 native binary.
//!
//! One binary, three modes: UI (default), headless harness, and MCP.
//! `main.rs` is a tiny dispatcher; the modes live in `src/ui.rs`,
//! `src/script.rs`, and `src/mcp.rs`.
//!
//! # Cargo features
//!
//! - `ui` (default) — compiles in winit + wgpu for the interactive
//!   verifier window. Required for the default UI mode.
//! - Without `ui` (`--no-default-features`) — the headless harness and
//!   `--mcp` still work; the default UI path errors at runtime with a
//!   "rebuild with `--features ui`" message. The smoke / trace / XRoar
//!   pipeline uses this build to skip the heavy graphics stack.
//!
//! Mode selection: `--mcp` wins; otherwise `--script` / `--headless` or
//! any flag the UI does not recognise (the headless harness has a large
//! flag surface — `--smoke-*`, `--type-command`, `--dump-*`, `--cycles`,
//! `--watch-*`, `--xroar-*`, …) selects the headless harness; otherwise
//! the interactive UI runs. A bare `--rom dragon32.rom` (with only
//! UI-shared flags) opens the window.

mod mcp;
mod script;

#[cfg(feature = "ui")]
mod ui;

use std::process;

/// Flags the interactive UI understands. Anything else that looks like a
/// flag means the headless harness was requested. Kept in sync with
/// `ui::parse_cli`.
const UI_FLAGS: &[&str] = &[
    "--model",
    "--rom",
    "--rom64",
    "--tape",
    "--cart",
    "--bin",
    "--snapshot",
    "--autoload",
    "--scale",
    "--video",
    "--help",
];

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Ui,
    Script,
    Mcp,
}

fn detect_mode(args: &[String]) -> Mode {
    if args.iter().any(|arg| arg == "--mcp") {
        Mode::Mcp
    } else if args.iter().any(|arg| is_headless_flag(arg)) {
        Mode::Script
    } else {
        Mode::Ui
    }
}

/// A flag selects the headless harness if it is the explicit
/// `--script` / `--headless`, or any other long flag the UI does not
/// recognise. Shared flags (`--rom`, `--model`, …) and the short `-h`
/// stay in the UI lane so `--rom game.rom` opens the window.
fn is_headless_flag(arg: &str) -> bool {
    if arg == "--script" || arg == "--headless" {
        return true;
    }
    arg.starts_with("--") && !UI_FLAGS.contains(&arg)
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
    fn detect_mode_treats_bare_rom_and_shared_flags_as_ui() {
        let args = vec![
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--model".to_owned(),
            "dragon32".to_owned(),
            "--autoload".to_owned(),
        ];
        assert_eq!(detect_mode(&args), Mode::Ui);
    }

    #[test]
    fn detect_mode_recognises_headless_via_harness_flags() {
        for flag in [
            "--script",
            "--headless",
            "--smoke-root",
            "--type-command",
            "--cycles",
            "--dump-text",
            "--xroar-bin",
        ] {
            let args = vec![
                "--rom".to_owned(),
                "dragon32.rom".to_owned(),
                flag.to_owned(),
            ];
            assert_eq!(
                detect_mode(&args),
                Mode::Script,
                "flag {flag} should select the headless harness"
            );
        }
    }

    #[test]
    fn detect_mode_mcp_takes_precedence() {
        let args = vec![
            "--mcp".to_owned(),
            "--smoke-root".to_owned(),
            "x".to_owned(),
        ];
        assert_eq!(detect_mode(&args), Mode::Mcp);
    }

    #[cfg(not(feature = "ui"))]
    #[test]
    fn run_ui_errors_when_feature_off() {
        assert!(run_ui(vec![]).is_err());
    }
}
