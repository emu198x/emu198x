//! One fixed timex-ts1000 board; RAM packs remain configurations of this board.
pub use common_sinclair_zx81::{
    BOARD_ROM_SIZE, FB_WIDTH, HiResBoard, IoEvent, KeyboardState, TelevisionStandard, Zx81Key,
    Zx81Video, hires, input, tape, video,
};
use common_sinclair_zx81::{Zx81Board, Zx81Core};
/// Fixed physical wiring.
pub struct Board;
impl Zx81Board for Board {
    const STANDARD: TelevisionStandard = TelevisionStandard::SixtyHz;
    const MACHINE_ID: &'static str = "timex-ts1000";
}
/// This physical machine, with its TV strap fixed at construction.
pub type Zx81 = Zx81Core<Board>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx81::Zx81State;
    fn firmware() -> Vec<u8> {
        include_bytes!("../../../test-data/synthetic-firmware/sinclair-zx81.rom").to_vec()
    }
    #[test]
    fn fixed_board_boots_guest_generated_pixels() {
        let mut m = Zx81::new(firmware(), 16384).expect("synthetic firmware");
        assert_eq!(m.board_id(), "timex-ts1000");
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
        let mut rom = vec![0; 8192];
        rom[..8].copy_from_slice(&[0x3E, 0xFD, 0xDB, 0xFE, 0x32, 0x00, 0x40, 0x76]);
        let mut m = Zx81::new(rom, 1024).expect("probe");
        m.press_key(Zx81Key::A);
        for _ in 0..8 {
            m.step_instruction();
        }
        assert_eq!(m.peek(0x4000) & 0x41, 0);
    }
    #[test]
    fn ram_pack_changes_capacity_without_changing_the_board() {
        for ram in [1024, 2048, 16384] {
            let mut m = Zx81::new(firmware(), ram).expect("RAM configuration");
            assert_eq!(m.board_id(), "timex-ts1000");
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
        let mut own = Zx81State::new(firmware(), 1024).expect("state");
        own.set_television_standard(TelevisionStandard::SixtyHz);
        assert!(Zx81::from_state(own).is_ok());
        let mut foreign = Zx81State::new(firmware(), 1024).expect("state");
        foreign.set_television_standard(TelevisionStandard::FiftyHz);
        assert!(Zx81::from_state(foreign).is_err());
    }
}
