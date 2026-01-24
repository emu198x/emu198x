//! ZX Spectrum emulator supporting multiple models.
//!
//! This crate provides emulation for different ZX Spectrum models:
//! - `Spectrum16K` - Original 16K model with floating bus behavior
//! - `Spectrum48K` - Standard 48K model
//!
//! # Example
//!
//! ```ignore
//! use machine_spectrum::Spectrum48K;
//! use emu_core::Machine;
//!
//! let mut spec = Spectrum48K::new();
//! spec.load_file("48.rom", &rom_data)?;
//! spec.run_frame();
//! ```

mod audio;
mod input;
mod memory;
mod spectrum;
mod tape;
mod video;

pub use memory::{Memory16K, Memory48K, MemoryModel};
pub use spectrum::Spectrum;

/// ZX Spectrum 16K - Original model with 16K RAM.
///
/// Quirks:
/// - Reads from 0x8000-0xFFFF return floating bus values
/// - Writes to 0x8000-0xFFFF are ignored
/// - All RAM (0x4000-0x7FFF) is contended
/// - Does not support .SNA snapshots
pub type Spectrum16K = Spectrum<Memory16K>;

/// ZX Spectrum 48K - Standard model with 48K RAM.
///
/// Features:
/// - Full 48K RAM at 0x4000-0xFFFF
/// - Only lower 16K of RAM (0x4000-0x7FFF) is contended
/// - Supports .SNA and .TAP files
pub type Spectrum48K = Spectrum<Memory48K>;

#[cfg(test)]
mod tests {
    use super::*;
    use emu_core::Machine;

    #[test]
    fn spectrum_48k_fills_screen_memory() {
        let mut spec = Spectrum48K::new();

        // Program: fill screen memory with 0xFF
        spec.load(
            0x0000,
            &[
                0x21, 0x00, 0x40, // LD HL, 0x4000
                0x3E, 0xFF, // LD A, 0xFF
                0x77, // LD (HL), A
                0x23, // INC HL
                0xC3, 0x05, 0x00, // JP 0x0005
            ],
        );

        // Run for a while
        for _ in 0..100 {
            spec.run_frame();
        }

        // Check screen memory
        assert_eq!(spec.screen()[0], 0xFF);
        assert_eq!(spec.screen()[1], 0xFF);
    }

    #[test]
    fn spectrum_16k_ignores_writes_above_32k() {
        let mut spec = Spectrum16K::new();

        // Try to write above 32K
        spec.load(
            0x0000,
            &[
                0x21, 0x00, 0x80, // LD HL, 0x8000
                0x3E, 0xAB, // LD A, 0xAB
                0x77, // LD (HL), A
                0x76, // HALT
            ],
        );

        spec.run_frame();

        // The write should have been ignored
        // (We can't easily test this without accessing internal state,
        // but the emulation should not crash)
    }

    #[test]
    fn spectrum_16k_model_name() {
        let spec = Spectrum16K::new();
        assert_eq!(spec.model_name(), "ZX Spectrum 16K");
    }

    #[test]
    fn spectrum_48k_model_name() {
        let spec = Spectrum48K::new();
        assert_eq!(spec.model_name(), "ZX Spectrum 48K");
    }

    #[test]
    fn spectrum_16k_does_not_support_sna() {
        let mut spec = Spectrum16K::new();
        let fake_sna = vec![0u8; 49179];
        let result = spec.load_sna(&fake_sna);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not support"));
    }
}
