//! MCP server mode — `--mcp` / `--mcp-stdio`.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_common_tools,
};
use runtime_commodore_vic_20::{Model, Vic20Runtime, Vic20SessionQueryProvider};

use crate::mcp_tools::register_vic20_tools;

const FRAME_TICKS_PAL: u64 = 71 * 312;

/// Runs MCP mode. Loads ROMs from `EMU198X_VIC20_{KERNAL,BASIC,CHAR}`
/// or `~/.emu198x/roms/commodore-vic-20/` when present.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let mut machine = Vic20Runtime::blank(Model::Vic20Pal);
    let kernal = rom_path("KERNAL", "kernal.rom");
    let basic = rom_path("BASIC", "basic.rom");
    let char_rom = rom_path("CHAR", "chargen.rom");
    if let (Some(kp), Some(bp), Some(cp)) = (kernal.as_ref(), basic.as_ref(), char_rom.as_ref())
        && let (Ok(k), Ok(b), Ok(c)) = (fs::read(kp), fs::read(bp), fs::read(cp)) {
            if k.len() == 8192 && b.len() == 8192 && c.len() == 4096 {
                machine
                    .set_roms(k, b, c)
                    .map_err(|err| format!("ROMs invalid: {err}"))?;
                eprintln!("emu198x-commodore-vic-20 mcp: loaded all 3 ROMs");
            } else {
                eprintln!(
                    "emu198x-commodore-vic-20 mcp: ROM sizes wrong; starting blank"
                );
            }
        }

    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS_PAL,
        Vic20SessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-commodore-vic-20",
        env!("CARGO_PKG_VERSION"),
    ));
    register_common_tools(server.registry_mut());
    register_vic20_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn rom_path(kind: &str, default_file: &str) -> Option<PathBuf> {
    let env_key = format!("EMU198X_VIC20_{kind}");
    if let Ok(p) = env::var(&env_key)
        && !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    let home = env::var("HOME").ok()?;
    let default =
        PathBuf::from(home).join(format!(".emu198x/roms/commodore-vic-20/{default_file}"));
    if default.exists() { Some(default) } else { None }
}
