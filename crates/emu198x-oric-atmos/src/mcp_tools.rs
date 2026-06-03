//! Oric Atmos-specific MCP tools.
//!
//! The CPU / memory / stepping surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set (6502 `disasm`
//! is pending the Asm198x crate; `io_trace` is unsupported on this
//! memory-mapped CPU). No Oric Atmos-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_oric_atmos::{OricRuntime, OricSessionQueryProvider};

type OricSession = HeadlessSession<OricRuntime, OricSessionQueryProvider>;

/// Register the Oric Atmos MCP tool surface: the shared debug tools.
pub fn register_oric_tools(registry: &mut ToolRegistry<OricSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
