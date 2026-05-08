//! MCP server mode.
//!
//! Boots the same eager 48K runtime that script mode uses, builds a
//! shell-side `Server` with one tool per `ScriptStep` variant, and
//! drives the JSON-RPC stdio loop. Tool dispatch goes through the
//! same `execute_step` interceptor as `--script`, so SetMachine /
//! AutoloadTape / LoadBasicProgram behave identically across both
//! modes.
//!
//! See `docs/brainstorms/2026-05-08-mcp-server-brainstorm.md` for the
//! design and the SOLID criterion 5 acceptance bar.

mod tools;

use common_sinclair_zx_spectrum::timing::TIMING_48K;
use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
};
use runtime_sinclair_zx_spectrum::SpectrumSessionQueryProvider;

use crate::AppError;
use crate::script::runner::boot_eager_48k;

/// Runs MCP mode. Boots an eager 48K session, registers every tool,
/// and runs the stdio loop until stdin closes.
///
/// # Errors
///
/// Returns an error if the 48K ROM cannot be loaded or the stdio loop
/// hits an I/O failure.
pub fn run() -> Result<(), AppError> {
    let runtime = boot_eager_48k()?;
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let mut server = Server::new(ServerInfo::new(
        "emu198x-spectrum",
        env!("CARGO_PKG_VERSION"),
    ));
    tools::register_all(server.registry_mut());

    serve_stdio(&mut server, &mut session).map_err(AppError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_all_publishes_eighteen_tools() {
        let mut server: Server<tools::SpectrumSession> =
            Server::new(ServerInfo::new("emu198x-spectrum", "0.0.0"));
        tools::register_all(server.registry_mut());
        assert_eq!(server.registry().len(), 18);
    }
}
