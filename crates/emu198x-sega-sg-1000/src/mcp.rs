//! MCP server mode — `--mcp` / `--mcp-stdio`.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_base_tools,
};
use runtime_sega_sg_1000::{Model, Sg1000Runtime, Sg1000SessionQueryProvider};

use crate::mcp_tools::register_sg1000_tools;

const SG1000_FRAME_TICKS_NTSC: u64 = 228 * 262;

/// Runs MCP mode. Starts blank; cartridge arrives via load_media.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let machine = Sg1000Runtime::blank(Model::Sg1000Ntsc);
    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        SG1000_FRAME_TICKS_NTSC,
        Sg1000SessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-sega-sg-1000",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    register_sg1000_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
