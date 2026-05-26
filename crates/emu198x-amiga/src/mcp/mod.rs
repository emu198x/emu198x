//! MCP server mode for the Amiga binary (Stage Q).
//!
//! Mirrors the Spectrum MCP pattern: boots one machine (A1200 with
//! KS 3.1 by default), registers a small tool set, drives the
//! JSON-RPC stdio loop until stdin closes.
//!
//! The Amiga MCP exists primarily as a *debugging surface* for the
//! KS-internals investigation (Stage P onward). It exposes raw chip
//! state — CPU registers, copper-list address, BPLCON0, CIA timers —
//! rather than the higher-level `ScriptStep` shape the Spectrum uses,
//! because the questions we're asking ("what's A5 right now?", "what
//! does graphics.library Text() actually do?") need that level of
//! access.
//!
//! Boot ROM resolution mirrors the existing `ks31_boot.rs` test:
//!
//!   1. `EMU198X_KS31_A1200_ROM` env var (explicit path)
//!   2. `~/.emu198x/roms/commodore-amiga/kick31a1200.rom` (default)

mod session;
mod tools;

use std::path::PathBuf;

use emu198x_shell::mcp::{Server, ServerInfo, serve_stdio};

use crate::AppError;
use session::AmigaA1200Session;

/// Runs MCP mode. Loads the KS 3.1 A1200 ROM, boots an A1200, registers
/// every tool, and runs the stdio loop until stdin closes.
///
/// # Errors
///
/// Returns an error if the ROM cannot be found / loaded or the stdio
/// loop hits an I/O failure.
pub fn run() -> Result<(), AppError> {
    let rom_path = resolve_rom_path()?;
    let rom_bytes = std::fs::read(&rom_path).map_err(AppError::Io)?;
    let mut session = AmigaA1200Session::new(rom_bytes, rom_path).map_err(AppError::Machine)?;

    let mut server: Server<AmigaA1200Session> = Server::new(ServerInfo::new(
        "emu198x-amiga",
        env!("CARGO_PKG_VERSION"),
    ));
    tools::register_all(server.registry_mut());

    serve_stdio(&mut server, &mut session).map_err(AppError::from)?;
    Ok(())
}

/// Resolve the KS 3.1 A1200 ROM path using the same order as
/// `ks31_boot.rs`.
fn resolve_rom_path() -> Result<PathBuf, AppError> {
    if let Ok(path) = std::env::var("EMU198X_KS31_A1200_ROM") {
        return Ok(PathBuf::from(path));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let default = PathBuf::from(home)
            .join(".emu198x")
            .join("roms")
            .join("commodore-amiga")
            .join("kick31a1200.rom");
        if default.exists() {
            return Ok(default);
        }
    }
    Err(AppError::MissingRom {
        path: "~/.emu198x/roms/commodore-amiga/kick31a1200.rom".to_string(),
    })
}
