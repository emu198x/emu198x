//! MCP server mode — `--mcp` / `--mcp-stdio`.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_base_tools,
};
use runtime_sega_game_gear::{Model, SmsSessionQueryProvider, blank};

const GG_FRAME_TICKS: u64 = 228 * 262;

/// Runs MCP mode. Starts blank; cartridge arrives via load_media.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let machine = blank(Model::GameGear);
    let mut session =
        HeadlessSession::new_with_query_provider(machine, GG_FRAME_TICKS, SmsSessionQueryProvider);
    let mut server = Server::new(ServerInfo::new(
        "emu198x-sega-game-gear",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
