//! MCP server mode — `--mcp`.
//!
//! Boots a blank DMG session and exposes the shared machine-agnostic
//! tool surface (run frames, query state, load media, snapshots, capture)
//! over JSON-RPC stdio. The cartridge is loaded at runtime via the
//! `load_media` tool, so the server starts without media — a client (or
//! Claude) drives it the same way the `--script` path drives a JSON
//! session.

use common_nintendo_game_boy::MCYCLES_PER_FRAME;
use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::{register_common_tools, register_debug_tools},
};
use runtime_nintendo_game_boy::{GameBoyRuntime, GameBoySessionQueryProvider, Model};

/// Runs MCP mode: builds the blank session, registers the shared tools,
/// and drives the stdio loop until stdin closes.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let machine = GameBoyRuntime::blank(Model::Dmg);
    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        u64::from(MCYCLES_PER_FRAME),
        GameBoySessionQueryProvider,
    );

    let mut server = Server::new(ServerInfo::new(
        "emu198x-game-boy",
        env!("CARGO_PKG_VERSION"),
    ));
    register_common_tools(server.registry_mut());
    // SM83 debug verbs (query_cpu, memory_read, poke, disasm, step,
    // run_until_pc) via the runtime's `DebugTarget`.
    register_debug_tools(server.registry_mut());

    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
