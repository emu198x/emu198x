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
    #[serde(default)]
    drive_active: [bool; DRIVE_COUNT],
    /// Per-drive DATA-line fold. The 1541 (VIA `via1d1541`) folds the drive's
    /// DATA contribution as `~data ^ cpu_bus`; the 1581 (CIA `cia1581d`) folds
    /// it as `data | cpu_bus`. They share this bus, so each drive records which
    /// hardware fold it uses. `false` = 1541 (the default).
    #[serde(default)]
    drive_data_or_fold: [bool; DRIVE_COUNT],
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
            drive_active: [false; DRIVE_COUNT],
            drive_data_or_fold: [false; DRIVE_COUNT],
            drive_port: DRIVE_READ_DATA | DRIVE_READ_CLOCK | DRIVE_READ_ATN,
        };
        bus.recompute_ports();
        bus
    }

    /// Updates the C64-side IEC output lines from CIA2 Port A.
    pub fn write_cpu_port_a(&mut self, port_a: u8) {
        self.cpu_bus = ((port_a << 2) & CPU_READ_DATA)
            | ((port_a << 2) & CPU_READ_CLOCK)
            | ((port_a << 1) & CPU_WRITE_ATN);
        for index in 0..DRIVE_COUNT {
            if self.drive_active[index] {
                self.recompute_drive_bus_entry(index);
            }
        }
        self.recompute_ports();
    }

    /// Updates one 1541-style drive bus contribution from VIA1 Port B.
    pub fn write_drive_port_b(&mut self, drive_number: u8, port_b: u8) {
        self.write_drive_port_b_folded(drive_number, port_b, false);
    }

    /// Updates one 1581-style drive bus contribution from its 8520 CIA Port B.
    /// The 1581's DATA-line fold differs from the 1541's (VICE `cia1581d`), so
    /// this records the alternate fold for the ATN-acknowledge path.
    pub fn write_drive_port_b_1581(&mut self, drive_number: u8, port_b: u8) {
        self.write_drive_port_b_folded(drive_number, port_b, true);
    }

    fn write_drive_port_b_folded(&mut self, drive_number: u8, port_b: u8, or_fold: bool) {
        let Some(index) = Self::drive_index(drive_number) else {
            return;
        };

        let data = !port_b;
        self.drive_data[index] = data;
        self.drive_active[index] = true;
        self.drive_data_or_fold[index] = or_fold;
        self.recompute_drive_bus_entry(index);
        self.recompute_ports();
    }

    /// Current CPU-visible IEC input bits as read through CIA2 Port A.
    #[must_use]
    pub const fn cpu_port(&self) -> u8 {
        self.cpu_port
    }

    /// Current raw C64-side IEC bus contribution before drive inputs are folded in.
    #[must_use]
    pub const fn cpu_bus(&self) -> u8 {
        self.cpu_bus
    }

    /// Current drive-visible IEC input bits as read through VIA1 Port B.
    #[must_use]
    pub const fn drive_port(&self) -> u8 {
        self.drive_port
    }

    /// Current raw IEC contribution from one drive bus entry.
    #[must_use]
    pub fn drive_bus(&self, drive_number: u8) -> Option<u8> {
        Self::drive_index(drive_number).map(|index| self.drive_bus[index])
    }

    /// Current raw decoded IEC output bits for one drive.
    #[must_use]
    pub fn drive_data(&self, drive_number: u8) -> Option<u8> {
        Self::drive_index(drive_number).map(|index| self.drive_data[index])
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

    fn recompute_drive_bus_entry(&mut self, index: usize) {
        let data = self.drive_data[index];
        // The DATA-line middle term differs by drive hardware: the 1541 folds
        // `~data ^ cpu_bus`, the 1581 folds `data | cpu_bus` (VICE
        // `via1d1541` vs `cia1581d`). The ATN acknowledge depends on it.
        let data_mid = if self.drive_data_or_fold[index] {
            data | self.cpu_bus
        } else {
            (!data) ^ self.cpu_bus
        };
        self.drive_bus[index] =
            ((data << 3) & CPU_READ_CLOCK) | ((data << 6) & (data_mid << 3) & CPU_READ_DATA);
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

        bus.write_cpu_port_a(0xF7);

        assert_eq!(bus.drive_port() & DRIVE_READ_ATN, 0x00);
        assert_eq!(bus.drive_port() & DRIVE_READ_CLOCK, DRIVE_READ_CLOCK);
        assert_eq!(bus.drive_port() & DRIVE_READ_DATA, DRIVE_READ_DATA);
        assert!(!bus.drive_atn_high());
    }

    #[test]
    fn drive_data_low_pulls_cpu_input_low() {
        let mut bus = IecBus::new();

        bus.write_drive_port_b(8, 0xF7);

        assert_eq!(bus.cpu_port() & CPU_READ_DATA, 0x00);
        assert_eq!(bus.cpu_port() & CPU_READ_CLOCK, CPU_READ_CLOCK);
    }

    #[test]
    fn cpu_bus_updates_recompute_drive_data_fold() {
        let mut bus = IecBus::new();

        bus.write_cpu_port_a(0xF7);
        bus.write_drive_port_b(8, 0x05);
        let initial_drive_bus = bus.drive_bus(8).expect("drive-8 should exist");

        bus.write_cpu_port_a(0xFF);

        let updated_drive_bus = bus.drive_bus(8).expect("drive-8 should exist");
        assert_ne!(initial_drive_bus, updated_drive_bus);
    }
}
