//! Sinclair ZX80-specific MCP tools.
//!
//! The CPU / memory / disasm / stepping / I/O-trace surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set; no
//! Sinclair ZX80-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_sinclair_zx80::{Zx80Runtime, Zx80SessionQueryProvider};

type Zx80Session = HeadlessSession<Zx80Runtime, Zx80SessionQueryProvider>;

/// Register the Sinclair ZX80 MCP tool surface: the shared debug tools.
pub fn register_zx80_tools(registry: &mut ToolRegistry<Zx80Session>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
