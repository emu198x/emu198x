//! Diagnostic harness that runs `blargg_nes_cpu_test5/{cpu,official}.nes`
//! and dumps the nametable + `$00FF` sentinel so we can see which
//! sub-test is currently failing.
//!
//! Not part of the normal sweep — `#[ignore]`d and used on-demand
//! while chasing advancing CRC failures.
//!
//! Run with:
//! ```sh
//! cargo test --release -p machine-nintendo-nes --test cpu_test5_probe \
//!     -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::Nes;

fn nes_test_roms_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let d = PathBuf::from(home).join("Projects/198x/assets/test-suites/nes-test-roms");
    d.is_dir().then_some(d)
}

fn render_nametable(nt: &[u8]) -> String {
    // The blargg shell uses tile codes that map 1:1 to ASCII for
    // printable characters. Decode the first nametable (32×30 tiles
    // = 960 bytes) into rows of text. `0x00` is the framework's
    // "passed" marker tile — rendered as `[OK]` so it surfaces in
    // the trace.
    let mut out = String::new();
    for row in 0..30 {
        let start = row * 32;
        let end = start + 32;
        let raw = &nt[start..end];
        // Drop trailing whitespace bytes (0x20 spaces) so the
        // [OK] marker shows up at end-of-line instead of buried
        // under padding.
        let last_meaningful = raw
            .iter()
            .rposition(|&b| b != 0x20)
            .map(|i| i + 1)
            .unwrap_or(0);
        if last_meaningful == 0 {
            continue;
        }
        let mut line = String::new();
        for &b in &raw[..last_meaningful] {
            match b {
                0x00 => line.push_str("[OK]"),
                0x20..=0x7E => line.push(b as char),
                _ => line.push('?'),
            }
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn run_one(rom_path: &PathBuf) {
    let bytes = std::fs::read(rom_path).expect("rom should read");
    let parsed = parse_ines(&bytes).expect("ines parse");
    let mut nes = Nes::new(parsed.mapper);

    // 100M ticks is enough for blargg_nes_cpu_test5/cpu.nes to
    // finish (sweep shows it stalls at ~88M ticks).
    let max = 100_000_000u64;
    while nes.master_clock() < max {
        nes.tick();
    }

    let nt = nes.ppu.nametable_ram();
    let text = render_nametable(nt);
    let sentinel = nes.peek(0x00FF);
    let result_byte = nes.peek(0x6000);

    println!("=== {} ===", rom_path.display());
    println!("ticks  : {}", nes.master_clock());
    println!("$6000  : {result_byte:02X}");
    println!("$00FF  : {sentinel:02X}");
    println!("nametable text:");
    for line in text.lines() {
        println!("  {line}");
    }
    println!();
}

#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn probe_cpu_nes() {
    let Some(root) = nes_test_roms_root() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    run_one(&root.join("blargg_nes_cpu_test5/cpu.nes"));
}

#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn probe_official_nes() {
    let Some(root) = nes_test_roms_root() else {
        eprintln!("nes-test-roms not found; skipping");
        return;
    };
    run_one(&root.join("blargg_nes_cpu_test5/official.nes"));
}
