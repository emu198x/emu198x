//! Original-ROM qualification for #2237: inspect pixels, not a thumbnail.
use emu198x_shell::{HeadlessSession, MachineCore, PixelFormat};
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model, RamConfig};
use sha2::{Digest, Sha256};
use std::{
    io::{BufReader, Cursor},
    path::PathBuf,
};

const ROM_SHA256: &str = "ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53";
// The unchanged original-engine capture has this identity. Independent PNG
// decoders reveal its disk/hand graphic; the inspection thumbnail hid it.
const FRAME_SHA256: &str = "f28bfeb54f3397f7983077641057b561e150627267c133eaeafbd7552624d436";
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn matches_reference(pixels: &[u8]) -> bool {
    pixels.len() == 768 * 576 * 4 && hash(pixels) == FRAME_SHA256
}
fn decode_png_rgba(encoded: Vec<u8>) -> Vec<u8> {
    let mut decoder = png::Decoder::new(BufReader::new(Cursor::new(encoded)))
        .read_info()
        .expect("decode PNG");
    let mut decoded = vec![0; decoder.output_buffer_size().expect("bounded image")];
    let info = decoder
        .next_frame(&mut decoded)
        .expect("decode all PNG pixels");
    assert_eq!(
        (info.width, info.height, info.color_type),
        (768, 576, png::ColorType::Rgba)
    );
    decoded.truncate(info.buffer_size());
    decoded
}
fn assert_negative_controls(pixels: &[u8]) {
    let mut blank = pixels.to_vec();
    for p in blank.as_chunks_mut::<4>().0.iter_mut() {
        if p[..3] != [0, 0, 0] {
            p.copy_from_slice(&[255, 255, 255, 255]);
        }
    }
    assert!(
        !matches_reference(&blank),
        "white screen with black border must reject"
    );
    let mut mutated = pixels.to_vec();
    mutated[1000] ^= 1;
    assert!(
        !matches_reference(&mutated),
        "one changed pixel byte must reject"
    );
}
#[test]
fn original_capture_contains_the_disk_graphic_even_when_a_thumbnail_hides_it() {
    let pixels = decode_png_rgba(include_bytes!("fixtures/kick13-pal-mixed.png").to_vec());
    assert!(matches_reference(&pixels));
    assert_eq!(
        &pixels[(150 * 768 + 300) * 4..(150 * 768 + 300) * 4 + 4],
        &[119, 119, 204, 255]
    );
    assert_negative_controls(&pixels);
}
fn check(owner: &str, chip_kb: u32) {
    let root = std::env::var_os("EMU198X_AMIGA_REFERENCE_ROM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                .join(".emu198x/roms/commodore-amiga")
        });
    let rom = std::fs::read(root.join("kick13.rom")).expect("original Kickstart 1.3 fixture");
    assert_eq!(rom.len(), 256 * 1024);
    assert_eq!(
        hash(&rom),
        ROM_SHA256,
        "verify complete original firmware before boot"
    );
    let model = Model::from_variant_id(owner).expect("fixed owner");
    let config = model.config().with_ram(RamConfig {
        chip_kb,
        slow_kb: 0,
        fast_kb: 0,
    });
    let runtime = AmigaRuntimeKind::with_config(config, rom.clone()).expect("original ROM");
    let mut session = HeadlessSession::new(runtime, model.frame_ticks());
    session.run_frames(650).expect("bounded original boot");
    let frame = session.latest_frame().expect("completed native frame");
    assert_eq!(
        (frame.width, frame.height, frame.format),
        (768, 576, PixelFormat::Rgba8888)
    );
    assert!(
        matches_reference(&frame.pixels),
        "{owner}: original disk/hand graphic"
    );
    let native: Vec<_> = session
        .machine()
        .framebuffer()
        .iter()
        .flat_map(|p| {
            [
                ((p >> 16) & 255) as u8,
                ((p >> 8) & 255) as u8,
                (p & 255) as u8,
                (p >> 24) as u8,
            ]
        })
        .collect();
    assert_eq!(
        frame.pixels, native,
        "capture must agree with live native output"
    );
    assert_eq!(
        frame
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| **p == [119, 119, 204, 255])
            .count(),
        15200
    );
    assert_eq!(
        frame
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| **p == [187, 187, 187, 255])
            .count(),
        3628
    );
    let encoded = frame.png_bytes().expect("encode actual capture");
    let decoded = decode_png_rgba(encoded);
    assert_eq!(decoded, frame.pixels, "PNG must retain every native pixel");
    assert_negative_controls(&frame.pixels);
    let saved = session.machine().snapshot().expect("live snapshot");
    let mut restored = AmigaRuntimeKind::with_config(config, rom).expect("same owner");
    restored
        .restore(&saved)
        .expect("restore without changing the schema");
    let mut replay = HeadlessSession::new(restored, model.frame_ticks());
    session.run_frames(2).expect("original continuation");
    replay.run_frames(2).expect("restored continuation");
    assert_eq!(
        session.latest_frame().expect("original").pixels,
        replay.latest_frame().expect("restored").pixels
    );
    assert_eq!(
        session.machine().snapshot().expect("original"),
        replay.machine().snapshot().expect("restored")
    );
    println!(
        "{owner}: exact original identity; live/captured/decoded disk graphic; white/mutation controls; exact continuation"
    );
}
#[test]
#[ignore = "FIXTURE: original kick13.rom in EMU198X_AMIGA_REFERENCE_ROM_DIR or the conventional ROM directory"]
fn a500_rev6a_pal_kick13_boot_pixels_and_restore() {
    check("commodore-amiga-a500-rev6a-pal", 512);
}
#[test]
#[ignore = "FIXTURE: original kick13.rom in EMU198X_AMIGA_REFERENCE_ROM_DIR or the conventional ROM directory"]
fn a2000cr_rev6_pal_kick13_boot_pixels_and_restore() {
    check("commodore-amiga-a2000cr-rev6-pal", 1024);
}
