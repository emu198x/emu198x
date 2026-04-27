//! Minimal Dragon 32 ROM bring-up harness.
//!
//! This is deliberately smaller than the full machine/runtime path. It gives us
//! an executable ROM/CPU loop while PIA, SAM, and VDG are still being rebuilt.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process;

use motorola_6809::Mc6809;
use motorola_pia_6821::{Pia6821, PiaPort};
use zip::ZipArchive;

const RAM_SIZE: usize = 0x8000;
const ROM_SIZE: usize = 0x4000;
const DEFAULT_CYCLES: u64 = 100_000;
const DEFAULT_TRACE_LIMIT: usize = 64;

const USAGE: &str = "\
Usage: emu198x-script-dragon --rom PATH [OPTIONS]

Firmware:
    --rom PATH          Dragon 32 BASIC ROM, exactly 16 KiB; .zip archives are accepted

Execution:
    --cycles N         maximum MC6809 bus cycles to run [default: 100000]
    --trace-limit N    number of recent instruction fetches to retain [default: 64]
    --press KEY        hold a named Dragon key closed; may be repeated
    --press-matrix R,C hold a raw keyboard matrix switch closed; may be repeated

Other:
    --help             print this help text
";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cli {
    rom: PathBuf,
    cycles: u64,
    trace_limit: usize,
    pressed_keys: Vec<MatrixKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MatrixKey {
    row: usize,
    column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragonKey {
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Colon,
    Semicolon,
    Comma,
    Minus,
    Period,
    Slash,
    At,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Up,
    Down,
    Left,
    Right,
    Space,
    Enter,
    Clear,
    Break,
    Shift,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FetchTrace {
    cycle: u64,
    pc: u16,
    opcode: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryEvent {
    DeviceRead {
        device: DeviceRegion,
        addr: u16,
        value: u8,
    },
    RomWrite {
        addr: u16,
        value: u8,
    },
    DeviceWrite {
        device: DeviceRegion,
        addr: u16,
        value: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopReason {
    CycleLimit,
    CpuHalted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReadonlyWrite {
    cycle: u64,
    addr: u16,
    value: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceRegion {
    Pia0,
    Pia1,
    Sam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeviceAccess {
    cycle: u64,
    rw: bool,
    device: DeviceRegion,
    addr: u16,
    value: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HarnessReport {
    stop_reason: StopReason,
    cycles: u64,
    instructions: u64,
    pc: u16,
    addr: u16,
    rw: bool,
    last_fetch: Option<FetchTrace>,
    trace: Vec<FetchTrace>,
    dropped_trace: usize,
    device_accesses: Vec<DeviceAccess>,
    dropped_device_accesses: usize,
    readonly_writes: Vec<ReadonlyWrite>,
    dropped_readonly_writes: usize,
}

#[derive(Debug, Clone)]
struct DragonMemory {
    ram: Box<[u8; RAM_SIZE]>,
    rom: Box<[u8; ROM_SIZE]>,
    pia0: Pia6821,
    pia1: Pia6821,
    keyboard: DragonKeyboard,
}

impl DragonMemory {
    #[cfg(test)]
    fn new(rom: &[u8; ROM_SIZE]) -> Self {
        Self::new_with_keyboard(rom, DragonKeyboard::new())
    }

    fn new_with_keyboard(rom: &[u8; ROM_SIZE], keyboard: DragonKeyboard) -> Self {
        Self {
            ram: Box::new([0; RAM_SIZE]),
            rom: Box::new(*rom),
            pia0: Pia6821::new(),
            pia1: Pia6821::new(),
            keyboard,
        }
    }

    fn read_fetch(&self, addr: u16) -> u8 {
        let addr = usize::from(addr);
        if addr < RAM_SIZE {
            self.ram[addr]
        } else {
            self.rom[(addr - RAM_SIZE) & (ROM_SIZE - 1)]
        }
    }

    fn read_bus(&mut self, addr: u16) -> (u8, Option<MemoryEvent>) {
        if let Some((device, offset)) = decode_pia(addr) {
            self.refresh_pia_inputs();
            let value = match device {
                DeviceRegion::Pia0 => self.pia0.read(offset),
                DeviceRegion::Pia1 => self.pia1.read(offset),
                DeviceRegion::Sam => unreachable!("SAM is not a PIA"),
            };
            return (
                value,
                Some(MemoryEvent::DeviceRead {
                    device,
                    addr,
                    value,
                }),
            );
        }

        (self.read_fetch(addr), None)
    }

    fn write(&mut self, addr: u16, value: u8) -> Option<MemoryEvent> {
        let index = usize::from(addr);
        if index < RAM_SIZE {
            self.ram[index] = value;
            None
        } else if let Some((device, offset)) = decode_pia(addr) {
            match device {
                DeviceRegion::Pia0 => {
                    self.pia0.write(offset, value);
                    self.refresh_pia_inputs();
                }
                DeviceRegion::Pia1 => self.pia1.write(offset, value),
                DeviceRegion::Sam => unreachable!("SAM is not a PIA"),
            }
            Some(MemoryEvent::DeviceWrite {
                device,
                addr,
                value,
            })
        } else if let Some(device) = decode_device_write(addr) {
            Some(MemoryEvent::DeviceWrite {
                device,
                addr,
                value,
            })
        } else {
            Some(MemoryEvent::RomWrite { addr, value })
        }
    }

    fn refresh_pia_inputs(&mut self) {
        let row_select = self.pia0.output_latch(PiaPort::B) | !self.pia0.ddr(PiaPort::B);
        self.pia0
            .set_input(PiaPort::A, self.keyboard.port_a_input(row_select));
        self.pia0.set_input(PiaPort::B, 0xFF);
        self.pia1.set_input(PiaPort::A, 0xFF);
        self.pia1.set_input(PiaPort::B, 0xFF);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DragonKeyboard {
    rows: [u8; 8],
}

impl DragonKeyboard {
    fn new() -> Self {
        Self { rows: [0xFF; 8] }
    }

    fn with_pressed_keys(keys: &[MatrixKey]) -> Result<Self, String> {
        let mut keyboard = Self::new();
        for key in keys {
            keyboard.press(*key)?;
        }
        Ok(keyboard)
    }

    fn press(&mut self, key: MatrixKey) -> Result<(), String> {
        if key.row >= self.rows.len() || key.column >= 8 {
            return Err(format!(
                "keyboard matrix key {},{} is outside the 8x8 matrix",
                key.row, key.column
            ));
        }

        self.rows[key.row] &= !(1 << key.column);
        Ok(())
    }

    fn port_a_input(&self, row_select: u8) -> u8 {
        let selected_rows = !row_select;
        if selected_rows == 0 {
            return 0xFF;
        }

        let mut input = 0xFF;
        for (row, columns) in self.rows.iter().enumerate() {
            if selected_rows & (1 << row) != 0 {
                input &= columns;
            }
        }
        input
    }
}

fn main() {
    if let Err(err) = run_main() {
        eprintln!("{err}");
        process::exit(1);
    }
}

fn run_main() -> Result<(), String> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return Ok(());
    }

    let cli = parse_cli(args)?;
    let rom = load_rom(&cli.rom)?;
    let keyboard = DragonKeyboard::with_pressed_keys(&cli.pressed_keys)?;
    let report = run_harness_with_keyboard(&rom, cli.cycles, cli.trace_limit, keyboard);
    print_report(&report);
    Ok(())
}

fn parse_cli<I>(args: I) -> Result<Cli, String>
where
    I: IntoIterator<Item = String>,
{
    let mut rom = None;
    let mut cycles = DEFAULT_CYCLES;
    let mut trace_limit = DEFAULT_TRACE_LIMIT;
    let mut pressed_keys = Vec::new();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom" => {
                rom = Some(PathBuf::from(next_value(&mut iter, "--rom")?));
            }
            "--cycles" => {
                cycles = parse_u64(&next_value(&mut iter, "--cycles")?, "--cycles")?;
            }
            "--trace-limit" => {
                trace_limit =
                    parse_usize(&next_value(&mut iter, "--trace-limit")?, "--trace-limit")?;
            }
            "--press" => {
                let key = parse_dragon_key(&next_value(&mut iter, "--press")?)?;
                pressed_keys.push(matrix_key_for_dragon_key(key));
            }
            "--press-matrix" => {
                pressed_keys.push(parse_matrix_key(&next_value(&mut iter, "--press-matrix")?)?);
            }
            "--help" | "-h" => return Err(USAGE.to_owned()),
            _ => return Err(format!("unknown argument: {arg}\n\n{USAGE}")),
        }
    }

    Ok(Cli {
        rom: rom.ok_or_else(|| format!("missing required --rom PATH\n\n{USAGE}"))?,
        cycles,
        trace_limit,
        pressed_keys,
    })
}

fn next_value<I>(iter: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    iter.next()
        .ok_or_else(|| format!("{flag} requires a value\n\n{USAGE}"))
}

fn parse_u64(value: &str, flag: &str) -> Result<u64, String> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).map_err(|err| format!("invalid {flag} value {value}: {err}"))
    } else {
        value
            .parse()
            .map_err(|err| format!("invalid {flag} value {value}: {err}"))
    }
}

fn parse_usize(value: &str, flag: &str) -> Result<usize, String> {
    let parsed = parse_u64(value, flag)?;
    usize::try_from(parsed).map_err(|err| format!("{flag} value {value} is too large: {err}"))
}

fn parse_matrix_key(value: &str) -> Result<MatrixKey, String> {
    let (row, column) = value
        .split_once(',')
        .ok_or_else(|| format!("invalid --press-matrix value {value}; expected R,C"))?;
    Ok(MatrixKey {
        row: parse_usize(row, "--press-matrix row")?,
        column: parse_usize(column, "--press-matrix column")?,
    })
}

fn parse_dragon_key(value: &str) -> Result<DragonKey, String> {
    match value {
        "0" => Ok(DragonKey::Digit0),
        "1" => Ok(DragonKey::Digit1),
        "2" => Ok(DragonKey::Digit2),
        "3" => Ok(DragonKey::Digit3),
        "4" => Ok(DragonKey::Digit4),
        "5" => Ok(DragonKey::Digit5),
        "6" => Ok(DragonKey::Digit6),
        "7" => Ok(DragonKey::Digit7),
        "8" => Ok(DragonKey::Digit8),
        "9" => Ok(DragonKey::Digit9),
        ":" => Ok(DragonKey::Colon),
        ";" => Ok(DragonKey::Semicolon),
        "," => Ok(DragonKey::Comma),
        "-" => Ok(DragonKey::Minus),
        "." => Ok(DragonKey::Period),
        "/" => Ok(DragonKey::Slash),
        "@" => Ok(DragonKey::At),
        " " => Ok(DragonKey::Space),
        _ if value.eq_ignore_ascii_case("a") => Ok(DragonKey::A),
        _ if value.eq_ignore_ascii_case("b") => Ok(DragonKey::B),
        _ if value.eq_ignore_ascii_case("c") => Ok(DragonKey::C),
        _ if value.eq_ignore_ascii_case("d") => Ok(DragonKey::D),
        _ if value.eq_ignore_ascii_case("e") => Ok(DragonKey::E),
        _ if value.eq_ignore_ascii_case("f") => Ok(DragonKey::F),
        _ if value.eq_ignore_ascii_case("g") => Ok(DragonKey::G),
        _ if value.eq_ignore_ascii_case("h") => Ok(DragonKey::H),
        _ if value.eq_ignore_ascii_case("i") => Ok(DragonKey::I),
        _ if value.eq_ignore_ascii_case("j") => Ok(DragonKey::J),
        _ if value.eq_ignore_ascii_case("k") => Ok(DragonKey::K),
        _ if value.eq_ignore_ascii_case("l") => Ok(DragonKey::L),
        _ if value.eq_ignore_ascii_case("m") => Ok(DragonKey::M),
        _ if value.eq_ignore_ascii_case("n") => Ok(DragonKey::N),
        _ if value.eq_ignore_ascii_case("o") => Ok(DragonKey::O),
        _ if value.eq_ignore_ascii_case("p") => Ok(DragonKey::P),
        _ if value.eq_ignore_ascii_case("q") => Ok(DragonKey::Q),
        _ if value.eq_ignore_ascii_case("r") => Ok(DragonKey::R),
        _ if value.eq_ignore_ascii_case("s") => Ok(DragonKey::S),
        _ if value.eq_ignore_ascii_case("t") => Ok(DragonKey::T),
        _ if value.eq_ignore_ascii_case("u") => Ok(DragonKey::U),
        _ if value.eq_ignore_ascii_case("v") => Ok(DragonKey::V),
        _ if value.eq_ignore_ascii_case("w") => Ok(DragonKey::W),
        _ if value.eq_ignore_ascii_case("x") => Ok(DragonKey::X),
        _ if value.eq_ignore_ascii_case("y") => Ok(DragonKey::Y),
        _ if value.eq_ignore_ascii_case("z") => Ok(DragonKey::Z),
        _ if value.eq_ignore_ascii_case("up") => Ok(DragonKey::Up),
        _ if value.eq_ignore_ascii_case("down") => Ok(DragonKey::Down),
        _ if value.eq_ignore_ascii_case("left") => Ok(DragonKey::Left),
        _ if value.eq_ignore_ascii_case("right") => Ok(DragonKey::Right),
        _ if value.eq_ignore_ascii_case("space") => Ok(DragonKey::Space),
        _ if value.eq_ignore_ascii_case("enter") || value.eq_ignore_ascii_case("return") => {
            Ok(DragonKey::Enter)
        }
        _ if value.eq_ignore_ascii_case("clear") || value.eq_ignore_ascii_case("clr") => {
            Ok(DragonKey::Clear)
        }
        _ if value.eq_ignore_ascii_case("break") || value.eq_ignore_ascii_case("brk") => {
            Ok(DragonKey::Break)
        }
        _ if value.eq_ignore_ascii_case("shift") => Ok(DragonKey::Shift),
        _ => Err(format!(
            "unknown Dragon key {value:?}; use a Dragon key label such as A, 1, @, enter, clear, break, shift, space, up, down, left, or right"
        )),
    }
}

fn matrix_key_for_dragon_key(key: DragonKey) -> MatrixKey {
    let (row, column) = match key {
        DragonKey::Digit0 => (0, 0),
        DragonKey::Digit1 => (0, 1),
        DragonKey::Digit2 => (0, 2),
        DragonKey::Digit3 => (0, 3),
        DragonKey::Digit4 => (0, 4),
        DragonKey::Digit5 => (0, 5),
        DragonKey::Digit6 => (0, 6),
        DragonKey::Digit7 => (0, 7),
        DragonKey::Digit8 => (1, 0),
        DragonKey::Digit9 => (1, 1),
        DragonKey::Colon => (1, 2),
        DragonKey::Semicolon => (1, 3),
        DragonKey::Comma => (1, 4),
        DragonKey::Minus => (1, 5),
        DragonKey::Period => (1, 6),
        DragonKey::Slash => (1, 7),
        DragonKey::At => (2, 0),
        DragonKey::A => (2, 1),
        DragonKey::B => (2, 2),
        DragonKey::C => (2, 3),
        DragonKey::D => (2, 4),
        DragonKey::E => (2, 5),
        DragonKey::F => (2, 6),
        DragonKey::G => (2, 7),
        DragonKey::H => (3, 0),
        DragonKey::I => (3, 1),
        DragonKey::J => (3, 2),
        DragonKey::K => (3, 3),
        DragonKey::L => (3, 4),
        DragonKey::M => (3, 5),
        DragonKey::N => (3, 6),
        DragonKey::O => (3, 7),
        DragonKey::P => (4, 0),
        DragonKey::Q => (4, 1),
        DragonKey::R => (4, 2),
        DragonKey::S => (4, 3),
        DragonKey::T => (4, 4),
        DragonKey::U => (4, 5),
        DragonKey::V => (4, 6),
        DragonKey::W => (4, 7),
        DragonKey::X => (5, 0),
        DragonKey::Y => (5, 1),
        DragonKey::Z => (5, 2),
        DragonKey::Up => (5, 3),
        DragonKey::Down => (5, 4),
        DragonKey::Left => (5, 5),
        DragonKey::Right => (5, 6),
        DragonKey::Space => (5, 7),
        DragonKey::Enter => (6, 0),
        DragonKey::Clear => (6, 1),
        DragonKey::Break => (6, 2),
        DragonKey::Shift => (6, 7),
    };
    MatrixKey { row, column }
}

fn load_rom(path: &Path) -> Result<[u8; ROM_SIZE], String> {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        return load_rom_from_zip(path);
    }

    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    exact_rom_from_bytes(path, bytes)
}

fn load_rom_from_zip(path: &Path) -> Result<[u8; ROM_SIZE], String> {
    let file =
        fs::File::open(path).map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let mut candidate = None;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| {
            format!(
                "failed to read zip entry {index} in {}: {err}",
                path.display()
            )
        })?;
        if entry.is_dir() {
            continue;
        }

        let entry_name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|err| format!("failed to read {entry_name} from {}: {err}", path.display()))?;

        if bytes.len() == ROM_SIZE {
            if candidate.is_some() {
                return Err(format!(
                    "{} contains multiple {ROM_SIZE}-byte ROM candidates",
                    path.display()
                ));
            }
            candidate = Some(
                bytes
                    .try_into()
                    .map_err(|_| format!("{entry_name} was not exactly {ROM_SIZE} bytes"))?,
            );
        }
    }

    candidate.ok_or_else(|| {
        format!(
            "{} did not contain a {ROM_SIZE}-byte Dragon ROM",
            path.display()
        )
    })
}

fn exact_rom_from_bytes(path: &Path, bytes: Vec<u8>) -> Result<[u8; ROM_SIZE], String> {
    let actual_len = bytes.len();
    bytes.try_into().map_err(|_| {
        format!(
            "{} must be exactly {ROM_SIZE} bytes; got {actual_len}",
            path.display()
        )
    })
}

#[cfg(test)]
fn run_harness(rom: &[u8; ROM_SIZE], cycle_limit: u64, trace_limit: usize) -> HarnessReport {
    run_harness_with_keyboard(rom, cycle_limit, trace_limit, DragonKeyboard::new())
}

fn run_harness_with_keyboard(
    rom: &[u8; ROM_SIZE],
    cycle_limit: u64,
    trace_limit: usize,
    keyboard: DragonKeyboard,
) -> HarnessReport {
    let mut cpu = Mc6809::new();
    let mut memory = DragonMemory::new_with_keyboard(rom, keyboard);
    let mut trace = Vec::new();
    let mut dropped_trace = 0usize;
    let mut device_accesses = Vec::new();
    let mut dropped_device_accesses = 0usize;
    let mut readonly_writes = Vec::new();
    let mut dropped_readonly_writes = 0usize;
    let mut last_fetch = None;
    let mut instructions = 0u64;
    let mut cycles = 0u64;
    let mut stop_reason = StopReason::CycleLimit;

    cpu.reset();

    for cycle in 0..cycle_limit {
        if cpu.instruction_boundary() && cpu.rw {
            let fetch = FetchTrace {
                cycle,
                pc: cpu.addr,
                opcode: memory.read_fetch(cpu.addr),
            };
            last_fetch = Some(fetch);
            instructions = instructions.saturating_add(1);
            retain_trace(&mut trace, &mut dropped_trace, trace_limit, fetch);
        }

        let event = drive_cycle(&mut cpu, &mut memory);
        cycles = cycle.saturating_add(1);

        match event {
            Some(MemoryEvent::DeviceRead {
                device,
                addr,
                value,
            }) => {
                retain_device_access(
                    &mut device_accesses,
                    &mut dropped_device_accesses,
                    trace_limit,
                    DeviceAccess {
                        cycle,
                        rw: true,
                        device,
                        addr,
                        value,
                    },
                );
            }
            Some(MemoryEvent::RomWrite { addr, value }) => {
                retain_readonly_write(
                    &mut readonly_writes,
                    &mut dropped_readonly_writes,
                    trace_limit,
                    ReadonlyWrite { cycle, addr, value },
                );
            }
            Some(MemoryEvent::DeviceWrite {
                device,
                addr,
                value,
            }) => {
                retain_device_access(
                    &mut device_accesses,
                    &mut dropped_device_accesses,
                    trace_limit,
                    DeviceAccess {
                        cycle,
                        rw: false,
                        device,
                        addr,
                        value,
                    },
                );
            }
            None => {}
        }

        if cpu.halt {
            stop_reason = StopReason::CpuHalted;
            break;
        }
    }

    HarnessReport {
        stop_reason,
        cycles,
        instructions,
        pc: cpu.regs.pc,
        addr: cpu.addr,
        rw: cpu.rw,
        last_fetch,
        trace,
        dropped_trace,
        device_accesses,
        dropped_device_accesses,
        readonly_writes,
        dropped_readonly_writes,
    }
}

fn retain_trace(
    trace: &mut Vec<FetchTrace>,
    dropped_trace: &mut usize,
    trace_limit: usize,
    fetch: FetchTrace,
) {
    if trace_limit == 0 {
        *dropped_trace = dropped_trace.saturating_add(1);
        return;
    }

    if trace.len() == trace_limit {
        trace.remove(0);
        *dropped_trace = dropped_trace.saturating_add(1);
    }
    trace.push(fetch);
}

fn retain_device_access(
    accesses: &mut Vec<DeviceAccess>,
    dropped_accesses: &mut usize,
    access_limit: usize,
    access: DeviceAccess,
) {
    if access_limit == 0 {
        *dropped_accesses = dropped_accesses.saturating_add(1);
        return;
    }

    if accesses.len() == access_limit {
        accesses.remove(0);
        *dropped_accesses = dropped_accesses.saturating_add(1);
    }
    accesses.push(access);
}

fn retain_readonly_write(
    writes: &mut Vec<ReadonlyWrite>,
    dropped_writes: &mut usize,
    write_limit: usize,
    write: ReadonlyWrite,
) {
    if write_limit == 0 {
        *dropped_writes = dropped_writes.saturating_add(1);
        return;
    }

    if writes.len() == write_limit {
        writes.remove(0);
        *dropped_writes = dropped_writes.saturating_add(1);
    }
    writes.push(write);
}

fn drive_cycle(cpu: &mut Mc6809, memory: &mut DragonMemory) -> Option<MemoryEvent> {
    let event = if cpu.rw {
        let (value, event) = memory.read_bus(cpu.addr);
        cpu.data_in = value;
        event
    } else {
        memory.write(cpu.addr, cpu.data)
    };
    cpu.tick();
    event
}

fn decode_pia(addr: u16) -> Option<(DeviceRegion, u8)> {
    match addr {
        0xFF00..=0xFF1F => Some((DeviceRegion::Pia0, (addr & 0x03) as u8)),
        0xFF20..=0xFF3F => Some((DeviceRegion::Pia1, (addr & 0x03) as u8)),
        _ => None,
    }
}

fn decode_device_write(addr: u16) -> Option<DeviceRegion> {
    match addr {
        0xFFC0..=0xFFDF => Some(DeviceRegion::Sam),
        _ => None,
    }
}

fn print_report(report: &HarnessReport) {
    println!("dragon harness summary");
    println!("status: {}", format_stop_reason(report.stop_reason));
    println!("cycles: {}", report.cycles);
    println!("instructions: {}", report.instructions);
    println!("pc: ${:04X}", report.pc);
    println!(
        "bus: addr=${:04X} rw={}",
        report.addr,
        if report.rw { "read" } else { "write" }
    );
    if let Some(fetch) = report.last_fetch {
        println!(
            "last fetch: cycle={} pc=${:04X} opcode=${:02X}",
            fetch.cycle, fetch.pc, fetch.opcode
        );
    }
    if report.dropped_trace != 0 {
        println!("trace dropped: {}", report.dropped_trace);
    }
    if report.dropped_device_accesses != 0 {
        println!(
            "device accesses dropped: {}",
            report.dropped_device_accesses
        );
    }
    println!("device accesses:");
    for access in &report.device_accesses {
        println!(
            "  cycle={} {} {} addr=${:04X} value=${:02X}",
            access.cycle,
            if access.rw { "read" } else { "write" },
            format_device_region(access.device),
            access.addr,
            access.value
        );
    }
    if report.dropped_readonly_writes != 0 {
        println!(
            "readonly writes dropped: {}",
            report.dropped_readonly_writes
        );
    }
    println!("readonly writes:");
    for write in &report.readonly_writes {
        println!(
            "  cycle={} addr=${:04X} value=${:02X}",
            write.cycle, write.addr, write.value
        );
    }
    println!("trace:");
    for fetch in &report.trace {
        println!(
            "  cycle={} pc=${:04X} opcode=${:02X}",
            fetch.cycle, fetch.pc, fetch.opcode
        );
    }
}

fn format_stop_reason(reason: StopReason) -> String {
    match reason {
        StopReason::CycleLimit => "cycle-limit".to_owned(),
        StopReason::CpuHalted => "cpu-halted".to_owned(),
    }
}

fn format_device_region(device: DeviceRegion) -> &'static str {
    match device {
        DeviceRegion::Pia0 => "pia0",
        DeviceRegion::Pia1 => "pia1",
        DeviceRegion::Sam => "sam",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn rom_with_reset_vector(pc: u16) -> [u8; ROM_SIZE] {
        let mut rom = [0; ROM_SIZE];
        let [hi, lo] = pc.to_be_bytes();
        rom[0x3FFE] = hi;
        rom[0x3FFF] = lo;
        rom
    }

    #[test]
    fn memory_maps_rom_and_vector_mirror() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0] = 0x12;
        rom[0x3FFE] = 0x80;
        rom[0x3FFF] = 0x00;

        let memory = DragonMemory::new(&rom);

        assert_eq!(memory.read_fetch(0x8000), 0x12);
        assert_eq!(memory.read_fetch(0xC000), 0x12);
        assert_eq!(memory.read_fetch(0xFFFE), 0x80);
        assert_eq!(memory.read_fetch(0xFFFF), 0x00);
    }

    #[test]
    fn harness_records_device_writes_without_stopping() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x86; // LDA #$55
        rom[0x0001] = 0x55;
        rom[0x0002] = 0xB7; // STA $FF00
        rom[0x0003] = 0xFF;
        rom[0x0004] = 0x00;
        rom[0x0005] = 0x01; // Illegal opcode stop after the write.

        let report = run_harness(&rom, 64, 8);

        assert_eq!(report.stop_reason, StopReason::CpuHalted);
        assert_eq!(report.device_accesses.len(), 1);
        assert_eq!(
            report.device_accesses[0],
            DeviceAccess {
                cycle: 7,
                rw: false,
                device: DeviceRegion::Pia0,
                addr: 0xFF00,
                value: 0x55,
            }
        );
    }

    #[test]
    fn harness_records_device_reads_without_stopping() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0xB6; // LDA $FF00
        rom[0x0001] = 0xFF;
        rom[0x0002] = 0x00;
        rom[0x0003] = 0x01;

        let report = run_harness(&rom, 64, 8);

        assert_eq!(report.stop_reason, StopReason::CpuHalted);
        assert_eq!(report.device_accesses.len(), 1);
        assert_eq!(
            report.device_accesses[0],
            DeviceAccess {
                cycle: 5,
                rw: true,
                device: DeviceRegion::Pia0,
                addr: 0xFF00,
                value: 0x00,
            }
        );
        assert_eq!(
            report.last_fetch,
            Some(FetchTrace {
                cycle: 6,
                pc: 0x8003,
                opcode: 0x01,
            })
        );
    }

    #[test]
    fn harness_records_readonly_rom_writes_without_stopping() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x86; // LDA #$55
        rom[0x0001] = 0x55;
        rom[0x0002] = 0xB7; // STA $9000
        rom[0x0003] = 0x90;
        rom[0x0004] = 0x00;
        rom[0x0005] = 0x01;

        let report = run_harness(&rom, 64, 8);

        assert_eq!(report.stop_reason, StopReason::CpuHalted);
        assert_eq!(report.readonly_writes.len(), 1);
        assert_eq!(
            report.readonly_writes[0],
            ReadonlyWrite {
                cycle: 7,
                addr: 0x9000,
                value: 0x55,
            }
        );
    }

    #[test]
    fn keyboard_returns_high_when_no_rows_are_selected() {
        let mut keyboard = DragonKeyboard::new();
        keyboard
            .press(MatrixKey { row: 2, column: 3 })
            .expect("matrix key should be valid");

        assert_eq!(keyboard.port_a_input(0xFF), 0xFF);
    }

    #[test]
    fn keyboard_pulls_column_low_when_selected_row_has_pressed_key() {
        let mut keyboard = DragonKeyboard::new();
        keyboard
            .press(MatrixKey { row: 2, column: 3 })
            .expect("matrix key should be valid");

        assert_eq!(keyboard.port_a_input(0xFB), 0xF7);
    }

    #[test]
    fn keyboard_ands_multiple_selected_rows() {
        let keyboard = DragonKeyboard::with_pressed_keys(&[
            MatrixKey { row: 1, column: 2 },
            MatrixKey { row: 3, column: 5 },
        ])
        .expect("matrix keys should be valid");

        assert_eq!(keyboard.port_a_input(0xF5), 0xDB);
    }

    #[test]
    fn keyboard_rejects_out_of_range_matrix_keys() {
        let err = DragonKeyboard::with_pressed_keys(&[MatrixKey { row: 8, column: 0 }])
            .expect_err("row 8 should be invalid");

        assert!(err.contains("outside the 8x8 matrix"));
    }

    #[test]
    fn memory_feeds_keyboard_matrix_through_pia0_port_a() {
        let rom = rom_with_reset_vector(0x8000);
        let keyboard = DragonKeyboard::with_pressed_keys(&[MatrixKey { row: 0, column: 1 }])
            .expect("matrix key should be valid");
        let mut memory = DragonMemory::new_with_keyboard(&rom, keyboard);

        memory.write(0xFF02, 0xFF); // PIA0 port B DDR: all row-drive bits output.
        memory.write(0xFF03, 0x04); // PIA0 port B data register selected.
        memory.write(0xFF02, 0xFE); // Drive row 0 low.
        memory.write(0xFF01, 0x04); // PIA0 port A data register selected.

        let (value, event) = memory.read_bus(0xFF00);

        assert_eq!(value, 0xFD);
        assert_eq!(
            event,
            Some(MemoryEvent::DeviceRead {
                device: DeviceRegion::Pia0,
                addr: 0xFF00,
                value: 0xFD,
            })
        );
    }

    #[test]
    fn harness_reports_halt_for_illegal_opcode() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x01;

        let report = run_harness(&rom, 16, 4);

        assert_eq!(report.stop_reason, StopReason::CpuHalted);
        assert_eq!(
            report.last_fetch,
            Some(FetchTrace {
                cycle: 2,
                pc: 0x8000,
                opcode: 0x01,
            })
        );
    }

    #[test]
    fn cli_requires_rom_path() {
        let err = parse_cli(Vec::<String>::new()).expect_err("missing ROM should fail");

        assert!(err.contains("missing required --rom"));
    }

    #[test]
    fn cli_parses_hex_cycles_and_trace_limit() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--cycles".to_owned(),
            "0x20".to_owned(),
            "--trace-limit".to_owned(),
            "3".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(cli.rom, PathBuf::from("dragon32.rom"));
        assert_eq!(cli.cycles, 32);
        assert_eq!(cli.trace_limit, 3);
        assert_eq!(cli.pressed_keys, Vec::new());
    }

    #[test]
    fn cli_parses_raw_matrix_key_presses() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--press-matrix".to_owned(),
            "2,3".to_owned(),
            "--press-matrix".to_owned(),
            "4,5".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(
            cli.pressed_keys,
            vec![
                MatrixKey { row: 2, column: 3 },
                MatrixKey { row: 4, column: 5 },
            ]
        );
    }

    #[test]
    fn dragon_key_labels_map_to_confirmed_matrix_positions() {
        assert_eq!(
            matrix_key_for_dragon_key(parse_dragon_key("a").expect("A should parse")),
            MatrixKey { row: 2, column: 1 }
        );
        assert_eq!(
            matrix_key_for_dragon_key(parse_dragon_key("A").expect("A should parse")),
            MatrixKey { row: 2, column: 1 }
        );
        assert_eq!(
            matrix_key_for_dragon_key(parse_dragon_key("@").expect("@ should parse")),
            MatrixKey { row: 2, column: 0 }
        );
        assert_eq!(
            matrix_key_for_dragon_key(parse_dragon_key("space").expect("space should parse")),
            MatrixKey { row: 5, column: 7 }
        );
        assert_eq!(
            matrix_key_for_dragon_key(parse_dragon_key("right").expect("right should parse")),
            MatrixKey { row: 5, column: 6 }
        );
    }

    #[test]
    fn dragon_key_parser_accepts_control_key_aliases() {
        assert_eq!(parse_dragon_key("return"), Ok(DragonKey::Enter));
        assert_eq!(parse_dragon_key("clr"), Ok(DragonKey::Clear));
        assert_eq!(parse_dragon_key("brk"), Ok(DragonKey::Break));
    }

    #[test]
    fn cli_parses_named_dragon_key_presses_semantically() {
        let cli = parse_cli([
            "--rom".to_owned(),
            "dragon32.rom".to_owned(),
            "--press".to_owned(),
            "A".to_owned(),
            "--press".to_owned(),
            "@".to_owned(),
            "--press".to_owned(),
            "enter".to_owned(),
        ])
        .expect("valid CLI should parse");

        assert_eq!(
            cli.pressed_keys,
            vec![
                MatrixKey { row: 2, column: 1 },
                MatrixKey { row: 2, column: 0 },
                MatrixKey { row: 6, column: 0 },
            ]
        );
    }

    #[test]
    fn load_rom_accepts_zip_archives() {
        let rom = rom_with_reset_vector(0x8000);
        let path = env::temp_dir().join(format!(
            "emu198x-dragon-rom-test-{}.zip",
            std::process::id()
        ));

        let file = fs::File::create(&path).expect("test zip should be creatable");
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("dragon32.rom", zip::write::SimpleFileOptions::default())
            .expect("zip entry should start");
        zip.write_all(&rom).expect("zip entry should be writable");
        zip.finish().expect("zip should finish");

        let loaded = load_rom(&path).expect("zip ROM should load");
        fs::remove_file(&path).expect("test zip should be removable");

        assert_eq!(loaded, rom);
    }
}
