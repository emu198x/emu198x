//! `emu198x-msx` — MSX1 native binary.
//!
//! Three modes: UI (default), headless script (the legacy
//! `--bios`/`--cart`/`--frames`/`--screenshot`/`--audio-capture` surface plus
//! the shared `--script` runner) and MCP (`--mcp` / `--mcp-stdio`). Building
//! with `--no-default-features` drops the `ui` feature (winit + wgpu) for the
//! headless screenshot / script pipeline.
//!
//! Mode selection: `--mcp` / `--mcp-stdio` win; an automation flag selects
//! script mode; otherwise the interactive UI.

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
    if args
        .iter()
        .any(|arg| arg == "--mcp" || arg == "--mcp-stdio")
    {
        Mode::Mcp
    } else if args.iter().any(|a| is_script_flag(a)) {
        Mode::Script
    } else {
        Mode::Ui
    }
}

/// Flags only the headless runner understands. Their presence routes to script
/// mode so existing invocations keep working; `--bios`/`--cart`/`--mapper`/
/// `--region` are shared with the UI, so a bare `--cart game.rom` opens the
/// interactive window.
fn is_script_flag(arg: &str) -> bool {
    matches!(
        arg,
        "--script" | "--frames" | "--screenshot" | "--audio-capture" | "--headless" | "--cart2"
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
        let args = vec!["--cart".to_owned(), "game.rom".to_owned()];
        assert_eq!(detect_mode(&args), Mode::Ui);
    }

    #[test]
    fn detect_mode_recognises_script_via_automation_flags() {
        for flag in ["--script", "--frames", "--screenshot", "--audio-capture"] {
            let args = vec!["--cart".to_owned(), "game.rom".to_owned(), flag.to_owned()];
            assert_eq!(
                detect_mode(&args),
                Mode::Script,
                "flag {flag} should be script"
            );
        }
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
