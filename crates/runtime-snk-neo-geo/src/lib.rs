//! SNK Neo Geo machine profile and runtime.
//!
//! One machine, the AES home console. Its BIOS is firmware the user
//! supplies; its cartridges are P/S/M/V/C ROM sets, read from a zip or
//! assembled by `format-snk-neo-geo-romset`.

mod debug;
mod input;
mod profiles;
mod queries;
mod runtime;
mod snapshot;

pub use format_snk_neo_geo_romset::{RomSet, RomSetError};
pub use machine_snk_neo_geo::{Buttons, Cartridge, NeoGeo};
pub use profiles::{BIOS_FIRMWARE_ID, Model, ZOOM_FIRMWARE_ID, blank, profile_for, profiles};
pub use queries::NeoGeoSessionQueryProvider;
pub use runtime::{FRAME_HEIGHT, FRAME_WIDTH, NeoGeoRuntime};

/// A stand-in BIOS and cartridge for tests here and in the crates above.
/// No real firmware is involved.
pub mod test_support {
    use crate::{Model, NeoGeoRuntime, RomSet};

    /// A BIOS, as dumped (words byte-swapped), that acknowledges the reset
    /// interrupt, swaps in the cartridge's vectors and jumps to `$000400`.
    #[must_use]
    pub fn bios() -> Vec<u8> {
        let mut rom = vec![0xFF; machine_snk_neo_geo::BIOS_SIZE];
        let words: [u16; 4] = [0x0010, 0xF300, 0x00C0, 0x0400];
        let code: [u16; 13] = [
            0x33FC, 0x0007, 0x003C, 0x000C, // MOVE.W #7,$3C000C
            0x13FC, 0x0000, 0x003A, 0x0013, // MOVE.B #0,$3A0013
            0x4EF9, 0x0000, 0x0400, // JMP $000400
            0x4E71, 0x4E71,
        ];
        for (at, word) in words
            .iter()
            .enumerate()
            .map(|(i, w)| (i * 2, w))
            .chain(code.iter().enumerate().map(|(i, w)| (0x400 + i * 2, w)))
        {
            // Stored as dumped: low byte first.
            rom[at..at + 2].copy_from_slice(&word.to_le_bytes());
        }
        rom
    }

    /// A P-ROM in the 68000's order with `program` at `$400`.
    #[must_use]
    pub fn p_rom(program: &[u16]) -> Vec<u8> {
        let mut p = vec![0; 0x1000];
        p[0..4].copy_from_slice(&0x0010_F300u32.to_be_bytes());
        p[4..8].copy_from_slice(&0x400u32.to_be_bytes());
        for (i, word) in program.iter().enumerate() {
            p[0x400 + i * 2..0x402 + i * 2].copy_from_slice(&word.to_be_bytes());
        }
        p
    }

    /// A runtime running `program` from the stand-in BIOS.
    ///
    /// # Panics
    ///
    /// Never: the stand-in BIOS is the right size.
    #[must_use]
    pub fn runtime_with_program(program: &[u16]) -> NeoGeoRuntime {
        let mut runtime = NeoGeoRuntime::new(Model::Aes, bios(), None).expect("stand-in BIOS");
        runtime.insert_rom_set(RomSet {
            p: p_rom(program),
            ..RomSet::default()
        });
        runtime
    }
}
