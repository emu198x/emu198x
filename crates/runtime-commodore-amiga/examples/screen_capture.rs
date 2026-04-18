//! Capture the framebuffer after 400 frames with disk inserted. Writes
//! a simple PPM to /tmp so we can see what the user sees.

use emu198x_shell::{MediaKind, read_media_asset};
use machine_commodore_amiga::{Amiga, PAL_RASTER_FB_HEIGHT, RASTER_FB_WIDTH};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);

    let disk_path = "/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip";
    let loaded = read_media_asset(Path::new(disk_path), MediaKind::Disk).unwrap();
    let adf = format_commodore_amiga_adf::Adf::from_bytes(loaded.bytes).unwrap();
    amiga.insert_disk(adf);
    amiga.floppy.acknowledge_disk_change();

    for _ in 0..400 {
        amiga.run_frame();
    }

    let fb = amiga.framebuffer();
    let w = RASTER_FB_WIDTH as usize;
    let h = PAL_RASTER_FB_HEIGHT as usize;
    let mut f = File::create("/tmp/amiga_screen.ppm").unwrap();
    writeln!(f, "P6").unwrap();
    writeln!(f, "{w} {h}").unwrap();
    writeln!(f, "255").unwrap();
    for &argb in fb.iter().take(w * h) {
        let r = ((argb >> 16) & 0xFF) as u8;
        let g = ((argb >> 8) & 0xFF) as u8;
        let b = (argb & 0xFF) as u8;
        f.write_all(&[r, g, b]).unwrap();
    }
    println!("Wrote /tmp/amiga_screen.ppm ({w}x{h})");

    // Sample a few representative pixels.
    let fb_slice = &fb[..];
    println!("Pixel samples (ARGB):");
    for y in (0..h).step_by(h / 10) {
        let p = fb_slice[y * w + w / 2];
        println!("  y={y:>3} mid: ${p:08X}");
    }
}
