//! MCP server mode for the Amiga binary (Stage Q).
//!
//! Mirrors the Spectrum MCP pattern: boots one machine, registers a
//! small tool set, drives the JSON-RPC stdio loop until stdin closes.
//!
//! The Amiga MCP exists primarily as a *debugging surface* for the
//! KS-internals investigation (Stage P onward). It exposes raw chip
//! state — CPU registers, copper-list address, BPLCON0, CIA timers —
//! rather than the higher-level `ScriptStep` shape the Spectrum uses,
//! because the questions we're asking ("what's A5 right now?", "what
//! does graphics.library Text() actually do?") need that level of
//! access.
//!
//! Since Stage AE-b/c/d/e, every chip-level tool drives the active
//! chipset variant through the [`AmigaLiveAccess`] trait — `--model`
//! picks OCS / ECS / AGA at boot time and the same tool set works
//! against any of them. AGA-only tooling (`query_aga`) gracefully
//! routes to the A1200 downcast.
//!
//! Default `--model` is `a500` (Kickstart 1.3) — the canonical Amiga
//! that vAmiga / FS-UAE / WinUAE also default to. Pass `--model a1200`
//! for the AGA chipset.
//!
//! ROM resolution piggybacks on the windowed UI's helpers:
//!
//!   1. `--kickstart PATH` explicit
//!   2. `--rom-dir DIR` directory
//!   3. `EMU198X_AMIGA_ROM_DIR` env var
//!   4. `~/.emu198x/roms/commodore-amiga/` or `~/.emu198x/roms/amiga/`
//!
//! Per-model candidate ROM names live in
//! [`crate::rom_candidates_for_model`].
//!
//! [`AmigaLiveAccess`]: runtime_commodore_amiga::AmigaLiveAccess

mod session;
mod tools;

use std::path::PathBuf;

use emu198x_shell::mcp::{Server, ServerInfo, serve_stdio};

use crate::{AppError, ModelArg, find_rom_path};
use session::AmigaSession;

/// MCP-mode CLI arguments. A trimmed subset of the windowed UI's
/// `Cli` — only flags relevant to a headless JSON-RPC session.
pub(crate) struct McpCli {
    pub model: ModelArg,
    pub rom_dir: Option<PathBuf>,
    pub kickstart: Option<PathBuf>,
}

/// Runs MCP mode. Resolves the boot ROM for the chosen `model`, boots
/// the matching chipset variant, registers every tool, and runs the
/// stdio loop until stdin closes.
///
/// # Errors
///
/// Returns an error if the ROM cannot be found / loaded or the stdio
/// loop hits an I/O failure.
pub fn run(cli: McpCli) -> Result<(), AppError> {
    let rom_path = find_rom_path(cli.model, cli.rom_dir.as_deref(), cli.kickstart.as_deref())
        .map_err(|reason| AppError::MissingRom { path: reason })?;
    let rom_bytes = std::fs::read(&rom_path).map_err(AppError::Io)?;
    let mut session = AmigaSession::new(cli.model.to_model(), rom_bytes, rom_path)
        .map_err(AppError::Machine)?;

    let mut server: Server<AmigaSession> = Server::new(ServerInfo::new(
        "emu198x-amiga",
        env!("CARGO_PKG_VERSION"),
    ));
    tools::register_all(server.registry_mut());

    serve_stdio(&mut server, &mut session).map_err(AppError::from)?;
    Ok(())
}
