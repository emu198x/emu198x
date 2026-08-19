//! Diagnostic: sample the emulator's framebuffer at several points
//! during boot to test the "archive kept the frame that BPLEN=1
//! produced" hypothesis for task #191.
//!
//! If an early frame (during the brief BPLEN=1 window, roughly
//! 108-185) shows the hand-disk image and later frames show a
//! white rectangle, the OCS regression is that Denise is
//! overwriting the framebuffer with colour-0 pixels when BPLEN=0.
//! The archive's Denise (same crate!) presumably held its last
//! bitplane output instead.
//!
//! Run with:
//!   cargo test -p runtime-commodore-amiga --test diag_framebuffer_timeline \
//!       -- --ignored --nocapture

use std::error::Error;
use std::path::{Path, PathBuf};

use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AmigaOcsRuntime, DISPLAY_HEIGHT, DISPLAY_WIDTH, Model,
};

fn load_ks13() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        emu198x_test_skip::record(&format!(
            "skipping: KS 1.3 ROM missing at {}",
            path.display()
        ));
        return None;
    }
    std::fs::read(&path).ok()
}

fn snapshot_to_png(rt: &AmigaOcsRuntime, path: &Path) {
    let fb = rt.machine().denise().framebuffer();
    assert_eq!(fb.len(), (DISPLAY_WIDTH * DISPLAY_HEIGHT) as usize);
    let mut rgb = Vec::with_capacity(fb.len() * 3);
    for &pixel in fb {
        rgb.push(((pixel >> 16) & 0xFF) as u8);
        rgb.push(((pixel >> 8) & 0xFF) as u8);
        rgb.push((pixel & 0xFF) as u8);
    }
    let file = std::fs::File::create(path).expect("create snapshot");
    let mut encoder = png::Encoder::new(file, DISPLAY_WIDTH, DISPLAY_HEIGHT);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&rgb).expect("png data");
}

fn summarise(rt: &AmigaOcsRuntime) -> (u32, u32, u32) {
    let fb = rt.machine().denise().framebuffer();
    let mut white = 0u32;
    let mut black = 0u32;
    let mut other = 0u32;
    for &p in fb {
        let rgb = p & 0x00FF_FFFF;
        if rgb == 0x00FF_FFFF {
            white += 1;
        } else if rgb == 0x0000_0000 {
            black += 1;
        } else {
            other += 1;
        }
    }
    (white, black, other)
}

#[test]
#[ignore = "needs KS 1.3 ROM — run with --ignored"]
fn sample_framebuffer_across_boot() -> Result<(), Box<dyn Error>> {
    let Some(rom) = load_ks13() else {
        return Ok(());
    };
    let mut rt = AmigaOcsRuntime::new(Model::A500OcsPalA501, rom)?;
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens");
    std::fs::create_dir_all(&out_dir).ok();

    // Checkpoints bracketing the DMACON transitions:
    //   108 — BPLEN/SPREN turn on (display becomes active)
    //   120, 150, 180 — during BPLEN=1 window; should show hand-disk
    //   186 — one frame after BPLEN clears
    //   250 — the golden-matrix settle frame
    let checkpoints = [60u64, 120, 200, 250, 300, 400, 500, 600];
    let mut cp_iter = checkpoints.iter().peekable();

    println!("=== framebuffer summary by frame (white, black, other) ===");
    println!(" frame |   white   |   black   |   other   | DMACON");
    for frame in 1..=700u64 {
        for _ in 0..A500_PAL_FRAME_TICKS {
            rt.machine_mut().tick();
        }
        if let Some(&&cp) = cp_iter.peek()
            && frame == cp
        {
            cp_iter.next();
            let (w, b, o) = summarise(&rt);
            let d = rt.machine().agnus().dmacon;
            println!(
                "  {:>4} | {:>9} | {:>9} | {:>9} | ${:04X}",
                frame, w, b, o, d
            );
            let png_path = out_dir.join(format!("diag-frame-{frame:03}.png"));
            snapshot_to_png(&rt, &png_path);
        }
    }
    println!();
    println!("PNG snapshots saved to tests/goldens/diag-frame-NNN.png");
    Ok(())
}
