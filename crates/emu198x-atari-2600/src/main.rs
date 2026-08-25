//! `emu198x-atari-2600` — Atari 2600 / VCS native binary.
//!
//! Three modes: interactive UI (default), headless script, and MCP. `--mcp`
//! wins; any automation flag (`--script`/`--frames`/`--screenshot`/
//! `--audio-capture`/`--headless`) selects script mode; otherwise the
//! interactive window runs. A bare `--cart`/positional path opens the UI.

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

/// Flags only the headless runner understands; their presence routes to script
/// mode so existing `--cart x --frames N --screenshot y` invocations keep
/// working. `--cart`/`--region` are shared with the UI, so a bare cart path
/// opens the interactive window.
fn is_script_flag(arg: &str) -> bool {
    matches!(
        arg,
        "--script" | "--headless" | "--frames" | "--screenshot" | "--audio-capture"
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match detect_mode(&args) {
        Mode::Ui => run_ui(args),
        Mode::Script => script::run(args),
        Mode::Mcp => mcp::run(&args),
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
    Err("this binary was built without the `ui` feature; rebuild with `--features ui`, or use --script / --mcp".to_owned())
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
        let args = ["--cart".to_owned(), "game.a26".to_owned()];
        assert_eq!(detect_mode(&args), Mode::Ui);
    }

    #[test]
    fn detect_mode_recognises_mcp() {
        assert_eq!(detect_mode(&["--mcp".to_owned()]), Mode::Mcp);
    }

    #[test]
    fn detect_mode_routes_automation_flags_to_script() {
        for flag in [
            "--frames",
            "--screenshot",
            "--audio-capture",
            "--headless",
            "--script",
        ] {
            let args = ["--cart".to_owned(), "game.a26".to_owned(), flag.to_owned()];
            assert_eq!(detect_mode(&args), Mode::Script, "{flag} should be script");
        }
    }
}
