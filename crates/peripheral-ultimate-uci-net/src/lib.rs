//! The network target of the Ultimate Command Interface.
//!
//! The UCI carries several targets — DOS, file and control among them. Only
//! the network one is modelled here, which is what this crate's name says: an
//! `ultimate-uci` would promise the rest of the interface as well.
//!
//! Software drives this device through four registers instead of timing a
//! serial line: write the command bytes, push them, read the response, accept
//! it. The device owns the socket, so the machine holds no bit periods, has no
//! framing to resynchronise and needs no interrupt serviced. A client that can
//! reach this hardware should prefer it; a bit-banged user port is the
//! fallback, not the other way round.
//!
//! Nothing here knows which machine it is plugged into. The host decodes the
//! addresses — `$DF1C`-`$DF1F` on a C64 — and calls [`UltimateUciNet::poll`] to
//! let the transport breathe.

use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;

/// Control-register write bits.
const CTRL_PUSH_CMD: u8 = 0x01;
const CTRL_DATA_ACCEPT: u8 = 0x02;
const CTRL_ABORT: u8 = 0x04;
const CTRL_CLEAR_ERROR: u8 = 0x08;

/// Control-register read bits.
const STAT_DATA_AV: u8 = 0x80;
const STAT_STATUS_AV: u8 = 0x40;
const STATE_DONE: u8 = 0x20;

/// Command targets.
const TARGET_NET: u8 = 0x03;

/// Network commands.
const CMD_OPEN_TCP: u8 = 0x07;
const CMD_CLOSE: u8 = 0x09;
const CMD_READ: u8 = 0x10;
const CMD_WRITE: u8 = 0x11;

/// Where the interface is in the command cycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    /// Ready to take command bytes.
    #[default]
    Idle,
    /// The command finished; response and status are waiting to be read.
    ///
    /// There is deliberately no busy state. Real hardware is busy for a while
    /// and software must poll until it clears, so completing inside the push
    /// write is a state software already copes with — it simply never sees the
    /// wait. Modelling a delay would only be inventing a duration.
    Done,
}

/// The network target of the Ultimate Command Interface.
#[derive(Debug, Default)]
pub struct UltimateUciNet {
    state: State,
    command: Vec<u8>,
    response: VecDeque<u8>,
    status: VecDeque<u8>,
    socket: Option<TcpStream>,
    /// Bytes pulled off the socket but not yet handed to the machine.
    incoming: VecDeque<u8>,
    handle: u8,
    last_error: Option<String>,
}

impl UltimateUciNet {
    /// Register offsets from the base address the host decodes.
    pub const REG_CONTROL: u8 = 0;
    pub const REG_COMMAND: u8 = 1;
    pub const REG_RESPONSE: u8 = 2;
    pub const REG_STATUS: u8 = 3;

    /// The identification byte a detect routine reads back from the command
    /// register. Its absence is how software decides no Ultimate is fitted.
    pub const IDENTIFICATION: u8 = 0xC9;

    #[must_use]
    pub fn new() -> Self {
        Self {
            handle: 1,
            ..Self::default()
        }
    }

    /// Whether a socket is currently open.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.socket.is_some()
    }

    /// The last transport failure, for host-side diagnostics.
    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Read one register.
    pub fn read(&mut self, register: u8) -> u8 {
        match register & 0x03 {
            Self::REG_CONTROL => self.control_status(),
            Self::REG_COMMAND => Self::IDENTIFICATION,
            Self::REG_RESPONSE => self.response.pop_front().unwrap_or(0),
            _ => self.status.pop_front().unwrap_or(0),
        }
    }

    /// Write one register.
    pub fn write(&mut self, register: u8, value: u8) {
        match register & 0x03 {
            Self::REG_CONTROL => self.control_write(value),
            // Command bytes accumulate until they are pushed. The bound keeps a
            // runaway writer from growing this without end.
            Self::REG_COMMAND if self.command.len() < 1024 => self.command.push(value),
            _ => {}
        }
    }

    fn control_status(&self) -> u8 {
        let mut status = match self.state {
            State::Idle => 0,
            State::Done => STATE_DONE,
        };
        if !self.response.is_empty() {
            status |= STAT_DATA_AV;
        }
        if !self.status.is_empty() {
            status |= STAT_STATUS_AV;
        }
        status
    }

    fn control_write(&mut self, value: u8) {
        if value & CTRL_ABORT != 0 {
            self.reset_cycle();
            return;
        }
        if value & CTRL_CLEAR_ERROR != 0 {
            self.last_error = None;
        }
        if value & CTRL_DATA_ACCEPT != 0 {
            self.reset_cycle();
        }
        if value & CTRL_PUSH_CMD != 0 {
            self.execute();
        }
    }

    fn reset_cycle(&mut self) {
        self.state = State::Idle;
        self.command.clear();
        self.response.clear();
        self.status.clear();
    }

    /// Status text is ASCII and a success leads with `"00"`, which is what
    /// software checks before trusting the response bytes.
    fn finish(&mut self, ok: bool) {
        let text: &[u8] = if ok { b"00,OK" } else { b"99,ERROR" };
        self.status.extend(text.iter().copied());
        self.state = State::Done;
    }

    fn execute(&mut self) {
        let command = std::mem::take(&mut self.command);
        self.response.clear();
        self.status.clear();

        // Only the network target is modelled; anything else reports an error
        // rather than pretending to have acted.
        let Some((&target, rest)) = command.split_first() else {
            self.finish(false);
            return;
        };
        if target != TARGET_NET {
            self.finish(false);
            return;
        }
        let Some((&opcode, args)) = rest.split_first() else {
            self.finish(false);
            return;
        };

        let ok = match opcode {
            CMD_OPEN_TCP => self.open_tcp(args),
            CMD_CLOSE => self.close_socket(),
            CMD_READ => self.read_socket(args),
            CMD_WRITE => self.write_socket(args),
            _ => false,
        };
        self.finish(ok);
    }

    fn open_tcp(&mut self, args: &[u8]) -> bool {
        // port (little endian), then a null-terminated hostname.
        if args.len() < 3 {
            return false;
        }
        let port = u16::from(args[0]) | (u16::from(args[1]) << 8);
        let host_bytes: Vec<u8> = args[2..].iter().copied().take_while(|&b| b != 0).collect();
        let Ok(host) = String::from_utf8(host_bytes) else {
            return false;
        };
        if host.is_empty() {
            return false;
        }

        match TcpStream::connect((host.as_str(), port)) {
            Ok(stream) => {
                if let Err(error) = stream.set_nonblocking(true) {
                    self.last_error = Some(error.to_string());
                    return false;
                }
                self.socket = Some(stream);
                self.incoming.clear();
                self.last_error = None;
                self.response.push_back(self.handle);
                true
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
                false
            }
        }
    }

    fn close_socket(&mut self) -> bool {
        self.socket = None;
        self.incoming.clear();
        true
    }

    fn read_socket(&mut self, args: &[u8]) -> bool {
        // socket handle, then the requested count (little endian).
        if args.len() < 3 {
            return false;
        }
        let wanted = usize::from(args[1]) | (usize::from(args[2]) << 8);
        self.service();

        let available = self.incoming.len().min(wanted);
        // The length leads the response so software knows how much followed,
        // and a length of zero is the ordinary "nothing yet" answer rather
        // than an error — this is how a client polls without blocking.
        let length = u16::try_from(available).unwrap_or(u16::MAX);
        self.response.push_back((length & 0xFF) as u8);
        self.response.push_back((length >> 8) as u8);
        for _ in 0..available {
            if let Some(byte) = self.incoming.pop_front() {
                self.response.push_back(byte);
            }
        }
        true
    }

    fn write_socket(&mut self, args: &[u8]) -> bool {
        let Some((_handle, payload)) = args.split_first() else {
            return false;
        };
        let Some(stream) = self.socket.as_mut() else {
            self.last_error = Some("write without an open socket".to_owned());
            return false;
        };
        match stream.write_all(payload) {
            Ok(()) => {
                let accepted = u16::try_from(payload.len()).unwrap_or(u16::MAX);
                self.response.push_back((accepted & 0xFF) as u8);
                self.response.push_back((accepted >> 8) as u8);
                true
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
                self.socket = None;
                false
            }
        }
    }

    /// Give the transport a chance to move bytes. Safe to call every cycle.
    pub fn poll(&mut self) {
        self.service();
    }

    fn service(&mut self) {
        let Some(stream) = self.socket.as_mut() else {
            return;
        };
        let mut buffer = [0u8; 512];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => {
                    // The peer closed the connection.
                    self.socket = None;
                    return;
                }
                Ok(count) => self.incoming.extend(&buffer[..count]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => return,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => {
                    self.last_error = Some(error.to_string());
                    self.socket = None;
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn push(uci: &mut UltimateUciNet, bytes: &[u8]) {
        for &byte in bytes {
            uci.write(UltimateUciNet::REG_COMMAND, byte);
        }
        uci.write(UltimateUciNet::REG_CONTROL, CTRL_PUSH_CMD);
    }

    fn drain_status(uci: &mut UltimateUciNet) -> Vec<u8> {
        let mut out = Vec::new();
        while uci.read(UltimateUciNet::REG_CONTROL) & STAT_STATUS_AV != 0 {
            out.push(uci.read(UltimateUciNet::REG_STATUS));
        }
        out
    }

    #[test]
    fn identification_is_what_detection_looks_for() {
        let mut uci = UltimateUciNet::new();
        assert_eq!(uci.read(UltimateUciNet::REG_COMMAND), 0xC9);
    }

    #[test]
    fn a_fresh_interface_reports_idle() {
        let mut uci = UltimateUciNet::new();
        // Software waits for the state bits to clear before writing a command.
        assert_eq!(uci.read(UltimateUciNet::REG_CONTROL) & 0x30, 0);
    }

    #[test]
    fn open_write_read_and_close_round_trip_over_a_real_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();

        let mut uci = UltimateUciNet::new();
        let mut command = vec![
            TARGET_NET,
            CMD_OPEN_TCP,
            (port & 0xFF) as u8,
            (port >> 8) as u8,
        ];
        command.extend_from_slice(b"127.0.0.1\0");
        push(&mut uci, &command);

        assert!(uci.read(UltimateUciNet::REG_CONTROL) & STAT_DATA_AV != 0);
        let handle = uci.read(UltimateUciNet::REG_RESPONSE);
        assert_eq!(drain_status(&mut uci)[..2], *b"00");
        uci.write(UltimateUciNet::REG_CONTROL, CTRL_DATA_ACCEPT);
        assert_eq!(
            uci.read(UltimateUciNet::REG_CONTROL) & 0x30,
            0,
            "back to idle"
        );

        let (mut peer, _) = listener.accept().expect("accept");

        // WRITE reports how many bytes it took.
        let mut command = vec![TARGET_NET, CMD_WRITE, handle];
        command.extend_from_slice(b"RACH");
        push(&mut uci, &command);
        let accepted = u16::from(uci.read(UltimateUciNet::REG_RESPONSE))
            | (u16::from(uci.read(UltimateUciNet::REG_RESPONSE)) << 8);
        assert_eq!(accepted, 4);
        uci.write(UltimateUciNet::REG_CONTROL, CTRL_DATA_ACCEPT);

        let mut seen = [0u8; 4];
        std::io::Read::read_exact(&mut peer, &mut seen).expect("read");
        assert_eq!(&seen, b"RACH");

        // READ leads with a little-endian length, so a client can ask for a
        // frame's worth and be told how much of it has actually arrived.
        peer.write_all(b"OK").expect("write");
        std::thread::sleep(std::time::Duration::from_millis(50));
        push(&mut uci, &[TARGET_NET, CMD_READ, handle, 64, 0]);
        let length = u16::from(uci.read(UltimateUciNet::REG_RESPONSE))
            | (u16::from(uci.read(UltimateUciNet::REG_RESPONSE)) << 8);
        assert_eq!(length, 2);
        assert_eq!(uci.read(UltimateUciNet::REG_RESPONSE), b'O');
        assert_eq!(uci.read(UltimateUciNet::REG_RESPONSE), b'K');
        uci.write(UltimateUciNet::REG_CONTROL, CTRL_DATA_ACCEPT);

        push(&mut uci, &[TARGET_NET, CMD_CLOSE, handle]);
        assert_eq!(drain_status(&mut uci)[..2], *b"00");
        assert!(!uci.is_connected());
    }

    #[test]
    fn reading_an_idle_socket_reports_zero_rather_than_failing() {
        // This is the ordinary polling case: no data yet is not an error, and
        // a client leans on it to stay non-blocking.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let mut uci = UltimateUciNet::new();
        let mut command = vec![
            TARGET_NET,
            CMD_OPEN_TCP,
            (port & 0xFF) as u8,
            (port >> 8) as u8,
        ];
        command.extend_from_slice(b"127.0.0.1\0");
        push(&mut uci, &command);
        let handle = uci.read(UltimateUciNet::REG_RESPONSE);
        uci.write(UltimateUciNet::REG_CONTROL, CTRL_DATA_ACCEPT);
        let _peer = listener.accept().expect("accept");

        push(&mut uci, &[TARGET_NET, CMD_READ, handle, 64, 0]);
        assert_eq!(uci.read(UltimateUciNet::REG_RESPONSE), 0);
        assert_eq!(uci.read(UltimateUciNet::REG_RESPONSE), 0);
        assert_eq!(
            drain_status(&mut uci)[..2],
            *b"00",
            "empty is still success"
        );
    }

    #[test]
    fn an_unknown_target_is_refused_rather_than_silently_accepted() {
        let mut uci = UltimateUciNet::new();
        push(&mut uci, &[0x01, 0x02]);
        assert_eq!(drain_status(&mut uci)[..2], *b"99");
    }
}
