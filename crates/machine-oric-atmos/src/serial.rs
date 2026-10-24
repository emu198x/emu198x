//! An optional 6551 ACIA serial card at `$031C-$031F`.
//!
//! The Oric has no serial port of its own. The Telestrat carried a 6551 on
//! its board, and add-on RS-232 cards for the Oric-1 and Atmos put one on
//! the expansion port. The address follows the Euphoric emulator's
//! convention, which Oric software written for those cards expects: data at
//! `$031C`, status `$031D`, command `$031E`, control `$031F` (#1303).
//! Oricutron uses the same default offset, `$031C`, and checks it ahead of
//! the VIA on every page-3 access, with the 6551's IRQ joining the CPU's
//! (`machine.c`, `6551.c`; consulted for facts only). No source in the
//! reference library gives the card's decode.
//!
//! The four registers sit inside the VIA's page. A card answering there has
//! to stop the VIA driving the bus for those addresses, so with the card
//! fitted `$031C-$031F` reach the ACIA and nothing else; the rest of page 3
//! still reaches the VIA and its 16-byte mirrors. The same four addresses
//! fall inside the Microdisc controller's registers, so the two cards cannot
//! share a machine. This crate models no Microdisc.
//!
//! The 6551 runs from its own 1.8432 MHz crystal, which does not divide the
//! 1 MHz CPU clock. Each CPU cycle advances it by the whole crystal ticks
//! owed and carries the remainder, so over a second it sees exactly
//! 1,843,200. Its IRQ output joins the VIA's on the 6502's IRQ line.
//!
//! The far end of the cable is the host: it queues bytes for the Oric to
//! receive and drains the bytes the Oric sent, the way the Dragon 64's
//! onboard port works.

use mos_acia_6551::{Acia6551, Acia6551Registers, XTAL_HZ};

use super::{CPU_CLOCK_HZ, OricAtmos};

/// Whether `addr` is one of the card's four registers.
pub(crate) fn is_acia_address(addr: u16) -> bool {
    (0x031C..=0x031F).contains(&addr)
}

impl OricAtmos {
    /// The fitted ACIA, when `addr` selects it.
    pub(crate) fn acia_at(&mut self, addr: u16) -> Option<&mut Acia6551> {
        if is_acia_address(addr) {
            self.acia.as_mut()
        } else {
            None
        }
    }

    /// Advance the ACIA's crystal by one CPU cycle's worth of ticks.
    pub(crate) fn advance_acia(&mut self) {
        let Some(acia) = &mut self.acia else {
            return;
        };
        let scaled = self.acia_xtal_remainder + u64::from(XTAL_HZ);
        let cpu_hz = u64::from(CPU_CLOCK_HZ);
        self.acia_xtal_remainder = scaled % cpu_hz;
        let xtal_ticks = u32::try_from(scaled / cpu_hz).unwrap_or(u32::MAX);
        acia.advance(xtal_ticks);
    }

    pub(crate) fn acia_irq(&self) -> bool {
        self.acia.as_ref().is_some_and(Acia6551::irq)
    }

    /// Fit or remove the 6551 serial card. Fitting a card that is already
    /// fitted keeps it as it is; fitting a new one powers it up reset.
    pub fn set_serial_card(&mut self, fitted: bool) {
        match (fitted, self.acia.is_some()) {
            (true, false) => {
                self.acia = Some(Acia6551::new());
                self.acia_xtal_remainder = 0;
            }
            (false, true) => self.acia = None,
            _ => {}
        }
    }

    /// Whether this machine has an RS-232 port: the optional ACIA card.
    #[must_use]
    pub fn has_serial_port(&self) -> bool {
        self.acia.is_some()
    }

    /// Host side of the RS-232 port: queue bytes for the device at the far
    /// end of the cable to send. Each arrives as a full frame at the ACIA's
    /// programmed rate and format, and only while the Oric holds `DTR`
    /// asserted. Returns `false`, dropping nothing into the machine, when no
    /// card is fitted.
    pub fn queue_serial_input(&mut self, bytes: &[u8]) -> bool {
        match &mut self.acia {
            Some(acia) => {
                acia.queue_received(bytes);
                true
            }
            None => false,
        }
    }

    /// Host side of the RS-232 port: bytes queued by
    /// [`Self::queue_serial_input`] that have not yet started on the line.
    #[must_use]
    pub fn serial_input_pending(&self) -> usize {
        self.acia.as_ref().map_or(0, Acia6551::pending_received)
    }

    /// Host side of the RS-232 port: drain the bytes the Oric has finished
    /// transmitting. Always empty without a card.
    pub fn drain_serial_output(&mut self) -> Vec<u8> {
        self.acia
            .as_mut()
            .map(Acia6551::take_transmitted)
            .unwrap_or_default()
    }

    /// Host side of the RS-232 port: drive the ACIA's modem-status inputs.
    /// `true` means asserted (the pin pulled low by a ready device), which
    /// is how a new card starts. Returns `false` when no card is fitted.
    pub fn set_serial_modem_inputs(&mut self, cts: bool, dsr: bool, dcd: bool) -> bool {
        match &mut self.acia {
            Some(acia) => {
                acia.set_cts(cts);
                acia.set_dsr(dsr);
                acia.set_dcd(dcd);
                true
            }
            None => false,
        }
    }

    /// Side-effect-free view of the card's ACIA registers.
    #[must_use]
    pub fn acia_registers(&self) -> Option<Acia6551Registers> {
        self.acia.as_ref().map(Acia6551::registers)
    }

    /// The card's programmed baud rate, or `None` when no card is fitted or
    /// it is set to the external clock.
    #[must_use]
    pub fn acia_baud_rate(&self) -> Option<f64> {
        self.acia.as_ref().and_then(Acia6551::baud_rate)
    }
}

#[cfg(test)]
mod tests {
    use crate::{OricAtmos, OricModel};

    /// A 16 KB ROM holding `program` at `$C000`, with every vector pointing
    /// there.
    fn rom_with(program: &[u8]) -> Vec<u8> {
        let mut rom = vec![0xEA; 0x4000];
        rom[..program.len()].copy_from_slice(program);
        for vector in [0x3FFA, 0x3FFC, 0x3FFE] {
            rom[vector] = 0x00;
            rom[vector + 1] = 0xC0;
        }
        rom
    }

    /// Program the card for 9600 baud 8N1, send "HI" polling TDRE, then
    /// wait on RDRF and store the received byte at `$0200`.
    const ECHO_PROGRAM: &[u8] = &[
        0x8D, 0x1D, 0x03, // STA $031D   ; programmed reset
        0xA9, 0x1E, //       LDA #$1E    ; 9600 baud, 8 bits, 1 stop, internal RxC
        0x8D, 0x1F, 0x03, // STA $031F   ; control
        0xA9, 0x0B, //       LDA #$0B    ; DTR on, RTS low, no IRQs
        0x8D, 0x1E, 0x03, // STA $031E   ; command
        0xAD, 0x1D, 0x03, // tx1: LDA $031D
        0x29, 0x10, //       AND #$10    ; TDRE
        0xF0, 0xF9, //       BEQ tx1
        0xA9, 0x48, //       LDA #'H'
        0x8D, 0x1C, 0x03, // STA $031C
        0xAD, 0x1D, 0x03, // tx2: LDA $031D
        0x29, 0x10, //       AND #$10
        0xF0, 0xF9, //       BEQ tx2
        0xA9, 0x49, //       LDA #'I'
        0x8D, 0x1C, 0x03, // STA $031C
        0xAD, 0x1D, 0x03, // rx: LDA $031D
        0x29, 0x08, //       AND #$08    ; RDRF
        0xF0, 0xF9, //       BEQ rx
        0xAD, 0x1C, 0x03, // LDA $031C
        0x8D, 0x00, 0x02, // STA $0200
        0x4C, 0x32, 0xC0, // done: JMP done
    ];

    fn fitted(program: &[u8]) -> OricAtmos {
        let mut sys = OricAtmos::new(rom_with(program), OricModel::Atmos);
        sys.set_serial_card(true);
        sys
    }

    #[test]
    fn a_6502_program_sends_and_receives_through_the_card() {
        let mut sys = fitted(ECHO_PROGRAM);
        assert!(sys.queue_serial_input(b"Z"));
        sys.run_frame();
        assert_eq!(sys.acia_baud_rate(), Some(9600.0));
        assert_eq!(sys.drain_serial_output(), b"HI");
        assert_eq!(sys.peek(0x0200), b'Z');
        assert_eq!(sys.serial_input_pending(), 0);
        assert_eq!(sys.cpu().regs.pc & 0xFFF0, 0xC030, "parked at done");
    }

    /// Two 10-bit frames at 9600 baud take about 2.08 ms; the second byte
    /// cannot have finished long before that, which pins the crystal
    /// conversion to the right order of magnitude.
    #[test]
    fn transmission_takes_the_line_time_9600_baud_allows() {
        let mut sys = fitted(ECHO_PROGRAM);
        let mut cycles = 0u64;
        let mut sent = Vec::new();
        while sent.len() < 2 && cycles < 20_000 {
            cycles += sys.step_instruction();
            sent.extend(sys.drain_serial_output());
        }
        assert_eq!(sent, b"HI");
        assert!((1_900..2_400).contains(&cycles), "{cycles} cycles");
    }

    /// The card's four registers leave the VIA. Without a card, `$031C`
    /// is the VIA's PCR seen through its 16-byte mirror.
    #[test]
    fn the_card_takes_031c_to_031f_from_the_via() {
        let mut bare = OricAtmos::new(rom_with(&[]), OricModel::Atmos);
        bare.mem_write(0x031C, 0xAA);
        assert_eq!(bare.mem_read(0x030C), 0xAA, "VIA PCR via its mirror");
        assert!(!bare.has_serial_port());
        assert!(!bare.queue_serial_input(b"x"));
        assert!(bare.acia_registers().is_none());

        let mut sys = fitted(&[]);
        sys.mem_write(0x030C, 0x00);
        sys.mem_write(0x031E, 0x0B); // ACIA command
        assert_eq!(sys.mem_read(0x030C), 0x00, "the VIA's PCR is untouched");
        assert_eq!(sys.acia_registers().map(|r| r.command), Some(0x0B));
        assert_eq!(sys.mem_read(0x031E), 0x0B);
        assert_eq!(sys.peek(0x031E), 0x0B);
        // The rest of the VIA's second mirror still answers.
        sys.mem_write(0x0313, 0x5A); // DDRA
        assert_eq!(sys.mem_read(0x0303), 0x5A);
    }

    /// With receive interrupts enabled, a received byte pulls the 6502's
    /// IRQ line low alongside the VIA's.
    #[test]
    fn the_card_interrupt_reaches_the_cpu() {
        let mut sys = fitted(&[0x4C, 0x00, 0xC0]); // JMP $C000
        sys.mem_write(0x031F, 0x1E);
        sys.mem_write(0x031E, 0x09); // DTR on, receive IRQ enabled
        sys.queue_serial_input(b"!");
        for _ in 0..2_000 {
            sys.step_instruction();
        }
        assert!(sys.acia_registers().is_some_and(|r| r.irq));
        assert!(sys.cpu.irq);
        assert_eq!(sys.mem_read(0x031C), b'!');
    }

    #[test]
    fn the_card_survives_a_snapshot() {
        let mut sys = fitted(ECHO_PROGRAM);
        sys.mem_write(0x031E, 0x0B);
        let bytes = postcard::to_allocvec(&sys).expect("encode snapshot");
        let restored: OricAtmos = postcard::from_bytes(&bytes).expect("decode snapshot");
        assert!(restored.has_serial_port());
        assert_eq!(restored.acia_registers().map(|r| r.command), Some(0x0B));
    }

    #[test]
    fn removing_the_card_returns_the_registers_to_the_via() {
        let mut sys = fitted(&[]);
        sys.set_serial_card(true);
        assert!(sys.has_serial_port());
        sys.set_serial_card(false);
        assert!(!sys.has_serial_port());
        assert!(!sys.set_serial_modem_inputs(true, true, true));
        sys.mem_write(0x031C, 0x55);
        assert_eq!(sys.mem_read(0x030C), 0x55);
    }
}
