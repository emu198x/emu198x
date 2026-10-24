//! Game Boy Color machine tests: banking, palettes, HDMA, the speed
//! switch, STOP and the boot ROM hand-over. Rendering as a whole is
//! checked by cgb-acid2 (`tests/cgb_acid2.rs`).

#![allow(clippy::unwrap_used)]

use super::*;

/// A 32 KiB ROM with `code` at `$0100` and the given CGB flag.
fn rom_with(code: &[u8], cgb_flag: u8) -> Vec<u8> {
    let mut rom = vec![0x00; 0x8000];
    rom[0x0100..0x0100 + code.len()].copy_from_slice(code);
    rom[0x0143] = cgb_flag;
    let mut checksum: u8 = 0;
    for &byte in &rom[0x0134..=0x014C] {
        checksum = checksum.wrapping_sub(byte).wrapping_sub(1);
    }
    rom[0x014D] = checksum;
    rom
}

/// `JR -2`: spin at `$0100`.
const SPIN: [u8; 2] = [0x18, 0xFE];

fn cgb(code: &[u8], cgb_flag: u8) -> GameBoy {
    let (_, gb) =
        GameBoy::from_rom_with_boot_profile(rom_with(code, cgb_flag), BootProfile::Cgb).unwrap();
    gb
}

/// A CGB-mode machine with the LCD off, so VRAM and palettes are open.
fn cgb_lcd_off() -> GameBoy {
    let mut gb = cgb(&SPIN, 0x80);
    gb.bus_write(0xFF40, 0x00);
    gb
}

#[test]
fn a_cgb_cartridge_starts_in_cgb_mode() {
    let gb = cgb(&SPIN, 0xC0);
    assert!(gb.is_cgb());
    assert!(gb.cgb_mode());
    assert_eq!(
        (gb.cpu.a, gb.cpu.f, gb.cpu.d, gb.cpu.e),
        (0x11, 0x80, 0xFF, 0x56)
    );
    assert!(gb.color_framebuffer().is_some());
}

#[test]
fn a_dmg_cartridge_runs_in_compatibility_mode_with_the_default_palettes() {
    let gb = cgb(&SPIN, 0x00);
    assert!(gb.is_cgb());
    assert!(!gb.cgb_mode());
    assert_eq!(
        (gb.cpu.a, gb.cpu.d, gb.cpu.e, gb.cpu.l),
        (0x11, 0x00, 0x08, 0x7C)
    );
    assert_eq!(gb.ppu.palette_colour(false, 0, 1), 0x1BEF);
    assert_eq!(gb.ppu.palette_colour(true, 0, 1), 0x421F);
    assert_eq!(gb.ppu.palette_colour(true, 1, 2), 0x1CF2);
}

#[test]
fn the_agb_differs_only_in_its_registers() {
    let (_, gb) =
        GameBoy::from_rom_with_boot_profile(rom_with(&SPIN, 0x80), BootProfile::Agb).unwrap();
    assert!(gb.cgb_mode());
    assert_eq!((gb.cpu.b, gb.cpu.f), (0x01, 0x00));
}

#[test]
fn a_monochrome_model_has_no_cgb_registers() {
    let (_, mut gb) = GameBoy::from_rom(rom_with(&SPIN, 0x80)).unwrap();
    assert!(!gb.is_cgb());
    assert!(gb.color_framebuffer().is_none());
    gb.bus_write(0xFF4F, 1);
    assert_eq!(gb.bus_read(0xFF4F), 0xFF);
    assert_eq!(gb.bus_read(0xFF4D), 0xFF);
    assert_eq!(gb.bus_read(0xFF70), 0xFF);
}

#[test]
fn vbk_selects_the_vram_bank_the_cpu_sees() {
    let mut gb = cgb_lcd_off();
    assert_eq!(gb.bus_read(0xFF4F), 0xFE);
    gb.bus_write(0x8000, 0x11);
    gb.bus_write(0xFF4F, 0xFF);
    assert_eq!(gb.bus_read(0xFF4F), 0xFF);
    assert_eq!(gb.bus_read(0x8000), 0x00);
    gb.bus_write(0x9FFF, 0x22);
    gb.bus_write(0xFF4F, 0x00);
    assert_eq!(gb.bus_read(0x8000), 0x11);
    assert_eq!(gb.bus_read(0x9FFF), 0x00);
    assert_eq!(gb.vram[0x3FFF], 0x22);
}

#[test]
fn svbk_selects_the_high_wram_bank_and_zero_means_one() {
    let mut gb = cgb_lcd_off();
    assert_eq!(gb.bus_read(0xFF70), 0xF9);
    gb.bus_write(0xC000, 0xAA);
    gb.bus_write(0xD000, 0x01);
    gb.bus_write(0xFF70, 7);
    assert_eq!(gb.bus_read(0xD000), 0x00);
    gb.bus_write(0xD000, 0x07);
    // Bank 0 is fixed, and echo RAM follows the banking.
    assert_eq!(gb.bus_read(0xC000), 0xAA);
    assert_eq!(gb.bus_read(0xF000), 0x07);
    gb.bus_write(0xFF70, 0);
    assert_eq!(gb.bus_read(0xFF70), 0xF8);
    assert_eq!(gb.bus_read(0xD000), 0x01);
}

#[test]
fn compatibility_mode_locks_the_cgb_only_registers() {
    let mut gb = cgb(&SPIN, 0x00);
    gb.bus_write(0xFF40, 0x00);
    gb.bus_write(0xFF4F, 1);
    assert_eq!(gb.bus_read(0xFF4F), 0xFE);
    gb.bus_write(0xFF70, 3);
    assert_eq!(gb.bus_read(0xFF70), 0xFF);
    assert_eq!(gb.bus_read(0xFF4D), 0xFF);
    assert_eq!(gb.bus_read(0xFF55), 0xFF);
    // The palettes belong to the boot ROM once it has gone.
    gb.bus_write(0xFF68, 0x80);
    gb.bus_write(0xFF69, 0x00);
    assert_eq!(gb.bus_read(0xFF69), 0xFF);
    assert_eq!(gb.ppu.palette_colour(false, 0, 0), 0x7FFF);
}

#[test]
fn palette_data_auto_increments_through_palette_ram() {
    let mut gb = cgb_lcd_off();
    gb.bus_write(0xFF68, 0x80 | 0x3E);
    assert_eq!(gb.bus_read(0xFF68), 0xFE);
    gb.bus_write(0xFF69, 0x1F);
    gb.bus_write(0xFF69, 0x00);
    // The index wraps from $3F to $00.
    assert_eq!(gb.bus_read(0xFF68), 0xC0);
    assert_eq!(gb.ppu.palette_colour(false, 7, 3), 0x001F);

    // Without bit 7 the index stays put, and reads never move it.
    gb.bus_write(0xFF6A, 0x08);
    gb.bus_write(0xFF6B, 0xE0);
    gb.bus_write(0xFF6B, 0x03);
    assert_eq!(gb.bus_read(0xFF6A), 0x48);
    assert_eq!(gb.bus_read(0xFF6B), 0x03);
}

#[test]
fn palette_data_is_out_of_reach_in_mode_3() {
    let mut gb = cgb(&SPIN, 0x80);
    while gb.ppu.mode() != 3 {
        gb.step_m_cycle();
    }
    gb.bus_write(0xFF68, 0x80);
    assert_eq!(gb.bus_read(0xFF69), 0xFF);
    gb.bus_write(0xFF69, 0x00);
    // The byte is lost, but auto-increment still moves the index.
    assert_eq!(gb.bus_read(0xFF68), 0xC1);
    assert_eq!(gb.ppu.palette_colour(false, 0, 0), 0x7FFF);
}

/// Fill `len` bytes of WRAM from `$C000` with a counting pattern.
fn fill_wram(gb: &mut GameBoy, len: u16) {
    for i in 0..len {
        gb.bus_write(0xC000 + i, (i as u8).wrapping_add(1));
    }
}

fn set_hdma(gb: &mut GameBoy, source: u16, dest: u16) {
    gb.bus_write(0xFF51, (source >> 8) as u8);
    gb.bus_write(0xFF52, source as u8);
    gb.bus_write(0xFF53, (dest >> 8) as u8);
    gb.bus_write(0xFF54, dest as u8);
}

/// Steps until no HDMA is copying, returning the m-cycles it held the
/// CPU and checking the CPU did not move meanwhile.
fn run_hdma(gb: &mut GameBoy) -> u32 {
    let pc = gb.cpu.pc;
    let mut cycles = 0;
    while gb.cgb.as_ref().unwrap().hdma_running {
        gb.step_m_cycle();
        assert_eq!(gb.cpu.pc, pc, "the CPU ran during HDMA");
        cycles += 1;
    }
    cycles
}

#[test]
fn general_purpose_dma_copies_everything_and_holds_the_cpu() {
    let mut gb = cgb_lcd_off();
    fill_wram(&mut gb, 0x20);
    gb.bus_write(0xFF4F, 1);
    set_hdma(&mut gb, 0xC000, 0x8010);
    gb.bus_write(0xFF55, 0x01); // two blocks
    // One m-cycle to start, then two bytes per m-cycle.
    assert_eq!(run_hdma(&mut gb), 1 + 16);
    for i in 0..0x20 {
        assert_eq!(gb.vram[0x2010 + i], i as u8 + 1);
    }
    assert_eq!(gb.vram[0x0010], 0, "bank 0 untouched");
    assert_eq!(gb.bus_read(0xFF55), 0xFF);
}

#[test]
fn general_purpose_dma_at_double_speed_copies_one_byte_per_m_cycle() {
    let mut gb = cgb_lcd_off();
    gb.cgb.as_mut().unwrap().double_speed = true;
    fill_wram(&mut gb, 0x10);
    set_hdma(&mut gb, 0xC000, 0x8000);
    gb.bus_write(0xFF55, 0x00);
    assert_eq!(run_hdma(&mut gb), 1 + 16);
    assert_eq!(gb.vram[0x000F], 0x10);
}

#[test]
fn hdma_cannot_read_vram_or_the_high_page() {
    let mut gb = cgb_lcd_off();
    gb.bus_write(0x8000, 0x55);
    set_hdma(&mut gb, 0x8000, 0x8800);
    gb.bus_write(0xFF55, 0x00);
    run_hdma(&mut gb);
    assert_eq!(gb.vram[0x0800], 0xFF);
    // $E000 and up behave like $F000: never a source.
    gb.bus_write(0xFF51, 0xE0);
    gb.bus_write(0xFF52, 0x00);
    assert_eq!(gb.cgb.as_ref().unwrap().hdma_source, 0xF000);
}

#[test]
fn hblank_dma_copies_one_block_per_hblank() {
    let mut gb = cgb(&SPIN, 0x80);
    // Wait for mode 2 so the transfer does not start at once.
    while gb.ppu.mode() != 2 {
        gb.step_m_cycle();
    }
    fill_wram(&mut gb, 0x20);
    set_hdma(&mut gb, 0xC000, 0x9000);
    gb.bus_write(0xFF55, 0x81);
    assert_eq!(gb.bus_read(0xFF55), 0x01);
    assert!(!gb.cgb.as_ref().unwrap().hdma_running);

    while gb.ppu.mode() != 0 {
        gb.step_m_cycle();
    }
    assert_eq!(run_hdma(&mut gb), 1 + 8);
    assert_eq!(gb.vram[0x100F], 0x10);
    assert_eq!(gb.vram[0x1010], 0x00, "only one block per HBlank");
    assert_eq!(gb.bus_read(0xFF55), 0x00);

    while gb.ppu.mode() == 0 {
        gb.step_m_cycle();
    }
    while gb.ppu.mode() != 0 {
        gb.step_m_cycle();
    }
    run_hdma(&mut gb);
    assert_eq!(gb.vram[0x101F], 0x20);
    assert_eq!(gb.bus_read(0xFF55), 0xFF);
}

#[test]
fn hblank_dma_started_during_hblank_copies_a_block_at_once() {
    let mut gb = cgb_lcd_off();
    fill_wram(&mut gb, 0x10);
    set_hdma(&mut gb, 0xC000, 0x8000);
    gb.bus_write(0xFF55, 0x80);
    assert_eq!(run_hdma(&mut gb), 1 + 8);
    assert_eq!(gb.vram[0x000F], 0x10);
}

#[test]
fn clearing_hdma5_bit_7_stops_an_hblank_transfer() {
    let mut gb = cgb(&SPIN, 0x80);
    while gb.ppu.mode() != 2 {
        gb.step_m_cycle();
    }
    set_hdma(&mut gb, 0xC000, 0x8000);
    gb.bus_write(0xFF55, 0x83);
    gb.bus_write(0xFF55, 0x02);
    // Stopped: bit 7 set, with the count just written.
    assert_eq!(gb.bus_read(0xFF55), 0x82);
    assert!(!gb.cgb.as_ref().unwrap().hdma_hblank);
}

/// `LD A,1 / LDH ($4D),A / STOP / JR -2` (the loop is at `$0106`).
const SPEED_SWITCH: [u8; 8] = [0x3E, 0x01, 0xE0, 0x4D, 0x10, 0x00, 0x18, 0xFE];

fn run_until_pc(gb: &mut GameBoy, pc: u16, limit: u32) -> u32 {
    for cycles in 0..limit {
        if gb.cpu.pc == pc && gb.cpu.instruction_complete() && !gb.cpu.halt_mode {
            return cycles;
        }
        gb.step_m_cycle();
    }
    panic!("PC never reached {pc:#06X}");
}

#[test]
fn stop_with_key1_armed_switches_to_double_speed() {
    let mut gb = cgb(&SPEED_SWITCH, 0x80);
    gb.ie_reg = 0;
    assert_eq!(gb.bus_read(0xFF4D), 0x7E);
    // At the STOP, with KEY1 armed.
    run_until_pc(&mut gb, 0x0104, 100);
    assert_eq!(gb.bus_read(0xFF4D), 0x7F, "armed");

    // STOP switches and then holds the CPU for $20008 CPU cycles.
    let held = run_until_pc(&mut gb, 0x0106, 0x9000);
    assert!(gb.double_speed());
    assert_eq!(gb.bus_read(0xFF4D), 0xFE);
    assert!(
        (0x8000..0x8010).contains(&held),
        "held for {held:#X} m-cycles"
    );

    // At double speed a frame takes twice as many m-cycles.
    gb.run_frame();
    let frame = gb.run_frame();
    assert_eq!(frame, 2 * 17_556);
}

#[test]
fn the_timer_runs_on_the_cpu_clock_at_double_speed() {
    let mut gb = cgb_lcd_off();
    gb.cgb.as_mut().unwrap().double_speed = true;
    gb.bus_write(0xFF04, 0);
    // DIV counts at 16384 Hz of CPU time: once per 64 m-cycles.
    for _ in 0..64 {
        gb.step_m_cycle();
    }
    assert_eq!(gb.bus_read(0xFF04), 1);
}

#[test]
fn a_pending_interrupt_cuts_the_speed_switch_halt_short() {
    let mut gb = cgb(&SPEED_SWITCH, 0x80);
    gb.ie_reg = 0;
    run_until_pc(&mut gb, 0x0104, 100);
    // Let STOP run, then raise an interrupt the CPU can see.
    for _ in 0..4 {
        gb.step_m_cycle();
    }
    gb.ie_reg = IF_TIMER;
    gb.if_reg |= IF_TIMER;
    let cycles = run_until_pc(&mut gb, 0x0106, 100);
    assert!(cycles < 10);
    assert!(gb.double_speed());
}

#[test]
fn stop_without_a_switch_waits_for_a_button() {
    // `STOP / JR -2` on a DMG: STOP mode until a selected button.
    let (_, mut gb) = GameBoy::from_rom(rom_with(&[0x10, 0x00, 0x18, 0xFE], 0x00)).unwrap();
    gb.ie_reg = 0;
    gb.bus_write(0xFF00, 0x10); // select the action buttons
    for _ in 0..8 {
        gb.step_m_cycle();
    }
    assert!(gb.in_stop_mode());
    assert_eq!(gb.bus_read(0xFF04), 0, "STOP reset DIV");
    let frozen = gb.cpu.pc;
    for _ in 0..1000 {
        gb.step_m_cycle();
    }
    assert_eq!(gb.cpu.pc, frozen);
    assert_eq!(gb.timer.counter, 0, "the timer stands still");

    gb.set_button(JoypadButton::A, true);
    gb.step_m_cycle();
    assert!(!gb.in_stop_mode());
    run_until_pc(&mut gb, 0x0102, 100);
}

#[test]
fn stop_with_a_button_held_halts_instead() {
    let (_, mut gb) = GameBoy::from_rom(rom_with(&[0x10, 0x00, 0x18, 0xFE], 0x00)).unwrap();
    gb.ie_reg = 0;
    gb.bus_write(0xFF00, 0x10);
    gb.set_button(JoypadButton::A, true);
    for _ in 0..8 {
        gb.step_m_cycle();
    }
    assert!(!gb.in_stop_mode());
    assert!(gb.cpu.halt_mode);
}

#[test]
fn the_boot_rom_maps_until_ff50_and_key0_picks_the_mode() {
    // LD A,$04 / LDH ($4C),A / LD A,$11 / LDH ($50),A, then NOPs into
    // the cartridge from $0008.
    let mut boot = vec![0x00; cgb::CGB_BOOT_ROM_SIZE];
    boot[..8].copy_from_slice(&[0x3E, 0x04, 0xE0, 0x4C, 0x3E, 0x11, 0xE0, 0x50]);
    boot[0x0200] = 0xA5;
    let mut rom = rom_with(&SPIN, 0x80);
    rom[0x0000] = 0x5A;
    rom[0x0200] = 0x77;
    let (_, cart) = load(rom).unwrap();
    let mut gb = GameBoy::new_cgb_with_boot_rom(cart, BootProfile::Cgb, boot);

    assert_eq!(gb.cpu.pc, 0);
    assert!(gb.cgb_mode(), "the boot ROM runs in CGB mode");
    assert_eq!(gb.bus_read(0x0000), 0x3E);
    assert_eq!(gb.bus_read(0x0200), 0xA5);
    // The cartridge header shows through.
    assert_eq!(gb.bus_read(0x0143), 0x80);
    assert_eq!(gb.bus_read(0xFF50), 0xFE);

    run_until_pc(&mut gb, 0x0009, 100);
    assert!(!gb.cgb_mode(), "KEY0 = $04 picks compatibility mode");
    assert_eq!(gb.bus_read(0xFF50), 0xFF);
    assert_eq!(gb.bus_read(0x0000), 0x5A);
    assert_eq!(gb.bus_read(0x0200), 0x77);
    // KEY0 is locked once the boot ROM is gone.
    gb.bus_write(0xFF4C, 0x80);
    assert!(!gb.cgb_mode());
}

#[test]
fn opri_reorders_objects_only_while_the_boot_rom_runs() {
    let mut boot = vec![0x00; cgb::CGB_BOOT_ROM_SIZE];
    boot[0] = 0x18;
    boot[1] = 0xFE;
    let (_, cart) = load(rom_with(&SPIN, 0x80)).unwrap();
    let mut gb = GameBoy::new_cgb_with_boot_rom(cart, BootProfile::Cgb, boot);
    gb.bus_write(0xFF6C, 0x01);
    assert_eq!(gb.bus_read(0xFF6C), 0xFF);
    assert!(!gb.ppu.object_priority_by_index());
    gb.bus_write(0xFF50, 0x01);
    gb.bus_write(0xFF6C, 0x00);
    assert_eq!(gb.bus_read(0xFF6C), 0xFE);
    assert!(!gb.ppu.object_priority_by_index());
}

#[test]
fn bg_attributes_pick_the_palette_and_the_tile_bank() {
    let mut gb = cgb_lcd_off();
    // BG palette 1, colour 3: pure red.
    gb.bus_write(0xFF68, 0x80 | (8 + 6));
    gb.bus_write(0xFF69, 0x1F);
    gb.bus_write(0xFF69, 0x00);
    // Tile 0 in bank 1 is solid colour 3; tile 0 in bank 0 is blank.
    for row in 0..16 {
        gb.vram[0x2000 + row] = 0xFF;
    }
    // Map entry 0 uses palette 1 from bank 1; entry 1 has no attributes.
    gb.vram[0x2000 + 0x1800] = 0x09;
    gb.bus_write(0xFF40, 0x91);
    gb.run_frame();
    gb.run_frame();
    let picture = gb.color_framebuffer().unwrap();
    assert_eq!(picture[0], 0x001F);
    assert_eq!(picture[7], 0x001F);
    assert_eq!(picture[8], 0x7FFF);
}

#[test]
fn snapshots_round_trip_the_cgb_state() {
    let mut gb = cgb_lcd_off();
    gb.bus_write(0xFF70, 5);
    gb.bus_write(0xD123, 0x42);
    gb.bus_write(0xFF4F, 1);
    let bytes = postcard::to_allocvec(&gb).unwrap();
    let back: GameBoy = postcard::from_bytes(&bytes).unwrap();
    assert!(back.cgb_mode());
    assert_eq!(back.bus_read(0xD123), 0x42);
    assert_eq!(back.bus_read(0xFF4F), 0xFF);
}
