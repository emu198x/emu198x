use std::io::Cursor;
use std::path::{Path, PathBuf};

use emu198x_shell::{FirmwareImage, FirmwareSet, HeadlessSession, InputEvent, read_firmware_asset};
use motorola_vdg_6847::{TEXT_VISIBLE_FRAMEBUFFER_HEIGHT, TEXT_VISIBLE_FRAMEBUFFER_WIDTH};
use runtime_dragon::{DragonRuntime, DragonSessionQueryProvider, Model};

const DRAGON_CPU_HZ: u64 = 894_886;
const DRAGON_FRAME_HZ: u64 = 50;
const DRAGON_FRAME_CYCLES: u64 = DRAGON_CPU_HZ / DRAGON_FRAME_HZ;
const BOOT_FRAME_BUDGET: u32 = 100;
const KEY_EDGE_FRAMES: u32 = 4;
const GOLDEN_NAME: &str = "dragon32-basic";

#[test]
fn dragon32_real_rom_reaches_basic_prompt_and_captures_frame() {
    let Some(session) = booted_dragon_session() else {
        return;
    };

    assert_eq!(
        session
            .query("dragon.text.base")
            .expect("text base query should work")
            .value,
        serde_json::json!(0x400)
    );

    let png = session
        .screenshot_png_bytes()
        .expect("booted Dragon runtime should have emitted a frame");
    assert_png_dimensions(
        &png,
        TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
        TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32,
    );

    compare_or_update_golden(GOLDEN_NAME, &png);
}

#[test]
fn dragon32_real_rom_echoes_basic_keyboard_input() {
    let Some(mut session) = booted_dragon_session() else {
        return;
    };

    tap_key(&mut session, "a");
    tap_key(&mut session, "b");
    tap_key(&mut session, "c");
    tap_key(&mut session, "1");
    tap_key(&mut session, "2");
    tap_key(&mut session, "3");

    if let Err(err) = session.wait_for_query_text_contains("screen.text.lines", "ABC123", 30) {
        panic!(
            "Dragon BASIC should echo typed keys: {err}\n{}",
            screen_text_lines(&session).join("\n")
        );
    }

    tap_key_combo(&mut session, &["shift", "2"]);
    if let Err(err) = session.wait_for_query_text_contains("screen.text.lines", "ABC123\"", 30) {
        panic!(
            "Dragon BASIC should echo shifted quote: {err}\n{}",
            screen_text_lines(&session).join("\n")
        );
    }
}

fn booted_dragon_session() -> Option<HeadlessSession<DragonRuntime, DragonSessionQueryProvider>> {
    let Some(rom_path) = dragon32_rom_path() else {
        eprintln!("skipping Dragon 32 real-ROM smoke: set EMU198X_DRAGON32_ROM");
        return None;
    };

    let loaded = read_firmware_asset(&rom_path)
        .unwrap_or_else(|err| panic!("read Dragon 32 ROM at {}: {err}", rom_path.display()));
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("dragon32-basic-rom", &loaded.bytes));
    let runtime = DragonRuntime::from_firmware(Model::Dragon32Pal, &firmware)
        .expect("real Dragon 32 ROM should create runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        DRAGON_FRAME_CYCLES,
        DragonSessionQueryProvider,
    );

    let boot = session
        .wait_for_boot(BOOT_FRAME_BUDGET)
        .expect("Dragon 32 ROM should reach BASIC prompt");

    assert_eq!(boot.reason, "basic-ok-prompt");
    assert!(boot.frames <= BOOT_FRAME_BUDGET);
    Some(session)
}

fn dragon32_rom_path() -> Option<PathBuf> {
    if let Some(path) = existing_env_path("EMU198X_DRAGON32_ROM") {
        return Some(path);
    }

    if let Some(path) = home_path(".emu198x/roms/dragon/dragon32.rom") {
        return Some(path);
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)?;
    let sibling_archive = repo_root
        .parent()?
        .join("Emu198x-docs-archive-2026-04-19/Reference/dragon/Dragon/Firmware/Dragon Data Dragon 32 BIOS (1982)(Dragon Data).zip");
    if sibling_archive.exists() {
        return Some(sibling_archive);
    }

    None
}

fn existing_env_path(var: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os(var)?);
    if path.exists() { Some(path) } else { None }
}

fn home_path(relative: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("HOME")?).join(relative);
    if path.exists() { Some(path) } else { None }
}

fn goldens_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

fn update_mode() -> bool {
    std::env::var_os("EMU198X_UPDATE_GOLDENS").is_some_and(|value| !value.is_empty())
}

fn compare_or_update_golden(name: &str, png: &[u8]) {
    let dir = goldens_dir();
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|err| panic!("create Dragon goldens dir {}: {err}", dir.display()));

    let golden_path = dir.join(format!("{name}.png"));
    if update_mode() {
        std::fs::write(&golden_path, png)
            .unwrap_or_else(|err| panic!("write golden {}: {err}", golden_path.display()));
        eprintln!("wrote Dragon golden at {}", golden_path.display());
        return;
    }

    if !golden_path.exists() {
        eprintln!(
            "skipping Dragon golden compare: missing {}",
            golden_path.display()
        );
        return;
    }

    let expected = std::fs::read(&golden_path)
        .unwrap_or_else(|err| panic!("read golden {}: {err}", golden_path.display()));
    if expected != png {
        let actual_path = dir.join(format!("{name}.actual.png"));
        std::fs::write(&actual_path, png)
            .unwrap_or_else(|err| panic!("write actual {}: {err}", actual_path.display()));
        panic!(
            "{name}: Dragon golden mismatch; wrote actual to {}",
            actual_path.display()
        );
    }
}

fn assert_png_dimensions(png: &[u8], expected_width: u32, expected_height: u32) {
    let decoder = png::Decoder::new(Cursor::new(png));
    let reader = decoder
        .read_info()
        .expect("headless screenshot should be a valid PNG");
    let info = reader.info();

    assert_eq!(info.width, expected_width);
    assert_eq!(info.height, expected_height);
}

fn screen_text_lines(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
) -> Vec<String> {
    let result = session
        .query("screen.text.lines")
        .expect("screen.text.lines query should work");
    result
        .value
        .as_array()
        .expect("screen.text.lines should be an array")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("screen.text.lines entries should be strings")
                .to_owned()
        })
        .collect()
}

fn tap_key(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    name: &'static str,
) {
    session.queue_input(InputEvent::Key {
        name: name.into(),
        pressed: true,
    });
    session
        .run_frames(KEY_EDGE_FRAMES)
        .expect("key press should advance Dragon runtime");
    session.queue_input(InputEvent::Key {
        name: name.into(),
        pressed: false,
    });
    session
        .run_frames(KEY_EDGE_FRAMES)
        .expect("key release should advance Dragon runtime");
}

fn tap_key_combo(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    names: &[&'static str],
) {
    for name in names {
        session.queue_input(InputEvent::Key {
            name: (*name).into(),
            pressed: true,
        });
    }
    session
        .run_frames(KEY_EDGE_FRAMES)
        .expect("key combo press should advance Dragon runtime");
    for name in names.iter().rev() {
        session.queue_input(InputEvent::Key {
            name: (*name).into(),
            pressed: false,
        });
    }
    session
        .run_frames(KEY_EDGE_FRAMES)
        .expect("key combo release should advance Dragon runtime");
}
