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
//!
//! # Cargo features
//!
//! - `ui` (default) — compiles in winit + wgpu + muda for the
//!   interactive Machine / File / State / View menus and the framed
//!   audio/video loop. Required for `--ui` mode.
//! - Without `ui` — `--script` and `--mcp` modes still work; `--ui`
//!   errors at runtime with a "rebuild with `--features ui`" message.
//!   Code198x's headless screenshot/video pipeline uses this build to
//!   skip the heavy graphics stack.

mod live_machine;
mod machine;
mod mcp;
mod script;

#[cfg(feature = "ui")]
mod ui;

use std::process;

use emu198x_shell::{AssetLoadError, MachineError, NativeAudioError, QueryError};
use thiserror::Error;

#[cfg(feature = "ui")]
use emu198x_native_video::VideoPresenterError;
#[cfg(feature = "ui")]
use winit::error::{EventLoopError, OsError};

use crate::machine::FirmwareError;

/// Top-level error type used across every mode. UI-only error arms
/// (window/audio/video) are gated behind the `ui` feature so headless
/// builds don't pull winit / wgpu / native-video unnecessarily.
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

    /// `NativeAudioError` lives in `emu198x-shell` (which always
    /// pulls cpal), so this arm is available regardless of the `ui`
    /// feature. The error itself is only constructed by the UI mode.
    #[error(transparent)]
    Audio(#[from] NativeAudioError),

    #[cfg(feature = "ui")]
    #[error(transparent)]
    Video(#[from] VideoPresenterError),

    #[cfg(feature = "ui")]
    #[error(transparent)]
    EventLoop(#[from] EventLoopError),

    #[cfg(feature = "ui")]
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

    /// `--ui` mode requested but the binary was built without the
    /// `ui` Cargo feature. Surfaces only on `--no-default-features`
    /// headless builds.
    #[error(
        "this binary was built without the `ui` feature; rebuild with `--features ui` for interactive mode, or use --script / --mcp instead"
    )]
    UiNotCompiledIn,
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
        Mode::Ui => run_ui(args),
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

#[cfg(feature = "ui")]
fn run_ui(args: Vec<String>) -> Result<(), AppError> {
    let cli = ui::parse_cli(args);
    ui::run(cli)
}

#[cfg(not(feature = "ui"))]
fn run_ui(_args: Vec<String>) -> Result<(), AppError> {
    Err(AppError::UiNotCompiledIn)
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

    #[cfg(not(feature = "ui"))]
    #[test]
    fn run_ui_returns_not_compiled_in_when_feature_off() {
        assert!(matches!(run_ui(vec![]), Err(AppError::UiNotCompiledIn)));
    }
}
