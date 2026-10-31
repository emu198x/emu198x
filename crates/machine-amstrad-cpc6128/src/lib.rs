//! One fixed Amstrad CPC6128 configuration.
pub use common_amstrad_cpc::CpcModel;
/// Immutable physical configuration.
pub struct Board;
impl common_amstrad_cpc::CpcBoard for Board {
    const MODEL: CpcModel = CpcModel::Cpc6128;
    const MACHINE_ID: &'static str = "amstrad-cpc6128";
}
/// This board over shared CPC hardware.
pub type AmstradCpc = common_amstrad_cpc::CpcCore<Board>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_amstrad_cpc::{DiskImage, DiskSector, DiskTrack, TapeSpan};
    const FIRMWARE: &[u8] = include_bytes!("../../../test-data/synthetic-firmware/amstrad-cpc.rom");
    fn boot() -> AmstradCpc {
        let mut m = AmstradCpc::new(FIRMWARE).expect("project firmware");
        // Let firmware program the initially zeroed CRTC before counting fields.
        for _ in 0..20 {
            m.run_frame();
        }
        for _ in 0..4 {
            assert!(m.run_native_frame().1);
        }
        m
    }
    #[test]
    fn fixed_identity_and_memory_match_this_owner() {
        let m = AmstradCpc::new(FIRMWARE).expect("firmware");
        assert_eq!(m.board_id(), "amstrad-cpc6128");
        assert_eq!(m.ram().len(), 131072);
        assert_eq!(m.firmware(), FIRMWARE);
        assert!(AmstradCpc::new(&[0; 3]).is_err());
    }
    #[test]
    fn guest_firmware_builds_a_raster_but_control_cannot() {
        let mut m = boot();
        for _ in 0..16 {
            assert!(m.run_native_frame().1);
        }
        assert!(m.framebuffer().iter().all(|p| *p == 0xff00_6060));
        let mut control = AmstradCpc::new(include_bytes!(
            "../../../test-data/synthetic-firmware/amstrad-cpc-control.rom"
        ))
        .expect("control");
        control.run_frame();
        assert!(control.framebuffer().iter().all(|p| *p == 0xff00_0000));
    }
    #[test]
    fn guest_reads_pressed_and_released_keys_through_ppi_and_ay() {
        let mut program = Vec::new();
        for (port, val) in [
            (0xf782_u16, 0x82_u8),
            (0xf400, 14),
            (0xf600, 0xc8),
            (0xf782, 0x92),
            (0xf600, 0x48),
        ] {
            program.extend([0x01, port as u8, (port >> 8) as u8, 0x3e, val, 0xed, 0x79]);
        }
        program.extend([0x01, 0x00, 0xf4, 0xed, 0x78, 0x32, 0x00, 0x80, 0x18, 0xf9]);
        let mut firmware = vec![0; 32768];
        firmware[..program.len()].copy_from_slice(&program);
        let mut m = AmstradCpc::new(&firmware).expect("guest");
        assert!(m.press_char('a'));
        m.advance_tstates(2000);
        assert_eq!(m.ram_byte(0x8000), 0xdf);
        m.release_char('a');
        m.advance_tstates(2000);
        assert_eq!(m.ram_byte(0x8000), 0xff);
    }
    #[test]
    fn ppi_drives_ay_sound_and_muting_stops_it() {
        let mut m = boot();
        m.take_audio_buffer();
        m.out(0xf782, 0x82);
        for (reg, val) in [(0, 40), (1, 0), (7, 0x3e), (8, 15)] {
            m.out(0xf400, reg);
            m.out(0xf600, 0xc0);
            m.out(0xf600, 0);
            m.out(0xf400, val);
            m.out(0xf600, 0x80);
            m.out(0xf600, 0);
        }
        m.run_native_frame();
        let sound = m.take_audio_buffer();
        assert!(sound.iter().any(|x| x.abs() > 0.001));
        assert!(sound.windows(2).any(|p| p[0] != p[1]));
        m.out(0xf400, 8);
        m.out(0xf600, 0xc0);
        m.out(0xf600, 0);
        m.out(0xf400, 0);
        m.out(0xf600, 0x80);
        m.out(0xf600, 0);
        m.run_native_frame();
        m.take_audio_buffer();
        m.run_native_frame();
        assert!(m.take_audio_buffer().iter().all(|x| *x == 0.0));
    }
    #[test]
    fn rom_overlay_and_physical_ram_banking_work_independently() {
        let mut m = AmstradCpc::new(FIRMWARE).expect("firmware");
        m.poke(0, 0x5a);
        assert_eq!(m.peek(0), FIRMWARE[0]);
        m.out(0x7f00, 0x8c);
        assert_eq!(m.peek(0), 0x5a);
        m.poke(0x4125, 0x11);
        m.out(0x7f00, 0xc4);
        m.poke(0x4125, 0x22);
        m.out(0x7f00, 0xc0);
        assert_eq!(m.peek(0x4125), if true { 0x11 } else { 0x22 });
        m.fit_upper_rom(3, &vec![0x53; 0x4000]).expect("upper ROM");
        m.out(0xdf00, 3);
        m.out(0x7f00, 0x80);
        assert_eq!(m.peek(0xc000), 0x53);
    }
    #[test]
    fn wired_disc_capability_and_cassette_motor_match_this_board() {
        let mut m = AmstradCpc::new(FIRMWARE).expect("firmware");
        let image = DiskImage {
            sides: 1,
            tracks_per_side: 1,
            tracks: vec![vec![DiskTrack {
                sectors: vec![DiskSector {
                    c: 0,
                    h: 0,
                    id: 0xc1,
                    n: 2,
                    st1: 0,
                    st2: 0,
                    data: vec![0xa5; 512],
                }],
            }]],
        };
        m.insert_disk(0, image, false);
        m.out(0xfa7e, 1);
        assert!(m.disk(0).is_some());
        assert!(m.disk_motor_on());
        if true {
            assert!(m.fdc().is_write_protected(0));
        }
        m.insert_tape(vec![TapeSpan::Pulse(1000)]);
        m.out(0xf782, 0x82);
        m.out(0xf600, 0x10);
        assert!(m.tape_motor_on());
        assert!(m.tape().is_playing());
        m.advance_tstates(137);
        assert!(m.tape().span_countdown() > 0);
        m.out(0xf600, 0);
        assert!(!m.tape().is_playing());
    }
    #[test]
    fn restored_partial_field_stops_at_the_same_native_boundary() {
        let mut live = boot();
        live.advance_tstates(137);
        let mut raw =
            common_amstrad_cpc::AmstradCpc::with_model(FIRMWARE, CpcModel::Cpc6128).expect("raw");
        for _ in 0..20 {
            raw.run_frame();
        }
        for _ in 0..4 {
            assert!(raw.run_native_frame().1);
        }
        raw.advance_tstates(137);
        let mut restored = AmstradCpc::from_state(raw).expect("matching board");
        for _ in 0..3 {
            let before = live.frame_count();
            assert_eq!(live.run_native_frame(), restored.run_native_frame());
            assert_eq!(live.frame_count(), before + 1);
            assert_eq!(live.cpu_tstates(), restored.cpu_tstates());
            assert_eq!(live.framebuffer(), restored.framebuffer());
            assert_eq!(live.take_audio_buffer(), restored.take_audio_buffer());
        }
        for foreign in [CpcModel::Cpc464, CpcModel::Cpc664, CpcModel::Cpc6128] {
            if foreign != CpcModel::Cpc6128 {
                assert!(
                    AmstradCpc::from_state(
                        common_amstrad_cpc::AmstradCpc::with_model(FIRMWARE, foreign)
                            .expect("foreign")
                    )
                    .is_err()
                );
            }
        }
    }
}
