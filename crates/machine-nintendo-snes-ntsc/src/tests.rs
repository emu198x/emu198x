use super::*;
use common_nintendo_snes::{Buttons, Cartridge, Map, Region, Snes as Raw};
fn machine(video: bool) -> Snes {
    let mut rom = vec![0; 0x8000];
    let mut code = vec![0xa9, 0x42, 0x8d, 0, 2];
    if video {
        for (a, v) in [(0x2100u16, 15), (0x2121, 0), (0x2122, 0xe0), (0x2122, 3)] {
            code.extend([0xa9, v, 0x8d, a as u8, (a >> 8) as u8]);
        }
    }
    let target = 0x8000 + code.len() as u16;
    code.extend([0x4c, target as u8, (target >> 8) as u8]);
    rom[..code.len()].copy_from_slice(&code);
    rom[0x7ffc..0x7ffe].copy_from_slice(&0x8000u16.to_le_bytes());
    Snes::new(Cartridge::from_rom(rom, Map::LoRom, 2048), None)
}
#[test]
fn fixed_region_and_ppu_agree() {
    let m = machine(false);
    assert_eq!(m.region(), Region::Ntsc);
    assert_eq!(m.master_hz(), 21477273);
    assert_eq!(format!("{:?}", m.ppu.region()), "Ntsc");
}
#[test]
fn guest_reset_vector_executes() {
    let mut m = machine(false);
    for _ in 0..1000 {
        m.tick();
    }
    assert_eq!(m.peek(0x200), 0x42);
}
#[test]
fn native_boundaries_agree_with_the_master_clock() {
    let mut m = machine(false);
    m.run_frame();
    let before = m.mclk();
    let f = m.ppu.frames();
    let clocks = m.run_frame();
    assert_eq!(m.mclk() - before, clocks);
    assert_eq!(m.ppu.frames(), f + 1);
    assert!((262 * 1364 - 8..=262 * 1364 + 8).contains(&clocks));
}
#[test]
fn guest_video_writes_light_the_screen_and_control_stays_dark() {
    let mut control = machine(false);
    control.run_frame();
    control.run_frame();
    assert!(control.framebuffer().iter().all(|p| *p == 0xff00_0000));
    let mut m = machine(true);
    m.run_frame();
    m.run_frame();
    assert!(m.framebuffer().iter().any(|p| *p & 0xffffff != 0));
}
#[test]
fn dsp_noise_reaches_the_audio_output() {
    let mut m = machine(false);
    for (a, v) in [
        (0x0c, 127),
        (0x1c, 127),
        (0, 127),
        (1, 127),
        (5, 0),
        (7, 127),
        (0x6c, 0x1f),
        (0x3d, 1),
        (0x5c, 0),
        (0x4c, 1),
    ] {
        m.apu_mut().dsp.write(a, v);
    }
    m.run_frame();
    m.run_frame();
    let sound = m.take_audio();
    assert!(sound.iter().any(|v| *v > 0.0));
    assert!(sound.iter().any(|v| *v < 0.0));
}
#[test]
fn held_pad_sram_and_queued_audio_survive_exact_restore() {
    let mut m = machine(true);
    m.run_frame();
    m.set_buttons(
        0,
        Buttons {
            b: true,
            r: true,
            ..Buttons::default()
        },
    );
    m.pads_mut()[0].latch(true);
    m.pads_mut()[0].latch(false);
    m.pads_mut()[0].data();
    m.poke(0x700000, 0xa5);
    for _ in 0..137 {
        m.tick();
    }
    let bytes = postcard::to_allocvec(&*m).expect("save");
    let mut copy = Snes::from_state(postcard::from_bytes(&bytes).expect("decode")).expect("bind");
    assert_eq!(copy.peek(0x700000), 0xa5);
    assert_eq!(m.take_audio(), copy.take_audio());
    for _ in 0..3 {
        m.run_frame();
        copy.run_frame();
        assert!(
            postcard::to_allocvec(&*m).expect("live")
                == postcard::to_allocvec(&*copy).expect("copy")
        );
    }
}
#[test]
fn foreign_board_and_malformed_firmware_are_refused() {
    let m = machine(false);
    let bytes = postcard::to_allocvec(&*m).expect("save");
    let mut raw: Raw = postcard::from_bytes(&bytes).expect("raw");
    raw.apu.ipl.pop();
    assert!(Snes::from_state(raw).is_err());
    let foreign = Raw::new(
        Cartridge::from_rom(vec![0; 0x8000], Map::LoRom, 0),
        Region::Pal,
        None,
    );
    assert!(Snes::from_state(foreign).is_err());
}
