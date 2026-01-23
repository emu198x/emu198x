//! 48K Spectrum memory model.
//!
//! The ZX Spectrum 48K has:
//! - 16K ROM at 0x0000-0x3FFF
//! - 48K RAM at 0x4000-0xFFFF
//! - Only lower 16K of RAM (0x4000-0x7FFF) is contended

use super::model::MemoryModel;
use super::ula::Ula;

/// 48K Spectrum memory model.
#[derive(Default)]
pub struct Memory48K;

impl MemoryModel for Memory48K {
    const RAM_SIZE: usize = 48 * 1024;
    const MODEL_NAME: &'static str = "ZX Spectrum 48K";

    fn read(&self, data: &[u8; 65536], addr: u16, _ula: &Ula) -> u8 {
        // Full 64K address space is mapped
        data[addr as usize]
    }

    fn write(&self, data: &mut [u8; 65536], addr: u16, value: u8) -> bool {
        if addr >= 0x4000 {
            // RAM at 0x4000-0xFFFF
            data[addr as usize] = value;
            true
        } else {
            // ROM at 0x0000-0x3FFF
            false
        }
    }

    fn is_contended(&self, addr: u16) -> bool {
        // Only lower 16K of RAM is contended (same as screen memory)
        addr >= 0x4000 && addr < 0x8000
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::ula::T_STATES_PER_FRAME_48K;

    #[test]
    fn read_full_address_space() {
        let model = Memory48K;
        let mut data = [0u8; 65536];
        let ula = Ula::new(T_STATES_PER_FRAME_48K);

        data[0x0000] = 0xF3;
        data[0x4000] = 0xAB;
        data[0x8000] = 0xCD;
        data[0xFFFF] = 0xEF;

        assert_eq!(model.read(&data, 0x0000, &ula), 0xF3);
        assert_eq!(model.read(&data, 0x4000, &ula), 0xAB);
        assert_eq!(model.read(&data, 0x8000, &ula), 0xCD);
        assert_eq!(model.read(&data, 0xFFFF, &ula), 0xEF);
    }

    #[test]
    fn write_to_ram() {
        let model = Memory48K;
        let mut data = [0u8; 65536];

        // Lower RAM
        assert!(model.write(&mut data, 0x4000, 0xAB));
        assert_eq!(data[0x4000], 0xAB);

        // Upper RAM
        assert!(model.write(&mut data, 0x8000, 0xCD));
        assert_eq!(data[0x8000], 0xCD);

        assert!(model.write(&mut data, 0xFFFF, 0xEF));
        assert_eq!(data[0xFFFF], 0xEF);
    }

    #[test]
    fn write_to_rom_ignored() {
        let model = Memory48K;
        let mut data = [0u8; 65536];

        assert!(!model.write(&mut data, 0x0000, 0xFF));
        assert_eq!(data[0x0000], 0x00);

        assert!(!model.write(&mut data, 0x3FFF, 0xFF));
        assert_eq!(data[0x3FFF], 0x00);
    }

    #[test]
    fn contention_only_in_lower_ram() {
        let model = Memory48K;

        // ROM not contended
        assert!(!model.is_contended(0x0000));
        assert!(!model.is_contended(0x3FFF));

        // Lower RAM is contended
        assert!(model.is_contended(0x4000));
        assert!(model.is_contended(0x5800));
        assert!(model.is_contended(0x7FFF));

        // Upper RAM not contended
        assert!(!model.is_contended(0x8000));
        assert!(!model.is_contended(0xFFFF));
    }

    #[test]
    fn supports_sna() {
        assert!(Memory48K::supports_sna(&Memory48K));
    }
}
