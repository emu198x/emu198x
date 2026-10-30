//! One fixed Aquarius Ntsc board over shared wiring.
use common_mattel_aquarius::{AquariusBoard, AquariusCore};
pub use common_mattel_aquarius::{AquariusRegion, FB_HEIGHT, FB_WIDTH, IoEvent};
/// Fixed physical wiring.
pub struct Board;
impl AquariusBoard for Board {
    const REGION: AquariusRegion = AquariusRegion::Ntsc;
    const MACHINE_ID: &'static str = "mattel-aquarius";
}
/// This physical board.
pub type Aquarius = AquariusCore<Board>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_mattel_aquarius::AquariusState;
    fn boot() -> Aquarius {
        Aquarius::new(
            include_bytes!("../../../test-data/synthetic-firmware/mattel-aquarius.rom").to_vec(),
            16,
        )
    }
    #[test]
    fn guest_boot_paints_this_board_and_control_does_not() {
        let mut m = boot();
        let mut control = Aquarius::new(
            include_bytes!("../../../test-data/synthetic-firmware/mattel-aquarius-control.rom")
                .to_vec(),
            0,
        );
        for _ in 0..10 {
            m.run_native_frame();
            control.run_native_frame();
        }
        assert_eq!(m.board_id(), "mattel-aquarius");
        assert_eq!(m.region(), AquariusRegion::Ntsc);
        assert!(m.framebuffer().iter().all(|p| *p == 0xff0000ff));
        assert!(control.framebuffer().iter().all(|p| *p == 0xff8080ff));
    }
    fn input_probe() -> Aquarius {
        let mut rom = vec![0; 8192];
        // Row zero -> $3800; controller one through AY register 15 -> $3801.
        let code = [
            0x01, 0xff, 0xfe, 0xed, 0x78, 0x32, 0x00, 0x38, 0x3e, 0x0f, 0xd3, 0xf7, 0xdb, 0xf6,
            0x32, 0x01, 0x38, 0xc3, 0, 0,
        ];
        rom[..code.len()].copy_from_slice(&code);
        Aquarius::new(rom, 0)
    }
    #[test]
    fn guest_reads_keyboard_and_controller_press_and_release() {
        let mut m = input_probe();
        m.set_key(0, 2, true);
        m.set_joystick(1, true, false, false, false, true);
        m.run_native_frame();
        assert_eq!(m.peek(0x3800), 0xfb);
        assert_eq!(m.peek(0x3801), 0xbb);
        m.set_key(0, 2, false);
        m.set_joystick(1, false, false, false, false, false);
        m.run_native_frame();
        assert_eq!(m.peek(0x3800), 0xff);
        assert_eq!(m.peek(0x3801), 0xff);
    }
    #[test]
    fn cartridge_and_ram_expansion_survive_checked_state_binding() {
        let mut m = boot();
        m.insert_cart(vec![0xa5; 16384]);
        m.poke(0x4000, 0x5a);
        m.poke(0x7fff, 0x3c);
        let bytes = postcard::to_allocvec(&m).expect("state");
        let raw = postcard::from_bytes(&bytes).expect("legacy payload");
        let m = Aquarius::from_state(raw).expect("same board");
        assert_eq!((m.peek(0xc000), m.peek(0xffff)), (0xa5, 0xa5));
        assert_eq!((m.peek(0x4000), m.peek(0x7fff)), (0x5a, 0x3c));
        assert_eq!(m.expansion_kb(), 16);
    }
    #[test]
    fn foreign_regional_hardware_is_rejected() {
        let wrong = AquariusState::new(vec![0; 8192], 0, AquariusRegion::Pal);
        assert!(Aquarius::from_state(wrong).is_err());
    }
    #[test]
    fn partial_native_field_keeps_clock_pixels_and_audio_aligned() {
        let mut full = boot();
        let mut partial = boot();
        full.run_native_frame();
        partial.run_native_frame();
        full.take_audio_buffer();
        partial.take_audio_buffer();
        let (whole, true) = full.run_native_frame() else {
            panic!("frame")
        };
        partial.advance_tstates(137);
        let (remaining, true) = partial.run_native_frame() else {
            panic!("frame")
        };
        assert_eq!(remaining + 137, whole);
        assert_eq!(partial.cpu_tstates(), full.cpu_tstates());
        assert_eq!(partial.frame_count(), full.frame_count());
        assert_eq!(partial.framebuffer(), full.framebuffer());
        assert_eq!(partial.take_audio_buffer(), full.take_audio_buffer());
    }
}
