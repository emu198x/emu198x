//! `emu198x-amiga` — Commodore Amiga native binary.
//!
//! One binary, three modes: UI (default), headless script, and MCP.
//! `main.rs` is a thin dispatcher plus the items shared across modes
//! (the [`AppError`] type, the [`ModelArg`] model selector, and ROM
//! resolution). The modes live in `src/ui.rs`, `src/script.rs`, and
//! `src/mcp/`.
//!
//! # Cargo features
//!
//! - `ui` (default) — compiles in winit + wgpu for the interactive
//!   verifier window. Required for the default UI mode.
//! - Without `ui` (`--no-default-features`) — `--script` and `--mcp`
//!   still work; the default UI path errors at runtime with a
//!   "rebuild with `--features ui`" message. The MCP debugging surface
//!   and the headless capture pipeline use this build to skip the heavy
//!   graphics stack.
//!
//! Mode selection: `--mcp` wins; otherwise a headless-only flag
//! (`--script`, `--headless`, `--frames`, `--screenshot`,
//! `--audio-capture`, `--wait-for-boot`, `--print-query`) selects
//! headless script mode; otherwise the interactive UI runs. Flags shared
//! with the UI (`--rom-dir`, `--kickstart`, `--model`, `--disk`,
//! `--scale`, `--video`) do not force headless, so `--model a500-a501
//! --disk workbench13.adf` opens the window.

mod mcp;
mod model;
mod script;

#[cfg(feature = "ui")]
mod ui;

// Re-export the model / ROM-resolution layer at the crate root so the
// UI, script, and MCP modules keep their `crate::ModelArg` /
// `crate::find_rom_path` paths. `model.rs` itself is also `#[path]`-
// included by the `mcp_smoke` test (which shares `tools.rs`), so the
// definitions live there, not here.
pub(crate) use model::{ModelArg, find_rom_path, firmware_id_for_model_arg};

use std::path::PathBuf;
use std::process;

use emu198x_shell::{MachineError, NativeAudioError};
use thiserror::Error;

pub(crate) const USAGE: &str = "\
Usage: emu198x-amiga [OPTIONS]

Options:
    --rom-dir DIR        directory containing Amiga ROM images
    --kickstart PATH     explicit ROM path (Kickstart on A500, bootstrap on A1000)
    --model MODEL        a1000 | a500 | a500-gvp-a530 | a500-a501 | a500-plus
                         | a500-maxed | a600 | a1200 | a2000 [default: a500]
    --disk PATH          insert one ADF image into DF0:
    --scale N            integer window scale, default 1
    --video MODE         raw | lcd | crt [default: raw]
    --mcp                run as headless MCP server (JSON-RPC over stdio)
                         accepts --model, --rom-dir, --kickstart; ignores --scale/--video/--disk
                         default --model is a500 (canonical Amiga, KS 1.3 ROM)
    --help, -h           show this help

Controls:
    Esc                  quit
    F12                  hard reset (keeps the inserted disk)
    Cmd/Ctrl+S / +L      quick save / load state
    Mouse                port-1 Amiga mouse (JOY0DAT)
    Gamepad              port-2 Amiga joystick (JOY1DAT)
    Page Up              toggle arrow/space joystick mode for port 2
    A-Z, 0-9             Amiga keyboard
    Space, Enter, Tab    Amiga keyboard
    Backspace            Amiga keyboard
    Machine menu         switch model live (A1000 / A500 family / A600 / A1200 / A2000)

ROM directory resolution (first match wins):
    1. --rom-dir DIR
    2. EMU198X_AMIGA_ROM_DIR
    3. ~/.emu198x/roms/commodore-amiga
    4. ~/.emu198x/roms/amiga

Headless (add --no-default-features to skip the graphics stack):
    emu198x-amiga --script steps.json --model a500-a501 --disk workbench13.adf
    emu198x-amiga --wait-for-boot 300 --screenshot kick13.png

Examples:
    emu198x-amiga --model a500-a501 --disk workbench13.adf
    emu198x-amiga --kickstart kick13.rom --disk workbench13.adf
    emu198x-amiga --model a1000 --kickstart a1000-bootstrap.rom --disk kick12.adf
";

/// Top-level error for the headless modes (script + MCP). The interactive UI
/// mode runs on the shared `emu198x-ui` harness, which owns its own error type
/// and surfaces failures as a `String`, so no UI-specific arms live here; the
/// MCP and script paths construct `Machine` / `Io` / `MissingRom` / `Setup`.
#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Machine(#[from] MachineError),

    #[error("invalid --scale value {value}")]
    InvalidScale { value: u32 },

    #[error("{reason}")]
    Setup { reason: String },

    #[error(transparent)]
    Audio(#[from] NativeAudioError),

    /// I/O error from the MCP mode's stdio loop or ROM read.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// MCP / headless boot ROM missing for the chosen model.
    #[error("Kickstart ROM not found at {path}")]
    MissingRom { path: String },
}

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
/// (`--rom-dir`, `--kickstart`, `--model`, `--disk`, `--scale`,
/// `--video`) are intentionally absent so an interactive invocation
/// opens the window.
fn is_script_flag(arg: &str) -> bool {
    matches!(
        arg,
        "--script"
            | "--headless"
            | "--frames"
            | "--screenshot"
            | "--audio-capture"
            | "--wait-for-boot"
            | "--print-query"
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let result: Result<(), String> = match detect_mode(&args) {
        Mode::Ui => run_ui(args),
        Mode::Script => script::run(args),
        Mode::Mcp => mcp::run(parse_mcp_cli(args)).map_err(|err| err.to_string()),
    };

    if let Err(err) = result {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

#[cfg(feature = "ui")]
fn run_ui(args: Vec<String>) -> Result<(), String> {
    let cli = ui::parse_cli(args);
    ui::run(cli)
}

#[cfg(not(feature = "ui"))]
fn run_ui(_args: Vec<String>) -> Result<(), String> {
    Err("this binary was built without the `ui` feature; rebuild with `--features ui` for interactive mode, or use --script / --mcp instead".to_owned())
}

/// Parse the MCP-relevant subset of CLI flags. The MCP mode skips
/// `--scale`, `--video`, `--disk` (no UI / no media-management on the
/// JSON-RPC surface today) — accepting them is a soft error so users
/// mistyping the mode get a clear hint.
fn parse_mcp_cli<I>(args: I) -> mcp::McpCli
where
    I: IntoIterator<Item = String>,
{
    // Default to the canonical Amiga — A500 OCS PAL with Kickstart 1.3.
    let mut model = ModelArg::A500;
    let mut rom_dir: Option<PathBuf> = None;
    let mut kickstart: Option<PathBuf> = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--mcp" => {} // mode selector — already handled
            "--model" => model = parse_model_arg(&next_arg(&mut iter, "--model")),
            "--rom-dir" => rom_dir = Some(PathBuf::from(next_arg(&mut iter, "--rom-dir"))),
            "--kickstart" => {
                kickstart = Some(PathBuf::from(next_arg(&mut iter, "--kickstart")));
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                process::exit(0);
            }
            // Flags that belong to the windowed-UI path only.
            "--scale" | "--video" | "--disk" => {
                let _ = next_arg(&mut iter, &arg); // consume the value
                eprintln!("warning: {arg} has no effect in --mcp mode");
            }
            _ => die(&format!("unknown flag: {arg}")),
        }
    }
    mcp::McpCli {
        model,
        rom_dir,
        kickstart,
    }
}

pub(crate) fn parse_model_arg(value: &str) -> ModelArg {
    ModelArg::from_id(value).unwrap_or_else(|| {
        die("--model expects a1000, a500, a500-gvp-a530, a500-a501, a500-plus, a500-maxed, a600, a1200, or a2000")
    })
}

pub(crate) fn next_arg<I>(iter: &mut I, flag: &str) -> String
where
    I: Iterator<Item = String>,
{
    iter.next()
        .unwrap_or_else(|| die(&format!("{flag} requires a path or value")))
}

pub(crate) fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!();
    eprintln!("{USAGE}");
    process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_mode_defaults_to_ui_with_no_args() {
        assert_eq!(detect_mode(&[]), Mode::Ui);
    }

    #[test]
    fn detect_mode_treats_interactive_flags_as_ui() {
        let args = vec![
            "--model".to_owned(),
            "a500-a501".to_owned(),
            "--disk".to_owned(),
            "wb.adf".to_owned(),
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
            "--print-query",
        ] {
            let args = vec!["--disk".to_owned(), "wb.adf".to_owned(), flag.to_owned()];
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

    #[test]
    fn parse_model_arg_covers_full_family() {
        use runtime_commodore_amiga::Model;
        assert_eq!(parse_model_arg("a1200").to_model(), Model::A1200AgaPal);
        assert_eq!(
            parse_model_arg("a500-maxed").to_model(),
            Model::A500OcsPalMaxed
        );
        assert_eq!(parse_model_arg("a1000").to_model(), Model::A1000OcsPal);
    }

    #[test]
    fn parse_mcp_cli_defaults_to_a500() {
        let cli = parse_mcp_cli(["--mcp".to_owned()]);
        assert_eq!(cli.model, ModelArg::A500);
        assert!(cli.rom_dir.is_none());
        assert!(cli.kickstart.is_none());
    }

    #[test]
    fn parse_mcp_cli_accepts_model_and_rom_overrides() {
        let cli = parse_mcp_cli([
            "--mcp".to_owned(),
            "--model".to_owned(),
            "a500-plus".to_owned(),
            "--kickstart".to_owned(),
            "/tmp/kick204.rom".to_owned(),
        ]);
        assert_eq!(cli.model, ModelArg::A500Plus);
        assert_eq!(cli.kickstart, Some(PathBuf::from("/tmp/kick204.rom")));
    }

    #[cfg(not(feature = "ui"))]
    #[test]
    fn run_ui_errors_when_feature_off() {
        assert!(run_ui(vec![]).is_err());
    }
}
