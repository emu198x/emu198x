//! MCP server mode — `--mcp` / `--mcp-stdio`.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::{register_base_tools, register_keyboard_tools},
};
use runtime_atari_800xl::{Atari800xlRuntime, Atari800xlSessionQueryProvider, Model};

const FRAME_TICKS_NTSC: u64 = 262 * 228;

/// Runs MCP mode. Starts blank — OS / BASIC / cart arrive via load_media.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let machine = Atari800xlRuntime::blank(Model::A800xlNtsc);
    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS_NTSC,
        Atari800xlSessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-atari-800xl",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    // The 800XL has a keyboard, so the shared press_key / type_string apply.
    register_keyboard_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
