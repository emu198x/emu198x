//! MOS 6551 Asynchronous Communication Interface Adapter (ACIA).
//!
//! Four registers behind two register-select lines, an on-chip baud-rate
//! generator clocked from a 1.8432 MHz crystal, a framed transmitter and
//! receiver, an open-drain `IRQ` output, and the `CTS`/`DSR`/`DCD` modem
//! inputs.
//!
//! # Sources
//!
//! - Register layout, bit meanings and the hardware/programmed reset values:
//!   Synertek SY6551 datasheet, 1979 Synertek Data Catalog pp. 5-91..5-98
//!   (`reference/by-topic/riot-6532/1979-synertek-data-catalog.txt`; the
//!   register figures were read from the page images, PDF pp. 227-228,
//!   because the text layer interleaves their columns).
//! - Baud-rate divisors, the `CTS` gate on `TDRE`, and the IRQ-source
//!   bookkeeping follow MAME's `mos6551.cpp` as the reference implementation
//!   (`198x/emulators/multi-system/mame/src/devices/machine/mos6551.cpp`).
//!
//! # Where this departs from the Synertek text
//!
//! Synertek's Figure 7 labels command bit 0 "Disable Receiver/Transmitter".
//! The Dragon 64 ROM — written for Rockwell's R6551 — transmits with that bit
//! clear: its reset code writes command `$0A`, and `SEROUT` (`$BE98` in the
//! compatible-mode ROM) only polls `TDRE` and writes the data register,
//! never touching `DTR`. `DLOAD` sends eight filename bytes back to back
//! through it, and serial printing sends whole lines. Were the transmitter
//! gated by `DTR`, the second byte would wait for a `TDRE` that never comes.
//! So here `DTR` disables the receiver and every interrupt source, and the
//! transmitter runs regardless. MAME gates both; it is the one place this
//! model chooses the ROM over the reference emulator.
//!
//! # Host endpoint
//!
//! The serial line itself is modelled at frame level, not as bit levels on a
//! wire. [`Acia6551::queue_received`] hands bytes to an attached peer that
//! drives them into the receiver one frame at a time, using the ACIA's own
//! baud rate and frame format — the peer is assumed to be configured to
//! match, as a real terminal or printer must be. It honours `DTR` as flow
//! control and starts a frame only while the receiver is enabled.
//! [`Acia6551::take_transmitted`] drains the bytes whose frames have
//! completed on `TxD`. Neither queue is machine state, so neither is
//! serialised: a snapshot captures the chip, not the device at the far end
//! of the cable.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Nominal crystal frequency the on-chip baud-rate generator divides.
pub const XTAL_HZ: u32 = 1_843_200;

/// Crystal periods per 16x bit clock for each control-register baud code.
///
/// Code 0 selects the external 16x clock input, which is not modelled; a
/// divisor of zero stops both directions. The rest give
/// `XTAL_HZ / (16 * divisor)` = 50, 75, 109.92, 134.58, 150, 300, 600, 1200,
/// 1800, 2400, 3600, 4800, 7200, 9600 and 19200 baud (datasheet Figure 6;
/// MAME `internal_divider`).
const BAUD_DIVISORS: [u32; 16] = [
    0, 2304, 1536, 1048, 856, 768, 384, 192, 96, 64, 48, 32, 24, 16, 12, 6,
];

/// 16x clocks in one bit time.
const BIT_CLOCKS: u32 = 16;

/// Status register bits (datasheet Figure 8).
pub mod status {
    /// Parity error in the last received character.
    pub const PARITY_ERROR: u8 = 0x01;
    /// Framing error in the last received character.
    pub const FRAMING_ERROR: u8 = 0x02;
    /// A character arrived while the receive data register was still full.
    pub const OVERRUN: u8 = 0x04;
    /// Receive data register full.
    pub const RDRF: u8 = 0x08;
    /// Transmit data register empty.
    pub const TDRE: u8 = 0x10;
    /// `DCD` input high (carrier not detected).
    pub const DCD_HIGH: u8 = 0x20;
    /// `DSR` input high (data set not ready).
    pub const DSR_HIGH: u8 = 0x40;
    /// An interrupt has occurred since the status register was last read.
    pub const IRQ: u8 = 0x80;
}

/// Command register bits (datasheet Figure 7).
pub mod command {
    /// Data terminal ready: 1 enables the receiver and interrupts, `DTR` low.
    pub const DTR: u8 = 0x01;
    /// Receiver interrupt *disable*: 1 masks the `RDRF` interrupt.
    pub const RX_IRQ_DISABLE: u8 = 0x02;
    /// Transmitter control field (bits 2-3).
    pub const TX_CONTROL_MASK: u8 = 0x0C;
    /// Receiver echo mode.
    pub const ECHO: u8 = 0x10;
    /// Parity enable (bit 5); bits 6-7 then select odd/even/mark/space.
    pub const PARITY_ENABLE: u8 = 0x20;
}

/// Control register bits (datasheet Figure 6).
pub mod control {
    /// Baud-rate code (bits 0-3).
    pub const BAUD_MASK: u8 = 0x0F;
    /// Receiver clock source: 1 = baud-rate generator, 0 = external `RxC`.
    pub const RX_CLOCK_INTERNAL: u8 = 0x10;
    /// Word-length field (bits 5-6): 00 = 8, 01 = 7, 10 = 6, 11 = 5 bits.
    pub const WORD_LENGTH_MASK: u8 = 0x60;
    /// Stop-bit select.
    pub const STOP_BITS: u8 = 0x80;
}

/// Command register value after a hardware reset (`RES` low): `0000 0010`.
pub const COMMAND_HARDWARE_RESET: u8 = 0x02;
/// Control register value after a hardware reset: all zero.
pub const CONTROL_HARDWARE_RESET: u8 = 0x00;

const IRQ_RDRF: u8 = 0x01;
const IRQ_TDRE: u8 = 0x02;
const IRQ_DCD: u8 = 0x04;
const IRQ_DSR: u8 = 0x08;

/// One character in flight on a serial line, timed in 16x clocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Frame {
    /// The character, already masked to the frame's word length.
    data: u8,
    /// 16x clocks elapsed since the leading edge of the start bit.
    elapsed: u32,
    /// 16x clocks from the start bit's leading edge to the middle of the
    /// first stop bit, where the receiver transfers the character.
    sample_at: u32,
    /// 16x clocks for the whole frame, stop bits included.
    length: u32,
    /// Receiver only: whether the character has reached the data register.
    delivered: bool,
}

impl Frame {
    fn new(data: u8, format: FrameFormat) -> Self {
        let bits_before_stop = 1 + u32::from(format.data_bits) + u32::from(format.parity);
        Self {
            data: data & format.data_mask(),
            elapsed: 0,
            sample_at: bits_before_stop * BIT_CLOCKS + BIT_CLOCKS / 2,
            length: bits_before_stop * BIT_CLOCKS + format.stop_clocks(),
            delivered: false,
        }
    }
}

/// The character framing selected by the control and command registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameFormat {
    /// Data bits per character, 5-8.
    pub data_bits: u8,
    /// Whether a parity bit follows the data bits.
    pub parity: bool,
    /// Stop bits in half-bit units: 2 = 1, 3 = 1.5, 4 = 2.
    pub stop_half_bits: u8,
}

impl FrameFormat {
    fn from_registers(control: u8, command: u8) -> Self {
        let data_bits = 8 - ((control & control::WORD_LENGTH_MASK) >> 5);
        let parity = command & command::PARITY_ENABLE != 0;
        // Figure 6: bit 7 selects two stop bits, except one for eight data
        // bits plus parity and one and a half for five bits without parity.
        let stop_half_bits = if control & control::STOP_BITS == 0 || (data_bits == 8 && parity) {
            2
        } else if data_bits == 5 && !parity {
            3
        } else {
            4
        };
        Self {
            data_bits,
            parity,
            stop_half_bits,
        }
    }

    fn data_mask(self) -> u8 {
        u8::MAX >> (8 - self.data_bits)
    }

    fn stop_clocks(self) -> u32 {
        u32::from(self.stop_half_bits) * BIT_CLOCKS / 2
    }

    /// 16x clocks one character occupies on the line, start to last stop bit.
    #[must_use]
    pub fn frame_clocks(self) -> u32 {
        (1 + u32::from(self.data_bits) + u32::from(self.parity)) * BIT_CLOCKS + self.stop_clocks()
    }
}

/// Side-effect-free view of the ACIA's registers and line state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Acia6551Registers {
    /// Status register as a read would return it, without clearing `IRQ`.
    pub status: u8,
    /// Command register.
    pub command: u8,
    /// Control register.
    pub control: u8,
    /// Receive data register.
    pub receive_data: u8,
    /// Transmit data register.
    pub transmit_data: u8,
    /// Whether a character is being shifted out on `TxD`.
    pub transmitting: bool,
    /// Whether a character is being shifted in on `RxD`.
    pub receiving: bool,
    /// Level of the `IRQ` output (true = asserted, pin low).
    pub irq: bool,
}

/// MOS 6551 ACIA.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Acia6551 {
    control: u8,
    command: u8,
    transmit_data: u8,
    receive_data: u8,
    tdre: bool,
    rdrf: bool,
    overrun: bool,
    framing_error: bool,
    parity_error: bool,
    /// Interrupt sources latched since the last status read.
    irq_sources: u8,
    /// `CTS` input asserted (pin low).
    cts: bool,
    /// `DSR` input asserted (pin low).
    dsr: bool,
    /// `DCD` input asserted (pin low).
    dcd: bool,
    /// Crystal periods into the current 16x clock.
    xtal_phase: u32,
    transmitter: Option<Frame>,
    receiver: Option<Frame>,
    #[serde(skip)]
    host_input: VecDeque<u8>,
    #[serde(skip)]
    host_output: Vec<u8>,
}

impl Default for Acia6551 {
    fn default() -> Self {
        Self::new()
    }
}

impl Acia6551 {
    /// A 6551 after a hardware reset, with its modem inputs asserted — the
    /// state of a cable to a powered, ready device.
    #[must_use]
    pub fn new() -> Self {
        let mut acia = Self {
            control: CONTROL_HARDWARE_RESET,
            command: COMMAND_HARDWARE_RESET,
            transmit_data: 0,
            receive_data: 0,
            tdre: true,
            rdrf: false,
            overrun: false,
            framing_error: false,
            parity_error: false,
            irq_sources: 0,
            cts: true,
            dsr: true,
            dcd: true,
            xtal_phase: 0,
            transmitter: None,
            receiver: None,
            host_input: VecDeque::new(),
            host_output: Vec::new(),
        };
        acia.reset();
        acia
    }

    /// Hardware reset (`RES` low).
    ///
    /// Control clears to `$00` and command to `$02`; status returns to
    /// `TDRE` set with every other latched bit clear (datasheet Figures 6-8).
    /// Characters part-way through the shift registers are abandoned. The
    /// modem inputs and the host endpoint are outside the chip and keep
    /// their state.
    pub fn reset(&mut self) {
        self.control = CONTROL_HARDWARE_RESET;
        self.command = COMMAND_HARDWARE_RESET;
        self.tdre = true;
        self.rdrf = false;
        self.overrun = false;
        self.framing_error = false;
        self.parity_error = false;
        self.irq_sources = 0;
        self.xtal_phase = 0;
        self.transmitter = None;
        self.receiver = None;
    }

    /// CPU read of register `rs` (`RS1:RS0`), with the datasheet side
    /// effects: reading the receive data register clears `RDRF` and the
    /// error bits, and reading status clears the interrupt.
    pub fn read(&mut self, rs: u8) -> u8 {
        match rs & 0x03 {
            0 => {
                self.rdrf = false;
                self.overrun = false;
                self.framing_error = false;
                self.parity_error = false;
                self.receive_data
            }
            1 => {
                let value = self.status();
                self.irq_sources = 0;
                value
            }
            2 => self.command,
            _ => self.control,
        }
    }

    /// Side-effect-free view of register `rs`.
    #[must_use]
    pub fn peek(&self, rs: u8) -> u8 {
        match rs & 0x03 {
            0 => self.receive_data,
            1 => self.status(),
            2 => self.command,
            _ => self.control,
        }
    }

    /// CPU write to register `rs` (`RS1:RS0`).
    ///
    /// Register 1 is not a register on write: any value performs the
    /// programmed reset, which clears the overrun latch and command bits
    /// 0-4 to `00010` while leaving parity and control untouched.
    pub fn write(&mut self, rs: u8, value: u8) {
        match rs & 0x03 {
            0 => {
                self.transmit_data = value;
                self.tdre = false;
            }
            1 => {
                self.overrun = false;
                self.irq_sources &= !(IRQ_DCD | IRQ_DSR);
                self.write_command((self.command & 0xE0) | COMMAND_HARDWARE_RESET);
            }
            2 => self.write_command(value),
            _ => {
                self.control = value;
                self.xtal_phase = 0;
            }
        }
    }

    fn write_command(&mut self, value: u8) {
        self.command = value;
        if !self.rx_irq_enabled() {
            self.irq_sources &= !IRQ_RDRF;
        }
        if !self.tx_irq_enabled() {
            self.irq_sources &= !IRQ_TDRE;
        } else if self.tdre {
            self.irq_sources |= IRQ_TDRE;
        }
    }

    fn status(&self) -> u8 {
        let mut value = 0;
        if self.parity_error {
            value |= status::PARITY_ERROR;
        }
        if self.framing_error {
            value |= status::FRAMING_ERROR;
        }
        if self.overrun {
            value |= status::OVERRUN;
        }
        if self.rdrf {
            value |= status::RDRF;
        }
        // CTS high holds TDRE off as well as the transmitter (MAME).
        if self.tdre && self.cts {
            value |= status::TDRE;
        }
        if !self.dcd {
            value |= status::DCD_HIGH;
        }
        if !self.dsr {
            value |= status::DSR_HIGH;
        }
        if self.irq_sources != 0 {
            value |= status::IRQ;
        }
        value
    }

    /// Level of the open-drain `IRQ` output: true when asserted (pin low).
    #[must_use]
    pub fn irq(&self) -> bool {
        self.irq_sources != 0
    }

    /// Whether the `DTR` output is asserted (pin low).
    #[must_use]
    pub fn dtr(&self) -> bool {
        self.command & command::DTR != 0
    }

    /// Whether the `RTS` output is asserted (pin low): transmitter control
    /// other than `00`, or echo mode.
    #[must_use]
    pub fn rts(&self) -> bool {
        self.tx_control() != 0 || self.command & command::ECHO != 0
    }

    /// Drive the `CTS` input; `asserted` means the pin is low.
    ///
    /// Deasserting `CTS` stops the transmitter at once, abandoning any
    /// character part-way out.
    pub fn set_cts(&mut self, asserted: bool) {
        self.cts = asserted;
        if !asserted {
            self.transmitter = None;
        }
    }

    /// Drive the `DSR` input; `asserted` means the pin is low. A change
    /// interrupts while `DTR` is asserted.
    pub fn set_dsr(&mut self, asserted: bool) {
        if self.dsr != asserted && self.dtr() {
            self.irq_sources |= IRQ_DSR;
        }
        self.dsr = asserted;
    }

    /// Drive the `DCD` input; `asserted` means the pin is low. A change
    /// interrupts while `DTR` is asserted.
    pub fn set_dcd(&mut self, asserted: bool) {
        if self.dcd != asserted && self.dtr() {
            self.irq_sources |= IRQ_DCD;
        }
        self.dcd = asserted;
    }

    /// The framing the registers currently select.
    #[must_use]
    pub fn frame_format(&self) -> FrameFormat {
        FrameFormat::from_registers(self.control, self.command)
    }

    /// Crystal periods per 16x clock, or `None` when the external clock is
    /// selected.
    #[must_use]
    pub fn clock_divisor(&self) -> Option<u32> {
        let divisor = BAUD_DIVISORS[usize::from(self.control & control::BAUD_MASK)];
        (divisor != 0).then_some(divisor)
    }

    /// The selected baud rate, or `None` for the external clock.
    #[must_use]
    pub fn baud_rate(&self) -> Option<f64> {
        self.clock_divisor()
            .map(|divisor| f64::from(XTAL_HZ) / f64::from(divisor * BIT_CLOCKS))
    }

    /// Side-effect-free diagnostic view.
    #[must_use]
    pub fn registers(&self) -> Acia6551Registers {
        Acia6551Registers {
            status: self.status(),
            command: self.command,
            control: self.control,
            receive_data: self.receive_data,
            transmit_data: self.transmit_data,
            transmitting: self.transmitter.is_some(),
            receiving: self.receiver.is_some(),
            irq: self.irq(),
        }
    }

    /// Host side: queue bytes for the peer to send into `RxD`.
    pub fn queue_received(&mut self, bytes: &[u8]) {
        self.host_input.extend(bytes);
    }

    /// Host side: bytes queued but not yet started on `RxD`.
    #[must_use]
    pub fn pending_received(&self) -> usize {
        self.host_input.len()
    }

    /// Host side: drain the characters whose frames have finished on `TxD`.
    pub fn take_transmitted(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.host_output)
    }

    /// Advance by `xtal_ticks` periods of the 1.8432 MHz crystal.
    pub fn advance(&mut self, xtal_ticks: u32) {
        let Some(divisor) = self.clock_divisor() else {
            return;
        };
        self.xtal_phase = self.xtal_phase.saturating_add(xtal_ticks);
        while self.xtal_phase >= divisor {
            self.xtal_phase -= divisor;
            self.clock_16x();
        }
    }

    fn tx_control(&self) -> u8 {
        (self.command & command::TX_CONTROL_MASK) >> 2
    }

    fn rx_irq_enabled(&self) -> bool {
        self.dtr() && self.command & command::RX_IRQ_DISABLE == 0
    }

    fn tx_irq_enabled(&self) -> bool {
        self.dtr() && self.tx_control() == 0b01
    }

    fn receiver_enabled(&self) -> bool {
        self.dtr() && self.control & control::RX_CLOCK_INTERNAL != 0
    }

    /// The transmitter needs `RTS` asserted and `CTS` asserted. `DTR` does
    /// not gate it; see the crate documentation.
    fn transmitter_enabled(&self) -> bool {
        self.cts && self.tx_control() != 0
    }

    fn clock_16x(&mut self) {
        self.clock_transmitter();
        self.clock_receiver();
    }

    fn clock_transmitter(&mut self) {
        if let Some(frame) = &mut self.transmitter {
            frame.elapsed += 1;
            if frame.elapsed >= frame.length {
                self.host_output.push(frame.data);
                self.transmitter = None;
            }
        }
        if self.transmitter.is_none() && !self.tdre && self.transmitter_enabled() {
            // The data register moves to the shift register as the start
            // bit begins, which is what empties it.
            self.transmitter = Some(Frame::new(self.transmit_data, self.frame_format()));
            self.tdre = true;
            if self.tx_irq_enabled() {
                self.irq_sources |= IRQ_TDRE;
            }
        }
    }

    fn clock_receiver(&mut self) {
        if !self.receiver_enabled() {
            // A receiver disabled mid-character loses it.
            self.receiver = None;
            return;
        }
        if let Some(frame) = &mut self.receiver {
            frame.elapsed += 1;
            if !frame.delivered && frame.elapsed >= frame.sample_at {
                frame.delivered = true;
                let data = frame.data;
                self.deliver(data);
            }
            if self
                .receiver
                .is_some_and(|frame| frame.elapsed >= frame.length)
            {
                self.receiver = None;
            }
        }
        if self.receiver.is_none()
            && let Some(data) = self.host_input.pop_front()
        {
            self.receiver = Some(Frame::new(data, self.frame_format()));
        }
    }

    fn deliver(&mut self, data: u8) {
        // A character arriving over an unread one sets the overrun latch and
        // replaces it, as MAME's receiver does.
        if self.rdrf {
            self.overrun = true;
        }
        self.receive_data = data;
        self.rdrf = true;
        if self.command & command::ECHO != 0 {
            self.host_output.push(data);
        }
        if self.rx_irq_enabled() {
            self.irq_sources |= IRQ_RDRF;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA: u8 = 0;
    const STATUS: u8 = 1;
    const COMMAND: u8 = 2;
    const CONTROL: u8 = 3;

    /// The Dragon 64 ROM's reset setting: 1200 baud from the generator,
    /// eight data bits, two stop bits; no parity, receiver interrupt off,
    /// transmitter interrupt off with RTS low, DTR off.
    const DRAGON_CONTROL: u8 = 0x98;
    const DRAGON_COMMAND: u8 = 0x0A;
    /// Crystal periods in one 1200 baud 8N2 frame: 11 bits x 16 x 96.
    const DRAGON_FRAME_XTAL: u32 = 11 * 16 * 96;

    fn dragon_acia() -> Acia6551 {
        let mut acia = Acia6551::new();
        acia.write(COMMAND, DRAGON_COMMAND);
        acia.write(CONTROL, DRAGON_CONTROL);
        acia
    }

    #[test]
    fn hardware_reset_values_match_the_datasheet() {
        let mut acia = Acia6551::new();
        acia.write(CONTROL, 0xFF);
        acia.write(COMMAND, 0xFF);
        acia.write(DATA, 0x55);
        acia.reset();

        assert_eq!(acia.peek(CONTROL), 0x00);
        assert_eq!(acia.peek(COMMAND), 0x02);
        assert_eq!(acia.peek(STATUS), status::TDRE);
        assert!(!acia.irq());
    }

    #[test]
    fn control_register_reads_back_and_selects_the_frame() {
        let mut acia = Acia6551::new();
        acia.write(CONTROL, DRAGON_CONTROL);

        assert_eq!(acia.read(CONTROL), DRAGON_CONTROL);
        assert_eq!(acia.clock_divisor(), Some(96));
        let baud = acia.baud_rate().expect("internal clock");
        assert!((baud - 1200.0).abs() < 1e-9, "{baud}");
        assert_eq!(
            acia.frame_format(),
            FrameFormat {
                data_bits: 8,
                parity: false,
                stop_half_bits: 4
            }
        );
    }

    #[test]
    fn baud_codes_divide_the_crystal_to_the_datasheet_rates() {
        let expected = [
            50.0, 75.0, 109.92, 134.58, 150.0, 300.0, 600.0, 1200.0, 1800.0, 2400.0, 3600.0,
            4800.0, 7200.0, 9600.0, 19200.0,
        ];
        let mut acia = Acia6551::new();
        acia.write(CONTROL, 0);
        assert_eq!(acia.baud_rate(), None, "code 0 is the external clock");
        for (code, want) in (1u8..).zip(expected) {
            acia.write(CONTROL, code);
            let got = acia.baud_rate().expect("internal clock");
            assert!((got - want).abs() < 0.01, "code {code}: {got} vs {want}");
        }
    }

    #[test]
    fn stop_bit_select_follows_figure_six() {
        let mut acia = Acia6551::new();
        let format = |acia: &Acia6551| acia.frame_format().stop_half_bits;
        acia.write(CONTROL, 0x00);
        assert_eq!(format(&acia), 2, "bit 7 clear: one stop bit");
        acia.write(CONTROL, 0x80);
        assert_eq!(format(&acia), 4, "8 bits, no parity: two");
        acia.write(COMMAND, command::PARITY_ENABLE);
        assert_eq!(format(&acia), 2, "8 bits plus parity: one");
        acia.write(COMMAND, 0);
        acia.write(CONTROL, 0xE0);
        assert_eq!(format(&acia), 3, "5 bits, no parity: one and a half");
        assert_eq!(acia.frame_format().data_bits, 5);
    }

    #[test]
    fn command_register_reads_back() {
        let mut acia = Acia6551::new();
        acia.write(COMMAND, 0xE5);
        assert_eq!(acia.read(COMMAND), 0xE5);
    }

    #[test]
    fn programmed_reset_clears_command_low_bits_and_overrun_only() {
        let mut acia = dragon_acia();
        acia.write(COMMAND, 0xFF);
        acia.overrun = true;
        acia.write(STATUS, 0x00);

        assert_eq!(acia.peek(COMMAND), 0xE2, "bits 5-7 kept, 0-4 = 00010");
        assert_eq!(acia.peek(CONTROL), DRAGON_CONTROL, "control unchanged");
        assert_eq!(acia.peek(STATUS) & status::OVERRUN, 0);
    }

    #[test]
    fn transmit_completes_after_one_frame_at_the_programmed_rate() {
        let mut acia = dragon_acia();
        acia.write(DATA, 0x41);
        assert_eq!(acia.peek(STATUS) & status::TDRE, 0, "write empties TDRE");

        acia.advance(96);
        assert_ne!(
            acia.peek(STATUS) & status::TDRE,
            0,
            "the shift register takes the byte at the first bit clock"
        );
        acia.advance(DRAGON_FRAME_XTAL - 1);
        assert!(acia.take_transmitted().is_empty(), "stop bits not done yet");
        acia.advance(1);
        assert_eq!(acia.take_transmitted(), vec![0x41]);
    }

    #[test]
    fn transmitter_runs_with_dtr_off_as_the_dragon_rom_requires() {
        let mut acia = dragon_acia();
        assert!(!acia.dtr());
        for byte in *b"DL" {
            while acia.peek(STATUS) & status::TDRE == 0 {
                acia.advance(1);
            }
            acia.write(DATA, byte);
        }
        acia.advance(3 * DRAGON_FRAME_XTAL);
        assert_eq!(acia.take_transmitted(), b"DL".to_vec());
    }

    #[test]
    fn rts_high_holds_the_transmitter() {
        let mut acia = dragon_acia();
        acia.write(COMMAND, 0x00);
        assert!(!acia.rts());
        acia.write(DATA, 0x41);
        acia.advance(2 * DRAGON_FRAME_XTAL);
        assert!(acia.take_transmitted().is_empty());
    }

    #[test]
    fn cts_high_masks_tdre_and_stops_the_transmitter() {
        let mut acia = dragon_acia();
        acia.set_cts(false);
        assert_eq!(acia.peek(STATUS) & status::TDRE, 0);
        acia.write(DATA, 0x41);
        acia.advance(2 * DRAGON_FRAME_XTAL);
        assert!(acia.take_transmitted().is_empty());

        acia.set_cts(true);
        acia.advance(2 * DRAGON_FRAME_XTAL);
        assert_eq!(acia.take_transmitted(), vec![0x41]);
    }

    #[test]
    fn receiver_waits_for_dtr_and_delivers_mid_stop_bit() {
        let mut acia = dragon_acia();
        acia.queue_received(&[0x5A]);
        acia.advance(4 * DRAGON_FRAME_XTAL);
        assert_eq!(acia.peek(STATUS) & status::RDRF, 0, "DTR off: no reception");
        assert_eq!(acia.pending_received(), 1);

        acia.write(COMMAND, DRAGON_COMMAND | command::DTR);
        // Start bit begins at the first 16x clock; the character lands in
        // the middle of the first stop bit, 9.5 bits later.
        acia.advance(96 + (9 * 16 + 8) * 96 - 1);
        assert_eq!(acia.peek(STATUS) & status::RDRF, 0);
        acia.advance(1);
        assert_ne!(acia.peek(STATUS) & status::RDRF, 0);
        assert!(!acia.irq(), "receiver interrupt disabled by command bit 1");

        assert_eq!(acia.read(DATA), 0x5A);
        assert_eq!(acia.peek(STATUS) & status::RDRF, 0, "reading clears RDRF");
    }

    #[test]
    fn receiver_strips_bits_beyond_the_word_length() {
        let mut acia = Acia6551::new();
        acia.write(CONTROL, 0x18 | 0x20); // 1200 baud, seven bits
        acia.write(COMMAND, 0x0B);
        acia.queue_received(&[0xC1]);
        acia.advance(2 * DRAGON_FRAME_XTAL);
        assert_eq!(acia.read(DATA), 0x41);
    }

    #[test]
    fn unread_character_sets_overrun_until_the_data_register_is_read() {
        let mut acia = dragon_acia();
        acia.write(COMMAND, DRAGON_COMMAND | command::DTR);
        acia.queue_received(&[0x01, 0x02]);
        acia.advance(3 * DRAGON_FRAME_XTAL);

        let status = acia.read(STATUS);
        assert_ne!(status & status::OVERRUN, 0);
        assert_ne!(status & status::RDRF, 0);
        assert_eq!(acia.read(DATA), 0x02);
        assert_eq!(acia.peek(STATUS) & (status::OVERRUN | status::RDRF), 0);
    }

    #[test]
    fn receive_interrupt_sets_status_bit_seven_until_status_is_read() {
        let mut acia = dragon_acia();
        // DTR on, receiver interrupt enabled (bit 1 clear).
        acia.write(COMMAND, 0x09);
        acia.queue_received(&[0x33]);
        acia.advance(2 * DRAGON_FRAME_XTAL);

        assert!(acia.irq());
        let status = acia.read(STATUS);
        assert_eq!(
            status & (status::IRQ | status::RDRF),
            status::IRQ | status::RDRF
        );
        assert!(!acia.irq(), "reading status releases IRQ");
        assert_eq!(acia.peek(STATUS) & status::IRQ, 0);
    }

    #[test]
    fn disabling_the_receiver_interrupt_withdraws_a_pending_one() {
        let mut acia = dragon_acia();
        acia.write(COMMAND, 0x09);
        acia.queue_received(&[0x33]);
        acia.advance(2 * DRAGON_FRAME_XTAL);
        assert!(acia.irq());
        acia.write(COMMAND, 0x0B);
        assert!(!acia.irq());
    }

    #[test]
    fn transmit_interrupt_fires_when_the_data_register_empties() {
        let mut acia = dragon_acia();
        // DTR on, receiver interrupt off, transmitter control 01.
        acia.write(COMMAND, 0x07);
        assert!(acia.irq(), "enabling with TDRE set interrupts at once");
        acia.read(STATUS);
        acia.write(DATA, 0x41);
        assert!(!acia.irq());
        acia.advance(96);
        assert!(acia.irq(), "TDRE set again as the start bit goes out");
    }

    #[test]
    fn transmit_interrupt_needs_dtr() {
        let mut acia = dragon_acia();
        acia.write(COMMAND, 0x06);
        acia.write(DATA, 0x41);
        acia.advance(2 * DRAGON_FRAME_XTAL);
        assert!(!acia.irq());
        assert_eq!(acia.take_transmitted(), vec![0x41]);
    }

    #[test]
    fn modem_inputs_show_in_status_and_interrupt_while_dtr_is_on() {
        let mut acia = dragon_acia();
        acia.set_dcd(false);
        assert_ne!(acia.peek(STATUS) & status::DCD_HIGH, 0);
        assert!(!acia.irq(), "DTR off masks the change interrupt");

        acia.write(COMMAND, 0x0B);
        acia.set_dsr(false);
        assert_ne!(acia.peek(STATUS) & status::DSR_HIGH, 0);
        assert!(acia.irq());
        acia.write(STATUS, 0);
        assert!(!acia.irq(), "programmed reset clears DCD/DSR interrupts");
    }

    #[test]
    fn echo_mode_retransmits_received_characters() {
        let mut acia = dragon_acia();
        acia.write(COMMAND, 0x13);
        acia.queue_received(&[0x7E]);
        acia.advance(2 * DRAGON_FRAME_XTAL);
        assert_eq!(acia.take_transmitted(), vec![0x7E]);
    }

    #[test]
    fn external_clock_selection_stops_both_directions() {
        let mut acia = dragon_acia();
        acia.write(CONTROL, DRAGON_CONTROL & !control::BAUD_MASK);
        acia.write(COMMAND, 0x0B);
        acia.write(DATA, 0x41);
        acia.queue_received(&[0x42]);
        acia.advance(10 * DRAGON_FRAME_XTAL);
        assert!(acia.take_transmitted().is_empty());
        assert_eq!(acia.peek(STATUS) & status::RDRF, 0);
    }

    #[test]
    fn faster_rate_shortens_the_frame() {
        let mut acia = dragon_acia();
        acia.write(CONTROL, 0x9F); // 19200 baud
        acia.write(DATA, 0x41);
        acia.advance(6 + 11 * 16 * 6 - 1);
        assert!(acia.take_transmitted().is_empty());
        acia.advance(1);
        assert_eq!(acia.take_transmitted(), vec![0x41]);
    }
}
