//! Acorn Atom-specific MCP tools.
//!
//! The CPU / memory / stepping surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set (6502 `disasm`
//! is pending the Asm198x crate; `io_trace` is unsupported on this
//! memory-mapped CPU). No Acorn Atom-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_acorn_atom::{AtomRuntime, AtomSessionQueryProvider};

type AtomSession = HeadlessSession<AtomRuntime, AtomSessionQueryProvider>;

/// Register the Acorn Atom MCP tool surface: the shared debug tools.
pub fn register_atom_tools(registry: &mut ToolRegistry<AtomSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
