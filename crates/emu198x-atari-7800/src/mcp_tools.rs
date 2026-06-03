//! Atari 7800-specific MCP tools.
//!
//! The CPU / memory / stepping surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set (6502 `disasm`
//! pending the Asm198x crate; `io_trace` unsupported on this
//! memory-mapped CPU). No Atari 7800-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_atari_7800::{Atari7800Runtime, Atari7800SessionQueryProvider};

type A7800Session = HeadlessSession<Atari7800Runtime, Atari7800SessionQueryProvider>;

/// Register the Atari 7800 MCP tool surface: the shared debug tools.
pub fn register_a7800_tools(registry: &mut ToolRegistry<A7800Session>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
