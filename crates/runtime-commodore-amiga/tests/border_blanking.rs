//! Exercise Lisa border black through the production board framebuffer path.
use commodore_agnus_ocs::Agnus;
use commodore_denise_aga::DeniseAga;
use common_commodore_amiga::{DeniseChip, denise::Denise, memory::Memory};

fn fixture(ecsena: bool, blank: bool, bpl1dat: bool, sprite: bool) -> (Denise<DeniseAga>, Agnus) {
    let mut denise = Denise::<DeniseAga>::new();
    denise.ocs.write_word(0x180, 0x0F00); // visibly red COLOR00
    denise.ocs.write_word(0x1A2, 0x0F00); // sprite COLOR17
    denise.ocs.advance_color_output_samples(1);
    denise.ocs.write_word(
        0x106,
        if blank { 0x20 } else { 0 } | if sprite { 2 } else { 0 },
    );
    let mut agnus = Agnus::new();
    agnus.vpos = 50;
    agnus.hpos = 0x12;
    agnus.diwstrt = 0;
    agnus.diwstop = 0xFF;
    denise.write_word(0x08E, agnus.diwstrt);
    denise.write_word(0x090, agnus.diwstop);
    agnus.bplcon0 = 0x1000 | u16::from(ecsena);
    let memory = Memory::new(vec![0; 2]);
    let ccks = agnus.current_line_ccks();
    // This fixture jumps to a later pixel. First clock HSTART so the
    // horizontal window has actually opened before testing its vertical gate.
    agnus.hpos = 0;
    denise.tick(0, None, true, &mut agnus, &memory, ccks);
    agnus.hpos = 0x12;
    // Exercise steady border controls after their raw BPLCON0 input has
    // crossed Lisa's normal stage; one output tick no longer settles it.
    for phase in 0..2 {
        denise.tick(phase, None, true, &mut agnus, &memory, ccks);
    }
    agnus.tick_cck();
    for phase in 0..2 {
        denise.tick(phase, None, true, &mut agnus, &memory, ccks);
    }
    if bpl1dat {
        denise.ocs.enable_sprites_from_bpl1dat();
    }
    if sprite {
        denise.ocs.write_sprite_pos(0, 127); // HSTART 254, visible at 256
        denise.ocs.write_sprite_ctl(0, 0);
        denise.ocs.write_sprite_datb(0, 0);
        denise.ocs.write_sprite_data(0, 0xFFFF);
    }
    agnus.hpos = 0x80;
    (denise, agnus)
}

fn render(denise: &mut Denise<DeniseAga>, agnus: &mut Agnus, inside: bool) -> u32 {
    let memory = Memory::new(vec![0; 2]);
    let ccks = agnus.current_line_ccks();
    denise.tick(0, None, inside, agnus, &memory, ccks);
    let y = u32::from(agnus.vpos - 0x19) * 2;
    let x = u32::from(agnus.hpos - 0x2C) * 8;
    let index = (y * denise.framebuffer_size().0 + x) as usize;
    assert_eq!(denise.framebuffer[index], denise.framebuffer[index + 1]);
    denise.framebuffer[index]
}

#[test]
fn border_blank_replaces_color_zero_only_when_enabled_and_outside_display() {
    for ecsena in [false, true] {
        for blank in [false, true] {
            for bpl1dat in [false, true] {
                for inside in [false, true] {
                    let (mut denise, mut agnus) = fixture(ecsena, blank, bpl1dat, false);
                    let pixel = render(&mut denise, &mut agnus, inside);
                    let black = ecsena && blank && (!inside || !bpl1dat);
                    assert_eq!(
                        pixel,
                        if black { 0xFF00_0000 } else { 0xFFFF_0000 },
                        "ECSENA={ecsena}, BRDRBLNK={blank}, BPL1DAT={bpl1dat}, DIW={inside}"
                    );
                }
            }
        }
    }
}

#[test]
fn border_blank_masks_output_without_stalling_color_or_sprite_state() {
    let (mut blanked, mut a) = fixture(true, true, false, true);
    let mut reference = blanked.clone();
    let mut b = a.clone();
    reference.ocs.write_word(0x106, 2);
    assert_eq!(render(&mut blanked, &mut a, false), 0xFF00_0000);
    assert_eq!(render(&mut reference, &mut b, false), 0xFFFF_0000);
    assert_eq!(
        blanked.ocs.diagnostic_snapshot(),
        reference.ocs.diagnostic_snapshot()
    );
    assert_eq!(
        blanked.ocs.as_inner().as_inner().diagnostic_snapshot(),
        reference.ocs.as_inner().as_inner().diagnostic_snapshot()
    );
}
