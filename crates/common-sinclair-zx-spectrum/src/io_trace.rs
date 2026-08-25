//! Host-side capture of Z80 `IN`/`OUT` traffic.
//!
//! The Spectrum decodes I/O on single address lines rather than on a
//! whole port number: bit 0 clear selects the ULA, so the keyboard,
//! border, speaker and tape all arrive at `$FE` and at every even
//! mirror of it. The 128K machines add paging at `$7FFD` and the AY at
//! `$FFFD`/`$BFFD`, which are distinguished by their *high* bytes.
//!
//! That is why [`IoEvent::port`] keeps all sixteen bits. A Z80 puts the
//! full address bus on an `IN`/`OUT`, and a trace truncated to the low
//! byte would report the AY register select and its data port as the
//! same port `$FD`.
//!
//! Tracing is off until something asks for it, and the buffer is
//! host-side only — it is deliberately outside the snapshot, because a
//! saved state should carry the machine, not whatever a debugger was
//! collecting at the time.

/// One `IN` or `OUT` observed on the Z80's I/O bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IoEvent {
    /// Program counter at the time of the access.
    pub pc: u16,
    /// The full sixteen-bit port address.
    pub port: u16,
    /// Byte written, or the byte returned on a read.
    pub value: u8,
    /// `true` for `OUT`, `false` for `IN`.
    pub write: bool,
}

/// Capture buffer: `None` until tracing starts.
#[derive(Default)]
pub struct IoTrace {
    events: Option<Vec<IoEvent>>,
}

impl IoTrace {
    /// Start, or restart, capturing.
    pub fn start(&mut self) {
        self.events = Some(Vec::new());
    }

    /// Stop capturing and take what was collected.
    pub fn take(&mut self) -> Vec<IoEvent> {
        self.events.take().unwrap_or_default()
    }

    /// Whether capturing is currently on.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.events.is_some()
    }

    /// Record one access. Does nothing while tracing is off, so the
    /// call can sit unconditionally in the bus handlers.
    pub fn record(&mut self, pc: u16, port: u16, value: u8, write: bool) {
        if let Some(events) = &mut self.events {
            events.push(IoEvent {
                pc,
                port,
                value,
                write,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_is_off_until_started() {
        let mut trace = IoTrace::default();
        assert!(!trace.is_active());
        trace.record(0x1234, 0x00FE, 0x07, true);
        assert!(trace.take().is_empty(), "nothing is kept before start");
    }

    #[test]
    fn taking_stops_the_capture() {
        let mut trace = IoTrace::default();
        trace.start();
        trace.record(0x1234, 0x00FE, 0x07, true);
        assert_eq!(trace.take().len(), 1);
        assert!(!trace.is_active(), "take stops tracing");
        trace.record(0x1234, 0x00FE, 0x07, true);
        assert!(trace.take().is_empty());
    }

    /// The whole point of a sixteen-bit port: on a 128K machine the AY
    /// register select and its data port share a low byte.
    #[test]
    fn the_full_address_bus_is_kept() {
        let mut trace = IoTrace::default();
        trace.start();
        trace.record(0x0000, 0xFFFD, 0x07, true);
        trace.record(0x0000, 0xBFFD, 0x3F, true);
        let events = trace.take();
        assert_eq!(events[0].port, 0xFFFD);
        assert_eq!(events[1].port, 0xBFFD);
        assert_ne!(
            events[0].port, events[1].port,
            "truncating to $FD would merge AY select with AY data"
        );
    }
}
