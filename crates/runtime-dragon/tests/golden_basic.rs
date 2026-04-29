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
        VDG_PAL_OVERSCAN_FRAMEBUFFER_WIDTH as u32,
        VDG_PAL_OVERSCAN_FRAMEBUFFER_HEIGHT as u32,
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
        query_u64(&session, "dragon.cpu.pc"),
        session
            .query("dragon.machine.halted")
            .expect("dragon.machine.halted query should work")
            .value,
        query_u64(&session, "dragon.pia0.control_a"),
        query_u64(&session, "dragon.pia0.control_b"),
        query_u64(&session, "dragon.pia0.ddr_a"),
        query_u64(&session, "dragon.pia0.ddr_b"),
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
        query_u64(&session, "dragon.pia1.control_a"),
        session
            .query("dragon.pia1.ca2")
            .expect("dragon.pia1.ca2 query should work")
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
        .wait_for_query_bool("dragon.tape.motor_on", false, 3_500)
        .expect("Dragon ROM should turn the cassette motor off after loading Textstar");
    let lines = screen_text_lines(&session);
    let prompt_count = ok_prompt_count(&lines);
    assert!(
        prompt_count >= 1 && !lines.iter().any(|line| line.contains("ERROR")),
        "Dragon BASIC should return to OK after loading Textstar; prompts={prompt_count} position={}/{} finished={} motor={}\n{}",
        query_u64(&session, "dragon.tape.position_bits"),
        query_u64(&session, "dragon.tape.length_bits"),
        session
            .query("dragon.tape.finished")
            .expect("dragon.tape.finished query should work")
            .value,
        session
            .query("dragon.tape.motor_on")
            .expect("dragon.tape.motor_on query should work")
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
        .wait_for_query_bool("dragon.tape.motor_on", false, 3_500)
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
            .query("dragon.tape.header.file_type")
            .expect("dragon.tape.header.file_type query should work")
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
        .wait_for_query_bool("dragon.tape.motor_on", false, 4_500)
        .expect("Dragon ROM should reach a cassette motor-off interval while loading CLOADM");
    assert!(
        wait_for_ok_prompt_without_error(&mut session, 4_500),
        "Dragon BASIC should return to OK after CLOADM; position={}/{} finished={} motor={}\n{}",
        query_u64(&session, "dragon.tape.position_bits"),
        query_u64(&session, "dragon.tape.length_bits"),
        session
            .query("dragon.tape.finished")
            .expect("dragon.tape.finished query should work")
            .value,
        session
            .query("dragon.tape.motor_on")
            .expect("dragon.tape.motor_on query should work")
            .value,
        screen_text_lines(&session).join("\n")
    );

    let lines = screen_text_lines(&session);
    let prompt_count = ok_prompt_count(&lines);
    assert!(
        prompt_count >= 1 && !lines.iter().any(|line| line.contains("ERROR")),
        "Dragon BASIC should return to OK after CLOADM; prompts={prompt_count} position={}/{} finished={} motor={}\n{}",
        query_u64(&session, "dragon.tape.position_bits"),
        query_u64(&session, "dragon.tape.length_bits"),
        session
            .query("dragon.tape.finished")
            .expect("dragon.tape.finished query should work")
            .value,
        session
            .query("dragon.tape.motor_on")
            .expect("dragon.tape.motor_on query should work")
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
        let position = query_u64(session, "dragon.tape.position_bits");
        if position > threshold {
            return position;
        }
        session
            .run_frames(1)
            .expect("Dragon runtime should advance while waiting for tape movement");
    }
    query_u64(session, "dragon.tape.position_bits")
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
