//! Diagnostic: boot WB 1.3 on the A500+A501 model and capture the
//! old-address RTC traffic (`$DC0000`) that `SetClock load` sees.
//!
//! Run with:
//!   cargo test -p runtime-commodore-amiga --test diag_wb13_rtc \
//!       trace_wb13_setclock_rtc_accesses -- --ignored --nocapture

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use format_commodore_amiga_adf::Adf;
use machine_commodore_amiga_ocs::RTC_BASE;
use png::{BitDepth, ColorType, Encoder};
use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AmigaRuntime, DISPLAY_HEIGHT, DISPLAY_WIDTH, Model,
};

fn load_artifact(path: &Path) -> Option<Vec<u8>> {
    if !path.exists() {
        eprintln!("skipping: missing {}", path.display());
        return None;
    }
    Some(std::fs::read(path).ok()?)
}

fn snapshot_to_png(rt: &AmigaRuntime, path: &Path) {
    let fb = rt.machine().denise().framebuffer();
    assert_eq!(fb.len(), (DISPLAY_WIDTH * DISPLAY_HEIGHT) as usize);

    let mut rgb = Vec::with_capacity(fb.len() * 3);
    for &pixel in fb {
        rgb.push(((pixel >> 16) & 0xFF) as u8);
        rgb.push(((pixel >> 8) & 0xFF) as u8);
        rgb.push((pixel & 0xFF) as u8);
    }

    let file = std::fs::File::create(path).expect("create screenshot");
    let mut encoder = Encoder::new(file, DISPLAY_WIDTH, DISPLAY_HEIGHT);
    encoder.set_color(ColorType::Rgb);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&rgb).expect("png data");
}

#[test]
#[ignore = "needs local KS 1.3 ROM + WB 1.3 disk"]
fn trace_wb13_setclock_rtc_accesses() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let Some(rom) = load_artifact(&home.join(".emu198x/roms/commodore-amiga/kick13.rom")) else {
        return;
    };
    let Some(adf_bytes) =
        load_artifact(&home.join(".emu198x/media/commodore-amiga/workbench-1.3.adf"))
    else {
        return;
    };

    let mut rt = AmigaRuntime::new(Model::A500OcsPalA501, rom).expect("build runtime");
    let adf = Adf::from_bytes(adf_bytes).expect("decode WB 1.3 ADF");
    rt.machine_mut().insert_adf(adf);

    for _ in 0..(900 * A500_PAL_FRAME_TICKS) {
        rt.machine_mut().tick();
    }

    let screenshot = PathBuf::from("/tmp/a500-ks13-wb13-900-rtc-a501.png");
    snapshot_to_png(&rt, &screenshot);
    println!("screenshot: {}", screenshot.display());

    let rtc_log = &rt.machine().debug_rtc_log;
    println!("rtc access count: {}", rtc_log.len());
    if rtc_log.is_empty() {
        println!("no RTC accesses observed");
        return;
    }

    let mut reg_histogram = BTreeMap::<u32, usize>::new();
    for &(_, _, addr24, _, _, _) in rtc_log {
        let reg = ((addr24 - RTC_BASE) >> 2) & 0x0F;
        *reg_histogram.entry(reg).or_insert(0) += 1;
    }
    println!("rtc register histogram:");
    for (reg, count) in reg_histogram {
        println!("  reg ${reg:X}: {count}");
    }

    println!("first RTC accesses:");
    for &(cck, pc, addr24, is_read, is_word, value) in rtc_log.iter().take(64) {
        println!(
            "  cck={cck:>8} pc=${pc:08X} {} {} addr=${addr24:06X} reg=${:X} value=${value:04X}",
            if is_read { "read " } else { "write" },
            if is_word { "word" } else { "byte" },
            ((addr24 - RTC_BASE) >> 2) & 0x0F,
        );
    }
}
