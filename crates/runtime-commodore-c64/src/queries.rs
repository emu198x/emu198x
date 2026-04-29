//! Family-owned query surface for the C64 runtime.
//!
//! Splits the SessionQueryProvider impl out of `runtime.rs` so the
//! 350-odd query paths and their handlers don't dominate the file.
//! The provider itself is stateless (`C64SessionQueryProvider`); all
//! the lookup logic lives here.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use serde_json::json;

use crate::runtime::{C64Runtime, SCREEN_RAM_BASE, SCREEN_TEXT_HEIGHT, SCREEN_TEXT_WIDTH};
use machine_commodore_c64::C64;

/// Every path the C64 runtime answers via `query()`. Wildcard paths
/// like `c64.memory.ram.<hex16>` are listed with the placeholder name
/// so `query_paths` returns a clean catalogue.
pub(crate) const C64_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.row",
    "boot.reason",
    "boot.offset",
    "c64.cpu.a",
    "c64.cpu.addr",
    "c64.cpu.data",
    "c64.cpu.irq",
    "c64.cpu.instruction_complete",
    "c64.cpu.nmi",
    "c64.cpu.p",
    "c64.cpu.pc",
    "c64.cpu.rdy",
    "c64.cpu.rw",
    "c64.cpu.sp",
    "c64.cpu.sync",
    "c64.cpu.total_cycles",
    "c64.cpu.x",
    "c64.cpu.y",
    "c64.cia1.flag",
    "c64.cia1.irq",
    "c64.cia1.icr_mask",
    "c64.cia1.icr_status",
    "c64.cia1.timer_a",
    "c64.cia1.timer_a_latch",
    "c64.cia1.timer_b",
    "c64.cia1.timer_b_latch",
    "c64.cia2.irq",
    "c64.cia2.pa",
    "c64.cia2.pb",
    "c64.cia2.port_a_latch",
    "c64.cia2.port_b_latch",
    "c64.cia2.ddra",
    "c64.cia2.ddrb",
    "c64.cia2.port_a_drive_state",
    "c64.cia2.port_b_drive_state",
    "c64.cia2.cra",
    "c64.cia2.crb",
    "c64.cia2.icr_mask",
    "c64.cia2.icr_status",
    "c64.cia2.timer_a",
    "c64.cia2.timer_a_latch",
    "c64.cia2.timer_b",
    "c64.cia2.timer_b_latch",
    "c64.drive8.attached",
    "c64.drive8.cpu.addr",
    "c64.drive8.cpu.cycles",
    "c64.drive8.cpu.data",
    "c64.drive8.cpu.instruction_complete",
    "c64.drive8.cpu.p",
    "c64.drive8.cpu.pc",
    "c64.drive8.cpu.rw",
    "c64.drive8.cpu.sp",
    "c64.drive8.cpu.sync",
    "c64.drive8.cpu.x",
    "c64.drive8.cpu.y",
    "c64.drive8.via1.irq",
    "c64.drive8.via1.ca1",
    "c64.drive8.via1.pa",
    "c64.drive8.via1.pb",
    "c64.drive8.via1.ora",
    "c64.drive8.via1.orb",
    "c64.drive8.via1.ddra",
    "c64.drive8.via1.ddrb",
    "c64.drive8.via1.acr",
    "c64.drive8.via1.pcr",
    "c64.drive8.via1.t1_counter",
    "c64.drive8.via1.t1_latch",
    "c64.drive8.via2.irq",
    "c64.drive8.via2.ca1",
    "c64.drive8.via2.pa",
    "c64.drive8.via2.pb",
    "c64.drive8.via2.ora",
    "c64.drive8.via2.orb",
    "c64.drive8.via2.ddra",
    "c64.drive8.via2.ddrb",
    "c64.drive8.via2.acr",
    "c64.drive8.via2.pcr",
    "c64.drive8.gcr_read",
    "c64.drive8.byte_ready",
    "c64.drive8.byte_ready_events",
    "c64.drive8.sync_detected",
    "c64.drive8.sync_events",
    "c64.drive8.motor_on",
    "c64.drive8.activity_led",
    "c64.drive8.head_position",
    "c64.drive8.density_code",
    "c64.drive8.disk.inserted",
    "c64.drive8.disk.name",
    "c64.drive8.disk.id",
    "c64.drive8.disk.write_protected",
    "c64.drive8.disk.directory",
    "c64.drive8.trace.recent_writes",
    "c64.drive8.mem.<hex16>",
    "c64.iec.cpu_port",
    "c64.iec.drive_port",
    "c64.memory.effective_port",
    "c64.memory.io_visible",
    "c64.memory.port_data",
    "c64.memory.port_ddr",
    "c64.machine.cycle_in_line",
    "c64.machine.frame_count",
    "c64.memory.ram.<hex16>",
    "c64.machine.raster_line",
    "c64.tape.loaded",
    "c64.tape.motor_on",
    "c64.tape.pulse_count",
    "c64.tape.pulse_index",
    "c64.tape.playing",
    "c64.tape.sense",
    "c64.vic.background_colour",
    "c64.vic.ba_low",
    "c64.vic.border_colour",
    "c64.vic.irq",
    "screen.text.lines",
];

/// Boot-status heuristic used by the `boot.*` query paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct C64BootStatus {
    pub detected: bool,
    pub reason: String,
    pub offset: Option<u16>,
    pub row: Option<u64>,
}

/// C64-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct C64SessionQueryProvider;

impl SessionQueryProvider<C64Runtime> for C64SessionQueryProvider {
    fn query_paths(&self, _machine: &C64Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = C64_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &C64Runtime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let boot = c64_boot_status(machine.machine());

        let value = match path {
            "boot.detected" => json!(boot.detected),
            "boot.row" => json!(boot.row),
            "boot.reason" => json!(boot.reason),
            "boot.offset" => json!(boot.offset),
            "c64.cpu.a" => json!(machine.machine().cpu().regs.a),
            "c64.cpu.addr" => json!(machine.machine().cpu().addr),
            "c64.cpu.data" => json!(machine.machine().cpu().data),
            "c64.cpu.irq" => json!(machine.machine().cpu().irq),
            "c64.cpu.instruction_complete" => {
                json!(machine.machine().cpu().instruction_complete())
            }
            "c64.cpu.nmi" => json!(machine.machine().cpu().nmi),
            "c64.cpu.p" => json!(machine.machine().cpu().regs.p),
            "c64.cpu.pc" => json!(machine.machine().cpu().regs.pc),
            "c64.cpu.rdy" => json!(machine.machine().cpu().rdy),
            "c64.cpu.rw" => json!(machine.machine().cpu().rw),
            "c64.cpu.sp" => json!(machine.machine().cpu().regs.sp),
            "c64.cpu.sync" => json!(machine.machine().cpu().sync),
            "c64.cpu.total_cycles" => json!(machine.machine().cpu().total_cycles),
            "c64.cpu.x" => json!(machine.machine().cpu().regs.x),
            "c64.cpu.y" => json!(machine.machine().cpu().regs.y),
            "c64.cia1.flag" => json!(machine.machine().cia1().flag),
            "c64.cia1.icr_mask" => json!(machine.machine().cia1().icr_mask()),
            "c64.cia1.icr_status" => json!(machine.machine().cia1().icr_status()),
            "c64.cia1.timer_a" => json!(machine.machine().cia1().timer_a()),
            "c64.cia1.timer_a_latch" => json!(machine.machine().cia1().timer_a_latch()),
            "c64.cia1.timer_b" => json!(machine.machine().cia1().timer_b()),
            "c64.cia1.timer_b_latch" => json!(machine.machine().cia1().timer_b_latch()),
            "c64.cia2.cra" => json!(machine.machine().cia2().cra()),
            "c64.cia2.crb" => json!(machine.machine().cia2().crb()),
            "c64.cia2.icr_mask" => json!(machine.machine().cia2().icr_mask()),
            "c64.cia2.icr_status" => json!(machine.machine().cia2().icr_status()),
            "c64.cia2.pa" => json!(machine.machine().cia2().pa),
            "c64.cia2.pb" => json!(machine.machine().cia2().pb),
            "c64.cia2.port_a_latch" => json!(machine.machine().cia2().port_a_latch()),
            "c64.cia2.port_b_latch" => json!(machine.machine().cia2().port_b_latch()),
            "c64.cia2.ddra" => json!(machine.machine().cia2().ddr_a()),
            "c64.cia2.ddrb" => json!(machine.machine().cia2().ddr_b()),
            "c64.cia2.port_a_drive_state" => {
                json!(machine.machine().cia2().port_a_drive_state())
            }
            "c64.cia2.port_b_drive_state" => {
                json!(machine.machine().cia2().port_b_drive_state())
            }
            "c64.cia2.timer_a" => json!(machine.machine().cia2().timer_a()),
            "c64.cia2.timer_a_latch" => json!(machine.machine().cia2().timer_a_latch()),
            "c64.cia2.timer_b" => json!(machine.machine().cia2().timer_b()),
            "c64.cia2.timer_b_latch" => json!(machine.machine().cia2().timer_b_latch()),
            "c64.drive8.attached" => json!(machine.drive8().is_some()),
            "c64.drive8.cpu.addr" => json!(machine.drive8().map(|drive| drive.cpu().addr)),
            "c64.drive8.cpu.cycles" => json!(machine.drive8().map(|drive| drive.cycles())),
            "c64.drive8.cpu.data" => json!(machine.drive8().map(|drive| drive.cpu().data)),
            "c64.drive8.cpu.instruction_complete" => {
                json!(
                    machine
                        .drive8()
                        .map(|drive| drive.cpu().instruction_complete())
                )
            }
            "c64.drive8.cpu.p" => json!(machine.drive8().map(|drive| drive.cpu().regs.p)),
            "c64.drive8.cpu.pc" => json!(machine.drive8().map(|drive| drive.cpu().regs.pc)),
            "c64.drive8.cpu.rw" => json!(machine.drive8().map(|drive| drive.cpu().rw)),
            "c64.drive8.cpu.sp" => json!(machine.drive8().map(|drive| drive.cpu().regs.sp)),
            "c64.drive8.cpu.sync" => json!(machine.drive8().map(|drive| drive.cpu().sync)),
            "c64.drive8.cpu.x" => json!(machine.drive8().map(|drive| drive.cpu().regs.x)),
            "c64.drive8.cpu.y" => json!(machine.drive8().map(|drive| drive.cpu().regs.y)),
            "c64.drive8.via1.irq" => json!(machine.drive8().map(|drive| drive.via1().irq)),
            "c64.drive8.via1.ca1" => json!(machine.drive8().map(|drive| drive.via1().ca1)),
            "c64.drive8.via1.pa" => json!(machine.drive8().map(|drive| drive.via1().pa)),
            "c64.drive8.via1.pb" => json!(machine.drive8().map(|drive| drive.via1().pb)),
            "c64.drive8.via1.ora" => json!(machine.drive8().map(|drive| drive.via1().ora())),
            "c64.drive8.via1.orb" => json!(machine.drive8().map(|drive| drive.via1().orb())),
            "c64.drive8.via1.ddra" => {
                json!(machine.drive8().map(|drive| drive.via1().ddra()))
            }
            "c64.drive8.via1.ddrb" => {
                json!(machine.drive8().map(|drive| drive.via1().ddrb()))
            }
            "c64.drive8.via1.acr" => json!(machine.drive8().map(|drive| drive.via1().peek(0x0B))),
            "c64.drive8.via1.pcr" => json!(machine.drive8().map(|drive| drive.via1().peek(0x0C))),
            "c64.drive8.via1.t1_counter" => json!(machine.drive8().map(|drive| {
                u16::from(drive.via1().peek(0x04)) | (u16::from(drive.via1().peek(0x05)) << 8)
            })),
            "c64.drive8.via1.t1_latch" => json!(machine.drive8().map(|drive| {
                u16::from(drive.via1().peek(0x06)) | (u16::from(drive.via1().peek(0x07)) << 8)
            })),
            "c64.drive8.via2.irq" => json!(machine.drive8().map(|drive| drive.via2().irq)),
            "c64.drive8.via2.ca1" => json!(machine.drive8().map(|drive| drive.via2().ca1)),
            "c64.drive8.via2.pa" => json!(machine.drive8().map(|drive| drive.via2().pa)),
            "c64.drive8.via2.pb" => json!(machine.drive8().map(|drive| drive.via2().pb)),
            "c64.drive8.via2.ora" => json!(machine.drive8().map(|drive| drive.via2().ora())),
            "c64.drive8.via2.orb" => json!(machine.drive8().map(|drive| drive.via2().orb())),
            "c64.drive8.via2.ddra" => {
                json!(machine.drive8().map(|drive| drive.via2().ddra()))
            }
            "c64.drive8.via2.ddrb" => {
                json!(machine.drive8().map(|drive| drive.via2().ddrb()))
            }
            "c64.drive8.via2.acr" => json!(machine.drive8().map(|drive| drive.via2().peek(0x0B))),
            "c64.drive8.via2.pcr" => json!(machine.drive8().map(|drive| drive.via2().peek(0x0C))),
            "c64.drive8.gcr_read" => json!(machine.drive8().map(|drive| drive.gcr_read())),
            "c64.drive8.byte_ready" => json!(machine.drive8().map(|drive| drive.byte_ready())),
            "c64.drive8.byte_ready_events" => {
                json!(machine.drive8().map(|drive| drive.byte_ready_event_count()))
            }
            "c64.drive8.sync_detected" => {
                json!(machine.drive8().map(|drive| drive.sync_detected()))
            }
            "c64.drive8.sync_events" => {
                json!(machine.drive8().map(|drive| drive.sync_event_count()))
            }
            "c64.drive8.motor_on" => json!(machine.drive8().map(|drive| drive.motor_on())),
            "c64.drive8.activity_led" => {
                json!(machine.drive8().map(|drive| drive.activity_led()))
            }
            "c64.drive8.head_position" => {
                json!(machine.drive8().map(|drive| drive.head_position()))
            }
            "c64.drive8.density_code" => {
                json!(machine.drive8().map(|drive| drive.density_code()))
            }
            "c64.drive8.disk.inserted" => {
                json!(machine.drive8().is_some_and(|drive| drive.disk_inserted()))
            }
            "c64.drive8.disk.name" => json!(
                machine
                    .drive8()
                    .and_then(|drive| drive.disk())
                    .map(|disk| disk.disk_name())
            ),
            "c64.drive8.disk.id" => json!(
                machine
                    .drive8()
                    .and_then(|drive| drive.disk())
                    .map(|disk| disk.disk_id())
            ),
            "c64.drive8.disk.write_protected" => json!(
                machine
                    .drive8()
                    .and_then(|drive| drive.disk())
                    .map(|disk| disk.write_protected())
            ),
            "c64.drive8.disk.directory" => json!(
                machine
                    .drive8()
                    .and_then(|drive| drive.disk())
                    .map(|disk| disk.directory_entries())
            ),
            "c64.drive8.trace.recent_writes" => {
                json!(machine.drive8().map(|drive| drive.recent_io_writes()))
            }
            "c64.iec.cpu_port" => json!(machine.iec_bus().cpu_port()),
            "c64.iec.drive_port" => json!(machine.iec_bus().drive_port()),
            "c64.memory.effective_port" => json!(machine.machine().memory().effective_port()),
            "c64.memory.io_visible" => json!(machine.machine().memory().is_io_visible()),
            "c64.memory.port_data" => json!(machine.machine().memory().port_data()),
            "c64.memory.port_ddr" => json!(machine.machine().memory().port_ddr()),
            "c64.machine.raster_line" => json!(machine.machine().raster_line()),
            "c64.machine.cycle_in_line" => json!(machine.machine().cycle_in_line()),
            "c64.machine.frame_count" => json!(machine.machine().frame_count()),
            "c64.tape.loaded" => json!(machine.machine().tape_is_loaded()),
            "c64.tape.motor_on" => json!(machine.machine().tape_motor_on()),
            "c64.tape.pulse_count" => json!(machine.machine().tape_pulse_count()),
            "c64.tape.pulse_index" => json!(machine.machine().tape_pulse_index()),
            "c64.tape.playing" => json!(machine.machine().tape_is_playing()),
            "c64.tape.sense" => json!(machine.machine().tape_sense_active()),
            "c64.vic.background_colour" => json!(machine.machine().vic_register(0x21) & 0x0F),
            "c64.vic.ba_low" => json!(machine.machine().vic().ba_is_low()),
            "c64.vic.border_colour" => json!(machine.machine().vic_register(0x20) & 0x0F),
            "c64.vic.irq" => json!(machine.machine().vic().irq_active()),
            "c64.cia1.irq" => json!(machine.machine().cia1().irq_active()),
            "c64.cia2.irq" => json!(machine.machine().cia2().irq_active()),
            "screen.text.lines" => json!(decode_screen_text_lines(machine.machine())),
            _ if path.starts_with("c64.memory.ram.") => {
                let suffix = &path["c64.memory.ram.".len()..];
                let addr = parse_hex_u16(suffix).ok_or_else(|| QueryError::UnknownPath {
                    path: path.to_owned(),
                })?;
                json!(machine.machine().memory().ram_read(addr))
            }
            _ if path.starts_with("c64.drive8.mem.") => {
                let suffix = &path["c64.drive8.mem.".len()..];
                let addr = parse_hex_u16(suffix).ok_or_else(|| QueryError::UnknownPath {
                    path: path.to_owned(),
                })?;
                json!(
                    machine
                        .drive8()
                        .map(|drive| drive.peek_with_iec_bus(addr, machine.iec_bus()))
                )
            }
            _ => return Ok(None),
        };

        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

/// Parse a hex `u16` from a query suffix; accepts optional `0x`/`0X`.
pub(crate) fn parse_hex_u16(value: &str) -> Option<u16> {
    let trimmed = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    u16::from_str_radix(trimmed, 16).ok()
}

/// Screen codes for "READY." — the text the C64 KERNAL prints once
/// the BASIC interpreter is up.
const READY_SCREEN_CODES: [u8; 6] = [18, 5, 1, 4, 25, 46];

/// Boot-status heuristic: scan the screen RAM for "READY." codes and
/// report the offset / row where they appear.
pub(crate) fn c64_boot_status(machine: &C64) -> C64BootStatus {
    let end = 0x07E8u16 - 0x0400u16 - READY_SCREEN_CODES.len() as u16;
    for offset in 0..=end {
        let mut matched = true;
        for (index, expected) in READY_SCREEN_CODES.iter().copied().enumerate() {
            if machine.memory().ram_read(0x0400 + offset + index as u16) != expected {
                matched = false;
                break;
            }
        }

        if matched {
            let row = u64::from(offset / SCREEN_TEXT_WIDTH as u16);
            return C64BootStatus {
                detected: true,
                reason: format!("found READY. screen codes at offset ${offset:04X} on row {row}"),
                offset: Some(offset),
                row: Some(row),
            };
        }
    }

    C64BootStatus {
        detected: false,
        reason: "READY. screen codes not visible".to_owned(),
        offset: None,
        row: None,
    }
}

/// Read the 25×40 character matrix at $0400 and decode it to text.
pub(crate) fn decode_screen_text_lines(machine: &C64) -> Vec<String> {
    let mut lines = Vec::with_capacity(SCREEN_TEXT_HEIGHT);
    for row in 0..SCREEN_TEXT_HEIGHT {
        let mut line = String::with_capacity(SCREEN_TEXT_WIDTH);
        for col in 0..SCREEN_TEXT_WIDTH {
            let address = SCREEN_RAM_BASE + (row * SCREEN_TEXT_WIDTH + col) as u16;
            let code = machine.memory().ram_read(address);
            line.push(decode_screen_code(code));
        }
        lines.push(line);
    }
    lines
}

/// Map one C64 screen code to its ASCII character (best-effort).
pub(crate) fn decode_screen_code(code: u8) -> char {
    match code {
        0x00 => '@',
        0x01..=0x1A => char::from(b'A' + (code - 1)),
        0x20 => ' ',
        0x21..=0x3F => char::from(code),
        0x40..=0x5A => char::from(code),
        0x5B => '[',
        0x5C => '\\',
        0x5D => ']',
        0x5E => '^',
        0x5F => '_',
        0x60 => '`',
        0x61..=0x7A => char::from(code - 0x20),
        _ => '?',
    }
}
