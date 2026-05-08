//! MCP server mode — *stub*.
//!
//! Reserved by SOLID criterion 4 ("Pipeline / single binary") so the
//! binary can advertise three modes — `--ui` (default), `--script`,
//! `--mcp` — without actually implementing the MCP protocol yet. SOLID
//! criterion 5 ("MCP server functional, exercised by Code198x") tracks
//! the implementation work; that lands in its own commit when criterion
//! 5 is tackled.
//!
//! When implementation begins, the eventual surface will be:
//! - Transport: stdin/stdout MCP protocol (jsonrpc-style framing)
//! - Tools: every script verb becomes a tool (`set_machine`,
//!   `load_media`, `media_transport`, `wait_for_boot`, `query`,
//!   `save_screenshot`, `save_audio_capture`, `load_snapshot`,
//!   `save_snapshot`, `autoload_tape`, …)
//! - State: a single `Box<dyn LiveSpectrumRuntime>` like the UI's
//!   `SpectrumRunner`, swapped in-place on `set_machine` calls
//!   (same enum-of-sessions wrapper that script-mode SetMachine
//!   support will introduce).
//! - At least one Code198x skill exercises the server end-to-end as
//!   the SOLID criterion 5 acceptance bar.
//!
//! For now: print a clear message and exit non-zero so callers don't
//! think the invocation succeeded.

use crate::AppError;

/// Runs MCP mode. Currently a stub that returns
/// [`AppError::McpNotImplemented`]; the dispatcher in `main.rs` prints
/// the error and exits non-zero.
pub fn run() -> Result<(), AppError> {
    Err(AppError::McpNotImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_returns_not_implemented_error() {
        assert!(matches!(run(), Err(AppError::McpNotImplemented)));
    }
}
