//! Headless NES test ROM runner.
//!
//! Runs NES test ROMs and reports pass/fail status.
//! Supports two protocols:
//! 1. Modern blargg protocol: $6000 status, $6004+ text
//! 2. Screen-based: Parse nametable for result text (older tests)

use emu_core::Machine;
use machine_nes::Nes;
use std::fs;
use std::path::Path;
use std::time::Instant;

/// Test result status codes.
mod status {
    pub const RUNNING: u8 = 0x80;
    #[allow(dead_code)]
    pub const NEEDS_RESET: u8 = 0x81;
    pub const PASSED: u8 = 0x01;
}

/// Result of running a test ROM.
#[derive(Debug)]
struct TestResult {
    name: String,
    passed: bool,
    code: u8,
    message: String,
    #[allow(dead_code)]
    frames: u32,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let verbose = args.iter().any(|a| a == "-v" || a == "--verbose");
    let paths: Vec<&str> = args[1..].iter()
        .filter(|a| !a.starts_with('-'))
        .map(|s| s.as_str())
        .collect();

    if paths.is_empty() {
        eprintln!("Usage: nes-test-runner [-v] <rom.nes> [rom2.nes ...]");
        eprintln!("       nes-test-runner test-roms/blargg_ppu_tests_2005.09.15b/*.nes");
        eprintln!("       -v, --verbose  Show full screen output");
        std::process::exit(1);
    }

    let mut total_passed = 0;
    let mut total_failed = 0;

    for path in paths {
        match run_test(path, verbose) {
            Ok(result) => {
                if result.passed {
                    total_passed += 1;
                    println!("[PASS] {} - ${:02X} ({})", result.name, result.code, result.message);
                } else {
                    total_failed += 1;
                    println!("[FAIL] {} - ${:02X} ({})", result.name, result.code, result.message);
                }
            }
            Err(e) => {
                total_failed += 1;
                println!("[ERROR] {} - {}", path, e);
            }
        }
    }

    println!();
    println!("Summary: {} passed, {} failed", total_passed, total_failed);

    if total_failed > 0 {
        std::process::exit(1);
    }
}

fn run_test(path: &str, verbose: bool) -> Result<TestResult, String> {
    let path = Path::new(path);
    let name = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Load ROM
    let data = fs::read(path).map_err(|e| format!("Failed to read: {}", e))?;

    let mut nes = Nes::new();
    nes.load_file(path.to_str().unwrap_or(""), &data)?;

    // Run until test completes or timeout
    let start = Instant::now();
    let max_frames = 600; // ~10 seconds at 60fps
    let mut frames = 0;
    let mut last_screen_text = String::new();
    let mut stable_frames = 0;

    loop {
        nes.run_frame();
        frames += 1;

        // Check test status at $6000 (modern protocol)
        let status = nes.memory.peek(0x6000);

        if status != status::RUNNING && status != 0x00 {
            // Modern protocol: test completed
            let message = read_test_message(&nes);
            let passed = status == status::PASSED;

            if verbose {
                println!("\n--- {} (modern protocol) ---", name);
                println!("Status: ${:02X}", status);
                println!("Message: {}", message);
                println!("Screen:\n{}", read_screen_text(&nes));
            }

            return Ok(TestResult {
                name,
                passed,
                code: status,
                message,
                frames,
            });
        }

        // For older tests: read screen text and look for result
        if frames >= 60 && frames % 30 == 0 {
            let screen_text = read_screen_text(&nes);

            // Look for result code pattern like "$01" or "$03"
            if let Some(code) = extract_result_code(&screen_text) {
                // Screen has a result - wait for it to stabilize
                if screen_text == last_screen_text {
                    stable_frames += 30;
                    if stable_frames >= 60 {
                        // Result has been stable for ~1 second
                        let passed = code == 0x01;

                        if verbose {
                            println!("\n--- {} (screen protocol) ---", name);
                            println!("Code: ${:02X}", code);
                            println!("Screen:\n{}", screen_text);
                        }

                        return Ok(TestResult {
                            name,
                            passed,
                            code,
                            message: screen_text.lines().next().unwrap_or("").to_string(),
                            frames,
                        });
                    }
                } else {
                    stable_frames = 0;
                    last_screen_text = screen_text;
                }
            }
        }

        if frames >= max_frames {
            // Timeout - report what we found
            let screen_text = read_screen_text(&nes);
            let code = extract_result_code(&screen_text).unwrap_or(0);

            if verbose {
                println!("\n--- {} (timeout) ---", name);
                println!("$6000: ${:02X}", nes.memory.peek(0x6000));
                println!("Screen:\n{}", screen_text);
            }

            let message = if screen_text.is_empty() {
                "No output detected".to_string()
            } else {
                format!("Screen: {}", screen_text.lines().next().unwrap_or(""))
            };

            return Ok(TestResult {
                name,
                passed: code == 0x01,
                code,
                message,
                frames,
            });
        }

        // Safety timeout
        if start.elapsed().as_secs() > 30 {
            return Err("Hard timeout after 30 seconds".to_string());
        }
    }
}

/// Read the null-terminated test message from $6004+.
fn read_test_message(nes: &Nes) -> String {
    let mut message = String::new();
    let mut addr = 0x6004u16;

    for _ in 0..256 {
        let byte = nes.memory.peek(addr);
        if byte == 0 {
            break;
        }
        if byte >= 0x20 && byte < 0x7F {
            message.push(byte as char);
        }
        addr = addr.wrapping_add(1);
    }

    if message.is_empty() {
        "No message".to_string()
    } else {
        message.trim().to_string()
    }
}

/// Read text from the screen by parsing the nametable.
/// Blargg tests use a simple ASCII-like font.
fn read_screen_text(nes: &Nes) -> String {
    let mut text = String::new();

    // Read first nametable at $2000 (960 tiles = 30 rows x 32 cols)
    // Skip first few rows which are usually blank
    for row in 2..28 {
        let mut line = String::new();
        for col in 0..32 {
            let addr = 0x2000 + row * 32 + col;
            let tile = nes.memory.ppu_read(addr);

            // Convert tile to ASCII (blargg font is roughly ASCII-mapped)
            let ch = tile_to_char(tile);
            line.push(ch);
        }

        // Only add non-empty lines
        let trimmed = line.trim_end();
        if !trimmed.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(trimmed);
        }
    }

    text
}

/// Convert a tile index to ASCII character.
/// Blargg tests use a custom font where tile numbers map to ASCII.
fn tile_to_char(tile: u8) -> char {
    // Blargg tests typically use tile indices that directly correspond to ASCII
    // Tile 0 = space, and printable ASCII starts at 0x20
    if tile == 0 {
        ' '
    } else if tile >= 0x20 && tile < 0x7F {
        tile as char
    } else {
        ' '
    }
}

/// Extract result code from screen text (e.g., "$01" -> 0x01).
fn extract_result_code(text: &str) -> Option<u8> {
    let text_lower = text.to_lowercase();

    // Check for "Passed" on screen (some tests use this instead of $01)
    if text_lower.contains("passed") {
        return Some(0x01);
    }

    // Check for "Failed" on screen
    if text_lower.contains("failed") {
        // Try to extract the failure number
        for line in text.lines() {
            if let Some(pos) = line.find('#') {
                let rest = &line[pos + 1..];
                if let Some(num_str) = rest.split_whitespace().next() {
                    if let Ok(code) = num_str.parse::<u8>() {
                        return Some(code);
                    }
                }
            }
        }
        return Some(0xFF); // Generic failure
    }

    // Look for pattern like "$01" or "$03"
    for line in text.lines() {
        if let Some(pos) = line.find('$') {
            let rest = &line[pos + 1..];
            if rest.len() >= 2 {
                let hex = &rest[..2];
                if let Ok(code) = u8::from_str_radix(hex, 16) {
                    return Some(code);
                }
            }
        }
    }
    None
}
