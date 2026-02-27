//! C64 Kernal boot test — verify the machine boots to BASIC READY. prompt.

use emu_core::Bus;
use emu_c64::{C64, C64Config, C64Model};
use std::fs;
use std::path::Path;

/// PETSCII codes for "READY."
const READY_PETSCII: [u8; 6] = [
    18, // R
    5,  // E
    1,  // A
    4,  // D
    25, // Y
    46, // .
];

#[test]
#[ignore] // Requires real C64 ROMs at roms/
fn test_boot_kernal() {
    let kernal = fs::read("../../roms/kernal.rom").expect("kernal.rom not found at roms/kernal.rom");
    let basic = fs::read("../../roms/basic.rom").expect("basic.rom not found at roms/basic.rom");
    let chargen =
        fs::read("../../roms/chargen.rom").expect("chargen.rom not found at roms/chargen.rom");

    let mut c64 = C64::new(&C64Config {
        model: C64Model::C64Pal,
        kernal_rom: kernal,
        basic_rom: basic,
        char_rom: chargen,
    });

    println!("Reset: PC=${:04X}", c64.cpu().regs.pc);

    let max_frames = 200;
    let mut found_ready = false;

    for frame in 0..max_frames {
        let cycles = c64.run_frame();

        // Every 50 frames (~1s), print diagnostic
        if frame % 50 == 0 {
            println!(
                "Frame {frame}: PC=${:04X} cycles={cycles}",
                c64.cpu().regs.pc
            );
        }

        // Search screen memory ($0400-$07E7) for "READY."
        if find_ready_in_screen(&c64) {
            println!("READY. found at frame {frame}!");
            found_ready = true;

            // Run a few more frames so the VIC renders the complete screen
            for _ in 0..2 {
                c64.run_frame();
            }

            // Save screenshot
            let out_dir = Path::new("../../test_output");
            fs::create_dir_all(out_dir).ok();
            let screenshot_path = out_dir.join("c64_boot_ready.png");
            emu_c64::capture::save_screenshot(&c64, &screenshot_path)
                .expect("Failed to save screenshot");
            println!("Screenshot saved to {}", screenshot_path.display());
            break;
        }
    }

    assert!(found_ready, "C64 did not reach READY. prompt within {max_frames} frames");
}

#[test]
#[ignore] // Requires real C64 ROMs at roms/
fn test_sid_produces_audio() {
    let kernal =
        fs::read("../../roms/kernal.rom").expect("kernal.rom not found at roms/kernal.rom");
    let basic = fs::read("../../roms/basic.rom").expect("basic.rom not found at roms/basic.rom");
    let chargen =
        fs::read("../../roms/chargen.rom").expect("chargen.rom not found at roms/chargen.rom");

    let mut c64 = C64::new(&C64Config {
        model: C64Model::C64Pal,
        kernal_rom: kernal,
        basic_rom: basic,
        char_rom: chargen,
    });

    // Boot past READY.
    for _ in 0..120 {
        c64.run_frame();
        let _ = c64.take_audio_buffer();
    }

    // Poke SID registers for a sawtooth tone via the bus (like a program would).
    // Voice 1: ~440 Hz sawtooth, instant attack, max sustain, volume 15.
    let sid_base = 0xD400u32;
    let freq: u16 = 7479; // 440 Hz
    c64.bus_mut().write(sid_base, (freq & 0xFF) as u8);       // Freq lo
    c64.bus_mut().write(sid_base + 1, (freq >> 8) as u8);     // Freq hi
    c64.bus_mut().write(sid_base + 5, 0x00);                  // AD: attack=0, decay=0
    c64.bus_mut().write(sid_base + 6, 0xF0);                  // SR: sustain=F, release=0
    c64.bus_mut().write(sid_base + 4, 0x21);                  // Sawtooth + gate on
    c64.bus_mut().write(sid_base + 0x18, 0x0F);               // Volume = 15

    // Run 50 frames (~1 second at PAL 50 Hz) to produce a usable audio clip.
    let mut audio = Vec::new();
    for _ in 0..50 {
        c64.run_frame();
        audio.extend_from_slice(&c64.take_audio_buffer());
    }

    println!("SID audio buffer: {} samples ({:.2}s)", audio.len(), audio.len() as f64 / 48_000.0);

    assert!(!audio.is_empty(), "SID should produce audio samples");

    // Verify non-silent: at least some samples above noise floor
    let max_abs = audio.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    println!("Max absolute sample: {max_abs}");
    assert!(
        max_abs > 0.01,
        "SID audio should be non-silent with sawtooth playing, max={max_abs}"
    );

    // Save as WAV for manual verification
    let out_dir = Path::new("../../test_output");
    fs::create_dir_all(out_dir).ok();
    let audio_path = out_dir.join("c64_sid_tone.wav");
    emu_c64::capture::save_audio(&audio, &audio_path).expect("Failed to save audio");
    println!("Audio saved to {}", audio_path.display());
}

#[test]
#[ignore] // Requires real C64 ROMs at roms/
fn test_badline_border_timing() {
    let kernal = fs::read("../../roms/kernal.rom").expect("kernal.rom not found");
    let basic = fs::read("../../roms/basic.rom").expect("basic.rom not found");
    let chargen = fs::read("../../roms/chargen.rom").expect("chargen.rom not found");

    let mut c64 = C64::new(&C64Config {
        model: C64Model::C64Pal,
        kernal_rom: kernal,
        basic_rom: basic,
        char_rom: chargen,
    });

    // Boot to READY. prompt
    for _ in 0..120 {
        c64.run_frame();
    }
    assert!(find_ready_in_screen(&c64), "C64 did not reach READY. prompt");

    // Poke a tight border-colour cycling loop at $C000:
    //   $C000: SEI            (78)       — disable Kernal IRQ
    //   $C001: INC $D020      (EE 20 D0) — increment border colour
    //   $C004: JMP $C001      (4C 01 C0) — loop forever
    //
    // Loop body = 9 cycles (INC abs = 6, JMP abs = 3).
    // On a 63-cycle PAL line:
    //   Normal line: ~7 INC iterations → 7 colour changes
    //   Badline:     CPU stalled 40 cycles → ~2 colour changes
    let program: &[(u16, u8)] = &[
        (0xC000, 0x78), // SEI
        (0xC001, 0xEE), // INC $D020
        (0xC002, 0x20),
        (0xC003, 0xD0),
        (0xC004, 0x4C), // JMP $C001
        (0xC005, 0x01),
        (0xC006, 0xC0),
    ];
    for &(addr, byte) in program {
        c64.bus_mut().memory.ram_write(addr, byte);
    }

    // Redirect CPU to our program
    c64.cpu_mut().regs.pc = 0xC000;

    // Run 3 frames to produce a stable visual pattern
    for _ in 0..3 {
        c64.run_frame();
    }

    // Save screenshot
    let out_dir = Path::new("../../test_output");
    fs::create_dir_all(out_dir).ok();
    let screenshot_path = out_dir.join("c64_badline_raster.png");
    emu_c64::capture::save_screenshot(&c64, &screenshot_path)
        .expect("Failed to save screenshot");
    println!("Screenshot saved to {}", screenshot_path.display());

    let fb = c64.framebuffer();
    let w = c64.framebuffer_width() as usize;

    // Helper: get pixel at (fb_x, fb_y)
    let pixel = |fb_x: usize, fb_y: usize| -> u32 { fb[fb_y * w + fb_x] };

    // With YSCROLL=3 (Kernal default $D011=$1B), badlines occur where (line & 7) == 3
    // within the display window (lines $30-$F7).
    //
    // The INC $D020 loop advances the border colour continuously. On a badline,
    // the CPU is stalled for 40 cycles, so fewer INC operations complete before
    // any given beam position. This shifts the border colour at a fixed X on
    // badline rows compared to their neighbors — the classic "staircase" effect.
    //
    // The right border is only 6 cycles wide (cycles 56-61), so we can't count
    // multiple transitions per line. Instead we verify the colour at a fixed X
    // differs between badline and normal lines across multiple 8-line groups.

    // Sample column at fb_x=384 (cycle 58, middle of right border).
    // Check 5 badline/normal pairs spaced across the display area.
    // Badlines at raster lines where (line & 7) == 3:
    //   raster 99 → fb_y 93,  raster 107 → fb_y 101,
    //   raster 115 → fb_y 109, raster 155 → fb_y 149,
    //   raster 195 → fb_y 189
    let sample_x = 384;
    let pairs: &[(usize, usize)] = &[
        (93, 94),   // raster 99/100
        (101, 102), // raster 107/108
        (109, 110), // raster 115/116
        (149, 150), // raster 155/156
        (189, 190), // raster 195/196
    ];

    // Assertion 1: Every badline/normal pair shows a colour difference at the
    // same X, proving the CPU stall shifts the border colour consistently.
    let mut mismatches = 0;
    for &(bl_y, nl_y) in pairs {
        let bl_px = pixel(sample_x, bl_y);
        let nl_px = pixel(sample_x, nl_y);
        let differs = bl_px != nl_px;
        if differs {
            mismatches += 1;
        }
        println!(
            "  fb_y {bl_y} (badline) = 0x{bl_px:08X}, \
             fb_y {nl_y} (normal) = 0x{nl_px:08X} — {}",
            if differs { "DIFFER" } else { "same" }
        );
    }
    println!("Badline/normal colour mismatches: {mismatches}/{}", pairs.len());
    assert!(
        mismatches >= 4,
        "At least 4 of {} badline/normal pairs should show different border colours \
         at x={sample_x}, got {mismatches}",
        pairs.len()
    );

    // Assertion 2: The badline effect is systematic across the entire display
    // area, not a one-off glitch. Check ALL badline/normal pairs in the
    // display window (raster lines $30-$F7). Badlines at every raster line
    // where (line & 7) == 3: lines 51, 59, 67, ..., 243.
    let mut total_pairs = 0;
    let mut differing_pairs = 0;
    let mut raster = 51u16;
    while raster <= 243 {
        let bl_y = (raster - 6) as usize;
        let nl_y = bl_y + 1;
        if nl_y < fb.len() / w {
            total_pairs += 1;
            if pixel(sample_x, bl_y) != pixel(sample_x, nl_y) {
                differing_pairs += 1;
            }
        }
        raster += 8;
    }
    println!(
        "Display-wide badline/normal pairs: {differing_pairs}/{total_pairs} differ"
    );
    assert!(
        differing_pairs >= total_pairs * 4 / 5,
        "At least 80% of badline/normal pairs should differ across the display area, \
         got {differing_pairs}/{total_pairs}"
    );
}

#[test]
#[ignore] // Requires real C64 ROMs at roms/
fn test_hires_bitmap_mode() {
    let kernal = fs::read("../../roms/kernal.rom").expect("kernal.rom not found");
    let basic = fs::read("../../roms/basic.rom").expect("basic.rom not found");
    let chargen = fs::read("../../roms/chargen.rom").expect("chargen.rom not found");

    let mut c64 = C64::new(&C64Config {
        model: C64Model::C64Pal,
        kernal_rom: kernal,
        basic_rom: basic,
        char_rom: chargen,
    });

    // Boot to READY.
    for _ in 0..120 {
        c64.run_frame();
    }
    assert!(find_ready_in_screen(&c64), "C64 did not reach READY.");

    // Poke program to enable hires bitmap mode and fill bitmap + screen RAM.
    //
    // $D011 = $3B: DEN + BMM + YSCROLL=3
    // $D018 = $3C: screen at $0C00, bitmap at $2000
    // Fill bitmap RAM ($2000-$3FFF) with a checkerboard.
    // Fill screen RAM ($0C00-$0FFF) with colour $F0 (white fg, black bg).
    c64.bus_mut().write(0xD011, 0x3B);
    c64.bus_mut().write(0xD018, 0x3C);

    // Fill bitmap with checkerboard (alternating $AA/$55)
    for addr in 0x2000u16..0x3FFF {
        let pattern = if (addr & 1) == 0 { 0xAA } else { 0x55 };
        c64.bus_mut().memory.ram_write(addr, pattern);
    }

    // Fill screen RAM with white-on-black
    for addr in 0x0C00u16..0x0FFF {
        c64.bus_mut().memory.ram_write(addr, 0xF0);
    }

    // Run a few frames
    for _ in 0..3 {
        c64.run_frame();
    }

    // Save screenshot
    let out_dir = Path::new("../../test_output");
    fs::create_dir_all(out_dir).ok();
    let screenshot_path = out_dir.join("c64_hires_bitmap.png");
    emu_c64::capture::save_screenshot(&c64, &screenshot_path)
        .expect("Failed to save screenshot");
    println!("Screenshot saved to {}", screenshot_path.display());

    // Verify that the display window contains non-uniform pixels.
    // In bitmap mode, our checkerboard should produce alternating white/black.
    let fb = c64.framebuffer();
    let w = c64.framebuffer_width() as usize;

    // Sample two adjacent pixels in the display area.
    // Display starts at fb_y ~42 (line $30 - line 6), fb_x ~48 (cycle 16 - cycle 10)*8.
    let fb_y = 50;
    let fb_x = 56;
    let idx0 = fb_y * w + fb_x;
    let idx1 = idx0 + 1;

    // The two pixels should be different colours (checkerboard)
    assert_ne!(
        fb[idx0], fb[idx1],
        "Adjacent bitmap pixels should differ in checkerboard"
    );
}

#[test]
#[ignore] // Requires real C64 ROMs at roms/
fn test_multicolour_text_mode() {
    let kernal = fs::read("../../roms/kernal.rom").expect("kernal.rom not found");
    let basic = fs::read("../../roms/basic.rom").expect("basic.rom not found");
    let chargen = fs::read("../../roms/chargen.rom").expect("chargen.rom not found");

    let mut c64 = C64::new(&C64Config {
        model: C64Model::C64Pal,
        kernal_rom: kernal,
        basic_rom: basic,
        char_rom: chargen,
    });

    // Boot to READY.
    for _ in 0..120 {
        c64.run_frame();
    }
    assert!(find_ready_in_screen(&c64), "C64 did not reach READY.");

    // Enable multicolour text mode
    // $D016 bit 4 = MCM on
    c64.bus_mut().write(0xD016, 0x18); // MCM=1
    c64.bus_mut().write(0xD021, 0x00); // BG0 = black
    c64.bus_mut().write(0xD022, 0x02); // BG1 = red
    c64.bus_mut().write(0xD023, 0x05); // BG2 = green

    // Fill colour RAM with bit 3 set (activates MCM per character)
    for offset in 0u16..1000 {
        c64.bus_mut().memory.colour_ram_write(offset, 0x0F);
    }

    // Fill screen with char 0 (uses chargen bitmap)
    for addr in 0x0400u16..0x07E8 {
        c64.bus_mut().memory.ram_write(addr, 0x00);
    }

    // Run frames
    for _ in 0..3 {
        c64.run_frame();
    }

    // Save screenshot
    let out_dir = Path::new("../../test_output");
    fs::create_dir_all(out_dir).ok();
    let screenshot_path = out_dir.join("c64_mcm_text.png");
    emu_c64::capture::save_screenshot(&c64, &screenshot_path)
        .expect("Failed to save screenshot");
    println!("Screenshot saved to {}", screenshot_path.display());

    // Verify MCM rendering: in MCM, adjacent pixels within a pair are the same
    // colour (each pair is 2 pixels wide). Sample in display area.
    let fb = c64.framebuffer();
    let w = c64.framebuffer_width() as usize;
    let fb_y = 50;
    let fb_x = 48; // Start of display window

    let idx0 = fb_y * w + fb_x;
    // In MCM, pixel 0 and pixel 1 should be the same colour (same bit pair)
    assert_eq!(
        fb[idx0],
        fb[idx0 + 1],
        "MCM pair pixels should be the same colour"
    );
}

/// Scan screen memory for the PETSCII sequence "READY."
fn find_ready_in_screen(c64: &C64) -> bool {
    let screen_start = 0x0400u16;
    let screen_end = 0x07E8u16;

    for addr in screen_start..screen_end {
        if addr + READY_PETSCII.len() as u16 > screen_end {
            break;
        }

        let mut matches = true;
        for (i, &expected) in READY_PETSCII.iter().enumerate() {
            let actual = c64.bus().memory.peek(addr + i as u16);
            if actual != expected {
                matches = false;
                break;
            }
        }

        if matches {
            return true;
        }
    }

    false
}
