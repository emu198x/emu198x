//! MCP server mode — `--mcp` / `--mcp-stdio`.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_common_tools,
};
use runtime_mattel_aquarius::{AquariusRuntime, AquariusSessionQueryProvider, Model};

use crate::mcp_tools::register_aquarius_tools;

/// Aquarius runs at ~3.58 MHz CPU; PAL frame = ~71,569 T-states.
const FRAME_TICKS_PAL: u64 = 71_590;

/// Runs MCP mode. Loads BIOS from `EMU198X_AQUARIUS_BIOS` or default path.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let mut machine = AquariusRuntime::blank(Model::Aquarius);
    if let Some(path) = bios_path() {
        if let Ok(bytes) = fs::read(&path) {
            if bytes.len() == 8 * 1024 {
                machine
                    .set_bios(bytes)
                    .map_err(|err| format!("BIOS invalid: {err}"))?;
                eprintln!(
                    "emu198x-mattel-aquarius mcp: loaded BIOS from {}",
                    path.display()
                );
            } else {
                eprintln!(
                    "emu198x-mattel-aquarius mcp: BIOS at {} is {} bytes; expected 8192 — starting blank",
                    path.display(),
                    bytes.len()
                );
            }
        }
    }

    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS_PAL,
        AquariusSessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-mattel-aquarius",
        env!("CARGO_PKG_VERSION"),
    ));
    register_common_tools(server.registry_mut());
    register_aquarius_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn bios_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_AQUARIUS_BIOS") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    let home = env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(".emu198x/roms/mattel-aquarius/aquarius.rom");
    if default.exists() { Some(default) } else { None }
}
