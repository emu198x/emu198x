//! One fixed atari-800xl-pal board over the original common engine.
use common_atari_800xl::{Atari800xlBoard, Atari800xlCore};
pub use common_atari_800xl::{Atari800xlRegion, Cartridge, CartridgeKind, Disk, MemoryModel};
/// Fixed physical wiring.
pub struct Board;
impl Atari800xlBoard for Board {
    const REGION: Atari800xlRegion = Atari800xlRegion::Pal;
    const MEMORY: MemoryModel = MemoryModel::Xl64k;
    const MACHINE_ID: &'static str = "atari-800xl-pal";
}
/// This physical board.
pub type Atari800xl = Atari800xlCore<Board>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_atari_800xl::Atari800xlState;
    fn cartridge() -> Vec<u8> {
        include_bytes!("../../../test-data/atari/gtia-modes/atari-800xl-gtia-modes.bin").to_vec()
    }
    fn new() -> Atari800xl {
        Atari800xl::new(None, None, Some(cartridge()), false).expect("project cartridge")
    }
    #[test]
    fn guest_boot_produces_pixels_on_the_fixed_board() {
        let mut m = new();
        assert_eq!(m.board_id(), "atari-800xl-pal");
        for _ in 0..3 {
            assert!(m.run_native_frame().1);
        }
        assert_eq!(m.region(), Atari800xlRegion::Pal);
        assert!(m.framebuffer().iter().any(|p| *p != m.framebuffer()[0]));
        assert!(m.master_clock() > 0);
    }
    #[test]
    fn guest_reads_pressed_and_released_joystick_and_fire_lines() {
        let mut rom = vec![0xEA; 8192];
        rom[..20].copy_from_slice(&[
            0xA9, 4, 0x8D, 2, 0xD3, 0xAD, 0, 0xD3, 0x8D, 0, 6, 0xAD, 0x10, 0xD0, 0x8D, 1, 6, 0x4C,
            0x11, 0xA0,
        ]);
        for (pressed, joystick, fire) in [(false, 0xFF, 1), (true, 0xFA, 0)] {
            let mut m = Atari800xl::new(None, None, Some(rom.clone()), false).expect("probe");
            m.set_joystick(pressed, false, pressed, false);
            m.set_fire(pressed);
            for _ in 0..16 {
                m.step_instruction();
            }
            assert_eq!(m.peek(0x0600), joystick);
            assert_eq!(m.peek(0x0601), fire);
        }
    }
    #[test]
    fn portb_selectors_follow_this_boards_memory_controller() {
        let mut m = new();
        assert_eq!(m.ram().len(), 65536);
        for bank in 0u8..4 {
            m.load_program_byte(0xD301, 0xE3 | (bank << 2));
            m.poke(0x4000, 0xA0 | bank);
        }
        for bank in 0u8..4 {
            m.load_program_byte(0xD301, 0xE3 | (bank << 2));
            assert_eq!(m.peek(0x4000), 0xA3);
            assert_eq!(m.extended_ram(usize::from(bank), 0), None);
        }
    }
    #[test]
    fn restore_rejects_foreign_region_or_memory_controller() {
        let own = Atari800xlState::new(None, None, Some(cartridge()), Atari800xlRegion::Pal, false)
            .expect("state")
            .with_memory(MemoryModel::Xl64k);
        assert!(Atari800xl::from_state(own).is_ok());
        for (region, memory) in [
            (Atari800xlRegion::Ntsc, MemoryModel::Xl64k),
            (Atari800xlRegion::Pal, MemoryModel::Xe128k),
        ] {
            let state = Atari800xlState::new(None, None, Some(cartridge()), region, false)
                .expect("state")
                .with_memory(memory);
            assert!(Atari800xl::from_state(state).is_err());
        }
    }
    #[test]
    fn a_partial_field_finishes_at_the_same_native_boundary() {
        let mut full = new();
        let mut partial = new();
        for _ in 0..3 {
            assert!(full.run_native_frame().1);
            assert!(partial.run_native_frame().1);
        }
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
}
