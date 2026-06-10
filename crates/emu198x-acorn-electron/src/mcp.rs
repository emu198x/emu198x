//! MCP server mode — `--mcp` / `--mcp-stdio`.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_base_tools,
};
use runtime_acorn_electron::{ElectronRuntime, ElectronSessionQueryProvider, Model};

use crate::mcp_tools::register_electron_tools;

/// Electron PAL: 312 lines × ~128 cycles/line at 2 MHz ≈ 40000 cycles.
const FRAME_TICKS_PAL: u64 = 40_000;

/// Runs MCP mode. Loads OS + BASIC from `EMU198X_ELECTRON_OS` /
/// `EMU198X_ELECTRON_BASIC` (or default paths) when present.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let mut machine = ElectronRuntime::blank(Model::Electron);
    if let (Some(os_path), Some(basic_path)) = (rom_path("OS"), rom_path("BASIC"))
        && let (Ok(os), Ok(basic)) = (fs::read(&os_path), fs::read(&basic_path))
    {
        if os.len() == 16 * 1024 && basic.len() == 16 * 1024 {
            machine
                .set_roms(os, basic)
                .map_err(|err| format!("ROM invalid: {err}"))?;
            eprintln!(
                "emu198x-acorn-electron mcp: loaded OS={} BASIC={}",
                os_path.display(),
                basic_path.display(),
            );
        } else {
            eprintln!(
                "emu198x-acorn-electron mcp: ROM sizes wrong (OS={} bytes, BASIC={} bytes) — starting blank",
                os.len(),
                basic.len()
            );
        }
    }

    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS_PAL,
        ElectronSessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-acorn-electron",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    register_electron_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn rom_path(kind: &str) -> Option<PathBuf> {
    let env_key = format!("EMU198X_ELECTRON_{kind}");
    if let Ok(p) = env::var(&env_key)
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    let file = format!("{}.rom", kind.to_ascii_lowercase());
    let default = PathBuf::from(home).join(format!(".emu198x/roms/acorn-electron/{file}"));
    if default.exists() {
        Some(default)
    } else {
        None
    }
}
