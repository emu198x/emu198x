//! The network target of the Ultimate Command Interface.
//!
//! The UCI carries three targets: network (3), control (4) and SoftIEC (5).
//! Only the network one is modelled here, which is what this crate's name
//! says: an `ultimate-uci` would promise the rest of the interface as well.
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
use std::net::{TcpStream, ToSocketAddrs, UdpSocket};

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

/// Network commands. `SET_INTERFACE` (0x03) is absent on purpose: the firmware
/// has it commented out, so real hardware answers it as an unknown command and
/// so does this.
const CMD_IDENTIFY: u8 = 0x01;
const CMD_GET_INTERFACE_COUNT: u8 = 0x02;
const CMD_GET_NETADDR: u8 = 0x04;
const CMD_GET_IPADDR: u8 = 0x05;
const CMD_SET_IPADDR: u8 = 0x06;
const CMD_OPEN_TCP: u8 = 0x07;
const CMD_OPEN_UDP: u8 = 0x08;
const CMD_CLOSE: u8 = 0x09;
const CMD_READ: u8 = 0x10;
const CMD_WRITE: u8 = 0x11;

/// What IDENTIFY answers with. Software reads this to confirm which target it
/// is talking to before trusting anything else.
const IDENTIFICATION_TEXT: &str = "ULTIMATE-II NETWORK INTERFACE V1.0";

/// What a target that is not fitted answers IDENTIFY with.
const NO_TARGET_TEXT: &str = "NO TARGET";

/// One synthetic interface. There is no NIC behind it — the host OS does the
/// networking — so the addresses below are plausible defaults that software can
/// read back and overwrite through SET_IPADDR.
const INTERFACE_COUNT: u8 = 1;

/// Locally administered, so it cannot collide with a real vendor's range.
const DEFAULT_MAC: [u8; 6] = [0x02, 0x00, 0x19, 0x86, 0x00, 0x01];

/// Address, netmask then gateway, four bytes each, in the order the firmware
/// memcpys them out of lwIP.
const DEFAULT_ADDRESS: [u8; 12] = [192, 168, 1, 64, 255, 255, 255, 0, 192, 168, 1, 1];

/// Status lines, matching the firmware's `network_target.cc` byte for byte.
///
/// The interface answers in ASCII: two digits, a comma, then prose. Software
/// tests the digits and nothing else, because the firmware appends an errno to
/// some of these and that number is meaningless off the real hardware.
const STATUS_OK: &str = "00,OK";
const STATUS_CONNECTION_CLOSED: &str = "01,CONNECTION CLOSED BY HOST";
const STATUS_NO_DATA: &str = "02,NO DATA";
const STATUS_UNKNOWN_COMMAND: &str = "21,UNKNOWN COMMAND";
const STATUS_INVALID_PARAMS: &str = "81,INVALID PARAMS";
const STATUS_OUT_OF_RANGE: &str = "82,PARAMETER(S) OUT OF RANGE";
const STATUS_UNRESOLVED_HOST: &str = "84,UNRESOLVED HOST";
const STATUS_NO_SOCKET: &str = "85,ERROR OPENING SOCKET";

/// The most a single READ may ask for: the reply buffer less the two length
/// bytes that lead it.
const MAX_READ_LEN: usize = 894;

/// Build one of the status lines that carries an error number. The firmware
/// prints lwIP's errno here; a host OS numbers things differently, so this is
/// diagnostic text rather than anything software may match on.
fn numbered_status(prefix: &str, error: &std::io::Error) -> String {
    match error.raw_os_error() {
        Some(code) => format!("{prefix}: {code}"),
        None => prefix.to_owned(),
    }
}

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

/// The open socket, whichever kind it is. A UDP socket is connected as well,
/// so READ and WRITE treat the two identically.
#[derive(Debug)]
enum Socket {
    Tcp(TcpStream),
    Udp(UdpSocket),
}

impl Socket {
    fn receive(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.read(buffer),
            Self::Udp(socket) => socket.recv(buffer),
        }
    }

    fn transmit(&mut self, payload: &[u8]) -> std::io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.write_all(payload),
            // A datagram goes whole or not at all, so a short send is a failure
            // rather than a tail to send separately.
            Self::Udp(socket) => socket.send(payload).and_then(|sent| {
                if sent == payload.len() {
                    Ok(())
                } else {
                    Err(std::io::Error::new(ErrorKind::WriteZero, "short datagram"))
                }
            }),
        }
    }

    /// Only a stream can be closed by the peer. A datagram socket with nothing
    /// to hand over has simply had nothing sent to it.
    fn is_stream(&self) -> bool {
        matches!(self, Self::Tcp(_))
    }
}

/// The network target of the Ultimate Command Interface.
#[derive(Debug)]
pub struct UltimateUciNet {
    state: State,
    command: Vec<u8>,
    response: VecDeque<u8>,
    status: VecDeque<u8>,
    socket: Option<Socket>,
    /// Bytes pulled off the socket but not yet handed to the machine.
    incoming: VecDeque<u8>,
    /// The peer hung up and the buffered bytes have yet to be collected.
    peer_closed: bool,
    handle: u8,
    last_error: Option<String>,
    /// Address, netmask and gateway of the synthetic interface.
    address: [u8; 12],
    mac: [u8; 6],
}

impl Default for UltimateUciNet {
    fn default() -> Self {
        Self::new()
    }
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
            state: State::Idle,
            command: Vec::new(),
            response: VecDeque::new(),
            status: VecDeque::new(),
            socket: None,
            incoming: VecDeque::new(),
            peer_closed: false,
            handle: 1,
            last_error: None,
            address: DEFAULT_ADDRESS,
            mac: DEFAULT_MAC,
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
            Self::REG_COMMAND if self.command.len() < 896 => self.command.push(value),
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

    /// Queue the status line and mark the command complete.
    fn finish(&mut self, status: &str) {
        self.status.extend(status.bytes());
        self.state = State::Done;
    }

    /// Push a 16-bit little-endian count, which is how both READ and WRITE
    /// lead their replies.
    fn push_count(&mut self, count: u16) {
        self.response.push_back((count & 0xFF) as u8);
        self.response.push_back((count >> 8) as u8);
    }

    fn execute(&mut self) {
        let command = std::mem::take(&mut self.command);
        self.response.clear();
        self.status.clear();

        // Only the network target is modelled. An unregistered target answers
        // "21,UNKNOWN COMMAND" on real hardware, so that is what anything else
        // gets here rather than a pretence of having acted.
        let Some((&target, rest)) = command.split_first() else {
            self.finish(STATUS_UNKNOWN_COMMAND);
            return;
        };
        if target != TARGET_NET {
            // A target that is not fitted still answers IDENTIFY, which is how
            // software enumerates what a machine has.
            if rest.first() == Some(&CMD_IDENTIFY) {
                self.response.extend(NO_TARGET_TEXT.bytes());
                self.finish(STATUS_OK);
            } else {
                self.finish(STATUS_UNKNOWN_COMMAND);
            }
            return;
        }
        let Some((&opcode, args)) = rest.split_first() else {
            self.finish(STATUS_UNKNOWN_COMMAND);
            return;
        };

        let outcome = match opcode {
            CMD_IDENTIFY => {
                self.response.extend(IDENTIFICATION_TEXT.bytes());
                Ok(())
            }
            CMD_GET_INTERFACE_COUNT => {
                self.response.push_back(INTERFACE_COUNT);
                Ok(())
            }
            CMD_GET_NETADDR => self.get_netaddr(args),
            CMD_GET_IPADDR => self.get_ipaddr(args),
            CMD_SET_IPADDR => self.set_ipaddr(args),
            CMD_OPEN_TCP => self.open_socket(args, false),
            CMD_OPEN_UDP => self.open_socket(args, true),
            CMD_CLOSE => self.close_socket(args),
            CMD_READ => self.read_socket(args),
            CMD_WRITE => self.write_socket(args),
            _ => Err(STATUS_UNKNOWN_COMMAND.to_owned()),
        };
        match outcome {
            Ok(()) => self.finish(STATUS_OK),
            Err(status) => self.finish(&status),
        }
    }

    /// Check the interface argument the address commands all share. The
    /// firmware tests the command length first and the index second, and
    /// answers each with its own code.
    fn interface(args: &[u8]) -> Result<(), String> {
        let [index] = args else {
            return Err(STATUS_INVALID_PARAMS.to_owned());
        };
        if *index >= INTERFACE_COUNT {
            return Err(STATUS_OUT_OF_RANGE.to_owned());
        }
        Ok(())
    }

    fn get_netaddr(&mut self, args: &[u8]) -> Result<(), String> {
        Self::interface(args)?;
        self.response.extend(self.mac);
        Ok(())
    }

    fn get_ipaddr(&mut self, args: &[u8]) -> Result<(), String> {
        Self::interface(args)?;
        self.response.extend(self.address);
        Ok(())
    }

    fn set_ipaddr(&mut self, args: &[u8]) -> Result<(), String> {
        // interface index, then address, netmask and gateway.
        let Some((index, address)) = args.split_first() else {
            return Err(STATUS_INVALID_PARAMS.to_owned());
        };
        if address.len() != 12 {
            return Err(STATUS_INVALID_PARAMS.to_owned());
        }
        if *index >= INTERFACE_COUNT {
            return Err(STATUS_OUT_OF_RANGE.to_owned());
        }
        self.address.copy_from_slice(address);
        Ok(())
    }

    fn open_socket(&mut self, args: &[u8], datagram: bool) -> Result<(), String> {
        // port (little endian), then a null-terminated hostname.
        if args.len() < 3 {
            return Err(STATUS_INVALID_PARAMS.to_owned());
        }
        let port = u16::from(args[0]) | (u16::from(args[1]) << 8);
        let host_bytes: Vec<u8> = args[2..].iter().copied().take_while(|&b| b != 0).collect();
        let Ok(host) = String::from_utf8(host_bytes) else {
            return Err(STATUS_UNRESOLVED_HOST.to_owned());
        };
        if host.is_empty() {
            return Err(STATUS_UNRESOLVED_HOST.to_owned());
        }

        // Resolving before connecting is what separates "84,UNRESOLVED HOST"
        // from "11,ERROR ON CONNECT" — the firmware calls gethostbyname_r and
        // connect as two steps for exactly this reason, and software leans on
        // the distinction to tell a typo from a server that is down.
        let Ok(mut addresses) = (host.as_str(), port).to_socket_addrs() else {
            return Err(STATUS_UNRESOLVED_HOST.to_owned());
        };
        let Some(address) = addresses.next() else {
            return Err(STATUS_UNRESOLVED_HOST.to_owned());
        };

        let opened = if datagram {
            UdpSocket::bind("0.0.0.0:0")
                .and_then(|socket| socket.connect(address).map(|()| Socket::Udp(socket)))
        } else {
            TcpStream::connect(address).map(Socket::Tcp)
        };

        match opened {
            Ok(socket) => {
                let nonblocking = match &socket {
                    Socket::Tcp(stream) => stream.set_nonblocking(true),
                    Socket::Udp(datagrams) => datagrams.set_nonblocking(true),
                };
                if let Err(error) = nonblocking {
                    self.last_error = Some(error.to_string());
                    return Err(STATUS_NO_SOCKET.to_owned());
                }
                self.socket = Some(socket);
                self.incoming.clear();
                self.peer_closed = false;
                self.last_error = None;
                self.response.push_back(self.handle);
                Ok(())
            }
            Err(error) => {
                let status = numbered_status("11,ERROR ON CONNECT", &error);
                self.last_error = Some(error.to_string());
                Err(status)
            }
        }
    }

    fn close_socket(&mut self, args: &[u8]) -> Result<(), String> {
        // Just the socket handle; the firmware checks the length exactly.
        if args.len() != 1 {
            return Err(STATUS_INVALID_PARAMS.to_owned());
        }
        self.socket = None;
        self.incoming.clear();
        self.peer_closed = false;
        Ok(())
    }

    fn read_socket(&mut self, args: &[u8]) -> Result<(), String> {
        // socket handle, then the requested count (little endian).
        if args.len() != 3 {
            return Err(STATUS_INVALID_PARAMS.to_owned());
        }
        let wanted = usize::from(args[1]) | (usize::from(args[2]) << 8);
        if wanted > MAX_READ_LEN {
            return Err(STATUS_OUT_OF_RANGE.to_owned());
        }
        self.service();

        let available = self.incoming.len().min(wanted);
        // The reply always leads with recv()'s return value as a signed 16-bit
        // little-endian count, and the status says how to read it. There is no
        // zero-length success: a poll that found nothing answers -1 with
        // "02,NO DATA", and a peer that hung up answers 0 with
        // "01,CONNECTION CLOSED BY HOST". Software that accepts only "00,OK"
        // would therefore call every idle poll a failure.
        if available == 0 {
            if self.peer_closed {
                // The firmware closes the socket here, so the next poll finds a
                // dead handle and reports no data. Closed is announced once.
                self.push_count(0);
                self.socket = None;
                self.peer_closed = false;
                return Err(STATUS_CONNECTION_CLOSED.to_owned());
            }
            self.push_count(0xFFFF);
            return Err(STATUS_NO_DATA.to_owned());
        }

        self.push_count(u16::try_from(available).unwrap_or(u16::MAX));
        for _ in 0..available {
            if let Some(byte) = self.incoming.pop_front() {
                self.response.push_back(byte);
            }
        }
        Ok(())
    }

    fn write_socket(&mut self, args: &[u8]) -> Result<(), String> {
        let Some((_handle, payload)) = args.split_first() else {
            return Err(STATUS_INVALID_PARAMS.to_owned());
        };
        let Some(socket) = self.socket.as_mut() else {
            self.last_error = Some("write without an open socket".to_owned());
            self.push_count(0xFFFF);
            return Err(STATUS_CONNECTION_CLOSED.to_owned());
        };
        let result = socket.transmit(payload);
        match result {
            Ok(()) => {
                let accepted = u16::try_from(payload.len()).unwrap_or(u16::MAX);
                self.push_count(accepted);
                Ok(())
            }
            Err(error) => {
                let status = numbered_status("12,SEND ERROR", &error);
                self.last_error = Some(error.to_string());
                self.socket = None;
                self.push_count(0xFFFF);
                Err(status)
            }
        }
    }

    /// Give the transport a chance to move bytes. Safe to call every cycle.
    pub fn poll(&mut self) {
        self.service();
    }

    fn service(&mut self) {
        let Some(socket) = self.socket.as_mut() else {
            return;
        };
        let stream = socket.is_stream();
        let mut buffer = [0u8; 512];
        let mut received: Vec<u8> = Vec::new();
        let mut closed = false;
        let mut failure = None;
        loop {
            match socket.receive(&mut buffer) {
                // A stream that reads zero has been closed by the peer; a
                // datagram socket has merely had an empty datagram sent to it.
                Ok(0) => {
                    closed = stream;
                    break;
                }
                Ok(count) => received.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => {
                    failure = Some(error.to_string());
                    closed = true;
                    break;
                }
            }
        }
        // Bytes already in hand are owed to the machine even when the peer has
        // gone, so they are kept and the close is reported once they run out.
        self.incoming.extend(received);
        if let Some(error) = failure {
            self.last_error = Some(error);
        }
        if closed {
            self.peer_closed = true;
            self.socket = None;
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
    fn an_idle_poll_answers_no_data_rather_than_a_zero_length_success() {
        // The ordinary polling case. Real hardware never answers "00,OK" with
        // nothing behind it, so a client that treats a non-"00" status as a
        // transport fault would break on every quiet poll.
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
        // recv() returned -1, reported as a signed 16-bit count.
        assert_eq!(uci.read(UltimateUciNet::REG_RESPONSE), 0xFF);
        assert_eq!(uci.read(UltimateUciNet::REG_RESPONSE), 0xFF);
        assert_eq!(drain_status(&mut uci), b"02,NO DATA");
    }

    #[test]
    fn a_peer_hanging_up_is_announced_before_the_socket_goes_away() {
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

        let (peer, _) = listener.accept().expect("accept");
        drop(peer);
        std::thread::sleep(std::time::Duration::from_millis(50));

        push(&mut uci, &[TARGET_NET, CMD_READ, handle, 64, 0]);
        assert_eq!(uci.read(UltimateUciNet::REG_RESPONSE), 0);
        assert_eq!(uci.read(UltimateUciNet::REG_RESPONSE), 0);
        assert_eq!(drain_status(&mut uci), b"01,CONNECTION CLOSED BY HOST");
        uci.write(UltimateUciNet::REG_CONTROL, CTRL_DATA_ACCEPT);

        // Once announced the handle is dead, and the firmware's recv on a
        // closed socket reports no data rather than closing twice.
        push(&mut uci, &[TARGET_NET, CMD_READ, handle, 64, 0]);
        assert_eq!(drain_status(&mut uci), b"02,NO DATA");
    }

    #[test]
    fn buffered_bytes_are_delivered_before_the_close_is_announced() {
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

        let (mut peer, _) = listener.accept().expect("accept");
        peer.write_all(b"LAST").expect("write");
        drop(peer);
        std::thread::sleep(std::time::Duration::from_millis(50));

        push(&mut uci, &[TARGET_NET, CMD_READ, handle, 64, 0]);
        assert_eq!(uci.read(UltimateUciNet::REG_RESPONSE), 4);
        assert_eq!(uci.read(UltimateUciNet::REG_RESPONSE), 0);
        assert_eq!(drain_status(&mut uci), b"00,OK");
    }

    #[test]
    fn an_oversized_read_is_out_of_range() {
        let mut uci = UltimateUciNet::new();
        push(&mut uci, &[TARGET_NET, CMD_READ, 1, 0xFF, 0x03]);
        assert_eq!(drain_status(&mut uci), b"82,PARAMETER(S) OUT OF RANGE");
    }

    #[test]
    fn an_unknown_target_is_refused_rather_than_silently_accepted() {
        let mut uci = UltimateUciNet::new();
        push(&mut uci, &[0x01, 0x02]);
        assert_eq!(drain_status(&mut uci), b"21,UNKNOWN COMMAND");
    }

    fn drain_response(uci: &mut UltimateUciNet) -> Vec<u8> {
        let mut out = Vec::new();
        while uci.read(UltimateUciNet::REG_CONTROL) & STAT_DATA_AV != 0 {
            out.push(uci.read(UltimateUciNet::REG_RESPONSE));
        }
        out
    }

    #[test]
    fn identify_names_the_target() {
        let mut uci = UltimateUciNet::new();
        push(&mut uci, &[TARGET_NET, CMD_IDENTIFY]);
        assert_eq!(
            drain_response(&mut uci),
            b"ULTIMATE-II NETWORK INTERFACE V1.0"
        );
        assert_eq!(drain_status(&mut uci), b"00,OK");
    }

    #[test]
    fn a_target_that_is_not_fitted_still_answers_identify() {
        let mut uci = UltimateUciNet::new();
        // The control target is real hardware but is not modelled here.
        push(&mut uci, &[0x04, CMD_IDENTIFY]);
        assert_eq!(drain_response(&mut uci), b"NO TARGET");
        assert_eq!(drain_status(&mut uci), b"00,OK");
    }

    #[test]
    fn one_interface_is_reported_and_its_addresses_read_back() {
        let mut uci = UltimateUciNet::new();
        push(&mut uci, &[TARGET_NET, CMD_GET_INTERFACE_COUNT]);
        assert_eq!(drain_response(&mut uci), vec![1]);
        uci.write(UltimateUciNet::REG_CONTROL, CTRL_DATA_ACCEPT);

        push(&mut uci, &[TARGET_NET, CMD_GET_NETADDR, 0]);
        assert_eq!(drain_response(&mut uci), DEFAULT_MAC);
        uci.write(UltimateUciNet::REG_CONTROL, CTRL_DATA_ACCEPT);

        push(&mut uci, &[TARGET_NET, CMD_GET_IPADDR, 0]);
        assert_eq!(drain_response(&mut uci), DEFAULT_ADDRESS);
    }

    #[test]
    fn an_interface_that_does_not_exist_is_out_of_range() {
        let mut uci = UltimateUciNet::new();
        push(&mut uci, &[TARGET_NET, CMD_GET_IPADDR, 7]);
        assert_eq!(drain_status(&mut uci), b"82,PARAMETER(S) OUT OF RANGE");
        uci.write(UltimateUciNet::REG_CONTROL, CTRL_DATA_ACCEPT);

        // A missing index is a bad command rather than a bad value.
        push(&mut uci, &[TARGET_NET, CMD_GET_IPADDR]);
        assert_eq!(drain_status(&mut uci), b"81,INVALID PARAMS");
    }

    #[test]
    fn set_ipaddr_is_read_back_by_get_ipaddr() {
        let mut uci = UltimateUciNet::new();
        let wanted = [10, 0, 0, 64, 255, 0, 0, 0, 10, 0, 0, 1];
        let mut command = vec![TARGET_NET, CMD_SET_IPADDR, 0];
        command.extend_from_slice(&wanted);
        push(&mut uci, &command);
        assert_eq!(drain_status(&mut uci), b"00,OK");
        uci.write(UltimateUciNet::REG_CONTROL, CTRL_DATA_ACCEPT);

        push(&mut uci, &[TARGET_NET, CMD_GET_IPADDR, 0]);
        assert_eq!(drain_response(&mut uci), wanted);
    }

    #[test]
    fn set_interface_is_unknown_because_the_firmware_never_enabled_it() {
        let mut uci = UltimateUciNet::new();
        push(&mut uci, &[TARGET_NET, 0x03, 0]);
        assert_eq!(drain_status(&mut uci), b"21,UNKNOWN COMMAND");
    }

    #[test]
    fn a_udp_socket_carries_datagrams_both_ways() {
        let peer = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind");
        let port = peer.local_addr().expect("addr").port();

        let mut uci = UltimateUciNet::new();
        let mut command = vec![
            TARGET_NET,
            CMD_OPEN_UDP,
            (port & 0xFF) as u8,
            (port >> 8) as u8,
        ];
        command.extend_from_slice(b"127.0.0.1\0");
        push(&mut uci, &command);
        let handle = uci.read(UltimateUciNet::REG_RESPONSE);
        assert_eq!(drain_status(&mut uci), b"00,OK");
        uci.write(UltimateUciNet::REG_CONTROL, CTRL_DATA_ACCEPT);

        let mut command = vec![TARGET_NET, CMD_WRITE, handle];
        command.extend_from_slice(b"RACH");
        push(&mut uci, &command);
        assert_eq!(drain_status(&mut uci), b"00,OK");
        uci.write(UltimateUciNet::REG_CONTROL, CTRL_DATA_ACCEPT);

        let mut seen = [0u8; 4];
        let (_, from) = peer.recv_from(&mut seen).expect("recv");
        assert_eq!(&seen, b"RACH");

        peer.send_to(b"OK", from).expect("send");
        std::thread::sleep(std::time::Duration::from_millis(50));
        push(&mut uci, &[TARGET_NET, CMD_READ, handle, 64, 0]);
        assert_eq!(drain_response(&mut uci), vec![2, 0, b'O', b'K']);
        assert_eq!(drain_status(&mut uci), b"00,OK");
    }

    #[test]
    fn a_name_that_does_not_resolve_is_told_apart_from_a_refused_connection() {
        let mut uci = UltimateUciNet::new();
        let mut command = vec![TARGET_NET, CMD_OPEN_TCP, 0x10, 0x27];
        command.extend_from_slice(b"no-such-host.invalid\0");
        push(&mut uci, &command);
        assert_eq!(drain_status(&mut uci), b"84,UNRESOLVED HOST");
    }
}
