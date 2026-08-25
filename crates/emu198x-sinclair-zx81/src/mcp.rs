//! MCP server mode — `--mcp` / `--mcp-stdio`.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::{register_base_tools, register_keyboard_tools},
};
use runtime_sinclair_zx81::{Model, Zx81Runtime, Zx81SessionQueryProvider};

/// Frame budget for the board the runtime is configured as. The 60 Hz strap
/// lays out a much shorter field, so this cannot be one shared constant.
fn frame_ticks(model: Model) -> u64 {
    u64::from(model.television_standard().slow_mode_frame_tstates())
}

/// Runs MCP mode. Loads ROM from `EMU198X_ZX81_ROM` or default path.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run(args: &[String]) -> Result<(), String> {
    let model = Model::Zx81;
    let mut machine = Zx81Runtime::blank(model);
    if let Some(path) = rom_path()
        && let Ok(bytes) = fs::read(&path)
    {
        if bytes.len() == 8 * 1024 {
            machine
                .set_rom(bytes)
                .map_err(|err| format!("ROM invalid: {err}"))?;
            eprintln!(
                "emu198x-sinclair-zx81 mcp: loaded ROM from {}",
                path.display()
            );
        } else {
            eprintln!(
                "emu198x-sinclair-zx81 mcp: ROM at {} is {} bytes; expected 8192 — starting blank",
                path.display(),
                bytes.len()
            );
        }
    }

    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        frame_ticks(model),
        Zx81SessionQueryProvider,
    );

    // Media named on the command line is loaded here so `--rom`
    // means the same thing in MCP mode as in the other two (#1180).
    emu198x_shell::startup_media::load_into(&mut session, args)?;
    let mut server = Server::new(ServerInfo::new(
        "emu198x-sinclair-zx81",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    // The machine has a keyboard, so the shared press_key / type_string apply.
    register_keyboard_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ZX81_ROM")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(".emu198x/roms/sinclair-zx81/zx81.rom");
    if default.exists() {
        Some(default)
    } else {
        None
    }
}
