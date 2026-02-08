// Test ULA rendering by writing known data directly to VRAM
use emu_core::Observable;
use emu_spectrum::{Spectrum, SpectrumConfig, SpectrumModel};

fn main() {
    // Use a minimal ROM that just halts (DI; HALT at $0000)
    let mut rom = vec![0u8; 0x4000];
    rom[0] = 0xF3; // DI
    rom[1] = 0x76; // HALT

    let config = SpectrumConfig {
        model: SpectrumModel::Spectrum48K,
        rom,
    };
    let mut spectrum = Spectrum::new(&config);

    // === Test 1: Write all $FF to bitmap, $38 (white on black) to attributes ===
    // This should make the entire screen white-on-black
    for addr in 0x4000..0x5800u16 {
        spectrum.bus_mut().memory.write(addr, 0xFF);
    }
    for addr in 0x5800..0x5B00u16 {
        spectrum.bus_mut().memory.write(addr, 0x38); // Paper=white(7), Ink=black(0)
    }

    // Run one frame to render
    spectrum.run_frame();

    // Check a few pixels in the screen area (fb_y=48..240, fb_x=32..288)
    let fb = spectrum.framebuffer();
    let fb_w = spectrum.framebuffer_width() as usize;

    // Check top-left of screen area
    let p_top_left = fb[48 * fb_w + 32];
    // Check center of screen area
    let p_center = fb[144 * fb_w + 160];
    // Check a border pixel
    let p_border = fb[10 * fb_w + 10];

    println!("=== Test 1: All $FF bitmap, $38 attributes ===");
    println!("Screen top-left pixel: 0x{:08X}", p_top_left);
    println!("Screen center pixel:   0x{:08X}", p_center);
    println!("Border pixel:          0x{:08X}", p_border);

    // With $FF bitmap and $38 attr (paper=white, ink=black), pixel=1 → ink (black)
    // Wait, FBPPPIII: attr $38 = 00 111 000 → paper=7(white), ink=0(black)
    // bitmap $FF = all bits set = all ink = all BLACK
    // So screen pixels should be BLACK (ink colour = 0 = black)
    let expected_ink = 0xFF00_0000u32; // black from palette
    let expected_paper = 0xFFCD_CDCDu32; // white from palette
    println!("Expected ink (black):  0x{:08X}", expected_ink);
    println!("Expected paper (white): 0x{:08X}", expected_paper);

    if p_top_left == expected_ink && p_center == expected_ink {
        println!("PASS: bitmap $FF + attr $38 → all ink (black)");
    } else {
        println!("FAIL: unexpected pixel values");
    }

    // === Test 2: All $00 bitmap, same attributes → should be all paper (white) ===
    for addr in 0x4000..0x5800u16 {
        spectrum.bus_mut().memory.write(addr, 0x00);
    }
    spectrum.run_frame();

    let fb = spectrum.framebuffer();
    let p2 = fb[48 * fb_w + 32];
    println!("\n=== Test 2: All $00 bitmap, $38 attributes ===");
    println!("Screen top-left pixel: 0x{:08X}", p2);
    if p2 == expected_paper {
        println!("PASS: bitmap $00 + attr $38 → all paper (white)");
    } else {
        println!("FAIL: expected paper 0x{:08X}, got 0x{:08X}", expected_paper, p2);
    }

    // === Test 3: Checkerboard bitmap ($AA), check alternating pixels ===
    for addr in 0x4000..0x5800u16 {
        spectrum.bus_mut().memory.write(addr, 0xAA); // 10101010
    }
    // Attr: ink=red(2), paper=blue(1) = 00 001 010 = $0A
    for addr in 0x5800..0x5B00u16 {
        spectrum.bus_mut().memory.write(addr, 0x0A);
    }
    spectrum.run_frame();

    let fb = spectrum.framebuffer();
    // First pixel of screen (bit 7 of bitmap = 1 → ink = red)
    let p3_ink = fb[48 * fb_w + 32];
    // Second pixel (bit 6 of bitmap = 0 → paper = blue)
    let p3_paper = fb[48 * fb_w + 33];
    println!("\n=== Test 3: Checkerboard $AA, attr $0A (ink=red, paper=blue) ===");
    println!("Pixel 0 (bit 7=1, ink): 0x{:08X}", p3_ink);
    println!("Pixel 1 (bit 6=0, paper): 0x{:08X}", p3_paper);

    let red = 0xFFCD_0000u32; // palette[2] = red
    let blue = 0xFF00_00CDu32; // palette[1] = blue
    println!("Expected ink (red):   0x{:08X}", red);
    println!("Expected paper (blue): 0x{:08X}", blue);

    if p3_ink == red && p3_paper == blue {
        println!("PASS: checkerboard renders correctly");
    } else {
        println!("FAIL: checkerboard pixel values wrong");
    }

    // === Test 4: Check bitmap address encoding (character on line 0) ===
    // Clear bitmap
    for addr in 0x4000..0x5800u16 {
        spectrum.bus_mut().memory.write(addr, 0x00);
    }
    // Write $FF to first byte of screen (should be top-left 8 pixels of line 0)
    spectrum.bus_mut().memory.write(0x4000, 0xFF);
    // Set attr for position (0,0): ink=white, paper=black = $07
    spectrum.bus_mut().memory.write(0x5800, 0x07);
    // Rest of attrs = $07
    for addr in 0x5801..0x5B00u16 {
        spectrum.bus_mut().memory.write(addr, 0x07);
    }

    spectrum.run_frame();

    let fb = spectrum.framebuffer();
    // Pixels 0-7 of line 0 should be white (ink), pixel 8 should be black (paper)
    let p4_pixel0 = fb[48 * fb_w + 32]; // x=0 of screen area
    let p4_pixel7 = fb[48 * fb_w + 39]; // x=7
    let p4_pixel8 = fb[48 * fb_w + 40]; // x=8 (next byte, which is $00)

    println!("\n=== Test 4: Single byte at $4000 ===");
    println!("Pixel 0 (ink):   0x{:08X}", p4_pixel0);
    println!("Pixel 7 (ink):   0x{:08X}", p4_pixel7);
    println!("Pixel 8 (paper): 0x{:08X}", p4_pixel8);

    let white = 0xFFCD_CDCDu32;
    let black = 0xFF00_0000u32;
    if p4_pixel0 == white && p4_pixel7 == white && p4_pixel8 == black {
        println!("PASS: bitmap address $4000 maps to top-left corner correctly");
    } else {
        println!("FAIL: bitmap address mapping incorrect");
    }

    // === Test 5: Verify the ROM actually gets loaded into memory ===
    println!("\n=== Test 5: ROM in memory ===");
    // Our test ROM has $F3 at $0000 and $76 at $0001
    let rom_byte0 = spectrum.bus().memory.peek(0x0000);
    let rom_byte1 = spectrum.bus().memory.peek(0x0001);
    println!("ROM[$0000] = 0x{:02X} (expected 0xF3)", rom_byte0);
    println!("ROM[$0001] = 0x{:02X} (expected 0x76)", rom_byte1);

    // Check RAM is accessible
    spectrum.bus_mut().memory.write(0xC000, 0x42);
    let ram_byte = spectrum.bus().memory.peek(0xC000);
    println!("RAM[$C000] = 0x{:02X} (expected 0x42)", ram_byte);

    // === Test 6: Check that real ROM boots and modifies RAM ===
    println!("\n=== Test 6: Real ROM boot check ===");
    let real_rom = include_bytes!("../../../roms/48.rom");
    let config2 = SpectrumConfig {
        model: SpectrumModel::Spectrum48K,
        rom: real_rom.to_vec(),
    };
    let mut spectrum2 = Spectrum::new(&config2);

    // Run 100 frames
    for _ in 0..100 {
        spectrum2.run_frame();
    }

    // Check system variables
    let err_nr = spectrum2.bus().memory.peek(0x5C3A);
    let pc = spectrum2.cpu().regs.pc;
    let sp = spectrum2.cpu().regs.sp;
    let im = spectrum2.cpu().regs.im;
    let iy = spectrum2.cpu().regs.iy;
    let iff1 = spectrum2.cpu().regs.iff1;

    println!("After 100 frames:");
    println!("  PC={:04X} SP={:04X} IM={} IY={:04X} IFF1={}", pc, sp, im, iy, iff1);
    println!("  ERR_NR=0x{:02X}", err_nr);

    // Check some RAM contents
    let mut nonzero_bitmap = 0u32;
    for addr in 0x4000..0x5800u16 {
        if spectrum2.bus().memory.peek(addr) != 0 {
            nonzero_bitmap += 1;
        }
    }
    println!("  Non-zero bitmap bytes: {}/6144", nonzero_bitmap);

    let mut attrs_38 = 0u32;
    for addr in 0x5800..0x5B00u16 {
        if spectrum2.bus().memory.peek(addr) == 0x38 {
            attrs_38 += 1;
        }
    }
    println!("  Attributes == $38: {}/768", attrs_38);

    // Check DF_CC (display file cursor)
    let df_cc = u16::from(spectrum2.bus().memory.peek(0x5C84))
        | (u16::from(spectrum2.bus().memory.peek(0x5C85)) << 8);
    println!("  DF_CC = ${:04X}", df_cc);

    // Check what the copyright message location looks like in memory
    // The copyright message starts at ROM address $1539 (varies by ROM version)
    // After boot, the message should be printed at the bottom of the screen
    // Let's check if anything was written to the screen's bottom rows
    println!("\n  Bottom screen row (line 184-191) bitmap bytes:");
    for row in 184..192u8 {
        let y7y6 = (row >> 6) & 0x03;
        let y5y4y3 = (row >> 3) & 0x07;
        let y2y1y0 = row & 0x07;
        let base: u16 = 0x4000
            | (u16::from(y7y6) << 11)
            | (u16::from(y2y1y0) << 8)
            | (u16::from(y5y4y3) << 5);
        let mut nonzero = 0;
        for col in 0..32u16 {
            if spectrum2.bus().memory.peek(base + col) != 0 {
                nonzero += 1;
            }
        }
        println!("    Line {}: base=${:04X}, non-zero bytes: {}/32", row, base, nonzero);
    }

    // Check top screen row too
    println!("\n  Top screen row (line 0-7) bitmap bytes:");
    for row in 0..8u8 {
        let y7y6 = (row >> 6) & 0x03;
        let y5y4y3 = (row >> 3) & 0x07;
        let y2y1y0 = row & 0x07;
        let base: u16 = 0x4000
            | (u16::from(y7y6) << 11)
            | (u16::from(y2y1y0) << 8)
            | (u16::from(y5y4y3) << 5);
        let mut nonzero = 0;
        for col in 0..32u16 {
            if spectrum2.bus().memory.peek(base + col) != 0 {
                nonzero += 1;
            }
        }
        println!("    Line {}: base=${:04X}, non-zero bytes: {}/32", row, base, nonzero);
    }
}
