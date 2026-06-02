//! MCP server mode — `--mcp` / `--mcp-stdio`.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_common_tools,
};
use runtime_coleco_colecovision::{CvRuntime, CvSessionQueryProvider, Model};

use crate::mcp_tools::register_cv_tools;

/// ColecoVision NTSC: 228 T-states × 262 scanlines.
const CV_FRAME_TICKS_NTSC: u64 = 228 * 262;

/// Runs MCP mode.
///
/// Loads BIOS from `EMU198X_COLECO_BIOS` (or
/// `~/.emu198x/roms/coleco-colecovision/colecovision.rom`) when present.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let mut machine = CvRuntime::blank(Model::CvNtsc);
    if let Some(path) = bios_path() {
        if let Ok(bytes) = fs::read(&path) {
            if bytes.len() == 8 * 1024 {
                machine
                    .set_bios(bytes)
                    .map_err(|err| format!("BIOS invalid: {err}"))?;
                eprintln!(
                    "emu198x-colecovision mcp: loaded BIOS from {}",
                    path.display()
                );
            } else {
                eprintln!(
                    "emu198x-colecovision mcp: BIOS at {} is {} bytes; expected 8192 — starting blank",
                    path.display(),
                    bytes.len()
                );
            }
        }
    }

    let mut session =
        HeadlessSession::new_with_query_provider(machine, CV_FRAME_TICKS_NTSC, CvSessionQueryProvider);
    let mut server = Server::new(ServerInfo::new(
        "emu198x-colecovision",
        env!("CARGO_PKG_VERSION"),
    ));
    register_common_tools(server.registry_mut());
    register_cv_tools(server.registry_mut());

    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn bios_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_COLECO_BIOS") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    let home = env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(".emu198x/roms/coleco-colecovision/colecovision.rom");
    if default.exists() { Some(default) } else { None }
}
