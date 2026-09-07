//! CPU I/O-port access for machines whose processor has a separate port
//! space (the Z80's `IN`/`OUT`).
//!
//! `port_read` / `port_write` script steps and MCP tools run through this
//! trait, so any machine that exposes it gets both surfaces at once. A
//! read goes through the live bus and may have side effects, exactly as the
//! CPU's own `IN` would; that is the point of the verb.

/// Port-space access on the live machine.
pub trait PortIoTarget {
    /// Read `port` through the bus, with whatever side effects the hardware
    /// has (a keyboard row latch, a tape edge, a ULA state change).
    fn port_read(&mut self, port: u16) -> u8;

    /// Write `value` to `port` through the bus.
    fn port_write(&mut self, port: u16, value: u8);
}
