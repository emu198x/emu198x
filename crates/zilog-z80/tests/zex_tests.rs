/// ZEXDOC / ZEXALL test harness.
///
/// These are Frank Cringle's Z80 exerciser programs, originally CP/M .COM files.
/// They test every Z80 instruction by running it with many inputs and comparing
/// a CRC of the resulting flags/registers against known-good values.
///
/// We load them at $0100 (CP/M TPA), trap BDOS calls at $0005 for console
/// output, and trap JP $0000 (warm boot) as exit.
mod support;

use std::path::{Path, PathBuf};

use support::find_zex_binary;
use zilog_z80::Z80;

const ZEX_CHECKPOINT_LABELS: [&str; 67] = [
    "<adc,sbc> hl,<bc,de,hl,sp>....",
    "add hl,<bc,de,hl,sp>..........",
    "add ix,<bc,de,ix,sp>..........",
    "add iy,<bc,de,iy,sp>..........",
    "aluop a,nn....................",
    "aluop a,<b,c,d,e,h,l,(hl),a>..",
    "aluop a,<ixh,ixl,iyh,iyl>.....",
    "aluop a,(<ix,iy>+1)...........",
    "bit n,(<ix,iy>+1).............",
    "bit n,<b,c,d,e,h,l,(hl),a>....",
    "cpd<r>........................",
    "cpi<r>........................",
    "<daa,cpl,scf,ccf>.............",
    "<inc,dec> a...................",
    "<inc,dec> b...................",
    "<inc,dec> bc..................",
    "<inc,dec> c...................",
    "<inc,dec> d...................",
    "<inc,dec> de..................",
    "<inc,dec> e...................",
    "<inc,dec> h...................",
    "<inc,dec> hl..................",
    "<inc,dec> ix..................",
    "<inc,dec> iy..................",
    "<inc,dec> l...................",
    "<inc,dec> (hl)................",
    "<inc,dec> sp..................",
    "<inc,dec> (<ix,iy>+1).........",
    "<inc,dec> ixh.................",
    "<inc,dec> ixl.................",
    "<inc,dec> iyh.................",
    "<inc,dec> iyl.................",
    "ld <bc,de>,(nnnn).............",
    "ld hl,(nnnn)..................",
    "ld sp,(nnnn)..................",
    "ld <ix,iy>,(nnnn).............",
    "ld (nnnn),<bc,de>.............",
    "ld (nnnn),hl..................",
    "ld (nnnn),sp..................",
    "ld (nnnn),<ix,iy>.............",
    "ld <bc,de,hl,sp>,nnnn.........",
    "ld <ix,iy>,nnnn...............",
    "ld a,<(bc),(de)>..............",
    "ld <b,c,d,e,h,l,(hl),a>,nn....",
    "ld (<ix,iy>+1),nn.............",
    "ld <b,c,d,e>,(<ix,iy>+1)......",
    "ld <h,l>,(<ix,iy>+1)..........",
    "ld a,(<ix,iy>+1)..............",
    "ld <ixh,ixl,iyh,iyl>,nn.......",
    "ld <bcdehla>,<bcdehla>........",
    "ld <bcdexya>,<bcdexya>........",
    "ld a,(nnnn) / ld (nnnn),a.....",
    "ldd<r> (1)....................",
    "ldd<r> (2)....................",
    "ldi<r> (1)....................",
    "ldi<r> (2)....................",
    "neg...........................",
    "<rrd,rld>.....................",
    "<rlca,rrca,rla,rra>...........",
    "shf/rot (<ix,iy>+1)...........",
    "shf/rot <b,c,d,e,h,l,(hl),a>..",
    "<set,res> n,<bcdehl(hl)a>.....",
    "<set,res> n,(<ix,iy>+1).......",
    "ld (<ix,iy>+1),<b,c,d,e>......",
    "ld (<ix,iy>+1),<h,l>..........",
    "ld (<ix,iy>+1),a..............",
    "ld (<bc,de>),a................",
];

const ZEX_CHECKPOINT_ENV: &str = "EMU198X_ZEX_CHECKPOINT";
const ZEX_SNAPSHOT_DIR_ENV: &str = "EMU198X_ZEX_SNAPSHOT_DIR";
const ZEX_SNAPSHOT_MAGIC: &[u8; 8] = b"ZEXSNAP1";
const ZEX_SNAPSHOT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
enum ZexSuite {
    Doc,
    All,
}

impl ZexSuite {
    fn binary_name(self) -> &'static str {
        match self {
            Self::Doc => "zexdoc",
            Self::All => "zexall",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Doc => "ZEXDOC",
            Self::All => "ZEXALL",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
enum ZexCheckpointStatus {
    Ok,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct ZexCheckpoint {
    index: usize,
    label: String,
    status: ZexCheckpointStatus,
    line: String,
    cycle_count: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ZexRunOptions {
    stop_after_checkpoint: Option<usize>,
    snapshot_dir: Option<PathBuf>,
}

#[derive(Debug, Default)]
struct ZexConsole {
    output: String,
    current_line: String,
    completed_lines: Vec<String>,
}

struct ZexRunResult {
    output: String,
    completed: bool,
    timed_out: bool,
    cycle_count: u64,
    stopped_at_requested_checkpoint: bool,
    resumed_from_checkpoint: Option<usize>,
    checkpoints: Vec<ZexCheckpoint>,
    last_line: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ZexHarnessSnapshot {
    version: u32,
    suite: ZexSuite,
    checkpoint_index: usize,
    cycle_count: u64,
    checkpoints: Vec<ZexCheckpoint>,
    z80: Z80,
}

struct ZexHarnessState {
    mem: CpmMemory,
    z80: Z80,
    console: ZexConsole,
    checkpoints: Vec<ZexCheckpoint>,
    cycle_count: u64,
    bdos_call_active: bool,
    last_line: Option<String>,
}

impl ZexConsole {
    fn push_char(&mut self, ch: char) {
        self.output.push(ch);
        match ch {
            '\r' | '\n' => self.finish_line(),
            _ => self.current_line.push(ch),
        }
    }

    fn push_text(&mut self, text: &str) {
        for ch in text.chars() {
            eprint!("{ch}");
            self.push_char(ch);
        }
    }

    fn finish_line(&mut self) {
        if !self.current_line.is_empty() {
            self.completed_lines
                .push(std::mem::take(&mut self.current_line));
        }
    }

    fn drain_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.completed_lines)
    }
}

impl ZexHarnessState {
    fn cold_boot(com_data: &[u8]) -> Self {
        let mut z80 = Z80::new();
        z80.regs.pc = 0x0100;
        z80.regs.sp = 0xFFFE;

        Self {
            mem: CpmMemory::new(com_data),
            z80,
            console: ZexConsole::default(),
            checkpoints: Vec::new(),
            cycle_count: 0,
            bdos_call_active: false,
            last_line: None,
        }
    }
}

fn parse_checkpoint_line(
    line: &str,
    checkpoints_seen: usize,
    cycle_count: u64,
) -> Result<Option<ZexCheckpoint>, String> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed == "Z80 instruction exerciser"
        || trimmed.starts_with("Tests complete")
    {
        return Ok(None);
    }

    let Some(label) = ZEX_CHECKPOINT_LABELS.get(checkpoints_seen).copied() else {
        return Ok(None);
    };

    if !trimmed.starts_with(label) {
        return Ok(None);
    }

    let status = if trimmed.contains("ERROR") {
        ZexCheckpointStatus::Error
    } else if trimmed.contains("OK") {
        ZexCheckpointStatus::Ok
    } else {
        return Ok(None);
    };

    Ok(Some(ZexCheckpoint {
        index: checkpoints_seen + 1,
        label: label.to_string(),
        status,
        line: trimmed.to_string(),
        cycle_count,
    }))
}

fn is_zex_completion_line(line: &str) -> bool {
    line.trim().starts_with("Tests complete")
}

fn load_zex_binary(suite: ZexSuite) -> Vec<u8> {
    let path = match find_zex_binary(suite.binary_name()) {
        Ok(path) => path,
        Err(message) => panic!("{message}"),
    };

    match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => panic!("failed to read {}: {error}", path.display()),
    }
}

fn zex_checkpoint_target_from_env(suite: ZexSuite) -> Option<usize> {
    let raw = match std::env::var(ZEX_CHECKPOINT_ENV) {
        Ok(value) => value,
        Err(_) => {
            eprintln!(
                "{} targeted checkpoint test skipped: set {} to a value from 1 to {}",
                suite.display_name(),
                ZEX_CHECKPOINT_ENV,
                ZEX_CHECKPOINT_LABELS.len()
            );
            return None;
        }
    };

    let parsed = match raw.parse::<usize>() {
        Ok(value) => value,
        Err(error) => panic!(
            "failed to parse {}='{}' as checkpoint index: {error}",
            ZEX_CHECKPOINT_ENV, raw
        ),
    };

    assert!(
        (1..=ZEX_CHECKPOINT_LABELS.len()).contains(&parsed),
        "{} must be between 1 and {}",
        ZEX_CHECKPOINT_ENV,
        ZEX_CHECKPOINT_LABELS.len()
    );

    Some(parsed)
}

fn default_zex_snapshot_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/zex-snapshots")
        .to_path_buf()
}

fn zex_snapshot_dir() -> PathBuf {
    match std::env::var_os(ZEX_SNAPSHOT_DIR_ENV) {
        Some(path) => PathBuf::from(path),
        None => default_zex_snapshot_dir(),
    }
}

fn zex_snapshot_path(snapshot_dir: &Path, suite: ZexSuite, checkpoint_index: usize) -> PathBuf {
    snapshot_dir
        .join(suite.binary_name())
        .join(format!("checkpoint-{checkpoint_index:02}.bin"))
}

fn save_zex_snapshot(
    snapshot_dir: &Path,
    suite: ZexSuite,
    state: &ZexHarnessState,
    checkpoint_index: usize,
) -> Result<(), String> {
    let path = zex_snapshot_path(snapshot_dir, suite, checkpoint_index);
    let metadata = ZexHarnessSnapshot {
        version: ZEX_SNAPSHOT_VERSION,
        suite,
        checkpoint_index,
        cycle_count: state.cycle_count,
        checkpoints: state.checkpoints.clone(),
        z80: state.z80.clone(),
    };
    let metadata_bytes = serde_json::to_vec(&metadata).map_err(|error| {
        format!(
            "failed to encode {} snapshot metadata: {error}",
            suite.display_name()
        )
    })?;
    let metadata_len = u32::try_from(metadata_bytes.len()).map_err(|_| {
        format!(
            "{} snapshot metadata exceeded u32 length",
            suite.display_name()
        )
    })?;

    let parent = match path.parent() {
        Some(parent) => parent,
        None => {
            return Err(format!(
                "failed to resolve parent directory for snapshot path {}",
                path.display()
            ));
        }
    };
    std::fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create {} snapshot directory {}: {error}",
            suite.display_name(),
            parent.display()
        )
    })?;

    let mut bytes = Vec::with_capacity(
        ZEX_SNAPSHOT_MAGIC.len() + 4 + metadata_bytes.len() + state.mem.mem.len(),
    );
    bytes.extend_from_slice(ZEX_SNAPSHOT_MAGIC);
    bytes.extend_from_slice(&metadata_len.to_le_bytes());
    bytes.extend_from_slice(&metadata_bytes);
    bytes.extend_from_slice(&state.mem.mem);

    std::fs::write(&path, bytes).map_err(|error| {
        format!(
            "failed to write {} snapshot {}: {error}",
            suite.display_name(),
            path.display()
        )
    })
}

fn load_zex_snapshot(path: &Path, suite: ZexSuite) -> Result<ZexHarnessState, String> {
    let bytes = std::fs::read(path).map_err(|error| {
        format!(
            "failed to read {} snapshot {}: {error}",
            suite.display_name(),
            path.display()
        )
    })?;

    let minimum_len = ZEX_SNAPSHOT_MAGIC.len() + 4 + 65_536;
    if bytes.len() < minimum_len {
        return Err(format!(
            "{} snapshot {} is truncated",
            suite.display_name(),
            path.display()
        ));
    }

    if &bytes[..ZEX_SNAPSHOT_MAGIC.len()] != ZEX_SNAPSHOT_MAGIC {
        return Err(format!(
            "{} snapshot {} has an invalid header",
            suite.display_name(),
            path.display()
        ));
    }

    let metadata_len_offset = ZEX_SNAPSHOT_MAGIC.len();
    let metadata_len_end = metadata_len_offset + 4;
    let metadata_len = u32::from_le_bytes([
        bytes[metadata_len_offset],
        bytes[metadata_len_offset + 1],
        bytes[metadata_len_offset + 2],
        bytes[metadata_len_offset + 3],
    ]) as usize;
    let metadata_start = metadata_len_end;
    let metadata_end = metadata_start + metadata_len;

    if bytes.len() < metadata_end + 65_536 {
        return Err(format!(
            "{} snapshot {} is truncated after metadata",
            suite.display_name(),
            path.display()
        ));
    }

    let mut metadata: ZexHarnessSnapshot =
        serde_json::from_slice(&bytes[metadata_start..metadata_end]).map_err(|error| {
            format!(
                "failed to decode {} snapshot metadata from {}: {error}",
                suite.display_name(),
                path.display()
            )
        })?;

    if metadata.version != ZEX_SNAPSHOT_VERSION {
        return Err(format!(
            "{} snapshot {} has version {} instead of {}",
            suite.display_name(),
            path.display(),
            metadata.version,
            ZEX_SNAPSHOT_VERSION
        ));
    }
    if metadata.suite != suite {
        return Err(format!(
            "{} snapshot {} is tagged for {:?}",
            suite.display_name(),
            path.display(),
            metadata.suite
        ));
    }
    if metadata.checkpoint_index != metadata.checkpoints.len() {
        return Err(format!(
            "{} snapshot {} has checkpoint count {} but index {}",
            suite.display_name(),
            path.display(),
            metadata.checkpoints.len(),
            metadata.checkpoint_index
        ));
    }
    metadata.z80.rehydrate_walker_sequence();

    let memory_bytes = &bytes[metadata_end..metadata_end + 65_536];
    let mut mem = [0u8; 65_536];
    mem.copy_from_slice(memory_bytes);

    Ok(ZexHarnessState {
        mem: CpmMemory { mem },
        z80: metadata.z80,
        console: ZexConsole::default(),
        checkpoints: metadata.checkpoints,
        cycle_count: metadata.cycle_count,
        bdos_call_active: false,
        last_line: None,
    })
}

fn load_best_zex_snapshot(
    snapshot_dir: &Path,
    suite: ZexSuite,
    target_checkpoint: usize,
) -> Result<Option<(usize, ZexHarnessState)>, String> {
    for checkpoint_index in (1..=target_checkpoint).rev() {
        let path = zex_snapshot_path(snapshot_dir, suite, checkpoint_index);
        if path.exists() {
            let state = load_zex_snapshot(&path, suite)?;
            return Ok(Some((checkpoint_index, state)));
        }
    }

    Ok(None)
}

fn assert_full_zex_success(suite: ZexSuite, result: &ZexRunResult) {
    let tests_ok = result
        .checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.status == ZexCheckpointStatus::Ok)
        .count();
    let tests_fail = result
        .checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.status == ZexCheckpointStatus::Error)
        .count();

    eprintln!(
        "{}: {} checkpoints, {} OK, {} ERROR",
        suite.display_name(),
        result.checkpoints.len(),
        tests_ok,
        tests_fail
    );

    assert!(
        !result.timed_out,
        "{} timed out after {} cycles; last line: {:?}\n{}",
        suite.display_name(),
        result.cycle_count,
        result.last_line,
        result.output
    );
    assert!(
        result.completed,
        "{} did not report completion; last line: {:?}\n{}",
        suite.display_name(),
        result.last_line,
        result.output
    );
    assert_eq!(
        result.checkpoints.len(),
        ZEX_CHECKPOINT_LABELS.len(),
        "{} reported {} checkpoints instead of {}",
        suite.display_name(),
        result.checkpoints.len(),
        ZEX_CHECKPOINT_LABELS.len()
    );
    assert_eq!(
        tests_fail,
        0,
        "{} had {} failures; last failing checkpoint: {:?}\n{}",
        suite.display_name(),
        tests_fail,
        result
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.status == ZexCheckpointStatus::Error),
        result.output
    );
}

fn assert_checkpoint_target_hit(suite: ZexSuite, result: &ZexRunResult, target: usize) {
    let checkpoint = match result.checkpoints.last() {
        Some(checkpoint) => checkpoint,
        None => panic!("{} produced no checkpoints", suite.display_name()),
    };

    if let Some(resumed_from_checkpoint) = result.resumed_from_checkpoint {
        eprintln!(
            "{} resumed from snapshot after checkpoint {}",
            suite.display_name(),
            resumed_from_checkpoint
        );
    }

    assert!(
        result.stopped_at_requested_checkpoint,
        "{} did not stop at checkpoint {}; got {} checkpoints\n{}",
        suite.display_name(),
        target,
        result.checkpoints.len(),
        result.output
    );
    assert_eq!(
        checkpoint.index,
        target,
        "{} stopped at checkpoint {} instead of {}",
        suite.display_name(),
        checkpoint.index,
        target
    );
    assert_eq!(
        checkpoint.label,
        ZEX_CHECKPOINT_LABELS[target - 1],
        "{} checkpoint {} label mismatch",
        suite.display_name(),
        target
    );
    assert_eq!(
        checkpoint.status,
        ZexCheckpointStatus::Ok,
        "{} checkpoint {} failed: {}",
        suite.display_name(),
        target,
        checkpoint.line
    );
}

fn run_zex_suite(suite: ZexSuite, options: ZexRunOptions) -> ZexRunResult {
    let com_data = load_zex_binary(suite);
    let options = ZexRunOptions {
        snapshot_dir: Some(options.snapshot_dir.unwrap_or_else(zex_snapshot_dir)),
        ..options
    };
    run_zex(suite, &com_data, options)
}

/// Minimal CP/M memory: 64K flat, .COM loaded at $0100.
#[derive(Clone)]
struct CpmMemory {
    mem: [u8; 65536],
}

impl CpmMemory {
    fn new(com: &[u8]) -> Self {
        let mut mem = [0u8; 65536];
        // Load .COM at $0100
        let end = (0x0100 + com.len()).min(65536);
        mem[0x0100..end].copy_from_slice(&com[..end - 0x0100]);
        // Put RET at $0005 (BDOS entry) — we trap before it executes
        mem[0x0005] = 0xC9; // RET
        // Put HALT at $0000 (warm boot) — we trap this
        mem[0x0000] = 0x76; // HALT
        Self { mem }
    }

    fn read(&self, addr: u16) -> u8 {
        self.mem[addr as usize]
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.mem[addr as usize] = val;
    }
}

/// Run a ZEX .COM file, returning the console output and completion status.
fn run_zex(suite: ZexSuite, com_data: &[u8], options: ZexRunOptions) -> ZexRunResult {
    let mut resumed_from_checkpoint = None;
    let resume_target = options
        .stop_after_checkpoint
        .map(|target| target.saturating_sub(1))
        .unwrap_or(ZEX_CHECKPOINT_LABELS.len());
    let mut state = if let Some(snapshot_dir) = options.snapshot_dir.as_deref() {
        match load_best_zex_snapshot(snapshot_dir, suite, resume_target) {
            Ok(Some((checkpoint_index, state))) => {
                resumed_from_checkpoint = Some(checkpoint_index);
                eprintln!(
                    "Resuming {} from snapshot after checkpoint {}",
                    suite.display_name(),
                    checkpoint_index
                );
                state
            }
            Ok(None) => ZexHarnessState::cold_boot(com_data),
            Err(message) => panic!("{message}"),
        }
    } else {
        ZexHarnessState::cold_boot(com_data)
    };
    let mut completed = false;
    let mut timed_out = false;
    let mut checkpoint_snapshot_pending = None;
    let mut stopped_at_requested_checkpoint = false;

    let max_cycles: u64 = 500_000_000_000; // Safety limit

    loop {
        state.z80.tick();
        state.cycle_count += 1;

        // Handle bus: memory read/write
        if state.z80.mreq && state.z80.rd {
            state.z80.data_in = state.mem.read(state.z80.addr);
        } else if state.z80.mreq && state.z80.wr {
            state.mem.write(state.z80.addr, state.z80.data);
        } else if state.z80.iorq && state.z80.m1 {
            // Interrupt ack (shouldn't happen — no interrupts)
            state.z80.data_in = 0xFF;
        }

        // Check for BDOS call: handle it once when the M1 fetch at $0005 begins.
        let at_bdos_fetch = state.z80.m1 && state.z80.addr == 0x0005;
        if at_bdos_fetch && !state.bdos_call_active {
            state.bdos_call_active = true;
            let func = state.z80.regs.bc & 0xFF; // C register = BDOS function
            match func as u8 {
                2 => {
                    // Print character (E register)
                    let ch = (state.z80.regs.de & 0xFF) as u8 as char;
                    state.console.push_char(ch);
                    eprint!("{ch}");
                }
                9 => {
                    // Print string (DE = address, '$' terminated)
                    let mut addr = state.z80.regs.de;
                    loop {
                        let ch = state.mem.read(addr);
                        if ch == b'$' {
                            break;
                        }
                        state.console.push_char(ch as char);
                        eprint!("{}", ch as char);
                        addr = addr.wrapping_add(1);
                    }
                }
                _ => {}
            }
            // The RET at $0005 will pop back to the caller
        } else if !at_bdos_fetch {
            state.bdos_call_active = false;
        }

        for line in state.console.drain_lines() {
            state.last_line = Some(line.clone());

            if is_zex_completion_line(&line) {
                completed = true;
            }

            match parse_checkpoint_line(&line, state.checkpoints.len(), state.cycle_count) {
                Ok(Some(checkpoint)) => {
                    let checkpoint_index = checkpoint.index;
                    let checkpoint_failed = checkpoint.status == ZexCheckpointStatus::Error;
                    state.checkpoints.push(checkpoint);

                    if checkpoint_failed {
                        break;
                    }

                    checkpoint_snapshot_pending = Some(checkpoint_index);
                    if options.stop_after_checkpoint == Some(checkpoint_index) {
                        stopped_at_requested_checkpoint = true;
                    }
                }
                Ok(None) => {}
                Err(message) => panic!("{message}\nlast line: {line}"),
            }
        }

        if let Some(checkpoint_index) = checkpoint_snapshot_pending
            && state.z80.instruction_complete()
        {
            if let Some(snapshot_dir) = options.snapshot_dir.as_deref()
                && let Err(message) =
                    save_zex_snapshot(snapshot_dir, suite, &state, checkpoint_index)
            {
                panic!("{message}");
            }
            checkpoint_snapshot_pending = None;

            if stopped_at_requested_checkpoint {
                eprintln!("\nZEX checkpoint stop after {} cycles", state.cycle_count);
                break;
            }
        }

        if state
            .checkpoints
            .last()
            .is_some_and(|checkpoint| checkpoint.status == ZexCheckpointStatus::Error)
        {
            eprintln!("\nZEX failed after {} cycles", state.cycle_count);
            break;
        }

        if completed && checkpoint_snapshot_pending.is_none() {
            eprintln!("\nZEX complete after {} cycles", state.cycle_count);
            break;
        }

        if state.z80.halt && checkpoint_snapshot_pending.is_none() {
            eprintln!("\nZEX complete after {} cycles", state.cycle_count);
            break;
        }

        if state.cycle_count > max_cycles {
            timed_out = true;
            eprintln!("\nZEX timed out after {} cycles", state.cycle_count);
            break;
        }
    }

    state.console.finish_line();
    for line in state.console.drain_lines() {
        state.last_line = Some(line.clone());
        if is_zex_completion_line(&line) {
            completed = true;
        }
        match parse_checkpoint_line(&line, state.checkpoints.len(), state.cycle_count) {
            Ok(Some(checkpoint)) => state.checkpoints.push(checkpoint),
            Ok(None) => {}
            Err(message) => panic!("{message}\nlast line: {line}"),
        }
    }

    ZexRunResult {
        output: state.console.output,
        completed,
        timed_out,
        cycle_count: state.cycle_count,
        stopped_at_requested_checkpoint,
        resumed_from_checkpoint,
        checkpoints: state.checkpoints,
        last_line: state.last_line,
    }
}

#[test]
#[ignore = "requires local ZEX corpus and runs for minutes"]
fn run_zexdoc() {
    let result = run_zex_suite(ZexSuite::Doc, ZexRunOptions::default());
    assert_full_zex_success(ZexSuite::Doc, &result);
}

#[test]
#[ignore = "requires local ZEX corpus and runs for minutes"]
fn run_zexall() {
    let result = run_zex_suite(ZexSuite::All, ZexRunOptions::default());
    assert_full_zex_success(ZexSuite::All, &result);
}

#[test]
#[ignore = "requires local ZEX corpus and EMU198X_ZEX_CHECKPOINT to target one checkpoint"]
fn run_zexdoc_checkpoint() {
    let Some(target) = zex_checkpoint_target_from_env(ZexSuite::Doc) else {
        emu198x_test_skip::skip!("ZEX corpus not staged");
    };

    let result = run_zex_suite(
        ZexSuite::Doc,
        ZexRunOptions {
            stop_after_checkpoint: Some(target),
            ..ZexRunOptions::default()
        },
    );
    assert_checkpoint_target_hit(ZexSuite::Doc, &result, target);
}

#[test]
#[ignore = "requires local ZEX corpus and EMU198X_ZEX_CHECKPOINT to target one checkpoint"]
fn run_zexall_checkpoint() {
    let Some(target) = zex_checkpoint_target_from_env(ZexSuite::All) else {
        emu198x_test_skip::skip!("ZEX corpus not staged");
    };

    let result = run_zex_suite(
        ZexSuite::All,
        ZexRunOptions {
            stop_after_checkpoint: Some(target),
            ..ZexRunOptions::default()
        },
    );
    assert_checkpoint_target_hit(ZexSuite::All, &result, target);
}

#[test]
fn parse_checkpoint_line_uses_expected_order() {
    let checkpoint = parse_checkpoint_line("<adc,sbc> hl,<bc,de,hl,sp>....  OK", 0, 12_345)
        .expect("checkpoint line should parse")
        .expect("checkpoint line should produce a checkpoint");

    assert_eq!(checkpoint.index, 1);
    assert_eq!(checkpoint.label, ZEX_CHECKPOINT_LABELS[0]);
    assert_eq!(checkpoint.status, ZexCheckpointStatus::Ok);
    assert_eq!(checkpoint.cycle_count, 12_345);
}

#[test]
fn console_capture_splits_crlf_lines() {
    let mut console = ZexConsole::default();
    console.push_text("one\r\ntwo\n\rthree");
    console.finish_line();

    assert_eq!(console.drain_lines(), vec!["one", "two", "three"]);
}

#[test]
fn tests_complete_line_is_treated_as_completion() {
    assert!(is_zex_completion_line("Tests complete"));
    assert!(is_zex_completion_line("Tests complete  OK"));
    assert!(!is_zex_completion_line(
        "ld a,(nnnn) / ld (nnnn),a.....  OK"
    ));
}

#[test]
fn extra_output_after_final_checkpoint_is_ignored() {
    assert!(
        parse_checkpoint_line("  OK", ZEX_CHECKPOINT_LABELS.len(), 99)
            .expect("extra line should not error")
            .is_none()
    );
}

fn unique_test_snapshot_dir(test_name: &str) -> PathBuf {
    let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos(),
        Err(error) => panic!("system clock is before UNIX_EPOCH: {error}"),
    };

    std::env::temp_dir().join(format!(
        "emu198x-zex-{test_name}-{}-{now}",
        std::process::id()
    ))
}

fn ok_checkpoint(index: usize, cycle_count: u64) -> ZexCheckpoint {
    ZexCheckpoint {
        index,
        label: ZEX_CHECKPOINT_LABELS[index - 1].to_string(),
        status: ZexCheckpointStatus::Ok,
        line: format!("{}  OK", ZEX_CHECKPOINT_LABELS[index - 1]),
        cycle_count,
    }
}

#[test]
fn zex_snapshot_roundtrip_restores_harness_state() {
    let snapshot_dir = unique_test_snapshot_dir("roundtrip");
    let mut state = ZexHarnessState::cold_boot(&[0x00, 0x76]);
    state.mem.write(0x2345, 0xA5);
    state.z80.regs.pc = 0x3456;
    state.z80.regs.sp = 0x4567;
    state.z80.regs.set_a(0x89);
    state.cycle_count = 98_765;
    state.checkpoints.push(ok_checkpoint(1, state.cycle_count));

    if let Err(message) = save_zex_snapshot(&snapshot_dir, ZexSuite::Doc, &state, 1) {
        panic!("{message}");
    }

    let snapshot_path = zex_snapshot_path(&snapshot_dir, ZexSuite::Doc, 1);
    let restored = match load_zex_snapshot(&snapshot_path, ZexSuite::Doc) {
        Ok(restored) => restored,
        Err(message) => panic!("{message}"),
    };

    assert_eq!(restored.mem.read(0x2345), 0xA5);
    assert_eq!(restored.z80.regs.pc, 0x3456);
    assert_eq!(restored.z80.regs.sp, 0x4567);
    assert_eq!(restored.z80.regs.a(), 0x89);
    assert_eq!(restored.cycle_count, 98_765);
    assert_eq!(restored.checkpoints.len(), 1);
    assert_eq!(restored.checkpoints[0].label, ZEX_CHECKPOINT_LABELS[0]);
    assert!(restored.console.output.is_empty());
    assert!(!restored.bdos_call_active);

    let _ = std::fs::remove_dir_all(snapshot_dir);
}

#[test]
fn zex_snapshot_loader_prefers_highest_cached_checkpoint_below_target() {
    let snapshot_dir = unique_test_snapshot_dir("best");

    let mut checkpoint_1 = ZexHarnessState::cold_boot(&[0x00, 0x76]);
    checkpoint_1.cycle_count = 1_000;
    checkpoint_1
        .checkpoints
        .push(ok_checkpoint(1, checkpoint_1.cycle_count));
    if let Err(message) = save_zex_snapshot(&snapshot_dir, ZexSuite::Doc, &checkpoint_1, 1) {
        panic!("{message}");
    }

    let mut checkpoint_3 = ZexHarnessState::cold_boot(&[0x00, 0x76]);
    checkpoint_3.cycle_count = 3_000;
    checkpoint_3.checkpoints.push(ok_checkpoint(1, 1_000));
    checkpoint_3.checkpoints.push(ok_checkpoint(2, 2_000));
    checkpoint_3.checkpoints.push(ok_checkpoint(3, 3_000));
    checkpoint_3.z80.regs.pc = 0x9999;
    if let Err(message) = save_zex_snapshot(&snapshot_dir, ZexSuite::Doc, &checkpoint_3, 3) {
        panic!("{message}");
    }

    let (checkpoint_index, restored) = match load_best_zex_snapshot(&snapshot_dir, ZexSuite::Doc, 4)
    {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => panic!("expected a cached snapshot"),
        Err(message) => panic!("{message}"),
    };

    assert_eq!(checkpoint_index, 3);
    assert_eq!(restored.cycle_count, 3_000);
    assert_eq!(restored.checkpoints.len(), 3);
    assert_eq!(restored.z80.regs.pc, 0x9999);

    let _ = std::fs::remove_dir_all(snapshot_dir);
}
