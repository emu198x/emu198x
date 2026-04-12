/// ZEXDOC / ZEXALL test harness.
///
/// These are Frank Cringle's Z80 exerciser programs, originally CP/M .COM files.
/// They test every Z80 instruction by running it with many inputs and comparing
/// a CRC of the resulting flags/registers against known-good values.
///
/// We load them at $0100 (CP/M TPA), trap BDOS calls at $0005 for console
/// output, and trap JP $0000 (warm boot) as exit.
mod support;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ZexCheckpointStatus {
    Ok,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ZexCheckpoint {
    index: usize,
    label: &'static str,
    status: ZexCheckpointStatus,
    line: String,
    cycle_count: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ZexRunOptions {
    stop_after_checkpoint: Option<usize>,
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
    checkpoints: Vec<ZexCheckpoint>,
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
        return Err(format!(
            "encountered extra ZEX output after the final checkpoint: {trimmed}"
        ));
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
        label,
        status,
        line: trimmed.to_string(),
        cycle_count,
    }))
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
    run_zex(&com_data, options)
}

/// Minimal CP/M memory: 64K flat, .COM loaded at $0100.
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
fn run_zex(com_data: &[u8], options: ZexRunOptions) -> ZexRunResult {
    let mut mem = CpmMemory::new(com_data);
    let mut z80 = Z80::new();
    let mut console = ZexConsole::default();
    let mut completed = false;
    let mut timed_out = false;
    let mut checkpoints = Vec::new();
    let mut bdos_call_active = false;
    let mut stopped_at_requested_checkpoint = false;
    let mut last_line = None;

    // CP/M entry: PC = $0100, SP = $FFFE (below BDOS)
    z80.regs.pc = 0x0100;
    z80.regs.sp = 0xFFFE;

    let mut cycle_count: u64 = 0;
    let max_cycles: u64 = 500_000_000_000; // Safety limit

    loop {
        z80.tick();
        cycle_count += 1;

        // Handle bus: memory read/write
        if z80.mreq && z80.rd {
            z80.data_in = mem.read(z80.addr);
        } else if z80.mreq && z80.wr {
            mem.write(z80.addr, z80.data);
        } else if z80.iorq && z80.m1 {
            // Interrupt ack (shouldn't happen — no interrupts)
            z80.data_in = 0xFF;
        }

        // Check for BDOS call: handle it once when the M1 fetch at $0005 begins.
        let at_bdos_fetch = z80.m1 && z80.addr == 0x0005;
        if at_bdos_fetch && !bdos_call_active {
            bdos_call_active = true;
            let func = z80.regs.bc & 0xFF; // C register = BDOS function
            match func as u8 {
                2 => {
                    // Print character (E register)
                    let ch = (z80.regs.de & 0xFF) as u8 as char;
                    console.push_char(ch);
                    eprint!("{ch}");
                }
                9 => {
                    // Print string (DE = address, '$' terminated)
                    let mut addr = z80.regs.de;
                    loop {
                        let ch = mem.read(addr);
                        if ch == b'$' {
                            break;
                        }
                        console.push_char(ch as char);
                        eprint!("{}", ch as char);
                        addr = addr.wrapping_add(1);
                    }
                }
                _ => {}
            }
            // The RET at $0005 will pop back to the caller
        } else if !at_bdos_fetch {
            bdos_call_active = false;
        }

        for line in console.drain_lines() {
            last_line = Some(line.clone());

            if line.trim().starts_with("Tests complete") {
                completed = line.contains("OK");
            }

            match parse_checkpoint_line(&line, checkpoints.len(), cycle_count) {
                Ok(Some(checkpoint)) => {
                    let checkpoint_index = checkpoint.index;
                    checkpoints.push(checkpoint);

                    if checkpoints
                        .last()
                        .is_some_and(|checkpoint| checkpoint.status == ZexCheckpointStatus::Error)
                    {
                        break;
                    }

                    if options.stop_after_checkpoint == Some(checkpoint_index) {
                        stopped_at_requested_checkpoint = true;
                        break;
                    }
                }
                Ok(None) => {}
                Err(message) => panic!("{message}\nlast line: {line}"),
            }
        }

        if stopped_at_requested_checkpoint {
            eprintln!("\nZEX checkpoint stop after {} cycles", cycle_count);
            break;
        }

        if checkpoints
            .last()
            .is_some_and(|checkpoint| checkpoint.status == ZexCheckpointStatus::Error)
        {
            eprintln!("\nZEX failed after {} cycles", cycle_count);
            break;
        }

        if completed {
            eprintln!("\nZEX complete after {} cycles", cycle_count);
            break;
        }

        if z80.halt {
            eprintln!("\nZEX complete after {} cycles", cycle_count);
            break;
        }

        if cycle_count > max_cycles {
            timed_out = true;
            eprintln!("\nZEX timed out after {} cycles", cycle_count);
            break;
        }
    }

    console.finish_line();
    for line in console.drain_lines() {
        last_line = Some(line.clone());
        match parse_checkpoint_line(&line, checkpoints.len(), cycle_count) {
            Ok(Some(checkpoint)) => checkpoints.push(checkpoint),
            Ok(None) => {}
            Err(message) => panic!("{message}\nlast line: {line}"),
        }
    }

    ZexRunResult {
        output: console.output,
        completed,
        timed_out,
        cycle_count,
        stopped_at_requested_checkpoint,
        checkpoints,
        last_line,
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
        return;
    };

    let result = run_zex_suite(
        ZexSuite::Doc,
        ZexRunOptions {
            stop_after_checkpoint: Some(target),
        },
    );
    assert_checkpoint_target_hit(ZexSuite::Doc, &result, target);
}

#[test]
#[ignore = "requires local ZEX corpus and EMU198X_ZEX_CHECKPOINT to target one checkpoint"]
fn run_zexall_checkpoint() {
    let Some(target) = zex_checkpoint_target_from_env(ZexSuite::All) else {
        return;
    };

    let result = run_zex_suite(
        ZexSuite::All,
        ZexRunOptions {
            stop_after_checkpoint: Some(target),
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
