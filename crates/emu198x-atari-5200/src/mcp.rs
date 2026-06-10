//! MCP server mode — `--mcp` / `--mcp-stdio`.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_base_tools,
};
use runtime_atari_5200::{Atari5200Runtime, Atari5200SessionQueryProvider, Model};

const FRAME_TICKS_NTSC: u64 = 262 * 228;

/// Runs MCP mode. Starts blank — cart arrives via load_media.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let machine = Atari5200Runtime::blank(Model::A5200Ntsc);
    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS_NTSC,
        Atari5200SessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-atari-5200",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
