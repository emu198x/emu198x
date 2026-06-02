//! MCP server mode — `--mcp` / `--mcp-stdio`.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_common_tools,
};
use runtime_acorn_bbc_micro::{BbcMicroRuntime, BbcMicroSessionQueryProvider, Model};

use crate::mcp_tools::register_bbc_tools;

const FRAME_TICKS_PAL: u64 = 2_000_000 / 50;

/// Runs MCP mode.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let mut machine = BbcMicroRuntime::blank(Model::BbcModelB);
    if let Some(path) = rom_path()
        && let Ok(bytes) = fs::read(&path) {
            if bytes.len() == 16 * 1024 {
                machine
                    .set_mos(bytes)
                    .map_err(|err| format!("MOS invalid: {err}"))?;
                eprintln!(
                    "emu198x-acorn-bbc-micro mcp: loaded MOS from {}",
                    path.display()
                );
            } else {
                eprintln!(
                    "emu198x-acorn-bbc-micro mcp: MOS at {} is {} bytes; expected 16384 — starting blank",
                    path.display(),
                    bytes.len()
                );
            }
        }

    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS_PAL,
        BbcMicroSessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-acorn-bbc-micro",
        env!("CARGO_PKG_VERSION"),
    ));
    register_common_tools(server.registry_mut());
    register_bbc_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_BBC_MOS")
        && !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    let home = env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(".emu198x/roms/acorn-bbc-micro/os.rom");
    if default.exists() { Some(default) } else { None }
}
