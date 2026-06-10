//! MCP server mode — `--mcp` / `--mcp-stdio`.
//!
//! Boots a blank NTSC MSX1 session (no BIOS yet) and exposes the shared
//! base tool surface (run frames / ticks, query state, load media,
//! snapshots, capture, CPU/memory/disasm debug) via `register_base_tools`.
//! MSX chip state — VDP, the AY-3-8910 PSG (`ay.*`), the 8255 PPI — is read
//! through the generic `query` tool as query paths (`vdp`, `ay`, `ppi`, and
//! their leaves), not bespoke tools.
//!
//! BIOS + cartridges arrive via the shared `load_media` tool — the
//! client drives the session the same way the `--script` path does.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_base_tools,
};
use runtime_msx::{Model, MsxRuntime, MsxSessionQueryProvider};

/// One MSX1 NTSC frame = 228 T-states × 262 scanlines.
const MSX_FRAME_TICKS_NTSC: u64 = 228 * 262;

/// Runs MCP mode.
///
/// Loads BIOS from `EMU198X_MSX_BIOS` (or `~/.emu198x/roms/microsoft-msx/msx.rom`)
/// when present, otherwise starts blank — the client can drive the machine
/// against a snapshot or wait for a future `load_firmware` tool.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let mut machine = MsxRuntime::blank(Model::Msx1Ntsc);

    if let Some(path) = bios_path() {
        if let Ok(bytes) = fs::read(&path) {
            if bytes.len() == 32 * 1024 {
                machine
                    .set_bios(bytes)
                    .map_err(|err| format!("BIOS at {} invalid: {err}", path.display()))?;
                eprintln!("emu198x-msx mcp: loaded BIOS from {}", path.display());
            } else {
                eprintln!(
                    "emu198x-msx mcp: BIOS at {} is {} bytes; expected 32768 — starting blank",
                    path.display(),
                    bytes.len()
                );
            }
        } else {
            eprintln!(
                "emu198x-msx mcp: BIOS path {} not readable — starting blank",
                path.display()
            );
        }
    }

    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        MSX_FRAME_TICKS_NTSC,
        MsxSessionQueryProvider,
    );

    let mut server = Server::new(ServerInfo::new("emu198x-msx", env!("CARGO_PKG_VERSION")));
    register_base_tools(server.registry_mut());

    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn bios_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_MSX_BIOS")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(".emu198x/roms/microsoft-msx/msx.rom");
    if default.exists() {
        Some(default)
    } else {
        None
    }
}
