//! Shared IEC serial-bus line state.
//!
//! This mirrors the line-level encoding used by VICE closely enough that the
//! C64 and 1541 board substrates can agree on the same DATA/CLOCK/ATN state
//! before higher-level IEC protocol handling exists.

use serde::{Deserialize, Serialize};

const CPU_WRITE_ATN: u8 = 0x10;
const CPU_READ_CLOCK: u8 = 0x40;
const CPU_READ_DATA: u8 = 0x80;

const DRIVE_READ_DATA: u8 = 0x01;
const DRIVE_READ_CLOCK: u8 = 0x04;
const DRIVE_READ_ATN: u8 = 0x80;

const DRIVE_COUNT: usize = 4;

/// Shared open-collector IEC bus state for the C64 and drives 8-11.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IecBus {
    cpu_bus: u8,
    cpu_port: u8,
    drive_bus: [u8; DRIVE_COUNT],
    drive_data: [u8; DRIVE_COUNT],
    drive_port: u8,
}

impl IecBus {
    /// Constructs one idle bus with all lines released high.
    #[must_use]
    pub fn new() -> Self {
        let mut bus = Self {
            cpu_bus: 0xFF,
            cpu_port: 0xFF,
            drive_bus: [0xFF; DRIVE_COUNT],
            drive_data: [0xFF; DRIVE_COUNT],
            drive_port: DRIVE_READ_DATA | DRIVE_READ_CLOCK | DRIVE_READ_ATN,
        };
        bus.recompute_ports();
        bus
    }

    /// Updates the C64-side IEC output lines from CIA2 Port A.
    pub fn write_cpu_port_a(&mut self, port_a: u8) {
        let data = !port_a;
        self.cpu_bus = ((data << 2) & CPU_READ_DATA)
            | ((data << 2) & CPU_READ_CLOCK)
            | ((data << 1) & CPU_WRITE_ATN);
        self.recompute_ports();
    }

    /// Updates one 1541-style drive bus contribution from VIA1 Port B.
    pub fn write_drive_port_b(&mut self, drive_number: u8, port_b: u8) {
        let Some(index) = Self::drive_index(drive_number) else {
            return;
        };

        let data = !port_b;
        self.drive_data[index] = data;
        self.drive_bus[index] = ((data << 3) & CPU_READ_CLOCK)
            | ((data << 6) & (((!data) ^ self.cpu_bus) << 3) & CPU_READ_DATA);
        self.recompute_ports();
    }

    /// Current CPU-visible IEC input bits as read through CIA2 Port A.
    #[must_use]
    pub const fn cpu_port(&self) -> u8 {
        self.cpu_port
    }

    /// Current drive-visible IEC input bits as read through VIA1 Port B.
    #[must_use]
    pub const fn drive_port(&self) -> u8 {
        self.drive_port
    }

    /// Returns `true` when the ATN line is released high on the drive side.
    #[must_use]
    pub const fn drive_atn_high(&self) -> bool {
        self.drive_port & DRIVE_READ_ATN != 0
    }

    fn recompute_ports(&mut self) {
        self.cpu_port = self.cpu_bus;
        for bus in self.drive_bus {
            self.cpu_port &= bus;
        }

        self.drive_port = ((self.cpu_port >> 4) & DRIVE_READ_CLOCK)
            | (self.cpu_port >> 7)
            | ((self.cpu_bus << 3) & DRIVE_READ_ATN);
    }

    fn drive_index(drive_number: u8) -> Option<usize> {
        if (8..=11).contains(&drive_number) {
            Some((drive_number - 8) as usize)
        } else {
            None
        }
    }
}

impl Default for IecBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CPU_READ_CLOCK, CPU_READ_DATA, DRIVE_READ_ATN, DRIVE_READ_CLOCK, DRIVE_READ_DATA, IecBus,
    };

    #[test]
    fn idle_bus_keeps_all_lines_high() {
        let bus = IecBus::new();

        assert_eq!(
            bus.cpu_port() & (CPU_READ_DATA | CPU_READ_CLOCK),
            CPU_READ_DATA | CPU_READ_CLOCK
        );
        assert_eq!(
            bus.drive_port() & (DRIVE_READ_DATA | DRIVE_READ_CLOCK | DRIVE_READ_ATN),
            DRIVE_READ_DATA | DRIVE_READ_CLOCK | DRIVE_READ_ATN
        );
        assert!(bus.drive_atn_high());
    }

    #[test]
    fn cpu_can_pull_drive_atn_low() {
        let mut bus = IecBus::new();

        bus.write_cpu_port_a(0xEF);

        assert_eq!(bus.drive_port() & DRIVE_READ_ATN, 0x00);
        assert_eq!(bus.drive_port() & DRIVE_READ_CLOCK, DRIVE_READ_CLOCK);
        assert_eq!(bus.drive_port() & DRIVE_READ_DATA, 0x00);
        assert!(!bus.drive_atn_high());
    }

    #[test]
    fn drive_data_low_pulls_cpu_input_low() {
        let mut bus = IecBus::new();

        bus.write_drive_port_b(8, 0xF7);

        assert_eq!(bus.cpu_port() & CPU_READ_DATA, 0x00);
        assert_eq!(bus.cpu_port() & CPU_READ_CLOCK, CPU_READ_CLOCK);
    }
}
