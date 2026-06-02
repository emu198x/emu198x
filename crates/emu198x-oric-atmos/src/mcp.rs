//! MCP server mode — `--mcp` / `--mcp-stdio`.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_common_tools,
};
use runtime_oric_atmos::{Model, OricRuntime, OricSessionQueryProvider};

use crate::mcp_tools::register_oric_tools;

// Oric: 6502 @ 1 MHz, 50 Hz PAL → ~20,000 cycles/frame.
const FRAME_TICKS: u64 = 20_000;

/// Runs MCP mode. Starts blank — ROM arrives via firmware load.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let machine = OricRuntime::blank(Model::Atmos);
    let mut session =
        HeadlessSession::new_with_query_provider(machine, FRAME_TICKS, OricSessionQueryProvider);
    let mut server = Server::new(ServerInfo::new(
        "emu198x-oric-atmos",
        env!("CARGO_PKG_VERSION"),
    ));
    register_common_tools(server.registry_mut());
    register_oric_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
