//! `emu198x-atari-5200` — Atari 5200 native binary.
//!
//! Three modes: UI (default), headless script, and MCP. `main.rs` is a tiny
//! dispatcher; the modes live in `src/ui.rs`, `src/script.rs`, and `src/mcp.rs`.
//! Building with `--no-default-features` drops the `ui` feature (winit + wgpu)
//! for the headless screenshot / script pipeline.

mod mcp;
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
    if args.iter().any(|a| a == "--mcp" || a == "--mcp-stdio") {
        Mode::Mcp
    } else if args.iter().any(|a| is_script_flag(a)) {
        Mode::Script
    } else {
        Mode::Ui
    }
}

/// Flags only the headless runner understands. Their presence routes to script
/// mode so existing invocations keep working; `--cart`/`--bios`/`--region` are
/// shared with the UI, so a bare `--cart game.a52` opens the interactive window.
fn is_script_flag(arg: &str) -> bool {
    matches!(
        arg,
        "--script" | "--frames" | "--screenshot" | "--audio-capture" | "--headless"
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
    ui::run(ui::parse_cli(args))
}

#[cfg(not(feature = "ui"))]
fn run_ui(_args: Vec<String>) -> Result<(), String> {
    Err("this binary was built without the `ui` feature; rebuild with `--features ui` for interactive mode, or use --script / --mcp instead".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_mode_defaults_to_ui() {
        assert_eq!(detect_mode(&[]), Mode::Ui);
    }

    #[test]
    fn detect_mode_treats_bare_cart_as_ui() {
        let args = vec!["--cart".to_owned(), "game.a52".to_owned()];
        assert_eq!(detect_mode(&args), Mode::Ui);
    }

    #[test]
    fn detect_mode_recognises_script_via_automation_flags() {
        for flag in ["--script", "--frames", "--screenshot", "--audio-capture"] {
            let args = vec!["--cart".to_owned(), "game.a52".to_owned(), flag.to_owned()];
            assert_eq!(
                detect_mode(&args),
                Mode::Script,
                "flag {flag} should be script"
            );
        }
    }

    #[test]
    fn detect_mode_recognises_mcp() {
        assert_eq!(detect_mode(&["--mcp".to_owned()]), Mode::Mcp);
    }
}
