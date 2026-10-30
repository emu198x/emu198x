//! Fixed Timex TC2068 board over shared wiring.
pub use common_timex_2068::memory;
use common_timex_2068::{Timex2068, TimexBoard, TimexModel};
/// Physical board selection.
pub struct Board;
impl TimexBoard for Board {
    const MODEL: TimexModel = TimexModel::TC2068;
    const MACHINE_ID: &'static str = "timex-tc2068";
}
/// This board, with no model-selection argument.
pub type TimexTC2068 = Timex2068<Board>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_timex_2068::{MemoryBus, SpectrumDriver};
    fn guest() -> TimexTC2068 {
        let mut m = TimexTC2068::new();
        let mut rom = [0; 16384];
        // DI; paint bitmap/attribute; scan row A; increment HOME RAM forever.
        let code = [
            0xf3, 0x3e, 0xff, 0x32, 0, 0x40, 0x3e, 0x47, 0x32, 0, 0x58, 0x01, 0xfe, 0xfd, 0xed,
            0x78, 0xe6, 0x1f, 0x32, 1, 0x80, 0x21, 0, 0x80, 0x34, 0xc3, 11, 0,
        ];
        rom[..code.len()].copy_from_slice(&code);
        m.memory.load_rom_data(&rom);
        m.memory.load_exrom_data(&[0xa5; 8192]);
        m
    }
    #[test]
    fn fixed_owner_keeps_its_identity_clock_and_native_boundary() {
        let mut m = guest();
        assert_eq!(m.model_id(), "timex-tc2068");
        assert_eq!(m.model(), TimexModel::TC2068);
        let frame_ticks = m.timing().halfcycles_per_frame;
        m.advance_halfcycles(frame_ticks - 1);
        assert_eq!(m.hc(), frame_ticks - 1);
        m.advance_halfcycles(1);
        assert_eq!(m.hc(), 0);
    }
    #[test]
    fn guest_executes_and_paints_standard_video() {
        let mut m = guest();
        m.run_frame();
        m.run_frame();
        assert_eq!(m.memory.read(0x4000), 0xff);
        assert_eq!(m.memory.read(0x5800), 0x47);
        assert!(
            m.framebuffer.contains(&15),
            "guest painted bright white pixels"
        );
        assert!(m.z80.instructions_retired() > 10);
    }
    #[test]
    fn guest_keyboard_scan_tracks_press_and_release() {
        let mut m = guest();
        m.keyboard[1] = 0xfe;
        m.run_frame();
        assert_eq!(m.memory.read(0x8001), 0x1e);
        m.keyboard[1] = 0xff;
        m.run_frame();
        assert_eq!(m.memory.read(0x8001), 0x1f);
    }
    #[test]
    fn ay_register_ports_and_beeper_remain_live() {
        let mut m = guest();
        m.port_write(0xf5, 8);
        m.port_write(0xf6, 9);
        assert_eq!(m.port_read(0xf5), 9);
        m.port_write(0xfffd, 8);
        m.port_write(0xbffd, 3);
        assert_eq!(
            m.port_read(0xf5),
            9,
            "128K AY ports do not select the Timex chip"
        );
        m.port_write(0xfe, 0x10);
        m.run_frame();
        assert!(m.audio_frame.iter().any(|s| s.abs() > 0.0), "beeper output");
        assert_eq!(m.port_read(0xf5), 9);
    }
    #[test]
    fn home_exrom_and_empty_dock_remain_distinct() {
        let mut m = guest();
        assert_eq!(m.memory.read(0), 0xf3);
        m.memory.write(0, 0);
        assert_eq!(m.memory.read(0), 0xf3, "ROM is immutable");
        m.port_write(0xff, 0x80);
        assert_eq!(m.memory.read(0), 0xa5);
        m.port_write(0xf4, 1);
        assert_eq!(m.memory.read(0), 0xff);
        m.port_write(0xf4, 0);
        m.port_write(0xff, 0);
        assert_eq!(m.memory.read(0), 0xf3);
        m.memory.write(0x9000, 0x5a);
        assert_eq!(m.memory.read(0x9000), 0x5a);
    }
    #[test]
    fn partial_fields_match_whole_fields() {
        let mut whole = guest();
        let mut partial = guest();
        whole.run_frame();
        partial.advance_halfcycles(137);
        partial.run_frame();
        assert_eq!(whole.hc(), partial.hc());
        assert_eq!(whole.framebuffer, partial.framebuffer);
        assert_eq!(whole.z80.regs.pc, partial.z80.regs.pc);
        assert_eq!(whole.memory.read(0x8000), partial.memory.read(0x8000));
        assert!(
            whole
                .audio_frame
                .iter()
                .map(|s| s.to_bits())
                .eq(partial.audio_frame.iter().map(|s| s.to_bits()))
        );
    }
    #[test]
    fn reset_keeps_this_board_and_loaded_rom_ram() {
        let mut m = guest();
        m.memory.write(0x9000, 0x5a);
        let before = (m.timing().master_hz, m.timing().halfcycles_per_frame);
        m.advance_halfcycles(137);
        m.reset();
        assert_eq!(m.model_id(), "timex-tc2068");
        assert_eq!(
            (m.timing().master_hz, m.timing().halfcycles_per_frame),
            before
        );
        assert_eq!(m.hc(), 0);
        assert_eq!(m.memory.read(0), 0xf3);
        assert_eq!(m.memory.read(0x9000), 0x5a);
    }
}
