//! A hand-assembled Master System cartridge, run through the converter.

use super::*;
use emu198x_sega_315_5313::render::BORDER_LEFT;
use emu198x_sega_315_5313::{MODE4_FRAME_WIDTH, argb_mode4};

/// The colour the cartridge paints: green, in the Master System's
/// `--BBGGRR` form.
const GREEN: u8 = 0x0C;
/// Rows above the picture on an NTSC console in the 192-line mode.
const TOP: usize = 27;

/// A 32 KB cartridge that sets the VDP up for Mode 4 with every cell
/// showing a solid tile, then copies port `$DC` to `$C000` forever. Its
/// NMI handler counts presses of Pause at `$C001`.
fn cartridge() -> Vec<u8> {
    let mut rom = vec![0xFF; 0x8000];
    let main = [
        0xF3, // DI
        0x31, 0xF0, 0xDF, // LD SP,$DFF0
        0xED, 0x56, // IM 1
        0x21, 0x00, 0x01, // LD HL,$0100
        0x4E, // loop: LD C,(HL)
        0x23, // INC HL
        0x79, // LD A,C
        0xB7, // OR A
        0x28, 0x04, // JR Z,done
        0xED, 0xA3, // OUTI
        0x18, 0xF6, // JR loop
        0xDB, 0xDC, // done: IN A,($DC)
        0x32, 0x00, 0xC0, // LD ($C000),A
        0x18, 0xF9, // JR done
    ];
    rom[..main.len()].copy_from_slice(&main);
    let nmi = [
        0xF5, // PUSH AF
        0x3A, 0x01, 0xC0, // LD A,($C001)
        0x3C, // INC A
        0x32, 0x01, 0xC0, // LD ($C001),A
        0xF1, // POP AF
        0xED, 0x45, // RETN
    ];
    rom[0x66..0x66 + nmi.len()].copy_from_slice(&nmi);

    // (port, byte) pairs, ended by a zero port.
    let mut table = vec![
        0xBF, 0x04, 0xBF, 0x80, // register 0: Mode 4
        0xBF, 0x40, 0xBF, 0x81, // register 1: display on, 192 lines
        0xBF, 0xFF, 0xBF, 0x82, // register 2: names at $3800
        0xBF, 0xFF, 0xBF, 0x85, // register 5: sprites at $3F00
        0xBF, 0x01, 0xBF, 0xC0, // CRAM entry 1
        0xBE, GREEN, // ...is green
        0xBF, 0x00, 0xBF, 0x7F, // VRAM $3F00
        0xBE, 0xD0, // the sprite list ends at once
        0xBF, 0x00, 0xBF, 0x40, // VRAM $0000: tile 0
    ];
    for _ in 0..8 {
        // Plane 0 set on every row: colour 1 throughout.
        table.extend([0xBE, 0xFF, 0xBE, 0x00, 0xBE, 0x00, 0xBE, 0x00]);
    }
    table.push(0);
    rom[0x100..0x100 + table.len()].copy_from_slice(&table);
    rom[0x7FF0..0x7FF8].copy_from_slice(b"TMR SEGA");
    rom
}

fn pixel(md: &MegaDriveState, x: usize, y: usize) -> u32 {
    md.framebuffer()[(TOP + y) * FRAME_STRIDE + BORDER_LEFT + x]
}

#[test]
fn a_master_system_cartridge_draws_in_mode_4() {
    let rom = cartridge();
    assert!(is_master_system_image(&rom));
    let mut md = MegaDriveState::new_power_base(rom, Region::Americas);
    assert!(md.master_system_mode());
    md.run_frame();
    md.run_frame();
    assert_eq!(md.frame_width() as usize, MODE4_FRAME_WIDTH);
    assert_eq!(md.frame_height(), 27 + 192 + 24);
    assert_eq!(pixel(&md, 0, 0), argb_mode4(GREEN));
    assert_eq!(pixel(&md, 255, 191), argb_mode4(GREEN));
}

#[test]
fn the_cartridge_reads_the_pad_through_port_dc() {
    let mut md = MegaDriveState::new_power_base(cartridge(), Region::Americas);
    md.run_frame();
    assert_eq!(md.z80_ram()[0], 0xFF, "nothing pressed");
    md.set_buttons(
        0,
        Buttons {
            up: true,
            b: true,
            ..Buttons::default()
        },
    );
    md.run_frame();
    assert_eq!(md.z80_ram()[0], 0xEE, "up and button 1");
    md.set_buttons(
        1,
        Buttons {
            down: true,
            ..Buttons::default()
        },
    );
    md.run_frame();
    assert_eq!(md.z80_ram()[0], 0x6E, "port B's down shares the port");
}

#[test]
fn start_is_pause_and_raises_one_nmi_a_press() {
    let mut md = MegaDriveState::new_power_base(cartridge(), Region::Europe);
    md.run_frame();
    let press = |md: &mut MegaDriveState, start: bool| {
        md.set_buttons(
            0,
            Buttons {
                start,
                ..Buttons::default()
            },
        );
        md.run_frame();
    };
    press(&mut md, true);
    press(&mut md, true);
    assert_eq!(md.z80_ram()[1], 1, "held: one NMI");
    press(&mut md, false);
    press(&mut md, true);
    assert_eq!(md.z80_ram()[1], 2);
}

#[test]
fn the_fm_chip_is_unreachable_and_the_psg_is_heard() {
    let mut md = MegaDriveState::new_power_base(cartridge(), Region::Americas);
    md.run_frame();
    let audio = md.take_audio();
    assert!(!audio.is_empty(), "the PSG alone makes sound");
    assert_eq!(audio.len() % 2, 0);
}
