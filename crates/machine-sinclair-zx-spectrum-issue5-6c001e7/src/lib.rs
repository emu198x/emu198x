//! Fixed issue5-6c001e7 physical owner. RAM fitted on this PCB is a profile.
//!
//! The exact board/ULA identity is saved independently of the runtime envelope.
//! The native functional ULA model does not qualify sub-cycle DRAM propagation
//! margins or analog video/power differences. Shared wiring is in the common
//! 48K-class crate; this owner never depends on another machine owner.
//!
//! The ULA cannot be selected by the caller:
//! ```compile_fail
//! use machine_sinclair_zx_spectrum_issue5_6c001e7::SpectrumIssue5Ula6c001e7With48k;
//! use common_sinclair_zx_spectrum_48k_class::UlaRevision;
//! let _ = SpectrumIssue5Ula6c001e7With48k::with_revision(UlaRevision::Ferranti5C);
//! ```

use common_sinclair_zx_spectrum::memory::{Spectrum16kMemory, Spectrum48kMemory};
use common_sinclair_zx_spectrum_48k_class::{PhysicalBoard, SpectrumMachineCore, Variant48kClass};

pub use common_sinclair_zx_spectrum::{
    AudioControls, SpeakerChannel,
    keyboard::{KeyboardMatrix, SpectrumKey},
};
pub use common_sinclair_zx_spectrum_48k_class::{
    KempstonButton, KempstonJoystick, TapeInput, UlaRevision,
};

/// Immutable physical board marker; neither RAM nor case changes this identity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Board;

impl Variant48kClass for Board {
    const MODEL_ID: &'static str = "sinclair-zx-spectrum-issue5-6c001e7";
    const BOARD: PhysicalBoard = PhysicalBoard::Issue5Ula6c001e7;
    const SPIDER: bool = Self::BOARD.has_spider();
}

/// This board populated with 16 KiB RAM.
pub type SpectrumIssue5Ula6c001e7With16k = SpectrumMachineCore<Spectrum16kMemory, Board>;
/// This board populated with 48 KiB RAM.
pub type SpectrumIssue5Ula6c001e7With48k = SpectrumMachineCore<Spectrum48kMemory, Board>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::memory::MemoryBus;

    #[test]
    fn both_populations_keep_the_same_fixed_physical_board() {
        let small = SpectrumIssue5Ula6c001e7With16k::new();
        let full = SpectrumIssue5Ula6c001e7With48k::new();
        assert_eq!(small.physical_board(), PhysicalBoard::Issue5Ula6c001e7);
        assert_eq!(full.physical_board(), small.physical_board());
        assert!(small.has_valid_hardware());
        assert!(full.has_valid_hardware());
        assert_eq!(small.memory().ram().len(), 16 * 1024);
        assert_eq!(full.memory().ram().len(), 48 * 1024);
        assert_eq!(
            small.revision(),
            PhysicalBoard::Issue5Ula6c001e7.ula_family()
        );
        assert_eq!(full.revision(), small.revision());
    }

    #[test]
    fn fitted_ram_changes_the_upper_map_and_rom_stays_read_only() {
        let mut small = SpectrumIssue5Ula6c001e7With16k::with_board_rom([0xa5; 16384]);
        let mut full = SpectrumIssue5Ula6c001e7With48k::with_board_rom([0xa5; 16384]);
        for address in [0, 0x3fff] {
            small.write(address, 0);
            full.write(address, 0);
            assert_eq!(small.read(address), 0xa5);
            assert_eq!(full.read(address), 0xa5);
        }
        for address in 0x4000..=u16::MAX {
            full.write(address, (address as u8).wrapping_mul(17) ^ 0x5a);
            small.write(address, (address as u8).wrapping_mul(17) ^ 0x5a);
        }
        for address in 0x4000..=u16::MAX {
            let expected = (address as u8).wrapping_mul(17) ^ 0x5a;
            assert_eq!(full.read(address), expected);
            if address < 0x8000 {
                assert_eq!(small.read(address), expected);
            } else {
                assert_eq!(small.read(address), 0xff);
            }
        }
    }
}
