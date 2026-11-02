//! The A2000's fitted clock must not depend on expansion RAM.
use runtime_commodore_amiga::{AmigaOcsRuntime, Model};

fn guest_clock_observation(model: Model) -> (u16, bool) {
    let mut rom = vec![0; 512 * 1024];
    rom[..4].copy_from_slice(&0x0003_F000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    let mut code = Vec::new();
    fn immediate(code: &mut Vec<u8>, opcode: u16, value: u16, address: u32) {
        code.extend(opcode.to_be_bytes());
        code.extend(value.to_be_bytes());
        code.extend(address.to_be_bytes());
    }
    // Remove the reset overlay through the real CIA-A registers.
    immediate(&mut code, 0x13FC, 3, 0x00BF_E201);
    immediate(&mut code, 0x13FC, 2, 0x00BF_E001);
    // Set HOLD, write seconds, then read through the real clock aperture.
    immediate(&mut code, 0x33FC, 1, 0x00DC_0034);
    immediate(&mut code, 0x33FC, 5, 0x00DC_0000);
    code.extend(0x33F9u16.to_be_bytes()); // MOVE.W $DC0000,$1000
    code.extend(0x00DC_0000u32.to_be_bytes());
    code.extend(0x0000_1000u32.to_be_bytes());
    immediate(&mut code, 0x33FC, 0x51A7, 0x0000_1002);
    code.extend(0x60FEu16.to_be_bytes());
    rom[8..8 + code.len()].copy_from_slice(&code);

    let mut runtime = AmigaOcsRuntime::new(model, rom).expect("owned guest firmware");
    for _ in 0..10_000 {
        runtime.machine_mut().tick();
    }
    assert_eq!(
        runtime.machine().read_word(0x1002),
        0x51A7,
        "guest must complete its register writes and readback"
    );
    (
        runtime.machine().read_word(0x1000) & 15,
        runtime.machine().gary().rtc_present(),
    )
}

#[test]
fn a2000_pal_onboard_clock_works_without_slow_ram() {
    assert_eq!(Model::A2000OcsPal.ram_config().slow_kb, 0);
    assert_eq!(guest_clock_observation(Model::A2000OcsPal), (5, true));
}

#[test]
fn a2000_ntsc_onboard_clock_works_without_slow_ram() {
    assert_eq!(Model::A2000OcsNtsc.ram_config().slow_kb, 0);
    assert_eq!(guest_clock_observation(Model::A2000OcsNtsc), (5, true));
}

#[test]
fn a501_clock_is_a_guest_visible_positive_control() {
    for model in [Model::A500OcsPalA501, Model::A500OcsNtscA501] {
        assert_eq!(guest_clock_observation(model), (5, true));
    }
}

#[test]
fn unexpanded_a500_does_not_gain_an_unfitted_clock() {
    for model in [Model::A500OcsPal, Model::A500OcsNtsc] {
        assert_eq!(guest_clock_observation(model), (15, false));
    }
}
