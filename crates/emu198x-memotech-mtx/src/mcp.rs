//! MCP server mode — `--mcp` / `--mcp-stdio`.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::{register_base_tools, register_keyboard_tools},
};
use runtime_memotech_mtx::{Model, MtxRuntime, MtxSessionQueryProvider};

// Stay below the TMS9918A's approximately 79,746-T-state PAL frame. The
// frame-granular runtime always finishes the current frame, so an 80,000-tick
// request crossed the next boundary and emitted two frames per MCP call.
const FRAME_TICKS_PAL: u64 = 79_700;

/// Runs MCP mode.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let mut machine = MtxRuntime::blank(Model::Mtx500);
    if let Some(path) = rom_path()
        && let Ok(bytes) = fs::read(&path)
    {
        if bytes.len() >= 16 * 1024 && bytes.len().is_multiple_of(0x2000) {
            machine
                .set_rom(bytes)
                .map_err(|err| format!("ROM invalid: {err}"))?;
            eprintln!(
                "emu198x-memotech-mtx mcp: loaded ROM from {}",
                path.display()
            );
        } else {
            eprintln!(
                "emu198x-memotech-mtx mcp: ROM at {} is {} bytes; expected the 8 KB OS \
                 plus 8 KB paged ROMs (a multiple of 8192) — starting blank",
                path.display(),
                bytes.len()
            );
        }
    }

    let mut session =
        HeadlessSession::new_with_query_provider(machine, FRAME_TICKS_PAL, MtxSessionQueryProvider);
    let mut server = Server::new(ServerInfo::new(
        "emu198x-memotech-mtx",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    // The machine has a keyboard, so the shared press_key / type_string apply.
    register_keyboard_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_MTX_ROM")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(".emu198x/roms/memotech-mtx/mtx.rom");
    if default.exists() {
        Some(default)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_budget_runs_exact_requested_frame_count() {
        let runtime = MtxRuntime::new(Model::Mtx500, vec![0; 16 * 1024]).expect("valid test ROM");
        let mut session = HeadlessSession::new(runtime, FRAME_TICKS_PAL);

        session.run_frames(1).expect("first frame");
        assert_eq!(
            session.machine().machine().expect("machine").frame_count(),
            1
        );

        session.run_frames(3).expect("three more frames");
        assert_eq!(
            session.machine().machine().expect("machine").frame_count(),
            4
        );
    }
}
