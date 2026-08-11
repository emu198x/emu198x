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

/// Watch the screen *while* the sub-tests run, printing each new state.
///
/// ⚠ The summary screen the other probes dump is not the whole story.
/// `instr_test_end.a`'s `@wrong` handler prints the failing opcode and
/// its mnemonic the moment a checksum mismatches — but this is a
/// BUILD_MULTI build, so the next sub-test overwrites that text long
/// before the run ends. Sampling during the run is the only way to read
/// what the ROM already told us.
fn watch(rom_path: &PathBuf, until: u64) {
    let bytes = std::fs::read(rom_path).expect("rom should read");
    let parsed = parse_ines(&bytes).expect("ines parse");
    let mut nes = Nes::new(parsed.mapper);

    println!("=== watch {} ===", rom_path.display());
    let mut last = String::new();
    let mut next_sample = 0u64;
    while nes.master_clock() < until {
        nes.tick();
        if nes.master_clock() >= next_sample {
            next_sample += 100_000;
            let text = render_nametable(nes.ppu.nametable_ram());
            if text != last {
                println!("--- t={} ---", nes.master_clock());
                for line in text.lines() {
                    println!("  {line}");
                }
                last = text;
            }
        }
    }
    println!();
}

/// Dump the summary screen as raw nametable bytes, in the same
/// `NT <row> <32 hex bytes>` form as
/// [`tools/mesen-nes-cross-check/nametable-dump.lua`], so the two can be
/// diffed directly instead of eyeballed through a renderer.
///
/// ⚠ The renderer is exactly what made this screen confusing: the pass
/// markers sit at column 31 of the row BELOW each test's name, which
/// reads as an off-by-one until you see Mesen2 produce it too.
#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn probe_cpu_test5_raw_nametable() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    for name in ["official", "cpu"] {
        let rom = root.join(format!("blargg_nes_cpu_test5/{name}.nes"));
        let bytes = std::fs::read(&rom).expect("rom should read");
        let parsed = parse_ines(&bytes).expect("ines parse");
        let mut nes = Nes::new(parsed.mapper);
        while nes.master_clock() < 100_000_000 {
            nes.tick();
        }
        let nt = nes.ppu.nametable_ram();
        for row in 0..30 {
            let hex: String = nt[row * 32..row * 32 + 32]
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect();
            println!("{name} NT {row:02} {hex}");
        }
        println!("{name} $00FF {:02X}", nes.peek(0x00FF));
    }
}

/// `01-implied`'s expected checksums, in source order, straight from
/// `blargg_nes_cpu_test5/source/01-implied.a`. Entry N's CRC is the Nth
/// `.dword`. The official build stops after `$EA NOP`; the six trailing
/// unofficial NOPs share NOP's checksum.
const IMPLIED_ENTRIES: &[(u8, &str, u32)] = &[
    (0x2A, "ROL A", 0x013A_2933),
    (0x0A, "ASL A", 0xA387_33B0),
    (0x6A, "ROR A", 0x6EC2_BCA6),
    (0x4A, "LSR A", 0x763F_EBC5),
    (0x8A, "TXA", 0x0FF1_C1E6),
    (0x98, "TYA", 0x5B2E_B5B7),
    (0xAA, "TAX", 0x1D8A_CEF5),
    (0xA8, "TAY", 0x83DC_03F9),
    (0xE8, "INX", 0x8EBD_F63B),
    (0xC8, "INY", 0xF34C_AA18),
    (0xCA, "DEX", 0x9123_FF08),
    (0x88, "DEY", 0x4889_7445),
    (0x38, "SEC", 0x4BE1_4840),
    (0x18, "CLC", 0xE7C7_ECC0),
    (0xF8, "SED", 0x408E_F097),
    (0xD8, "CLD", 0xA6AE_F749),
    (0x78, "SEI", 0x8F06_AD7B),
    (0x58, "CLI", 0xFC96_AE14),
    (0xB8, "CLV", 0x28F1_0ADA),
    (0xEA, "NOP", 0xCA7E_6620),
];

/// Which of `01-implied`'s expected checksums our CPU ever produces.
///
/// The framework keeps a running CRC in a 4-byte zero-page variable and
/// compares it against `correct_checksums` after each opcode. A passing
/// opcode therefore makes that variable hold the expected value, however
/// briefly. Sampling all of zero page and looking for each expected
/// dword sidesteps having to locate the variable at all — and an
/// expected value that never appears names a failing opcode.
///
/// ⚠ Byte order is not assumed. `check_result` walks `checksum,x` with
/// x from 3 down to 0 against `correct_checksums,y` ascending, so the
/// two are stored in opposite orders; both are searched.
#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn probe_implied_checksums() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    let rom = root.join("blargg_nes_cpu_test5/official.nes");
    let bytes = std::fs::read(&rom).expect("rom should read");
    let parsed = parse_ines(&bytes).expect("ines parse");
    let mut nes = Nes::new(parsed.mapper);

    // Test 01 finishes well before 6M ticks (02-immediate starts at
    // ~4.8M), so this covers it with room to spare.
    let mut seen: Vec<Option<u64>> = vec![None; IMPLIED_ENTRIES.len()];
    let mut sample_at = 0u64;
    while nes.master_clock() < 6_000_000 {
        nes.tick();
        if nes.master_clock() < sample_at {
            continue;
        }
        sample_at = nes.master_clock() + 500;
        let zp: Vec<u8> = (0u16..=0xFF).map(|a| nes.peek(a)).collect();
        for (i, (_, _, crc)) in IMPLIED_ENTRIES.iter().enumerate() {
            if seen[i].is_some() {
                continue;
            }
            let be = crc.to_be_bytes();
            let le = crc.to_le_bytes();
            if zp.windows(4).any(|w| w == be || w == le) {
                seen[i] = Some(nes.master_clock());
            }
        }
    }

    println!("=== 01-implied expected checksums observed in zero page ===");
    for (i, (op, name, crc)) in IMPLIED_ENTRIES.iter().enumerate() {
        match seen[i] {
            Some(t) => println!("  ok      {op:02X} {name:<6} {crc:08X}  (t={t})"),
            None => println!("  MISSING {op:02X} {name:<6} {crc:08X}"),
        }
    }
}

#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn probe_official_screen_during_run() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    // Test 01 runs first, so the failing-opcode print lands early.
    watch(&root.join("blargg_nes_cpu_test5/official.nes"), 30_000_000);
}

#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn probe_cpu_nes() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    run_one(&root.join("blargg_nes_cpu_test5/cpu.nes"));
}

#[test]
#[ignore = "diagnostic; requires local nes-test-roms"]
fn probe_official_nes() {
    let Some(root) = nes_test_roms_root() else {
        emu198x_test_skip::skip!("nes-test-roms corpus not staged (test-suites/nes-test-roms)");
    };
    run_one(&root.join("blargg_nes_cpu_test5/official.nes"));
}
