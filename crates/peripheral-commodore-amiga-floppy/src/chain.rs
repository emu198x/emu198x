//! The Amiga's floppy drives as one chain: the internal drive, DF0, and
//! up to three external drives, DF1 to DF3.
//!
//! Amiga Hardware Reference Manual (1989), CIA-B port B: PB3 to PB6 are
//! /DSKSEL0 to /DSKSEL3, one select line per drive, all active low. PB7
//! /DSKMOTOR, PB2 /DSKSIDE, PB1 DSKDIREC and PB0 /DSKSTEP are shared by
//! every drive. Appendix E (external disk connector): each drive clocks
//! the motor line into its own flip-flop when it is selected; /DSKRDY,
//! /DSKPROT, /DSKTRACK0 and /DSKCHANGE are driven by the selected drive;
//! and at most three external drives can be fitted.

use serde::{Deserialize, Serialize};

use crate::{AmigaFloppyDrive, DriveStatus};

/// Drive positions on the disk port: DF0 plus three external drives.
pub const MAX_DRIVES: usize = 4;

/// DF0 (always fitted) and DF1 to DF3 (each fitted or absent), wired to
/// CIA-B port B for control and CIA-A port A for status.
#[derive(Clone, Serialize, Deserialize)]
pub struct AmigaFloppyChain {
    df0: AmigaFloppyDrive,
    /// DF1 to DF3. `None` is an empty position: nothing answers its
    /// select line.
    external: [Option<AmigaFloppyDrive>; MAX_DRIVES - 1],
    /// Select lines asserted by the last port B write; bit `n` is
    /// /DSKSELn.
    selects: u8,
}

impl AmigaFloppyChain {
    /// A one-drive Amiga: DF0 only.
    #[must_use]
    pub fn new() -> Self {
        Self {
            df0: AmigaFloppyDrive::new(),
            external: [None, None, None],
            selects: 0,
        }
    }

    /// Fit a drive at `unit`. DF0 is always fitted; fitting a drive that
    /// is already there leaves it, and any disk in it, as it is.
    ///
    /// Returns `false` for a unit beyond DF3.
    pub fn fit(&mut self, unit: usize) -> bool {
        match unit {
            0 => true,
            1..MAX_DRIVES => {
                self.external[unit - 1].get_or_insert_with(AmigaFloppyDrive::external);
                true
            }
            _ => false,
        }
    }

    /// Whether a drive is fitted at `unit`.
    #[must_use]
    pub fn is_fitted(&self, unit: usize) -> bool {
        self.drive(unit).is_some()
    }

    /// The drive at `unit`, or `None` when that position is empty.
    #[must_use]
    pub fn drive(&self, unit: usize) -> Option<&AmigaFloppyDrive> {
        match unit {
            0 => Some(&self.df0),
            1..MAX_DRIVES => self.external[unit - 1].as_ref(),
            _ => None,
        }
    }

    /// Mutable access to the drive at `unit`, or `None` when that
    /// position is empty.
    pub fn drive_mut(&mut self, unit: usize) -> Option<&mut AmigaFloppyDrive> {
        match unit {
            0 => Some(&mut self.df0),
            1..MAX_DRIVES => self.external[unit - 1].as_mut(),
            _ => None,
        }
    }

    /// The internal drive.
    #[must_use]
    pub fn df0(&self) -> &AmigaFloppyDrive {
        &self.df0
    }

    /// Mutable access to the internal drive.
    pub fn df0_mut(&mut self) -> &mut AmigaFloppyDrive {
        &mut self.df0
    }

    /// Units with a drive fitted, in order.
    pub fn fitted_units(&self) -> impl Iterator<Item = usize> + '_ {
        (0..MAX_DRIVES).filter(|&unit| self.is_fitted(unit))
    }

    /// Apply a CIA-B port B output byte. Every fitted drive sees the
    /// shared step, direction, side and motor lines; each sees its own
    /// select line.
    pub fn update_control(&mut self, prb: u8) {
        let step = prb & 0x01 == 0;
        let dir_inward = prb & 0x02 == 0;
        let side_upper = prb & 0x04 == 0;
        let motor = prb & 0x80 == 0;
        self.selects = !(prb >> 3) & 0x0F;
        for unit in 0..MAX_DRIVES {
            let selected = self.selects & (1 << unit) != 0;
            if let Some(drive) = self.drive_mut(unit) {
                drive.update_control(step, dir_inward, side_upper, selected, motor);
            }
        }
    }

    /// The four status lines CIA-A port A reads, as active-low booleans
    /// (`true` = pin low).
    ///
    /// Each selected drive pulls the lines it asserts; an empty position
    /// leaves them released. With no select line asserted, the lines
    /// carry DF0's status, as the one-drive machine always has.
    #[must_use]
    pub fn status(&self) -> DriveStatus {
        let mut lines = DriveStatus {
            disk_change: false,
            write_protect: false,
            track0: false,
            ready: false,
        };
        for unit in self.fitted_units() {
            if !self.drives_status_lines(unit) {
                continue;
            }
            let Some(drive) = self.drive(unit) else {
                continue;
            };
            let status = drive.status();
            lines.disk_change |= status.disk_change;
            lines.write_protect |= status.write_protect;
            lines.track0 |= status.track0;
            lines.ready |= status.ready;
        }
        lines
    }

    fn drives_status_lines(&self, unit: usize) -> bool {
        let selected = self.selects & (1 << unit) != 0;
        selected || (unit == 0 && self.selects == 0)
    }

    /// The drive disk DMA reads from and writes to: the lowest-numbered
    /// selected drive, or DF0 when no fitted drive is selected.
    #[must_use]
    pub fn dma_unit(&self) -> usize {
        self.fitted_units()
            .find(|&unit| self.selects & (1 << unit) != 0)
            .unwrap_or(0)
    }

    /// The drive disk DMA reads from and writes to.
    #[must_use]
    pub fn dma_drive(&self) -> &AmigaFloppyDrive {
        self.drive(self.dma_unit()).unwrap_or(&self.df0)
    }

    /// Mutable access to the drive disk DMA reads from and writes to.
    pub fn dma_drive_mut(&mut self) -> &mut AmigaFloppyDrive {
        let unit = self.dma_unit();
        match self.external.get_mut(unit.wrapping_sub(1)) {
            Some(Some(drive)) => drive,
            _ => &mut self.df0,
        }
    }

    /// Whether the DMA drive can supply read data now.
    #[must_use]
    pub fn read_data_available(&self) -> bool {
        self.dma_drive().read_data_available()
    }

    /// Advance every fitted drive by one E-clock tick. Returns `true`
    /// when any of them emits an index pulse.
    pub fn tick(&mut self, e_clock_hz: u64) -> bool {
        let mut index = false;
        for unit in 0..MAX_DRIVES {
            if let Some(drive) = self.drive_mut(unit) {
                index |= drive.tick(e_clock_hz);
            }
        }
        index
    }
}

impl Default for AmigaFloppyChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Adf, DD, STANDARD_DRIVE_ID};

    /// A port B byte with the given drives selected and the shared lines
    /// set. Every line is active low except DSKDIREC, where 0 is inward.
    fn prb(selected: &[usize], motor_on: bool, step: bool, inward: bool) -> u8 {
        let mut value = 0xFF;
        for unit in selected {
            value &= !(1 << (3 + unit));
        }
        if motor_on {
            value &= !0x80;
        }
        if step {
            value &= !0x01;
        }
        if inward {
            value &= !0x02;
        }
        value
    }

    /// The HRM Appendix E identification sequence: motor on then off,
    /// then 32 select pulses, reading /DSKRDY while selected. The stream
    /// is active low, so a low pin is a 1 bit.
    fn read_id(chain: &mut AmigaFloppyChain, unit: usize) -> u32 {
        chain.update_control(prb(&[], true, false, false));
        chain.update_control(prb(&[unit], true, false, false));
        chain.update_control(prb(&[], true, false, false));
        chain.update_control(prb(&[], false, false, false));
        chain.update_control(prb(&[unit], false, false, false));
        chain.update_control(prb(&[], false, false, false));
        let mut id = 0;
        for _ in 0..32 {
            chain.update_control(prb(&[unit], false, false, false));
            id = (id << 1) | u32::from(chain.status().ready);
            chain.update_control(prb(&[], false, false, false));
        }
        id
    }

    #[test]
    fn a_fitted_external_drive_identifies_as_a_standard_drive() {
        let mut chain = AmigaFloppyChain::new();
        assert!(chain.fit(1));
        assert_eq!(read_id(&mut chain, 1), STANDARD_DRIVE_ID);
    }

    #[test]
    fn an_empty_position_identifies_as_no_drive() {
        let mut chain = AmigaFloppyChain::new();
        chain.fit(1);
        for unit in [2, 3] {
            assert!(!chain.is_fitted(unit));
            assert_eq!(read_id(&mut chain, unit), 0, "DF{unit}");
        }
    }

    #[test]
    fn an_empty_position_leaves_every_status_line_released() {
        let mut chain = AmigaFloppyChain::new();
        // DF0 at track 0 with no disk asserts /DSKTRACK0 and /DSKCHANGE.
        chain.update_control(prb(&[0], false, false, false));
        assert!(chain.status().track0 && chain.status().disk_change);
        chain.update_control(prb(&[2], false, false, false));
        let status = chain.status();
        assert!(!status.disk_change && !status.write_protect);
        assert!(!status.track0 && !status.ready);
    }

    #[test]
    fn each_drive_latches_the_motor_line_only_while_selected() {
        let mut chain = AmigaFloppyChain::new();
        chain.fit(1);
        chain.update_control(prb(&[0], true, false, false));
        chain.update_control(prb(&[], true, false, false));
        chain.update_control(prb(&[1], false, false, false));
        chain.update_control(prb(&[], false, false, false));
        assert!(chain.drive(0).is_some_and(AmigaFloppyDrive::motor_on));
        assert!(!chain.drive(1).is_some_and(AmigaFloppyDrive::motor_on));
    }

    #[test]
    fn a_step_pulse_moves_only_the_selected_drive() {
        let mut chain = AmigaFloppyChain::new();
        chain.fit(1);
        chain.update_control(prb(&[1], true, false, true));
        chain.update_control(prb(&[1], true, true, true));
        chain.update_control(prb(&[1], true, false, true));
        assert_eq!(chain.drive(1).map(AmigaFloppyDrive::cylinder), Some(1));
        assert_eq!(chain.df0().cylinder(), 0);
    }

    #[test]
    fn the_selected_drive_drives_the_status_lines() {
        let mut chain = AmigaFloppyChain::new();
        chain.fit(1);
        // Step DF0 off track 0; DF1 stays there.
        chain.update_control(prb(&[0], true, false, true));
        chain.update_control(prb(&[0], true, true, true));
        chain.update_control(prb(&[0], true, false, true));
        assert!(!chain.status().track0);
        chain.update_control(prb(&[1], true, false, true));
        assert!(chain.status().track0);
    }

    #[test]
    fn disk_dma_follows_the_selected_drive() {
        let mut chain = AmigaFloppyChain::new();
        chain.fit(1);
        assert_eq!(chain.dma_unit(), 0);
        chain.update_control(prb(&[1], true, false, false));
        assert_eq!(chain.dma_unit(), 1);
        chain.update_control(prb(&[2], true, false, false));
        assert_eq!(chain.dma_unit(), 0, "nothing fitted answers DF2");
    }

    #[test]
    fn a_one_drive_chain_reports_df0_unless_another_select_is_asserted() {
        let mut chain = AmigaFloppyChain::new();
        chain
            .df0_mut()
            .insert_disk(Adf::from_bytes(vec![0; DD.len()]).expect("valid"));
        for byte in [prb(&[], false, false, false), prb(&[0], true, false, false)] {
            chain.update_control(byte);
            let expected = chain.df0().status();
            let status = chain.status();
            assert_eq!(status.disk_change, expected.disk_change);
            assert_eq!(status.write_protect, expected.write_protect);
            assert_eq!(status.track0, expected.track0);
            assert_eq!(status.ready, expected.ready);
        }
    }

    #[test]
    fn fitting_keeps_a_drive_already_there_and_stops_at_df3() {
        let mut chain = AmigaFloppyChain::new();
        chain.fit(3);
        chain
            .drive_mut(3)
            .expect("fitted")
            .insert_disk(Adf::from_bytes(vec![0; DD.len()]).expect("valid"));
        assert!(chain.fit(3));
        assert!(chain.drive(3).is_some_and(AmigaFloppyDrive::has_disk));
        assert!(!chain.fit(4));
        assert_eq!(chain.fitted_units().collect::<Vec<_>>(), [0, 3]);
    }
}
