//! MCP server mode — `--mcp`.
//!
//! Boots a blank NTSC NES session and exposes:
//! - the shared base surface via `register_base_tools` — run/query/media/
//!   capture (`register_common_tools`) plus the generic 6502 debug verbs
//!   (`query_cpu`, `memory_read`, `disasm`, `step`, `poke_*`,
//!   `run_until_pc`, `run_until_any_pc`, `run_until_mem_change`) driven
//!   through the NES `DebugTarget`, AND
//! - the NES-specific PPU dumps (`dump_palette`, `dump_oam`,
//!   `dump_nametable`) via `register_nes_tools`. The debug verbs the NES
//!   once shadowed are now served by the shared tier (RULES.md #30).
//!
//! The chip-register snapshots (`cpu` / `ppu` / `apu` / `mapper`) are
//! served as folded query paths on the generic `query` tool, not as
//! bespoke tools (#456).
//!
//! Cartridge is loaded at runtime via the `load_media` tool, so
//! the server starts without media — the client drives it the
//! same way the `--script` path drives a JSON session.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_base_tools,
};
use runtime_nintendo_nes::{Model, NesRuntime, NesSessionQueryProvider};

use crate::mcp_tools::register_nes_tools;

const NES_FRAME_TICKS: u64 = 341 * 262;

/// Runs MCP mode. Builds the blank session, registers the shared
/// and NES-specific tool surfaces, and drives the stdio loop
/// until stdin closes.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run(args: &[String]) -> Result<(), String> {
    let machine = NesRuntime::blank(Model::NesNtsc);
    let mut session =
        HeadlessSession::new_with_query_provider(machine, NES_FRAME_TICKS, NesSessionQueryProvider);

    // Media named on the command line is loaded here so `--rom`
    // means the same thing in MCP mode as in the other two (#1180).
    emu198x_shell::startup_media::load_into(&mut session, args)?;

    let mut server = Server::new(ServerInfo::new("emu198x-nes", env!("CARGO_PKG_VERSION")));
    register_base_tools(server.registry_mut());
    register_nes_tools(server.registry_mut());

    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
