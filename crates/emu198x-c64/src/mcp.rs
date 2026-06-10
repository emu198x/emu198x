//! MCP server mode — `--mcp`.
//!
//! Boots a C64 session with KERNAL/BASIC/chargen/1541 firmware resolved
//! from the default ROM directory and exposes the shared
//! machine-agnostic tool surface (run frames, query state, load media,
//! snapshots, capture) over JSON-RPC stdio. The boot ROMs are firmware,
//! not loadable media, so they must be present at startup — the same way
//! the Spectrum and Amiga MCP servers resolve their ROMs. A client (or
//! Claude) then drives the session to debug it.

use emu198x_shell::mcp::{Server, ServerInfo, serve_stdio};
use emu198x_shell::mcp_tools::register_base_tools;

use crate::mcp_tools::register_c64_tools;

/// Runs MCP mode: builds the booted session, registers the shared tools,
/// and drives the stdio loop until stdin closes.
///
/// # Errors
///
/// Returns an error string if the C64 ROMs cannot be resolved, the
/// machine fails to boot, or the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let mut session = crate::script::mcp_session()?;

    let mut server = Server::new(ServerInfo::new("emu198x-c64", env!("CARGO_PKG_VERSION")));
    register_base_tools(server.registry_mut());
    register_c64_tools(server.registry_mut());

    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
