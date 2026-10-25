use super::*;

/// A 192 KB ROM whose reset vectors start a loop at `$FC0008`:
/// `move.w #$2700,sr; bra.s *`.
fn loop_rom() -> Vec<u8> {
    let mut rom = vec![0; 0x3_0000];
    rom[0..4].copy_from_slice(&0x0000_8000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00FC_0008u32.to_be_bytes());
    rom[8..14].copy_from_slice(&[0x46, 0xFC, 0x27, 0x00, 0x60, 0xFE]);
    rom
}

fn st(model: Model, standard: VideoStandard, monitor: Monitor) -> AtariSt {
    AtariSt::new(model, standard, monitor, loop_rom()).expect("192 KB ROM")
}

#[test]
fn tos_images_must_be_192_or_256_kb() {
    let error = AtariSt::new(
        Model::St520,
        VideoStandard::Pal,
        Monitor::Colour,
        vec![0; 1000],
    )
    .err()
    .expect("refused");
    assert_eq!(error, RomError::Size(1000));
    assert!(error.to_string().contains("1000"));
    let mut rom = loop_rom();
    rom.resize(0x4_0000, 0);
    let machine =
        AtariSt::new(Model::St1040, VideoStandard::Ntsc, Monitor::Mono, rom).expect("256 KB ROM");
    assert_eq!(machine.ram().len(), 1024 * 1024);
    assert_eq!(machine.cpu_hz(), NTSC_MASTER_HZ / 4);
    assert_eq!(machine.master_hz(), NTSC_MASTER_HZ);
    assert_eq!(machine.model(), Model::St1040);
    assert_eq!(machine.standard(), VideoStandard::Ntsc);
    assert_eq!(machine.monitor(), Monitor::Mono);
}

#[test]
fn a_pal_frame_is_313_lines_of_512_cycles() {
    let mut machine = st(Model::St520, VideoStandard::Pal, Monitor::Colour);
    machine.run_frame();
    assert_eq!(machine.run_frame(), 313 * 512);
    assert_eq!(machine.frame_width(), COLOUR_WIDTH);
    assert_eq!(machine.frame_height(), COLOUR_HEIGHT);
    assert_eq!(machine.framebuffer().len(), COLOUR_WIDTH * COLOUR_HEIGHT);
}

#[test]
fn the_reset_vectors_come_from_rom_and_ram_is_mapped() {
    let mut machine = st(Model::St520, VideoStandard::Pal, Monitor::Colour);
    assert_eq!(machine.peek(4), 0x00);
    assert_eq!(machine.peek(5), 0xFC);
    assert_eq!(machine.peek(0xFC_0008), 0x46);
    machine.poke(0x1000, 0xAB);
    assert_eq!(machine.peek(0x1000), 0xAB);
    assert_eq!(machine.peek(0xFF_8800), 0xFF);
    for _ in 0..1000 {
        machine.tick();
    }
    assert_eq!(machine.cycles(), 1000);
}

#[test]
fn the_mono_monitor_pulls_gpip_bit_7_low() {
    let colour = st(Model::St520, VideoStandard::Pal, Monitor::Colour);
    let mono = st(Model::St520, VideoStandard::Pal, Monitor::Mono);
    assert_eq!(
        colour.mfp.peek(emu198x_motorola_mfp_68901::reg::GPIP) & 0x80,
        0x80
    );
    assert_eq!(
        mono.mfp.peek(emu198x_motorola_mfp_68901::reg::GPIP) & 0x80,
        0
    );
}

#[test]
fn supervisor_io_and_bus_errors_follow_the_stf_map() {
    let mut machine = st(Model::St520, VideoStandard::Pal, Monitor::Colour);
    assert_eq!(machine.read(0x40_0000, true, true), None, "past RAM");
    assert_eq!(machine.read(0xFF_8A00, true, true), None, "no blitter");
    assert_eq!(machine.read(0xFF_8240, true, false), None, "user I/O");
    assert_eq!(machine.read(0x400, true, false), None, "user low RAM");
    assert_eq!(
        machine.read(0xFA_0000, true, false),
        Some(0xFFFF),
        "cartridge"
    );
    assert_eq!(machine.write(0xFC_0000, true, 0, true), None, "ROM write");
    assert_eq!(
        machine.read(0xFF_820B, false, true),
        Some(0xFF),
        "void byte"
    );
    assert_eq!(machine.read(0xFF_FA00, false, true), None, "MFP even byte");

    machine
        .write(0xFF_8240, true, 0x0777, true)
        .expect("palette");
    assert_eq!(machine.read(0xFF_8240, true, true), Some(0x0777));
    machine
        .write(0xFF_8201, false, 0x12, true)
        .expect("video base high");
    assert_eq!(machine.read(0xFF_8201, false, true), Some(0x12));
    machine
        .write(0xFF_8260, false, 0x01, true)
        .expect("resolution");
    assert_eq!(machine.read(0xFF_8260, false, true).map(|v| v & 3), Some(1));
}

#[test]
fn psg_port_a_selects_the_floppy_drive_and_side() {
    let mut machine = st(Model::St520, VideoStandard::Pal, Monitor::Colour);
    machine
        .write(0xFF_8800, false, 7, true)
        .expect("select mixer");
    machine
        .write(0xFF_8802, false, 0xC0, true)
        .expect("ports out");
    machine
        .write(0xFF_8800, false, 14, true)
        .expect("select port A");
    machine
        .write(0xFF_8802, false, 0x04, true)
        .expect("drive A, side 1");
    assert_eq!(machine.floppy_side, 1);
    machine
        .write(0xFF_8800, false, 14, true)
        .expect("select port A");
    assert_eq!(machine.read(0xFF_8800, false, true), Some(0x04));
}

#[test]
fn disk_images_load_in_either_format() {
    let raw = vec![0u8; 737_280];
    let disk = disk_from_image(&raw).expect(".st");
    assert_eq!(disk.data().len(), 737_280);
    let msa = format_atari_st_msa::write(9, 2, &raw);
    let disk = disk_from_image(&msa).expect(".msa");
    assert_eq!(disk.data().len(), 737_280);
    assert_eq!(msa_from_disk(&disk), Some(msa));
    assert_eq!(msa_from_disk(&Disk::new(vec![0; 100], 1, 1, 1, 512)), None);
    assert!(disk_from_image(&[0x0E, 0x0F, 0]).is_err());
    let error = disk_from_st(&[0; 100]).expect_err("too small");
    assert!(!error.to_string().is_empty());

    let mut machine = st(Model::St520, VideoStandard::Pal, Monitor::Colour);
    machine.insert_disk(0, disk);
    assert!(machine.disk(0).is_some());
    machine.reset();
    assert!(machine.disk(0).is_some(), "reset keeps the disk in");
    assert!(machine.eject_disk(0).is_some());
}

#[test]
fn a_snapshot_round_trips_without_the_rom() {
    let mut machine = st(Model::St520, VideoStandard::Pal, Monitor::Colour);
    machine.run_frame();
    let json = serde_json::to_string(&machine).expect("serialise");
    let mut restored: AtariSt = serde_json::from_str(&json).expect("deserialise");
    assert!(restored.rom().is_empty());
    restored.finish_restore(loop_rom()).expect("ROM");
    assert_eq!(restored.cycles(), machine.cycles());
    assert_eq!(restored.psg.variant(), Variant::Ym2149);
    assert_eq!(restored.run_frame(), machine.run_frame());
}

#[test]
fn audio_comes_out_at_48_khz() {
    let mut machine = st(Model::St520, VideoStandard::Pal, Monitor::Colour);
    for _ in 0..10 {
        machine.run_frame();
    }
    let samples = machine.take_audio().len() as u64;
    let expected = 10 * 313 * 512 * u64::from(AUDIO_SAMPLE_RATE) / machine.cpu_hz();
    assert!(samples.abs_diff(expected) <= 1, "{samples} vs {expected}");
}

// ---- EmuTOS ----

fn emutos() -> Option<Vec<u8>> {
    let home = std::env::var_os("HOME")?;
    let path = std::path::Path::new(&home).join(".emu198x/test-data/emutos/etos192us.img");
    std::fs::read(path).ok()
}

/// The fraction of `pixels` equal to `colour`.
fn share(pixels: &[u32], colour: u32) -> f64 {
    pixels.iter().filter(|&&p| p == colour).count() as f64 / pixels.len() as f64
}

#[test]
fn emutos_boots_to_the_colour_desktop() {
    let Some(rom) = emutos() else {
        emu198x_test_skip::skip!("EmuTOS 192K image not in ~/.emu198x/test-data/emutos");
    };
    let mut machine =
        AtariSt::new(Model::St520, VideoStandard::Pal, Monitor::Colour, rom).expect("EmuTOS");
    for _ in 0..50 * 12 {
        machine.run_frame();
    }
    if let Some(path) = std::env::var_os("ST_DUMP") {
        dump(&machine, std::path::Path::new(&path));
    }
    let width = machine.frame_width();
    let row = |y: usize| &machine.framebuffer()[y * width..(y + 1) * width];
    // Low resolution, 320 × 200 doubled, inside 64 pixels of border.
    let screen_row = |y: usize| &row(usize::from(BORDER_LINES) + y)[64..64 + 640];
    // The menu bar is white on the top line of text.
    let menu: Vec<u32> = (2..8).flat_map(|y| screen_row(y).to_vec()).collect();
    assert!(share(&menu, 0xFF_FFFF) > 0.5, "menu bar");
    // The desktop beneath is a green and white dither, edge to edge.
    for (left, right) in [(0, 40), (300, 340), (600, 640)] {
        let desk: Vec<u32> = (60..180)
            .flat_map(|y| screen_row(y)[left..right].to_vec())
            .collect();
        let green = share(&desk, 0x00_FF00);
        assert!(
            green > 0.4 && green < 0.6,
            "green desktop at {left}: {green}"
        );
    }
}

#[test]
fn emutos_boots_to_the_mono_desktop() {
    let Some(rom) = emutos() else {
        emu198x_test_skip::skip!("EmuTOS 192K image not in ~/.emu198x/test-data/emutos");
    };
    let mut machine =
        AtariSt::new(Model::St1040, VideoStandard::Pal, Monitor::Mono, rom).expect("EmuTOS");
    for _ in 0..71 * 12 {
        machine.run_frame();
    }
    if let Some(path) = std::env::var_os("ST_DUMP_MONO") {
        dump(&machine, std::path::Path::new(&path));
    }
    assert_eq!(machine.frame_width(), MONO_WIDTH);
    let pixels = machine.framebuffer();
    let black = share(pixels, 0);
    // The menu bar's white runs edge to edge, below its row of text.
    assert_eq!(
        share(&pixels[17 * MONO_WIDTH..18 * MONO_WIDTH], 0xFF_FFFF),
        1.0,
        "menu bar"
    );
    // The desktop is a dither pattern, the menu bar white with black text.
    assert!(black > 0.2 && black < 0.8, "black share {black}");
}

/// Write the framebuffer as a binary PPM, for looking at by eye.
fn dump(machine: &AtariSt, path: &std::path::Path) {
    let mut out = format!(
        "P6 {} {} 255\n",
        machine.frame_width(),
        machine.frame_height()
    )
    .into_bytes();
    for &pixel in machine.framebuffer() {
        out.extend([(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8]);
    }
    std::fs::write(path, out).expect("dump");
}

/// A 720 KB disk whose executable boot sector stores `$CAFEBABE` at `$600`.
fn boot_disk() -> Vec<u8> {
    let mut disk = vec![0u8; 737_280];
    let boot = &mut disk[..512];
    boot[0..2].copy_from_slice(&[0x60, 0x3C]);
    boot[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
    boot[0x0D] = 2;
    boot[0x0E..0x10].copy_from_slice(&1u16.to_le_bytes());
    boot[0x10] = 2;
    boot[0x11..0x13].copy_from_slice(&112u16.to_le_bytes());
    boot[0x13..0x15].copy_from_slice(&1440u16.to_le_bytes());
    boot[0x15] = 0xF9;
    boot[0x16..0x18].copy_from_slice(&5u16.to_le_bytes());
    boot[0x18..0x1A].copy_from_slice(&9u16.to_le_bytes());
    boot[0x1A..0x1C].copy_from_slice(&2u16.to_le_bytes());
    // move.l #$CAFEBABE,$600.w; rts
    boot[0x3E..0x48].copy_from_slice(&[0x21, 0xFC, 0xCA, 0xFE, 0xBA, 0xBE, 0x06, 0x00, 0x4E, 0x75]);
    // TOS runs a boot sector whose words sum to $1234.
    let sum = boot[..510].chunks(2).fold(0u16, |sum, w| {
        sum.wrapping_add(u16::from_be_bytes([w[0], w[1]]))
    });
    boot[510..512].copy_from_slice(&0x1234u16.wrapping_sub(sum).to_be_bytes());
    disk
}

#[test]
fn emutos_runs_a_floppy_boot_sector_through_the_dma_chip() {
    let Some(rom) = emutos() else {
        emu198x_test_skip::skip!("EmuTOS 192K image not in ~/.emu198x/test-data/emutos");
    };
    let mut machine =
        AtariSt::new(Model::St520, VideoStandard::Pal, Monitor::Colour, rom).expect("EmuTOS");
    machine.insert_disk(0, disk_from_st(&boot_disk()).expect("720 KB"));
    for _ in 0..50 * 10 {
        machine.run_frame();
        if machine.peek(0x600) == 0xCA {
            break;
        }
    }
    let marker: Vec<u8> = (0x600..0x604).map(|a| machine.peek(a)).collect();
    assert_eq!(marker, [0xCA, 0xFE, 0xBA, 0xBE]);
}
