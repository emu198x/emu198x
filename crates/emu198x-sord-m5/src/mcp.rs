//! MCP server mode — `--mcp` / `--mcp-stdio`.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_base_tools,
};
use runtime_sord_m5::{M5Runtime, M5SessionQueryProvider, Model};

use crate::mcp_tools::register_m5_tools;

const FRAME_TICKS_NTSC: u64 = 228 * 262;

/// Runs MCP mode.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let mut machine = M5Runtime::blank(Model::M5Ntsc);
    if let Some(path) = rom_path()
        && let Ok(bytes) = fs::read(&path)
    {
        machine.set_rom(bytes);
        eprintln!("emu198x-sord-m5 mcp: loaded ROM from {}", path.display());
    }

    let mut session =
        HeadlessSession::new_with_query_provider(machine, FRAME_TICKS_NTSC, M5SessionQueryProvider);
    let mut server = Server::new(ServerInfo::new(
        "emu198x-sord-m5",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    register_m5_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_SORD_M5_ROM")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(".emu198x/roms/sord-m5/sord-m5.rom");
    if default.exists() {
        Some(default)
    } else {
        None
    }
}
