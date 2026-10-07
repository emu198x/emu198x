//! The user port's CIA lines, as pins a host-side device reads and drives.
//!
//! Pinout from the Commodore 64 Programmer's Reference Guide ("The User
//! Port", p. 359): pin 4 CNT1 and pin 5 SP1 from CIA #1, pin 6 CNT2, pin 7 SP2
//! and pin 8 PC2 from CIA #2, pin B FLAG2, pins C–L PB0–PB7 and pin M PA2.
//! PA2 and PB0 keep their original accessors in `machine.rs`
//! ([`C64::user_port_pa2`], [`C64::user_port_pb0`],
//! [`C64::set_user_port_pb0`]); the rest live here.
//!
//! A device follows the same per-cycle pattern as the ESP-AT modem: before
//! each `phi2` tick it reads the C64's levels and drives its own. A `set_*`
//! level is the device's drive, `true` meaning released; a getter returns
//! the level on the wire. CNT and SP are open drain at the 6526 (datasheet,
//! "Serial Port"), so the wire is the AND of both drives, and a CIA counting
//! or shifting on CNT sees exactly that.

use super::C64;

impl C64 {
    /// The level on user-port PB0–PB7 (pins C–L), after DDRB. Output bits
    /// show CIA #2's latch; input bits show the external drive.
    #[must_use]
    pub fn user_port_pb(&self) -> u8 {
        self.cia2.pb
    }

    /// Drive user-port PB0–PB7 (pins C–L); `0xFF` releases them to the
    /// pull-ups. Lines DDRB makes outputs ignore the drive, as at the pin.
    /// [`C64::set_user_port_pb0`] drives bit 0 alone.
    pub fn set_user_port_pb(&mut self, levels: u8) {
        self.cia2.pb_in = levels;
    }

    /// Drive user-port PA2 (pin M) while DDRA makes it an input.
    pub fn set_user_port_pa2(&mut self, high: bool) {
        if high {
            self.cia2.pa_in |= 0x04;
        } else {
            self.cia2.pa_in &= !0x04;
        }
    }

    /// Drive /FLAG2 (pin B). A falling edge sets CIA #2's FLAG interrupt
    /// bit (`$DD0D` bit 4), which reaches the CPU as an NMI when enabled.
    pub fn set_user_port_flag2(&mut self, high: bool) {
        self.cia2.flag = high;
    }

    /// The level on /PC2 (pin 8): low for the one cycle after CIA #2's
    /// port B is read or written, the "data ready / data taken" strobe.
    #[must_use]
    pub fn user_port_pc2(&self) -> bool {
        self.cia2.pc
    }

    /// The level on CNT1 (pin 4), CIA #1's serial clock.
    #[must_use]
    pub fn user_port_cnt1(&self) -> bool {
        self.cia1.cnt_line()
    }

    /// Drive CNT1 (pin 4).
    pub fn set_user_port_cnt1(&mut self, high: bool) {
        self.cia1.cnt_in = high;
    }

    /// The level on SP1 (pin 5), CIA #1's serial data.
    #[must_use]
    pub fn user_port_sp1(&self) -> bool {
        self.cia1.sp_line()
    }

    /// Drive SP1 (pin 5).
    pub fn set_user_port_sp1(&mut self, high: bool) {
        self.cia1.sp_in = high;
    }

    /// The level on CNT2 (pin 6), CIA #2's serial clock.
    #[must_use]
    pub fn user_port_cnt2(&self) -> bool {
        self.cia2.cnt_line()
    }

    /// Drive CNT2 (pin 6).
    pub fn set_user_port_cnt2(&mut self, high: bool) {
        self.cia2.cnt_in = high;
    }

    /// The level on SP2 (pin 7), CIA #2's serial data.
    #[must_use]
    pub fn user_port_sp2(&self) -> bool {
        self.cia2.sp_line()
    }

    /// Drive SP2 (pin 7).
    pub fn set_user_port_sp2(&mut self, high: bool) {
        self.cia2.sp_in = high;
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{C64Config, C64Model};
    use crate::machine::C64;

    fn stub_machine() -> C64 {
        let mut kernal = [0xEA; 0x2000];
        kernal[0x1FFC] = 0x00;
        kernal[0x1FFD] = 0xE0;
        C64::new(C64Config {
            model: C64Model::PalBreadbin,
            kernal_rom: &kernal,
            basic_rom: &[0xBB; 0x2000],
            character_rom: &[0xCC; 0x1000],
        })
        .expect("stub ROM sizes should be valid")
    }

    /// A user-port loopback plug: SP1 to SP2 and CNT1 to CNT2, as fast
    /// loaders and C64-to-C64 networking cables wire the two serial ports.
    /// One `phi2` cycle of the plug, then one of the machine.
    fn tick_with_loopback(machine: &mut C64) {
        let sp = machine.user_port_sp1();
        let cnt = machine.user_port_cnt1();
        machine.set_user_port_sp2(sp);
        machine.set_user_port_cnt2(cnt);
        machine.tick();
    }

    /// CIA #1 sends with its serial port as an output, Timer A at `latch`
    /// as the baud generator; CIA #2 receives with its port as an input.
    fn set_up_loopback(machine: &mut C64, latch: u8) {
        machine.cpu_write(0xDC0D, 0x7F); // CIA #1: no interrupts to the CPU
        machine.cpu_write(0xDD0D, 0x7F); // CIA #2: no NMIs
        machine.cpu_read(0xDC0D);
        machine.cpu_read(0xDD0D);
        machine.cpu_write(0xDD0E, 0x00); // CIA #2: SP input, Timer A stopped
        machine.cpu_write(0xDC04, latch);
        machine.cpu_write(0xDC05, 0x00);
        machine.cpu_write(0xDC0E, 0x51); // CIA #1: SP output, force load, start
        for _ in 0..8 {
            tick_with_loopback(machine);
        }
    }

    /// Run until CIA #2's SDR flag (`$DD0D` bit 3) appears, reading the ICR
    /// as a receive loop would; returns the cycles taken.
    fn wait_for_received_byte(machine: &mut C64, limit: u32) -> Option<u32> {
        for cycle in 0..limit {
            tick_with_loopback(machine);
            if machine.cia2().icr_status() & 0x08 != 0 {
                machine.cpu_read(0xDD0D);
                return Some(cycle);
            }
        }
        None
    }

    #[test]
    fn a_byte_sent_on_sp1_arrives_in_cia2s_sdr() {
        let mut machine = stub_machine();
        set_up_loopback(&mut machine, 4);

        machine.cpu_write(0xDC0C, 0xA5);
        assert!(
            wait_for_received_byte(&mut machine, 400).is_some(),
            "CIA #2 never raised its SDR interrupt"
        );
        assert_eq!(machine.cpu_read(0xDD0C), 0xA5);
    }

    #[test]
    fn back_to_back_bytes_arrive_in_order() {
        let mut machine = stub_machine();
        set_up_loopback(&mut machine, 4);

        // The second byte waits in CIA #1's SDR behind the first, so the
        // transfer runs on without a gap (datasheet: "If the microprocessor
        // stays one byte ahead of the shift register, transmission will be
        // continuous").
        let mut received = Vec::new();
        machine.cpu_write(0xDC0C, 0x3C);
        for _ in 0..8 {
            tick_with_loopback(&mut machine);
        }
        machine.cpu_write(0xDC0C, 0xC3);
        for _ in 0..2 {
            wait_for_received_byte(&mut machine, 400).expect("byte should arrive");
            received.push(machine.cpu_read(0xDD0C));
        }
        assert_eq!(received, vec![0x3C, 0xC3]);
    }

    #[test]
    fn every_byte_value_survives_the_loopback() {
        let mut machine = stub_machine();
        set_up_loopback(&mut machine, 2);
        for byte in 0..=0xFFu8 {
            machine.cpu_write(0xDC0C, byte);
            wait_for_received_byte(&mut machine, 400).expect("byte should arrive");
            assert_eq!(machine.cpu_read(0xDD0C), byte);
        }
    }

    #[test]
    fn idle_serial_lines_float_high() {
        let mut machine = stub_machine();
        machine.tick();
        assert!(machine.user_port_cnt1());
        assert!(machine.user_port_sp1());
        assert!(machine.user_port_cnt2());
        assert!(machine.user_port_sp2());
        assert!(machine.user_port_pc2());
    }

    #[test]
    fn pc2_strobes_low_for_one_cycle_after_a_port_b_access() {
        let mut machine = stub_machine();
        machine.tick();
        machine.cpu_write(0xDD01, 0x55);
        assert!(machine.user_port_pc2(), "the strobe follows the access");
        machine.tick();
        assert!(
            !machine.user_port_pc2(),
            "low for the cycle after the write"
        );
        machine.tick();
        assert!(machine.user_port_pc2(), "and only that cycle");

        machine.cpu_read(0xDD01);
        machine.tick();
        assert!(!machine.user_port_pc2(), "a read strobes it too");
    }

    #[test]
    fn a_falling_flag2_raises_cia2s_flag_interrupt() {
        let mut machine = stub_machine();
        machine.cpu_write(0xDD0D, 0x90); // enable the /FLAG2 interrupt
        machine.set_user_port_flag2(true);
        machine.tick();
        machine.set_user_port_flag2(false);
        machine.tick();
        machine.tick();
        assert!(
            machine.cia2().irq,
            "/FLAG2 should reach CIA #2's /IRQ (NMI)"
        );
        assert_eq!(machine.cpu_read(0xDD0D) & 0x10, 0x10);
    }

    #[test]
    fn pb_drives_every_input_line_and_outputs_win() {
        let mut machine = stub_machine();
        machine.cpu_write(0xDD03, 0x0F); // PB0-3 outputs, PB4-7 inputs
        machine.cpu_write(0xDD01, 0x05);
        machine.set_user_port_pb(0xA0);
        machine.tick();
        assert_eq!(machine.user_port_pb(), 0xA5);
        assert_eq!(machine.cpu_read(0xDD01), 0xA5);
    }

    #[test]
    fn pa2_reads_the_external_level_as_an_input() {
        let mut machine = stub_machine();
        machine.cpu_write(0xDD02, 0x03); // PA2 an input
        machine.set_user_port_pa2(false);
        machine.tick();
        assert_eq!(machine.cpu_read(0xDD00) & 0x04, 0);
        machine.set_user_port_pa2(true);
        machine.tick();
        assert_eq!(machine.cpu_read(0xDD00) & 0x04, 0x04);
    }

    #[test]
    fn a_device_clocking_cnt2_drives_cia2s_timer_a() {
        // Timer A counting CNT (CRA bit 5) is how a receiver counts external
        // clock pulses; nothing on a bare user port moves it.
        let mut machine = stub_machine();
        machine.cpu_write(0xDD04, 0x10);
        machine.cpu_write(0xDD05, 0x00);
        machine.cpu_write(0xDD0E, 0x31); // count CNT, force load, start
        for _ in 0..20 {
            machine.tick();
        }
        assert_eq!(machine.cia2().timer_a(), 0x10, "an idle CNT counts nothing");

        for _ in 0..5 {
            machine.set_user_port_cnt2(false);
            machine.tick();
            machine.tick();
            machine.set_user_port_cnt2(true);
            machine.tick();
            machine.tick();
        }
        for _ in 0..6 {
            machine.tick();
        }
        assert_eq!(
            machine.cia2().timer_a(),
            0x10 - 5,
            "one count per rising edge"
        );
    }
}
