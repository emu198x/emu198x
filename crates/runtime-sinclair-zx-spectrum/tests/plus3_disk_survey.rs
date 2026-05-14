//! Diagnostic survey: load a list of +3 DSK titles, press ENTER on the
//! Loader menu option, run for a fixed number of frames, and print the
//! framebuffer hash + a PNG dump for each. Used to seed catalogue
//! entries after the disk-loading fix landed — copy the printed hashes
//! into `manifest/spectrum.toml` and let the catalogue runner verify.
//!
//!     cargo test -p runtime-sinclair-zx-spectrum \
//!         --test plus3_disk_survey -- --ignored --nocapture

use std::env;
use std::path::{Path, PathBuf};

use emu198x_shell::{
    FirmwareImage, FirmwareSet, HeadlessSession, InputEvent, MediaImage, MediaKind, MediaSet,
    read_firmware_asset, read_media_asset,
};

use std::hash::Hasher;

use common_sinclair_zx_spectrum::SPECTRUM_PALETTE;
use common_sinclair_zx_spectrum::timing::TIMING_PLUS2A;
use machine_sinclair_zx_spectrum_plus3::SpectrumPlus3;
use runtime_sinclair_zx_spectrum::{Model, SpectrumPlus3Runtime, SpectrumSessionQueryProvider};
use twox_hash::XxHash64;

/// Mirror of `emu198x_catalogue::hash_xxh64` so the printed values can
/// be pasted directly into the manifest.
fn hash_xxh64(bytes: &[u8]) -> String {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(bytes);
    format!("xxh64:{:016x}", hasher.finish())
}

fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn dsk_root() -> PathBuf {
    home().join("Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[DSK]")
}

/// (catalogue-id, display name, DSK filename relative to dsk_root)
fn survey_titles() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "chase-hq-plus3",
            "Chase H.Q. (+3) [Speedlock 7+, Ocean]",
            "Chase H.Q. (1989)(Ocean)(+3).zip",
        ),
        (
            "rainbow-islands-plus3",
            "Rainbow Islands (+3) [Speedlock, Ocean]",
            "Rainbow Islands - The Story of Bubble Bobble 2 (1990)(Ocean)(+3).zip",
        ),
        (
            "operation-wolf-plus3",
            "Operation Wolf (+3) [Speedlock, Ocean]",
            "Operation Wolf (1988)(Ocean)(+3).zip",
        ),
        (
            "robocop-plus3",
            "RoboCop (+3) [Speedlock, Ocean]",
            "RoboCop (1988)(Ocean)(+3).zip",
        ),
        (
            "where-time-stood-still-plus3",
            "Where Time Stood Still (+3) [Speedlock, Ocean]",
            "Where Time Stood Still (1988)(Ocean)(+3)[aka Land That Time Forgot, The][aka Tibet].zip",
        ),
        (
            "cybernoid-plus3",
            "Cybernoid (+3) [Hewson, custom]",
            "Cybernoid - The Fighting Machine (1988)(Hewson Consultants)(+3).zip",
        ),
        (
            "cybernoid-2-plus3",
            "Cybernoid II (+3) [Hewson, custom]",
            "Cybernoid II - The Revenge (1988)(Hewson Consultants)(+3).zip",
        ),
        (
            "saboteur-ii-plus3",
            "Saboteur II (+3) [Durell]",
            "Saboteur II - Avenging Angel (1987)(Durell)(+3)[h Alex Rider, 2015][tr pl][Speed-up version].zip",
        ),
        (
            "turrican-plus3",
            "Turrican (+3) Side A [Rainbow Arts]",
            "Turrican (1990)(Rainbow Arts)(+3)(Side A).zip",
        ),
        // ─── Wider protection coverage: distinct loader families ───
        (
            "stormlord-plus3",
            "Stormlord (+3) [Hewson, Alkatraz]",
            "Stormlord (1989)(Hewson Consultants)(+3).zip",
        ),
        (
            "sim-city-plus3",
            "Sim City (+3) [Infogrames, plain +3DOS]",
            "Sim City (1990)(Infogrames)(+3)(FR)(en).zip",
        ),
        (
            "starglider-2-plus3",
            "Starglider 2 (+3) [Rainbird]",
            "Starglider 2 - The Egrons Strike Back (1989)(Rainbird)(+3).zip",
        ),
        (
            "lotus-esprit-plus3",
            "Lotus Esprit Turbo Challenge (+3) [Gremlin]",
            "Lotus Esprit Turbo Challenge (1990)(Gremlin Graphics)(+3).zip",
        ),
        (
            "combat-school-plus3",
            "Combat School (+3) [Ocean, pre-Speedlock]",
            "Combat School + Gryzor Preview (1987)(Ocean)(+3).zip",
        ),
        (
            "dragon-ninja-plus3",
            "Bad Dudes vs. Dragon Ninja (+3) [Imagine/Ocean]",
            "Bad Dudes vs. Dragon Ninja (1988)(Imagine)(+3).zip",
        ),
        (
            "tetris-plus3",
            "Tetris (+3) [Mirrorsoft, format-track protection]",
            "Tetris (1988)(Mirrorsoft)(+3).zip",
        ),
    ]
}

#[test]
#[ignore = "diagnostic — needs +3 ROMs and a local DSK reference library"]
fn survey_plus3_disk_titles() {
    let firmware_root = home().join(".emu198x/roms/amstrad-zx-spectrum-plus3");
    if !firmware_root.exists() {
        eprintln!("[skip] missing +3 ROMs at {firmware_root:?}");
        return;
    }

    let mut firmware_set_storage: Vec<Vec<u8>> = Vec::with_capacity(4);
    for i in 0..4 {
        let path = firmware_root.join(format!("plus3-{i}.rom"));
        let bytes = read_firmware_asset(&path).expect("plus3 rom");
        firmware_set_storage.push(bytes.bytes);
    }

    let frame_budget: u32 = env::var("SURVEY_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6000);

    eprintln!(
        "=== Plus3 DSK survey ({} frames per title) ===",
        frame_budget
    );

    let dsk_root = dsk_root();
    for (id, label, dsk_file) in survey_titles() {
        let dsk_path = dsk_root.join(dsk_file);
        if !dsk_path.exists() {
            eprintln!("[skip] {id} — DSK not found at {dsk_path:?}");
            continue;
        }

        let result = run_one(id, &firmware_set_storage, &dsk_path, frame_budget);
        match result {
            Ok((fb_hash, audio_hash)) => {
                eprintln!("[OK]    {id:35} frame={fb_hash}  audio={audio_hash}  ({label})");
            }
            Err(reason) => {
                eprintln!("[ERR]   {id:35} {reason}  ({label})");
            }
        }
    }

    eprintln!("=== Survey complete ===");
}

fn run_one(
    id: &str,
    firmware_set_storage: &[Vec<u8>],
    dsk_path: &Path,
    frame_budget: u32,
) -> Result<(String, String), String> {
    let mut firmware_set = FirmwareSet::new();
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

    let media_loaded =
        read_media_asset(dsk_path, MediaKind::Disk).map_err(|err| format!("read media: {err}"))?;
    let mut media_set = MediaSet::new();
    media_set.push(MediaImage::new(
        "disk-a".to_owned(),
        MediaKind::Disk,
        &media_loaded.bytes,
    ));
    session
        .prepare(&media_set, &[])
        .map_err(|err| format!("prepare: {err}"))?;

    session
        .wait_for_boot(250)
        .map_err(|err| format!("wait_for_boot: {err}"))?;
    session
        .run_frames(50)
        .map_err(|err| format!("settle: {err}"))?;
    session.queue_input(InputEvent::Key {
        name: "enter".into(),
        pressed: true,
    });
    session
        .run_frames(5)
        .map_err(|err| format!("enter press: {err}"))?;
    session.queue_input(InputEvent::Key {
        name: "enter".into(),
        pressed: false,
    });
    session
        .run_frames(frame_budget)
        .map_err(|err| format!("loader run: {err}"))?;

    // Frame hash via the catalogue's convention: hash the rgba pixels
    // of the latest rendered frame.
    let frame = session
        .latest_frame()
        .ok_or_else(|| "no frame".to_owned())?;
    let rgba = frame.rgba_pixels().map_err(|err| format!("rgba: {err}"))?;
    let fb_hash = hash_xxh64(&rgba);

    // Audio hash via the catalogue's convention: capture the WAV
    // bytes of the audio buffer.
    let audio_wav = session
        .audio_wav_bytes()
        .map_err(|err| format!("audio wav: {err}"))?;
    let audio_hash = hash_xxh64(&audio_wav);

    let fb = session.machine().machine().framebuffer.clone();

    // PNG dump for visual inspection.
    let png_path = PathBuf::from("/tmp").join(format!("plus3_survey_{id}.png"));
    let mut palette_rgb = Vec::with_capacity(SPECTRUM_PALETTE.len() * 3);
    for entry in &SPECTRUM_PALETTE {
        let r = ((entry >> 24) & 0xFF) as u8;
        let g = ((entry >> 16) & 0xFF) as u8;
        let b = ((entry >> 8) & 0xFF) as u8;
        palette_rgb.extend_from_slice(&[r, g, b]);
    }
    let file = std::fs::File::create(&png_path).map_err(|err| format!("png create: {err}"))?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, 352, 296);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(palette_rgb);
    let mut writer = encoder
        .write_header()
        .map_err(|err| format!("png header: {err}"))?;
    writer
        .write_image_data(&fb)
        .map_err(|err| format!("png data: {err}"))?;

    Ok((fb_hash, audio_hash))
}
