//! Fixed Atari 7800 NTSC board over the original common engine.
use common_atari_7800::{Atari7800Board, Atari7800Core};
pub use common_atari_7800::{
    Atari7800Region, Cartridge, M6502, Maria, MariaRegion, PokeyLocation, TiaAudio,
};
/// This board's fixed wiring.
pub struct Board;
impl Atari7800Board for Board {
    const REGION: Atari7800Region = Atari7800Region::Ntsc;
    const MACHINE_ID: &'static str = "atari-7800";
}
/// This physical machine.
pub type Atari7800 = Atari7800Core<Board>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_atari_7800::Atari7800State;
    fn cartridge() -> Vec<u8> {
        include_bytes!("../../../test-data/synthetic-cartridges/atari-7800-logo.bin").to_vec()
    }
    #[test]
    fn fixed_board_boots_guest_and_ram() {
        let mut m = Atari7800::new(cartridge()).expect("cartridge");
        assert_eq!(m.board_id(), "atari-7800");
        m.poke(0x1800, 0xA5);
        assert_eq!(m.peek(0x1800), 0xA5);
        for _ in 0..3 {
            assert!(m.run_native_frame().1);
        }
        assert!(m.framebuffer().iter().any(|p| *p != m.framebuffer()[0]));
        assert_eq!(m.region(), Atari7800Region::Ntsc);
    }
    #[test]
    fn partial_execution_reaches_the_same_native_boundary() {
        let mut full = Atari7800::new(cartridge()).expect("cartridge");
        let mut partial = Atari7800::new(cartridge()).expect("cartridge");
        assert!(full.run_native_frame().1);
        assert!(partial.run_native_frame().1);
        let start = partial.master_clock();
        let (whole, completed) = full.run_native_frame();
        assert!(completed);
        for _ in 0..37 {
            partial.step_instruction();
        }
        let (remaining, completed) = partial.run_native_frame();
        assert!(completed);
        assert!(remaining < whole);
        assert_eq!(partial.master_clock() - start, whole);
        assert_eq!(partial.master_clock(), full.master_clock());
        assert_eq!(partial.frame_count(), full.frame_count());
        assert_eq!(partial.framebuffer(), full.framebuffer());
    }
    #[test]
    fn foreign_board_state_is_rejected() {
        let state = Atari7800State::new(cartridge(), Atari7800Region::Ntsc).expect("state");
        assert!(Atari7800::from_state(state).is_ok());
        let foreign = Atari7800State::new(cartridge(), Atari7800Region::Pal).expect("state");
        assert!(Atari7800::from_state(foreign).is_err());
    }
    #[test]
    fn cold_boot_preserves_board_and_controller_bus_reads() {
        let mut rom = cartridge();
        let len = rom.len();
        let entry = u16::from_le_bytes([rom[len - 4], rom[len - 3]]);
        let offset = usize::from(entry) - (65536 - len);
        rom[offset..offset + 8].copy_from_slice(&[
            0xAD,
            0x80,
            0x02,
            0x85,
            0x80,
            0x4C,
            (entry + 5) as u8,
            ((entry + 5) >> 8) as u8,
        ]);
        let original = Atari7800::new(rom).expect("controller probe");
        let mut m = original.cold_boot();
        assert_eq!(m.board_id(), "atari-7800");
        assert_eq!(m.region(), Atari7800Region::Ntsc);
        m.set_joystick(true, false, true, false);
        for _ in 0..8 {
            m.step_instruction();
        }
        assert_eq!(m.peek(0x80), 0xAF);
    }
}
