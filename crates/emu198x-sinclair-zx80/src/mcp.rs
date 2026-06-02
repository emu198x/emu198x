//! MCP server mode — `--mcp` / `--mcp-stdio`.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_common_tools,
};
use runtime_sinclair_zx80::{Model, Zx80Runtime, Zx80SessionQueryProvider};

use crate::mcp_tools::register_zx80_tools;

const FRAME_TICKS_PAL: u64 = 207 * 312;

/// Runs MCP mode. Loads ROM from `EMU198X_ZX80_ROM` or default path.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let mut machine = Zx80Runtime::blank(Model::Zx80);
    if let Some(path) = rom_path() {
        if let Ok(bytes) = fs::read(&path) {
            if bytes.len() == 4 * 1024 {
                machine
                    .set_rom(bytes)
                    .map_err(|err| format!("ROM invalid: {err}"))?;
                eprintln!("emu198x-sinclair-zx80 mcp: loaded ROM from {}", path.display());
            } else {
                eprintln!(
                    "emu198x-sinclair-zx80 mcp: ROM at {} is {} bytes; expected 4096 — starting blank",
                    path.display(),
                    bytes.len()
                );
            }
        }
    }

    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS_PAL,
        Zx80SessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-sinclair-zx80",
        env!("CARGO_PKG_VERSION"),
    ));
    register_common_tools(server.registry_mut());
    register_zx80_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ZX80_ROM") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    let home = env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(".emu198x/roms/sinclair-zx80/zx80.rom");
    if default.exists() { Some(default) } else { None }
}
