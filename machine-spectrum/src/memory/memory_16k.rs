//! 16K Spectrum memory model.
//!
//! The original ZX Spectrum 16K has:
//! - 16K ROM at 0x0000-0x3FFF
//! - 16K RAM at 0x4000-0x7FFF
//! - Reads from 0x8000-0xFFFF return floating bus values
//! - Writes to 0x8000-0xFFFF are ignored
//! - All RAM (0x4000-0x7FFF) is contended

use super::model::MemoryModel;
use super::ula::Ula;

/// 16K Spectrum memory model.
#[derive(Default)]
pub struct Memory16K;

impl MemoryModel for Memory16K {
    const RAM_SIZE: usize = 16 * 1024;
    const MODEL_NAME: &'static str = "ZX Spectrum 16K";

    fn read(&self, data: &[u8; 65536], addr: u16, ula: &Ula) -> u8 {
        if addr < 0x8000 {
            // ROM (0x0000-0x3FFF) or RAM (0x4000-0x7FFF)
            data[addr as usize]
        } else {
            // 0x8000-0xFFFF: floating bus
            // Return what the ULA is currently reading from screen memory
            let screen_data = &data[0x4000..0x5B00];
            ula.floating_bus(screen_data)
        }
    }

    fn write(&self, data: &mut [u8; 65536], addr: u16, value: u8) -> bool {
        if addr >= 0x4000 && addr < 0x8000 {
            // RAM at 0x4000-0x7FFF
            data[addr as usize] = value;
            true
        } else {
            // ROM (0x0000-0x3FFF) or unmapped (0x8000-0xFFFF)
            false
        }
    }

    fn is_contended(&self, addr: u16) -> bool {
        // All RAM is contended on 16K (it's the same physical RAM as screen memory)
        addr >= 0x4000 && addr < 0x8000
    }

    fn supports_sna(&self) -> bool {
        // 16K doesn't have enough RAM for .SNA files
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::ula::T_STATES_PER_FRAME_48K;

    #[test]
    fn read_from_ram() {
        let model = Memory16K;
        let mut data = [0u8; 65536];
        let ula = Ula::new(T_STATES_PER_FRAME_48K);

        data[0x4000] = 0xAB;
        assert_eq!(model.read(&data, 0x4000, &ula), 0xAB);
    }

    #[test]
    fn read_from_rom() {
        let model = Memory16K;
        let mut data = [0u8; 65536];
        let ula = Ula::new(T_STATES_PER_FRAME_48K);

        data[0x0000] = 0xF3; // DI instruction in ROM
        assert_eq!(model.read(&data, 0x0000, &ula), 0xF3);
    }

    #[test]
    fn read_above_32k_returns_floating_bus() {
        let model = Memory16K;
        let mut data = [0u8; 65536];
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);

        // Put known data in RAM that shouldn't be read directly
        data[0x8000] = 0x42;

        // In border period, floating bus returns 0xFF
        ula.frame_t_state = 100;
        assert_eq!(model.read(&data, 0x8000, &ula), 0xFF);
        assert_eq!(model.read(&data, 0xFFFF, &ula), 0xFF);
    }

    #[test]
    fn write_to_ram() {
        let model = Memory16K;
        let mut data = [0u8; 65536];

        assert!(model.write(&mut data, 0x4000, 0xCD));
        assert_eq!(data[0x4000], 0xCD);

        assert!(model.write(&mut data, 0x7FFF, 0xEF));
        assert_eq!(data[0x7FFF], 0xEF);
    }

    #[test]
    fn write_to_rom_ignored() {
        let model = Memory16K;
        let mut data = [0u8; 65536];

        assert!(!model.write(&mut data, 0x0000, 0xFF));
        assert_eq!(data[0x0000], 0x00);
    }

    #[test]
    fn write_above_32k_ignored() {
        let model = Memory16K;
        let mut data = [0u8; 65536];

        assert!(!model.write(&mut data, 0x8000, 0xFF));
        assert_eq!(data[0x8000], 0x00);

        assert!(!model.write(&mut data, 0xFFFF, 0xFF));
        assert_eq!(data[0xFFFF], 0x00);
    }

    #[test]
    fn all_ram_is_contended() {
        let model = Memory16K;

        // ROM not contended
        assert!(!model.is_contended(0x0000));
        assert!(!model.is_contended(0x3FFF));

        // RAM is contended
        assert!(model.is_contended(0x4000));
        assert!(model.is_contended(0x5800));
        assert!(model.is_contended(0x7FFF));

        // Unmapped area not contended (doesn't exist)
        assert!(!model.is_contended(0x8000));
        assert!(!model.is_contended(0xFFFF));
    }

    #[test]
    fn does_not_support_sna() {
        assert!(!Memory16K::supports_sna(&Memory16K));
    }
}
