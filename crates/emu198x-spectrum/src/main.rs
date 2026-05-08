//! `emu198x-spectrum` — Spectrum SOLID native binary.
//!
//! One binary, three modes (UI default, script, MCP). `main.rs` is a
//! tiny dispatcher; the modes live in `src/ui/`, `src/script/`,
//! `src/mcp/`. Shared state: `src/machine.rs` (MachineKind, ROM
//! resolver) and `src/live_machine.rs` (LiveSpectrumRuntime trait +
//! per-variant `build_runtime` factory).
//!
//! See `docs/brainstorms/2026-05-08-track-1b-single-binary-brainstorm.md`
//! for the design that drove this layout.

mod live_machine;
mod machine;
mod mcp;
mod script;
mod ui;

use std::process;

use emu198x_native_video::VideoPresenterError;
use emu198x_shell::{
    AssetLoadError, MachineError, NativeAudioError, QueryError,
};
use thiserror::Error;
use winit::error::{EventLoopError, OsError};

use crate::machine::FirmwareError;

/// Top-level error type used across every mode. Mode-specific error
/// arms (UI: window/audio/video; script: file I/O; MCP: protocol) all
/// land here via `From` impls. Cargo feature gating in a follow-up
/// will conditionally compile mode-specific arms; until then every
/// arm is unconditionally available.
#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Asset(#[from] AssetLoadError),

    #[error(transparent)]
    Machine(#[from] MachineError),

    #[error(transparent)]
    Query(#[from] QueryError),

    #[error(transparent)]
    Session(#[from] emu198x_shell::SessionError),

    #[error(transparent)]
    SpectrumAutoload(#[from] runtime_sinclair_zx_spectrum::SpectrumAutoloadError),

    #[error(transparent)]
    Audio(#[from] NativeAudioError),

    #[error(transparent)]
    Video(#[from] VideoPresenterError),

    #[error(transparent)]
    EventLoop(#[from] EventLoopError),

    #[error(transparent)]
    Os(#[from] OsError),

    #[error("invalid --scale value {value}")]
    InvalidScale { value: u32 },

    #[error("no ROM supplied and default Spectrum ROM was not found at {path}")]
    MissingRom { path: String },

    #[error(transparent)]
    Firmware(#[from] FirmwareError),

    #[error("tape transport requested without tape media")]
    MissingTape,

    #[error("--autoload-tape conflicts with --play-tape")]
    ConflictingTapeWorkflow,

    /// One script step is recognised by the shell vocabulary but not
    /// yet handled by this binary (currently `set_machine`).
    #[error("script step `{step}` is unsupported: {reason}")]
    ScriptUnsupported {
        /// The step's serde tag (e.g. `"set_machine"`).
        step: &'static str,
        /// Human-readable reason for the binary's refusal.
        reason: String,
    },

    /// `--mcp` mode is reserved by SOLID criterion 4 but its
    /// implementation lands in a follow-up commit. Fail loudly so the
    /// caller doesn't think it succeeded.
    #[error("--mcp mode is not yet implemented")]
    McpNotImplemented,
}

/// Mode-flag detection. Scans args for `--mcp` / `--headless` /
/// `--script`; defaults to UI when none of those appear. The
/// dispatcher then hands the same arg list to the per-mode parser
/// (which knows how to consume its own flags).
#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Ui,
    Script,
    Mcp,
}

fn detect_mode(args: &[String]) -> Mode {
    if args.iter().any(|a| a == "--mcp") {
        Mode::Mcp
    } else if args.iter().any(|a| a == "--headless" || a == "--script") {
        Mode::Script
    } else {
        Mode::Ui
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = detect_mode(&args);

    let result = match mode {
        Mode::Ui => {
            let cli = ui::parse_cli(args);
            ui::run(cli)
        }
        Mode::Script => {
            let cli = script::parse_cli(args);
            script::run(cli)
        }
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
    fn detect_mode_defaults_to_ui_with_no_args() {
        assert_eq!(detect_mode(&[]), Mode::Ui);
    }

    #[test]
    fn detect_mode_recognises_script_via_script_flag() {
        let args = vec!["--script".to_owned(), "boot.json".to_owned()];
        assert_eq!(detect_mode(&args), Mode::Script);
    }

    #[test]
    fn detect_mode_recognises_script_via_headless_flag() {
        let args = vec!["--headless".to_owned()];
        assert_eq!(detect_mode(&args), Mode::Script);
    }

    #[test]
    fn detect_mode_mcp_takes_precedence_over_script() {
        let args = vec![
            "--mcp".to_owned(),
            "--script".to_owned(),
            "boot.json".to_owned(),
        ];
        assert_eq!(detect_mode(&args), Mode::Mcp);
    }
}
