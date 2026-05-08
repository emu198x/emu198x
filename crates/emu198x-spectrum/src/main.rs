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
}

fn main() {
    // Mode dispatch is currently UI-only; --headless / --script / --mcp
    // arrive in a follow-up commit. For now, every invocation runs the
    // UI path with the existing CLI surface.
    let cli = ui::parse_cli(std::env::args().skip(1));
    if let Err(err) = ui::run(cli) {
        eprintln!("error: {err}");
        process::exit(1);
    }
}
