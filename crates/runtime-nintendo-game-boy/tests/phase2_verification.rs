//! DMG Phase-2 verification harness.
//!
//! Mirrors the ordered acceptance gate list in
//! `knowledge/systems/nintendo-game-boy/overview.md`:
//!
//! 1. Blargg `cpu_instrs` (all 11 sub-tests)
//! 2. Blargg `instr_timing`
//! 3. Blargg `mem_timing` v1 + v2
//! 4. mooneye-gb acceptance gate set
//! 5. `dmg-acid2.gb`
//! 6. Blargg `oam_bug` (aggregate and eight singles)
//! 7. Blargg `interrupt_time` (normal-speed check and full ROM verdict)
//!
//! These tests are ignored by default because they depend on local
//! ROM corpora outside the repository. They are intended to be the
//! standard DMG verification harness once the core is ready.

use std::path::{Path, PathBuf};

use common_nintendo_game_boy::MemoryBus;
use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink, StopReason,
};
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};

const MAX_SERIAL_TEST_FRAMES: u32 = 1_200;
// The successful reference OAM aggregate exceeds 1,200 frames including boot.
const MAX_OAM_TEST_FRAMES: u32 = 3_600;
const OAM_BUG_TESTS: &[&str] = &[
    "oam_bug/oam_bug.gb",
    "oam_bug/rom_singles/1-lcd_sync.gb",
    "oam_bug/rom_singles/2-causes.gb",
    "oam_bug/rom_singles/3-non_causes.gb",
    "oam_bug/rom_singles/4-scanline_timing.gb",
    "oam_bug/rom_singles/5-timing_bug.gb",
    "oam_bug/rom_singles/6-timing_no_bug.gb",
    "oam_bug/rom_singles/7-timing_effect.gb",
    "oam_bug/rom_singles/8-instr_effect.gb",
];
const MOONEYE_SWEEP_FRAMES: u32 = 1_200;
const DMG_ACID2_FRAMES: u32 = 180;

const CPU_INSTRS_SUBTESTS: &[&str] = &[
    "cpu_instrs/individual/01-special.gb",
    "cpu_instrs/individual/02-interrupts.gb",
    "cpu_instrs/individual/03-op sp,hl.gb",
    "cpu_instrs/individual/04-op r,imm.gb",
    "cpu_instrs/individual/05-op rp.gb",
    "cpu_instrs/individual/06-ld r,r.gb",
    "cpu_instrs/individual/07-jr,jp,call,ret,rst.gb",
    "cpu_instrs/individual/08-misc instrs.gb",
    "cpu_instrs/individual/09-op r,r.gb",
    "cpu_instrs/individual/10-bit ops.gb",
    "cpu_instrs/individual/11-op a,(hl).gb",
];

const MOONEYE_GATE_SET: &[&str] = &[
    "acceptance/ei_sequence.gb",
    "acceptance/ei_timing.gb",
    "acceptance/halt_ime0_ei.gb",
    "acceptance/halt_ime0_nointr_timing.gb",
    "acceptance/halt_ime1_timing.gb",
    "acceptance/interrupts/ie_push.gb",
    "acceptance/intr_timing.gb",
    "acceptance/timer/tima_reload.gb",
    "acceptance/timer/tima_write_reloading.gb",
];
const MOONEYE_PASS_BYTES: &[u8] = &[3, 5, 8, 13, 21, 34];
const MOONEYE_FAIL_BYTES: &[u8] = &[0x42, 0x42, 0x42, 0x42, 0x42, 0x42];

#[derive(Debug)]
struct BlarggRunResult {
    output: String,
    frames: u32,
    status_code: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MooneyeVerdict {
    Passed,
    Failed,
}

#[derive(Debug)]
struct MooneyeRunResult {
    verdict: MooneyeVerdict,
    frames: u32,
    serial: Vec<u8>,
    state: String,
}

#[derive(Default)]
struct SweepCounts {
    passed: usize,
    failed: usize,
    timed_out: usize,
    load_errors: usize,
}

fn load_runtime(rom_path: &Path) -> Result<GameBoyRuntime, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(rom_path)?;
    let mut runtime = GameBoyRuntime::blank(model_for_mooneye_rom(rom_path));
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &bytes));
    runtime.load_media(&media)?;
    Ok(runtime)
}

fn model_for_mooneye_rom(rom_path: &Path) -> Model {
    let Some(name) = rom_path.file_name().and_then(|name| name.to_str()) else {
        return Model::Dmg;
    };

    if name.contains("-dmg0") {
        Model::Dmg0
    } else if name.contains("-mgb") {
        Model::Mgb
    } else if name == "boot_div2-S.gb" || name.contains("-sgb2") {
        Model::Sgb2
    } else if name.contains("-sgb") || name.contains("-S.") {
        Model::Sgb
    } else {
        Model::Dmg
    }
}

fn blargg_ram_verdict(runtime: &GameBoyRuntime) -> Option<(u8, String)> {
    let machine = runtime.machine()?;
    parse_blargg_ram(machine.cartridge().ram())
}

fn parse_blargg_ram(ram: &[u8]) -> Option<(u8, String)> {
    if ram.len() < 4 || ram[1..4] != [0xDE, 0xB0, 0x61] {
        return None;
    }

    let status = ram[0];
    let text_bytes = ram[4..]
        .iter()
        .copied()
        .take_while(|&byte| byte != 0)
        .collect::<Vec<_>>();
    let text = String::from_utf8_lossy(&text_bytes).into_owned();
    Some((status, text))
}

fn run_until_blargg_verdict(
    runtime: &mut GameBoyRuntime,
    max_frames: u32,
) -> Result<BlarggRunResult, Box<dyn std::error::Error>> {
    let mut output = String::new();
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;

    for frame in 1..=max_frames {
        let target = MachineTime::new(
            runtime.time().get() + u64::from(common_nintendo_game_boy::MCYCLES_PER_FRAME),
        );
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        let result = runtime.run_until(target, &mut host)?;
        assert_eq!(result.stop_reason, StopReason::ReachedTarget);

        let serial = runtime
            .machine_mut()
            .expect("cartridge should stay loaded")
            .drain_serial();
        output.push_str(&String::from_utf8_lossy(&serial));
        if let Some(result) = blargg_report(&output, blargg_ram_verdict(runtime), frame) {
            return Ok(result);
        }
    }

    Err(format!(
        "no Blargg verdict after {max_frames} frames; serial: {output:?}; RAM: {:?}",
        blargg_ram_verdict(runtime)
    )
    .into())
}

fn mooneye_verdict(serial: &[u8]) -> Option<MooneyeVerdict> {
    if serial
        .windows(MOONEYE_PASS_BYTES.len())
        .any(|window| window == MOONEYE_PASS_BYTES)
    {
        return Some(MooneyeVerdict::Passed);
    }
    if serial
        .windows(MOONEYE_FAIL_BYTES.len())
        .any(|window| window == MOONEYE_FAIL_BYTES)
    {
        return Some(MooneyeVerdict::Failed);
    }
    None
}

fn run_until_mooneye_verdict(
    runtime: &mut GameBoyRuntime,
    max_frames: u32,
) -> Result<MooneyeRunResult, Box<dyn std::error::Error>> {
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    let mut serial_log = Vec::new();

    for frame in 1..=max_frames {
        let target = MachineTime::new(
            runtime.time().get() + u64::from(common_nintendo_game_boy::MCYCLES_PER_FRAME),
        );
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        let result = runtime.run_until(target, &mut host)?;
        assert_eq!(result.stop_reason, StopReason::ReachedTarget);

        let serial = runtime
            .machine_mut()
            .expect("cartridge should stay loaded")
            .drain_serial();
        if !serial.is_empty() {
            serial_log.extend_from_slice(&serial);
            if let Some(verdict) = mooneye_verdict(&serial_log) {
                let state = runtime
                    .machine_mut()
                    .map(|machine| {
                        let pc = machine.cpu_pc();
                        let if_reg = machine.read(0xFF0F);
                        let ie_reg = machine.read(0xFFFF);
                        let hram = (0xFF80..=0xFF90)
                            .map(|addr| machine.read(addr))
                            .collect::<Vec<_>>();
                        let diag_addr = u16::from(hram[0]) | (u16::from(hram[1]) << 8);
                        let diag_read = machine.read(diag_addr);
                        format!(
                            "PC=${pc:04X} IF=${if_reg:02X} IE=${ie_reg:02X} HRAM={hram:02X?} DIAG_READ(${diag_addr:04X})=${diag_read:02X}"
                        )
                    })
                    .unwrap_or_else(|| "no machine loaded".to_string());
                return Ok(MooneyeRunResult {
                    verdict,
                    frames: frame,
                    serial: serial_log,
                    state,
                });
            }
        }
    }

    let state = runtime
        .machine_mut()
        .map(|machine| {
            let pc = machine.cpu_pc();
            let if_reg = machine.read(0xFF0F);
            let ie_reg = machine.read(0xFFFF);
            let hram = (0xFF80..=0xFF90)
                .map(|addr| machine.read(addr))
                .collect::<Vec<_>>();
            let diag_addr = u16::from(hram[0]) | (u16::from(hram[1]) << 8);
            let diag_read = machine.read(diag_addr);
            format!(
                "PC=${pc:04X} IF=${if_reg:02X} IE=${ie_reg:02X} HRAM={hram:02X?} DIAG_READ(${diag_addr:04X})=${diag_read:02X}"
            )
        })
        .unwrap_or_else(|| "no machine loaded".to_string());
    Err(format!(
        "no mooneye verdict after {max_frames} frames ({state}); serial bytes: {:02X?}",
        serial_log
    )
    .into())
}

fn blargg_root() -> Option<PathBuf> {
    let Some(root) = std::env::var_os("EMU198X_GB_BLARGG_ROOT").map(PathBuf::from) else {
        emu198x_test_skip::record("skipping: set EMU198X_GB_BLARGG_ROOT to a gb-test-roms root");
        return None;
    };
    if !root.exists() {
        eprintln!(
            "skipping: Game Boy Blargg ROM root missing at {}",
            root.display()
        );
        return None;
    }
    Some(root)
}

fn dmg_acid2_rom() -> Option<PathBuf> {
    let Some(path) = std::env::var_os("EMU198X_GB_DMG_ACID2_ROM").map(PathBuf::from) else {
        emu198x_test_skip::record("skipping: set EMU198X_GB_DMG_ACID2_ROM to a dmg-acid2.gb path");
        return None;
    };
    if !path.exists() {
        emu198x_test_skip::record(&format!(
            "skipping: dmg-acid2 ROM missing at {}",
            path.display()
        ));
        return None;
    }
    Some(path)
}

fn mooneye_root() -> Option<PathBuf> {
    let Some(root) = std::env::var_os("EMU198X_GB_MOONEYE_ROOT").map(PathBuf::from) else {
        emu198x_test_skip::record(
            "skipping: set EMU198X_GB_MOONEYE_ROOT to a mooneye-gb tests root",
        );
        return None;
    };
    if !root.exists() {
        emu198x_test_skip::record(&format!(
            "skipping: mooneye root missing at {}",
            root.display()
        ));
        return None;
    }
    Some(root)
}

fn collect_gb_files(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut paths = Vec::new();
    if !dir.exists() {
        return Ok(paths);
    }

    let mut entries = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            paths.extend(collect_gb_files(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "gb") {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn rel_path<'a>(root: &'a Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}

fn sweep_mooneye_bucket(
    root: &Path,
    bucket: &str,
) -> Result<SweepCounts, Box<dyn std::error::Error>> {
    let paths = collect_gb_files(&root.join(bucket))?;
    let mut counts = SweepCounts::default();
    eprintln!("mooneye sweep: {bucket} ({} ROMs)", paths.len());

    for path in paths {
        let rel = rel_path(root, &path).display();
        let mut runtime = match load_runtime(&path) {
            Ok(runtime) => runtime,
            Err(err) => {
                counts.load_errors += 1;
                eprintln!("  LOAD-ERR {rel}: {err}");
                continue;
            }
        };

        match run_until_mooneye_verdict(&mut runtime, MOONEYE_SWEEP_FRAMES) {
            Ok(result) if result.verdict == MooneyeVerdict::Passed => {
                counts.passed += 1;
                eprintln!("  PASS     {rel} after {} frames", result.frames);
            }
            Ok(result) => {
                counts.failed += 1;
                eprintln!(
                    "  FAIL     {rel} after {} frames ({}) serial={:02X?}",
                    result.frames, result.state, result.serial
                );
            }
            Err(err) => {
                counts.timed_out += 1;
                eprintln!("  TIMEOUT  {rel}: {err}");
            }
        }
    }

    eprintln!(
        "mooneye sweep summary {bucket}: pass={} fail={} timeout={} load_err={}",
        counts.passed, counts.failed, counts.timed_out, counts.load_errors
    );
    Ok(counts)
}

#[test]
#[ignore = "FIXTURE: needs local Blargg Game Boy ROMs"]
fn blargg_cpu_instrs_passes_all_11_subtests() -> Result<(), Box<dyn std::error::Error>> {
    let Some(root) = blargg_root() else {
        return Ok(());
    };

    for rel in CPU_INSTRS_SUBTESTS {
        let path = root.join(rel);
        let mut runtime = load_runtime(&path)?;
        let result = run_until_blargg_verdict(&mut runtime, MAX_SERIAL_TEST_FRAMES)
            .map_err(|err| format!("{}: {err}", path.display()))?;
        assert!(
            blargg_passed(&result),
            "{} failed after {} frames (status {:?}): {:?}",
            path.display(),
            result.frames,
            result.status_code,
            result.output
        );
    }

    Ok(())
}

#[test]
#[ignore = "FIXTURE: needs local Blargg Game Boy ROMs"]
fn blargg_instr_timing_passes() -> Result<(), Box<dyn std::error::Error>> {
    let Some(root) = blargg_root() else {
        return Ok(());
    };

    let path = root.join("instr_timing/instr_timing.gb");
    let mut runtime = load_runtime(&path)?;
    let result = run_until_blargg_verdict(&mut runtime, MAX_SERIAL_TEST_FRAMES)
        .map_err(|err| format!("{}: {err}", path.display()))?;
    assert!(
        blargg_passed(&result),
        "{} failed after {} frames (status {:?}): {:?}",
        path.display(),
        result.frames,
        result.status_code,
        result.output
    );

    Ok(())
}

#[test]
#[ignore = "FIXTURE: needs local Blargg Game Boy ROMs"]
fn blargg_mem_timing_v1_and_v2_pass() -> Result<(), Box<dyn std::error::Error>> {
    let Some(root) = blargg_root() else {
        return Ok(());
    };

    for rel in ["mem_timing/mem_timing.gb", "mem_timing-2/mem_timing.gb"] {
        let path = root.join(rel);
        let mut runtime = load_runtime(&path)?;
        let result = run_until_blargg_verdict(&mut runtime, MAX_SERIAL_TEST_FRAMES)
            .map_err(|err| format!("{}: {err}", path.display()))?;
        assert!(
            blargg_passed(&result),
            "{} failed after {} frames (status {:?}): {:?}",
            path.display(),
            result.frames,
            result.status_code,
            result.output
        );
    }

    Ok(())
}

#[test]
#[ignore = "FIXTURE: needs local mooneye-gb acceptance ROMs"]
fn mooneye_acceptance_gate_set_passes() -> Result<(), Box<dyn std::error::Error>> {
    let Some(root) = mooneye_root() else {
        return Ok(());
    };

    for rel in MOONEYE_GATE_SET {
        let path = root.join(rel);
        if !path.exists() {
            eprintln!("skipping missing mooneye ROM {}", path.display());
            continue;
        }

        let mut runtime = load_runtime(&path)?;
        let result = run_until_mooneye_verdict(&mut runtime, MAX_SERIAL_TEST_FRAMES)
            .map_err(|err| format!("{}: {err}", path.display()))?;
        assert_eq!(
            result.verdict,
            MooneyeVerdict::Passed,
            "{} failed after {} frames with serial {:02X?} ({})",
            path.display(),
            result.frames,
            result.serial,
            result.state
        );
    }

    Ok(())
}

#[test]
#[ignore = "FIXTURE: needs local dmg-acid2 ROM"]
fn dmg_acid2_renders_non_trivial_frame() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = dmg_acid2_rom() else {
        return Ok(());
    };
    let mut runtime = load_runtime(&path)?;
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;

    for _ in 0..DMG_ACID2_FRAMES {
        let target = MachineTime::new(
            runtime.time().get() + u64::from(common_nintendo_game_boy::MCYCLES_PER_FRAME),
        );
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        let result = runtime.run_until(target, &mut host)?;
        assert_eq!(result.stop_reason, StopReason::ReachedTarget);
    }

    let frame = runtime
        .machine()
        .expect("cartridge should stay loaded")
        .framebuffer();
    let non_zero = frame.iter().filter(|&&pixel| pixel != 0).count();
    let unique_shades = {
        let mut shades = [false; 4];
        for &pixel in frame {
            if let Some(slot) = shades.get_mut(pixel as usize) {
                *slot = true;
            }
        }
        shades.into_iter().filter(|present| *present).count()
    };

    assert!(non_zero > frame.len() / 8, "frame stayed mostly blank");
    assert!(unique_shades >= 3, "expected a non-trivial acid2 frame");

    Ok(())
}

#[test]
#[ignore = "DIAGNOSTIC: reports broad local mooneye coverage without gating CI"]
fn mooneye_broad_acceptance_sweep_reports_baseline() -> Result<(), Box<dyn std::error::Error>> {
    let Some(root) = mooneye_root() else {
        return Ok(());
    };

    let buckets = [
        "acceptance",
        "emulator-only/mbc1",
        "emulator-only/mbc2",
        "emulator-only/mbc5",
    ];
    let mut total = SweepCounts::default();
    for bucket in buckets {
        let counts = sweep_mooneye_bucket(&root, bucket)?;
        total.passed += counts.passed;
        total.failed += counts.failed;
        total.timed_out += counts.timed_out;
        total.load_errors += counts.load_errors;
    }

    eprintln!(
        "mooneye broad sweep total: pass={} fail={} timeout={} load_err={}",
        total.passed, total.failed, total.timed_out, total.load_errors
    );
    Ok(())
}

// A signed RAM report is authoritative, including its pending state. Refresh
// its text on every poll: the first nonempty report is normally just a title.
fn blargg_report(serial: &str, ram: Option<(u8, String)>, frames: u32) -> Option<BlarggRunResult> {
    if let Some((status, output)) = ram {
        return (status < 0x80).then_some(BlarggRunResult {
            output,
            frames,
            status_code: Some(status),
        });
    }
    text_verdict(serial).map(|_| BlarggRunResult {
        output: serial.to_owned(),
        frames,
        status_code: None,
    })
}

fn text_verdict(output: &str) -> Option<bool> {
    if output
        .lines()
        .any(|line| line.trim() == "Failed" || line.trim().starts_with("Failed #"))
    {
        Some(false)
    } else if output.lines().any(|line| line.trim() == "Passed") {
        Some(true)
    } else {
        None
    }
}

fn blargg_passed(result: &BlarggRunResult) -> bool {
    match result.status_code {
        Some(status) => status == 0,
        None => text_verdict(&result.output) == Some(true),
    }
}

fn required_blargg_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::var_os("EMU198X_GB_BLARGG_ROOT")
            .ok_or("explicit fixture run requires EMU198X_GB_BLARGG_ROOT")?,
    );
    if !root.is_dir() {
        return Err(format!("Blargg directory missing: {}", root.display()).into());
    }
    Ok(root)
}

// This particular binary reports only through its ASCII tile console. Read
// the visible background rows with side-effect-free peeks; never patch the ROM
// or manufacture cartridge RAM. See test-data/game-boy-blargg-oam-interrupt.md.
fn interrupt_console(runtime: &GameBoyRuntime) -> Option<String> {
    let machine = runtime.machine()?;
    let lcdc = machine.peek(0xff40);
    let scy = machine.peek(0xff42);
    if lcdc & 0x99 != 0x91 || machine.peek(0xff43) != 0 || scy % 8 != 0 {
        return None;
    }
    let mut output = String::new();
    for row in 0..18u16 {
        let y = (u16::from(scy / 8) + row) % 32;
        let mut line = String::new();
        for col in 0..20 {
            // Bit 7 selects the console's inverse glyph set.
            let byte = machine.peek(0x9800 + y * 32 + col) & 0x7f;
            if !(0x20..=0x7e).contains(&byte) {
                return None;
            }
            line.push(char::from(byte));
        }
        output.push_str(line.trim_end());
        output.push('\n');
    }
    output
        .lines()
        .any(|line| line == "interrupt time")
        .then_some(output)
}

fn run_interrupt_time() -> Result<BlarggRunResult, Box<dyn std::error::Error>> {
    let path = required_blargg_root()?.join("interrupt_time/interrupt_time.gb");
    let mut runtime = load_runtime(&path)?;
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    for frame in 1..=MAX_SERIAL_TEST_FRAMES {
        let target = MachineTime::new(
            runtime.time().get() + u64::from(common_nintendo_game_boy::MCYCLES_PER_FRAME),
        );
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        let result = runtime.run_until(target, &mut host)?;
        assert_eq!(result.stop_reason, StopReason::ReachedTarget);
        if let Some(output) = interrupt_console(&runtime)
            && text_verdict(&output).is_some()
        {
            eprintln!(
                "interrupt_time: frame={frame} full_verdict={:?}\n{output}",
                text_verdict(&output)
            );
            return Ok(BlarggRunResult {
                output,
                frames: frame,
                status_code: None,
            });
        }
    }
    Err(format!(
        "interrupt_time: no console verdict after {MAX_SERIAL_TEST_FRAMES} frames; {:?}",
        interrupt_console(&runtime)
    )
    .into())
}

fn interrupt_measurements(output: &str) -> Result<Vec<[u8; 3]>, String> {
    let mut rows = Vec::new();
    for line in output
        .lines()
        .filter(|line| line.len() == 8 && line.as_bytes()[2] == b' ' && line.as_bytes()[5] == b' ')
    {
        let fields = line
            .split_whitespace()
            .map(|field| u8::from_str_radix(field, 16))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| err.to_string())?;
        rows.push(fields.try_into().map_err(|_| "invalid measurement row")?);
    }
    if rows.len() != 4 {
        return Err(format!(
            "expected four interrupt measurements, got {rows:?}"
        ));
    }
    Ok(rows)
}

fn normal_speed_interrupts_pass(output: &str) -> Result<(), String> {
    let rows = interrupt_measurements(output)?;
    // Author's source: zero when no interrupt, 13 M-cycles with dispatch,
    // vector JP and RET. This is not a claim that dispatch alone takes 13.
    if rows[..2] != [[0, 0, 0], [0, 8, 13]] {
        return Err(format!(
            "wrong normal-speed interrupt measurements: {rows:?}"
        ));
    }
    Ok(())
}

#[test]
#[ignore = "KNOWN DIVERGENCE: #1728 DMG OAM corruption; needs local Blargg ROMs"]
fn blargg_oam_bug_passes_aggregate_and_all_singles() -> Result<(), Box<dyn std::error::Error>> {
    let root = required_blargg_root()?;
    let mut failures = Vec::new();
    for rel in OAM_BUG_TESTS {
        let result = load_runtime(&root.join(rel))
            .and_then(|mut runtime| run_until_blargg_verdict(&mut runtime, MAX_OAM_TEST_FRAMES));
        match result {
            Ok(result) => {
                let passed = blargg_passed(&result);
                eprintln!(
                    "{} {rel}: frame={} status={:?}\n{}",
                    if passed { "PASS" } else { "FAIL" },
                    result.frames,
                    result.status_code,
                    result.output
                );
                if !passed {
                    failures.push((*rel).to_owned());
                }
            }
            Err(err) => {
                eprintln!("ERROR {rel}: {err}");
                failures.push(format!("{rel}: {err}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "OAM failures (aggregate plus eight singles): {failures:#?}"
    );
    Ok(())
}

#[test]
#[ignore = "FIXTURE: needs local Blargg interrupt_time ROM; checks normal speed only"]
fn blargg_interrupt_time_normal_speed_passes() -> Result<(), Box<dyn std::error::Error>> {
    normal_speed_interrupts_pass(&run_interrupt_time()?.output)?;
    Ok(())
}

#[test]
#[ignore = "KNOWN LIMITATION: #324 full interrupt_time requires CGB; needs local Blargg ROM"]
fn blargg_interrupt_time_full_rom_passes() -> Result<(), Box<dyn std::error::Error>> {
    let result = run_interrupt_time()?;
    assert!(
        blargg_passed(&result),
        "full interrupt_time verdict fails on DMG; CGB speed switching is tracked in #324: {}",
        result.output
    );
    Ok(())
}

#[test]
fn blargg_ram_report_keeps_final_text_and_status() {
    let pending = [0x80, 0xde, 0xb0, 0x61, b't', 0];
    assert!(blargg_report("Passed\n", parse_blargg_ram(&pending), 1).is_none());
    let mut final_ram = vec![2, 0xde, 0xb0, 0x61];
    final_ram.extend_from_slice(b"title\nFailed #2\n\0ignored");
    let result = blargg_report("Passed\n", parse_blargg_ram(&final_ram), 2).expect("terminal RAM");
    assert_eq!(result.output, "title\nFailed #2\n");
    assert!(!blargg_passed(&result));
    final_ram[0] = 0;
    assert!(blargg_passed(
        &blargg_report("", parse_blargg_ram(&final_ram), 3).expect("pass status")
    ));
    final_ram[1] = 0;
    assert!(parse_blargg_ram(&final_ram).is_none());
    assert!(parse_blargg_ram(&[0, 0xde, 0xb0]).is_none());
    assert!(blargg_report("not Passed yet", None, 1).is_none());
    assert!(!blargg_passed(
        &blargg_report("Passed\nFailed #2\n", None, 1).expect("failure")
    ));
}

#[test]
fn interrupt_measurement_check_rejects_incomplete_and_wrong_counts() {
    let output = "interrupt time\n00 00 00\n00 08 0D\n00 00 00\n00 08 0D\nFailed\n";
    assert!(normal_speed_interrupts_pass(output).is_ok());
    assert!(normal_speed_interrupts_pass(&output.replace("0D", "0C")).is_err());
    assert!(normal_speed_interrupts_pass("interrupt time\nPassed\n").is_err());
    assert!(normal_speed_interrupts_pass(&output.replacen("00 08", "01 08", 1)).is_err());
    assert!(normal_speed_interrupts_pass(&output.replace("0D", "ZZ")).is_err());
}
