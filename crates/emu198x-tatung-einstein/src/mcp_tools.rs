//! Tatung Einstein-specific MCP tools.
//!
//! The CPU / memory / disasm / stepping / I/O-trace surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set; no
//! Tatung Einstein-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_tatung_einstein::{EinsteinRuntime, EinsteinSessionQueryProvider};

type EinsteinSession = HeadlessSession<EinsteinRuntime, EinsteinSessionQueryProvider>;

/// Register the Tatung Einstein MCP tool surface: the shared debug tools.
pub fn register_einstein_tools(registry: &mut ToolRegistry<EinsteinSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
