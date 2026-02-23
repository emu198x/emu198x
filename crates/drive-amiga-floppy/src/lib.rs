//! Amiga floppy drive mechanism emulator.
//!
//! Emulates the physical drive: head positioning, motor control, disk
//! change detection, and MFM track encoding. Control signals come from
//! CIA-B port B; status signals feed back to CIA-A port A.

pub mod mfm;

use format_adf::Adf;
use mfm::encode_mfm_track;

/// E-clock ticks for motor spin-up (~500ms at 709 kHz).
const MOTOR_SPINUP_TICKS: u32 = 350_000;

/// Drive status bits for CIA-A PRA (active-low: 0 = asserted).
pub struct DriveStatus {
    /// PA2: /DSKCHANGE — low when disk has been removed since last step.
    pub disk_change: bool,
    /// PA3: /DSKPROT — low when disk is write-protected.
    pub write_protect: bool,
    /// PA4: /DSKTRACK0 — low when head is at cylinder 0.
    pub track0: bool,
    /// PA5: /DSKRDY — low when motor is at speed.
    pub ready: bool,
}

pub struct AmigaFloppyDrive {
    disk: Option<Adf>,
    cylinder: u32,
    head: u32,
    motor_on: bool,
    motor_spinning: bool,
    spin_timer: u32,
    selected: bool,
    disk_changed: bool,
    prev_step: bool,
}

impl AmigaFloppyDrive {
    pub fn new() -> Self {
        Self {
            disk: None,
            cylinder: 0,
            head: 0,
            motor_on: false,
            motor_spinning: false,
            spin_timer: 0,
            selected: false,
            disk_changed: true, // No disk at power-on
            prev_step: true,    // Active-low: idle = high
        }
    }

    pub fn insert_disk(&mut self, adf: Adf) {
        self.disk = Some(adf);
        self.disk_changed = false;
    }

    pub fn eject_disk(&mut self) {
        self.disk = None;
        self.disk_changed = true;
    }

    /// Update control signals from CIA-B PRB.
    /// All active-low: the boolean parameters are true when the signal
    /// is asserted (pin driven low).
    pub fn update_control(
        &mut self,
        step: bool,
        dir_inward: bool,
        side_upper: bool,
        sel: bool,
        motor: bool,
    ) {
        // Drive select latches motor state (active-low select)
        if sel {
            self.selected = true;
            self.motor_on = motor;
            if motor && !self.motor_spinning {
                self.spin_timer = 0;
            }
            if !motor {
                self.motor_spinning = false;
                self.spin_timer = 0;
            }
        } else {
            self.selected = false;
        }

        // Head side: 0 = upper (head 1), 1 = lower (head 0)
        // The parameter is already decoded: side_upper = true means DSKSIDE* asserted (low)
        self.head = if side_upper { 1 } else { 0 };

        // Step on falling edge (prev was high/deasserted, now low/asserted)
        let step_edge = step && !self.prev_step;
        self.prev_step = step;

        if step_edge {
            if dir_inward {
                if self.cylinder < 79 {
                    self.cylinder += 1;
                }
            } else if self.cylinder > 0 {
                self.cylinder -= 1;
            }
            // Any step pulse clears DSKCHANGE when a disk is present
            if self.disk.is_some() {
                self.disk_changed = false;
            }
        }
    }

    /// Advance motor spin-up timer. Call at E-clock rate.
    pub fn tick(&mut self) {
        if self.motor_on && !self.motor_spinning {
            self.spin_timer += 1;
            if self.spin_timer >= MOTOR_SPINUP_TICKS {
                self.motor_spinning = true;
            }
        }
    }

    /// Current drive status for CIA-A PRA input.
    /// All values are active-low booleans (true = signal asserted = pin low).
    pub fn status(&self) -> DriveStatus {
        DriveStatus {
            disk_change: self.disk_changed,
            write_protect: false, // Not write-protected
            track0: self.cylinder == 0,
            ready: self.motor_spinning,
        }
    }

    /// Encode the current track as raw MFM data. Returns `None` if no disk.
    pub fn encode_mfm_track(&self) -> Option<Vec<u8>> {
        let adf = self.disk.as_ref()?;
        let track_num = (self.cylinder * 2 + self.head) as u8;
        let sectors = adf.read_track_sectors(self.cylinder, self.head);
        Some(encode_mfm_track(
            sectors,
            track_num,
            adf.sectors_per_track(),
        ))
    }

    pub fn has_disk(&self) -> bool {
        self.disk.is_some()
    }

    pub fn cylinder(&self) -> u32 {
        self.cylinder
    }

    pub fn head(&self) -> u32 {
        self.head
    }

    pub fn motor_on(&self) -> bool {
        self.motor_on
    }
}

impl Default for AmigaFloppyDrive {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_toward_center() {
        let mut drive = AmigaFloppyDrive::new();
        assert_eq!(drive.cylinder(), 0);

        // Select drive, motor on, direction inward
        drive.update_control(false, true, false, true, true);
        // Step pulse: deasserted -> asserted (falling edge)
        drive.update_control(true, true, false, true, true);
        assert_eq!(drive.cylinder(), 1);
    }

    #[test]
    fn step_toward_edge() {
        let mut drive = AmigaFloppyDrive::new();
        // First move inward
        drive.update_control(false, true, false, true, true);
        drive.update_control(true, true, false, true, true);
        drive.update_control(false, true, false, true, true);
        drive.update_control(true, true, false, true, true);
        assert_eq!(drive.cylinder(), 2);

        // Now step toward edge (dir_inward = false)
        drive.update_control(false, false, false, true, true);
        drive.update_control(true, false, false, true, true);
        assert_eq!(drive.cylinder(), 1);
    }

    #[test]
    fn no_step_below_zero() {
        let mut drive = AmigaFloppyDrive::new();
        drive.update_control(false, false, false, true, true);
        drive.update_control(true, false, false, true, true);
        assert_eq!(drive.cylinder(), 0);
    }

    #[test]
    fn no_step_above_79() {
        let mut drive = AmigaFloppyDrive::new();
        // Move to track 79
        for _ in 0..80 {
            drive.update_control(false, true, false, true, true);
            drive.update_control(true, true, false, true, true);
        }
        assert_eq!(drive.cylinder(), 79);
    }

    #[test]
    fn track0_status() {
        let drive = AmigaFloppyDrive::new();
        assert!(drive.status().track0);
    }

    #[test]
    fn motor_spinup() {
        let mut drive = AmigaFloppyDrive::new();
        drive.update_control(false, false, false, true, true);
        assert!(!drive.status().ready);

        for _ in 0..MOTOR_SPINUP_TICKS {
            drive.tick();
        }
        assert!(drive.status().ready);
    }

    #[test]
    fn disk_change_cleared_by_step() {
        let mut drive = AmigaFloppyDrive::new();
        let adf = Adf::from_bytes(vec![0; format_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf);
        assert!(!drive.status().disk_change);

        drive.eject_disk();
        assert!(drive.status().disk_change);

        // Insert new disk — change still set until step
        let adf2 = Adf::from_bytes(vec![0; format_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf2);

        // Step clears change flag
        drive.update_control(false, true, false, true, true);
        drive.update_control(true, true, false, true, true);
        assert!(!drive.status().disk_change);
    }

    #[test]
    fn encode_track_returns_data_with_disk() {
        let mut drive = AmigaFloppyDrive::new();
        let adf = Adf::from_bytes(vec![0; format_adf::ADF_SIZE_DD]).expect("valid");
        drive.insert_disk(adf);

        let mfm = drive.encode_mfm_track();
        assert!(mfm.is_some());
        assert_eq!(mfm.expect("some").len(), mfm::MFM_TRACK_BYTES);
    }

    #[test]
    fn encode_track_returns_none_without_disk() {
        let drive = AmigaFloppyDrive::new();
        assert!(drive.encode_mfm_track().is_none());
    }

    #[test]
    fn head_select() {
        let mut drive = AmigaFloppyDrive::new();
        // side_upper = true means upper head (head 1)
        drive.update_control(false, false, true, true, true);
        assert_eq!(drive.head(), 1);
        // side_upper = false means lower head (head 0)
        drive.update_control(false, false, false, true, true);
        assert_eq!(drive.head(), 0);
    }
}
