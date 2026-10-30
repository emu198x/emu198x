//! One fixed Americas Mega Drive board over shared wiring.
pub use common_sega_mega_drive::{Buttons, Device, Region};
use common_sega_mega_drive::{MegaDriveBoard, MegaDriveCore};
/// Fixed physical wiring.
pub struct Board;
impl MegaDriveBoard for Board {
    const REGION: Region = Region::Americas;
    const MACHINE_ID: &'static str = "sega-genesis";
}
/// This physical board.
pub type MegaDrive = MegaDriveCore<Board>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_sega_mega_drive::MegaDriveState;
    fn native() -> MegaDrive {
        MegaDrive::new(
            include_bytes!("../../../test-data/sega-mega-drive/board-split-v2/native.bin").to_vec(),
        )
    }
    fn pbc() -> MegaDrive {
        MegaDrive::new_power_base(
            include_bytes!("../../../test-data/sega-mega-drive/board-split-v2/power-base.sms")
                .to_vec(),
        )
    }
    #[test]
    fn guest_paints_pixels_and_reads_this_boards_region_register() {
        let mut m = native();
        for _ in 0..3 {
            m.run_frame();
        }
        assert_eq!(m.board_id(), "sega-genesis");
        assert_eq!(m.region(), Region::Americas);
        assert_eq!(m.peek(0xff0010), 0xa0);
        assert!(m.framebuffer().contains(&0xffff0000), "guest painted red");
        assert!(!m.vdp().pal_machine);
    }
    #[test]
    fn guest_reads_buttons_and_checked_restore_keeps_held_controls() {
        let mut m = native();
        m.set_buttons(
            0,
            Buttons {
                up: true,
                b: true,
                ..Buttons::default()
            },
        );
        m.run_frame();
        assert_eq!(m.peek(0xff0011), 0x6e);
        let bytes = postcard::to_allocvec(&m).expect("state");
        let mut restored =
            MegaDrive::from_state(postcard::from_bytes(&bytes).expect("payload")).expect("board");
        restored.finish_restore();
        assert!(restored.ports()[0].buttons.up && restored.ports()[0].buttons.b);
        let mut held = restored.ports()[0].buttons;
        held.up = false;
        restored.set_buttons(0, held);
        restored.run_frame();
        assert_eq!(restored.peek(0xff0011), 0x6f);
    }
    #[test]
    fn power_base_guest_runs_z80_and_keeps_its_selected_cartridge_bank() {
        let mut m = pbc();
        for _ in 0..3 {
            m.run_frame();
        }
        assert!(m.master_system_mode());
        assert_eq!(m.z80_ram()[1], 0x53, "bank 3 marker");
        assert!(m.z80_ram()[0] >= 2, "guest counted VBlank");
        let bytes = postcard::to_allocvec(&m).expect("state");
        let mut restored =
            MegaDrive::from_state(postcard::from_bytes(&bytes).expect("payload")).expect("board");
        restored.finish_restore();
        restored.run_frame();
        assert_eq!(restored.z80_ram()[1], 0x53);
        assert!(restored.master_system_mode());
    }
    #[test]
    fn native_guest_hands_the_bus_to_the_sound_cpu() {
        let mut rom = vec![0; 0x800];
        rom[..4].copy_from_slice(&0x00fffe00u32.to_be_bytes());
        rom[4..8].copy_from_slice(&0x200u32.to_be_bytes());
        let mut p = vec![
            0x33fcu16, 0x100, 0xa1, 0x1100, 0x0839, 0, 0xa1, 0x1100, 0x66f6,
        ];
        for (i, b) in [0x3eu16, 0x42, 0x32, 0, 0x10, 0x18, 0xfe]
            .into_iter()
            .enumerate()
        {
            p.extend([0x13fc, b, 0xa0, i as u16]);
        }
        p.extend([0x33fc, 0, 0xa1, 0x1100, 0x33fc, 0x100, 0xa1, 0x1200, 0x60fe]);
        for (i, w) in p.iter().enumerate() {
            rom[0x200 + 2 * i..0x202 + 2 * i].copy_from_slice(&w.to_be_bytes());
        }
        let mut m = MegaDrive::new(rom);
        for _ in 0..2 {
            m.run_frame();
        }
        assert!(!m.z80_in_reset());
        assert_eq!(m.z80_ram()[0x1000], 0x42);
    }
    #[test]
    fn state_binding_rejects_foreign_market_or_video_wiring() {
        let rom =
            include_bytes!("../../../test-data/sega-mega-drive/board-split-v2/native.bin").to_vec();
        for region in [Region::Japan, Region::Americas, Region::Europe] {
            if region == Region::Americas {
                continue;
            }
            assert!(MegaDrive::from_state(MegaDriveState::new(rom.clone(), region)).is_err());
            assert!(
                MegaDrive::from_state(MegaDriveState::new_power_base(rom.clone(), region)).is_err()
            );
        }
        let mut wrong = MegaDriveState::new(rom, Region::Americas);
        wrong.vdp.pal_machine = !false;
        assert!(MegaDrive::from_state(wrong).is_err());
    }
    #[test]
    fn partial_fields_match_whole_fields_in_native_and_converter_modes() {
        for converter in [false, true] {
            let mut full = if converter { pbc() } else { native() };
            let mut partial = if converter { pbc() } else { native() };
            full.run_frame();
            partial.run_frame();
            full.take_audio();
            partial.take_audio();
            let whole = full.run_frame();
            for _ in 0..137 {
                partial.tick();
            }
            let remaining = partial.run_frame();
            assert_eq!(remaining + 137, whole);
            assert_eq!(partial.mclk(), full.mclk());
            assert_eq!(partial.vdp().frames(), full.vdp().frames());
            assert_eq!(partial.framebuffer(), full.framebuffer());
            assert_eq!(partial.take_audio(), full.take_audio());
        }
    }
}
