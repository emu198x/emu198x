use std::io::Cursor;
use std::path::{Path, PathBuf};

use emu198x_shell::{
    FirmwareImage, FirmwareSet, HeadlessSession, InputEvent, MachineCore, MediaImage, MediaKind,
    MediaSet, read_firmware_asset, read_media_asset,
};
use motorola_vdg_6847::{VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT, VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH};
use runtime_dragon::{DragonRuntime, DragonSessionQueryProvider, Model};

const DRAGON_CPU_HZ: u64 = 894_886;
const DRAGON_FRAME_HZ: u64 = 50;
const DRAGON_FRAME_CYCLES: u64 = DRAGON_CPU_HZ / DRAGON_FRAME_HZ;
const BOOT_FRAME_BUDGET: u32 = 100;
const KEY_EDGE_FRAMES: u32 = 8;
const GOLDEN_NAME: &str = "dragon32-basic";

#[test]
fn dragon32_real_rom_reaches_basic_prompt_and_captures_frame() {
    let Some(session) = booted_dragon_session() else {
        return;
    };

    assert_eq!(
        session
            .query("text.base")
            .expect("text base query should work")
            .value,
        serde_json::json!(0x400)
    );

    let png = session
        .screenshot_png_bytes()
        .expect("booted Dragon runtime should have emitted a frame");
    assert_png_dimensions(
        &png,
        VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH as u32,
        VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT as u32,
    );

    compare_or_update_golden(GOLDEN_NAME, &png);
}

#[test]
fn dragon64_real_rom_reaches_basic_prompt() {
    let Some(mut session) = booted_dragon64_session() else {
        return;
    };

    let boot = session
        .wait_for_boot(BOOT_FRAME_BUDGET)
        .unwrap_or_else(|err| {
            panic!(
                "Dragon 64 ROM should reach BASIC prompt: {err}; pc=${:04X} s=${:04X} text_base=${:04X} display_base=${:04X}\n{}",
                query_u64(&session, "cpu.pc"),
                query_u64(&session, "cpu.s"),
                query_u64(&session, "text.base"),
                query_u64(&session, "video.display_base"),
                screen_text_lines(&session).join("\n")
            )
        });

    assert_eq!(boot.reason, "basic-ok-prompt");
    assert!(boot.frames <= BOOT_FRAME_BUDGET);
    session
        .run_frames(30)
        .expect("Dragon 64 ROM should idle after reaching BASIC prompt");
}

#[test]
fn dragon64_exec_48000_enters_sixty_four_kib_mode() {
    let Some(mut session) = booted_dragon64_session() else {
        return;
    };

    let boot = session
        .wait_for_boot(BOOT_FRAME_BUDGET)
        .expect("Dragon 64 ROM should reach BASIC prompt before EXEC 48000");
    assert_eq!(boot.reason, "basic-ok-prompt");
    session
        .run_frames(30)
        .expect("Dragon 64 ROM should idle after reaching BASIC prompt");

    for name in [
        "e", "x", "e", "c", "space", "4", "8", "0", "0", "0", "enter",
    ] {
        tap_key(&mut session, name);
    }
    session
        .run_frames(200)
        .expect("Dragon 64 mode transition should advance");

    let model = session
        .query("hardware.model")
        .expect("hardware model query should work")
        .value;
    assert_eq!(
        model,
        serde_json::json!("dragon64-mode"),
        "Dragon 64 EXEC 48000 did not enter 64K mode; pc=${:04X} s=${:04X} pia1_cb=${:02X} pia1_ddrb=${:02X} pia1_ob=${:02X} pia1_pb=${:02X}\n{}",
        query_u64(&session, "cpu.pc"),
        query_u64(&session, "cpu.s"),
        query_u64(&session, "pia1.control_b"),
        query_u64(&session, "pia1.ddr_b"),
        query_u64(&session, "pia1.output_b"),
        query_u64(&session, "pia1.pins_b"),
        screen_text_lines(&session).join("\n")
    );

    for name in ["p", "r", "i", "n", "t", "space", "1", "enter"] {
        tap_key(&mut session, name);
    }
    if let Err(err) = session.wait_for_query_text_contains("screen.text.lines", "PRINT 1", 60) {
        panic!(
            "Dragon 64 64-mode BASIC should echo input after EXEC 48000: {err}; pc=${:04X} s=${:04X}\n{}",
            query_u64(&session, "cpu.pc"),
            query_u64(&session, "cpu.s"),
            screen_text_lines(&session).join("\n")
        );
    }
    session
        .run_frames(60)
        .expect("Dragon 64 64-mode BASIC should evaluate PRINT 1");
    let lines = screen_text_lines(&session);
    assert!(
        lines.iter().any(|line| line.trim() == "1"),
        "Dragon 64 64-mode BASIC should print a numeric result after EXEC 48000; pc=${:04X} s=${:04X}\n{}",
        query_u64(&session, "cpu.pc"),
        query_u64(&session, "cpu.s"),
        lines.join("\n")
    );
}

#[test]
fn dragon32_real_rom_echoes_basic_keyboard_input() {
    let Some(mut session) = booted_dragon_session() else {
        return;
    };

    tap_key(&mut session, "a");
    tap_key(&mut session, "b");
    tap_key(&mut session, "c");
    tap_key(&mut session, "p");
    tap_key(&mut session, "1");
    tap_key(&mut session, "2");
    tap_key(&mut session, "3");

    if let Err(err) = session.wait_for_query_text_contains("screen.text.lines", "ABCP123", 30) {
        panic!(
            "Dragon BASIC should echo typed keys, including column-0 P: {err}\n{}",
            screen_text_lines(&session).join("\n")
        );
    }

    tap_key_combo(&mut session, &["shift", "2"]);
    if let Err(err) = session.wait_for_query_text_contains("screen.text.lines", "ABCP123\"", 30) {
        panic!(
            "Dragon BASIC should echo shifted quote: {err}\n{}",
            screen_text_lines(&session).join("\n")
        );
    }
}

#[test]
fn dragon32_real_rom_accepts_enter_key() {
    let Some(mut session) = booted_dragon_session() else {
        return;
    };

    for name in ["p", "r", "i", "n", "t", "space", "1"] {
        tap_key(&mut session, name);
    }
    tap_key_for_frames(&mut session, "enter", 30);
    session
        .run_frames(300)
        .expect("Dragon runtime should advance after enter");

    let lines = screen_text_lines(&session);
    assert!(
        lines.iter().any(|line| line.trim() == "1"),
        "Dragon BASIC should accept Enter and execute PRINT 1; PC=${:04X} halted={}; PIA0 CRA=${:02X} CRB=${:02X} DDRA=${:02X} DDRB=${:02X}\n{}",
        query_u64(&session, "cpu.pc"),
        session
            .query("machine.halted")
            .expect("machine.halted query should work")
            .value,
        query_u64(&session, "pia0.control_a"),
        query_u64(&session, "pia0.control_b"),
        query_u64(&session, "pia0.ddr_a"),
        query_u64(&session, "pia0.ddr_b"),
        lines.join("\n")
    );
}

#[test]
fn dragon_runtime_mounts_real_textstar_cas_zip_when_available() {
    let Some(cas_path) = dragon_textstar_cas_path() else {
        eprintln!("skipping Dragon CAS smoke: local Textstar CAS archive not found");
        return;
    };

    let loaded = read_media_asset(&cas_path, MediaKind::Tape)
        .unwrap_or_else(|err| panic!("read Dragon CAS at {}: {err}", cas_path.display()));
    let mut runtime = DragonRuntime::blank(Model::Dragon32Pal);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));

    runtime
        .load_media(&media)
        .expect("real Dragon CAS should mount");

    let summary = runtime
        .tape_summary()
        .expect("mounted CAS should produce a summary");
    assert!(
        summary.blocks > 2,
        "Textstar should have multiple CAS blocks"
    );
    assert!(
        summary.checksums_valid,
        "Textstar CAS checksums should pass"
    );
    assert_eq!(summary.header_name.as_deref(), Some("TEXTSTAR"));
    assert_eq!(summary.header_file_type, Some("basic"));
}

#[test]
fn dragon_runtime_starts_real_textstar_cas_after_cload_when_available() {
    let Some(mut session) = booted_dragon_session() else {
        return;
    };
    let Some(cas_path) = dragon_textstar_cas_path() else {
        eprintln!("skipping Dragon CLOAD smoke: local Textstar CAS archive not found");
        return;
    };

    let loaded = read_media_asset(&cas_path, MediaKind::Tape)
        .unwrap_or_else(|err| panic!("read Dragon CAS at {}: {err}", cas_path.display()));
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));
    session
        .load_media(&media)
        .expect("real Dragon CAS should mount into the booted runtime");

    for name in ["c", "l", "o", "a", "d", "enter"] {
        tap_key(&mut session, name);
    }

    let position = wait_for_tape_position_above(&mut session, 0, 180);
    assert!(
        position > 0,
        "Dragon ROM did not consume tape bits after CLOAD; PIA1 control A=${:02X} CA2={}\n{}",
        query_u64(&session, "pia1.control_a"),
        session
            .query("pia1.ca2")
            .expect("pia1.ca2 query should work")
            .value,
        screen_text_lines(&session).join("\n")
    );
}

#[test]
fn dragon_runtime_loads_real_textstar_cas_to_basic_prompt_when_available() {
    let Some(mut session) = booted_dragon_session() else {
        return;
    };
    let Some(cas_path) = dragon_textstar_cas_path() else {
        eprintln!("skipping Dragon CLOAD completion smoke: local Textstar CAS archive not found");
        return;
    };

    let loaded = read_media_asset(&cas_path, MediaKind::Tape)
        .unwrap_or_else(|err| panic!("read Dragon CAS at {}: {err}", cas_path.display()));
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));
    session
        .load_media(&media)
        .expect("real Dragon CAS should mount into the booted runtime");

    for name in ["c", "l", "o", "a", "d", "enter"] {
        tap_key(&mut session, name);
    }

    let moved_to = wait_for_tape_position_above(&mut session, 0, 180);
    assert!(
        moved_to > 0,
        "Dragon ROM should start consuming Textstar tape bits"
    );
    session
        .wait_for_query_bool("tape.motor_on", false, 3_500)
        .expect("Dragon ROM should turn the cassette motor off after loading Textstar");
    let returned_to_prompt = wait_for_ok_prompt_without_error(&mut session, 180);
    let lines = screen_text_lines(&session);
    let prompt_count = ok_prompt_count(&lines);
    assert!(
        returned_to_prompt,
        "Dragon BASIC should return to OK after loading Textstar; prompts={prompt_count} position={}/{} finished={} motor={}\n{}",
        query_u64(&session, "tape.position_bits"),
        query_u64(&session, "tape.length_bits"),
        session
            .query("tape.finished")
            .expect("tape.finished query should work")
            .value,
        session
            .query("tape.motor_on")
            .expect("tape.motor_on query should work")
            .value,
        lines.join("\n")
    );
}

#[test]
fn dragon_runtime_runs_real_textstar_after_cload_when_available() {
    let Some(mut session) = booted_dragon_session() else {
        return;
    };
    let Some(cas_path) = dragon_textstar_cas_path() else {
        eprintln!("skipping Dragon RUN smoke: local Textstar CAS archive not found");
        return;
    };

    let loaded = read_media_asset(&cas_path, MediaKind::Tape)
        .unwrap_or_else(|err| panic!("read Dragon CAS at {}: {err}", cas_path.display()));
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));
    session
        .load_media(&media)
        .expect("real Dragon CAS should mount into the booted runtime");

    for name in ["c", "l", "o", "a", "d", "enter"] {
        tap_key(&mut session, name);
    }
    let moved_to = wait_for_tape_position_above(&mut session, 0, 180);
    assert!(moved_to > 0);
    session
        .wait_for_query_bool("tape.motor_on", false, 3_500)
        .expect("Dragon ROM should turn the cassette motor off after loading Textstar");

    for name in ["r", "u", "n"] {
        tap_key(&mut session, name);
    }
    let before_enter = screen_text_lines(&session);
    tap_key(&mut session, "enter");
    let changed = wait_for_screen_text_change(&mut session, &before_enter, 300);
    let lines = screen_text_lines(&session);

    assert!(
        changed && !lines.iter().any(|line| line.contains("ERROR")),
        "Dragon BASIC should run Textstar after CLOAD/RUN\n{}",
        lines.join("\n")
    );
}

#[test]
fn dragon_runtime_loads_and_executes_real_machine_code_cas_when_available() {
    let Some(mut session) = booted_dragon_session() else {
        return;
    };
    let Some(cas_path) = dragon_machine_code_cas_path() else {
        eprintln!("skipping Dragon CLOADM smoke: local machine-code CAS archive not found");
        return;
    };

    let loaded = read_media_asset(&cas_path, MediaKind::Tape)
        .unwrap_or_else(|err| panic!("read Dragon CAS at {}: {err}", cas_path.display()));
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));
    session
        .load_media(&media)
        .expect("real Dragon machine-code CAS should mount into the booted runtime");

    assert_eq!(
        session
            .query("tape.header.file_type")
            .expect("tape.header.file_type query should work")
            .value,
        serde_json::json!("machine-code")
    );

    for name in ["c", "l", "o", "a", "d", "m", "enter"] {
        tap_key(&mut session, name);
    }
    let moved_to = wait_for_tape_position_above(&mut session, 0, 180);
    assert!(
        moved_to > 0,
        "Dragon ROM should start consuming machine-code tape bits after CLOADM"
    );
    session
        .wait_for_query_bool("tape.motor_on", false, 4_500)
        .expect("Dragon ROM should reach a cassette motor-off interval while loading CLOADM");
    assert!(
        wait_for_ok_prompt_without_error(&mut session, 4_500),
        "Dragon BASIC should return to OK after CLOADM; position={}/{} finished={} motor={}\n{}",
        query_u64(&session, "tape.position_bits"),
        query_u64(&session, "tape.length_bits"),
        session
            .query("tape.finished")
            .expect("tape.finished query should work")
            .value,
        session
            .query("tape.motor_on")
            .expect("tape.motor_on query should work")
            .value,
        screen_text_lines(&session).join("\n")
    );

    let lines = screen_text_lines(&session);
    let prompt_count = ok_prompt_count(&lines);
    assert!(
        prompt_count >= 1 && !lines.iter().any(|line| line.contains("ERROR")),
        "Dragon BASIC should return to OK after CLOADM; prompts={prompt_count} position={}/{} finished={} motor={}\n{}",
        query_u64(&session, "tape.position_bits"),
        query_u64(&session, "tape.length_bits"),
        session
            .query("tape.finished")
            .expect("tape.finished query should work")
            .value,
        session
            .query("tape.motor_on")
            .expect("tape.motor_on query should work")
            .value,
        lines.join("\n")
    );

    let before_exec = session
        .screenshot_png_bytes()
        .expect("Dragon runtime should capture a frame before EXEC");
    for name in ["e", "x", "e", "c", "enter"] {
        tap_key(&mut session, name);
    }
    let changed = wait_for_screenshot_change(&mut session, &before_exec, 500);
    let lines = screen_text_lines(&session);
    assert!(
        changed && !lines.iter().any(|line| line.contains("ERROR")),
        "Dragon machine-code program should visibly start after EXEC\n{}",
        lines.join("\n")
    );
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
    session
        .run_frames(30)
        .expect("Dragon 32 ROM should idle after reaching BASIC prompt");
    Some(session)
}

fn booted_dragon64_session() -> Option<HeadlessSession<DragonRuntime, DragonSessionQueryProvider>> {
    let Some(compat_rom_path) = dragon64_compatible_rom_path() else {
        eprintln!("skipping Dragon 64 real-ROM smoke: set EMU198X_DRAGON64_COMPAT_ROM");
        return None;
    };
    let Some(mode_rom_path) = dragon64_rom_path() else {
        eprintln!("skipping Dragon 64 real-ROM smoke: set EMU198X_DRAGON64_ROM");
        return None;
    };

    let loaded = read_firmware_asset(&compat_rom_path).unwrap_or_else(|err| {
        panic!(
            "read Dragon 64 compatible-mode ROM at {}: {err}",
            compat_rom_path.display()
        )
    });
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("dragon64-compatible-rom", &loaded.bytes));
    let mode_rom = read_firmware_asset(&mode_rom_path).unwrap_or_else(|err| {
        panic!(
            "read Dragon 64 mode ROM at {}: {err}",
            mode_rom_path.display()
        )
    });
    firmware.push(FirmwareImage::new("dragon64-basic-rom", &mode_rom.bytes));
    let runtime = DragonRuntime::from_firmware(Model::Dragon64Pal, &firmware)
        .expect("real Dragon 64 ROM should create runtime");
    Some(HeadlessSession::new_with_query_provider(
        runtime,
        DRAGON_FRAME_CYCLES,
        DragonSessionQueryProvider,
    ))
}

fn dragon_textstar_cas_path() -> Option<PathBuf> {
    if let Some(path) = existing_env_path("EMU198X_DRAGON_CAS") {
        return Some(path);
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)?;
    let sibling_archive = repo_root
        .parent()?
        .join("Emu198x-docs-archive-2026-04-19/Reference/dragon/Dragon/Applications/[CAS]/Textstar (1982)(Personal Software Services).zip");
    if sibling_archive.exists() {
        return Some(sibling_archive);
    }

    None
}

fn dragon_machine_code_cas_path() -> Option<PathBuf> {
    if let Some(path) = existing_env_path("EMU198X_DRAGON_MACHINE_CAS") {
        return Some(path);
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)?;
    let sibling_archive = repo_root
        .parent()?
        .join("Emu198x-docs-archive-2026-04-19/Reference/dragon/Dragon/Games/[CAS]/Color Invaders (1982)(Microdeal).zip");
    if sibling_archive.exists() {
        return Some(sibling_archive);
    }

    None
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

fn dragon64_compatible_rom_path() -> Option<PathBuf> {
    if let Some(path) = existing_env_path("EMU198X_DRAGON64_COMPAT_ROM") {
        return Some(path);
    }

    if let Some(path) = home_path(".emu198x/roms/dragon/dragon64-compat.rom") {
        return Some(path);
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)?;
    // This image reaches the Dragon 64 compatible-mode BASIC prompt and is one
    // of XRoar's accepted Dragon 64 32K-mode ROM CRCs.
    let sibling_archive = repo_root
        .parent()?
        .join("Emu198x-docs-archive-2026-04-19/Reference/dragon/Dragon/Firmware/Dragon Data Dragon 64 BIOS (1983)(Dragon Data)[24Kb RAM].zip");
    if sibling_archive.exists() {
        return Some(sibling_archive);
    }

    None
}

fn dragon64_rom_path() -> Option<PathBuf> {
    if let Some(path) = existing_env_path("EMU198X_DRAGON64_ROM") {
        return Some(path);
    }

    if let Some(path) = home_path(".emu198x/roms/dragon/dragon64.rom") {
        return Some(path);
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)?;
    let sibling_archive = repo_root
        .parent()?
        .join("Emu198x-docs-archive-2026-04-19/Reference/dragon/Dragon/Firmware/Dragon Data Dragon 64 BIOS (1983)(Dragon Data)[48Kb RAM].zip");
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

    // Compare decoded images, not encoded bytes. A byte comparison also
    // gates on the PNG encoder: `png` 0.17 -> 0.18 changed the default
    // deflate settings and re-encoded this identical frame from 14,935
    // bytes to 2,667, which reads as a Dragon regression and is not one.
    let expected_image = decode_golden(&expected, &golden_path.display().to_string());
    let actual_image = decode_golden(png, "captured frame");
    if expected_image != actual_image {
        let actual_path = dir.join(format!("{name}.actual.png"));
        std::fs::write(&actual_path, png)
            .unwrap_or_else(|err| panic!("write actual {}: {err}", actual_path.display()));
        let (ew, eh, _) = &expected_image;
        let (aw, ah, _) = &actual_image;
        panic!(
            "{name}: Dragon golden mismatch ({ew}x{eh} golden vs {aw}x{ah} actual); \
             wrote actual to {}",
            actual_path.display()
        );
    }
}

/// Decode a PNG to `(width, height, raw pixel bytes)` so goldens compare on
/// image content alone.
fn decode_golden(png: &[u8], what: &str) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(Cursor::new(png));
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|err| panic!("{what} should be a valid PNG: {err}"));
    let mut buf = vec![0; reader.output_buffer_size().expect("bounded frame size")];
    let info = reader
        .next_frame(&mut buf)
        .unwrap_or_else(|err| panic!("decode {what}: {err}"));
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
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

fn tap_key(session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>, name: &str) {
    tap_key_for_frames(session, name, KEY_EDGE_FRAMES);
}

fn tap_key_for_frames(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    name: &str,
    frames: u32,
) {
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed: true,
    });
    session
        .run_frames(frames)
        .expect("key press should advance Dragon runtime");
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
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

fn wait_for_tape_position_above(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    threshold: u64,
    max_frames: u32,
) -> u64 {
    for _ in 0..=max_frames {
        let position = query_u64(session, "tape.position_bits");
        if position > threshold {
            return position;
        }
        session
            .run_frames(1)
            .expect("Dragon runtime should advance while waiting for tape movement");
    }
    query_u64(session, "tape.position_bits")
}

fn ok_prompt_count(lines: &[String]) -> usize {
    lines.iter().filter(|line| line.trim_end() == "OK").count()
}

fn wait_for_ok_prompt_without_error(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    max_frames: u32,
) -> bool {
    for _ in 0..=max_frames {
        let lines = screen_text_lines(session);
        if ok_prompt_count(&lines) >= 1 && !lines.iter().any(|line| line.contains("ERROR")) {
            return true;
        }
        session
            .run_frames(1)
            .expect("Dragon runtime should advance while waiting for OK prompt");
    }
    false
}

fn wait_for_screen_text_change(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    before: &[String],
    max_frames: u32,
) -> bool {
    for _ in 0..=max_frames {
        if screen_text_lines(session) != before {
            return true;
        }
        session
            .run_frames(1)
            .expect("Dragon runtime should advance while waiting for screen change");
    }
    false
}

fn wait_for_screenshot_change(
    session: &mut HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    before: &[u8],
    max_frames: u32,
) -> bool {
    for _ in 0..=max_frames {
        if session
            .screenshot_png_bytes()
            .expect("Dragon runtime should capture frames while waiting for screenshot change")
            != before
        {
            return true;
        }
        session
            .run_frames(1)
            .expect("Dragon runtime should advance while waiting for screenshot change");
    }
    false
}

fn query_u64(
    session: &HeadlessSession<DragonRuntime, DragonSessionQueryProvider>,
    path: &str,
) -> u64 {
    session
        .query(path)
        .unwrap_or_else(|err| panic!("{path} query should work: {err}"))
        .value
        .as_u64()
        .unwrap_or_else(|| panic!("{path} query should be an unsigned integer"))
}

/// Encode an RGBA image at a chosen deflate level, so the tests below can
/// produce two different byte streams for the same picture.
fn encode_rgba(width: u32, height: u32, pixels: &[u8], level: png::Compression) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(level);
    let mut writer = encoder.write_header().expect("write PNG header");
    writer.write_image_data(pixels).expect("write PNG pixels");
    writer.finish().expect("finish PNG");
    out
}

fn solid_rgba(width: u32, height: u32) -> Vec<u8> {
    (0..(width * height))
        .flat_map(|i| {
            let v = (i % 251) as u8;
            [v, v.wrapping_add(7), v.wrapping_add(29), 0xFF]
        })
        .collect()
}

/// The bug this comparison replaced: `png` 0.17 -> 0.18 re-encoded an
/// unchanged Dragon frame from 14,935 bytes to 2,667 and the byte-comparing
/// golden read that as a regression.
#[test]
fn golden_compare_ignores_the_png_encoder() {
    let pixels = solid_rgba(32, 16);
    let fast = encode_rgba(32, 16, &pixels, png::Compression::Fast);
    let best = encode_rgba(32, 16, &pixels, png::Compression::NoCompression);

    assert_ne!(fast, best, "the two encodings must differ as bytes");
    assert_eq!(decode_golden(&fast, "fast"), decode_golden(&best, "best"));
}

/// And the comparison must still be able to fail, or it gates nothing.
#[test]
fn golden_compare_catches_a_single_changed_pixel() {
    let pixels = solid_rgba(32, 16);
    let mut mutated = pixels.clone();
    mutated[4 * (8 * 32 + 17)] ^= 0xFF;

    let original = encode_rgba(32, 16, &pixels, png::Compression::Fast);
    let changed = encode_rgba(32, 16, &mutated, png::Compression::Fast);

    assert_ne!(
        decode_golden(&original, "original"),
        decode_golden(&changed, "changed"),
    );
}

/// A frame of the wrong size must fail too, rather than compare as a prefix.
#[test]
fn golden_compare_catches_a_dimension_change() {
    let wide = encode_rgba(32, 16, &solid_rgba(32, 16), png::Compression::Fast);
    let tall = encode_rgba(16, 32, &solid_rgba(16, 32), png::Compression::Fast);

    assert_ne!(decode_golden(&wide, "wide"), decode_golden(&tall, "tall"));
}
