//! Fixed Atari 5200 CA020108 assembly. No VCS adaptor is emulated.
use common_atari_5200::{Atari5200Board, Atari5200Core};
pub use common_atari_5200::{Atari5200Region, CartLayout, Cartridge};
pub struct Board;
impl Atari5200Board for Board {
    const MACHINE_ID: &'static str = "atari-5200-ca020108";
    const CONTROLLER_PORTS: usize = 4;
}
pub type Atari5200 = Atari5200Core<Board>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_atari_5200::{Atari5200Region, Atari5200State};
    fn cart() -> Vec<u8> {
        let mut rom = vec![0xea; 32768];
        rom[..8].copy_from_slice(&[0xa9, 0x46, 0x8d, 0x1a, 0xc0, 0x4c, 5, 0x40]);
        rom[0x7ffc..0x7ffe].copy_from_slice(&[0, 0x40]);
        rom
    }
    fn new() -> Atari5200 {
        Atari5200::new(cart(), vec![]).expect("synthetic cartridge")
    }
    #[test]
    fn guest_changes_the_fixed_ntsc_raster() {
        let mut m = new();
        let before = m.framebuffer().to_vec();
        for _ in 0..3 {
            assert!(m.run_native_frame().1);
        }
        assert_eq!(m.region(), Atari5200Region::Ntsc);
        assert_eq!(m.master_clock(), 3 * 262 * 228);
        assert_ne!(m.framebuffer(), before);
        assert_eq!(m.peek(0x4000), 0xa9);
    }
    #[test]
    fn only_fitted_controller_ports_change_hardware_state() {
        for port in 0..4 {
            let mut m = new();
            let before = postcard::to_allocvec(&m).expect("before");
            m.set_controller_joystick(port, 17, 219);
            m.set_controller_fire(port, false, true);
            m.set_controller_fire(port, true, true);
            m.set_controller_keypad(port, 0x1e, true);
            let after = postcard::to_allocvec(&m).expect("after");
            assert_eq!(before != after, port < PORTS, "physical port {port}");
            assert_eq!(
                m.gtia().read(0x10 + port as u8),
                if port < PORTS { 0 } else { 1 }
            );
        }
    }
    #[test]
    fn restore_continues_at_the_same_next_native_boundary() {
        let mut original = new();
        for _ in 0..37 {
            original.step_instruction();
        }
        original.take_audio_buffer(); // Compare future audio, after consuming the host queue.
        let bytes = postcard::to_allocvec(&original).expect("original");
        let state: Atari5200State = postcard::from_bytes(&bytes).expect("v20 payload");
        let mut restored = Atari5200::from_state(state).expect("fixed board");
        assert_eq!(postcard::to_allocvec(&restored).expect("restored"), bytes);
        assert_eq!(original.run_native_frame(), restored.run_native_frame());
        assert_eq!(original.framebuffer(), restored.framebuffer());
        assert!(
            original.take_audio_buffer() == restored.take_audio_buffer(),
            "future audio differs"
        );
        assert_eq!(
            postcard::to_allocvec(&original).expect("final"),
            postcard::to_allocvec(&restored).expect("final")
        );
    }
    #[test]
    fn a_pal_payload_cannot_change_this_boards_wiring() {
        let foreign = Atari5200State::new(cart(), vec![], Atari5200Region::Pal)
            .expect("experimental payload");
        assert!(Atari5200::from_state(foreign).is_err());
    }
    #[test]
    fn parsed_cartridge_layout_and_firmware_survive_construction() {
        let cart = Cartridge::from_rom_with_layout(&vec![0xea; 16384], CartLayout::TwoChip16K)
            .expect("explicit layout");
        let m = Atari5200::from_cartridge(cart, vec![0x53; 2048]);
        assert_eq!(m.cart().layout(), CartLayout::TwoChip16K);
        assert_eq!(m.peek(0xf800), 0x53);
    }
    const PORTS: usize = 4;
}
