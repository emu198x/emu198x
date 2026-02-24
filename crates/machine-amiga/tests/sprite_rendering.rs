use machine_amiga::memory::ROM_BASE;
use machine_amiga::Amiga;
use machine_amiga::TICKS_PER_CCK;

const REG_DMACON: u16 = 0x096;
const REG_DDFSTRT: u16 = 0x092;
const REG_BPLCON0: u16 = 0x100;
const REG_BPLCON2: u16 = 0x104;
const REG_SPR0PTH: u16 = 0x120;
const REG_SPR0PTL: u16 = 0x122;
const REG_SPR1PTH: u16 = 0x124;
const REG_SPR1PTL: u16 = 0x126;

const DMACON_SPREN: u16 = 0x0020;
const DMACON_DMAEN: u16 = 0x0200;

const DISPLAY_VSTART: u16 = 0x2C;
const TARGET_HPOS: u16 = 0x14; // Beam X = 40 pixels

fn rgb12_to_argb32(rgb12: u16) -> u32 {
    let r = ((rgb12 >> 8) & 0xF) as u8;
    let g = ((rgb12 >> 4) & 0xF) as u8;
    let b = (rgb12 & 0xF) as u8;
    let r8 = (r << 4) | r;
    let g8 = (g << 4) | g;
    let b8 = (b << 4) | b;
    0xFF000000 | (u32::from(r8) << 16) | (u32::from(g8) << 8) | u32::from(b8)
}

fn make_test_amiga() -> Amiga {
    let mut rom = vec![0u8; 256 * 1024];

    let ssp = 0x0007FFF0u32;
    rom[0] = (ssp >> 24) as u8;
    rom[1] = (ssp >> 16) as u8;
    rom[2] = (ssp >> 8) as u8;
    rom[3] = ssp as u8;

    let pc = ROM_BASE + 8;
    rom[4] = (pc >> 24) as u8;
    rom[5] = (pc >> 16) as u8;
    rom[6] = (pc >> 8) as u8;
    rom[7] = pc as u8;

    // BRA.S *
    rom[8] = 0x60;
    rom[9] = 0xFE;

    Amiga::new(rom)
}

fn tick_ccks(amiga: &mut Amiga, ccks: u32) {
    for _ in 0..ccks {
        for _ in 0..TICKS_PER_CCK {
            amiga.tick();
        }
    }
}

fn tick_until_vh(amiga: &mut Amiga, vpos: u16, hpos: u16, max_ccks: u32) {
    for _ in 0..max_ccks {
        if amiga.agnus.vpos == vpos && amiga.agnus.hpos == hpos {
            return;
        }
        tick_ccks(amiga, 1);
    }
    panic!(
        "timed out waiting for beam vpos=${vpos:02X} hpos=${hpos:02X}; got vpos=${:02X} hpos=${:02X}",
        amiga.agnus.vpos, amiga.agnus.hpos
    );
}

fn encode_sprite_pos_ctl(x: u16, vstart: u16, vstop: u16) -> (u16, u16) {
    let pos = ((vstart & 0x00FF) << 8) | ((x >> 1) & 0x00FF);
    let ctl =
        ((vstop & 0x00FF) << 8) | (((vstart >> 8) & 1) << 2) | (((vstop >> 8) & 1) << 1) | (x & 1);
    (pos, ctl)
}

fn write_word(amiga: &mut Amiga, addr: u32, val: u16) {
    amiga.memory.write_byte(addr, (val >> 8) as u8);
    amiga.memory.write_byte(addr + 1, val as u8);
}

fn sprite_target_fb_coords() -> (usize, usize) {
    // ddfstrt is set to 0 in these tests, and machine-amiga maps fb_x from
    // hpos using first_pixel_cck = ddfstrt + 8.
    let fb_x = usize::from((TARGET_HPOS - 8) * 2);
    let fb_y = 1usize; // We render on line DISPLAY_VSTART + 1
    (fb_x, fb_y)
}

fn setup_sprite_render_baseline(amiga: &mut Amiga) {
    amiga.write_custom_reg(REG_DDFSTRT, 0);
    amiga.agnus.vpos = DISPLAY_VSTART;
    amiga.agnus.hpos = 0x0A; // just before sprite 0 slot pair
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_SPREN);
}

fn run_to_render_cck(amiga: &mut Amiga) {
    tick_until_vh(amiga, DISPLAY_VSTART + 1, TARGET_HPOS, 1024);
}

#[test]
fn sprite0_dma_renders_pixel_into_framebuffer() {
    let mut amiga = make_test_amiga();
    let spr0_addr = 0x0000_3000u32;
    let beam_x = u16::from(TARGET_HPOS) * 2;
    let (pos, ctl) = encode_sprite_pos_ctl(beam_x, DISPLAY_VSTART, DISPLAY_VSTART + 2);

    write_word(&mut amiga, spr0_addr, pos);
    write_word(&mut amiga, spr0_addr + 2, ctl);
    write_word(&mut amiga, spr0_addr + 4, 0x8000); // color code 1 at leftmost pixel
    write_word(&mut amiga, spr0_addr + 6, 0x0000);

    amiga.write_custom_reg(REG_SPR0PTH, (spr0_addr >> 16) as u16);
    amiga.write_custom_reg(REG_SPR0PTL, (spr0_addr & 0xFFFF) as u16);
    amiga.denise.set_palette(17, 0xF00);

    setup_sprite_render_baseline(&mut amiga);
    run_to_render_cck(&mut amiga);
    tick_ccks(&mut amiga, 1);

    let (fb_x, fb_y) = sprite_target_fb_coords();
    assert_eq!(
        amiga.framebuffer()[fb_y * 320 + fb_x],
        rgb12_to_argb32(0xF00),
        "sprite pixel should appear at the expected framebuffer location"
    );
}

#[test]
fn attached_sprite_pair_renders_4bit_color_at_machine_level() {
    let mut amiga = make_test_amiga();
    let spr0_addr = 0x0000_3000u32;
    let spr1_addr = 0x0000_3040u32;
    let beam_x = u16::from(TARGET_HPOS) * 2;
    let (pos, ctl) = encode_sprite_pos_ctl(beam_x, DISPLAY_VSTART, DISPLAY_VSTART + 2);

    write_word(&mut amiga, spr0_addr, pos);
    write_word(&mut amiga, spr0_addr + 2, ctl);
    write_word(&mut amiga, spr0_addr + 4, 0x8000); // even code = 01
    write_word(&mut amiga, spr0_addr + 6, 0x0000);

    write_word(&mut amiga, spr1_addr, pos);
    write_word(&mut amiga, spr1_addr + 2, ctl | 0x0080); // ATTACH on odd sprite
    write_word(&mut amiga, spr1_addr + 4, 0x0000);
    write_word(&mut amiga, spr1_addr + 6, 0x8000); // odd code = 10 => combined 1001 => COLOR25

    amiga.write_custom_reg(REG_SPR0PTH, (spr0_addr >> 16) as u16);
    amiga.write_custom_reg(REG_SPR0PTL, (spr0_addr & 0xFFFF) as u16);
    amiga.write_custom_reg(REG_SPR1PTH, (spr1_addr >> 16) as u16);
    amiga.write_custom_reg(REG_SPR1PTL, (spr1_addr & 0xFFFF) as u16);
    amiga.denise.set_palette(25, 0x0F0);

    setup_sprite_render_baseline(&mut amiga);
    run_to_render_cck(&mut amiga);
    tick_ccks(&mut amiga, 1);

    let (fb_x, fb_y) = sprite_target_fb_coords();
    assert_eq!(
        amiga.framebuffer()[fb_y * 320 + fb_x],
        rgb12_to_argb32(0x0F0),
        "attached sprite pair should use the 4-bit combined sprite color"
    );
}

fn render_sprite_vs_playfield_pixel(pf1_priority_pos: u16) -> u32 {
    let mut amiga = make_test_amiga();
    let spr0_addr = 0x0000_3000u32;
    let beam_x = u16::from(TARGET_HPOS) * 2;
    let (pos, ctl) = encode_sprite_pos_ctl(beam_x, DISPLAY_VSTART, DISPLAY_VSTART + 2);

    write_word(&mut amiga, spr0_addr, pos);
    write_word(&mut amiga, spr0_addr + 2, ctl);
    write_word(&mut amiga, spr0_addr + 4, 0x8000);
    write_word(&mut amiga, spr0_addr + 6, 0x0000);

    amiga.write_custom_reg(REG_SPR0PTH, (spr0_addr >> 16) as u16);
    amiga.write_custom_reg(REG_SPR0PTL, (spr0_addr & 0xFFFF) as u16);
    amiga.write_custom_reg(REG_BPLCON2, pf1_priority_pos);
    amiga.denise.set_palette(1, 0x00F); // playfield color
    amiga.denise.set_palette(17, 0xF00); // sprite color

    setup_sprite_render_baseline(&mut amiga);
    run_to_render_cck(&mut amiga);

    // Seed one nonzero playfield pixel into Denise's shifter for the left pixel
    // of the target CCK. This keeps the test focused on machine integration of
    // beam mapping + sprite DMA + Denise priority composition.
    amiga.denise.bpl_shift[0] = 0x8000;
    amiga.denise.shift_count = 1;
    tick_ccks(&mut amiga, 1);

    let (fb_x, fb_y) = sprite_target_fb_coords();
    amiga.framebuffer()[fb_y * 320 + fb_x]
}

#[test]
fn bplcon2_priority_affects_sprite_visibility_at_machine_level() {
    let hidden = render_sprite_vs_playfield_pixel(0x0000); // PF1P=0 => PF1 in front of all sprite groups
    let shown = render_sprite_vs_playfield_pixel(0x0001); // PF1P=1 => SP01 in front of PF1

    assert_eq!(hidden, rgb12_to_argb32(0x00F));
    assert_eq!(shown, rgb12_to_argb32(0xF00));
}

fn render_dual_playfield_sprite_priority_pixel(bplcon2: u16) -> u32 {
    let mut amiga = make_test_amiga();
    let spr0_addr = 0x0000_3000u32;
    let beam_x = u16::from(TARGET_HPOS) * 2;
    let (pos, ctl) = encode_sprite_pos_ctl(beam_x, DISPLAY_VSTART, DISPLAY_VSTART + 2);

    write_word(&mut amiga, spr0_addr, pos);
    write_word(&mut amiga, spr0_addr + 2, ctl);
    write_word(&mut amiga, spr0_addr + 4, 0x8000);
    write_word(&mut amiga, spr0_addr + 6, 0x0000);

    amiga.write_custom_reg(REG_SPR0PTH, (spr0_addr >> 16) as u16);
    amiga.write_custom_reg(REG_SPR0PTL, (spr0_addr & 0xFFFF) as u16);
    amiga.write_custom_reg(REG_BPLCON0, 0x0400); // DBLPF
    amiga.write_custom_reg(REG_BPLCON2, bplcon2);
    amiga.denise.set_palette(1, 0x00F); // PF1 color
    amiga.denise.set_palette(9, 0x0F0); // PF2 color
    amiga.denise.set_palette(17, 0xF00); // sprite color

    setup_sprite_render_baseline(&mut amiga);
    run_to_render_cck(&mut amiga);

    // Dual playfields active on this pixel: PF1 code 1 (plane 1) and PF2 code 1 (plane 2).
    amiga.denise.bpl_shift[0] = 0x8000; // BPL1 -> PF1 color 1
    amiga.denise.bpl_shift[1] = 0x8000; // BPL2 -> PF2 color 9
    amiga.denise.shift_count = 1;
    tick_ccks(&mut amiga, 1);

    let (fb_x, fb_y) = sprite_target_fb_coords();
    amiga.framebuffer()[fb_y * 320 + fb_x]
}

#[test]
fn dual_playfield_pf2pri_and_pf2p_priority_affect_sprite_visibility() {
    let hidden = render_dual_playfield_sprite_priority_pixel(0x0044);
    let shown = render_dual_playfield_sprite_priority_pixel(0x004C);

    assert_eq!(hidden, rgb12_to_argb32(0x0F0));
    assert_eq!(shown, rgb12_to_argb32(0xF00));
}
