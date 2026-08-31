//! Deterministic cycle-driven ESP-AT modem on a bit-banged 8N1 line.
//!
//! Nothing here knows which machine it is attached to. A runtime supplies the
//! transmit level each tick and reads the receive level back, so the wiring —
//! VIA CB2/PB0 on a VIC-20, CIA2 PA2/PB0 on a C64 — stays with the machine and
//! only the timing, protocol and transport live here.
//!
//! Extracted from `runtime-commodore-vic-20` when the C64 needed the same
//! capability; see RULES.md #30.

use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;

/// A byte-stream boundary backed by physical 8N1 levels.
///
/// The computer drives one transmit line and samples one receive line; which
/// port pins those are is the attaching machine's business. Timing is expressed
/// in emulated CPU cycles so tests and recordings remain independent of host
/// scheduling.
#[derive(Debug)]
pub struct BitBangSerial {
    cycles_per_bit: u32,
    previous_tx: bool,
    receive_countdown: u32,
    receive_bit: u8,
    receive_byte: u8,
    received: VecDeque<u8>,
    transmit: VecDeque<u8>,
    transmit_byte: Option<u8>,
    transmit_bit: u8,
    transmit_countdown: u32,
    transmit_delay: u32,
}

/// Deterministic subset of ESP-AT used by the Rachel vintage clients.
#[derive(Debug)]
pub struct EspAtModem {
    serial: BitBangSerial,
    initial_cycles_per_bit: u32,
    command: Vec<u8>,
    payload_remaining: usize,
    payload: Vec<u8>,
    outbound_packets: VecDeque<Vec<u8>>,
    connect_requests: VecDeque<(String, u16)>,
    connected: bool,
    diagnostic_received: VecDeque<u8>,
    pending_cycles_per_bit: Option<u32>,
}

/// Optional real TCP transport behind the deterministic ESP-AT serial model.
#[derive(Debug)]
pub struct EspAtTcpBridge {
    modem: EspAtModem,
    stream: Option<TcpStream>,
    incoming: Vec<u8>,
    frame_size: usize,
    poll_countdown: u32,
    last_error: Option<String>,
}

impl EspAtTcpBridge {
    #[must_use]
    pub fn new(cycles_per_bit: u32, frame_size: usize) -> Self {
        assert!(frame_size != 0, "TCP frame size must not be zero");
        Self {
            modem: EspAtModem::new(cycles_per_bit),
            stream: None,
            incoming: Vec::new(),
            frame_size,
            poll_countdown: 0,
            last_error: None,
        }
    }

    pub fn tick(&mut self, transmit: bool) -> bool {
        let receive = self.modem.tick(transmit);
        if self.poll_countdown == 0 {
            self.service_tcp();
            self.poll_countdown = 255;
        } else {
            self.poll_countdown -= 1;
        }
        receive
    }

    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Whether the emulated modem currently holds an open TCP connection.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.modem.connected
    }

    /// Diagnostic query leaves this peripheral answers, relative to wherever a
    /// host runtime mounts it. A runtime advertises these while the modem is
    /// attached and drops them when it is unplugged, so the peripheral owns the
    /// names and the runtime owns only the mount point.
    #[cfg(feature = "query")]
    pub const QUERY_LEAVES: &'static [&'static str] = &["connected", "error", "received_hex"];

    /// Resolve one leaf from [`Self::QUERY_LEAVES`].
    ///
    /// Returns `None` for any other name, so a caller can fall through to its
    /// own paths rather than having to pre-filter.
    #[cfg(feature = "query")]
    #[must_use]
    pub fn query_leaf(&self, leaf: &str) -> Option<serde_json::Value> {
        match leaf {
            "connected" => Some(serde_json::Value::from(self.is_connected())),
            "error" => Some(serde_json::Value::from(self.last_error())),
            "received_hex" => Some(serde_json::Value::from(
                self.diagnostic_received()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
            )),
            _ => None,
        }
    }

    #[must_use]
    pub fn diagnostic_received(&self) -> Vec<u8> {
        self.modem.diagnostic_received()
    }

    fn service_tcp(&mut self) {
        if let Some((host, port)) = self.modem.take_connect_request() {
            match TcpStream::connect((host.as_str(), port)) {
                Ok(stream) => {
                    if let Err(error) = stream.set_nonblocking(true) {
                        self.last_error = Some(error.to_string());
                        self.modem.set_connected(false);
                    } else {
                        self.stream = Some(stream);
                        self.last_error = None;
                        self.modem.set_connected(true);
                    }
                }
                Err(error) => {
                    self.last_error = Some(error.to_string());
                    self.modem.set_connected(false);
                }
            }
        }

        while let Some(packet) = self.modem.take_outbound_packet() {
            let Some(stream) = &mut self.stream else {
                self.last_error = Some("CIPSEND payload received without a TCP connection".into());
                continue;
            };
            if let Err(error) = stream.write_all(&packet) {
                self.last_error = Some(error.to_string());
                self.stream = None;
                self.modem.set_link_closed();
                break;
            }
        }

        let Some(stream) = &mut self.stream else {
            return;
        };
        let mut buffer = [0u8; 1024];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => {
                    self.stream = None;
                    self.modem.set_link_closed();
                    break;
                }
                Ok(count) => self.incoming.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) => {
                    self.last_error = Some(error.to_string());
                    self.stream = None;
                    self.modem.set_link_closed();
                    break;
                }
            }
        }
        while self.incoming.len() >= self.frame_size {
            let frame: Vec<u8> = self.incoming.drain(..self.frame_size).collect();
            self.modem.queue_network_packet(&frame);
        }
    }
}

impl EspAtModem {
    #[must_use]
    pub fn new(cycles_per_bit: u32) -> Self {
        Self {
            serial: BitBangSerial::new(cycles_per_bit),
            initial_cycles_per_bit: cycles_per_bit,
            command: Vec::new(),
            payload_remaining: 0,
            payload: Vec::new(),
            outbound_packets: VecDeque::new(),
            connect_requests: VecDeque::new(),
            connected: false,
            diagnostic_received: VecDeque::new(),
            pending_cycles_per_bit: None,
        }
    }

    /// Advance the physical serial side by one emulated CPU cycle.
    pub fn tick(&mut self, transmit: bool) -> bool {
        let receive = self.serial.tick(transmit);
        for byte in self.serial.take_output() {
            self.accept_byte(byte);
        }
        if self.serial.input_idle()
            && let Some(cycles) = self.pending_cycles_per_bit.take()
        {
            self.serial.set_cycles_per_bit(cycles);
        }
        receive
    }

    /// Queue bytes received from the network. ESP-AT presents active TCP data
    /// using its `+IPD,<length>:` envelope.
    pub fn queue_network_packet(&mut self, bytes: &[u8]) {
        let header = format!("\r\n+IPD,{}:", bytes.len());
        self.serial.queue_input(header.as_bytes());
        self.serial.queue_input(bytes);
    }

    pub fn take_outbound_packet(&mut self) -> Option<Vec<u8>> {
        self.outbound_packets.pop_front()
    }

    pub fn take_connect_request(&mut self) -> Option<(String, u16)> {
        self.connect_requests.pop_front()
    }

    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
        if connected {
            self.queue_response(b"\r\nCONNECT\r\nOK\r\n");
        } else {
            self.queue_response(b"\r\nERROR\r\n");
        }
    }

    /// Report an established TCP link closing, matching ESP-AT's unsolicited
    /// status rather than silently leaving the emulated modem connected.
    pub fn set_link_closed(&mut self) {
        self.connected = false;
        self.payload_remaining = 0;
        self.payload.clear();
        self.queue_response(b"\r\nCLOSED\r\n");
    }

    #[must_use]
    pub fn diagnostic_received(&self) -> Vec<u8> {
        self.diagnostic_received.iter().copied().collect()
    }

    fn accept_byte(&mut self, byte: u8) {
        if self.diagnostic_received.len() == 512 {
            self.diagnostic_received.pop_front();
        }
        self.diagnostic_received.push_back(byte);
        if self.payload_remaining != 0 {
            self.payload.push(byte);
            self.payload_remaining -= 1;
            if self.payload_remaining == 0 {
                self.outbound_packets
                    .push_back(std::mem::take(&mut self.payload));
                self.queue_response(b"\r\nSEND OK\r\n");
            }
            return;
        }

        if byte == b'\r' {
            let command = String::from_utf8_lossy(&self.command).into_owned();
            self.command.clear();
            self.accept_command(&command);
        } else if byte != b'\n' && self.command.len() < 256 {
            self.command.push(byte);
        }
    }

    fn accept_command(&mut self, command: &str) {
        if let Some(rest) = command.strip_prefix("AT+CIPSEND=") {
            match rest.parse::<usize>() {
                Ok(length) if self.connected && length != 0 => {
                    self.payload_remaining = length;
                    self.payload.clear();
                    self.queue_response(b"> ");
                }
                _ => self.queue_response(b"\r\nERROR\r\n"),
            }
        } else if let Some(endpoint) = parse_cipstart(command) {
            self.connect_requests.push_back(endpoint);
        } else if command == "AT+UART_CUR=2400,8,1,0,0" {
            // Complete OK at the old rate, then switch both serial directions.
            self.queue_response(b"\r\nOK\r\n");
            self.pending_cycles_per_bit = Some(self.initial_cycles_per_bit * 4);
        } else if command == "AT+CIPCLOSE" {
            self.connected = false;
            self.queue_response(b"\r\nCLOSED\r\nOK\r\n");
        } else if command == "AT" || command.starts_with("AT+") {
            self.queue_response(b"\r\nOK\r\n");
        } else if !command.is_empty() {
            self.queue_response(b"\r\nERROR\r\n");
        }
    }

    fn queue_response(&mut self, bytes: &[u8]) {
        // Do not begin a reply while the computer is still returning from its
        // transmit routine. Real ESP firmware has millisecond-scale command
        // processing latency; ten bit cells provide a deterministic ~1 ms
        // turnaround at 9600 baud.
        self.serial
            .queue_input_after(bytes, self.serial.cycles_per_bit * 10);
    }
}

fn parse_cipstart(command: &str) -> Option<(String, u16)> {
    let rest = command.strip_prefix("AT+CIPSTART=\"TCP\",\"")?;
    let (host, port) = rest.rsplit_once("\",")?;
    Some((host.to_owned(), port.parse().ok()?))
}

impl BitBangSerial {
    #[must_use]
    pub fn new(cycles_per_bit: u32) -> Self {
        assert!(
            cycles_per_bit >= 2,
            "serial bit period must be at least two cycles"
        );
        Self {
            cycles_per_bit,
            previous_tx: true,
            receive_countdown: 0,
            receive_bit: 0,
            receive_byte: 0,
            received: VecDeque::new(),
            transmit: VecDeque::new(),
            transmit_byte: None,
            transmit_bit: 0,
            transmit_countdown: 0,
            transmit_delay: 0,
        }
    }

    pub fn queue_input(&mut self, bytes: &[u8]) {
        self.transmit.extend(bytes.iter().copied());
    }

    fn queue_input_after(&mut self, bytes: &[u8], cycles: u32) {
        self.transmit.extend(bytes.iter().copied());
        self.transmit_delay = self.transmit_delay.max(cycles);
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        self.received.drain(..).collect()
    }

    fn input_idle(&self) -> bool {
        self.transmit.is_empty() && self.transmit_byte.is_none() && self.transmit_delay == 0
    }

    fn set_cycles_per_bit(&mut self, cycles_per_bit: u32) {
        assert!(cycles_per_bit >= 2);
        self.cycles_per_bit = cycles_per_bit;
    }

    /// Advance one emulated CPU cycle and return the PB0 level for that cycle.
    pub fn tick(&mut self, transmit: bool) -> bool {
        self.receive_tick(transmit);
        self.previous_tx = transmit;
        self.transmit_tick()
    }

    fn receive_tick(&mut self, level: bool) {
        if self.receive_countdown == 0 {
            if self.receive_bit == 0 {
                if self.previous_tx && !level {
                    // Sample data bit zero in the middle of its cell: one full
                    // start bit plus half a data bit from the falling edge.
                    self.receive_bit = 1;
                    self.receive_byte = 0;
                    self.receive_countdown = self.cycles_per_bit + self.cycles_per_bit / 2 - 1;
                }
                return;
            }

            if self.receive_bit <= 8 {
                if level {
                    self.receive_byte |= 1 << (self.receive_bit - 1);
                }
                self.receive_bit += 1;
                self.receive_countdown = self.cycles_per_bit - 1;
            } else {
                if level {
                    self.received.push_back(self.receive_byte);
                }
                self.receive_bit = 0;
            }
        } else {
            self.receive_countdown -= 1;
        }
    }

    fn transmit_tick(&mut self) -> bool {
        if self.transmit_byte.is_none() {
            if self.transmit_delay != 0 {
                self.transmit_delay -= 1;
                return true;
            }
            self.transmit_byte = self.transmit.pop_front();
            self.transmit_bit = 0;
            self.transmit_countdown = self.cycles_per_bit;
        }
        let Some(byte) = self.transmit_byte else {
            return true;
        };

        let level = match self.transmit_bit {
            0 => false,
            1..=8 => byte & (1 << (self.transmit_bit - 1)) != 0,
            _ => true,
        };
        self.transmit_countdown -= 1;
        if self.transmit_countdown == 0 {
            self.transmit_bit += 1;
            self.transmit_countdown = self.cycles_per_bit;
            if self.transmit_bit > 9 {
                self.transmit_byte = None;
            }
        }
        level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive_byte(serial: &mut BitBangSerial, byte: u8, period: u32) {
        for bit in 0..10 {
            let level = match bit {
                0 => false,
                1..=8 => byte & (1 << (bit - 1)) != 0,
                _ => true,
            };
            for _ in 0..period {
                serial.tick(level);
            }
        }
        serial.tick(true);
    }

    #[test]
    fn decodes_8n1_bytes_from_cycle_levels() {
        let mut serial = BitBangSerial::new(12);
        drive_byte(&mut serial, 0xA5, 12);
        assert_eq!(serial.take_output(), [0xA5]);
    }

    #[test]
    fn emits_start_data_and_stop_bits_for_queued_byte() {
        let mut serial = BitBangSerial::new(4);
        serial.queue_input(&[0x03]);
        let levels: Vec<bool> = (0..40).map(|_| serial.tick(true)).collect();
        for bit in 0..10 {
            let expected = match bit {
                0 => false,
                1..=8 => 0x03 & (1 << (bit - 1)) != 0,
                _ => true,
            };
            assert!(levels[bit * 4..bit * 4 + 4].iter().all(|&v| v == expected));
        }
    }

    fn send_host_bytes(modem: &mut EspAtModem, bytes: &[u8], period: u32) {
        for &byte in bytes {
            for bit in 0..10 {
                let level = match bit {
                    0 => false,
                    1..=8 => byte & (1 << (bit - 1)) != 0,
                    _ => true,
                };
                for _ in 0..period {
                    modem.tick(level);
                }
            }
            modem.tick(true);
        }
    }

    fn drain_device_bytes(modem: &mut EspAtModem, cycles: usize) -> Vec<u8> {
        let mut decoder = BitBangSerial::new(modem.serial.cycles_per_bit);
        for _ in 0..cycles {
            let level = modem.tick(true);
            decoder.tick(level);
        }
        decoder.take_output()
    }

    #[test]
    fn esp_at_parses_connect_and_emits_result() {
        let mut modem = EspAtModem::new(12);
        send_host_bytes(&mut modem, b"AT+CIPSTART=\"TCP\",\"127.0.0.1\",6502\r", 12);
        assert_eq!(
            modem.take_connect_request(),
            Some(("127.0.0.1".to_owned(), 6502))
        );
        modem.set_connected(true);
        let response = drain_device_bytes(&mut modem, 12 * 10 * 40);
        assert!(response.windows(7).any(|window| window == b"CONNECT"));
        assert!(response.windows(2).any(|window| window == b"OK"));
    }

    #[test]
    fn esp_at_cipsend_collects_exact_payload() {
        let mut modem = EspAtModem::new(12);
        modem.connected = true;
        send_host_bytes(&mut modem, b"AT+CIPSEND=4\r", 12);
        let prompt = drain_device_bytes(&mut modem, 12 * 10 * 16);
        assert!(prompt.contains(&b'>'));
        send_host_bytes(&mut modem, b"RACH", 12);
        assert_eq!(modem.take_outbound_packet(), Some(b"RACH".to_vec()));
    }

    #[test]
    fn esp_at_switches_baud_after_uart_ok_finishes() {
        let mut modem = EspAtModem::new(12);
        send_host_bytes(&mut modem, b"AT+UART_CUR=2400,8,1,0,0\r", 12);
        assert_eq!(modem.serial.cycles_per_bit, 12);
        let response = drain_device_bytes(&mut modem, 12 * 10 * 16);
        assert!(response.windows(2).any(|window| window == b"OK"));
        assert_eq!(modem.serial.cycles_per_bit, 48);

        modem.connected = true;
        send_host_bytes(&mut modem, b"AT+CIPSEND=4\r", 48);
        let prompt = drain_device_bytes(&mut modem, 48 * 10 * 16);
        assert!(prompt.contains(&b'>'));
        send_host_bytes(&mut modem, b"RACH", 48);
        let _ = drain_device_bytes(&mut modem, 48 * 10 * 16);

        send_host_bytes(&mut modem, b"AT+UART_CUR=2400,8,1,0,0\r", 48);
        let response = drain_device_bytes(&mut modem, 48 * 10 * 16);
        assert!(response.windows(2).any(|window| window == b"OK"));
        assert_eq!(modem.serial.cycles_per_bit, 48);
    }

    #[test]
    fn esp_at_wraps_network_input_in_ipd() {
        let mut modem = EspAtModem::new(12);
        modem.queue_network_packet(b"RACH");
        let bytes = drain_device_bytes(&mut modem, 12 * 10 * 20);
        assert!(bytes.windows(11).any(|window| window == b"+IPD,4:RACH"));
    }

    #[test]
    fn esp_at_reports_established_link_closure() {
        let mut modem = EspAtModem::new(12);
        modem.connected = true;
        modem.set_link_closed();
        let bytes = drain_device_bytes(&mut modem, 12 * 10 * 20);
        assert!(!modem.connected);
        assert!(bytes.windows(6).any(|window| window == b"CLOSED"));
    }

    #[test]
    fn cipstart_parser_rejects_non_tcp_and_bad_ports() {
        assert_eq!(
            parse_cipstart("AT+CIPSTART=\"TCP\",\"example.test\",6502"),
            Some(("example.test".to_owned(), 6502))
        );
        assert_eq!(parse_cipstart("AT+CIPSTART=\"UDP\",\"x\",1"), None);
        assert_eq!(parse_cipstart("AT+CIPSTART=\"TCP\",\"x\",bad"), None);
    }
}
