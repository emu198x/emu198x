//! MCP server mode — `--mcp` / `--mcp-stdio`.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_common_tools,
};
use runtime_jupiter_ace::{JupiterAceRuntime, JupiterAceSessionQueryProvider, Model};

use crate::mcp_tools::register_ace_tools;

// Jupiter Ace runs at ~3.25 MHz, ~50 Hz PAL → ~65,000 t-states/frame.
const FRAME_TICKS: u64 = 65_000;

/// Runs MCP mode. Starts blank — ROM arrives via firmware load.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let machine = JupiterAceRuntime::blank(Model::Ace3k);
    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS,
        JupiterAceSessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-jupiter-ace",
        env!("CARGO_PKG_VERSION"),
    ));
    register_common_tools(server.registry_mut());
    register_ace_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
