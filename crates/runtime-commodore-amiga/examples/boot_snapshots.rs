//! Capture screen snapshots at several points during boot so we can see
//! visual progress beyond the insert-disk screen.

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

    let checkpoints = [500u32, 1500, 3000, 5000, 7500, 10000];
    let mut prev = 0u32;
    for cp in checkpoints {
        for _ in prev..cp {
            amiga.run_frame();
        }
        prev = cp;
        let path = format!("/tmp/amiga_boot_{cp:04}.ppm");
        save_ppm(&amiga, &path);
        println!(
            "[frame {cp:4}] motor={} spin={} sel={} cyl={} PC=${:08X}  →  {path}",
            amiga.floppy.motor_on(),
            amiga.floppy.motor_spinning(),
            amiga.floppy.selected(),
            amiga.floppy.cylinder(),
            amiga.cpu.instr_start_pc,
            path = path,
        );
    }
}

fn save_ppm(amiga: &Amiga, path: &str) {
    let fb = amiga.framebuffer();
    let w = RASTER_FB_WIDTH as usize;
    let h = PAL_RASTER_FB_HEIGHT as usize;
    let mut f = File::create(path).unwrap();
    writeln!(f, "P6").unwrap();
    writeln!(f, "{w} {h}").unwrap();
    writeln!(f, "255").unwrap();
    for &argb in fb.iter().take(w * h) {
        let r = ((argb >> 16) & 0xFF) as u8;
        let g = ((argb >> 8) & 0xFF) as u8;
        let b = (argb & 0xFF) as u8;
        f.write_all(&[r, g, b]).unwrap();
    }
}
