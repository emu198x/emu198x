//! Minimal hires bitplane rendering test.
//!
//! Sets up a 2-plane hires display with known data via copper list,
//! renders one frame, and checks pixel output.

use machine_amiga::memory::ROM_BASE;
use machine_amiga::{Amiga, TICKS_PER_CCK};

fn make_hires_test_amiga() -> Amiga {
    let mut rom = vec![0u8; 256 * 1024];
    // SSP
    let ssp = 0x0007_FFF0u32;
    rom[0..4].copy_from_slice(&ssp.to_be_bytes());
    // PC = ROM_BASE + 8 (BRA *-2 loop)
    let pc = ROM_BASE + 8;
    rom[4..8].copy_from_slice(&pc.to_be_bytes());
    rom[8] = 0x60;
    rom[9] = 0xFE; // BRA.S *
    Amiga::new(rom)
}

fn tick_one_frame(amiga: &mut Amiga) {
    // PAL frame = 227 CCKs × 312 lines
    let frame_ccks = 227 * 312;
    for _ in 0..frame_ccks {
        for _ in 0..TICKS_PER_CCK {
            amiga.tick();
        }
    }
}

#[test]
fn hires_2plane_renders_correct_pixels() {
    let mut amiga = make_hires_test_amiga();

    // Set up bitplane data in chip RAM.
    // Use a simple pattern: plane 0 = $FFFF (all 1s), plane 1 = $0000 (all 0s)
    // This should produce color 1 (from plane 0 only) for all 16 pixels.
    let bpl0_addr = 0x0002_0000u32; // plane 0 bitmap
    let bpl1_addr = 0x0002_8000u32; // plane 1 bitmap

    // Fill plane 0 with alternating $FF00 pattern (8 white pixels, 8 bg)
    for i in (0..80u32).step_by(2) {
        amiga.memory.write_byte(bpl0_addr + i, 0xFF);
        amiga.memory.write_byte(bpl0_addr + i + 1, 0x00);
    }
    // Fill plane 1 with all zeros
    for i in 0..80u32 {
        amiga.memory.write_byte(bpl1_addr + i, 0x00);
    }

    // Write copper list to set up hires 2-plane display.
    // Standard hires: DDFSTRT=$3C, DDFSTOP=$D4, DIWSTRT=$2C81, DIWSTOP=$2CC1
    let cop_addr = 0x0001_0000u32;
    let mut cop = cop_addr;
    let write_cop = |amiga: &mut Amiga, addr: &mut u32, reg: u16, val: u16| {
        amiga.memory.write_byte(*addr, (reg >> 8) as u8);
        amiga.memory.write_byte(*addr + 1, reg as u8);
        amiga.memory.write_byte(*addr + 2, (val >> 8) as u8);
        amiga.memory.write_byte(*addr + 3, val as u8);
        *addr += 4;
    };

    // BPLCON0: HIRES + 2 planes + COLOR
    write_cop(&mut amiga, &mut cop, 0x100, 0xA200);
    // BPLCON1: no scroll
    write_cop(&mut amiga, &mut cop, 0x102, 0x0000);
    // BPLCON2: default
    write_cop(&mut amiga, &mut cop, 0x104, 0x0024);
    // DDFSTRT/DDFSTOP for hires
    write_cop(&mut amiga, &mut cop, 0x092, 0x003C);
    write_cop(&mut amiga, &mut cop, 0x094, 0x00D4);
    // DIWSTRT/DIWSTOP
    write_cop(&mut amiga, &mut cop, 0x08E, 0x2C81);
    write_cop(&mut amiga, &mut cop, 0x090, 0x2CC1);
    // BPL1PT (plane 0)
    write_cop(&mut amiga, &mut cop, 0x0E0, (bpl0_addr >> 16) as u16);
    write_cop(&mut amiga, &mut cop, 0x0E2, bpl0_addr as u16);
    // BPL2PT (plane 1)
    write_cop(&mut amiga, &mut cop, 0x0E4, (bpl1_addr >> 16) as u16);
    write_cop(&mut amiga, &mut cop, 0x0E6, bpl1_addr as u16);
    // BPL1MOD / BPL2MOD = -80 to loop back to start of same line.
    // Hires 2-plane fetches 40 words/line = 80 bytes. Modulo of -80
    // resets the pointer to the line start after each scanline.
    let mod_val = (-80i16) as u16; // $FFB0
    write_cop(&mut amiga, &mut cop, 0x108, mod_val);
    write_cop(&mut amiga, &mut cop, 0x10A, mod_val);
    // Colors: 0=blue($00F), 1=white($FFF), 2=black($000), 3=orange($F80)
    write_cop(&mut amiga, &mut cop, 0x180, 0x000F); // COLOR00
    write_cop(&mut amiga, &mut cop, 0x182, 0x0FFF); // COLOR01
    write_cop(&mut amiga, &mut cop, 0x184, 0x0000); // COLOR02
    write_cop(&mut amiga, &mut cop, 0x186, 0x0F80); // COLOR03
    // End of copper list (WAIT $FFFF,$FFFE = never matches)
    amiga.memory.write_byte(cop, 0xFF);
    amiga.memory.write_byte(cop + 1, 0xFF);
    amiga.memory.write_byte(cop + 2, 0xFF);
    amiga.memory.write_byte(cop + 3, 0xFE);

    // Enable DMA (must be done via direct register write, not copper)
    // DMACON: SET + DMAEN + BPLEN + COPEN
    amiga.write_custom_reg(0x096, 0x8380);
    // Set COP1LC and start copper
    amiga.write_custom_reg(0x080, (cop_addr >> 16) as u16);
    amiga.write_custom_reg(0x082, cop_addr as u16);
    amiga.write_custom_reg(0x088, 0x0000); // COPJMP1

    // Run for 2 frames to let copper execute
    tick_one_frame(&mut amiga);
    tick_one_frame(&mut amiga);

    // Debug: check register state
    println!("BPLCON0 = ${:04X}", amiga.denise.bplcon0);
    println!("DMACON  = ${:04X}", amiga.agnus.dmacon);
    println!("DDFSTRT = ${:04X}", amiga.agnus.ddfstrt);
    println!("DDFSTOP = ${:04X}", amiga.agnus.ddfstop);
    println!("DIWSTRT = ${:04X}", amiga.agnus.diwstrt);
    println!("DIWSTOP = ${:04X}", amiga.agnus.diwstop);
    println!("BPL1PT  = ${:08X}", amiga.agnus.bpl_pt[0]);
    println!("BPL2PT  = ${:08X}", amiga.agnus.bpl_pt[1]);
    println!("BPL1MOD = {}", amiga.agnus.bpl1mod);
    println!("BPL2MOD = {}", amiga.agnus.bpl2mod);
    // Check if plane 0 data is at expected address
    println!(
        "Data at BPL0 base: {:02X} {:02X} {:02X} {:02X}",
        amiga.memory.read_chip_byte(bpl0_addr),
        amiga.memory.read_chip_byte(bpl0_addr + 1),
        amiga.memory.read_chip_byte(bpl0_addr + 2),
        amiga.memory.read_chip_byte(bpl0_addr + 3),
    );

    // Check raster output for line 100 (well within the display window).
    // The display window starts at line $2C = 44, so line 100 is active.
    // In the raster buffer, each line is at vpos*2 (double-height).
    // Hires pixels at hpos=64 (start of display window): raster_x = 64*4 = 256
    let fb_w = amiga.denise.raster_fb_width;
    let scan_row = 100 * 2; // double-height buffer
    let display_start_x = 64 * 4; // beam_x 128 = hpos 64, 4 hires pixels per CCK

    // Scan the ENTIRE scanline for non-black, non-blue pixels
    let blue = 0xFF_00_00_FFu32;
    let black = 0xFF_00_00_00u32;
    println!("\nScanning entire line 100 for non-background pixels:");
    for px in 0..fb_w {
        let idx = (scan_row * fb_w + px) as usize;
        let color = amiga.denise.framebuffer_raster[idx];
        if color != blue && color != black && color != 0xFF000000 && color != 0 {
            let r = (color >> 16) & 0xFF;
            let g = (color >> 8) & 0xFF;
            let b = color & 0xFF;
            print!("  x={px}: #{r:02X}{g:02X}{b:02X}");
        }
    }
    println!();

    // Collect the first 32 hires pixels from the display window
    let mut pixels = Vec::new();
    for px in 0..32 {
        let idx = (scan_row * fb_w + display_start_x + px) as usize;
        let color = amiga.denise.framebuffer_raster[idx];
        pixels.push(color);
    }

    println!("First 32 hires pixels at line 100:");
    for (i, &c) in pixels.iter().enumerate() {
        let r = (c >> 16) & 0xFF;
        let g = (c >> 8) & 0xFF;
        let b = c & 0xFF;
        print!("{i:2}: #{r:02X}{g:02X}{b:02X}  ");
        if i % 8 == 7 {
            println!();
        }
    }

    let blue = 0xFF_00_00_FFu32;

    // With plane 0 = $FF00 pattern: first 8 hires pixels should be color 1 (white),
    // next 8 should be color 0 (blue), repeating.
    let non_blue_count = pixels
        .iter()
        .filter(|&&c| c != blue && c != 0xFF000000)
        .count();
    println!("\nNon-blue pixels: {non_blue_count} / {}", pixels.len());

    // Save a screenshot to verify visually
    use machine_amiga::commodore_denise_ocs::ViewportPreset;
    let viewport = amiga
        .denise
        .extract_viewport(ViewportPreset::Standard, true, true);
    let path = "../../test_output/amiga/hires_test_pattern.png";
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let file = std::fs::File::create(path).expect("create file");
    let w = &mut std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, viewport.width, viewport.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("header");
    let mut rgba = Vec::with_capacity((viewport.width * viewport.height * 4) as usize);
    for &pixel in &viewport.pixels {
        rgba.push(((pixel >> 16) & 0xFF) as u8);
        rgba.push(((pixel >> 8) & 0xFF) as u8);
        rgba.push((pixel & 0xFF) as u8);
        rgba.push(((pixel >> 24) & 0xFF) as u8);
    }
    writer.write_image_data(&rgba).expect("data");
    println!("Saved {path} ({}x{})", viewport.width, viewport.height);

    // Check the non-blue count in the viewport window area
    let non_blue_count = pixels
        .iter()
        .filter(|&&c| c != blue && c != 0xFF000000)
        .count();
    assert!(
        non_blue_count > 0,
        "expected some white pixels in display window"
    );
}
