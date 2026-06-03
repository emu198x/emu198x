//! Mattel Aquarius-specific MCP tools.
//!
//! The CPU / memory / disasm / stepping / I/O-trace surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set; no
//! Mattel Aquarius-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_mattel_aquarius::{AquariusRuntime, AquariusSessionQueryProvider};

type AquariusSession = HeadlessSession<AquariusRuntime, AquariusSessionQueryProvider>;

/// Register the Mattel Aquarius MCP tool surface: the shared debug tools.
pub fn register_aquarius_tools(registry: &mut ToolRegistry<AquariusSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
