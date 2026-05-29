//! MCP server mode — `--mcp`.
//!
//! Boots a blank NTSC NES session and exposes the shared machine-agnostic
//! tool surface (run frames, query state, load media, snapshots, capture)
//! over JSON-RPC stdio. The cartridge is loaded at runtime via the
//! `load_media` tool, so the server starts without media — the client
//! drives it the same way the `--script` path drives a JSON session.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_common_tools,
};
use runtime_nintendo_nes::{Model, NesRuntime, NesSessionQueryProvider};

const NES_FRAME_TICKS: u64 = 341 * 262;

/// Runs MCP mode: builds the blank session, registers the shared tools,
/// and drives the stdio loop until stdin closes.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let machine = NesRuntime::blank(Model::NesNtsc);
    let mut session =
        HeadlessSession::new_with_query_provider(machine, NES_FRAME_TICKS, NesSessionQueryProvider);

    let mut server = Server::new(ServerInfo::new("emu198x-nes", env!("CARGO_PKG_VERSION")));
    register_common_tools(server.registry_mut());

    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
