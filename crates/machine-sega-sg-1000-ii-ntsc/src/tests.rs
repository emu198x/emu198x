use super::*;
use common_sega_sg_1000::{Sg1000 as Raw, Sg1000Region};
fn cartridge(active: bool) -> Vec<u8> {
    let mut rom = vec![0x76; 8192];
    rom[..3].copy_from_slice(&[0xc3, 0, 1]);
    rom[0x66..0x6d].copy_from_slice(&[0x3e, 0xa7, 0x32, 3, 0xc0, 0xed, 0x45]);
    let mut code = vec![0xf3, 0x31, 0xff, 0xc3, 0x3e, 0x42, 0x32, 0, 0xc0];
    fn out(code: &mut Vec<u8>, port: u8, value: u8) {
        code.extend([0x3e, value, 0xd3, port]);
    }
    if active {
        for (r, v) in [(1, 0x40), (2, 0x0e), (3, 0x80), (4, 0), (7, 4)] {
            out(&mut code, 0xbf, v);
            out(&mut code, 0xbf, 0x80 | r);
        }
        out(&mut code, 0xbf, 0);
        out(&mut code, 0xbf, 0x40);
        for _ in 0..8 {
            out(&mut code, 0xbe, 0xff);
        }
        out(&mut code, 0xbf, 0);
        out(&mut code, 0xbf, 0x60);
        out(&mut code, 0xbe, 0xf2);
        for v in [0x80, 0x10, 0x90] {
            out(&mut code, 0x7f, v);
        }
    }
    let entry = 0x100 + code.len() as u16;
    code.extend([
        0xdb,
        0xdd,
        0x32,
        1,
        0xc0,
        0xc3,
        entry as u8,
        (entry >> 8) as u8,
    ]);
    rom[0x100..0x100 + code.len()].copy_from_slice(&code);
    rom
}
#[test]
fn owner_selects_fixed_clock_and_video() {
    let m = Sg1000::new(cartridge(false));
    assert_eq!(MACHINE_ID, "sega-sg-1000-ii-ntsc");
    assert_eq!(m.region(), Sg1000Region::Ntsc);
    assert_eq!(m.region().cpu_hz(), 3579545);
    assert_eq!(m.framebuffer_height(), 240);
}
#[test]
fn reset_vector_runs_the_guest_program() {
    let mut m = Sg1000::new(cartridge(false));
    assert!(m.run_native_frame().1);
    assert_eq!(m.peek(0xc000), 0x42);
}
#[test]
fn one_kib_ram_is_mirrored_through_the_upper_address_window() {
    let mut m = Sg1000::new(cartridge(false));
    m.poke(0xc123, 0x5a);
    for address in [0xc123, 0xc523, 0xe123, 0xfd23] {
        assert_eq!(m.peek(address), 0x5a);
    }
}
#[test]
fn native_wrap_reports_the_actual_elapsed_cpu_clocks() {
    let mut m = Sg1000::new(cartridge(false));
    m.run_native_frame();
    for _ in 0..4 {
        let before = m.cpu_tstates();
        let count = m.frame_count();
        let (ticks, complete) = m.run_native_frame();
        assert!(complete);
        assert_eq!(m.cpu_tstates() - before, ticks);
        assert_eq!(m.frame_count(), count + 1);
        assert_eq!(m.vdp().scanline(), 0);
        assert!(ticks.abs_diff(59736) <= 1);
    }
}
#[test]
fn guest_video_and_sound_differ_from_the_inactive_control() {
    let mut control = Sg1000::new(cartridge(false));
    control.run_native_frame();
    control.run_native_frame();
    assert!(control.framebuffer().iter().all(|p| *p == 0xff000000));
    assert!(control.take_audio_buffer().iter().all(|v| v.abs() < 0.001));
    let mut m = Sg1000::new(cartridge(true));
    m.run_native_frame();
    m.run_native_frame();
    assert!(m.framebuffer().iter().any(|p| *p != 0xff000000));
    assert!(m.take_audio_buffer().iter().any(|v| v.abs() > 0.001));
}
#[test]
fn held_input_nmi_and_queued_output_continue_after_restore() {
    let mut m = Sg1000::new(cartridge(true));
    m.run_native_frame();
    m.controller2_mut().right = true;
    m.controller2_mut().button2 = true;
    m.set_pause_pressed(true);
    m.run_native_frame();
    assert_eq!(m.peek(0xc003), 0xa7);
    assert_eq!(m.peek(0xc001) & 0x0a, 0);
    let bytes = postcard::to_allocvec(&*m).expect("save");
    let mut copy = Sg1000::from_state(postcard::from_bytes(&bytes).expect("raw")).expect("bind");
    assert_eq!(m.take_audio_buffer(), copy.take_audio_buffer());
    for _ in 0..3 {
        m.run_native_frame();
        copy.run_native_frame();
        assert!(
            postcard::to_allocvec(&*m).expect("live")
                == postcard::to_allocvec(&*copy).expect("restored")
        );
    }
    copy.controller2_mut().right = false;
    copy.controller2_mut().button2 = false;
    copy.set_pause_pressed(false);
    copy.run_native_frame();
    assert_eq!(copy.peek(0xc001) & 0x0a, 0x0a);
}
#[test]
fn a_foreign_video_board_cannot_be_bound() {
    let raw = Raw::new(cartridge(false), Sg1000Region::Pal);
    assert!(Sg1000::from_state(raw).is_err());
}
