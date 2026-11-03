//! CX52 switch network, from Atari service-manual Figure 6-9 and board
//! Figure 2-4 (CX5200 Field Service Manual, Rev. 4, July 1983).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub(super) struct Controller {
    pub pots: [u8; 2],
    pub keys: u16,
    pub upper_fire: bool,
}

impl Default for Controller {
    fn default() -> Self {
        Self {
            pots: [super::POT_CENTER; 2],
            keys: 0,
            upper_fire: false,
        }
    }
}

impl Controller {
    pub fn key_at(&self, address: u8) -> bool {
        // K0 and K5 are unused: each physical switch appears four times.
        let key = usize::from((address >> 1) & 15);
        let row = key >> 2;
        let column = key & 3;
        let rows: [u8; 4] = std::array::from_fn(|r| ((self.keys >> (r * 4)) & 15) as u8);
        // Follow alternating row/column connections. With no isolation diodes,
        // three held corners also close the fourth corner electrically.
        let mut reached_rows = 1u8 << row;
        let mut reached_columns = 0;
        loop {
            let previous = (reached_rows, reached_columns);
            for (r, &columns) in rows.iter().enumerate() {
                if reached_rows & (1 << r) != 0 {
                    reached_columns |= columns;
                }
                if reached_columns & columns != 0 {
                    reached_rows |= 1 << r;
                }
            }
            if previous == (reached_rows, reached_columns) {
                break;
            }
        }
        reached_columns & (1 << column) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::Controller;

    #[test]
    fn all_switch_subsets_match_frozen_reference_connectivity() {
        // Each u16 is the 16 physical intersections from unchanged Altirra's
        // effective-matrix method. Source: docs' October controller evidence,
        // matrix-reference.csv.gz, both upper-fire states checked on export.
        let reference = include_bytes!("controller-matrix.bin");
        assert_eq!(reference.len(), 32768 * 2);
        for (held, bytes) in reference.as_chunks::<2>().0.iter().enumerate() {
            let expected = u16::from_le_bytes([bytes[0], bytes[1]]);
            for upper_fire in [false, true] {
                let controller = Controller {
                    keys: (held as u16) << 1,
                    upper_fire,
                    ..Controller::default()
                };
                for address in 0..64 {
                    let key = (address >> 1) & 15;
                    assert_eq!(
                        controller.key_at(address),
                        expected & (1 << key) != 0,
                        "held {held:#06x}, upper {upper_fire}, address {address:#04x}"
                    );
                }
            }
        }
    }
}
