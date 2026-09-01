//! Family-owned query surface for the C64 runtime.
//!
//! Splits the SessionQueryProvider impl out of `runtime.rs` so the
//! 350-odd query paths and their handlers don't dominate the file.
//! The provider itself is stateless (`C64SessionQueryProvider`); all
//! the lookup logic lives here.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use serde_json::json;

use crate::runtime::{C64Runtime, SCREEN_RAM_BASE, SCREEN_TEXT_HEIGHT, SCREEN_TEXT_WIDTH};
use emu198x_esp_at_modem::EspAtTcpBridge;
use machine_commodore_c64::C64;

/// Every path the C64 runtime answers via `query()`. Wildcard paths
/// like `c64.memory.ram.<hex16>` are listed with the placeholder name
/// so `query_paths` returns a clean catalogue.
pub(crate) const C64_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.row",
    "boot.reason",
    "boot.offset",
    "cpu.a",
    "cpu.addr",
    "cpu.data",
    "cpu.data_in",
    "cpu.irq",
    "cpu.instruction_complete",
    "cpu.nmi",
    "cpu.p",
    "cpu.pc",
    "cpu.rdy",
    "cpu.rw",
    "cpu.sp",
    "cpu.sync",
    "cpu.total_cycles",
    "cpu.x",
    "cpu.y",
    "cia1.flag",
    "cia1.irq",
    "cia1.icr_mask",
    "cia1.icr_status",
    "cia1.timer_a",
    "cia1.timer_a_latch",
    "cia1.timer_b",
    "cia1.timer_b_latch",
    "cia2.irq",
    "cia2.pa",
    "cia2.pb",
    "cia2.port_a_latch",
    "cia2.port_b_latch",
    "cia2.ddra",
    "cia2.ddrb",
    "cia2.port_a_drive_state",
    "cia2.port_b_drive_state",
    "cia2.cra",
    "cia2.crb",
    "cia2.icr_mask",
    "cia2.icr_status",
    "cia2.timer_a",
    "cia2.timer_a_latch",
    "cia2.timer_b",
    "cia2.timer_b_latch",
    "userport.esp_at.attached",
    "drive8.attached",
    "drive8.cpu.addr",
    "drive8.cpu.cycles",
    "drive8.cpu.data",
    "drive8.cpu.instruction_complete",
    "drive8.cpu.p",
    "drive8.cpu.pc",
    "drive8.cpu.rw",
    "drive8.cpu.sp",
    "drive8.cpu.sync",
    "drive8.cpu.x",
    "drive8.cpu.y",
    "drive8.via1.irq",
    "drive8.via1.ca1",
    "drive8.via1.pa",
    "drive8.via1.pb",
    "drive8.via1.ora",
    "drive8.via1.orb",
    "drive8.via1.ddra",
    "drive8.via1.ddrb",
    "drive8.via1.acr",
    "drive8.via1.pcr",
    "drive8.via1.t1_counter",
    "drive8.via1.t1_latch",
    "drive8.via2.irq",
    "drive8.via2.ca1",
    "drive8.via2.pa",
    "drive8.via2.pb",
    "drive8.via2.ora",
    "drive8.via2.orb",
    "drive8.via2.ddra",
    "drive8.via2.ddrb",
    "drive8.via2.acr",
    "drive8.via2.pcr",
    "drive8.gcr_read",
    "drive8.byte_ready",
    "drive8.byte_ready_events",
    "drive8.sync_detected",
    "drive8.sync_events",
    "drive8.motor_on",
    "drive8.activity_led",
    "drive8.head_position",
    "drive8.density_code",
    "drive8.disk.inserted",
    "drive8.disk.name",
    "drive8.disk.id",
    "drive8.disk.write_protected",
    "drive8.disk.directory",
    "drive8.trace.recent_writes",
    "drive8.mem.<hex16>",
    "iec.cpu_port",
    "iec.drive_port",
    "memory.effective_port",
    "memory.io_visible",
    "memory.port_data",
    "memory.port_ddr",
    "machine.cycle_in_line",
    "machine.frame_count",
    "memory.ram.<hex16>",
    "machine.raster_line",
    "tape.loaded",
    "tape.motor_on",
    "tape.pulse_count",
    "tape.pulse_index",
    "tape.playing",
    "tape.sense",
    "vic.background_colour",
    "vic.aec_low",
    "vic.badline",
    "vic.badline_ba_low",
    "vic.ba_low",
    "vic.ba_low_cycles",
    "vic.border_colour",
    "vic.c_access_active",
    "vic.cpu_stalled",
    "vic.forced_badline_cdata_carry_age",
    "vic.forced_badline_cdata_carry_cycles_remaining",
    "vic.forced_badline_cdata_carry_pending",
    "vic.forced_badline_cdata_carry_slot",
    "vic.forced_badline_cdata_carry_value",
    "vic.forced_badline_cdata_destination_vmli",
    "vic.forced_badline_cdata_eligibility_cycles_remaining",
    "vic.forced_badline_output_delay",
    "vic.idle_state",
    "vic.irq",
    "vic.last_bus_data",
    "vic.late_badline_window",
    "vic.late_badline_fetches_remaining",
    "vic.pending_d011_write_cycle",
    "vic.rc",
    "vic.sprite_ba_low",
    "vic.vc",
    "vic.vcbase",
    "vic.vmli",
    "screen.text.lines",
];

/// Where the C64 mounts the ESP-AT modem's own query leaves. The peripheral
/// owns the leaf names ([`EspAtTcpBridge::QUERY_LEAVES`]); the runtime owns
/// only this mount point, and advertises the leaves solely while the modem is
/// plugged into the user port.
const ESP_AT_QUERY_PREFIX: &str = "userport.esp_at.";

/// Paths owned by whichever optional peripherals are currently attached.
///
/// Kept apart from [`C64_QUERY_PATHS`] because these come and go with the
/// hardware: attaching a peripheral registers its paths and detaching it
/// deregisters them, so a catalogue never advertises a path that cannot answer.
fn attached_peripheral_query_paths(machine: &C64Runtime) -> Vec<String> {
    let Some(_bridge) = machine.esp_at_tcp_bridge() else {
        return Vec::new();
    };
    EspAtTcpBridge::QUERY_LEAVES
        .iter()
        .map(|leaf| format!("{ESP_AT_QUERY_PREFIX}{leaf}"))
        .collect()
}

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
    fn query_paths(&self, machine: &C64Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = C64_QUERY_PATHS
            .iter()
            .copied()
            .map(str::to_owned)
            .chain(attached_peripheral_query_paths(machine))
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
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
            "cpu.a" => json!(machine.machine().cpu().regs.a),
            "cpu.addr" => json!(machine.machine().cpu().addr),
            "cpu.data" => json!(machine.machine().cpu().data),
            "cpu.data_in" => json!(machine.machine().cpu().data_in),
            "cpu.irq" => json!(machine.machine().cpu().irq),
            "cpu.instruction_complete" => {
                json!(machine.machine().cpu().instruction_complete())
            }
            "cpu.nmi" => json!(machine.machine().cpu().nmi),
            "cpu.p" => json!(machine.machine().cpu().regs.p),
            "cpu.pc" => json!(machine.machine().cpu().regs.pc),
            "cpu.rdy" => json!(machine.machine().cpu().rdy),
            "cpu.rw" => json!(machine.machine().cpu().rw),
            "cpu.sp" => json!(machine.machine().cpu().regs.sp),
            "cpu.sync" => json!(machine.machine().cpu().sync),
            "cpu.total_cycles" => json!(machine.machine().cpu().total_cycles),
            "cpu.x" => json!(machine.machine().cpu().regs.x),
            "cpu.y" => json!(machine.machine().cpu().regs.y),
            "cia1.flag" => json!(machine.machine().cia1().flag),
            "cia1.icr_mask" => json!(machine.machine().cia1().icr_mask()),
            "cia1.icr_status" => json!(machine.machine().cia1().icr_status()),
            "cia1.timer_a" => json!(machine.machine().cia1().timer_a()),
            "cia1.timer_a_latch" => json!(machine.machine().cia1().timer_a_latch()),
            "cia1.timer_b" => json!(machine.machine().cia1().timer_b()),
            "cia1.timer_b_latch" => json!(machine.machine().cia1().timer_b_latch()),
            "cia2.cra" => json!(machine.machine().cia2().cra()),
            "cia2.crb" => json!(machine.machine().cia2().crb()),
            "cia2.icr_mask" => json!(machine.machine().cia2().icr_mask()),
            "cia2.icr_status" => json!(machine.machine().cia2().icr_status()),
            "cia2.pa" => json!(machine.machine().cia2().pa),
            "cia2.pb" => json!(machine.machine().cia2().pb),
            "cia2.port_a_latch" => json!(machine.machine().cia2().port_a_latch()),
            "cia2.port_b_latch" => json!(machine.machine().cia2().port_b_latch()),
            "cia2.ddra" => json!(machine.machine().cia2().ddr_a()),
            "cia2.ddrb" => json!(machine.machine().cia2().ddr_b()),
            "cia2.port_a_drive_state" => {
                json!(machine.machine().cia2().port_a_drive_state())
            }
            "cia2.port_b_drive_state" => {
                json!(machine.machine().cia2().port_b_drive_state())
            }
            "cia2.timer_a" => json!(machine.machine().cia2().timer_a()),
            "cia2.timer_a_latch" => json!(machine.machine().cia2().timer_a_latch()),
            "cia2.timer_b" => json!(machine.machine().cia2().timer_b()),
            "cia2.timer_b_latch" => json!(machine.machine().cia2().timer_b_latch()),
            "userport.esp_at.attached" => json!(machine.esp_at_tcp_bridge().is_some()),
            "drive8.attached" => json!(machine.drive8().is_some()),
            "drive8.cpu.addr" => json!(machine.drive8().map(|drive| drive.cpu().addr)),
            "drive8.cpu.cycles" => json!(machine.drive8().map(|drive| drive.cycles())),
            "drive8.cpu.data" => json!(machine.drive8().map(|drive| drive.cpu().data)),
            "drive8.cpu.instruction_complete" => {
                json!(
                    machine
                        .drive8()
                        .map(|drive| drive.cpu().instruction_complete())
                )
            }
            "drive8.cpu.p" => json!(machine.drive8().map(|drive| drive.cpu().regs.p)),
            "drive8.cpu.pc" => json!(machine.drive8().map(|drive| drive.cpu().regs.pc)),
            "drive8.cpu.rw" => json!(machine.drive8().map(|drive| drive.cpu().rw)),
            "drive8.cpu.sp" => json!(machine.drive8().map(|drive| drive.cpu().regs.sp)),
            "drive8.cpu.sync" => json!(machine.drive8().map(|drive| drive.cpu().sync)),
            "drive8.cpu.x" => json!(machine.drive8().map(|drive| drive.cpu().regs.x)),
            "drive8.cpu.y" => json!(machine.drive8().map(|drive| drive.cpu().regs.y)),
            "drive8.via1.irq" => json!(machine.drive8().map(|drive| drive.via1().irq)),
            "drive8.via1.ca1" => json!(machine.drive8().map(|drive| drive.via1().ca1)),
            "drive8.via1.pa" => json!(machine.drive8().map(|drive| drive.via1().pa)),
            "drive8.via1.pb" => json!(machine.drive8().map(|drive| drive.via1().pb)),
            "drive8.via1.ora" => json!(machine.drive8().map(|drive| drive.via1().ora())),
            "drive8.via1.orb" => json!(machine.drive8().map(|drive| drive.via1().orb())),
            "drive8.via1.ddra" => {
                json!(machine.drive8().map(|drive| drive.via1().ddra()))
            }
            "drive8.via1.ddrb" => {
                json!(machine.drive8().map(|drive| drive.via1().ddrb()))
            }
            "drive8.via1.acr" => json!(machine.drive8().map(|drive| drive.via1().peek(0x0B))),
            "drive8.via1.pcr" => json!(machine.drive8().map(|drive| drive.via1().peek(0x0C))),
            "drive8.via1.t1_counter" => json!(machine.drive8().map(|drive| {
                u16::from(drive.via1().peek(0x04)) | (u16::from(drive.via1().peek(0x05)) << 8)
            })),
            "drive8.via1.t1_latch" => json!(machine.drive8().map(|drive| {
                u16::from(drive.via1().peek(0x06)) | (u16::from(drive.via1().peek(0x07)) << 8)
            })),
            "drive8.via2.irq" => json!(machine.drive8().map(|drive| drive.via2().irq)),
            "drive8.via2.ca1" => json!(machine.drive8().map(|drive| drive.via2().ca1)),
            "drive8.via2.pa" => json!(machine.drive8().map(|drive| drive.via2().pa)),
            "drive8.via2.pb" => json!(machine.drive8().map(|drive| drive.via2().pb)),
            "drive8.via2.ora" => json!(machine.drive8().map(|drive| drive.via2().ora())),
            "drive8.via2.orb" => json!(machine.drive8().map(|drive| drive.via2().orb())),
            "drive8.via2.ddra" => {
                json!(machine.drive8().map(|drive| drive.via2().ddra()))
            }
            "drive8.via2.ddrb" => {
                json!(machine.drive8().map(|drive| drive.via2().ddrb()))
            }
            "drive8.via2.acr" => json!(machine.drive8().map(|drive| drive.via2().peek(0x0B))),
            "drive8.via2.pcr" => json!(machine.drive8().map(|drive| drive.via2().peek(0x0C))),
            "drive8.gcr_read" => json!(machine.drive8().map(|drive| drive.gcr_read())),
            "drive8.byte_ready" => json!(machine.drive8().map(|drive| drive.byte_ready())),
            "drive8.byte_ready_events" => {
                json!(machine.drive8().map(|drive| drive.byte_ready_event_count()))
            }
            "drive8.sync_detected" => {
                json!(machine.drive8().map(|drive| drive.sync_detected()))
            }
            "drive8.sync_events" => {
                json!(machine.drive8().map(|drive| drive.sync_event_count()))
            }
            "drive8.motor_on" => json!(machine.drive8().map(|drive| drive.motor_on())),
            "drive8.activity_led" => {
                json!(machine.drive8().map(|drive| drive.activity_led()))
            }
            "drive8.head_position" => {
                json!(machine.drive8().map(|drive| drive.head_position()))
            }
            "drive8.density_code" => {
                json!(machine.drive8().map(|drive| drive.density_code()))
            }
            "drive8.disk.inserted" => {
                json!(machine.drive8().is_some_and(|drive| drive.disk_inserted()))
            }
            "drive8.disk.name" => json!(
                machine
                    .drive8()
                    .and_then(|drive| drive.disk())
                    .map(|disk| disk.disk_name())
            ),
            "drive8.disk.id" => json!(
                machine
                    .drive8()
                    .and_then(|drive| drive.disk())
                    .map(|disk| disk.disk_id())
            ),
            "drive8.disk.write_protected" => json!(
                machine
                    .drive8()
                    .and_then(|drive| drive.disk())
                    .map(|disk| disk.write_protected())
            ),
            "drive8.disk.directory" => json!(
                machine
                    .drive8()
                    .and_then(|drive| drive.disk())
                    .map(|disk| disk.directory_entries())
            ),
            "drive8.trace.recent_writes" => {
                json!(machine.drive8().map(|drive| drive.recent_io_writes()))
            }
            "iec.cpu_port" => json!(machine.iec_bus().cpu_port()),
            "iec.drive_port" => json!(machine.iec_bus().drive_port()),
            "memory.effective_port" => json!(machine.machine().memory().effective_port()),
            "memory.io_visible" => json!(machine.machine().memory().is_io_visible()),
            "memory.port_data" => json!(machine.machine().memory().port_data()),
            "memory.port_ddr" => json!(machine.machine().memory().port_ddr()),
            "machine.raster_line" => json!(machine.machine().raster_line()),
            "machine.cycle_in_line" => json!(machine.machine().cycle_in_line()),
            "machine.frame_count" => json!(machine.machine().frame_count()),
            "tape.loaded" => json!(machine.machine().tape_is_loaded()),
            "tape.motor_on" => json!(machine.machine().tape_motor_on()),
            "tape.pulse_count" => json!(machine.machine().tape_pulse_count()),
            "tape.pulse_index" => json!(machine.machine().tape_pulse_index()),
            "tape.playing" => json!(machine.machine().tape_is_playing()),
            "tape.sense" => json!(machine.machine().tape_sense_active()),
            "vic.background_colour" => json!(machine.machine().vic_register(0x21) & 0x0F),
            "vic.aec_low" => json!(machine.machine().vic().aec_is_low()),
            "vic.badline" => json!(machine.machine().vic().is_badline()),
            "vic.badline_ba_low" => json!(machine.machine().vic().badline_ba_is_low()),
            "vic.ba_low" => json!(machine.machine().vic().ba_is_low()),
            "vic.ba_low_cycles" => json!(machine.machine().vic().ba_low_cycles()),
            "vic.border_colour" => json!(machine.machine().vic_register(0x20) & 0x0F),
            "vic.c_access_active" => json!(machine.machine().vic().c_access_is_active()),
            "vic.cpu_stalled" => json!(machine.machine().vic().cpu_stalled),
            "vic.forced_badline_cdata_carry_age" => {
                json!(machine.machine().vic().forced_badline_cdata_carry_age())
            }
            "vic.forced_badline_cdata_carry_cycles_remaining" => {
                json!(
                    machine
                        .machine()
                        .vic()
                        .forced_badline_cdata_carry_cycles_remaining()
                )
            }
            "vic.forced_badline_cdata_eligibility_cycles_remaining" => {
                json!(
                    machine
                        .machine()
                        .vic()
                        .forced_badline_cdata_eligibility_cycles_remaining()
                )
            }
            "vic.forced_badline_cdata_carry_pending" => {
                json!(machine.machine().vic().forced_badline_cdata_carry_pending())
            }
            "vic.forced_badline_cdata_carry_slot" => {
                json!(machine.machine().vic().forced_badline_cdata_carry_slot())
            }
            "vic.forced_badline_cdata_carry_value" => {
                json!(machine.machine().vic().forced_badline_cdata_carry_value())
            }
            "vic.forced_badline_cdata_destination_vmli" => {
                json!(
                    machine
                        .machine()
                        .vic()
                        .forced_badline_cdata_destination_vmli()
                )
            }
            "vic.forced_badline_output_delay" => {
                json!(machine.machine().vic().forced_badline_output_delay())
            }
            "vic.idle_state" => json!(machine.machine().vic().idle_state()),
            "vic.irq" => json!(machine.machine().vic().irq_active()),
            "vic.last_bus_data" => json!(machine.machine().vic().last_bus_data()),
            "vic.late_badline_window" => {
                json!(machine.machine().vic().uses_late_badline_window())
            }
            "vic.late_badline_fetches_remaining" => {
                json!(machine.machine().vic().late_badline_fetches_remaining())
            }
            "vic.pending_d011_write_cycle" => {
                json!(machine.machine().vic().pending_d011_write_cycle())
            }
            "vic.rc" => json!(machine.machine().vic().rc()),
            "vic.sprite_ba_low" => json!(machine.machine().vic().sprite_ba_is_low()),
            "vic.vc" => json!(machine.machine().vic().vc()),
            "vic.vcbase" => json!(machine.machine().vic().vcbase()),
            "vic.vmli" => json!(machine.machine().vic().vmli()),
            "cia1.irq" => json!(machine.machine().cia1().irq_active()),
            "cia2.irq" => json!(machine.machine().cia2().irq_active()),
            "screen.text.lines" => json!(decode_screen_text_lines(machine.machine())),
            _ if path.starts_with("memory.ram.") => {
                let suffix = &path["memory.ram.".len()..];
                let addr = parse_hex_u16(suffix).ok_or_else(|| QueryError::UnknownPath {
                    path: path.to_owned(),
                })?;
                json!(machine.machine().memory().ram_read(addr))
            }
            _ if path.starts_with("drive8.mem.") => {
                let suffix = &path["drive8.mem.".len()..];
                let addr = parse_hex_u16(suffix).ok_or_else(|| QueryError::UnknownPath {
                    path: path.to_owned(),
                })?;
                json!(
                    machine
                        .drive8()
                        .map(|drive| drive.peek_with_iec_bus(addr, machine.iec_bus()))
                )
            }
            // Peripheral-owned leaves. The modem answers its own names, so
            // adding one there needs no change here; an unplugged modem
            // reports the leaf as unavailable rather than unknown, which is
            // the honest distinction for hardware that is simply absent.
            _ if let Some(leaf) = path.strip_prefix(ESP_AT_QUERY_PREFIX) => {
                let Some(bridge) = machine.esp_at_tcp_bridge() else {
                    return Err(QueryError::UnavailablePath {
                        path: path.to_owned(),
                        reason: "no ESP-AT modem is attached to the user port",
                    });
                };
                match bridge.query_leaf(leaf) {
                    Some(value) => value,
                    None => return Ok(None),
                }
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

#[cfg(test)]
mod tests {
    use super::{
        C64_QUERY_PATHS, c64_boot_status, decode_screen_code, decode_screen_text_lines,
        parse_hex_u16,
    };
    use crate::Model;
    use crate::runtime::C64Runtime;
    use emu198x_esp_at_modem::EspAtTcpBridge;

    /// Each match arm in `decode_screen_code` is a chip-spec invariant
    /// (the C64 KERNAL puts each character at a fixed code-point).
    /// One assert per arm catches a regression where someone widens a
    /// range and silently swaps two characters.
    #[test]
    fn decode_screen_code_covers_every_arm() {
        assert_eq!(decode_screen_code(0x00), '@');
        assert_eq!(decode_screen_code(0x01), 'A');
        assert_eq!(decode_screen_code(0x1A), 'Z');
        assert_eq!(decode_screen_code(0x1B), '?', "reverse-video gap");
        assert_eq!(decode_screen_code(0x20), ' ');
        assert_eq!(decode_screen_code(0x21), '!');
        assert_eq!(decode_screen_code(0x3F), '?');
        assert_eq!(decode_screen_code(0x40), '@');
        assert_eq!(decode_screen_code(0x5A), 'Z');
        assert_eq!(decode_screen_code(0x5B), '[');
        assert_eq!(decode_screen_code(0x5C), '\\');
        assert_eq!(decode_screen_code(0x5D), ']');
        assert_eq!(decode_screen_code(0x5E), '^');
        assert_eq!(decode_screen_code(0x5F), '_');
        assert_eq!(decode_screen_code(0x60), '`');
        assert_eq!(decode_screen_code(0x61), 'A');
        assert_eq!(decode_screen_code(0x7A), 'Z');
        assert_eq!(decode_screen_code(0x7B), '?', "outside printable arms");
        assert_eq!(decode_screen_code(0xFF), '?', "high bit fallback");
    }

    #[test]
    fn attaching_the_modem_registers_its_own_query_leaves() {
        use emu198x_shell::SessionQueryProvider;

        let mut machine = C64Runtime::blank(Model::C64PalBreadbin);
        let provider = super::C64SessionQueryProvider;

        // Unplugged: the peripheral's leaves are not advertised, and the
        // runtime's own `attached` fact still answers.
        let paths = provider.query_paths(&machine, Some("userport."));
        assert_eq!(paths, vec!["userport.esp_at.attached".to_owned()]);

        machine.attach_esp_at_tcp_bridge(103, 64);
        let paths = provider.query_paths(&machine, Some("userport."));
        for leaf in EspAtTcpBridge::QUERY_LEAVES {
            let path = format!("userport.esp_at.{leaf}");
            assert!(paths.contains(&path), "{path} was not registered");
        }

        // Detaching deregisters them again.
        machine.detach_esp_at_tcp_bridge();
        let paths = provider.query_paths(&machine, Some("userport."));
        assert_eq!(paths, vec!["userport.esp_at.attached".to_owned()]);
    }

    #[test]
    fn the_modem_answers_its_own_leaves_and_reports_absence_when_unplugged() {
        use emu198x_shell::{QueryError, SessionQueryProvider};

        let mut machine = C64Runtime::blank(Model::C64PalBreadbin);
        let provider = super::C64SessionQueryProvider;

        assert!(matches!(
            provider.query(&machine, "userport.esp_at.connected"),
            Err(QueryError::UnavailablePath { .. }),
        ));

        machine.attach_esp_at_tcp_bridge(103, 64);
        let result = provider
            .query(&machine, "userport.esp_at.connected")
            .expect("attached modem answers its own leaf")
            .expect("leaf is owned by the peripheral");
        assert_eq!(result.value, serde_json::json!(false));

        // A name the peripheral does not own stays unknown, not unavailable.
        assert!(matches!(
            provider.query(&machine, "userport.esp_at.nonsense"),
            Ok(None),
        ));
    }

    #[test]
    fn parse_hex_u16_accepts_optional_prefix() {
        assert_eq!(parse_hex_u16("0400"), Some(0x0400));
        assert_eq!(parse_hex_u16("0x0400"), Some(0x0400));
        assert_eq!(parse_hex_u16("0XDEAD"), Some(0xDEAD));
        assert_eq!(parse_hex_u16("FFFF"), Some(0xFFFF));
        assert_eq!(parse_hex_u16("zz"), None);
        assert_eq!(parse_hex_u16("10000"), None, "overflow rejects");
    }

    /// `c64_boot_status` finds the READY. screen-code sequence
    /// anywhere in the 1000-byte screen RAM. Hand-poke the 6 codes
    /// into RAM at a known offset so we drive the detected branch
    /// without booting a real ROM.
    #[test]
    fn c64_boot_status_detects_ready_anywhere_in_screen_ram() {
        let mut runtime = C64Runtime::blank(Model::C64PalBreadbin);
        let ready: [u8; 6] = [18, 5, 1, 4, 25, 46];
        let offset = 80; // row 2, column 0
        for (i, code) in ready.iter().enumerate() {
            runtime
                .machine_mut()
                .cpu_write(0x0400 + offset + i as u16, *code);
        }

        let status = c64_boot_status(runtime.machine());
        assert!(status.detected);
        assert_eq!(status.row, Some(2));
        assert_eq!(status.offset, Some(80));
        assert!(status.reason.contains("READY"));
    }

    #[test]
    fn decode_screen_text_lines_reports_the_full_25_rows() {
        let runtime = C64Runtime::blank(Model::C64PalBreadbin);
        let lines = decode_screen_text_lines(runtime.machine());
        assert_eq!(lines.len(), 25);
        assert!(lines.iter().all(|line| line.chars().count() == 40));
    }

    /// Catalogue invariant: every advertised path is unique. Doubles
    /// would silently clobber each other in a sorted query_paths
    /// listing.
    #[test]
    fn advertised_query_paths_are_unique() {
        let mut sorted: Vec<&&str> = C64_QUERY_PATHS.iter().collect();
        sorted.sort();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(sorted.len(), deduped.len(), "duplicate query paths");
    }
}
