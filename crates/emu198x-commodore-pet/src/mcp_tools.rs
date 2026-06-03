//! Commodore PET-specific MCP tools.
//!
//! The CPU / memory / stepping surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set (6502 `disasm`
//! is pending the Asm198x crate; `io_trace` is unsupported on this
//! memory-mapped CPU). No Commodore PET-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_commodore_pet::{PetRuntime, PetSessionQueryProvider};

type PetSession = HeadlessSession<PetRuntime, PetSessionQueryProvider>;

/// Register the Commodore PET MCP tool surface: the shared debug tools.
pub fn register_pet_tools(registry: &mut ToolRegistry<PetSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
