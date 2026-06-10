//! MCP server mode — `--mcp` / `--mcp-stdio`.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_base_tools,
};
use runtime_atari_2600::{Atari2600Runtime, Atari2600SessionQueryProvider, Model};

/// Atari 2600 NTSC frame = 262 lines × 228 colour clocks.
const FRAME_TICKS_NTSC: u64 = 262 * 228;

/// Runs MCP mode. Starts blank; cartridge arrives via load_media.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let machine = Atari2600Runtime::blank(Model::Vcs2600Ntsc);
    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS_NTSC,
        Atari2600SessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-atari-2600",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
