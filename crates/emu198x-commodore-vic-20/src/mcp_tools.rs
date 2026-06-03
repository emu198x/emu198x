//! VIC-20-specific MCP tools.
//!
//! The VIC-20's CPU / memory / stepping surface is entirely covered by the
//! shared [`emu198x_shell::mcp_tools::register_debug_tools`] set
//! (`query_cpu`, `memory_read`, `poke_byte`, `poke_word`, `run_until_pc`,
//! `step`; `disasm` reports cleanly until the Asm198x 6502 disassembler
//! lands; `io_trace` is unsupported on this memory-mapped CPU). No
//! VIC-20-specific tools are needed yet — chip queries land here when a
//! concrete debugging need surfaces.

use emu198x_shell::mcp::ToolRegistry;
use runtime_commodore_vic_20::{Vic20Runtime, Vic20SessionQueryProvider};

use emu198x_shell::HeadlessSession;

type Vic20Session = HeadlessSession<Vic20Runtime, Vic20SessionQueryProvider>;

/// Register the VIC-20 MCP tool surface: the shared debug tools.
pub fn register_vic20_tools(registry: &mut ToolRegistry<Vic20Session>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
