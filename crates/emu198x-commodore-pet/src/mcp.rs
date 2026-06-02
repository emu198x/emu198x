//! MCP server mode — `--mcp` / `--mcp-stdio`.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::register_common_tools,
};
use runtime_commodore_pet::{Model, PetRuntime, PetSessionQueryProvider};

use crate::mcp_tools::register_pet_tools;

/// PET 40-col PAL: ~20000 cycles per 50 Hz frame at 1 MHz.
const FRAME_TICKS: u64 = 20_000;

/// Runs MCP mode. Loads ROMs from `EMU198X_PET_{KERNAL,BASIC,EDITOR,CHAR}`
/// or default paths in ~/.emu198x/roms/commodore-pet/.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let mut machine = PetRuntime::blank(Model::Pet40Col);
    let kernal = rom_path("KERNAL", "kernal.rom");
    let basic = rom_path("BASIC", "basic.rom");
    let editor = rom_path("EDITOR", "editor.rom");
    let char_rom = rom_path("CHAR", "chargen.rom");
    if let (Some(kp), Some(bp), Some(ep), Some(cp)) =
        (kernal.as_ref(), basic.as_ref(), editor.as_ref(), char_rom.as_ref())
        && let (Ok(k), Ok(b), Ok(e), Ok(c)) = (fs::read(kp), fs::read(bp), fs::read(ep), fs::read(cp))
        {
            if k.len() == 4096 && b.len() == 8192 && e.len() == 2048 && c.len() == 4096 {
                machine
                    .set_roms(k, b, e, c)
                    .map_err(|err| format!("ROMs invalid: {err}"))?;
                eprintln!("emu198x-commodore-pet mcp: loaded all 4 ROMs");
            } else {
                eprintln!(
                    "emu198x-commodore-pet mcp: ROM sizes wrong; starting blank"
                );
            }
        }

    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS,
        PetSessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-commodore-pet",
        env!("CARGO_PKG_VERSION"),
    ));
    register_common_tools(server.registry_mut());
    register_pet_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn rom_path(kind: &str, default_file: &str) -> Option<PathBuf> {
    let env_key = format!("EMU198X_PET_{kind}");
    if let Ok(p) = env::var(&env_key)
        && !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    let home = env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(format!(".emu198x/roms/commodore-pet/{default_file}"));
    if default.exists() { Some(default) } else { None }
}
