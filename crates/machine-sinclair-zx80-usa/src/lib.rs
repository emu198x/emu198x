//! One fixed sinclair-zx80-usa board; RAM packs remain configurations of this board.
pub use common_sinclair_zx80::{
    FB_WIDTH, IoEvent, KeyboardState, TelevisionStandard, Zx80Key, Zx80Video, input, tape,
};
use common_sinclair_zx80::{Zx80Board, Zx80Core};
/// Fixed physical wiring.
pub struct Board;
impl Zx80Board for Board {
    const STANDARD: TelevisionStandard = TelevisionStandard::SixtyHz;
    const MACHINE_ID: &'static str = "sinclair-zx80-usa";
}
/// This physical machine, with its TV strap fixed at construction.
pub type Zx80 = Zx80Core<Board>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx80::Zx80State;
    fn firmware() -> Vec<u8> {
        include_bytes!("../../../test-data/synthetic-firmware/sinclair-zx80.rom").to_vec()
    }
    #[test]
    fn fixed_board_boots_guest_generated_pixels() {
        let mut m = Zx80::new(firmware(), 16384).expect("synthetic firmware");
        assert_eq!(m.board_id(), "sinclair-zx80-usa");
        for _ in 0..20 {
            m.run_frame();
        }
        assert!(m.master_clock() > 0);
        assert_eq!(m.frame_count(), 20);
        assert_eq!(m.television_standard(), TelevisionStandard::SixtyHz);
        assert!(m.framebuffer().contains(&0xFF000000), "CPU-generated ink");
        assert!(m.framebuffer().contains(&0xFFFFFFFF), "paper/border");
    }
    #[test]
    fn guest_reads_fixed_strap_and_selected_keyboard_row() {
        let mut rom = vec![0; 4096];
        rom[..8].copy_from_slice(&[0x3E, 0xFD, 0xDB, 0xFE, 0x32, 0x00, 0x40, 0x76]);
        let mut m = Zx80::new(rom, 1024).expect("probe");
        m.press_key(Zx80Key::A);
        for _ in 0..8 {
            m.step_instruction();
        }
        assert_eq!(m.peek(0x4000) & 0x41, 0);
    }
    #[test]
    fn ram_pack_changes_capacity_without_changing_the_board() {
        for ram in [1024, 2048, 16384] {
            let mut m = Zx80::new(firmware(), ram).expect("RAM configuration");
            assert_eq!(m.board_id(), "sinclair-zx80-usa");
            assert_eq!(m.ram_bytes(), ram);
            m.poke(0x4000, 0xA5);
            assert_eq!(
                m.peek(if ram < 16384 {
                    0x4000 + ram as u16
                } else {
                    0xC000
                }),
                0xA5,
                "RAM decode mirror"
            );
        }
    }
    #[test]
    fn original_state_binds_only_to_the_matching_board() {
        let mut own = Zx80State::new(firmware(), 1024).expect("state");
        own.set_television_standard(TelevisionStandard::SixtyHz);
        assert!(Zx80::from_state(own).is_ok());
        let mut foreign = Zx80State::new(firmware(), 1024).expect("state");
        foreign.set_television_standard(TelevisionStandard::FiftyHz);
        assert!(Zx80::from_state(foreign).is_err());
    }
}
