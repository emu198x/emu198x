//! Fixed Atari 520st-pal configuration over common ST wiring.
pub use common_atari_st::{Model, Monitor, VideoStandard};
/// Immutable model and master oscillator selection.
pub struct Board;
impl common_atari_st::StBoard for Board {
    const MODEL: Model = Model::St520;
    const STANDARD: VideoStandard = VideoStandard::Pal;
    const MACHINE_ID: &'static str = "atari-520st-pal";
}
/// This fixed board with a selectable monitor and external TOS.
pub type AtariSt = common_atari_st::StCore<Board>;

#[cfg(test)]
mod tests {
    use super::*;
    fn tos() -> Vec<u8> {
        let mut rom = vec![0; 0x30000];
        rom[..4].copy_from_slice(&0x8000_u32.to_be_bytes());
        rom[4..8].copy_from_slice(&0xfc0008_u32.to_be_bytes());
        rom[8..20].copy_from_slice(&[0x46, 0xfc, 0x27, 0, 0x52, 0x79, 0, 1, 0, 0, 0x60, 0xf8]);
        rom
    }
    fn machine(monitor: Monitor) -> AtariSt {
        let mut m = AtariSt::new(monitor, tos()).expect("project TOS");
        m.set_memory_config(m.glue().matching_config());
        m.write_glue(0x20a, 2);
        m
    }
    fn picture(monitor: Monitor) -> AtariSt {
        let mut m = machine(monitor);
        m.set_resolution(if monitor == Monitor::Mono { 2 } else { 0 });
        m.write_glue(0x201, 2);
        m.write_glue(0x203, 0);
        for i in 0..16 {
            m.write_palette(i, if i == 0 { 0 } else { 0x777 });
        }
        for addr in 0x20000..0x28000 {
            m.poke(addr, if addr & 1 == 0 { 0x5a } else { 0xa5 });
        }
        m.run_frame();
        m.run_frame();
        m
    }
    #[test]
    fn owner_fixes_identity_clock_and_physical_banks() {
        let m = machine(Monitor::Colour);
        assert_eq!(m.board_id(), "atari-520st-pal");
        assert_eq!(m.master_hz(), 32_084_988);
        assert_eq!(m.cpu_hz(), 32_084_988 / 4);
        assert_eq!(m.ram().len(), 524288);
        assert_eq!(m.glue().physical_banks(), [524288, 0]);
        assert!(AtariSt::new(Monitor::Colour, vec![0; 100]).is_err());
    }
    #[test]
    fn guest_runs_from_tos_vectors_and_changes_mapped_ram() {
        let mut m = machine(Monitor::Colour);
        assert_eq!(m.cpu().regs.pc, 0xfc0008);
        assert_eq!(m.peek(0x10000), 0);
        assert_eq!(m.peek(0x10001), 0);
        for _ in 0..10000 {
            m.tick();
        }
        assert!(u16::from_be_bytes([m.peek(0x10000), m.peek(0x10001)]) > 0);
        assert!(m.cpu().instruction_starts > 10);
    }
    #[test]
    fn native_fields_follow_software_selected_video_timing() {
        let mut m = machine(Monitor::Colour);
        m.run_frame();
        let before = m.glue().frames();
        assert_eq!(m.run_frame(), 313 * 512);
        assert_eq!(m.glue().frames(), before + 1);
        m.write_glue(0x20a, 2);
        m.run_frame();
        assert_eq!(m.run_frame(), 313 * 512);
        m.write_glue(0x20a, 0);
        m.run_frame();
        assert_eq!(m.run_frame(), 263 * 508);
        assert_eq!(m.master_hz(), 32_084_988);
    }
    #[test]
    fn colour_and_monochrome_monitors_render_their_own_live_pixels() {
        for (monitor, width, height) in [(Monitor::Colour, 768, 258), (Monitor::Mono, 640, 400)] {
            let m = picture(monitor);
            assert_eq!((m.frame_width(), m.frame_height()), (width, height));
            assert!(m.framebuffer().contains(&0));
            assert!(m.framebuffer().contains(&0xffffff));
            assert_eq!(
                m.mfp().peek(0) & 0x80,
                if monitor == Monitor::Mono { 0 } else { 0x80 }
            );
        }
    }
    #[test]
    fn ym2149_sound_changes_and_muting_stops_it() {
        let mut m = machine(Monitor::Colour);
        for (reg, val) in [(0, 40), (1, 0), (7, 0x3e), (8, 15)] {
            m.select_psg_register(reg);
            m.write_psg_data(val);
        }
        m.run_frame();
        let audio = m.take_audio();
        assert!(audio.iter().any(|s| s.abs() > 0.001));
        assert!(audio.windows(2).any(|p| p[0] != p[1]));
        m.select_psg_register(8);
        m.write_psg_data(0);
        m.run_frame();
        m.take_audio();
        m.run_frame();
        assert!(m.take_audio().iter().all(|s| *s == 0.0));
    }
    #[test]
    fn key_press_and_release_cross_the_keyboard_acia() {
        let mut m = machine(Monitor::Colour);
        m.write_keyboard_acia(false, 0x96);
        m.key(0x1e, true);
        m.run_frame();
        assert_eq!(m.read_keyboard_acia(true), 0x1e);
        m.key(0x1e, false);
        m.run_frame();
        assert_eq!(m.read_keyboard_acia(true), 0x9e);
    }
    #[test]
    fn restored_board_and_reset_keep_external_tos_and_discs() {
        let mut raw =
            common_atari_st::AtariSt::new(Model::St520, VideoStandard::Pal, Monitor::Colour, tos())
                .expect("raw");
        raw.glue.set_config(raw.glue.matching_config());
        raw.glue.write(0x20a, 2);
        let disk = common_atari_st::disk_from_st(&vec![0xa5; 368640]).expect("disk");
        raw.insert_disk(0, disk);
        raw.run_frame();
        for _ in 0..137 {
            raw.tick();
        }
        let clock = raw.cycles();
        let mut m = AtariSt::from_state(raw, tos()).expect("bind");
        assert_eq!(m.cycles(), clock);
        assert_eq!(m.rom(), tos());
        m.run_frame();
        m.reset();
        assert_eq!(m.cpu().regs.pc, 0xfc0008);
        assert!(
            m.disk(0)
                .expect("disk retained")
                .data()
                .iter()
                .all(|b| *b == 0xa5)
        );
        let foreign = common_atari_st::AtariSt::new(
            Model::St1040,
            VideoStandard::Pal,
            Monitor::Colour,
            tos(),
        )
        .expect("foreign");
        assert!(AtariSt::from_state(foreign, tos()).is_err());
    }
}
