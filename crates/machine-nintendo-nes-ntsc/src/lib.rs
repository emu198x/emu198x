//! One fixed NTSC NES board.
pub use common_nintendo_nes::{ApuChannel, AudioControls, InputDevices, NesSnapshot, Region};
/// Immutable physical board selection.
pub struct Board;
impl common_nintendo_nes::NesBoard for Board {
    const REGION: Region = Region::Ntsc;
    const MACHINE_ID: &'static str = "nintendo-nes-ntsc";
}
/// This physical board with its fixed chips and clock tree.
pub type Nes = common_nintendo_nes::NesCore<Board>;

#[cfg(test)]
mod tests {
    use super::*;
    use common_nintendo_nes::cartridge::{Mapper, Mirroring, Nrom, parse_ines};
    fn mapper(program: &[u8]) -> Box<dyn Mapper> {
        let mut prg = vec![0xea; 16384];
        prg[..program.len()].copy_from_slice(program);
        prg[0x3ffc..0x3ffe].copy_from_slice(&[0, 0x80]);
        Box::new(Nrom::new(prg, vec![], Mirroring::Horizontal))
    }
    fn picture() -> Nes {
        let mut rom =
            include_bytes!("../../../test-data/synthetic-cartridges/nintendo-nes-logo.nes")
                .to_vec();
        let vector = 16 + usize::from(rom[4]) * 16384 - 4;
        let init = [
            0xa9,
            1,
            0x8d,
            0x15,
            0x40,
            0xa9,
            0xbf,
            0x8d,
            0,
            0x40,
            0xa9,
            0x40,
            0x8d,
            2,
            0x40,
            0xa9,
            8,
            0x8d,
            3,
            0x40,
            0x4c,
            rom[vector],
            rom[vector + 1],
        ];
        assert!(
            rom[16 + 0x3f00..16 + 0x3f00 + init.len()]
                .iter()
                .all(|b| *b == 0)
        );
        rom[16 + 0x3f00..16 + 0x3f00 + init.len()].copy_from_slice(&init);
        rom[vector..vector + 2].copy_from_slice(&[0, 0xbf]);
        let mut m = Nes::new(parse_ines(&rom).expect("our cartridge").mapper);
        for _ in 0..8 {
            m.run_native_frame();
        }
        m.take_audio_buffer();
        m
    }
    #[test]
    fn guest_boot_paints_through_this_boards_chips() {
        let m = picture();
        assert_eq!(m.board_id(), "nintendo-nes-ntsc");
        assert_eq!(m.region(), Region::Ntsc);
        assert_eq!(m.ppu().master_divider(), m.region().dot_units());
        assert_eq!(m.ppu().pre_render_line(), m.region().pre_render_line());
        assert_eq!(m.apu().region(), m.region().apu_region());
        assert!(m.framebuffer().iter().any(|p| *p != m.framebuffer()[0]));
    }
    #[test]
    fn guest_pulse_audio_continues_bit_identically_after_restore() {
        let mut live = picture();
        for _ in 0..20_137 {
            live.tick();
        }
        live.take_audio_buffer();
        let mut restored = Nes::from_snapshot(live.snapshot()).expect("same board");
        for _ in 0..3 {
            assert_eq!(live.run_native_frame(), restored.run_native_frame());
            let a = live.take_audio_buffer();
            let b = restored.take_audio_buffer();
            assert!(a.iter().any(|s| s.abs() > 0.0001));
            assert_eq!(
                a.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
                b.iter().map(|s| s.to_bits()).collect::<Vec<_>>()
            );
            assert_eq!(live.ppu().signal_codes(), restored.ppu().signal_codes());
            assert_eq!(live.framebuffer(), restored.framebuffer());
        }
    }
    #[test]
    fn guest_reads_both_pads_and_a_release_keeps_the_other_pad_held() {
        let code = [
            0xa9, 1, 0x8d, 0x16, 0x40, 0xa9, 0, 0x8d, 0x16, 0x40, 0xad, 0x16, 0x40, 0x29, 1, 0x85,
            1, 0xad, 0x17, 0x40, 0x29, 1, 0x85, 2, 0x4c, 0, 0x80,
        ];
        let mut m = Nes::new(mapper(&code));
        m.set_controller1(1);
        m.set_controller2(1);
        m.run_frame();
        assert_eq!((m.peek(1), m.peek(2)), (1, 1));
        let mut resumed = Nes::from_snapshot(m.snapshot()).expect("held controls");
        resumed.set_controller1(0);
        resumed.run_frame();
        assert_eq!((resumed.peek(1), resumed.peek(2)), (0, 1));
    }
    #[test]
    fn cpu_ram_mirrors_and_rom_writes_keep_the_board_map() {
        let mut m = Nes::new(mapper(&[0x4c, 0, 0x80]));
        m.poke(0x125, 0x5a);
        for addr in [0x125, 0x925, 0x1125, 0x1925] {
            assert_eq!(m.peek(addr), 0x5a);
        }
        m.poke(0x8000, 0);
        assert_eq!(m.peek(0x8000), 0x4c);
    }
    #[test]
    fn native_frame_finishes_at_the_observed_raster_wrap() {
        let mut m = picture();
        for _ in 0..137 {
            m.tick();
        }
        let mut control = Nes::from_snapshot(m.snapshot()).expect("control");
        let start = control.master_clock();
        let mut wrapped = false;
        for _ in 0..200_000 {
            let before = control.ppu().scanline();
            control.tick();
            if before == control.ppu().pre_render_line() && control.ppu().scanline() == 0 {
                wrapped = true;
                break;
            }
        }
        assert!(wrapped, "control must observe a real boundary");
        assert_eq!(m.run_native_frame(), (control.master_clock() - start, true));
        assert_eq!(m.framebuffer(), control.framebuffer());
        assert_eq!(m.cpu().regs.pc, control.cpu().regs.pc);
    }
    #[test]
    fn soft_reset_and_in_place_restore_keep_this_board_and_devices() {
        let mut m = picture();
        m.set_input_devices(InputDevices::FourScore);
        m.set_controller3(0x44);
        m.poke(0x125, 0x5a);
        let saved = m.snapshot();
        let mut expected = Nes::from_snapshot(saved.clone()).expect("control");
        m.poke(0x125, 0);
        m.restore_snapshot(saved).expect("in-place restore");
        assert_eq!(m.peek(0x125), 0x5a);
        assert_eq!(m.input_devices(), InputDevices::FourScore);
        assert_eq!(m.controller3_state(), 0x44);
        assert_eq!(m.run_native_frame(), expected.run_native_frame());
        assert_eq!(m.framebuffer(), expected.framebuffer());
        let rom = m.peek(0x8000);
        m.soft_reset();
        assert_eq!(m.peek(0x8000), rom);
        assert_eq!(m.peek(0x125), 0x5a);
        assert_eq!(m.board_id(), "nintendo-nes-ntsc");
    }
    #[test]
    fn foreign_board_state_is_rejected_before_live_hardware_changes() {
        let foreign =
            common_nintendo_nes::Nes::new_with_region(mapper(&[0x4c, 0, 0x80]), Region::Pal)
                .snapshot();
        assert!(Nes::from_snapshot(foreign.clone()).is_err());
        let mut m = picture();
        m.poke(0x125, 0x5a);
        let before = (m.master_clock(), m.cpu().regs.pc, m.framebuffer().to_vec());
        assert!(m.restore_snapshot(foreign).is_err());
        assert_eq!(
            (m.master_clock(), m.cpu().regs.pc, m.framebuffer().to_vec()),
            before
        );
        assert_eq!(m.peek(0x125), 0x5a);
    }
}
