//! MCP server mode — `--mcp` / `--mcp-stdio`.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_common_tools,
};
use runtime_atari_7800::{Atari7800Runtime, Atari7800SessionQueryProvider, Model};

use crate::mcp_tools::register_a7800_tools;

const FRAME_TICKS_NTSC: u64 = 262 * 228;

/// Runs MCP mode. Starts blank — cart arrives via load_media.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let machine = Atari7800Runtime::blank(Model::A7800Ntsc);
    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS_NTSC,
        Atari7800SessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-atari-7800",
        env!("CARGO_PKG_VERSION"),
    ));
    register_common_tools(server.registry_mut());
    register_a7800_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
