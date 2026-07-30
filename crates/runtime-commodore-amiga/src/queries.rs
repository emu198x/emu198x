//! Family-owned query surface for the Amiga runtime.
//!
//! Splits the `SessionQueryProvider` impl out of `runtime.rs` so the
//! query path catalogue, the boot-status heuristic, and the dispatch
//! table all live alongside each other. The provider itself is
//! stateless (`AmigaSessionQueryProvider`); all the lookup logic lives
//! here.
//!
//! The provider is generic over `M: AmigaMachine` so a single type
//! covers every present and future variant. Variant-specific paths
//! (anything outside the runtime-owned `boot.*` and
//! `amiga.machine.*` namespaces) are pushed down to the machine via
//! `M::resolve_variant_query`.

use commodore_agnus_ocs::{
    AgnusBlitterCompletionDiagnosticPhase, BlitterBusDiagnosticAuthority, BlitterDmaOp,
    PaulaReturnProgressPolicy, SlotOwner,
};
use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use serde_json::{Value, json};

use crate::AmigaRuntime;
use crate::live_access::AmigaLiveAccess;
use crate::variants::AmigaMachine;

/// Runtime-owned query paths shared by every Amiga variant. Variant-
/// specific paths come from `M::variant_query_paths()` and are joined
/// in by `query_paths`.
pub(crate) const SHARED_QUERY_PATHS: &[&str] = &[
    // Boot-status heuristic. `HeadlessSession::wait_for_boot` keys
    // off `boot.detected` so scripts can sleep-until-ready.
    "boot.detected",
    "boot.reason",
    "boot.row",
    "machine.frame_count",
    "runtime",
    "runtime.machine_time",
    "runtime.frame_count",
    "runtime.video_field_count",
    "runtime.non_black_pixels",
    "runtime.non_white_pixels",
    "runtime.first_active_row",
    "runtime.firmware_rom_bytes",
    "runtime.floppy0_image_bytes",
    "runtime.audio_sample_accumulator",
    "runtime.audio_buffer_samples",
    "runtime.tick_hz",
    "runtime.audio_sample_rate_hz",
    "runtime.audio_channels",
    "runtime.cpu_trace_armed",
    "runtime.cpu_trace_pc_filter",
    "runtime.cpu_trace_max_entries",
    "runtime.cpu_trace_entry_count",
    "runtime.cpu_trace_at_limit",
];

/// Group roots whose object fields are added to discovery from the live,
/// side-effect-free snapshot. Static variant catalogues remain a compatibility
/// baseline; this closes the gap where a newly exposed diagnostic field could
/// be returned by a group but omitted from `query_paths`.
const GROUPED_VARIANT_QUERY_ROOTS: &[&str] = &[
    "chipset",
    "agnus",
    "denise",
    "copper",
    "scheduler",
    "dma",
    "blitter",
    "paula",
    "cia",
    "rtc",
    "keyboard",
    "gayle",
    "input",
    "debug",
    "disk",
    "aga",
];

fn collect_value_paths(prefix: &str, value: &Value, paths: &mut Vec<String>) {
    paths.push(prefix.to_owned());
    let Some(object) = value.as_object() else {
        return;
    };
    for (field, child) in object {
        let path = format!("{prefix}.{field}");
        collect_value_paths(&path, child, paths);
    }
}

/// Boot-status snapshot derived from the most recent frame. Matches
/// the archive's `AmigaBootStatus` heuristic: a mostly-coloured
/// framebuffer with visible pixels above row zero counts as boot-
/// detected, matching the Kickstart insert-disk screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AmigaBootStatus {
    pub detected: bool,
    pub reason: &'static str,
    pub row: Option<u32>,
}

/// Boot-status heuristic matching the archive's semantics:
///   - `display-active` once the framebuffer has mostly non-white
///     content and a non-zero first active row (Kickstart insert-disk
///     screen or beyond)
///   - `monochrome-framebuffer` if some pixels lit but below the
///     threshold
///   - `no-visible-output` before the copper has programmed the
///     palette at all
pub(crate) fn boot_status<M: AmigaMachine>(runtime: &AmigaRuntime<M>) -> AmigaBootStatus {
    if let Some(row) = runtime.first_active_row()
        && runtime.non_white_pixels() > 1_000
    {
        AmigaBootStatus {
            detected: true,
            reason: "display-active",
            row: Some(row),
        }
    } else if runtime.non_black_pixels() > 0 {
        AmigaBootStatus {
            detected: false,
            reason: "monochrome-framebuffer",
            row: runtime.first_active_row(),
        }
    } else {
        AmigaBootStatus {
            detected: false,
            reason: "no-visible-output",
            row: None,
        }
    }
}

/// Amiga-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AmigaSessionQueryProvider;

impl<M: AmigaMachine> SessionQueryProvider<AmigaRuntime<M>> for AmigaSessionQueryProvider {
    fn query_paths(&self, machine: &AmigaRuntime<M>, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = SHARED_QUERY_PATHS
            .iter()
            .chain(M::variant_query_paths().iter())
            .copied()
            .map(str::to_owned)
            .collect();
        collect_value_paths("runtime", &runtime_snapshot(machine), &mut paths);
        for group in GROUPED_VARIANT_QUERY_ROOTS {
            if let Ok(Some(snapshot)) = machine.machine().resolve_variant_query(group) {
                collect_value_paths(group, &snapshot, &mut paths);
            }
        }
        paths.retain(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)));
        paths.sort_unstable();
        paths.dedup();
        paths
    }

    fn query(
        &self,
        machine: &AmigaRuntime<M>,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        if is_chip(path, "runtime") {
            let Some(value) = chip_field(path, "runtime", runtime_snapshot(machine)) else {
                return Ok(None);
            };
            return Ok(Some(QueryResult {
                path: path.to_owned(),
                value,
            }));
        }

        // Runtime-owned paths come first.
        let value = match path {
            "boot.detected" => json!(boot_status(machine).detected),
            "boot.reason" => json!(boot_status(machine).reason),
            "boot.row" => json!(boot_status(machine).row),
            "machine.frame_count" => json!(machine.frame_count()),
            _ => {
                // Push everything else down to the variant.
                return match machine.machine().resolve_variant_query(path)? {
                    Some(value) => Ok(Some(QueryResult {
                        path: path.to_owned(),
                        value,
                    })),
                    None => Ok(None),
                };
            }
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

/// Runtime-owned host-integration and diagnostic-buffer state.
pub(crate) fn runtime_snapshot<M: AmigaMachine>(runtime: &AmigaRuntime<M>) -> Value {
    let trace_count = runtime.cpu_trace_entries().len();
    let trace_limit = runtime.cpu_trace_max_entries();
    json!({
        "machine_time": runtime.time_value().get(),
        "frame_count": runtime.frame_count(),
        "video_field_count": runtime.machine().video_field_count(),
        "non_black_pixels": runtime.non_black_pixels(),
        "non_white_pixels": runtime.non_white_pixels(),
        "first_active_row": runtime.first_active_row(),
        "firmware_rom_bytes": runtime.firmware_rom().len(),
        "floppy0_image_bytes": runtime.floppy0_bytes().map(<[u8]>::len),
        "audio_sample_accumulator": runtime.audio_sample_accumulator(),
        "audio_buffer_samples": runtime.audio_buffer_samples(),
        "tick_hz": runtime.tick_hz(),
        "audio_sample_rate_hz": crate::runtime::AUDIO_SAMPLE_RATE_HZ,
        "audio_channels": crate::runtime::AUDIO_CHANNELS,
        "cpu_trace_armed": runtime.cpu_trace_armed(),
        "cpu_trace_pc_filter": runtime.cpu_trace_pc_filter(),
        "cpu_trace_max_entries": trace_limit,
        "cpu_trace_entry_count": trace_count,
        "cpu_trace_at_limit": trace_count >= trace_limit,
    })
}

/// Same provider, but dispatching over the runtime-time
/// `AmigaRuntimeKind` enum so verifier binaries that store
/// `AmigaRuntimeKind` (rather than a concrete `AmigaOcsRuntime` /
/// `AmigaEcsRuntime`) can use this provider directly. The OCS and
/// ECS impl blocks share the same query catalogue today, so the
/// dispatch is trivial.
impl SessionQueryProvider<crate::variants::AmigaRuntimeKind> for AmigaSessionQueryProvider {
    fn query_paths(
        &self,
        machine: &crate::variants::AmigaRuntimeKind,
        prefix: Option<&str>,
    ) -> Vec<String> {
        match machine {
            crate::variants::AmigaRuntimeKind::Ocs(rt) => self.query_paths(rt, prefix),
            crate::variants::AmigaRuntimeKind::Ecs(rt) => self.query_paths(rt, prefix),
            crate::variants::AmigaRuntimeKind::Aga(rt) => self.query_paths(rt, prefix),
        }
    }

    fn query(
        &self,
        machine: &crate::variants::AmigaRuntimeKind,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        match machine {
            crate::variants::AmigaRuntimeKind::Ocs(rt) => self.query(rt, path),
            crate::variants::AmigaRuntimeKind::Ecs(rt) => self.query(rt, path),
            crate::variants::AmigaRuntimeKind::Aga(rt) => self.query(rt, path),
        }
    }
}

// ===================================================================
// Folded chip-snapshot query paths (#456)
//
// The bespoke `query_agnus` / `query_paula` / `query_cia` /
// `query_blitter` / `query_chipset` / `query_disk` / `query_aga` MCP
// tools became grouped objects (`agnus`, …) plus per-field leaves on the
// generic `query` surface. The builders run over `&dyn AmigaLiveAccess`
// — the same trait the old tools used — so one set of helpers serves
// every variant's `resolve_variant_query`. Values are raw numbers,
// matching the existing Amiga leaves and the rest of the fleet.
// ===================================================================

/// Does `path` address chip `chip` — the bare group name or a dotted
/// leaf beneath it (`agnus`, `agnus.vpos`)?
pub(crate) fn is_chip(path: &str, chip: &str) -> bool {
    path == chip
        || path
            .strip_prefix(chip)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Resolve a chip path against a built snapshot: the bare group name
/// returns the whole object, a `chip.field` leaf returns that field, and
/// an unknown sub-field returns `None` (an unknown path, not a null).
/// Returns the bare `Value` the Amiga `resolve_variant_query` contract
/// expects (the provider wraps it into a `QueryResult`).
pub(crate) fn chip_field(path: &str, chip: &str, snapshot: Value) -> Option<Value> {
    if path == chip {
        return Some(snapshot);
    }
    let field = path.strip_prefix(chip)?.strip_prefix('.')?;
    let mut value = &snapshot;
    for segment in field.split('.') {
        value = value.get(segment)?;
    }
    Some(value.clone())
}

/// Decode the Paula INTENA/INTREQ bit layout into a readable map.
/// Bit 14 = master enable; bits 13..0 are individual interrupt sources.
fn decode_int_bits(val: u16) -> Value {
    const NAMES: [&str; 15] = [
        "TBE", "DSKBLK", "SOFT", "PORTS", "COPER", "VERTB", "BLIT", "AUD0", "AUD1", "AUD2", "AUD3",
        "RBF", "DSKSYN", "EXTER", "INTEN",
    ];
    let mut out = serde_json::Map::new();
    for (bit, name) in NAMES.iter().enumerate() {
        out.insert((*name).to_string(), Value::Bool(val & (1 << bit) != 0));
    }
    Value::Object(out)
}

fn decode_named_bits(val: u16, names: &[(&str, u16)]) -> Value {
    let mut out = serde_json::Map::new();
    for &(name, mask) in names {
        out.insert(name.to_owned(), Value::Bool(val & mask != 0));
    }
    Value::Object(out)
}

fn decode_adkcon_bits(val: u16) -> Value {
    decode_named_bits(
        val,
        &[
            ("PRECOMP1", 0x4000),
            ("PRECOMP0", 0x2000),
            ("MFMPREC", 0x1000),
            ("UARTBRK", 0x0800),
            ("WORDSYNC", 0x0400),
            ("MSBSYNC", 0x0200),
            ("FAST", 0x0100),
            ("USE3PN", 0x0080),
            ("USE2P3", 0x0040),
            ("USE1P2", 0x0020),
            ("USE0P1", 0x0010),
            ("USE3VN", 0x0008),
            ("USE2V3", 0x0004),
            ("USE1V2", 0x0002),
            ("USE0V1", 0x0001),
        ],
    )
}

fn decode_serdatr_bits(val: u16) -> Value {
    decode_named_bits(
        val,
        &[
            ("OVRUN", 0x8000),
            ("RBF", 0x4000),
            ("TBE", 0x2000),
            ("TSRE", 0x1000),
        ],
    )
}

fn decode_pot_bits(val: u16) -> Value {
    decode_named_bits(
        val,
        &[
            ("OUTRY", 0x8000),
            ("DATRY", 0x4000),
            ("OUTLY", 0x2000),
            ("DATLY", 0x1000),
            ("OUTRX", 0x0800),
            ("DATRX", 0x0400),
            ("OUTLX", 0x0200),
            ("DATLX", 0x0100),
        ],
    )
}

/// BPLCON0 / DMACON / ADKCON / COLOR00 / copper pointers / overlay.
pub(crate) fn chipset_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let enhanced_agnus = m.ecs_agnus_timing();
    let enhanced_denise = m.enhanced_denise();
    json!({
        "bplcon0": m.bplcon0(),
        "bplcon3": enhanced_denise.map(|denise| denise.bplcon3),
        "dmacon": m.dmacon(),
        "adkcon": m.adkcon(),
        "color00": m.color(0),
        "cop1lc": m.copper_cop1lc(),
        "cop2lc": m.copper_cop2lc(),
        "copper_pc": m.copper_pc(),
        "overlay": m.overlay(),
        "ecsena_enabled": enhanced_denise.map(|denise| denise.ecsena_enabled),
        "extblken_enabled": enhanced_denise.map(|denise| denise.extblken_enabled),
        "blanken_enabled": enhanced_agnus.map(|agnus| agnus.blanken_enabled),
        "programmed_hblank_output_active":
            enhanced_denise.map(|denise| denise.programmed_hblank_active),
    })
}

/// Common Copper pipeline state and bounded MOVE-log summary.
pub(crate) fn copper_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let copper = m.copper();
    let move_log = m.copper_move_log();
    let last_move = move_log.last().map(|&(tick, vpos, hpos, register, value)| {
        json!({
            "tick": tick,
            "vpos": vpos,
            "hpos": hpos,
            "register": register,
            "value": value,
        })
    });
    json!({
        "pc": copper.pc,
        "cop1lc": copper.cop1lc,
        "cop2lc": copper.cop2lc,
        "waiting": copper.waiting,
        "wait_target": copper.wait_target,
        "wait_mask": copper.wait_mask,
        "wait_bfd": copper.wait_bfd,
        "cck_phase": copper.cck_phase,
        "pending_wait_delay": copper.pending_wait_delay,
        "pending_wait_target": copper.pending_wait_target,
        "pending_wait_mask": copper.pending_wait_mask,
        "pending_wait_bfd": copper.pending_wait_bfd,
        "pending_wait_is_skip": copper.pending_wait_is_skip,
        "stopped": copper.stopped,
        "cdang": copper.cdang,
        "bus_used_this_cck": copper.bus_used_this_cck,
        "move_log_count": move_log.len(),
        "last_move": last_move,
    })
}

/// Board scheduler, CPU clock-domain and pending boundary state.
pub(crate) fn scheduler_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let state = m.scheduler_diagnostic_snapshot();
    let pending_cpu_boundaries: Vec<Value> = state
        .pending_cpu_boundaries
        .iter()
        .map(|boundary| {
            json!({
                "system_tick": boundary.system_tick,
                "instr_start_pc": boundary.instr_start_pc,
                "sr": boundary.sr,
                "opcode": boundary.opcode,
            })
        })
        .collect();
    json!({
        "tick_count": state.tick_count,
        "cck_count": state.cck_count,
        "cck_phase": state.cck_phase,
        "e_clock_phase": state.e_clock_phase,
        "prev_vertb_level": state.prev_vertb_level,
        "prev_cia_a_irq": state.prev_cia_a_irq,
        "prev_cia_b_irq": state.prev_cia_b_irq,
        "prev_cia_a_spmode": state.prev_cia_a_spmode,
        "cpu_clock_numerator": state.cpu_clock_numerator,
        "cpu_clock_denominator": state.cpu_clock_denominator,
        "cpu_clock_phase": state.cpu_clock_phase,
        "cpu_clock_maximum_edges_per_tick": state.cpu_clock_maximum_edges_per_tick,
        "cpu_domain_idle": state.cpu_domain_idle,
        "cpu_domain_edges_remaining": state.cpu_domain_edges_remaining,
        "cpu_domain_motherboard_slot_pending":
            state.cpu_domain_motherboard_slot_pending,
        "cpu_domain_coherent": state.cpu_domain_coherent,
        "pending_cpu_boundary_count": pending_cpu_boundaries.len(),
        "pending_cpu_boundaries": pending_cpu_boundaries,
        "pending_cpu_boundary_capacity": state.pending_cpu_boundary_capacity,
        "pending_cpu_boundary_at_capacity":
            state.pending_cpu_boundaries.len() >= state.pending_cpu_boundary_capacity,
    })
}

/// Complete Paula interrupt, audio, serial, pot-port and component-log state.
pub(crate) fn paula_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let paula = m.paula();
    let interrupt = paula.interrupt_diagnostic_snapshot();
    let audio = paula.audio_diagnostic_snapshot();
    let serial = paula.serial_diagnostic_snapshot();
    let pot = paula.pot_diagnostic_snapshot();
    let logs = paula.log_diagnostic_snapshot();
    json!({
        "intena": interrupt.intena,
        "intreq": interrupt.intreq,
        "adkcon": interrupt.adkcon,
        "active_sources": interrupt.active_sources,
        "ipl": interrupt.ipl,
        "master_enable": (interrupt.intena & 0x4000) != 0,
        "intena_bits": decode_int_bits(interrupt.intena),
        "intreq_bits": decode_int_bits(interrupt.intreq),
        "active_source_bits": decode_int_bits(interrupt.active_sources),
        "adkcon_bits": decode_adkcon_bits(interrupt.adkcon),
        "audio": {
            "channels": {
                "channel0": audio.channels[0],
                "channel1": audio.channels[1],
                "channel2": audio.channels[2],
                "channel3": audio.channels[3],
            },
            "controls": audio.controls,
        },
        "serial": {
            "serdat": serial.serdat,
            "serper": serial.serper,
            "serdatr": serial.serdatr,
            "serdatr_bits": decode_serdatr_bits(serial.serdatr),
            "receive_data": serial.receive_data,
            "receive_full": serial.receive_full,
            "receive_overrun": serial.receive_overrun,
        },
        "pot": {
            "potgo": pot.potgo,
            "potgo_bits": decode_pot_bits(pot.potgo),
            "raw_pin_levels": pot.raw_pin_levels,
            "raw_pin_bits": decode_pot_bits(pot.raw_pin_levels),
            "potgor": pot.potgor,
            "potgor_bits": decode_pot_bits(pot.potgor),
            "pot0dat": pot.pot0dat,
            "pot1dat": pot.pot1dat,
        },
        "logs": logs,
    })
}

fn cia_control_fields(cra: u8, crb: u8) -> (Value, Value, &'static str, &'static str) {
    let cra_bits = decode_named_bits(
        u16::from(cra),
        &[
            ("START", 0x01),
            ("PBON", 0x02),
            ("OUTMODE", 0x04),
            ("RUNMODE", 0x08),
            ("INMODE", 0x20),
            ("SPMODE", 0x40),
            ("TOD_RATE", 0x80),
        ],
    );
    let crb_bits = decode_named_bits(
        u16::from(crb),
        &[
            ("START", 0x01),
            ("PBON", 0x02),
            ("OUTMODE", 0x04),
            ("RUNMODE", 0x08),
            ("INMODE0", 0x20),
            ("INMODE1", 0x40),
            ("ALARM_SELECT", 0x80),
        ],
    );
    let timer_a_input = if cra & 0x20 == 0 { "phi2" } else { "cnt" };
    let timer_b_input = match crb & 0x60 {
        0x00 => "phi2",
        0x20 => "cnt",
        0x40 => "timer-a",
        _ => "cnt-and-timer-a",
    };
    (cra_bits, crb_bits, timer_a_input, timer_b_input)
}

fn decode_cia_interrupt_bits(value: u8) -> Value {
    decode_named_bits(
        u16::from(value),
        &[
            ("TA", 0x01),
            ("TB", 0x02),
            ("ALARM", 0x04),
            ("SP", 0x08),
            ("FLAG", 0x10),
            ("IR", 0x80),
        ],
    )
}

/// One CIA-8520's complete implemented register, latch and pin state.
fn cia_fields(c: &machine_commodore_amiga_ocs::Cia) -> Value {
    let state = c.diagnostic_snapshot();
    let (cra_bits, crb_bits, timer_a_input, timer_b_input) =
        cia_control_fields(state.cra, state.crb);
    let mut fields = serde_json::to_value(state)
        .expect("the CIA diagnostic snapshot serialises")
        .as_object()
        .cloned()
        .expect("the CIA diagnostic snapshot is an object");
    fields.insert("cra_bits".to_owned(), cra_bits);
    fields.insert("crb_bits".to_owned(), crb_bits);
    fields.insert("timer_a_input".to_owned(), json!(timer_a_input));
    fields.insert("timer_b_input".to_owned(), json!(timer_b_input));
    fields.insert(
        "icr_status_bits".to_owned(),
        decode_cia_interrupt_bits(state.icr_status),
    );
    fields.insert(
        "icr_mask_bits".to_owned(),
        decode_cia_interrupt_bits(state.icr_mask),
    );
    Value::Object(fields)
}

/// Both CIAs (`cia_a` = U7 / keyboard / floppy control, `cia_b` = U8 /
/// serial / disk step).
pub(crate) fn cia_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    json!({
        "cia_a": cia_fields(m.cia_a()),
        "cia_b": cia_fields(m.cia_b()),
    })
}

/// Battery-backed clock value, decoded calendar and raw/decoded controls.
pub(crate) fn rtc_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    json!(m.rtc_diagnostic_snapshot())
}

/// Keyboard controller protocol progress and its current CIA-A serial
/// interface. All reads are side-effect-free.
pub(crate) fn keyboard_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let keyboard = m.keyboard().diagnostic_snapshot();
    let mut snapshot = serde_json::to_value(&keyboard)
        .expect("Amiga keyboard diagnostic snapshot should serialize");
    let fields = snapshot
        .as_object_mut()
        .expect("Amiga keyboard diagnostic snapshot should be an object");
    // Retain the established compact aliases while exposing the component's
    // complete protocol snapshot.
    fields.insert("timer".to_owned(), json!(keyboard.timer_ticks));
    fields.insert("queued".to_owned(), json!(keyboard.queue_count));
    fields.insert("cia_a_sdr".to_owned(), json!(m.cia_a().peek(0x0C)));
    fields.insert(
        "cia_a_spmode".to_owned(),
        json!((m.cia_a().cra() & 0x40) != 0),
    );
    snapshot
}

/// Complete Gayle board-controller state on machines which contain it.
///
/// A600 and A1200 return a snapshot; A500+ and OCS-shaped machines return
/// `None`, so discovery reflects the actual configured board.
pub(crate) fn gayle_snapshot(m: &dyn AmigaLiveAccess) -> Option<Value> {
    m.gayle_diagnostic_snapshot()
        .map(|snapshot| json!(snapshot))
}

/// Board-level controller counters and Paula proportional-input readback.
pub(crate) fn input_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let state = m.input_diagnostic_snapshot();
    let pot = m.paula().pot_diagnostic_snapshot();
    json!({
        "joy0_x": state.joy0_x,
        "joy0_y": state.joy0_y,
        "joy0dat": state.joy0dat,
        "joy1_x": state.joy1_x,
        "joy1_y": state.joy1_y,
        "joy1dat": state.joy1dat,
        "port0_primary_button_pressed": state.port0_primary_button_pressed,
        "port1_primary_button_pressed": state.port1_primary_button_pressed,
        "joystick1_up": state.joystick1_up,
        "joystick1_down": state.joystick1_down,
        "joystick1_left": state.joystick1_left,
        "joystick1_right": state.joystick1_right,
        "joystick1_fire": state.joystick1_fire,
        "joystick1_button2": state.joystick1_button2,
        "joystick1_button3": state.joystick1_button3,
        "potgo": pot.potgo,
        "potgor": pot.potgor,
        "pot_raw_pin_levels": pot.raw_pin_levels,
        "pot0dat": pot.pot0dat,
        "pot1dat": pot.pot1dat,
    })
}

fn register_read_counts(m: &dyn AmigaLiveAccess) -> Vec<Value> {
    let mut counts: Vec<(u16, u64)> = m
        .register_read_counts()
        .iter()
        .map(|(&register, &count)| (register, count))
        .collect();
    counts.sort_unstable_by_key(|&(register, _)| register);
    counts
        .into_iter()
        .map(|(register, count)| json!({"register": register, "count": count}))
        .collect()
}

fn cia_read_counts(counts: Option<&std::collections::HashMap<u8, u64>>) -> Option<Vec<Value>> {
    let mut counts: Vec<(u8, u64)> = counts?
        .iter()
        .map(|(&register, &count)| (register, count))
        .collect();
    counts.sort_unstable_by_key(|&(register, _)| register);
    Some(
        counts
            .into_iter()
            .map(|(register, count)| json!({"register": register, "count": count}))
            .collect(),
    )
}

/// Non-behavioural counters and bounded trace-buffer summaries. Large logs
/// stay available through their dedicated trace tools; this query reports
/// counts and the most recent entry without serialising megabytes of history.
pub(crate) fn debug_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let last_transition = |entry: Option<&(u64, u32, u16, u16, u16)>| {
        entry.map(|&(tick, pc, written, before, after)| {
            json!({
                "tick": tick,
                "pc": pc,
                "written": written,
                "before": before,
                "after": after,
            })
        })
    };
    let last_pointer = |entry: Option<&(u64, u32, u32)>| {
        entry.map(|&(tick, pc, value)| json!({"tick": tick, "pc": pc, "value": value}))
    };
    let last_cia_write = |entry: Option<&(u64, u32, u8, u8)>| {
        entry.map(|&(tick, pc, register, value)| {
            json!({"tick": tick, "pc": pc, "register": register, "value": value})
        })
    };
    json!({
        "register_read_counts": register_read_counts(m),
        "register_read_log_count": m.reg_read_log().len(),
        "last_register_read": m.reg_read_log().last().map(
            |&(tick, pc, register, value)| {
                json!({"tick": tick, "pc": pc, "register": register, "value": value})
            }
        ),
        "custom_write_log_count": m.custom_write_log().len(),
        "last_custom_write": m.custom_write_log().last().map(
            |&(tick, pc, address, register, value, is_word)| {
                json!({
                    "tick": tick,
                    "pc": pc,
                    "address": address,
                    "register": register,
                    "value": value,
                    "is_word": is_word,
                })
            }
        ),
        "palette_log_count": m.palette_log().len(),
        "last_palette_write": m.palette_log().last().map(
            |&(tick, pc, register, value, bplcon3)| {
                json!({
                    "tick": tick,
                    "pc": pc,
                    "register": register,
                    "value": value,
                    "bplcon3": bplcon3,
                })
            }
        ),
        "bplcon0_log_count": m.bplcon0_log().len(),
        "last_bplcon0_write": m.bplcon0_log().last().map(
            |&(tick, pc, value)| json!({"tick": tick, "pc": pc, "value": value})
        ),
        "peak_intena": m.peak_intena(),
        "intena_write_count": m.intena_write_count(),
        "intena_transition_count": m.intena_log().len(),
        "last_intena_transition": last_transition(m.intena_log().last()),
        "dmacon_transition_count": m.dmacon_log().len(),
        "last_dmacon_transition": last_transition(m.dmacon_log().last()),
        "cop1lc_write_count": m.cop1lc_log().len(),
        "last_cop1lc_write": last_pointer(m.cop1lc_log().last()),
        "cop2lc_write_count": m.cop2lc_log().len(),
        "last_cop2lc_write": last_pointer(m.cop2lc_log().last()),
        "dsk_write_count": m.dsk_write_log().len(),
        "last_dsk_write": m.dsk_write_log().last().map(
            |&(tick, pc, register, value)| {
                json!({"tick": tick, "pc": pc, "register": register, "value": value})
            }
        ),
        "blitter_start_count": m.blitter_start_count(),
        "blitter_log_count": m.blitter_log().len(),
        "last_blitter_start": m.blitter_log().last().map(
            |&(tick, pc, bltcon0, bltcon1, apt, bpt, cpt, dpt, bltsize)| {
                json!({
                    "tick": tick,
                    "pc": pc,
                    "bltcon0": bltcon0,
                    "bltcon1": bltcon1,
                    "apt": apt,
                    "bpt": bpt,
                    "cpt": cpt,
                    "dpt": dpt,
                    "bltsize": bltsize,
                })
            }
        ),
        "cia_a_write_count": m.cia_a_write_log().len(),
        "last_cia_a_write": last_cia_write(m.cia_a_write_log().last()),
        "cia_b_write_count": m.cia_b_write_log().len(),
        "last_cia_b_write": last_cia_write(m.cia_b_write_log().last()),
        "cia_a_read_counts": cia_read_counts(m.cia_a_read_counts()),
        "cia_b_read_counts": cia_read_counts(m.cia_b_read_counts()),
        "rtc_access_count": m.rtc_access_log().len(),
        "last_rtc_access": m.rtc_access_log().last().map(
            |&(tick, pc, address, is_read, is_word, value)| {
                json!({
                    "tick": tick,
                    "pc": pc,
                    "address": address,
                    "is_read": is_read,
                    "is_word": is_word,
                    "value": value,
                })
            }
        ),
        "watch_range": m.watch_range().map(
            |(base, length)| json!({"base": base, "length": length})
        ),
        "watch_write_count": m.watch_log().len(),
        "last_watch_write": m.watch_log().last().map(
            |&(tick, pc, address, value, is_word)| {
                json!({
                    "tick": tick,
                    "pc": pc,
                    "address": address,
                    "value": value,
                    "is_word": is_word,
                })
            }
        ),
    })
}

/// Agnus: beam position, DMA pointers, blitter pointers, the display
/// window / data-fetch registers, modulos, and the fetch-width / plane
/// decode. `dmacon` / `bplcon0` mirror the chipset registers Agnus owns.
pub(crate) fn agnus_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let a = m.agnus();
    let ecs = m.ecs_agnus_timing();
    let mut snapshot = json!({
        "vpos": a.vpos,
        "hpos": a.hpos,
        "dmacon": m.dmacon(),
        "bplcon0": m.bplcon0(),
        "blitter_busy": a.blitter_busy,
        "blitter_busy_visible": a.blitter_busy_visible(),
        "blitter_busy_copper": a.blitter_busy_copper(),
        "blitter_exec_pending": a.blitter_exec_pending,
        "blitter_startup_ccks_remaining": a.blitter_startup_ccks_remaining(),
        "blitter_ccks_remaining": a.blitter_ccks_remaining,
        "blitter_completion_phase": a.blitter_completion_phase(),
        "blitter_completion_ccks_remaining": a.blitter_completion_ccks_remaining(),
        "blitter_final_d_pending": a.blitter_final_d_pending(),
        "bpl_pt": (0..8).map(|i| a.bpl_pt[i]).collect::<Vec<_>>(),
        "blt_apt": a.blt_apt,
        "blt_bpt": a.blt_bpt,
        "blt_cpt": a.blt_cpt,
        "blt_dpt": a.blt_dpt,
        "fmode": a.fmode,
        "bpl_fetch_width": a.bpl_fetch_width(),
        "spr_fetch_width": a.spr_fetch_width(),
        "diwstrt": a.diwstrt,
        "diwstop": a.diwstop,
        "ddfstrt": a.ddfstrt,
        "ddfstop": a.ddfstop,
        "bpl1mod": a.bpl1mod,
        "bpl2mod": a.bpl2mod,
        "num_bitplanes": a.num_bitplanes(),
    })
    .as_object()
    .cloned()
    .expect("the base Agnus snapshot is an object");
    let enhanced_registers = json!({
        "beamcon0": ecs.map(|state| state.beamcon0),
        "htotal": ecs.map(|state| state.htotal),
        "hsstop": ecs.map(|state| state.hsstop),
        "hbstrt": ecs.map(|state| state.hbstrt),
        "hbstop": ecs.map(|state| state.hbstop),
        "vtotal": ecs.map(|state| state.vtotal),
        "vsstop": ecs.map(|state| state.vsstop),
        "vbstrt": ecs.map(|state| state.vbstrt),
        "vbstop": ecs.map(|state| state.vbstop),
        "hsstrt": ecs.map(|state| state.hsstrt),
        "vsstrt": ecs.map(|state| state.vsstrt),
        "diwhigh": ecs.map(|state| state.diwhigh),
        "diwhigh_written": ecs.map(|state| state.diwhigh_written),
        "bltsizv": ecs.map(|state| state.bltsizv),
        "bltsizh": ecs.map(|state| state.bltsizh),
    });
    let enhanced_events = json!({
        "programmed_vertical_accessed":
            ecs.map(|state| state.programmed_vertical_accessed),
        "programmed_vblank_active": ecs.map(|state| state.programmed_vblank_active),
        "programmed_vblank_start_event":
            ecs.map(|state| state.programmed_vblank_start_event),
        "programmed_vblank_stop_event":
            ecs.map(|state| state.programmed_vblank_stop_event),
        "programmed_hblank_active": ecs.map(|state| state.programmed_hblank_active),
        "programmed_hblank_routed_active":
            ecs.map(|state| state.programmed_hblank_routed_active),
        "vertical_diw_active": ecs.map(|state| state.vertical_diw_active),
        "current_line_ccks": ecs.map(|state| state.current_line_ccks),
        "copper_comparator_hpos": ecs.map(|state| state.copper_comparator_hpos),
    });
    let enhanced_signals = json!({
        "pal_enabled": ecs.map(|state| state.pal_enabled),
        "dual_enabled": ecs.map(|state| state.dual_enabled),
        "varbeamen_enabled": ecs.map(|state| state.varbeamen_enabled),
        "varvben_enabled": ecs.map(|state| state.varvben_enabled),
        "varvsyen_enabled": ecs.map(|state| state.varvsyen_enabled),
        "varhsyen_enabled": ecs.map(|state| state.varhsyen_enabled),
        "cscben_enabled": ecs.map(|state| state.cscben_enabled),
        "varcsyen_enabled": ecs.map(|state| state.varcsyen_enabled),
        "harddis_enabled": ecs.map(|state| state.harddis_enabled),
        "blanken_enabled": ecs.map(|state| state.blanken_enabled),
        "loldis_enabled": ecs.map(|state| state.loldis_enabled),
        "lpendis_enabled": ecs.map(|state| state.lpendis_enabled),
        "csytrue_enabled": ecs.map(|state| state.csytrue_enabled),
        "vsytrue_enabled": ecs.map(|state| state.vsytrue_enabled),
        "hsytrue_enabled": ecs.map(|state| state.hsytrue_enabled),
        "harddis_hblank_window_active":
            ecs.map(|state| state.harddis_hblank_window_active),
        "vblank_window_active": ecs.map(|state| state.vblank_window_active),
        "hsync_window_active": ecs.map(|state| state.hsync_window_active),
        "vsync_window_active": ecs.map(|state| state.vsync_window_active),
        "sync_pin_hsync": ecs.map(|state| state.sync_pin_hsync),
        "sync_pin_vsync": ecs.map(|state| state.sync_pin_vsync),
        "sync_pin_csync": ecs.map(|state| state.sync_pin_csync),
        "sync_pin_blank": ecs.map(|state| state.sync_pin_blank),
    });
    for enhanced in [enhanced_registers, enhanced_events, enhanced_signals] {
        snapshot.extend(
            enhanced
                .as_object()
                .expect("the enhanced Agnus snapshot section is an object")
                .clone(),
        );
    }
    Value::Object(snapshot)
}

/// Enhanced Denise state shared by ECS Super Denise and AGA Lisa. OCS keeps
/// the same discoverable schema with `null` values.
pub(crate) fn denise_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let denise = m.enhanced_denise();
    let core = m.denise_diagnostic_snapshot();
    let mut snapshot = json!({
        "palette_12": core.palette_12,
        "palette_24": core.palette_24.to_vec(),
        "raster_width": core.raster_width,
        "raster_height": core.raster_height,
        "framebuffer_pixels": core.framebuffer_pixels,
        "interlace_active": core.interlace_active,
        "long_frame": core.long_frame,
        "maximum_bitplanes": core.maximum_bitplanes,
        "active_bitplanes": core.active_bitplanes,
        "bplcon0": core.bplcon0,
        "bplcon1": core.bplcon1,
        "bplcon2": core.bplcon2,
        "bplcon4": core.bplcon4,
        "clxcon": core.clxcon,
        "clxdat": core.clxdat,
        "bitplanes": {
            "holding_data": core.bitplanes.holding_data,
            "shift_data": core.bitplanes.shift_data,
            "aggregate_shift_count": core.bitplanes.aggregate_shift_count,
            "shift_counts": core.bitplanes.shift_counts,
            "shift_delays": core.bitplanes.shift_delays,
            "previous_data": core.bitplanes.previous_data,
            "pending_data": core.bitplanes.pending_data,
            "pending_copy_odd_planes": core.bitplanes.pending_copy_odd_planes,
            "pending_copy_even_planes": core.bitplanes.pending_copy_even_planes,
            "scroll_pending_line": core.bitplanes.scroll_pending_line,
            "active_fifo": core.bitplanes.active_fifo,
            "active_fifo_lengths": core.bitplanes.active_fifo_lengths,
            "staged_fetch_tails": core.bitplanes.staged_fetch_tails,
            "staged_fetch_tail_lengths": core.bitplanes.staged_fetch_tail_lengths,
            "deferred_shift_load_source_pixels":
                core.bitplanes.deferred_shift_load_source_pixels,
        },
        "sprite_width": core.sprite_width,
        "sprites": core.sprites,
        "sprite_bpl1dat_enabled": core.sprite_bpl1dat_enabled,
        "sprite_runtime_line_valid": core.sprite_runtime_line_valid,
        "sprite_runtime_beam_x": core.sprite_runtime_beam_x,
        "sprite_runtime_beam_y": core.sprite_runtime_beam_y,
        "ham_previous_rgb12": core.ham_previous_rgb12,
        "ham_previous_rgb24": core.ham_previous_rgb24,
        "last_shift_load": {
            "hires": core.last_shift_load.hires,
            "odd_scroll": core.last_shift_load.odd_scroll,
            "even_scroll": core.last_shift_load.even_scroll,
            "num_bitplanes": core.last_shift_load.num_bitplanes,
            "planes": core.last_shift_load.planes,
        },
    })
    .as_object()
    .cloned()
    .expect("the common Denise snapshot is an object");
    let enhanced = json!({
        "deniseid": denise.map(|state| state.deniseid),
        "bplcon3": denise.map(|state| state.bplcon3),
        "ecsena_enabled": denise.map(|state| state.ecsena_enabled),
        "extblken_enabled": denise.map(|state| state.extblken_enabled),
        "shres_enabled": denise.map(|state| state.shres_enabled),
        "bplhwrm_enabled": denise.map(|state| state.bplhwrm_enabled),
        "sprhwrm_enabled": denise.map(|state| state.sprhwrm_enabled),
        "bplcon3_extensions_enabled":
            denise.map(|state| state.bplcon3_extensions_enabled),
        "border_blank_enabled": denise.map(|state| state.border_blank_enabled),
        "border_opaque_enabled": denise.map(|state| state.border_opaque_enabled),
        "killehb_enabled": denise.map(|state| state.killehb_enabled),
        "programmed_hblank_active":
            denise.map(|state| state.programmed_hblank_active),
    });
    snapshot.extend(
        enhanced
            .as_object()
            .expect("the enhanced Denise snapshot is an object")
            .clone(),
    );
    Value::Object(snapshot)
}

fn slot_owner_fields(owner: SlotOwner) -> (&'static str, Option<u8>) {
    match owner {
        SlotOwner::Cpu => ("cpu", None),
        SlotOwner::Refresh => ("refresh", None),
        SlotOwner::Disk => ("disk", None),
        SlotOwner::Audio(channel) => ("audio", Some(channel)),
        SlotOwner::Sprite(channel) => ("sprite", Some(channel)),
        SlotOwner::Bitplane(channel) => ("bitplane", Some(channel)),
        SlotOwner::Copper => ("copper", None),
    }
}

fn blitter_dma_op_name(operation: BlitterDmaOp) -> &'static str {
    match operation {
        BlitterDmaOp::ReadA => "read-a",
        BlitterDmaOp::ReadB => "read-b",
        BlitterDmaOp::ReadC => "read-c",
        BlitterDmaOp::WriteD => "write-d",
        BlitterDmaOp::Internal => "internal",
    }
}

/// Agnus's current arbitration plan, recorded same-CCK use and complete DDF
/// comparator/run state.
pub(crate) fn dma_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let bus = m.agnus_bus_diagnostic_snapshot();
    let ddf = m.agnus().ddf_diagnostic_snapshot();
    let (slot_owner, slot_channel) = slot_owner_fields(bus.plan.slot_owner);
    let return_policy = match bus.plan.paula_return_progress_policy {
        PaulaReturnProgressPolicy::Advance => "advance",
        PaulaReturnProgressPolicy::Stall => "stall",
        PaulaReturnProgressPolicy::CopperFetchConditional => "copper-fetch-conditional",
    };
    let blitter_authority = match bus.blitter_authority {
        BlitterBusDiagnosticAuthority::CurrentPlanFallback => "current-plan-fallback",
        BlitterBusDiagnosticAuthority::RecordedCckState => "recorded-cck-state",
    };
    json!({
        "vpos": bus.vpos,
        "hpos": bus.hpos,
        "dmacon": m.dmacon(),
        "dmacon_bits": decode_named_bits(
            m.dmacon(),
            &[
                ("BLTPRI", 0x0400),
                ("DMAEN", 0x0200),
                ("BPLEN", 0x0100),
                ("COPEN", 0x0080),
                ("BLTEN", 0x0040),
                ("SPREN", 0x0020),
                ("DSKEN", 0x0010),
                ("AUD3EN", 0x0008),
                ("AUD2EN", 0x0004),
                ("AUD1EN", 0x0002),
                ("AUD0EN", 0x0001),
            ],
        ),
        "plan": {
            "slot_owner": slot_owner,
            "slot_channel": slot_channel,
            "disk_dma_slot_granted": bus.plan.disk_dma_slot_granted,
            "sprite_dma_service_channel": bus.plan.sprite_dma_service_channel,
            "audio_dma_service_channel": bus.plan.audio_dma_service_channel,
            "bitplane_dma_fetch_plane": bus.plan.bitplane_dma_fetch_plane,
            "copper_dma_slot_granted": bus.plan.copper_dma_slot_granted,
            "cpu_chip_bus_granted": bus.plan.cpu_chip_bus_granted,
            "blitter_chip_bus_granted": bus.plan.blitter_chip_bus_granted,
            "blitter_dma_progress_granted": bus.plan.blitter_dma_progress_granted,
            "paula_return_progress_policy": return_policy,
        },
        "actual": {
            "sprite_bus_used_this_cck": bus.sprite_bus_used_this_cck,
            "sprite_holds_bus": bus.sprite_holds_bus,
            "blitter_bus_used_this_cck": bus.blitter_bus_used_this_cck,
            "blitter_nasty_owned_this_cck": bus.blitter_nasty_owned_this_cck,
            "blitter_cck_bus_state_recorded": bus.blitter_cck_bus_state_recorded,
            "blitter_authority": blitter_authority,
            "blitter_holds_bus": bus.blitter_holds_bus,
        },
        "ddf": ddf,
    })
}

/// Complete Agnus blitter register, scheduling, word, line and area state.
pub(crate) fn blitter_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let state = m.agnus().blitter_diagnostic_snapshot();
    let completion = &state.execution.completion_phase;
    let (completion_phase, final_write_address, final_write_value) = match completion {
        AgnusBlitterCompletionDiagnosticPhase::None => ("none", None, None),
        AgnusBlitterCompletionDiagnosticPhase::FinalResult => ("final-result", None, None),
        AgnusBlitterCompletionDiagnosticPhase::FinalWrite { address, value } => {
            ("final-write", Some(*address), Some(*value))
        }
    };
    let word = state.word.as_ref();
    let line = state.line.as_ref();
    let area = state.area.as_ref();
    let execution = json!({
        "agnus_id": state.execution.agnus_id,
        "original_revision": state.execution.original_revision,
        "dmacon": state.execution.dmacon,
        "dma_enabled": state.execution.dma_enabled,
        "priority_enabled": state.execution.priority_enabled,
        "busy": state.execution.busy,
        "busy_visible": state.execution.busy_visible,
        "busy_copper": state.execution.busy_copper,
        "nasty_active": state.execution.nasty_active,
        "startup_ccks_remaining": state.execution.startup_ccks_remaining,
        "completion_phase": completion_phase,
        "final_write_address": final_write_address,
        "final_write_value": final_write_value,
        "completion_ccks_remaining": state.execution.completion_ccks_remaining,
        "final_d_pending": state.execution.final_d_pending,
        "finish_emitted": state.execution.finish_emitted,
        "dmacon_busy_hold_ccks": state.execution.dmacon_busy_hold_ccks,
        "copper_busy_hold_ccks": state.execution.copper_busy_hold_ccks,
        "exec_pending": state.execution.exec_pending,
        "exec_ready": state.execution.exec_ready,
        "zero": state.execution.zero,
        "height": state.execution.height,
        "width_words": state.execution.width_words,
        "ccks_remaining": state.execution.ccks_remaining,
        "next_dma_request": state.execution.next_dma_request.map(blitter_dma_op_name),
        "next_progress_uses_bus": state.execution.next_progress_uses_bus,
        "word_complete": state.execution.word_complete,
        "incremental_runtime_present": state.execution.incremental_runtime_present,
    });
    let word = json!({
        "present": word.is_some(),
        "need_a": word.map(|state| state.need_a),
        "need_b": word.map(|state| state.need_b),
        "need_c": word.map(|state| state.need_c),
        "need_d": word.map(|state| state.need_d),
        "reads_done": word.map(|state| state.reads_done),
        "internal_only": word.map(|state| state.internal_only),
        "internal_done": word.map(|state| state.internal_done),
    });
    let line = json!({
        "present": line.is_some(),
        "steps_remaining": line.map(|state| state.steps_remaining),
        "error": line.map(|state| state.error),
        "error_add": line.map(|state| state.error_add),
        "error_sub": line.map(|state| state.error_sub),
        "cpt": line.map(|state| state.cpt),
        "dpt": line.map(|state| state.dpt),
        "pixel_bit": line.map(|state| state.pixel_bit),
        "row_mod": line.map(|state| state.row_mod),
        "texture": line.map(|state| state.texture),
        "texture_bit": line.map(|state| state.texture_bit),
        "lf": line.map(|state| state.lf),
        "sing": line.map(|state| state.sing),
        "one_dot_drawn": line.map(|state| state.one_dot_drawn),
        "major_is_y": line.map(|state| state.major_is_y),
        "x_negative": line.map(|state| state.x_negative),
        "y_negative": line.map(|state| state.y_negative),
        "last_c_word": line.map(|state| state.last_c_word),
        "have_c_word": line.map(|state| state.have_c_word),
    });
    let area = json!({
        "present": area.is_some(),
        "rows_remaining": area.map(|state| state.rows_remaining),
        "width_words": area.map(|state| state.width_words),
        "words_remaining_in_row": area.map(|state| state.words_remaining_in_row),
        "use_a": area.map(|state| state.use_a),
        "use_b": area.map(|state| state.use_b),
        "use_c": area.map(|state| state.use_c),
        "use_d": area.map(|state| state.use_d),
        "lf": area.map(|state| state.lf),
        "a_shift": area.map(|state| state.a_shift),
        "b_shift": area.map(|state| state.b_shift),
        "descending": area.map(|state| state.descending),
        "pointer_step": area.map(|state| state.pointer_step),
        "modulo_direction": area.map(|state| state.modulo_direction),
        "fill_enabled": area.map(|state| state.fill_enabled),
        "inclusive_fill_enabled": area.map(|state| state.inclusive_fill_enabled),
        "exclusive_fill_enabled": area.map(|state| state.exclusive_fill_enabled),
        "fill_carry_initial": area.map(|state| state.fill_carry_initial),
        "fill_carry": area.map(|state| state.fill_carry),
        "apt": area.map(|state| state.apt),
        "bpt": area.map(|state| state.bpt),
        "cpt": area.map(|state| state.cpt),
        "dpt": area.map(|state| state.dpt),
        "amod": area.map(|state| state.amod),
        "bmod": area.map(|state| state.bmod),
        "cmod": area.map(|state| state.cmod),
        "dmod": area.map(|state| state.dmod),
        "a_previous": area.map(|state| state.a_previous),
        "b_previous": area.map(|state| state.b_previous),
        "a_raw": area.map(|state| state.a_raw),
        "b_raw": area.map(|state| state.b_raw),
        "c_value": area.map(|state| state.c_value),
    });
    json!({
        // Compatibility leaves retained from the original compact snapshot.
        "busy": state.execution.busy,
        "busy_visible": state.execution.busy_visible,
        "busy_copper": state.execution.busy_copper,
        "exec_pending": state.execution.exec_pending,
        "startup_ccks_remaining": state.execution.startup_ccks_remaining,
        "ccks_remaining": state.execution.ccks_remaining,
        "completion_phase": completion_phase,
        "completion_ccks_remaining": state.execution.completion_ccks_remaining,
        "final_d_pending": state.execution.final_d_pending,
        "apt": state.registers.blt_apt,
        "bpt": state.registers.blt_bpt,
        "cpt": state.registers.blt_cpt,
        "dpt": state.registers.blt_dpt,
        "registers": state.registers,
        "execution": execution,
        "word": word,
        "line": line,
        "area": area,
    })
}

/// DF0 drive state. `motor_spinning` reports the mechanical state while
/// `ready_low` and the raw four-line `status` report the multiplexed drive
/// pins; READY can carry identification bits while the motor is off.
pub(crate) fn disk_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let drive = m.drive();
    let mechanism = drive.diagnostic_snapshot();
    let controller = m.paula().disk_diagnostic_snapshot();
    let track_stream = m.track_stream_diagnostic_snapshot();
    let mut snapshot = json!({
        "inserted": mechanism.has_disk,
        "writable": mechanism.disk_writable,
        "sectors_per_track": mechanism.sectors_per_track,
        "read_data_available": mechanism.read_data_available,
        "change_pending": mechanism.disk_change,
        "cylinder": mechanism.cylinder,
        "head": mechanism.head,
        "motor_on": mechanism.motor_on,
        "motor_spinning": mechanism.motor_spinning,
        "ready_low": mechanism.ready,
        "step_events": mechanism.step_event_counter,
        "selected": mechanism.selected,
        "status": {
            "disk_change_low": mechanism.disk_change,
            "write_protect_low": mechanism.write_protect,
            "track0_low": mechanism.track0,
            "ready_low": mechanism.ready,
        },
    })
    .as_object()
    .cloned()
    .expect("the base disk snapshot is an object");
    let mechanism_details = json!({
        "spin_timer": mechanism.spin_timer,
        "index_timer": mechanism.index_timer,
        "disk_changed_latch": mechanism.disk_changed,
        "prev_step": mechanism.prev_step,
        "write_capture_words": mechanism.write_mfm_capture_words,
        "write_pending_words": mechanism.write_mfm_pending_words,
        "id_shift_register": mechanism.id_shift_register,
        "id_bit": mechanism.id_bit,
        "id_ready_bit": mechanism.id_ready_bit,
        "write_protect_low": mechanism.write_protect,
        "track0_low": mechanism.track0,
    });
    let controller_registers = json!({
        "dskpt": m.agnus().dsk_pt,
        "dsklen": controller.dsklen,
        "dsksync": controller.dsksync,
        "dskdatr": controller.dskdatr,
        "dskdat": controller.dskdat,
        "dskbytr": m.paula().peek_dskbytr(m.dmacon()),
        "dskbytr_data": controller.dskbytr_data,
        "dskbytr_next_data": controller.dskbytr_next_data,
        "dskbytr_next_delay_cck": controller.dskbytr_next_delay_cck,
        "dskbytr_valid": controller.dskbytr_valid,
        "dskbytr_wordequal": controller.dskbytr_wordequal,
        "dskbytr_wordequal_delay_cck": controller.dskbytr_wordequal_delay_cck,
        "dskdat_queue": controller.dskdat_queue,
    });
    let controller_state = json!({
        "dsklen_armed": controller.dsklen_armed,
        "dma_pending": controller.disk_dma_pending,
        "dma_words_remaining": controller.disk_dma_words_remaining,
        "dma_is_write": controller.disk_dma_is_write,
        "dma_wordsync_waiting": controller.disk_dma_wordsync_waiting,
        "dma_write_active": controller.disk_dma_write_active,
        "dsklen_dma_enabled": controller.dsklen_dma_enabled,
        "dsklen_write_enabled": controller.dsklen_write_enabled,
        "wordsync_enabled": controller.wordsync_enabled,
        "fast_enabled": controller.fast_enabled,
        "disk_byte_delay_cck": controller.disk_byte_delay_cck,
        "pll_phase": controller.disk_pll_phase,
        "pll_variable_rate": controller.disk_pll_variable_rate,
    });
    let track_stream_state = json!({
        "track_cache_present": track_stream.cache_present,
        "track_cache_cylinder": track_stream.cache_cylinder,
        "track_cache_head": track_stream.cache_head,
        "track_cache_bytes": track_stream.cache_bytes,
        "track_word_count": track_stream.word_count,
        "track_word_cursor": track_stream.word_cursor,
        "track_pacer_ccks": track_stream.pacer_ccks,
        "track_word_interval_ccks": track_stream.word_interval_ccks,
    });
    for details in [
        mechanism_details,
        controller_registers,
        controller_state,
        track_stream_state,
    ] {
        snapshot.extend(
            details
                .as_object()
                .expect("the disk snapshot section is an object")
                .clone(),
        );
    }
    Value::Object(snapshot)
}

/// Dispatch the chipset chip groups shared by every variant (`agnus`,
/// `paula`, `cia`, `blitter`, `chipset`, `disk`). Returns `Some(value)`
/// for an owned group or leaf, and `None` both for a non-chip path and
/// for an unknown sub-field — the caller's own match then handles the
/// former and reports the latter as an unknown path. The AGA-only `aga`
/// group is routed separately by the AGA variant.
pub(crate) fn resolve_chip_query(m: &dyn AmigaLiveAccess, path: &str) -> Option<Value> {
    if is_chip(path, "agnus") {
        return chip_field(path, "agnus", agnus_snapshot(m));
    }
    if is_chip(path, "denise") {
        return chip_field(path, "denise", denise_snapshot(m));
    }
    if is_chip(path, "copper") {
        return chip_field(path, "copper", copper_snapshot(m));
    }
    if is_chip(path, "scheduler") {
        return chip_field(path, "scheduler", scheduler_snapshot(m));
    }
    if is_chip(path, "dma") {
        return chip_field(path, "dma", dma_snapshot(m));
    }
    if is_chip(path, "paula") {
        return chip_field(path, "paula", paula_snapshot(m));
    }
    if is_chip(path, "cia") {
        return chip_field(path, "cia", cia_snapshot(m));
    }
    if is_chip(path, "rtc") {
        return chip_field(path, "rtc", rtc_snapshot(m));
    }
    if is_chip(path, "keyboard") {
        return chip_field(path, "keyboard", keyboard_snapshot(m));
    }
    if is_chip(path, "gayle") {
        return gayle_snapshot(m).and_then(|snapshot| chip_field(path, "gayle", snapshot));
    }
    if is_chip(path, "input") {
        return chip_field(path, "input", input_snapshot(m));
    }
    if is_chip(path, "debug") {
        return chip_field(path, "debug", debug_snapshot(m));
    }
    if is_chip(path, "blitter") {
        return chip_field(path, "blitter", blitter_snapshot(m));
    }
    if is_chip(path, "chipset") {
        return chip_field(path, "chipset", chipset_snapshot(m));
    }
    if is_chip(path, "disk") {
        return chip_field(path, "disk", disk_snapshot(m));
    }
    None
}

/// AGA Lisa register + palette snapshot. AGA-only; the caller routes
/// this path only on the A1200 variant, where `aga_lisa()` is `Some`.
pub(crate) fn aga_snapshot(m: &dyn AmigaLiveAccess) -> Option<Value> {
    let aga = m.aga_lisa()?;
    let bplcon3 = aga.bplcon3;
    let mut bank_nonzero: [u32; 8] = [0; 8];
    for (i, &c) in aga.palette_24.iter().enumerate() {
        if c != 0 {
            bank_nonzero[i / 32] += 1;
        }
    }
    let bank0: Vec<u32> = aga.palette_24[0..32].to_vec();
    let ocs_palette: Vec<u16> = (0..32).map(|i| m.color(i)).collect();
    Some(json!({
        "deniseid": aga.deniseid,
        "bplcon3": bplcon3,
        "bplcon3_bank": (bplcon3 >> 13) & 7,
        "bplcon3_loct": (bplcon3 & 0x0200) != 0,
        "bplcon4": aga.bplcon4,
        "spr_width": aga.spr_width,
        "ham_prev_rgb24": aga.ham_prev_rgb24,
        "programmed_hblank_active": aga.programmed_hblank_active,
        "palette_24_nonzero_per_bank": bank_nonzero,
        "palette_24_bank0": bank0,
        "ocs_palette_12bit": ocs_palette,
    }))
}

#[cfg(test)]
mod tests {
    use super::SHARED_QUERY_PATHS;

    /// Catalogue invariant: every advertised shared path is unique.
    /// Doubles would silently clobber each other in a sorted listing.
    /// The variant catalogues are checked separately in `variants.rs`
    /// (one test per variant impl).
    #[test]
    fn shared_query_paths_are_unique() {
        let mut sorted: Vec<&&str> = SHARED_QUERY_PATHS.iter().collect();
        sorted.sort();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(sorted.len(), deduped.len(), "duplicate shared query paths");
    }
}
