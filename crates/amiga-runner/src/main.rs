//! Minimal runner for the Amiga machine core.
//!
//! Scope: video output and basic Paula audio playback/capture. Loads a
//! Kickstart ROM and optionally inserts an ADF into DF0:, then either runs a
//! windowed frontend or captures a framebuffer screenshot/audio in headless
//! mode.

#![allow(clippy::cast_possible_truncation)]

use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use machine_amiga::format_adf::Adf;
use machine_amiga::{
    Amiga, AmigaChipset, AmigaConfig, AmigaModel, BeamDebugSnapshot, commodore_denise_ocs,
};
use pixels::{Pixels, SurfaceTexture};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const FB_WIDTH: u32 = commodore_denise_ocs::FB_WIDTH;
const FB_HEIGHT: u32 = commodore_denise_ocs::FB_HEIGHT;
const SCALE: u32 = 3;
const FRAME_DURATION: Duration = Duration::from_millis(20); // PAL ~50 Hz
const AUDIO_CHANNELS: usize = 2;
const AUDIO_QUEUE_SECONDS: usize = 2;
const PAL_CRYSTAL_HZ: u64 = 28_375_160;

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

struct CliArgs {
    rom_path: PathBuf,
    adf_path: Option<PathBuf>,
    chipset: AmigaChipset,
    headless: bool,
    frames: u32,
    beam_debug: bool,
    beam_debug_filter: BeamDebugFilter,
    bench_insert_screen: bool,
    screenshot_path: Option<PathBuf>,
    audio_path: Option<PathBuf>,
    mute: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BeamDebugFilter {
    hsync: bool,
    vsync: bool,
    hblank: bool,
    vblank: bool,
    visible: bool,
    hsync_pin: bool,
    vsync_pin: bool,
    csync_pin: bool,
    blank_pin: bool,
}

impl BeamDebugFilter {
    const fn all() -> Self {
        Self {
            hsync: true,
            vsync: true,
            hblank: true,
            vblank: true,
            visible: true,
            hsync_pin: true,
            vsync_pin: true,
            csync_pin: true,
            blank_pin: true,
        }
    }

    const fn none() -> Self {
        Self {
            hsync: false,
            vsync: false,
            hblank: false,
            vblank: false,
            visible: false,
            hsync_pin: false,
            vsync_pin: false,
            csync_pin: false,
            blank_pin: false,
        }
    }
}

fn print_usage_and_exit(code: i32) -> ! {
    eprintln!("Usage: amiga-runner [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --rom <file>   Kickstart ROM file (or use AMIGA_KS13_ROM env var)");
    eprintln!("  --adf <file>   Optional ADF disk image to insert into DF0:");
    eprintln!("  --chipset <ocs|ecs>  Select chipset [default: ocs]");
    eprintln!("  --headless     Run without a window");
    eprintln!("  --frames <n>   Frames to run in headless mode [default: 300]");
    eprintln!("  --beam-debug   Print beam sync/blank/visibility edge transitions (headless)");
    eprintln!(
        "  --beam-debug-filter <classes>  Edge classes: all,sync,blank,visible,pins or comma list"
    );
    eprintln!("  --bench-insert-screen  Stop on first KS1.3 insert-screen match and print speed");
    eprintln!("  --screenshot <file.png>  Save a framebuffer screenshot (headless)");
    eprintln!("  --audio <file.wav>  Save a WAV audio dump (headless)");
    eprintln!("  --mute         Disable host audio playback (windowed)");
    eprintln!("  -h, --help     Show this help");
    process::exit(code);
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut rom_path: Option<PathBuf> = None;
    let mut adf_path: Option<PathBuf> = None;
    let mut chipset = AmigaChipset::Ocs;
    let mut headless = false;
    let mut frames = 300;
    let mut beam_debug = false;
    let mut beam_debug_filter = BeamDebugFilter::all();
    let mut bench_insert_screen = false;
    let mut screenshot_path: Option<PathBuf> = None;
    let mut audio_path: Option<PathBuf> = None;
    let mut mute = false;

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
            "--chipset" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    eprintln!("Missing value for --chipset (expected ocs or ecs)");
                    print_usage_and_exit(1);
                };
                chipset = parse_chipset_arg(value).unwrap_or_else(|e| {
                    eprintln!("{e}");
                    print_usage_and_exit(1);
                });
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
            "--beam-debug" => {
                beam_debug = true;
            }
            "--beam-debug-filter" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    eprintln!("Missing value for --beam-debug-filter");
                    print_usage_and_exit(1);
                };
                beam_debug_filter = parse_beam_debug_filter_arg(value).unwrap_or_else(|e| {
                    eprintln!("{e}");
                    print_usage_and_exit(1);
                });
                beam_debug = true;
            }
            "--bench-insert-screen" => {
                bench_insert_screen = true;
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
            "-h" | "--help" => print_usage_and_exit(0),
            other => {
                eprintln!("Unknown argument: {other}");
                print_usage_and_exit(1);
            }
        }
        i += 1;
    }

    let rom_path = rom_path
        .or_else(|| std::env::var_os("AMIGA_KS13_ROM").map(PathBuf::from))
        .unwrap_or_else(|| {
            eprintln!("No Kickstart ROM specified.");
            print_usage_and_exit(1);
        });

    if screenshot_path.is_some() || audio_path.is_some() || bench_insert_screen || beam_debug {
        headless = true;
    }

    CliArgs {
        rom_path,
        adf_path,
        chipset,
        headless,
        frames,
        beam_debug,
        beam_debug_filter,
        bench_insert_screen,
        screenshot_path,
        audio_path,
        mute,
    }
}

fn parse_chipset_arg(value: &str) -> Result<AmigaChipset, String> {
    match value.to_ascii_lowercase().as_str() {
        "ocs" => Ok(AmigaChipset::Ocs),
        "ecs" => Ok(AmigaChipset::Ecs),
        other => Err(format!(
            "Invalid --chipset value '{other}' (expected 'ocs' or 'ecs')"
        )),
    }
}

fn parse_beam_debug_filter_arg(value: &str) -> Result<BeamDebugFilter, String> {
    let mut filter = BeamDebugFilter::none();
    for raw_token in value.split(',') {
        let token = raw_token.trim().to_ascii_lowercase();
        if token.is_empty() {
            return Err(String::from(
                "Invalid --beam-debug-filter value (empty filter token)",
            ));
        }
        match token.as_str() {
            "all" => filter = BeamDebugFilter::all(),
            "sync" => {
                filter.hsync = true;
                filter.vsync = true;
            }
            "blank" => {
                filter.hblank = true;
                filter.vblank = true;
            }
            "visible" => {
                filter.visible = true;
            }
            "pins" => {
                filter.hsync_pin = true;
                filter.vsync_pin = true;
                filter.csync_pin = true;
                filter.blank_pin = true;
            }
            "hsync" => filter.hsync = true,
            "vsync" => filter.vsync = true,
            "hblank" => filter.hblank = true,
            "vblank" => filter.vblank = true,
            "hsync-pin" | "pin-hsync" => filter.hsync_pin = true,
            "vsync-pin" | "pin-vsync" => filter.vsync_pin = true,
            "csync-pin" | "pin-csync" => filter.csync_pin = true,
            "blank-pin" | "pin-blank" => filter.blank_pin = true,
            other => {
                return Err(format!(
                    "Invalid --beam-debug-filter token '{other}' (use all,sync,blank,visible,pins or hsync/vsync/hblank/vblank and *-pin)"
                ));
            }
        }
    }
    Ok(filter)
}

fn chipset_name(chipset: AmigaChipset) -> &'static str {
    match chipset {
        AmigaChipset::Ocs => "OCS",
        AmigaChipset::Ecs => "ECS",
    }
}

fn make_amiga(cli: &CliArgs) -> Amiga {
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
        model: AmigaModel::A500,
        chipset: cli.chipset,
        kickstart,
    });

    if let Some(adf_path) = &cli.adf_path {
        let adf_bytes = match std::fs::read(adf_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read ADF {}: {e}", adf_path.display());
                process::exit(1);
            }
        };
        let adf = match Adf::from_bytes(adf_bytes) {
            Ok(adf) => adf,
            Err(e) => {
                eprintln!("Invalid ADF {}: {e}", adf_path.display());
                process::exit(1);
            }
        };
        amiga.insert_disk(adf);
        eprintln!("Inserted disk: {}", adf_path.display());
    }

    eprintln!(
        "Loaded Kickstart ROM: {} (chipset {})",
        cli.rom_path.display(),
        chipset_name(cli.chipset)
    );
    amiga
}

fn save_screenshot(amiga: &Amiga, path: &PathBuf) -> Result<(), String> {
    let file = File::create(path)
        .map_err(|e| format!("failed to create screenshot {}: {e}", path.display()))?;
    let writer = BufWriter::new(file);

    let mut encoder = png::Encoder::new(writer, FB_WIDTH, FB_HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut png_writer = encoder
        .write_header()
        .map_err(|e| format!("failed to write PNG header {}: {e}", path.display()))?;

    let mut bytes = vec![0u8; (FB_WIDTH * FB_HEIGHT * 4) as usize];
    for (i, &argb) in amiga.framebuffer().iter().enumerate() {
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

fn matches_ks13_insert_screen(framebuffer: &[u32]) -> bool {
    const WHITE: u32 = 0xFFFF_FFFF;
    const BLACK: u32 = 0xFF00_0000;
    const FLOPPY_BLUE: u32 = 0xFF77_77CC;
    const METAL_GRAY: u32 = 0xFFBB_BBBB;

    let px = |x: u32, y: u32| -> u32 { framebuffer[(y * FB_WIDTH + x) as usize] };
    if px(0, 0) != WHITE
        || px(103, 50) != BLACK
        || px(106, 52) != FLOPPY_BLUE
        || px(131, 52) != METAL_GRAY
    {
        return false;
    }

    let mut white_count = 0u32;
    let mut black_count = 0u32;
    let mut blue_count = 0u32;
    let mut gray_count = 0u32;
    let mut non_white_pixels = 0u32;
    let mut min_x = FB_WIDTH;
    let mut min_y = FB_HEIGHT;
    let mut max_x = 0u32;
    let mut max_y = 0u32;

    for y in 0..FB_HEIGHT {
        for x in 0..FB_WIDTH {
            let p = px(x, y);
            match p {
                WHITE => white_count += 1,
                BLACK => black_count += 1,
                FLOPPY_BLUE => blue_count += 1,
                METAL_GRAY => gray_count += 1,
                _ => return false,
            }

            if p != WHITE {
                non_white_pixels += 1;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    (70_000..=78_000).contains(&white_count)
        && (2_000..=4_000).contains(&black_count)
        && (3_000..=5_000).contains(&blue_count)
        && (700..=1_300).contains(&gray_count)
        && (6_000..=9_000).contains(&non_white_pixels)
        && (75..=90).contains(&min_x)
        && (45..=60).contains(&min_y)
        && (200..=215).contains(&max_x)
        && (170..=185).contains(&max_y)
}

struct BeamEdgeLogger {
    initialized: bool,
    last_snapshot: Option<BeamDebugSnapshot>,
    filter: BeamDebugFilter,
    edge_count: u64,
}

impl BeamEdgeLogger {
    fn new(filter: BeamDebugFilter) -> Self {
        Self {
            initialized: false,
            last_snapshot: None,
            filter,
            edge_count: 0,
        }
    }

    fn observe(&mut self, amiga: &Amiga) {
        let snapshot = amiga.current_beam_debug_snapshot();
        let edges = amiga.current_beam_edge_flags();
        let visible = snapshot.fb_coords.is_some();

        if !self.initialized {
            self.initialized = true;
            self.last_snapshot = Some(snapshot);
            eprintln!(
                "[beam] init mc={} v={} h={} hsync={} vsync={} hblank={} vblank={} visible={} hpin={} vpin={} cpin={} blank_pin={}",
                amiga.master_clock,
                snapshot.vpos,
                snapshot.hpos_cck,
                snapshot.sync.hsync,
                snapshot.sync.vsync,
                snapshot.hblank,
                snapshot.vblank,
                visible,
                snapshot.pins.hsync_high,
                snapshot.pins.vsync_high,
                snapshot.pins.csync_high,
                snapshot.pins.blank_active,
            );
            return;
        }

        let prev = self.last_snapshot.unwrap_or(snapshot);
        let mut changes = Vec::with_capacity(5);
        if self.filter.hsync && edges.hsync_changed {
            changes.push(format!("hsync={}", snapshot.sync.hsync));
        }
        if self.filter.vsync && edges.vsync_changed {
            changes.push(format!("vsync={}", snapshot.sync.vsync));
        }
        if self.filter.hblank && edges.hblank_changed {
            changes.push(format!("hblank={}", snapshot.hblank));
        }
        if self.filter.vblank && edges.vblank_changed {
            changes.push(format!("vblank={}", snapshot.vblank));
        }
        if self.filter.visible && edges.visible_changed {
            changes.push(format!("visible={visible}"));
        }
        if self.filter.hsync_pin && prev.pins.hsync_high != snapshot.pins.hsync_high {
            changes.push(format!("hpin={}", snapshot.pins.hsync_high));
        }
        if self.filter.vsync_pin && prev.pins.vsync_high != snapshot.pins.vsync_high {
            changes.push(format!("vpin={}", snapshot.pins.vsync_high));
        }
        if self.filter.csync_pin && prev.pins.csync_high != snapshot.pins.csync_high {
            changes.push(format!("cpin={}", snapshot.pins.csync_high));
        }
        if self.filter.blank_pin && prev.pins.blank_active != snapshot.pins.blank_active {
            changes.push(format!("blank_pin={}", snapshot.pins.blank_active));
        }

        if !changes.is_empty() {
            self.edge_count += 1;
            eprintln!(
                "[beam] edge#{:06} mc={} v={} h={} {} fb={:?}",
                self.edge_count,
                amiga.master_clock,
                snapshot.vpos,
                snapshot.hpos_cck,
                changes.join(" "),
                snapshot.fb_coords
            );
        }

        self.last_snapshot = Some(snapshot);
    }
}

fn run_frame_with_optional_beam_debug(amiga: &mut Amiga, beam_debug: Option<&mut BeamEdgeLogger>) {
    let Some(logger) = beam_debug else {
        amiga.run_frame();
        return;
    };

    for _ in 0..machine_amiga::PAL_FRAME_TICKS {
        amiga.tick();
        if amiga.master_clock % machine_amiga::TICKS_PER_CCK == 0 {
            logger.observe(amiga);
        }
    }
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

        let mut queue = match self.queue.lock() {
            Ok(queue) => queue,
            Err(_) => return,
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
    let mut guard = match queue.lock() {
        Ok(guard) => guard,
        Err(_) => {
            data.fill(0.0);
            return;
        }
    };

    for sample in data {
        *sample = guard.pop_front().unwrap_or(0.0);
    }
}

fn write_audio_data_i16(data: &mut [i16], queue: &Arc<Mutex<VecDeque<f32>>>) {
    let mut guard = match queue.lock() {
        Ok(guard) => guard,
        Err(_) => {
            data.fill(0);
            return;
        }
    };

    for sample in data {
        let value = guard.pop_front().unwrap_or(0.0).clamp(-1.0, 1.0);
        *sample = (value * f32::from(i16::MAX)) as i16;
    }
}

fn write_audio_data_u16(data: &mut [u16], queue: &Arc<Mutex<VecDeque<f32>>>) {
    let mut guard = match queue.lock() {
        Ok(guard) => guard,
        Err(_) => {
            data.fill(u16::MAX / 2);
            return;
        }
    };

    for sample in data {
        let value = guard.pop_front().unwrap_or(0.0).clamp(-1.0, 1.0);
        let scaled = ((value * 0.5) + 0.5) * f32::from(u16::MAX);
        *sample = scaled as u16;
    }
}

fn run_headless(cli: &CliArgs) {
    let mut amiga = make_amiga(cli);
    let mut beam_debug = cli
        .beam_debug
        .then(|| BeamEdgeLogger::new(cli.beam_debug_filter));
    let mut all_audio = if cli.audio_path.is_some() {
        Some(Vec::new())
    } else {
        None
    };
    let bench_start = cli.bench_insert_screen.then(Instant::now);
    let mut bench_hit_frame: Option<u32> = None;
    let mut frames_executed = 0u32;

    for frame_idx in 0..cli.frames {
        run_frame_with_optional_beam_debug(&mut amiga, beam_debug.as_mut());
        frames_executed = frame_idx + 1;
        let audio = amiga.take_audio_buffer();
        if let Some(buffer) = all_audio.as_mut() {
            buffer.extend_from_slice(&audio);
        }

        if cli.bench_insert_screen && matches_ks13_insert_screen(amiga.framebuffer()) {
            bench_hit_frame = Some(frames_executed);
            break;
        }
    }

    if let Some(logger) = &beam_debug {
        eprintln!("[beam] total edge transitions: {}", logger.edge_count);
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

    if cli.bench_insert_screen {
        let wall_seconds = bench_start
            .map(|start| start.elapsed().as_secs_f64())
            .unwrap_or_default();
        let measured_frames = bench_hit_frame.unwrap_or(frames_executed);
        let emu_seconds = (f64::from(measured_frames) * machine_amiga::PAL_FRAME_TICKS as f64)
            / PAL_CRYSTAL_HZ as f64;
        let ratio = if wall_seconds > 0.0 {
            emu_seconds / wall_seconds
        } else {
            0.0
        };

        if bench_hit_frame.is_some() {
            eprintln!("KS1.3 insert-screen detected.");
        } else {
            eprintln!(
                "KS1.3 insert-screen not detected within {} frames.",
                cli.frames
            );
        }
        eprintln!("  Frames run: {measured_frames}");
        eprintln!("  Emulated time: {emu_seconds:.3}s");
        eprintln!("  Wall time: {wall_seconds:.3}s");
        eprintln!("  Realtime ratio: {ratio:.3}x");
        if ratio >= 1.0 {
            eprintln!("  Speed: {:.2}x faster than real time", ratio);
        } else if ratio > 0.0 {
            eprintln!("  Speed: {:.2}x slower than real time", 1.0 / ratio);
        }

        if bench_hit_frame.is_none() {
            process::exit(2);
        }
    }
}

struct App {
    amiga: Amiga,
    audio: Option<AudioOutput>,
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
    last_frame_time: Instant,
    active_keys: HashMap<KeyCode, ActiveKeyMapping>,
    host_left_shift_down: bool,
    host_right_shift_down: bool,
}

impl App {
    fn new(amiga: Amiga, audio: Option<AudioOutput>) -> Self {
        Self {
            amiga,
            audio,
            window: None,
            pixels: None,
            last_frame_time: Instant::now(),
            active_keys: HashMap::new(),
            host_left_shift_down: false,
            host_right_shift_down: false,
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

    fn handle_keyboard_input(&mut self, event_loop: &ActiveEventLoop, event: KeyEvent) {
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

    fn update_pixels(&mut self) {
        let Some(pixels) = self.pixels.as_mut() else {
            return;
        };

        let frame = pixels.frame_mut();
        let fb = self.amiga.framebuffer();

        for (i, &argb) in fb.iter().enumerate() {
            let o = i * 4;
            frame[o] = ((argb >> 16) & 0xFF) as u8; // R
            frame[o + 1] = ((argb >> 8) & 0xFF) as u8; // G
            frame[o + 2] = (argb & 0xFF) as u8; // B
            frame[o + 3] = ((argb >> 24) & 0xFF) as u8; // A
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let size = winit::dpi::LogicalSize::new(FB_WIDTH * SCALE, FB_HEIGHT * SCALE);
        let attrs = WindowAttributes::default()
            .with_title(format!(
                "Amiga Runner (A500/{})",
                chipset_name(self.amiga.chipset)
            ))
            .with_inner_size(size)
            .with_resizable(false);

        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window: &'static Window = Box::leak(Box::new(window));
                let inner = window.inner_size();
                let surface = SurfaceTexture::new(inner.width, inner.height, window);
                let pixels = match Pixels::new(FB_WIDTH, FB_HEIGHT, surface) {
                    Ok(pixels) => pixels,
                    Err(e) => {
                        eprintln!("Failed to create pixels surface: {e}");
                        event_loop.exit();
                        return;
                    }
                };

                self.pixels = Some(pixels);
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
                self.handle_keyboard_input(event_loop, event);
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
                    self.update_pixels();
                    self.last_frame_time = now;
                }

                if let Some(pixels) = self.pixels.as_ref()
                    && let Err(e) = pixels.render()
                {
                    eprintln!("Render error: {e}");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window {
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

#[cfg(test)]
mod tests {
    use super::{
        BeamDebugFilter, map_char_to_amiga_key, map_printable_physical_key,
        parse_beam_debug_filter_arg, parse_chipset_arg,
    };
    use machine_amiga::AmigaChipset;
    use winit::keyboard::KeyCode;

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
    fn chipset_arg_parser_accepts_ocs_and_ecs_case_insensitively() {
        assert_eq!(parse_chipset_arg("ocs"), Ok(AmigaChipset::Ocs));
        assert_eq!(parse_chipset_arg("ECS"), Ok(AmigaChipset::Ecs));
    }

    #[test]
    fn chipset_arg_parser_rejects_invalid_values() {
        assert!(parse_chipset_arg("aga").is_err());
    }

    #[test]
    fn beam_debug_filter_parser_accepts_group_aliases_and_individuals() {
        assert_eq!(
            parse_beam_debug_filter_arg("sync,visible"),
            Ok(BeamDebugFilter {
                hsync: true,
                vsync: true,
                hblank: false,
                vblank: false,
                visible: true,
                hsync_pin: false,
                vsync_pin: false,
                csync_pin: false,
                blank_pin: false,
            })
        );
        assert_eq!(
            parse_beam_debug_filter_arg("hblank,vblank"),
            Ok(BeamDebugFilter {
                hsync: false,
                vsync: false,
                hblank: true,
                vblank: true,
                visible: false,
                hsync_pin: false,
                vsync_pin: false,
                csync_pin: false,
                blank_pin: false,
            })
        );
    }

    #[test]
    fn beam_debug_filter_parser_accepts_all_and_rejects_invalid_tokens() {
        assert_eq!(
            parse_beam_debug_filter_arg("all"),
            Ok(BeamDebugFilter::all())
        );
        assert!(parse_beam_debug_filter_arg("sync,foo").is_err());
        assert!(parse_beam_debug_filter_arg("sync,").is_err());
    }

    #[test]
    fn beam_debug_filter_parser_accepts_pin_group_and_pin_tokens() {
        assert_eq!(
            parse_beam_debug_filter_arg("pins"),
            Ok(BeamDebugFilter {
                hsync: false,
                vsync: false,
                hblank: false,
                vblank: false,
                visible: false,
                hsync_pin: true,
                vsync_pin: true,
                csync_pin: true,
                blank_pin: true,
            })
        );
        assert_eq!(
            parse_beam_debug_filter_arg("pin-hsync,csync-pin"),
            Ok(BeamDebugFilter {
                hsync: false,
                vsync: false,
                hblank: false,
                vblank: false,
                visible: false,
                hsync_pin: true,
                vsync_pin: false,
                csync_pin: true,
                blank_pin: false,
            })
        );
    }
}

fn main() {
    let cli = parse_args();

    if cli.headless {
        run_headless(&cli);
        return;
    }

    let amiga = make_amiga(&cli);
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
    let mut app = App::new(amiga, audio);

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
