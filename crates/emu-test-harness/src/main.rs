//! Bulk ROM testing harness for Emu198x.
//!
//! Scans a directory tree of TOSEC/No-Intro/GoodTools ROM sets, identifies
//! each ROM's target system by extension and header inspection, runs it
//! headlessly for a configurable number of frames, captures a screenshot,
//! and produces a JSON report with per-ROM status, timing, and hashes.
//!
//! Usage:
//!   emu-test-harness <rom-dir> [OPTIONS]
//!
//! Options:
//!   --frames <n>        Frames to run per ROM [default: 300]
//!   --output <file>     Report output file [default: test-report.json]
//!   --screenshots <dir> Screenshot output directory [default: screenshots/]
//!   --parallel <n>      Number of parallel workers [default: CPU count]
//!   --system <name>     Only test a specific system (spectrum, nes, c64, etc.)
//!   --roms-dir <dir>    Directory containing system ROMs (BIOS files)

use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{Duration, Instant};

use emu_core::Machine;
use rayon::prelude::*;
use serde::Serialize;
use sha1::{Digest, Sha1};

// ---------------------------------------------------------------------------
// ROM identification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum System {
    Spectrum,
    C64,
    Nes,
    Amiga,
    Atari2600,
    Sg1000,
    ColecoVision,
    Msx,
    Sms,
    GameGear,
    BbcMicro,
}

impl System {
    fn name(self) -> &'static str {
        match self {
            Self::Spectrum => "spectrum",
            Self::C64 => "c64",
            Self::Nes => "nes",
            Self::Amiga => "amiga",
            Self::Atari2600 => "atari2600",
            Self::Sg1000 => "sg1000",
            Self::ColecoVision => "colecovision",
            Self::Msx => "msx",
            Self::Sms => "sms",
            Self::GameGear => "gamegear",
            Self::BbcMicro => "bbc",
        }
    }
}

fn identify_system(path: &Path, data: &[u8]) -> Option<System> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        // Spectrum
        "z80" | "sna" | "tap" | "tzx" => Some(System::Spectrum),
        // NES
        "nes" if data.len() >= 4 && &data[0..4] == b"NES\x1a" => Some(System::Nes),
        "nes" => Some(System::Nes),
        // C64
        "prg" | "d64" | "t64" | "crt" => Some(System::C64),
        // Amiga
        "adf" if data.len() == 901_120 => Some(System::Amiga),
        // Atari 2600
        "a26" => Some(System::Atari2600),
        "bin" if data.len() <= 32768 && is_likely_2600(data) => Some(System::Atari2600),
        // SG-1000
        "sg" | "sc" => Some(System::Sg1000),
        // ColecoVision
        "col" => Some(System::ColecoVision),
        // MSX
        "rom" if data.len() >= 2 && data[0] == 0x41 && data[1] == 0x42 => Some(System::Msx),
        // SMS / Game Gear
        "sms" => Some(System::Sms),
        "gg" => Some(System::GameGear),
        // BBC Micro (SSD/DSD disk images)
        "ssd" | "dsd" => Some(System::BbcMicro),
        // Ambiguous .bin — try to identify by size/content
        "bin" if data.len() >= 16384 && data.len() <= 49152 && looks_like_sg1000(data) => {
            Some(System::Sg1000)
        }
        "bin" if data.len() >= 16384 && data.len() <= 524288 && looks_like_sms(data) => {
            Some(System::Sms)
        }
        _ => None,
    }
}

fn is_likely_2600(data: &[u8]) -> bool {
    // Atari 2600 ROMs are 2K, 4K, 8K, 16K, or 32K
    matches!(data.len(), 2048 | 4096 | 8192 | 16384 | 32768)
}

fn looks_like_sg1000(data: &[u8]) -> bool {
    // SG-1000 ROMs are 8-48K, start with typical Z80 instructions
    data.len() <= 49152 && (data[0] == 0xF3 || data[0] == 0xC3 || data[0] == 0x31)
}

fn looks_like_sms(data: &[u8]) -> bool {
    // SMS ROMs have "TMR SEGA" at $7FF0
    if data.len() >= 0x8000 {
        let header = &data[0x7FF0..0x7FF8];
        header == b"TMR SEGA"
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Test execution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum TestStatus {
    Passed,
    Failed,
    Skipped,
    Error,
}

#[derive(Debug, Clone, Serialize)]
struct TestResult {
    path: String,
    system: Option<System>,
    sha1: String,
    file_size: usize,
    status: TestStatus,
    message: String,
    frames_run: u64,
    elapsed_ms: u64,
    screenshot_path: Option<String>,
    /// Whether the framebuffer had any visible content (not all one color).
    has_display_output: bool,
    /// Number of unique colors in the final framebuffer.
    unique_colors: usize,
}

struct TestConfig {
    frames: u32,
    screenshots_dir: PathBuf,
    roms_dir: Option<PathBuf>,
    system_filter: Option<String>,
}

fn run_test(path: &Path, config: &TestConfig) -> TestResult {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            return TestResult {
                path: path.display().to_string(),
                system: None,
                sha1: String::new(),
                file_size: 0,
                status: TestStatus::Error,
                message: format!("Read error: {e}"),
                frames_run: 0,
                elapsed_ms: 0,
                screenshot_path: None,
                has_display_output: false,
                unique_colors: 0,
            };
        }
    };

    let sha1 = hex_sha1(&data);
    let file_size = data.len();
    let system = identify_system(path, &data);

    // Filter by system if requested
    if let Some(ref filter) = config.system_filter {
        if let Some(sys) = system {
            if sys.name() != filter {
                return TestResult {
                    path: path.display().to_string(),
                    system,
                    sha1,
                    file_size,
                    status: TestStatus::Skipped,
                    message: "Filtered out".to_string(),
                    frames_run: 0,
                    elapsed_ms: 0,
                    screenshot_path: None,
                    has_display_output: false,
                    unique_colors: 0,
                };
            }
        }
    }

    let Some(system) = system else {
        return TestResult {
            path: path.display().to_string(),
            system: None,
            sha1,
            file_size,
            status: TestStatus::Skipped,
            message: "Unrecognised format".to_string(),
            frames_run: 0,
            elapsed_ms: 0,
            screenshot_path: None,
            has_display_output: false,
            unique_colors: 0,
        };
    };

    // Try to create the machine
    let machine_result = create_machine(system, &data, config);
    let mut machine: Box<dyn Machine> = match machine_result {
        Ok(m) => m,
        Err(msg) => {
            return TestResult {
                path: path.display().to_string(),
                system: Some(system),
                sha1,
                file_size,
                status: TestStatus::Error,
                message: msg,
                frames_run: 0,
                elapsed_ms: 0,
                screenshot_path: None,
                has_display_output: false,
                unique_colors: 0,
            };
        }
    };

    // Run frames
    let start = Instant::now();
    for _ in 0..config.frames {
        machine.run_frame();
        let _ = machine.take_audio_buffer();
    }
    let elapsed = start.elapsed();

    // Analyse framebuffer
    let fb = machine.framebuffer();
    let unique_colors = count_unique_colors(fb);
    let has_display_output = unique_colors > 1;

    // Save screenshot
    let screenshot_path = save_test_screenshot(
        fb,
        machine.framebuffer_width(),
        machine.framebuffer_height(),
        &sha1,
        system,
        &config.screenshots_dir,
    );

    let status = if has_display_output {
        TestStatus::Passed
    } else {
        TestStatus::Failed
    };

    let message = if has_display_output {
        format!("{unique_colors} unique colors after {} frames", config.frames)
    } else {
        format!("Blank display after {} frames", config.frames)
    };

    TestResult {
        path: path.display().to_string(),
        system: Some(system),
        sha1,
        file_size,
        status,
        message,
        frames_run: config.frames as u64,
        elapsed_ms: elapsed.as_millis() as u64,
        screenshot_path,
        has_display_output,
        unique_colors,
    }
}

fn create_machine(
    system: System,
    data: &[u8],
    config: &TestConfig,
) -> Result<Box<dyn Machine>, String> {
    match system {
        System::Spectrum => {
            // Try loading as a snapshot first, otherwise as tape
            let ext = "z80"; // Default — actual detection would use the file extension
            let rom_48k: &[u8] = include_bytes!("../../../roms/48.rom");
            let cfg = emu_spectrum::SpectrumConfig {
                model: emu_spectrum::SpectrumModel::Spectrum48K,
                rom: rom_48k.to_vec(),
            };
            let mut spec = emu_spectrum::Spectrum::new(&cfg);
            // Try to load as Z80 snapshot
            if data.len() > 30 && emu_spectrum::load_z80(&mut spec, data).is_ok() {
                return Ok(Box::new(spec));
            }
            // Try as SNA
            if emu_spectrum::load_sna(&mut spec, data).is_ok() {
                return Ok(Box::new(spec));
            }
            // Try as TAP
            if let Ok(tap) = emu_spectrum::TapFile::parse(data) {
                spec.insert_tap(tap);
                return Ok(Box::new(spec));
            }
            // Try as TZX
            if let Ok(tzx) = emu_spectrum::TzxFile::parse(data) {
                spec.insert_tzx(tzx);
                return Ok(Box::new(spec));
            }
            Ok(Box::new(spec))
        }
        System::Nes => {
            let cfg = emu_nes::NesConfig {
                rom_data: data.to_vec(),
                region: emu_nes::NesRegion::Ntsc,
            };
            emu_nes::Nes::new(&cfg)
                .map(|nes| Box::new(nes) as Box<dyn Machine>)
                .map_err(|e| format!("NES load error: {e}"))
        }
        System::Sg1000 => {
            Ok(Box::new(emu_sg1000::Sg1000::new(
                data.to_vec(),
                emu_sg1000::Sg1000Region::Ntsc,
            )))
        }
        System::Sms => {
            Ok(Box::new(emu_sms::Sms::new(
                data.to_vec(),
                emu_sms::SmsVariant::SmsNtsc,
            )))
        }
        System::GameGear => {
            Ok(Box::new(emu_sms::Sms::new(
                data.to_vec(),
                emu_sms::SmsVariant::GameGear,
            )))
        }
        System::Atari2600 => {
            let cfg = emu_atari_2600::Atari2600Config {
                rom_data: data.to_vec(),
                region: emu_atari_2600::Atari2600Region::Ntsc,
            };
            emu_atari_2600::Atari2600::new(&cfg)
                .map(|sys| Box::new(sys) as Box<dyn Machine>)
                .map_err(|e| format!("Atari 2600 load error: {e}"))
        }
        System::ColecoVision => {
            // Needs BIOS
            let bios_path = config
                .roms_dir
                .as_ref()
                .map(|d| d.join("coleco.rom"))
                .unwrap_or_else(|| PathBuf::from("roms/coleco.rom"));
            let bios = std::fs::read(&bios_path)
                .map_err(|e| format!("ColecoVision BIOS not found at {}: {e}", bios_path.display()))?;
            Ok(Box::new(emu_colecovision::ColecoVision::new(
                bios,
                data.to_vec(),
                emu_colecovision::CvRegion::Ntsc,
            )))
        }
        System::Msx => {
            // Needs BIOS
            let bios_path = config
                .roms_dir
                .as_ref()
                .map(|d| d.join("msx.rom"))
                .unwrap_or_else(|| PathBuf::from("roms/msx.rom"));
            let bios = std::fs::read(&bios_path)
                .map_err(|e| format!("MSX BIOS not found at {}: {e}", bios_path.display()))?;
            let mut msx = emu_msx::Msx::new(bios, emu_msx::MsxRegion::Ntsc);
            msx.insert_cart1(data.to_vec(), emu_msx::MapperType::Plain);
            Ok(Box::new(msx))
        }
        System::BbcMicro => {
            let mos_path = config
                .roms_dir
                .as_ref()
                .map(|d| d.join("bbc-mos.rom"))
                .unwrap_or_else(|| PathBuf::from("roms/bbc-mos.rom"));
            let mos = std::fs::read(&mos_path)
                .map_err(|e| format!("BBC MOS ROM not found at {}: {e}", mos_path.display()))?;
            Ok(Box::new(emu_bbc_micro::BbcMicro::new(mos)))
        }
        System::C64 => {
            Err("C64 ROM testing requires kernal/basic/chargen ROMs — not yet integrated".to_string())
        }
        System::Amiga => {
            Err("Amiga testing requires Kickstart ROM — not yet integrated".to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn hex_sha1(data: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    let result = hasher.finalize();
    result.iter().map(|b| format!("{b:02x}")).collect()
}

fn count_unique_colors(fb: &[u32]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for &pixel in fb {
        seen.insert(pixel);
        if seen.len() > 256 {
            return seen.len(); // Early exit — clearly has content
        }
    }
    seen.len()
}

fn save_test_screenshot(
    fb: &[u32],
    width: u32,
    height: u32,
    sha1: &str,
    system: System,
    output_dir: &Path,
) -> Option<String> {
    let dir = output_dir.join(system.name());
    std::fs::create_dir_all(&dir).ok()?;

    let filename = format!("{sha1}.png");
    let path = dir.join(&filename);

    let file = std::fs::File::create(&path).ok()?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = encoder.write_header().ok()?;

    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for &argb in fb {
        rgba.push(((argb >> 16) & 0xFF) as u8);
        rgba.push(((argb >> 8) & 0xFF) as u8);
        rgba.push((argb & 0xFF) as u8);
        rgba.push(0xFF);
    }
    png_writer.write_image_data(&rgba).ok()?;

    Some(path.display().to_string())
}

// ---------------------------------------------------------------------------
// Directory scanning
// ---------------------------------------------------------------------------

fn scan_roms(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    scan_roms_recursive(dir, &mut files);
    files.sort();
    files
}

fn scan_roms_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_roms_recursive(&path, files);
        } else if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .unwrap_or_default();

            let known = matches!(
                ext.as_str(),
                "z80" | "sna" | "tap" | "tzx"
                    | "nes"
                    | "prg" | "d64" | "t64" | "crt"
                    | "adf"
                    | "a26"
                    | "sg" | "sc"
                    | "col"
                    | "rom"
                    | "sms" | "gg"
                    | "ssd" | "dsd"
                    | "bin"
            );

            if known {
                files.push(path);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CLI and main
// ---------------------------------------------------------------------------

struct CliArgs {
    rom_dir: PathBuf,
    frames: u32,
    output: PathBuf,
    screenshots_dir: PathBuf,
    parallel: usize,
    system_filter: Option<String>,
    roms_dir: Option<PathBuf>,
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut cli = CliArgs {
        rom_dir: PathBuf::new(),
        frames: 300,
        output: PathBuf::from("test-report.json"),
        screenshots_dir: PathBuf::from("screenshots"),
        parallel: num_cpus(),
        system_filter: None,
        roms_dir: None,
    };

    if args.len() < 2 {
        eprintln!("Usage: emu-test-harness <rom-dir> [OPTIONS]");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  --frames <n>        Frames per ROM [default: 300]");
        eprintln!("  --output <file>     Report file [default: test-report.json]");
        eprintln!("  --screenshots <dir> Screenshot dir [default: screenshots/]");
        eprintln!("  --parallel <n>      Workers [default: CPU count]");
        eprintln!("  --system <name>     Filter by system");
        eprintln!("  --roms-dir <dir>    BIOS ROM directory [default: roms/]");
        process::exit(1);
    }

    cli.rom_dir = PathBuf::from(&args[1]);

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--frames" => {
                i += 1;
                cli.frames = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(300);
            }
            "--output" => {
                i += 1;
                if let Some(s) = args.get(i) {
                    cli.output = PathBuf::from(s);
                }
            }
            "--screenshots" => {
                i += 1;
                if let Some(s) = args.get(i) {
                    cli.screenshots_dir = PathBuf::from(s);
                }
            }
            "--parallel" => {
                i += 1;
                cli.parallel = args.get(i).and_then(|s| s.parse().ok()).unwrap_or_else(num_cpus);
            }
            "--system" => {
                i += 1;
                cli.system_filter = args.get(i).cloned();
            }
            "--roms-dir" => {
                i += 1;
                cli.roms_dir = args.get(i).map(PathBuf::from);
            }
            _ => {}
        }
        i += 1;
    }

    cli
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

fn main() {
    let cli = parse_args();

    eprintln!("Scanning {} for ROM files...", cli.rom_dir.display());
    let files = scan_roms(&cli.rom_dir);
    eprintln!("Found {} ROM files", files.len());

    if files.is_empty() {
        eprintln!("No ROM files found.");
        process::exit(0);
    }

    // Create screenshots directory
    std::fs::create_dir_all(&cli.screenshots_dir).ok();

    let config = TestConfig {
        frames: cli.frames,
        screenshots_dir: cli.screenshots_dir.clone(),
        roms_dir: cli.roms_dir.clone(),
        system_filter: cli.system_filter.clone(),
    };

    eprintln!(
        "Running {} frames per ROM with {} workers...",
        cli.frames, cli.parallel
    );

    // Configure rayon thread pool
    rayon::ThreadPoolBuilder::new()
        .num_threads(cli.parallel)
        .build_global()
        .ok();

    let start = Instant::now();

    let results: Vec<TestResult> = files
        .par_iter()
        .enumerate()
        .map(|(i, path)| {
            // Catch panics so one bad ROM doesn't kill the whole run.
            let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_test(path, &config)
            })) {
                Ok(r) => r,
                Err(_) => TestResult {
                    path: path.display().to_string(),
                    system: None,
                    sha1: String::new(),
                    file_size: 0,
                    status: TestStatus::Error,
                    message: "Panic during execution".to_string(),
                    frames_run: 0,
                    elapsed_ms: 0,
                    screenshot_path: None,
                    has_display_output: false,
                    unique_colors: 0,
                },
            };
            let status_char = match result.status {
                TestStatus::Passed => '.',
                TestStatus::Failed => 'F',
                TestStatus::Skipped => 's',
                TestStatus::Error => 'E',
            };
            eprint!("{status_char}");
            if (i + 1) % 80 == 0 {
                eprintln!(" [{}/{}]", i + 1, files.len());
            }
            result
        })
        .collect();

    let elapsed = start.elapsed();
    eprintln!();

    // Summary
    let passed = results.iter().filter(|r| matches!(r.status, TestStatus::Passed)).count();
    let failed = results.iter().filter(|r| matches!(r.status, TestStatus::Failed)).count();
    let skipped = results.iter().filter(|r| matches!(r.status, TestStatus::Skipped)).count();
    let errors = results.iter().filter(|r| matches!(r.status, TestStatus::Error)).count();

    eprintln!();
    eprintln!("Results: {passed} passed, {failed} failed, {skipped} skipped, {errors} errors");
    eprintln!("Total time: {:.1}s", elapsed.as_secs_f64());

    // Per-system breakdown
    let mut systems: std::collections::BTreeMap<String, (usize, usize, usize)> =
        std::collections::BTreeMap::new();
    for r in &results {
        if let Some(sys) = r.system {
            let entry = systems.entry(sys.name().to_string()).or_default();
            match r.status {
                TestStatus::Passed => entry.0 += 1,
                TestStatus::Failed => entry.1 += 1,
                _ => entry.2 += 1,
            }
        }
    }
    if !systems.is_empty() {
        eprintln!();
        eprintln!("Per-system breakdown:");
        for (sys, (p, f, o)) in &systems {
            eprintln!("  {sys:15} {p:5} passed  {f:5} failed  {o:5} other");
        }
    }

    // Write JSON report
    let report = serde_json::to_string_pretty(&results).unwrap_or_else(|e| {
        eprintln!("JSON serialization error: {e}");
        process::exit(1);
    });

    if let Err(e) = std::fs::write(&cli.output, &report) {
        eprintln!("Failed to write report to {}: {e}", cli.output.display());
        process::exit(1);
    }

    eprintln!("Report written to {}", cli.output.display());
    eprintln!(
        "Screenshots in {}",
        cli.screenshots_dir.display()
    );
}
