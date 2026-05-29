//! MCP server mode — `--mcp`.
//!
//! Boots a Dragon 32 session with the BASIC ROM resolved from the
//! environment (or the default ROM directory) and exposes the shared
//! machine-agnostic tool surface (run frames, query state, load media,
//! snapshots, capture) over JSON-RPC stdio. Unlike the cartridge
//! machines, the Dragon boot ROM is firmware, not loadable media, so it
//! must be present at startup — the same way the Spectrum and Amiga MCP
//! servers resolve their ROMs. A client (or Claude) then drives the
//! session to debug it.

use std::path::PathBuf;

use emu198x_shell::{
    FirmwareImage, FirmwareSet, HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_common_tools,
    read_firmware_asset,
};
use runtime_dragon::{DragonRuntime, DragonSessionQueryProvider, Model};

const DRAGON_FRAME_CYCLES: u64 = 894_886 / 50;
const ROM_ENV: &str = "EMU198X_DRAGON32_ROM";

/// Runs MCP mode: resolves the Dragon 32 ROM, builds the session,
/// registers the shared tools, and drives the stdio loop until stdin
/// closes.
///
/// # Errors
///
/// Returns an error string if the ROM cannot be found or loaded, the
/// runtime fails to build, or the JSON-RPC stdio loop hits an I/O
/// failure.
pub fn run() -> Result<(), String> {
    let rom_path = resolve_dragon32_rom()?;
    let rom = read_firmware_asset(&rom_path)
        .map_err(|err| format!("failed to load Dragon 32 ROM {}: {err}", rom_path.display()))?;

    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("dragon32-basic-rom", &rom.bytes));
    let runtime = DragonRuntime::from_firmware(Model::Dragon32Pal, &firmware)
        .map_err(|err| format!("failed to build Dragon runtime: {err}"))?;

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        DRAGON_FRAME_CYCLES,
        DragonSessionQueryProvider,
    );

    let mut server = Server::new(ServerInfo::new("emu198x-dragon", env!("CARGO_PKG_VERSION")));
    register_common_tools(server.registry_mut());

    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn resolve_dragon32_rom() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(ROM_ENV) {
        return Ok(PathBuf::from(path));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = PathBuf::from(home).join(".emu198x/roms/dragon/dragon32.rom");
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "Dragon 32 ROM not found; set {ROM_ENV} or place the ROM at ~/.emu198x/roms/dragon/dragon32.rom"
    ))
}
