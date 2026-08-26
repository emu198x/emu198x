//! Diagnostic: load Chase H.Q. +3 on the +3, press ENTER for Loader,
//! then sample the Z80 PC over time + dump screen text. Used to figure
//! out which ROM routine the BIOS is stuck in during the disk-load
//! hang documented at
//! `knowledge/decisions/spectrum-plus3-disk-loading-incomplete.md`.
//!
//! Run with:
//!
//!     cargo test -p runtime-sinclair-zx-spectrum \
//!         --test plus3_disk_trace -- --ignored --nocapture

use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;

use emu198x_shell::{
    FirmwareImage, FirmwareSet, HeadlessSession, InputEvent, MediaImage, MediaKind, MediaSet,
    read_firmware_asset, read_media_asset,
};

use common_sinclair_zx_spectrum::timing::TIMING_PLUS2A;
use machine_sinclair_zx_spectrum_plus3::SpectrumPlus3;
use runtime_sinclair_zx_spectrum::{Model, SpectrumPlus3Runtime, SpectrumSessionQueryProvider};

fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[test]
#[ignore = "DIAGNOSTIC: diagnostic — needs +3 ROMs and a +3 DSK (default: Chase H.Q.)"]
fn trace_plus3_loader_pc_histogram() {
    let firmware_root = home().join(".emu198x/roms/amstrad-zx-spectrum-plus3");
    // Override target with `PLUS3_TRACE_DSK=<filename inside the reference [DSK] dir>`
    // so the same diagnostic can be re-pointed at the failure cases
    // (Operation Wolf, RoboCop, Where Time Stood Still, Turrican, …)
    // without recompiling. Default is the title we know loads end-to-end.
    let dsk_file = env::var("PLUS3_TRACE_DSK")
        .unwrap_or_else(|_| "Chase H.Q. (1989)(Ocean)(+3).zip".to_owned());
    let dsk_path = home()
        .join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[DSK]")
        .join(&dsk_file);
    eprintln!("=== Tracing {dsk_file} ===");
    if !firmware_root.exists() || !dsk_path.exists() {
        emu198x_test_skip::skip!("missing +3 ROMs or Chase H.Q. (+3) DSK");
    }

    let mut firmware_set_storage: Vec<Vec<u8>> = Vec::with_capacity(4);
    let mut firmware_set = FirmwareSet::new();
    for i in 0..4 {
        let path = firmware_root.join(format!("plus3-{i}.rom"));
        let bytes = read_firmware_asset(&path).expect("plus3 rom");
        firmware_set_storage.push(bytes.bytes);
    }
    for (i, bytes) in firmware_set_storage.iter().enumerate() {
        firmware_set.push(FirmwareImage::new(
            format!("sinclair-zx-spectrum-plus3-rom-{i}"),
            bytes,
        ));
    }

    let mut machine = SpectrumPlus3::new();
    machine.memory.load_roms(
        &firmware_set_storage[0],
        &firmware_set_storage[1],
        &firmware_set_storage[2],
        &firmware_set_storage[3],
    );
    let runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, machine);

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PLUS2A.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media_loaded = read_media_asset(&dsk_path, MediaKind::Disk).expect("dsk media");
    let mut media_set = MediaSet::new();
    media_set.push(MediaImage::new(
        "disk-a".to_owned(),
        MediaKind::Disk,
        &media_loaded.bytes,
    ));
    session.prepare(&media_set, &[]).expect("prepare");

    session
        .wait_for_boot(250)
        .expect("plus3 boot banner appears");
    session.run_frames(50).expect("menu settle");

    eprintln!("=== Pressing ENTER for Loader ===");
    session.queue_input(InputEvent::Key {
        name: "enter".into(),
        pressed: true,
    });
    session.run_frames(5).expect("enter press");
    session.queue_input(InputEvent::Key {
        name: "enter".into(),
        pressed: false,
    });

    // Sample the Z80 PC at frame boundaries. We bin into 256-byte
    // pages so the histogram doesn't blow up.
    let mut page_hits: BTreeMap<u8, u32> = BTreeMap::new();
    let mut last_pcs: Vec<u16> = Vec::new();
    let frame_budget: usize = std::env::var("PLUS3_TRACE_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    eprintln!("=== Sampling {frame_budget} frames of Z80 PC ===");
    for _ in 0..frame_budget {
        session.run_frames(1).expect("frame");
        let pc = session.machine().machine().z80.regs.pc;
        *page_hits.entry((pc >> 8) as u8).or_insert(0) += 1;
        last_pcs.push(pc);
    }

    eprintln!("\n=== PC page histogram ({frame_budget} frame samples) ===");
    let mut pages: Vec<_> = page_hits.iter().collect();
    pages.sort_by(|a, b| b.1.cmp(a.1));
    for (page, count) in pages.iter().take(10) {
        eprintln!("  page {:#06x}xx : {} samples", (**page as u16) << 8, count);
    }

    eprintln!("\n=== Last 20 PCs ===");
    for pc in last_pcs.iter().rev().take(20) {
        eprintln!("  PC = {pc:#06x}");
    }

    // CPU state at the stuck point — does the loader have interrupts
    // enabled?  The border-stripe loop in many tape/disk loaders is
    // *deliberately* infinite; an IRQ service routine breaks out when
    // the next chunk is ready. If IFF1 is false and we're sat in
    // a border loop, the loader is waiting on something we're not
    // delivering (NMI? polled I/O?).
    {
        let z80 = &session.machine().machine().z80;
        eprintln!(
            "\n=== CPU state ===  iff1={}  iff2={}  i=${:02x}  halt={}",
            z80.regs.iff1, z80.regs.iff2, z80.regs.i, z80.halt,
        );
    }

    // Optional second memory dump at a fixed address (for tracing the
    // loader's *decision* code, which can be far from where execution
    // ended up trapped). Set PLUS3_TRACE_DUMP=0xFEA4 to dump 256 bytes
    // starting at $FEA4.
    if let Ok(addr_s) = env::var("PLUS3_TRACE_DUMP")
        && let Some(s) = addr_s
            .strip_prefix("0x")
            .or_else(|| addr_s.strip_prefix("0X"))
        && let Ok(addr) = u16::from_str_radix(s, 16)
    {
        eprintln!("\n=== Memory dump at ${addr:04x}..+0x100 ===");
        let mem = &session.machine().machine().memory;
        for off in 0..0x100u16 {
            let a = addr.wrapping_add(off);
            let b = common_sinclair_zx_spectrum::memory::MemoryBus::read(mem, a);
            if off % 16 == 0 {
                eprint!("  ${a:04x}:");
            }
            eprint!(" {b:02x}");
            if off % 16 == 15 {
                eprintln!();
            }
        }
    }

    // Dump 256 bytes around the most-recent PC so we can disassemble
    // the polling loop the loader is stuck in.
    if let Some(&latest_pc) = last_pcs.last() {
        let start = latest_pc.wrapping_sub(0x80);
        eprintln!("\n=== Memory at PC-0x80..PC+0x80 (latest PC=${latest_pc:04x}) ===");
        let mem = &session.machine().machine().memory;
        let mut bytes = Vec::with_capacity(256);
        for off in 0..256u16 {
            let addr = start.wrapping_add(off);
            bytes.push(common_sinclair_zx_spectrum::memory::MemoryBus::read(
                mem, addr,
            ));
        }
        for (i, b) in bytes.iter().enumerate() {
            let addr = start.wrapping_add(i as u16);
            if i % 16 == 0 {
                eprint!("  ${addr:04x}:");
            }
            eprint!(" {b:02x}");
            if i % 16 == 15 {
                eprintln!();
            }
        }
        if bytes.len() % 16 != 0 {
            eprintln!();
        }
    }

    // Dump the rendered screen text.
    eprintln!("\n=== Screen text after {frame_budget} frames ===");
    if let Ok(result) = session.query("screen.text.lines")
        && let Some(lines) = result.value.as_array()
    {
        for (row, line) in lines.iter().enumerate() {
            eprintln!("  {row:2}: {}", line.as_str().unwrap_or(""));
        }
    }

    // Dump the live framebuffer to /tmp as a 16-colour indexed PNG so
    // bitmap-heavy loader / title screens (which the text scraper can't
    // read because they aren't tile-based) can be eyeballed directly.
    let fb = session.machine().machine().framebuffer.clone();
    let png_path = PathBuf::from("/tmp/plus3_disk_trace.png");
    let palette_rgb = {
        let mut bytes = Vec::with_capacity(common_sinclair_zx_spectrum::SPECTRUM_PALETTE.len() * 3);
        for entry in &common_sinclair_zx_spectrum::SPECTRUM_PALETTE {
            let r = ((entry >> 24) & 0xFF) as u8;
            let g = ((entry >> 16) & 0xFF) as u8;
            let b = ((entry >> 8) & 0xFF) as u8;
            bytes.extend_from_slice(&[r, g, b]);
        }
        bytes
    };
    let file = std::fs::File::create(&png_path).expect("create png");
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, 352, 296);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(palette_rgb);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&fb).expect("png data");
    eprintln!("\n=== Framebuffer dumped to {} ===", png_path.display());
}
