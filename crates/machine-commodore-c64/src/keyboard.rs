//! C64 keyboard matrix state.

use serde::{Deserialize, Serialize};

/// 8×8 keyboard matrix.
///
/// Internally indexed by row. Each row byte stores one bit per column, where
/// `1` means pressed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardMatrix {
    rows: [u8; 8],
    shift_lock: bool,
}

impl KeyboardMatrix {
    /// Creates a cleared matrix.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rows: [0; 8],
            shift_lock: false,
        }
    }

    /// Sets the mechanical SHIFT LOCK contact, independently of left Shift.
    /// `true` closes its PA1/PB7 connection; repeated values are idempotent.
    pub fn set_shift_lock(&mut self, locked: bool) {
        self.shift_lock = locked;
    }

    /// Sets or clears one key position.
    pub fn set_key(&mut self, row: u8, col: u8, pressed: bool) {
        if row >= 8 || col >= 8 {
            return;
        }

        let mask = 1u8 << col;
        let slot = &mut self.rows[usize::from(row)];
        if pressed {
            *slot |= mask;
        } else {
            *slot &= !mask;
        }
    }

    /// Returns the CIA1 Port B input value for one active-low row-select mask
    /// driven through CIA1 Port A.
    #[must_use]
    pub fn scan(&self, row_mask: u8) -> u8 {
        let mut cols = 0u8;
        for (row, row_data) in self.rows.iter().enumerate() {
            if row_mask & (1u8 << row) == 0 {
                cols |= *row_data;
            }
        }
        if self.shift_lock && row_mask & 0x02 == 0 {
            cols |= 0x80;
        }
        !cols
    }

    /// Returns PA inputs pulled low by direct contacts to low PB pins.
    /// This is one step through the matrix, used when resolving a connected
    /// group and when a driven PB high suppresses an indirect reverse path.
    #[must_use]
    pub(crate) fn scan_reverse(&self, column_mask: u8) -> u8 {
        let mut rows = 0u8;
        for (row, row_data) in self.rows.iter().enumerate() {
            if row_data & !column_mask != 0 {
                rows |= 1 << row;
            }
        }
        if self.shift_lock && column_mask & 0x80 == 0 {
            rows |= 0x02;
        }
        !rows
    }

    /// Resolve closed contacts and CIA1 output contention.
    ///
    /// `pa`/`pb` are DDR-resolved drives (including PB timer outputs).
    /// `pa_low` and `pb_high` identify output-latch drivers, excluding input
    /// pull-ups. Joystick masks are external active-low inputs. The rules
    /// follow VICE 3.10 c64cia1.c, including SHIFT LOCK's stronger contact.
    #[must_use]
    pub(crate) fn resolve_ports(
        &self,
        pa: u8,
        pb: u8,
        pa_low: u8,
        pb_high: u8,
        joy_a: u8,
        joy_b: u8,
    ) -> (u8, u8) {
        if !self.shift_lock && self.rows == [0; 8] {
            return (pa & joy_a, pb & joy_b);
        }
        let mut read_a = 0xFF;
        let mut read_b = 0xFF;
        let mut strong_b = pb_high;
        for pin in 0..8 {
            let bit = 1 << pin;
            if pa & joy_a & bit == 0 {
                let (rows, columns) = self.connected(bit, 0);
                read_a &= !rows;
                read_b &= !columns;
                // A PB output high defeats one ordinary PA output low. Two
                // connected low PA drivers can defeat that high instead, as
                // can SHIFT LOCK when PA1 is itself an output low. A joystick
                // grounding PA1 does not meet that driver condition.
                if (rows & pa_low).count_ones() >= 2
                    || (self.shift_lock && pin == 1 && pa_low & bit != 0)
                {
                    strong_b &= !columns;
                }
            }
            if pb & joy_b & bit == 0 {
                let (rows, columns) = self.connected(0, bit);
                read_b &= !columns;
                // A driven high on another connected PB pin suppresses
                // ghost rows, but not the direct contact to this low pin.
                read_a &= if columns & pb_high == 0 {
                    !rows
                } else {
                    self.scan_reverse(!bit)
                };
            }
        }
        (read_a & pa & joy_a, ((read_b & pb) | strong_b) & joy_b)
    }

    /// Follow closed contacts until the group contains every reachable pin.
    fn connected(&self, mut rows: u8, mut columns: u8) -> (u8, u8) {
        loop {
            let previous = (rows, columns);
            columns |= !self.scan(!rows);
            rows |= !self.scan_reverse(!columns);
            if previous == (rows, columns) {
                return (rows, columns);
            }
        }
    }

    /// Releases all keys.
    pub fn release_all(&mut self) {
        self.rows = [0; 8];
        self.shift_lock = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_matrix_reads_all_high() {
        let matrix = KeyboardMatrix::new();
        assert_eq!(matrix.scan(0x00), 0xFF);
        assert_eq!(matrix.scan(0xFF), 0xFF);
    }

    #[test]
    fn selected_row_reads_pressed_column_low() {
        let mut matrix = KeyboardMatrix::new();
        matrix.set_key(0, 1, true);
        assert_eq!(matrix.scan(0xFE) & 0x02, 0x00);
        assert_eq!(matrix.scan(0xFD), 0xFF);
    }

    #[test]
    fn other_rows_remain_high_for_same_column() {
        let mut matrix = KeyboardMatrix::new();
        matrix.set_key(0, 1, true);
        assert_eq!(matrix.scan(0xFD), 0xFF);
    }

    #[test]
    fn release_all_clears_rows() {
        let mut matrix = KeyboardMatrix::new();
        matrix.set_key(0, 0, true);
        matrix.set_key(3, 7, true);
        matrix.release_all();
        assert_eq!(matrix.scan(0x00), 0xFF);
    }
}
