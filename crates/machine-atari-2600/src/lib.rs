//! Fixed Atari 2600 NTSC board over the original common engine.
use common_atari_2600::{Atari2600Board, Atari2600Core};
pub use common_atari_2600::{
    Atari2600Region, BankingScheme, Cartridge, M6502, Riot6532, Tia, TiaRegion,
};
/// This board's fixed wiring.
pub struct Board;
impl Atari2600Board for Board {
    const REGION: Atari2600Region = Atari2600Region::Ntsc;
    const MACHINE_ID: &'static str = "atari-2600";
}
/// This physical machine.
pub type Atari2600 = Atari2600Core<Board>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_atari_2600::Atari2600State;
    fn cartridge() -> Vec<u8> {
        include_bytes!("../../../test-data/synthetic-cartridges/atari-2600-logo.bin").to_vec()
    }
    #[test]
    fn fixed_board_boots_guest_and_keeps_controls() {
        let mut m = Atari2600::new(cartridge()).expect("project cartridge");
        assert_eq!(m.board_id(), "atari-2600");
        m.poke(0x80, 0xA5);
        assert_eq!(m.peek(0x80), 0xA5);
        let mut control_rom = cartridge();
        let entry = u16::from_le_bytes([control_rom[0xFFC], control_rom[0xFFD]]);
        let offset = usize::from(entry & 0xFFF);
        // LDA SWCHA; STA $80; JMP to itself: exercise the real guest bus read.
        control_rom[offset..offset + 8].copy_from_slice(&[
            0xAD,
            0x80,
            0x02,
            0x85,
            0x80,
            0x4C,
            (entry + 5) as u8,
            ((entry + 5) >> 8) as u8,
        ]);
        let mut controls = Atari2600::new(control_rom).expect("controller probe");
        controls.set_joystick_input(0xAF);
        for _ in 0..8 {
            controls.step_instruction();
        }
        assert_eq!(controls.peek(0x80), 0xAF);
        for _ in 0..3 {
            assert!(m.run_native_frame().1);
        }
        assert!(m.framebuffer().iter().any(|p| *p != m.framebuffer()[0]));
        assert_eq!(m.region(), Atari2600Region::Ntsc);
    }
    #[test]
    fn partial_execution_reaches_the_same_native_boundary() {
        let mut full = Atari2600::new(cartridge()).expect("cartridge");
        let mut partial = Atari2600::new(cartridge()).expect("cartridge");
        assert!(full.run_native_frame().1);
        assert!(partial.run_native_frame().1);
        let start = partial.master_clock();
        let (whole, complete) = full.run_native_frame();
        assert!(complete);
        for _ in 0..37 {
            partial.step_instruction();
        }
        let (remaining, complete) = partial.run_native_frame();
        assert!(complete);
        assert!(remaining < whole);
        assert_eq!(partial.master_clock() - start, whole);
        assert_eq!(partial.master_clock(), full.master_clock());
        assert_eq!(partial.frame_count(), full.frame_count());
        assert_eq!(partial.framebuffer(), full.framebuffer());
    }
    #[test]
    fn restore_and_cold_boot_keep_the_physical_board_fixed() {
        let state = Atari2600State::new(cartridge(), Atari2600Region::Ntsc).expect("state");
        let m = Atari2600::from_state(state).expect("matching board");
        assert_eq!(m.cold_boot().board_id(), "atari-2600");
        assert_eq!(m.cold_boot().region(), Atari2600Region::Ntsc);
        let foreign = Atari2600State::new(cartridge(), Atari2600Region::Pal).expect("state");
        assert!(Atari2600::from_state(foreign).is_err());
    }
    #[test]
    fn guest_without_vsync_does_not_report_a_completed_frame() {
        let rom = include_bytes!("../../../test-data/atari/synthetic-cart/atari-2600.a26");
        let mut m = Atari2600::new(rom.to_vec()).expect("no-sync cartridge");
        let before = m.frame_count();
        let (ticks, completed) = m.run_native_frame();
        assert!(ticks > 0);
        assert!(!completed);
        assert_eq!(m.frame_count(), before);
    }
}
