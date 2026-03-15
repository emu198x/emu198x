//! Minimal runner for the Amiga machine core.
//!
//! Scope: video output and basic Paula audio playback/capture. Loads a
//! Kickstart ROM and optionally inserts an ADF into DF0:, then either runs a
//! windowed frontend or captures a framebuffer screenshot/audio in headless
//! mode.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::struct_excessive_bools,
    clippy::too_many_lines
)]

mod kickstart_db;

use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use commodore_denise_ocs::ViewportPreset;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use emu_core::renderer::Renderer;
use machine_amiga::format_adf::Adf;
use machine_amiga::mcp::{AmigaMcp, McpServer as AmigaMcpServer};
use machine_amiga::{
    Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion, commodore_denise_ocs,
};
use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

/// Standard PAL viewport dimensions (hires resolution, deinterlaced).
const VIEWPORT_WIDTH: u32 = 640; // (0xE0 - 0x40) * 4 = 160 CCKs * 4
const VIEWPORT_HEIGHT: u32 = 256; // (0x12C - 0x2C) = 256 lines
const STATUS_BAR_HEIGHT: u32 = 12;
const SCALE: u32 = 2;
const FRAME_DURATION: Duration = Duration::from_millis(20); // PAL ~50 Hz
const AUDIO_CHANNELS: usize = 2;
const AUDIO_QUEUE_SECONDS: usize = 2;

// Amiga raw keycodes (US keyboard positional defaults)
const AK_SPACE: u8 = 0x40;
const AK_TAB: u8 = 0x42;
const AK_RETURN: u8 = 0x44;
const AK_ESCAPE: u8 = 0x45;
const AK_BACKSPACE: u8 = 0x41;
const AK_DELETE: u8 = 0x46;
const AK_CURSOR_UP: u8 = 0x4C;
const AK_CURSOR_DOWN: u8 = 0x4D;
const AK_CURSOR_RIGHT: u8 = 0x4E;
const AK_CURSOR_LEFT: u8 = 0x4F;
const AK_LSHIFT: u8 = 0x60;
const AK_RSHIFT: u8 = 0x61;
const AK_CAPSLOCK: u8 = 0x62;
const AK_CTRL: u8 = 0x63;
const AK_LALT: u8 = 0x64;
const AK_RALT: u8 = 0x65;
const AK_LAMIGA: u8 = 0x66;
const AK_RAMIGA: u8 = 0x67;

#[derive(Debug, Clone, Copy)]
struct ActiveKeyMapping {
    raw_keycode: u8,
    synthetic_left_shift: bool,
}

#[derive(Debug)]
struct CliArgs {
    rom_path: PathBuf,
    adf_path: Option<PathBuf>,
    disk_path: Option<PathBuf>,
    model: AmigaModel,
    headless: bool,
    frames: u32,
    screenshot_path: Option<PathBuf>,
    audio_path: Option<PathBuf>,
    mute: bool,
    mcp: bool,
    script_path: Option<PathBuf>,
    drive_sounds: bool,
}

fn print_usage() {
    eprintln!("Usage: emu-amiga [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --rom <file>   Kickstart ROM file (or use AMIGA_KS13_ROM env var)");
    eprintln!("  --adf <file>   Optional ADF disk image to insert into DF0:");
    eprintln!("  --disk <file>  Optional ADF or IPF disk image to insert into DF0:");
    eprintln!(
        "  --model <a1000|a500|a500plus|a600|a1200|a2000|a3000|a4000>  Select machine model [default: a500; chipset derives from model]"
    );
    eprintln!("  --headless     Run without a window");
    eprintln!("  --frames <n>   Frames to run in headless mode [default: 300]");
    eprintln!("  --screenshot <file.png>  Save a framebuffer screenshot (headless)");
    eprintln!("  --audio <file.wav>  Save a WAV audio dump (headless)");
    eprintln!("  --mute         Disable host audio playback (windowed)");
    eprintln!("  --no-drive-sounds  Disable mechanical floppy drive sounds");
    eprintln!("  --mcp          Run as MCP JSON-RPC server (headless, stdin/stdout)");
    eprintln!("  --script <file.json>  Run a JSON script file (headless batch mode)");
    eprintln!("  -h, --help     Show this help");
}

fn print_usage_and_exit(code: i32) -> ! {
    print_usage();
    process::exit(code);
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let env_rom_path = std::env::var_os("AMIGA_KS13_ROM").map(PathBuf::from);

    match parse_args_from(&args, env_rom_path) {
        Ok(Some(cli)) => cli,
        Ok(None) => print_usage_and_exit(0),
        Err(e) => {
            eprintln!("{e}");
            print_usage_and_exit(1);
        }
    }
}

fn parse_args_from(
    args: &[String],
    env_rom_path: Option<PathBuf>,
) -> Result<Option<CliArgs>, String> {
    let mut rom_path: Option<PathBuf> = None;
    let mut adf_path: Option<PathBuf> = None;
    let mut disk_path: Option<PathBuf> = None;
    let mut model = AmigaModel::A500;
    let mut headless = false;
    let mut frames = 300;
    let mut screenshot_path: Option<PathBuf> = None;
    let mut audio_path: Option<PathBuf> = None;
    let mut mute = false;
    let mut drive_sounds = true;
    let mut mcp = false;
    let mut script_path: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--rom" => {
                i += 1;
                rom_path = args.get(i).map(PathBuf::from);
            }
            "--adf" => {
                i += 1;
                adf_path = args.get(i).map(PathBuf::from);
            }
            "--disk" => {
                i += 1;
                disk_path = args.get(i).map(PathBuf::from);
            }
            "--model" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    return Err(String::from(
                        "Missing value for --model (expected a500 or a500plus)",
                    ));
                };
                model = parse_model_arg(value)?;
            }
            "--headless" => {
                headless = true;
            }
            "--frames" => {
                i += 1;
                if let Some(value) = args.get(i) {
                    frames = value.parse().unwrap_or(300);
                }
            }
            "--screenshot" => {
                i += 1;
                screenshot_path = args.get(i).map(PathBuf::from);
            }
            "--audio" => {
                i += 1;
                audio_path = args.get(i).map(PathBuf::from);
            }
            "--mute" => {
                mute = true;
            }
            "--no-drive-sounds" => {
                drive_sounds = false;
            }
            "--mcp" => {
                mcp = true;
            }
            "--script" => {
                i += 1;
                script_path = args.get(i).map(PathBuf::from);
            }
            "-h" | "--help" => return Ok(None),
            other => {
                return Err(format!("Unknown argument: {other}"));
            }
        }
        i += 1;
    }

    let rom_path = if mcp || script_path.is_some() {
        // MCP/script mode: ROM is provided via boot method, not required upfront
        rom_path.or(env_rom_path).unwrap_or_default()
    } else {
        rom_path
            .or(env_rom_path)
            .ok_or_else(|| String::from("No Kickstart ROM specified."))?
    };

    if screenshot_path.is_some() || audio_path.is_some() {
        headless = true;
    }
    Ok(Some(CliArgs {
        rom_path,
        adf_path,
        disk_path,
        model,
        headless,
        frames,
        screenshot_path,
        audio_path,
        mute,
        mcp,
        script_path,
        drive_sounds,
    }))
}

fn parse_model_arg(value: &str) -> Result<AmigaModel, String> {
    match value.to_ascii_lowercase().as_str() {
        "a1000" => Ok(AmigaModel::A1000),
        "a500" => Ok(AmigaModel::A500),
        "a500+" | "a500plus" => Ok(AmigaModel::A500Plus),
        "a600" => Ok(AmigaModel::A600),
        "a1200" => Ok(AmigaModel::A1200),
        "a2000" => Ok(AmigaModel::A2000),
        "a3000" => Ok(AmigaModel::A3000),
        "a4000" => Ok(AmigaModel::A4000),
        other => Err(format!(
            "Invalid --model value '{other}' (expected 'a1000', 'a500', 'a500plus', 'a600', 'a1200', 'a2000', 'a3000', or 'a4000')"
        )),
    }
}

const fn chipset_for_model(model: AmigaModel) -> AmigaChipset {
    match model {
        AmigaModel::A1000 | AmigaModel::A500 | AmigaModel::A2000 => AmigaChipset::Ocs,
        AmigaModel::A500Plus | AmigaModel::A600 | AmigaModel::A3000 => AmigaChipset::Ecs,
        AmigaModel::A1200 | AmigaModel::A4000 => AmigaChipset::Aga,
    }
}

fn model_name(model: AmigaModel) -> &'static str {
    match model {
        AmigaModel::A1000 => "A1000",
        AmigaModel::A500 => "A500",
        AmigaModel::A500Plus => "A500+",
        AmigaModel::A600 => "A600",
        AmigaModel::A1200 => "A1200",
        AmigaModel::A2000 => "A2000",
        AmigaModel::A3000 => "A3000",
        AmigaModel::A4000 => "A4000",
    }
}

fn chipset_name(chipset: AmigaChipset) -> &'static str {
    match chipset {
        AmigaChipset::Ocs => "OCS",
        AmigaChipset::Ecs => "ECS",
        AmigaChipset::Aga => "AGA",
    }
}

/// Find the roms/ directory relative to the executable or current directory.
fn find_roms_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(std::path::Path::to_path_buf);
        for _ in 0..5 {
            if let Some(ref d) = dir {
                let roms = d.join("roms");
                if roms.is_dir() {
                    return roms;
                }
                dir = d.parent().map(std::path::Path::to_path_buf);
            }
        }
    }
    PathBuf::from("roms")
}

fn make_amiga(cli: &CliArgs) -> Amiga {
    let chipset = chipset_for_model(cli.model);
    let kickstart = match std::fs::read(&cli.rom_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!(
                "Failed to read Kickstart ROM {}: {e}",
                cli.rom_path.display()
            );
            process::exit(1);
        }
    };

    let mut amiga = Amiga::new_with_config(AmigaConfig {
        model: cli.model,
        chipset,
        region: AmigaRegion::Pal,
        kickstart,
        slow_ram_size: 0,
        ide_disk: None,
        scsi_disk: None,
        pcmcia_card: None,
    });

    // --disk auto-detects format; --adf forces ADF.
    let disk_to_load = cli.disk_path.as_ref().or(cli.adf_path.as_ref());
    if let Some(disk_path) = disk_to_load {
        let disk_bytes = match std::fs::read(disk_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read disk {}: {e}", disk_path.display());
                process::exit(1);
            }
        };

        if cli.adf_path.is_some() || !format_ipf::IpfImage::is_ipf(&disk_bytes) {
            // ADF path.
            let adf = match Adf::from_bytes(disk_bytes) {
                Ok(adf) => adf,
                Err(e) => {
                    eprintln!("Invalid ADF {}: {e}", disk_path.display());
                    process::exit(1);
                }
            };
            amiga.insert_disk(adf);
            eprintln!("Inserted ADF: {}", disk_path.display());
        } else {
            // IPF path.
            let ipf = match format_ipf::IpfImage::from_bytes(&disk_bytes) {
                Ok(ipf) => ipf,
                Err(e) => {
                    eprintln!("Invalid IPF {}: {e}", disk_path.display());
                    process::exit(1);
                }
            };
            amiga.insert_disk_image(Box::new(ipf));
            eprintln!("Inserted IPF: {}", disk_path.display());
        }
    }

    if !cli.drive_sounds {
        amiga.drive_sounds.enabled = false;
    }

    eprintln!(
        "Loaded Kickstart ROM: {} (model {}, chipset {})",
        cli.rom_path.display(),
        model_name(cli.model),
        chipset_name(chipset)
    );
    amiga
}

fn save_screenshot(amiga: &Amiga, path: &PathBuf) -> Result<(), String> {
    let file = File::create(path)
        .map_err(|e| format!("failed to create screenshot {}: {e}", path.display()))?;
    let writer = BufWriter::new(file);

    let pal = amiga.region == AmigaRegion::Pal;
    let viewport = amiga
        .denise
        .extract_viewport(ViewportPreset::Standard, pal, true);

    let mut encoder = png::Encoder::new(writer, viewport.width, viewport.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut png_writer = encoder
        .write_header()
        .map_err(|e| format!("failed to write PNG header {}: {e}", path.display()))?;

    let mut bytes = vec![0u8; (viewport.width * viewport.height * 4) as usize];
    for (i, &argb) in viewport.pixels.iter().enumerate() {
        let o = i * 4;
        bytes[o] = ((argb >> 16) & 0xFF) as u8;
        bytes[o + 1] = ((argb >> 8) & 0xFF) as u8;
        bytes[o + 2] = (argb & 0xFF) as u8;
        bytes[o + 3] = ((argb >> 24) & 0xFF) as u8;
    }

    png_writer
        .write_image_data(&bytes)
        .map_err(|e| format!("failed to write PNG data {}: {e}", path.display()))
}

fn save_audio_wav(samples: &[f32], path: &PathBuf) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: AUDIO_CHANNELS as u16,
        sample_rate: machine_amiga::AUDIO_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("failed to create WAV {}: {e}", path.display()))?;

    for &sample in samples {
        let scaled = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        writer
            .write_sample(scaled)
            .map_err(|e| format!("failed to write WAV sample {}: {e}", path.display()))?;
    }

    writer
        .finalize()
        .map_err(|e| format!("failed to finalize WAV {}: {e}", path.display()))
}

struct AudioOutput {
    _stream: cpal::Stream,
    queue: Arc<Mutex<VecDeque<f32>>>,
    max_samples: usize,
}

impl AudioOutput {
    fn new() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| String::from("no default audio output device"))?;

        let supported_configs = device
            .supported_output_configs()
            .map_err(|e| format!("failed to query output configs: {e}"))?;

        let desired = supported_configs
            .filter(|cfg| cfg.channels() == AUDIO_CHANNELS as u16)
            .find(|cfg| {
                let min = cfg.min_sample_rate().0;
                let max = cfg.max_sample_rate().0;
                min <= machine_amiga::AUDIO_SAMPLE_RATE && machine_amiga::AUDIO_SAMPLE_RATE <= max
            })
            .ok_or_else(|| {
                format!(
                    "no {}-channel output config supports {} Hz",
                    AUDIO_CHANNELS,
                    machine_amiga::AUDIO_SAMPLE_RATE
                )
            })?;

        let sample_format = desired.sample_format();
        let config = desired
            .with_sample_rate(cpal::SampleRate(machine_amiga::AUDIO_SAMPLE_RATE))
            .config();

        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let max_samples =
            (machine_amiga::AUDIO_SAMPLE_RATE as usize) * AUDIO_CHANNELS * AUDIO_QUEUE_SECONDS;
        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let callback_queue = Arc::clone(&queue);
                device
                    .build_output_stream(
                        &config,
                        move |data: &mut [f32], _| write_audio_data_f32(data, &callback_queue),
                        |err| eprintln!("Audio stream error: {err}"),
                        None,
                    )
                    .map_err(|e| format!("failed to build f32 audio stream: {e}"))?
            }
            cpal::SampleFormat::I16 => {
                let callback_queue = Arc::clone(&queue);
                device
                    .build_output_stream(
                        &config,
                        move |data: &mut [i16], _| write_audio_data_i16(data, &callback_queue),
                        |err| eprintln!("Audio stream error: {err}"),
                        None,
                    )
                    .map_err(|e| format!("failed to build i16 audio stream: {e}"))?
            }
            cpal::SampleFormat::U16 => {
                let callback_queue = Arc::clone(&queue);
                device
                    .build_output_stream(
                        &config,
                        move |data: &mut [u16], _| write_audio_data_u16(data, &callback_queue),
                        |err| eprintln!("Audio stream error: {err}"),
                        None,
                    )
                    .map_err(|e| format!("failed to build u16 audio stream: {e}"))?
            }
            other => {
                return Err(format!("unsupported audio sample format: {other:?}"));
            }
        };

        stream
            .play()
            .map_err(|e| format!("failed to start audio stream: {e}"))?;

        Ok(Self {
            _stream: stream,
            queue,
            max_samples,
        })
    }

    fn push_samples(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }

        let Ok(mut queue) = self.queue.lock() else {
            return;
        };

        for &sample in samples {
            queue.push_back(sample);
        }

        while queue.len() > self.max_samples {
            let _ = queue.pop_front();
        }
    }
}

fn write_audio_data_f32(data: &mut [f32], queue: &Arc<Mutex<VecDeque<f32>>>) {
    let Ok(mut guard) = queue.lock() else {
        data.fill(0.0);
        return;
    };

    for sample in data {
        *sample = guard.pop_front().unwrap_or(0.0);
    }
}

fn write_audio_data_i16(data: &mut [i16], queue: &Arc<Mutex<VecDeque<f32>>>) {
    let Ok(mut guard) = queue.lock() else {
        data.fill(0);
        return;
    };

    for sample in data {
        let value = guard.pop_front().unwrap_or(0.0).clamp(-1.0, 1.0);
        *sample = (value * f32::from(i16::MAX)) as i16;
    }
}

fn write_audio_data_u16(data: &mut [u16], queue: &Arc<Mutex<VecDeque<f32>>>) {
    let Ok(mut guard) = queue.lock() else {
        data.fill(u16::MAX / 2);
        return;
    };

    for sample in data {
        let value = guard.pop_front().unwrap_or(0.0).clamp(-1.0, 1.0);
        let scaled = ((value * 0.5) + 0.5) * f32::from(u16::MAX);
        *sample = scaled as u16;
    }
}

fn run_headless(cli: &CliArgs) {
    let mut amiga = make_amiga(cli);
    let mut all_audio = if cli.audio_path.is_some() {
        Some(Vec::new())
    } else {
        None
    };

    for _ in 0..cli.frames {
        amiga.run_frame();
        let audio = amiga.take_audio_buffer();
        if let Some(buffer) = all_audio.as_mut() {
            buffer.extend_from_slice(&audio);
        }
    }

    if let Some(path) = &cli.screenshot_path {
        if let Err(e) = save_screenshot(&amiga, path) {
            eprintln!("{e}");
            process::exit(1);
        }
        eprintln!("Screenshot saved to {}", path.display());
    }

    if let Some(path) = &cli.audio_path {
        let samples = all_audio.as_deref().unwrap_or(&[]);
        if let Err(e) = save_audio_wav(samples, path) {
            eprintln!("{e}");
            process::exit(1);
        }
        eprintln!("Audio saved to {}", path.display());
    }
}

// ---------------------------------------------------------------------------
// Native menus (muda)
// ---------------------------------------------------------------------------

struct MenuIds {
    screenshot: MenuId,
    quit: MenuId,
    soft_reset: MenuId,
    hard_reset: MenuId,
    model_a1000: MenuId,
    model_a500: MenuId,
    model_a2000: MenuId,
    model_a500plus: MenuId,
    model_a600: MenuId,
    model_a3000: MenuId,
    model_a1200: MenuId,
    model_a4000: MenuId,
}

fn build_menu(
    scanned_roms: &[kickstart_db::ScannedRom],
    has_cli_rom: bool,
) -> (Menu, MenuIds) {
    let menu = Menu::new();

    // File menu.
    let file_menu = Submenu::new("File", true);
    let screenshot = MenuItem::new("Screenshot\tCtrl+P", true, None);
    let quit = MenuItem::new("Quit\tCtrl+Q", true, None);
    file_menu.append(&screenshot).ok();
    file_menu.append(&PredefinedMenuItem::separator()).ok();
    file_menu.append(&quit).ok();

    // System menu.
    let system_menu = Submenu::new("System", true);
    let soft_reset = MenuItem::new("Soft Reset\tCtrl+R", true, None);
    let hard_reset = MenuItem::new("Hard Reset\tCtrl+Shift+R", true, None);
    system_menu.append(&soft_reset).ok();
    system_menu.append(&hard_reset).ok();

    // Model submenu — items disabled when no compatible ROM is found.
    let model_menu = Submenu::new("Model", true);

    let has_rom = |model: AmigaModel| -> bool {
        has_cli_rom || kickstart_db::best_rom_for_model(scanned_roms, model).is_some()
    };

    // OCS
    let model_a1000 = MenuItem::new("A1000", has_rom(AmigaModel::A1000), None);
    let model_a500 = MenuItem::new("A500", has_rom(AmigaModel::A500), None);
    let model_a2000 = MenuItem::new("A2000", has_rom(AmigaModel::A2000), None);
    model_menu.append(&model_a1000).ok();
    model_menu.append(&model_a500).ok();
    model_menu.append(&model_a2000).ok();

    model_menu.append(&PredefinedMenuItem::separator()).ok();

    // ECS
    let model_a500plus = MenuItem::new("A500+", has_rom(AmigaModel::A500Plus), None);
    let model_a600 = MenuItem::new("A600", has_rom(AmigaModel::A600), None);
    let model_a3000 = MenuItem::new("A3000", has_rom(AmigaModel::A3000), None);
    model_menu.append(&model_a500plus).ok();
    model_menu.append(&model_a600).ok();
    model_menu.append(&model_a3000).ok();

    model_menu.append(&PredefinedMenuItem::separator()).ok();

    // AGA
    let model_a1200 = MenuItem::new("A1200", has_rom(AmigaModel::A1200), None);
    let model_a4000 = MenuItem::new("A4000", has_rom(AmigaModel::A4000), None);
    model_menu.append(&model_a1200).ok();
    model_menu.append(&model_a4000).ok();

    system_menu.append(&PredefinedMenuItem::separator()).ok();
    system_menu.append(&model_menu).ok();

    menu.append(&file_menu).ok();
    menu.append(&system_menu).ok();

    let ids = MenuIds {
        screenshot: screenshot.id().clone(),
        quit: quit.id().clone(),
        soft_reset: soft_reset.id().clone(),
        hard_reset: hard_reset.id().clone(),
        model_a1000: model_a1000.id().clone(),
        model_a500: model_a500.id().clone(),
        model_a2000: model_a2000.id().clone(),
        model_a500plus: model_a500plus.id().clone(),
        model_a600: model_a600.id().clone(),
        model_a3000: model_a3000.id().clone(),
        model_a1200: model_a1200.id().clone(),
        model_a4000: model_a4000.id().clone(),
    };

    (menu, ids)
}

// ---------------------------------------------------------------------------
// Windowed mode (winit + wgpu + muda)
// ---------------------------------------------------------------------------

struct App {
    amiga: Amiga,
    audio: Option<AudioOutput>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    last_frame_time: Instant,
    active_keys: HashMap<KeyCode, ActiveKeyMapping>,
    host_left_shift_down: bool,
    host_right_shift_down: bool,
    menu_ids: MenuIds,
    _menu: Menu,
    scanned_roms: Vec<kickstart_db::ScannedRom>,
    cli_rom_data: Option<Vec<u8>>,
    disk_data: Option<Vec<u8>>,
    disk_is_ipf: bool,
    drive_sounds: bool,
}

impl App {
    fn new(
        amiga: Amiga,
        audio: Option<AudioOutput>,
        menu: Menu,
        menu_ids: MenuIds,
        scanned_roms: Vec<kickstart_db::ScannedRom>,
    ) -> Self {
        Self {
            amiga,
            audio,
            window: None,
            renderer: None,
            last_frame_time: Instant::now(),
            active_keys: HashMap::new(),
            host_left_shift_down: false,
            host_right_shift_down: false,
            menu_ids,
            _menu: menu,
            scanned_roms,
            cli_rom_data: None,
            disk_data: None,
            disk_is_ipf: false,
            drive_sounds: true,
        }
    }

    fn host_shift_down(&self) -> bool {
        self.host_left_shift_down || self.host_right_shift_down
    }

    fn send_amiga_key(&mut self, raw_keycode: u8, pressed: bool) {
        self.amiga.key_event(raw_keycode, pressed);
    }

    fn update_host_shift_state(&mut self, code: KeyCode, pressed: bool) {
        match code {
            KeyCode::ShiftLeft => self.host_left_shift_down = pressed,
            KeyCode::ShiftRight => self.host_right_shift_down = pressed,
            _ => {}
        }
    }

    fn resolve_key_press(&self, code: KeyCode, logical_key: &Key) -> Option<ActiveKeyMapping> {
        if let Some(raw_keycode) = map_special_physical_key(code, logical_key) {
            return Some(ActiveKeyMapping {
                raw_keycode,
                synthetic_left_shift: false,
            });
        }

        if let Some((raw_keycode, needs_shift)) = map_logical_char_key(logical_key) {
            return Some(ActiveKeyMapping {
                raw_keycode,
                synthetic_left_shift: needs_shift && !self.host_shift_down(),
            });
        }

        map_printable_physical_key(code).map(|raw_keycode| ActiveKeyMapping {
            raw_keycode,
            synthetic_left_shift: false,
        })
    }

    fn handle_keyboard_input(&mut self, event_loop: &ActiveEventLoop, event: &KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let pressed = event.state == ElementState::Pressed;

        // Runner hotkey: keep F12 reserved for quit so Escape remains usable in the Amiga.
        if code == KeyCode::F12 && pressed {
            event_loop.exit();
            return;
        }

        self.update_host_shift_state(code, pressed);

        if pressed {
            if event.repeat || self.active_keys.contains_key(&code) {
                return;
            }

            let Some(mapping) = self.resolve_key_press(code, &event.logical_key) else {
                return;
            };

            if mapping.synthetic_left_shift {
                self.send_amiga_key(AK_LSHIFT, true);
            }
            self.send_amiga_key(mapping.raw_keycode, true);
            self.active_keys.insert(code, mapping);
            return;
        }

        let Some(mapping) = self.active_keys.remove(&code) else {
            return;
        };
        self.send_amiga_key(mapping.raw_keycode, false);
        if mapping.synthetic_left_shift {
            self.send_amiga_key(AK_LSHIFT, false);
        }
    }

    fn build_composite_framebuffer(&self) -> Vec<u32> {
        let viewport = self.amiga.denise.extract_viewport(
            ViewportPreset::Standard,
            self.amiga.region == AmigaRegion::Pal,
            true,
        );

        let w = VIEWPORT_WIDTH as usize;
        let total_pixels = w * (VIEWPORT_HEIGHT + STATUS_BAR_HEIGHT) as usize;
        let mut fb = Vec::with_capacity(total_pixels);

        // Copy viewport pixels.
        fb.extend_from_slice(&viewport.pixels);

        // Status bar: 12 rows below the viewport.
        let indicators = self.amiga.indicator_state();
        let bar_bg: u32 = 0xFF1A_1A1A;
        fb.resize(total_pixels, bar_bg);

        // Power LED: 8x8 rect at (8, 2) from bar top.
        let power_argb: u32 = if indicators.power_led_on {
            0xFF00_CC00 // bright green
        } else {
            0xFF00_3300 // dim green
        };
        draw_status_rect_argb(&mut fb, w, VIEWPORT_HEIGHT as usize, 8, 2, 8, 8, power_argb);

        // Drive LED: 8x8 rect at (36, 2) from bar top.
        let drive_argb: u32 = if indicators.drive_motor_on && indicators.drive_dma_active {
            0xFF00_CC00 // bright green — active DMA
        } else if indicators.drive_motor_on {
            0xFF00_6600 // dim green — motor spinning
        } else {
            bar_bg // background — off
        };
        draw_status_rect_argb(&mut fb, w, VIEWPORT_HEIGHT as usize, 36, 2, 8, 8, drive_argb);

        fb
    }

    fn switch_model(&mut self, model: AmigaModel) {
        if model == self.amiga.model {
            return;
        }

        // Find the best ROM — CLI override takes priority.
        let kickstart = if let Some(ref data) = self.cli_rom_data {
            data.clone()
        } else if let Some(rom) = kickstart_db::best_rom_for_model(&self.scanned_roms, model) {
            rom.data.clone()
        } else {
            eprintln!("No compatible Kickstart ROM found for {}", model_name(model));
            return;
        };

        let chipset = chipset_for_model(model);

        // Slow RAM defaults per model.
        let slow_ram_size = match model {
            AmigaModel::A1000 | AmigaModel::A500 | AmigaModel::A2000 => 512 * 1024,
            _ => 0,
        };

        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model,
            chipset,
            region: AmigaRegion::Pal,
            kickstart,
            slow_ram_size,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });

        // Re-insert disk if present.
        if let Some(ref disk_bytes) = self.disk_data {
            if self.disk_is_ipf {
                if let Ok(ipf) = format_ipf::IpfImage::from_bytes(disk_bytes) {
                    amiga.insert_disk_image(Box::new(ipf));
                }
            } else if let Ok(adf) = Adf::from_bytes(disk_bytes.clone()) {
                amiga.insert_disk(adf);
            }
        }

        if !self.drive_sounds {
            amiga.drive_sounds.enabled = false;
        }

        self.amiga = amiga;

        let title = format!(
            "Amiga Runner ({}/{})",
            model_name(model),
            chipset_name(chipset)
        );
        if let Some(window) = &self.window {
            window.set_title(&title);
        }
        eprintln!("Switched to {title}");
    }

    fn handle_menu_event(&mut self, id: &MenuId, event_loop: &ActiveEventLoop) {
        if *id == self.menu_ids.quit {
            event_loop.exit();
        } else if *id == self.menu_ids.soft_reset {
            self.amiga.soft_reset();
        } else if *id == self.menu_ids.hard_reset {
            self.amiga.hard_reset();
        } else if *id == self.menu_ids.screenshot {
            let path = std::path::PathBuf::from("screenshot.png");
            match save_screenshot(&self.amiga, &path) {
                Ok(()) => eprintln!("Screenshot saved to {}", path.display()),
                Err(e) => eprintln!("Screenshot error: {e}"),
            }
        } else if *id == self.menu_ids.model_a1000 {
            self.switch_model(AmigaModel::A1000);
        } else if *id == self.menu_ids.model_a500 {
            self.switch_model(AmigaModel::A500);
        } else if *id == self.menu_ids.model_a2000 {
            self.switch_model(AmigaModel::A2000);
        } else if *id == self.menu_ids.model_a500plus {
            self.switch_model(AmigaModel::A500Plus);
        } else if *id == self.menu_ids.model_a600 {
            self.switch_model(AmigaModel::A600);
        } else if *id == self.menu_ids.model_a3000 {
            self.switch_model(AmigaModel::A3000);
        } else if *id == self.menu_ids.model_a1200 {
            self.switch_model(AmigaModel::A1200);
        } else if *id == self.menu_ids.model_a4000 {
            self.switch_model(AmigaModel::A4000);
        }
    }
}

/// Draw a filled rectangle into the status bar region of an ARGB32 buffer.
fn draw_status_rect_argb(
    fb: &mut [u32],
    stride: usize,
    bar_y_start: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    argb: u32,
) {
    for row in y..y + h {
        for col in x..x + w {
            let idx = (bar_y_start + row) * stride + col;
            if idx < fb.len() {
                fb[idx] = argb;
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let size = winit::dpi::LogicalSize::new(
            VIEWPORT_WIDTH * SCALE,
            (VIEWPORT_HEIGHT + STATUS_BAR_HEIGHT) * SCALE,
        );
        let attrs = WindowAttributes::default()
            .with_title(format!(
                "Amiga Runner ({}/{})",
                model_name(self.amiga.model),
                chipset_name(self.amiga.chipset)
            ))
            .with_inner_size(size)
            .with_resizable(false);

        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window = Arc::new(window);

                // Attach native menu.
                #[cfg(target_os = "macos")]
                {
                    self._menu.init_for_nsapp();
                }
                #[cfg(target_os = "windows")]
                {
                    use winit::raw_window_handle::HasWindowHandle;
                    if let Ok(handle) = window.window_handle() {
                        if let winit::raw_window_handle::RawWindowHandle::Win32(h) =
                            handle.as_raw()
                        {
                            unsafe {
                                self._menu
                                    .init_for_hwnd(h.hwnd.get() as _)
                                    .ok();
                            }
                        }
                    }
                }

                let renderer = Renderer::new(
                    window.clone(),
                    VIEWPORT_WIDTH,
                    VIEWPORT_HEIGHT + STATUS_BAR_HEIGHT,
                    emu_core::renderer::FilterMode::Nearest,
                );
                self.renderer = Some(renderer);
                self.window = Some(window);
            }
            Err(e) => {
                eprintln!("Failed to create window: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_keyboard_input(event_loop, &event);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= FRAME_DURATION {
                    self.amiga.run_frame();
                    if let Some(audio) = &self.audio {
                        let samples = self.amiga.take_audio_buffer();
                        audio.push_samples(&samples);
                    } else {
                        let _ = self.amiga.take_audio_buffer();
                    }

                    let fb = self.build_composite_framebuffer();
                    if let Some(renderer) = &mut self.renderer {
                        renderer.upload_framebuffer(&fb);
                    }

                    self.last_frame_time = now;
                }

                if let Some(renderer) = &self.renderer {
                    if let Err(e) = renderer.render() {
                        eprintln!("Render error: {e}");
                        event_loop.exit();
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Process menu events.
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            self.handle_menu_event(event.id(), event_loop);
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn map_special_physical_key(code: KeyCode, logical_key: &Key) -> Option<u8> {
    let raw = match code {
        KeyCode::Space => AK_SPACE,
        KeyCode::Tab => AK_TAB,
        KeyCode::Enter => AK_RETURN,
        KeyCode::NumpadEnter => 0x43,
        KeyCode::Escape => AK_ESCAPE,
        KeyCode::Backspace => AK_BACKSPACE,
        KeyCode::Delete => AK_DELETE,
        KeyCode::ArrowUp => AK_CURSOR_UP,
        KeyCode::ArrowDown => AK_CURSOR_DOWN,
        KeyCode::ArrowRight => AK_CURSOR_RIGHT,
        KeyCode::ArrowLeft => AK_CURSOR_LEFT,
        KeyCode::F1 => 0x50,
        KeyCode::F2 => 0x51,
        KeyCode::F3 => 0x52,
        KeyCode::F4 => 0x53,
        KeyCode::F5 => 0x54,
        KeyCode::F6 => 0x55,
        KeyCode::F7 => 0x56,
        KeyCode::F8 => 0x57,
        KeyCode::F9 => 0x58,
        KeyCode::F10 => 0x59,
        KeyCode::ShiftLeft => AK_LSHIFT,
        KeyCode::ShiftRight => AK_RSHIFT,
        KeyCode::CapsLock => AK_CAPSLOCK,
        KeyCode::ControlLeft | KeyCode::ControlRight => AK_CTRL,
        KeyCode::AltLeft => AK_LALT,
        KeyCode::AltRight => {
            if matches!(logical_key, Key::Named(NamedKey::AltGraph)) {
                return None;
            }
            AK_RALT
        }
        KeyCode::SuperLeft => AK_LAMIGA,
        KeyCode::SuperRight => AK_RAMIGA,
        KeyCode::Numpad0 => 0x0F,
        KeyCode::Numpad1 => 0x1D,
        KeyCode::Numpad2 => 0x1E,
        KeyCode::Numpad3 => 0x1F,
        KeyCode::Numpad4 => 0x2D,
        KeyCode::Numpad5 => 0x2E,
        KeyCode::Numpad6 => 0x2F,
        KeyCode::Numpad7 => 0x3D,
        KeyCode::Numpad8 => 0x3E,
        KeyCode::Numpad9 => 0x3F,
        KeyCode::NumpadDecimal => 0x3C,
        KeyCode::NumpadSubtract => 0x4A,
        KeyCode::NumpadAdd => 0x5E,
        KeyCode::NumpadDivide => 0x5C,
        KeyCode::NumpadMultiply => 0x5D,
        KeyCode::NumpadParenLeft => 0x5A,
        KeyCode::NumpadParenRight => 0x5B,
        _ => return None,
    };
    Some(raw)
}

fn map_logical_char_key(logical_key: &Key) -> Option<(u8, bool)> {
    let Key::Character(text) = logical_key else {
        return None;
    };

    let mut chars = text.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }

    map_char_to_amiga_key(ch)
}

fn map_char_to_amiga_key(ch: char) -> Option<(u8, bool)> {
    let lowered = ch.to_ascii_lowercase();
    let is_uppercase_ascii = ch.is_ascii_alphabetic() && ch.is_ascii_uppercase();

    let (raw, needs_shift) = match lowered {
        '`' => (0x00, false),
        '1' => (0x01, false),
        '2' => (0x02, false),
        '3' => (0x03, false),
        '4' => (0x04, false),
        '5' => (0x05, false),
        '6' => (0x06, false),
        '7' => (0x07, false),
        '8' => (0x08, false),
        '9' => (0x09, false),
        '0' => (0x0A, false),
        '-' => (0x0B, false),
        '=' => (0x0C, false),
        '\\' => (0x0D, false),
        'q' => (0x10, false),
        'w' => (0x11, false),
        'e' => (0x12, false),
        'r' => (0x13, false),
        't' => (0x14, false),
        'y' => (0x15, false),
        'u' => (0x16, false),
        'i' => (0x17, false),
        'o' => (0x18, false),
        'p' => (0x19, false),
        '[' => (0x1A, false),
        ']' => (0x1B, false),
        'a' => (0x20, false),
        's' => (0x21, false),
        'd' => (0x22, false),
        'f' => (0x23, false),
        'g' => (0x24, false),
        'h' => (0x25, false),
        'j' => (0x26, false),
        'k' => (0x27, false),
        'l' => (0x28, false),
        ';' => (0x29, false),
        '\'' => (0x2A, false),
        'z' => (0x31, false),
        'x' => (0x32, false),
        'c' => (0x33, false),
        'v' => (0x34, false),
        'b' => (0x35, false),
        'n' => (0x36, false),
        'm' => (0x37, false),
        ',' => (0x38, false),
        '.' => (0x39, false),
        '/' => (0x3A, false),
        ' ' => (AK_SPACE, false),

        '~' => (0x00, true),
        '!' => (0x01, true),
        '@' => (0x02, true),
        '#' => (0x03, true),
        '$' => (0x04, true),
        '%' => (0x05, true),
        '^' => (0x06, true),
        '&' => (0x07, true),
        '*' => (0x08, true),
        '(' => (0x09, true),
        ')' => (0x0A, true),
        '_' => (0x0B, true),
        '+' => (0x0C, true),
        '|' => (0x0D, true),
        '{' => (0x1A, true),
        '}' => (0x1B, true),
        ':' => (0x29, true),
        '"' => (0x2A, true),
        '<' => (0x38, true),
        '>' => (0x39, true),
        '?' => (0x3A, true),
        _ => return None,
    };

    Some((raw, needs_shift || is_uppercase_ascii))
}

fn map_printable_physical_key(code: KeyCode) -> Option<u8> {
    let raw = match code {
        KeyCode::Backquote => 0x00,
        KeyCode::Digit1 => 0x01,
        KeyCode::Digit2 => 0x02,
        KeyCode::Digit3 => 0x03,
        KeyCode::Digit4 => 0x04,
        KeyCode::Digit5 => 0x05,
        KeyCode::Digit6 => 0x06,
        KeyCode::Digit7 => 0x07,
        KeyCode::Digit8 => 0x08,
        KeyCode::Digit9 => 0x09,
        KeyCode::Digit0 => 0x0A,
        KeyCode::Minus => 0x0B,
        KeyCode::Equal => 0x0C,
        KeyCode::Backslash => 0x0D,
        KeyCode::KeyQ => 0x10,
        KeyCode::KeyW => 0x11,
        KeyCode::KeyE => 0x12,
        KeyCode::KeyR => 0x13,
        KeyCode::KeyT => 0x14,
        KeyCode::KeyY => 0x15,
        KeyCode::KeyU => 0x16,
        KeyCode::KeyI => 0x17,
        KeyCode::KeyO => 0x18,
        KeyCode::KeyP => 0x19,
        KeyCode::BracketLeft => 0x1A,
        KeyCode::BracketRight => 0x1B,
        KeyCode::KeyA => 0x20,
        KeyCode::KeyS => 0x21,
        KeyCode::KeyD => 0x22,
        KeyCode::KeyF => 0x23,
        KeyCode::KeyG => 0x24,
        KeyCode::KeyH => 0x25,
        KeyCode::KeyJ => 0x26,
        KeyCode::KeyK => 0x27,
        KeyCode::KeyL => 0x28,
        KeyCode::Semicolon => 0x29,
        KeyCode::Quote => 0x2A,
        KeyCode::IntlBackslash => 0x30, // international cut-out key
        KeyCode::KeyZ => 0x31,
        KeyCode::KeyX => 0x32,
        KeyCode::KeyC => 0x33,
        KeyCode::KeyV => 0x34,
        KeyCode::KeyB => 0x35,
        KeyCode::KeyN => 0x36,
        KeyCode::KeyM => 0x37,
        KeyCode::Comma => 0x38,
        KeyCode::Period => 0x39,
        KeyCode::Slash => 0x3A,
        _ => return None,
    };
    Some(raw)
}

fn main() {
    let cli = parse_args();

    if cli.mcp {
        AmigaMcpServer::new(AmigaMcp::new()).run();
        return;
    }

    if let Some(ref path) = cli.script_path {
        let mut server = AmigaMcpServer::new(AmigaMcp::new());
        if let Err(e) = server.run_script(path) {
            eprintln!("Script error: {e}");
            process::exit(1);
        }
        return;
    }

    if cli.headless {
        run_headless(&cli);
        return;
    }

    // Scan roms directory for known Kickstart ROMs.
    let roms_dir = find_roms_dir();
    eprintln!("Scanning {} for Kickstart ROMs...", roms_dir.display());
    let scanned_roms = kickstart_db::scan_roms(&roms_dir);
    eprintln!("Found {} known Kickstart ROM(s)", scanned_roms.len());

    let has_cli_rom = !cli.rom_path.as_os_str().is_empty();

    let amiga = make_amiga(&cli);

    // Cache disk data for model switching.
    let disk_path_ref = cli.disk_path.as_ref().or(cli.adf_path.as_ref());
    let disk_data = disk_path_ref.and_then(|p| std::fs::read(p).ok());
    let disk_is_ipf = disk_data
        .as_ref()
        .is_some_and(|d| format_ipf::IpfImage::is_ipf(d));

    let audio = if cli.mute {
        None
    } else {
        match AudioOutput::new() {
            Ok(output) => Some(output),
            Err(e) => {
                eprintln!("Audio disabled: {e}");
                None
            }
        }
    };
    let (menu, menu_ids) = build_menu(&scanned_roms, has_cli_rom);
    let mut app = App::new(amiga, audio, menu, menu_ids, scanned_roms);
    app.cli_rom_data = if has_cli_rom {
        std::fs::read(&cli.rom_path).ok()
    } else {
        None
    };
    app.disk_data = disk_data;
    app.disk_is_ipf = disk_is_ipf;
    app.drive_sounds = cli.drive_sounds;

    let event_loop = match EventLoop::new() {
        Ok(loop_) => loop_,
        Err(e) => {
            eprintln!("Failed to create event loop: {e}");
            process::exit(1);
        }
    };

    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("Event loop error: {e}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CliArgs, chipset_for_model, map_char_to_amiga_key, map_printable_physical_key,
        parse_args_from, parse_model_arg,
    };
    use machine_amiga::{AmigaChipset, AmigaModel};
    use std::path::PathBuf;
    use winit::keyboard::KeyCode;

    fn parse_cli(args: &[&str], env_rom: Option<&str>) -> Result<Option<CliArgs>, String> {
        let args = args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>();
        parse_args_from(&args, env_rom.map(PathBuf::from))
    }

    #[test]
    fn shifted_digit_two_maps_to_amiga_at() {
        assert_eq!(map_char_to_amiga_key('@'), Some((0x02, true)));
    }

    #[test]
    fn uppercase_letter_requires_shift() {
        assert_eq!(map_char_to_amiga_key('A'), Some((0x20, true)));
        assert_eq!(map_char_to_amiga_key('a'), Some((0x20, false)));
    }

    #[test]
    fn physical_fallback_keeps_position_for_digit_two() {
        assert_eq!(map_printable_physical_key(KeyCode::Digit2), Some(0x02));
    }

    #[test]
    fn chipset_is_derived_from_model() {
        assert_eq!(chipset_for_model(AmigaModel::A1000), AmigaChipset::Ocs);
        assert_eq!(chipset_for_model(AmigaModel::A500), AmigaChipset::Ocs);
        assert_eq!(chipset_for_model(AmigaModel::A2000), AmigaChipset::Ocs);
        assert_eq!(chipset_for_model(AmigaModel::A500Plus), AmigaChipset::Ecs);
        assert_eq!(chipset_for_model(AmigaModel::A600), AmigaChipset::Ecs);
        assert_eq!(chipset_for_model(AmigaModel::A3000), AmigaChipset::Ecs);
        assert_eq!(chipset_for_model(AmigaModel::A1200), AmigaChipset::Aga);
        assert_eq!(chipset_for_model(AmigaModel::A4000), AmigaChipset::Aga);
    }

    #[test]
    fn model_arg_parser_accepts_all_models() {
        assert_eq!(parse_model_arg("a1000"), Ok(AmigaModel::A1000));
        assert_eq!(parse_model_arg("a500"), Ok(AmigaModel::A500));
        assert_eq!(parse_model_arg("A500+"), Ok(AmigaModel::A500Plus));
        assert_eq!(parse_model_arg("a500plus"), Ok(AmigaModel::A500Plus));
        assert_eq!(parse_model_arg("a600"), Ok(AmigaModel::A600));
        assert_eq!(parse_model_arg("a1200"), Ok(AmigaModel::A1200));
        assert_eq!(parse_model_arg("a2000"), Ok(AmigaModel::A2000));
        assert_eq!(parse_model_arg("a3000"), Ok(AmigaModel::A3000));
        assert_eq!(parse_model_arg("a4000"), Ok(AmigaModel::A4000));
    }

    #[test]
    fn model_arg_parser_rejects_invalid_values() {
        assert!(parse_model_arg("a9999").is_err());
    }

    #[test]
    fn cli_parser_uses_env_rom_for_non_mcp_modes() {
        let cli = parse_cli(&["emu-amiga", "--headless"], Some("/tmp/kick.rom"))
            .expect("parse should succeed")
            .expect("help was not requested");

        assert_eq!(cli.rom_path, PathBuf::from("/tmp/kick.rom"));
        assert!(cli.headless);
        assert_eq!(cli.model, AmigaModel::A500);
        assert_eq!(chipset_for_model(cli.model), AmigaChipset::Ocs);
    }

    #[test]
    fn cli_parser_allows_empty_rom_in_mcp_mode() {
        let cli = parse_cli(&["emu-amiga", "--mcp"], None)
            .expect("parse should succeed")
            .expect("help was not requested");

        assert!(cli.mcp);
        assert!(cli.rom_path.as_os_str().is_empty());
    }

    #[test]
    fn cli_parser_allows_empty_rom_in_script_mode() {
        let cli = parse_cli(&["emu-amiga", "--script", "boot.json"], None)
            .expect("parse should succeed")
            .expect("help was not requested");

        assert_eq!(cli.script_path, Some(PathBuf::from("boot.json")));
        assert!(cli.rom_path.as_os_str().is_empty());
    }

    #[test]
    fn cli_parser_promotes_capture_modes_to_headless() {
        let cli = parse_cli(
            &[
                "emu-amiga",
                "--rom",
                "kick.rom",
                "--screenshot",
                "out.png",
                "--audio",
                "out.wav",
            ],
            None,
        )
        .expect("parse should succeed")
        .expect("help was not requested");

        assert!(cli.headless);
        assert_eq!(cli.screenshot_path, Some(PathBuf::from("out.png")));
        assert_eq!(cli.audio_path, Some(PathBuf::from("out.wav")));
    }

    #[test]
    fn cli_parser_derives_chipset_from_selected_model() {
        let ecs = parse_cli(
            &["emu-amiga", "--rom", "kick.rom", "--model", "a500plus"],
            None,
        )
        .expect("parse should succeed")
        .expect("help was not requested");
        let aga = parse_cli(
            &["emu-amiga", "--rom", "kick.rom", "--model", "a1200"],
            None,
        )
        .expect("parse should succeed")
        .expect("help was not requested");

        assert_eq!(ecs.model, AmigaModel::A500Plus);
        assert_eq!(chipset_for_model(ecs.model), AmigaChipset::Ecs);
        assert_eq!(aga.model, AmigaModel::A1200);
        assert_eq!(chipset_for_model(aga.model), AmigaChipset::Aga);
    }

    #[test]
    fn cli_parser_rejects_removed_chipset_override() {
        let result = parse_cli(
            &["emu-amiga", "--rom", "kick.rom", "--chipset", "ecs"],
            None,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown argument: --chipset"));
    }

    #[test]
    fn cli_parser_reports_help_and_unknown_arguments() {
        assert!(matches!(
            parse_cli(&["emu-amiga", "--help"], None).expect("help parse should succeed"),
            None
        ));

        let result = parse_cli(&["emu-amiga", "--bogus"], None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown argument: --bogus"));
    }
}
