//! Per-variant trait + impls for the Amiga family.
//!
//! Three chipset-tier machines currently implement the shared surface:
//! OCS-shaped A1000/A500/A2000 configurations, ECS A500+/A600
//! configurations, and the AGA A1200. PAL and NTSC regions, RAM layouts,
//! and the A500's optional GVP A530 accelerator remain canonical
//! configuration rather than parallel machine types. Future chipset tiers
//! can implement `AmigaMachine` without reshaping the runtime.
//!
//! See `knowledge/decisions/runtime-internal-shape.md` for the playbook
//! and the Amiga long-term-scope memory note for the full target
//! list (Vampire AC68080 + SAGA + RTG framebuffer slots, plus the
//! PAL/NTSC region matrix with NTSC's short/long line alternation
//! still pending in the chip layer).

use emu198x_shell::QueryError;
use format_commodore_amiga_adf::Adf;
use gvp_a530::A530Config;
use machine_commodore_amiga_a1200::{AmigaA1200, AmigaA1200Snapshot};
// `CiaExt::power_led` — the LED-filter gate. The trait is re-exported
// identically by every Amiga machine crate (it originates in
// `common_commodore_amiga::cia`); one import covers all three variants.
use machine_commodore_amiga_ecs::CiaExt;
use machine_commodore_amiga_ecs::{AmigaEcs, AmigaEcsSnapshot};
use machine_commodore_amiga_ocs::{
    AgnusRegion, AmigaOcs, AmigaOcsSnapshot, AutoconfigBoard, AutoconfigState, FB_HEIGHT, FB_WIDTH,
};
use motorola_68000::CpuModel;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::live_access::AmigaLiveAccess;
use crate::queries::{aga_snapshot, chip_field, is_chip, resolve_chip_query};
use crate::{
    A500_NTSC_CCK_HZ, A500_PAL_CCK_HZ, Accelerator, AmigaConfig, AmigaRuntime, CpuTraceEntry, Model,
};

/// Per-variant machine surface for the Amiga family.
///
/// Implemented by every concrete machine type that wants to plug
/// into `AmigaRuntime<M>`. The trait is deliberately agnostic to:
///
///   * which supported Motorola CPU is running through `ActiveCpu`
///   * which chipset is producing the chipset framebuffer (OCS,
///     ECS, or AGA today; SAGA remains a future chip-stack
///     replacement rather than an OCS wrapper)
///   * which graphics output is in use (chipset-only today; RTG
///     cards arrive via a slotted framebuffer accessor without
///     reshaping the trait)
pub trait AmigaMachine {
    /// Chipset framebuffer width in pixels (Denise / Lisa / SAGA
    /// video output, before any host-side scaling).
    const CHIPSET_FB_WIDTH: u32;

    /// Chipset framebuffer height in pixels.
    const CHIPSET_FB_HEIGHT: u32;

    /// Snapshot envelope for the chip stack. Encoded by serde +
    /// postcard inside the runtime's snapshot envelope.
    type Snapshot: Serialize + DeserializeOwned;

    // ---------- clock / lifecycle ----------

    /// Build a fresh chip stack from firmware and canonical construction
    /// configuration. Snapshot restore uses this to validate a candidate
    /// machine without touching the live one.
    fn build(firmware: &[u8], config: AmigaConfig) -> Self
    where
        Self: Sized;

    /// Rebuild the chip stack from scratch using the supplied firmware
    /// and canonical construction configuration. Drives
    /// `MachineCore::reset`.
    fn rebuild(&mut self, firmware: &[u8], config: AmigaConfig)
    where
        Self: Sized,
    {
        *self = Self::build(firmware, config);
    }

    /// Advance the machine by one tick (master / 4 = half-CCK).
    fn tick(&mut self);

    /// Advance either to the next CPU instruction boundary or through one
    /// complete system tick when no boundary is crossed.
    ///
    /// Faster processors may return at a boundary while retaining later CPU
    /// edges from the same system tick. The next call resumes those edges
    /// before advancing the chipset.
    fn advance_to_cpu_boundary(&mut self) -> bool;

    /// Drain every CPU instruction boundary retained by the machine.
    ///
    /// The queue decouples CPU edges from runtime observations: a faster
    /// processor can cross multiple instruction boundaries during one Amiga
    /// system tick, and the runtime must preserve each one for tracing.
    fn drain_cpu_boundaries(&mut self) -> impl Iterator<Item = CpuTraceEntry>;

    /// Nominal number of machine ticks in one video field for this variant.
    /// PAL OCS = 312 lines × 227 CCK × 2 ticks/CCK = 141,648. The
    /// runtime uses this as its host pacing quantum; actual field
    /// delivery follows [`Self::video_field_count`] because interlace
    /// can add a line after a field has already begun.
    fn frame_ticks(&self) -> u64;

    /// Number of video fields completed by the display pipeline.
    ///
    /// This counter is the authoritative output boundary. It prevents runtime
    /// frame delivery from cutting a long interlace field short when guest
    /// software changes `LACE` during the field, and from publishing after
    /// Agnus wraps while Denise still owns carried pixels from the prior row.
    fn video_field_count(&self) -> u64;

    /// Master CCK rate in Hz. Used by the runtime to downsample
    /// Paula audio to the host sample rate. PAL ≈ 3,546,895.
    fn cck_hz(&self) -> u64;

    // ---------- video ----------

    /// Borrow the chipset framebuffer as ARGB pixels. Length always
    /// equals `CHIPSET_FB_WIDTH * CHIPSET_FB_HEIGHT`.
    fn chipset_framebuffer(&self) -> &[u32];

    // ---------- audio ----------

    /// One stereo audio sample (left, right) at the current CCK.
    /// The runtime's resampler invokes this every machine tick so
    /// the sample stream is continuous across frame boundaries.
    fn mix_audio_stereo(&self) -> (f32, f32);

    /// Whether the switchable LED audio filter is currently engaged —
    /// CIA-A's power-LED line (PRA bit 1) is bright. The runtime feeds
    /// this to Paula's analog filter chain each host sample.
    fn led_filter_engaged(&self) -> bool;

    // ---------- input ----------

    fn key_event(&mut self, code: u8, pressed: bool);
    fn move_mouse_port0(&mut self, dx: i32, dy: i32);
    fn set_mouse_button_port0(&mut self, button: &str, pressed: bool);
    fn set_joystick_control(&mut self, port: u8, name: &str, pressed: bool);

    // ---------- media ----------

    /// Insert an ADF image into DF0. The `change_pending` flag drives
    /// the disk-change-pending bookkeeping the A1000 boot path needs;
    /// post-A1000 firmware boots happily without it.
    fn insert_floppy0(&mut self, adf: Adf, change_pending: bool);

    // ---------- snapshot ----------

    fn snapshot_state(&self) -> Self::Snapshot;
    fn restore_snapshot_state(&mut self, snapshot: Self::Snapshot);
    /// Confirm that restored machine state still represents the canonical
    /// construction configuration carried by the runtime envelope.
    fn validate_configuration(&self, config: AmigaConfig) -> Result<(), String>;

    // ---------- queries ----------

    /// Variant-specific query path catalogue. The runtime adds the
    /// shared `boot.*` and `amiga.machine.*` paths on top of this.
    fn variant_query_paths() -> &'static [&'static str];

    /// Resolve a variant-specific query path. Returns `Ok(None)` if
    /// the variant doesn't recognise the path so the runtime can
    /// surface UnknownPath cleanly.
    fn resolve_variant_query(&self, path: &str) -> Result<Option<Value>, QueryError>;
}

struct MachineConfigurationState<'a> {
    region: AgnusRegion,
    agnus_id: u16,
    cpu_model: CpuModel,
    cpu_variant_state_coherent: bool,
    cpu_clock_numerator: u64,
    cpu_clock_denominator: u64,
    cpu_domain_phase_coherent: bool,
    chip_ram_bytes: usize,
    slow_ram_bytes: usize,
    fast_ram: Option<&'a AutoconfigBoard>,
    a530_config: Option<A530Config>,
    a530_ram_bytes: Option<u32>,
    a530_autoconfig_state: Option<AutoconfigState>,
    a530_configuration_coherent: bool,
    synchronized_bridge_present: bool,
    synchronized_bridge_coherent: bool,
}

fn validate_machine_configuration(
    config: AmigaConfig,
    state: MachineConfigurationState<'_>,
) -> Result<(), String> {
    let expected_region = if config.model().is_ntsc() {
        AgnusRegion::Ntsc
    } else {
        AgnusRegion::Pal
    };
    if state.region != expected_region {
        return Err(format!(
            "video region mismatch: machine is {:?}, configuration requires {expected_region:?}",
            state.region
        ));
    }
    let expected_agnus_id = expected_agnus_id(config.model());
    if state.agnus_id != expected_agnus_id {
        return Err(format!(
            "Agnus identity mismatch: machine reports ${:04X}, configuration requires ${expected_agnus_id:04X}",
            state.agnus_id
        ));
    }

    let expected_cpu_model = config.cpu().model();
    if state.cpu_model != expected_cpu_model {
        return Err(format!(
            "CPU mismatch: machine is {:?}, configuration requires {expected_cpu_model:?}",
            state.cpu_model
        ));
    }
    if !state.cpu_variant_state_coherent {
        return Err("processor-family state is not coherent with the active CPU variant".into());
    }

    let system_tick_hz = if config.model().is_ntsc() {
        A500_NTSC_CCK_HZ * 2
    } else {
        A500_PAL_CCK_HZ * 2
    };
    let divisor = greatest_common_divisor(config.cpu().clock_hz(), system_tick_hz);
    let expected_clock_numerator = config.cpu().clock_hz() / divisor;
    let expected_clock_denominator = system_tick_hz / divisor;
    if state.cpu_clock_numerator != expected_clock_numerator
        || state.cpu_clock_denominator != expected_clock_denominator
    {
        return Err(format!(
            "CPU clock mismatch: machine is {}/{}, configuration requires {}/{}",
            state.cpu_clock_numerator,
            state.cpu_clock_denominator,
            expected_clock_numerator,
            expected_clock_denominator
        ));
    }
    if !state.cpu_domain_phase_coherent {
        return Err("partial CPU-domain state is not coherent with its clock".into());
    }

    let ram = config.ram();
    if state.chip_ram_bytes != ram.chip_kb as usize * 1024
        || state.slow_ram_bytes != ram.slow_kb as usize * 1024
    {
        return Err(format!(
            "motherboard RAM mismatch: machine has {} chip/{} slow bytes, configuration requires {} chip/{} slow bytes",
            state.chip_ram_bytes,
            state.slow_ram_bytes,
            ram.chip_kb as usize * 1024,
            ram.slow_kb as usize * 1024
        ));
    }
    let expected_fast_ram_bytes = [8192, 4096, 2048, 1024, 512, 256, 128, 64]
        .into_iter()
        .find(|&kib| ram.fast_kb >= kib)
        .map(|kib| kib * 1024);
    if state.fast_ram.map(AutoconfigBoard::ram_size) != expected_fast_ram_bytes {
        return Err(format!(
            "generic Fast RAM mismatch: machine has {:?} bytes, configuration requires {expected_fast_ram_bytes:?}",
            state.fast_ram.map(AutoconfigBoard::ram_size)
        ));
    }
    if state.fast_ram.is_some_and(|board| {
        !board.configuration_is_coherent() || !board.has_default_fast_ram_identity()
    }) {
        return Err(
            "generic Fast RAM backing, Autoconfig identity, and configuration disagree".into(),
        );
    }

    let expected_a530 = config
        .accelerator()
        .map(|Accelerator::GvpA530(board)| board);
    if state.a530_config != expected_a530 {
        return Err(format!(
            "GVP A530 mismatch: machine has {:?}, configuration requires {expected_a530:?}",
            state.a530_config
        ));
    }
    let expected_a530_ram_bytes = expected_a530.map(|board| board.ram_size().bytes());
    if state.a530_ram_bytes != expected_a530_ram_bytes {
        return Err(format!(
            "GVP A530 local RAM mismatch: machine has {:?} bytes, configuration requires {expected_a530_ram_bytes:?}",
            state.a530_ram_bytes
        ));
    }
    if state.a530_autoconfig_state.is_some() != expected_a530.is_some() {
        return Err(format!(
            "GVP A530 Autoconfig state mismatch: present={}, expected={}",
            state.a530_autoconfig_state.is_some(),
            expected_a530.is_some()
        ));
    }
    if !state.a530_configuration_coherent {
        return Err("GVP A530 backing, Autoconfig identity, and configuration disagree".into());
    }
    validate_autoconfig_chain(
        state.a530_autoconfig_state,
        state.a530_ram_bytes,
        state.fast_ram,
    )?;
    if state.synchronized_bridge_present != expected_a530.is_some() {
        return Err(format!(
            "synchronized bridge mismatch: present={}, expected={}",
            state.synchronized_bridge_present,
            expected_a530.is_some()
        ));
    }
    if !state.synchronized_bridge_coherent {
        return Err("synchronized bridge state does not belong to the waiting CPU cycle".into());
    }
    Ok(())
}

const fn expected_agnus_id(model: Model) -> u16 {
    if model.is_aga() {
        if model.is_ntsc() { 0x3300 } else { 0x2300 }
    } else if model.is_ecs() || model.uses_fat_agnus_8372a() {
        if model.is_ntsc() { 0x3000 } else { 0x2000 }
    } else if model.is_ntsc() {
        0x1000
    } else {
        0x0000
    }
}

fn validate_autoconfig_chain(
    first_state: Option<AutoconfigState>,
    first_ram_bytes: Option<u32>,
    downstream: Option<&AutoconfigBoard>,
) -> Result<(), String> {
    let Some(first_state) = first_state else {
        return Ok(());
    };

    if matches!(
        first_state,
        AutoconfigState::Unconfigured | AutoconfigState::WaitingHighBase { .. }
    ) && downstream.is_some_and(|board| board.state() != AutoconfigState::Unconfigured)
    {
        return Err(
            "downstream Fast RAM advanced before the preceding GVP A530 left the probe window"
                .into(),
        );
    }

    let (AutoconfigState::Configured { base: first_base }, Some(first_size), Some(downstream)) =
        (first_state, first_ram_bytes, downstream)
    else {
        return Ok(());
    };
    let Some(downstream_base) = downstream.base() else {
        return Ok(());
    };

    let first_end = first_base
        .checked_add(first_size)
        .ok_or_else(|| "GVP A530 mapped range overflows the address space".to_owned())?;
    let downstream_end = downstream_base
        .checked_add(downstream.ram_size())
        .ok_or_else(|| "generic Fast RAM mapped range overflows the address space".to_owned())?;
    if first_base < downstream_end && downstream_base < first_end {
        return Err("configured Autoconfig board ranges overlap".into());
    }

    Ok(())
}

const fn greatest_common_divisor(mut lhs: u64, mut rhs: u64) -> u64 {
    while rhs != 0 {
        let remainder = lhs % rhs;
        lhs = rhs;
        rhs = remainder;
    }
    lhs
}

// ===================================================================
// AmigaOcs impl — covers A1000 + A500 family + A500+ + maxed-A500.
// ===================================================================

macro_rules! amiga_variant_query_paths {
    ($($variant_path:expr),* $(,)?) => {
        &[
    "memory.overlay",
    "cpu.pc",
    "cpu.sr",
    "cpu.ipl",
    // Folded chip groups (#456). Every top-level field is also
    // advertised as a leaf so discovery never requires guessing a
    // path after inspecting the grouped object.
    "chipset",
    "agnus",
    "denise",
    "copper",
    "scheduler",
    "blitter",
    "paula",
    "cia",
    "keyboard",
    "input",
    "debug",
    "disk",
    "chipset.bplcon0",
    "chipset.bplcon3",
    "chipset.dmacon",
    "chipset.adkcon",
    "chipset.color00",
    "chipset.cop1lc",
    "chipset.cop2lc",
    "chipset.copper_pc",
    "chipset.overlay",
    "chipset.ecsena_enabled",
    "chipset.extblken_enabled",
    "chipset.blanken_enabled",
    "chipset.programmed_hblank_output_active",
    "agnus.vpos",
    "agnus.hpos",
    "agnus.dmacon",
    "agnus.bplcon0",
    "agnus.blitter_busy",
    "agnus.blitter_busy_visible",
    "agnus.blitter_busy_copper",
    "agnus.blitter_exec_pending",
    "agnus.blitter_startup_ccks_remaining",
    "agnus.blitter_ccks_remaining",
    "agnus.blitter_completion_phase",
    "agnus.blitter_completion_ccks_remaining",
    "agnus.blitter_final_d_pending",
    "agnus.bpl_pt",
    "agnus.blt_apt",
    "agnus.blt_bpt",
    "agnus.blt_cpt",
    "agnus.blt_dpt",
    "agnus.fmode",
    "agnus.bpl_fetch_width",
    "agnus.spr_fetch_width",
    "agnus.diwstrt",
    "agnus.diwstop",
    "agnus.ddfstrt",
    "agnus.ddfstop",
    "agnus.bpl1mod",
    "agnus.bpl2mod",
    "agnus.num_bitplanes",
    "agnus.beamcon0",
    "agnus.htotal",
    "agnus.hsstop",
    "agnus.hbstrt",
    "agnus.hbstop",
    "agnus.vtotal",
    "agnus.vsstop",
    "agnus.vbstrt",
    "agnus.vbstop",
    "agnus.hsstrt",
    "agnus.vsstrt",
    "agnus.diwhigh",
    "agnus.diwhigh_written",
    "agnus.bltsizv",
    "agnus.bltsizh",
    "agnus.programmed_vertical_accessed",
    "agnus.programmed_vblank_active",
    "agnus.programmed_vblank_start_event",
    "agnus.programmed_vblank_stop_event",
    "agnus.programmed_hblank_active",
    "agnus.programmed_hblank_routed_active",
    "agnus.vertical_diw_active",
    "agnus.current_line_ccks",
    "agnus.copper_comparator_hpos",
    "agnus.pal_enabled",
    "agnus.dual_enabled",
    "agnus.varbeamen_enabled",
    "agnus.varvben_enabled",
    "agnus.varvsyen_enabled",
    "agnus.varhsyen_enabled",
    "agnus.cscben_enabled",
    "agnus.varcsyen_enabled",
    "agnus.harddis_enabled",
    "agnus.blanken_enabled",
    "agnus.loldis_enabled",
    "agnus.lpendis_enabled",
    "agnus.csytrue_enabled",
    "agnus.vsytrue_enabled",
    "agnus.hsytrue_enabled",
    "agnus.harddis_hblank_window_active",
    "agnus.vblank_window_active",
    "agnus.hsync_window_active",
    "agnus.vsync_window_active",
    "agnus.sync_pin_hsync",
    "agnus.sync_pin_vsync",
    "agnus.sync_pin_csync",
    "agnus.sync_pin_blank",
    "denise.palette_12",
    "denise.palette_24",
    "denise.raster_width",
    "denise.raster_height",
    "denise.framebuffer_pixels",
    "denise.interlace_active",
    "denise.long_frame",
    "denise.maximum_bitplanes",
    "denise.active_bitplanes",
    "denise.bplcon0",
    "denise.bplcon1",
    "denise.bplcon2",
    "denise.bplcon4",
    "denise.clxcon",
    "denise.clxdat",
    "denise.bitplanes",
    "denise.bitplanes.holding_data",
    "denise.bitplanes.shift_data",
    "denise.bitplanes.aggregate_shift_count",
    "denise.bitplanes.shift_counts",
    "denise.bitplanes.shift_delays",
    "denise.bitplanes.previous_data",
    "denise.bitplanes.pending_data",
    "denise.bitplanes.pending_copy_odd_planes",
    "denise.bitplanes.pending_copy_even_planes",
    "denise.bitplanes.scroll_pending_line",
    "denise.bitplanes.active_fifo",
    "denise.bitplanes.active_fifo_lengths",
    "denise.bitplanes.staged_fetch_tails",
    "denise.bitplanes.staged_fetch_tail_lengths",
    "denise.bitplanes.deferred_shift_load_source_pixels",
    "denise.sprite_width",
    "denise.sprites",
    "denise.sprite_bpl1dat_enabled",
    "denise.sprite_runtime_line_valid",
    "denise.sprite_runtime_beam_x",
    "denise.sprite_runtime_beam_y",
    "denise.ham_previous_rgb12",
    "denise.ham_previous_rgb24",
    "denise.last_shift_load",
    "denise.last_shift_load.hires",
    "denise.last_shift_load.odd_scroll",
    "denise.last_shift_load.even_scroll",
    "denise.last_shift_load.num_bitplanes",
    "denise.last_shift_load.planes",
    "denise.deniseid",
    "denise.bplcon3",
    "denise.ecsena_enabled",
    "denise.extblken_enabled",
    "denise.shres_enabled",
    "denise.bplhwrm_enabled",
    "denise.sprhwrm_enabled",
    "denise.bplcon3_extensions_enabled",
    "denise.border_blank_enabled",
    "denise.border_opaque_enabled",
    "denise.killehb_enabled",
    "denise.programmed_hblank_active",
    "copper.pc",
    "copper.cop1lc",
    "copper.cop2lc",
    "copper.waiting",
    "copper.wait_target",
    "copper.wait_mask",
    "copper.wait_bfd",
    "copper.cck_phase",
    "copper.pending_wait_delay",
    "copper.pending_wait_target",
    "copper.pending_wait_mask",
    "copper.pending_wait_bfd",
    "copper.pending_wait_is_skip",
    "copper.stopped",
    "copper.cdang",
    "copper.bus_used_this_cck",
    "copper.move_log_count",
    "copper.last_move",
    "scheduler.tick_count",
    "scheduler.cck_count",
    "scheduler.cck_phase",
    "scheduler.e_clock_phase",
    "scheduler.prev_vertb_level",
    "scheduler.prev_cia_a_irq",
    "scheduler.prev_cia_b_irq",
    "scheduler.prev_cia_a_spmode",
    "scheduler.cpu_clock_numerator",
    "scheduler.cpu_clock_denominator",
    "scheduler.cpu_clock_phase",
    "scheduler.cpu_clock_maximum_edges_per_tick",
    "scheduler.cpu_domain_idle",
    "scheduler.cpu_domain_edges_remaining",
    "scheduler.cpu_domain_motherboard_slot_pending",
    "scheduler.cpu_domain_coherent",
    "scheduler.pending_cpu_boundary_count",
    "scheduler.pending_cpu_boundaries",
    "scheduler.pending_cpu_boundary_capacity",
    "scheduler.pending_cpu_boundary_at_capacity",
    "blitter.busy",
    "blitter.busy_visible",
    "blitter.busy_copper",
    "blitter.exec_pending",
    "blitter.startup_ccks_remaining",
    "blitter.ccks_remaining",
    "blitter.completion_phase",
    "blitter.completion_ccks_remaining",
    "blitter.final_d_pending",
    "blitter.apt",
    "blitter.bpt",
    "blitter.cpt",
    "blitter.dpt",
    "paula.intena",
    "paula.intreq",
    "paula.adkcon",
    "paula.active_sources",
    "paula.ipl",
    "paula.master_enable",
    "paula.intena_bits",
    "paula.intreq_bits",
    "paula.active_source_bits",
    "paula.adkcon_bits",
    "paula.intena_bits.TBE",
    "paula.intena_bits.DSKBLK",
    "paula.intena_bits.SOFT",
    "paula.intena_bits.PORTS",
    "paula.intena_bits.COPER",
    "paula.intena_bits.VERTB",
    "paula.intena_bits.BLIT",
    "paula.intena_bits.AUD0",
    "paula.intena_bits.AUD1",
    "paula.intena_bits.AUD2",
    "paula.intena_bits.AUD3",
    "paula.intena_bits.RBF",
    "paula.intena_bits.DSKSYN",
    "paula.intena_bits.EXTER",
    "paula.intena_bits.INTEN",
    "paula.intreq_bits.TBE",
    "paula.intreq_bits.DSKBLK",
    "paula.intreq_bits.SOFT",
    "paula.intreq_bits.PORTS",
    "paula.intreq_bits.COPER",
    "paula.intreq_bits.VERTB",
    "paula.intreq_bits.BLIT",
    "paula.intreq_bits.AUD0",
    "paula.intreq_bits.AUD1",
    "paula.intreq_bits.AUD2",
    "paula.intreq_bits.AUD3",
    "paula.intreq_bits.RBF",
    "paula.intreq_bits.DSKSYN",
    "paula.intreq_bits.EXTER",
    "paula.intreq_bits.INTEN",
    "paula.active_source_bits.TBE",
    "paula.active_source_bits.DSKBLK",
    "paula.active_source_bits.SOFT",
    "paula.active_source_bits.PORTS",
    "paula.active_source_bits.COPER",
    "paula.active_source_bits.VERTB",
    "paula.active_source_bits.BLIT",
    "paula.active_source_bits.AUD0",
    "paula.active_source_bits.AUD1",
    "paula.active_source_bits.AUD2",
    "paula.active_source_bits.AUD3",
    "paula.active_source_bits.RBF",
    "paula.active_source_bits.DSKSYN",
    "paula.active_source_bits.EXTER",
    "paula.active_source_bits.INTEN",
    "paula.adkcon_bits.PRECOMP1",
    "paula.adkcon_bits.PRECOMP0",
    "paula.adkcon_bits.MFMPREC",
    "paula.adkcon_bits.UARTBRK",
    "paula.adkcon_bits.WORDSYNC",
    "paula.adkcon_bits.MSBSYNC",
    "paula.adkcon_bits.FAST",
    "paula.adkcon_bits.USE3PN",
    "paula.adkcon_bits.USE2P3",
    "paula.adkcon_bits.USE1P2",
    "paula.adkcon_bits.USE0P1",
    "paula.adkcon_bits.USE3VN",
    "paula.adkcon_bits.USE2V3",
    "paula.adkcon_bits.USE1V2",
    "paula.adkcon_bits.USE0V1",
    "paula.audio",
    "paula.audio.channels",
    "paula.audio.channels.channel0",
    "paula.audio.channels.channel0.location",
    "paula.audio.channels.channel0.dma_pointer",
    "paula.audio.channels.channel0.length_words",
    "paula.audio.channels.channel0.programmed_length_words",
    "paula.audio.channels.channel0.words_remaining",
    "paula.audio.channels.channel0.period",
    "paula.audio.channels.channel0.effective_period",
    "paula.audio.channels.channel0.volume",
    "paula.audio.channels.channel0.data",
    "paula.audio.channels.channel0.current_word",
    "paula.audio.channels.channel0.next_word",
    "paula.audio.channels.channel0.next_byte_is_high",
    "paula.audio.channels.channel0.period_counter",
    "paula.audio.channels.channel0.output_sample",
    "paula.audio.channels.channel0.state",
    "paula.audio.channels.channel0.dma_active",
    "paula.audio.channels.channel0.dma_enabled_previous",
    "paula.audio.channels.channel0.dma_requests_pending",
    "paula.audio.channels.channel0.period_modulation_enabled",
    "paula.audio.channels.channel0.volume_modulation_enabled",
    "paula.audio.channels.channel0.host_control",
    "paula.audio.channels.channel0.host_control.enabled",
    "paula.audio.channels.channel0.host_control.gain",
    "paula.audio.channels.channel1",
    "paula.audio.channels.channel1.location",
    "paula.audio.channels.channel1.dma_pointer",
    "paula.audio.channels.channel1.length_words",
    "paula.audio.channels.channel1.programmed_length_words",
    "paula.audio.channels.channel1.words_remaining",
    "paula.audio.channels.channel1.period",
    "paula.audio.channels.channel1.effective_period",
    "paula.audio.channels.channel1.volume",
    "paula.audio.channels.channel1.data",
    "paula.audio.channels.channel1.current_word",
    "paula.audio.channels.channel1.next_word",
    "paula.audio.channels.channel1.next_byte_is_high",
    "paula.audio.channels.channel1.period_counter",
    "paula.audio.channels.channel1.output_sample",
    "paula.audio.channels.channel1.state",
    "paula.audio.channels.channel1.dma_active",
    "paula.audio.channels.channel1.dma_enabled_previous",
    "paula.audio.channels.channel1.dma_requests_pending",
    "paula.audio.channels.channel1.period_modulation_enabled",
    "paula.audio.channels.channel1.volume_modulation_enabled",
    "paula.audio.channels.channel1.host_control",
    "paula.audio.channels.channel1.host_control.enabled",
    "paula.audio.channels.channel1.host_control.gain",
    "paula.audio.channels.channel2",
    "paula.audio.channels.channel2.location",
    "paula.audio.channels.channel2.dma_pointer",
    "paula.audio.channels.channel2.length_words",
    "paula.audio.channels.channel2.programmed_length_words",
    "paula.audio.channels.channel2.words_remaining",
    "paula.audio.channels.channel2.period",
    "paula.audio.channels.channel2.effective_period",
    "paula.audio.channels.channel2.volume",
    "paula.audio.channels.channel2.data",
    "paula.audio.channels.channel2.current_word",
    "paula.audio.channels.channel2.next_word",
    "paula.audio.channels.channel2.next_byte_is_high",
    "paula.audio.channels.channel2.period_counter",
    "paula.audio.channels.channel2.output_sample",
    "paula.audio.channels.channel2.state",
    "paula.audio.channels.channel2.dma_active",
    "paula.audio.channels.channel2.dma_enabled_previous",
    "paula.audio.channels.channel2.dma_requests_pending",
    "paula.audio.channels.channel2.period_modulation_enabled",
    "paula.audio.channels.channel2.volume_modulation_enabled",
    "paula.audio.channels.channel2.host_control",
    "paula.audio.channels.channel2.host_control.enabled",
    "paula.audio.channels.channel2.host_control.gain",
    "paula.audio.channels.channel3",
    "paula.audio.channels.channel3.location",
    "paula.audio.channels.channel3.dma_pointer",
    "paula.audio.channels.channel3.length_words",
    "paula.audio.channels.channel3.programmed_length_words",
    "paula.audio.channels.channel3.words_remaining",
    "paula.audio.channels.channel3.period",
    "paula.audio.channels.channel3.effective_period",
    "paula.audio.channels.channel3.volume",
    "paula.audio.channels.channel3.data",
    "paula.audio.channels.channel3.current_word",
    "paula.audio.channels.channel3.next_word",
    "paula.audio.channels.channel3.next_byte_is_high",
    "paula.audio.channels.channel3.period_counter",
    "paula.audio.channels.channel3.output_sample",
    "paula.audio.channels.channel3.state",
    "paula.audio.channels.channel3.dma_active",
    "paula.audio.channels.channel3.dma_enabled_previous",
    "paula.audio.channels.channel3.dma_requests_pending",
    "paula.audio.channels.channel3.period_modulation_enabled",
    "paula.audio.channels.channel3.volume_modulation_enabled",
    "paula.audio.channels.channel3.host_control",
    "paula.audio.channels.channel3.host_control.enabled",
    "paula.audio.channels.channel3.host_control.gain",
    "paula.audio.controls",
    "paula.audio.controls.master_gain",
    "paula.audio.controls.channels",
    "paula.serial",
    "paula.serial.serdat",
    "paula.serial.serper",
    "paula.serial.serdatr",
    "paula.serial.serdatr_bits",
    "paula.serial.serdatr_bits.OVRUN",
    "paula.serial.serdatr_bits.RBF",
    "paula.serial.serdatr_bits.TBE",
    "paula.serial.serdatr_bits.TSRE",
    "paula.serial.receive_data",
    "paula.serial.receive_full",
    "paula.serial.receive_overrun",
    "paula.pot",
    "paula.pot.potgo",
    "paula.pot.potgo_bits",
    "paula.pot.potgo_bits.OUTRY",
    "paula.pot.potgo_bits.DATRY",
    "paula.pot.potgo_bits.OUTLY",
    "paula.pot.potgo_bits.DATLY",
    "paula.pot.potgo_bits.OUTRX",
    "paula.pot.potgo_bits.DATRX",
    "paula.pot.potgo_bits.OUTLX",
    "paula.pot.potgo_bits.DATLX",
    "paula.pot.raw_pin_levels",
    "paula.pot.raw_pin_bits",
    "paula.pot.raw_pin_bits.OUTRY",
    "paula.pot.raw_pin_bits.DATRY",
    "paula.pot.raw_pin_bits.OUTLY",
    "paula.pot.raw_pin_bits.DATLY",
    "paula.pot.raw_pin_bits.OUTRX",
    "paula.pot.raw_pin_bits.DATRX",
    "paula.pot.raw_pin_bits.OUTLX",
    "paula.pot.raw_pin_bits.DATLX",
    "paula.pot.potgor",
    "paula.pot.potgor_bits",
    "paula.pot.potgor_bits.OUTRY",
    "paula.pot.potgor_bits.DATRY",
    "paula.pot.potgor_bits.OUTLY",
    "paula.pot.potgor_bits.DATLY",
    "paula.pot.potgor_bits.OUTRX",
    "paula.pot.potgor_bits.DATRX",
    "paula.pot.potgor_bits.OUTLX",
    "paula.pot.potgor_bits.DATLX",
    "paula.pot.pot0dat",
    "paula.pot.pot1dat",
    "paula.logs",
    "paula.logs.intena_writes",
    "paula.logs.intena_write_count",
    "paula.logs.last_intena_write",
    "paula.logs.intreq_writes",
    "paula.logs.intreq_write_count",
    "paula.logs.last_intreq_write",
    "paula.logs.disk_write_dma_count",
    "paula.logs.last_disk_write_dma_word",
    "paula.logs.disk_write_pio_count",
    "paula.logs.last_disk_write_pio_word",
    "cia.cia_a",
    "cia.cia_b",
    "cia.cia_a.cra",
    "cia.cia_a.crb",
    "cia.cia_a.timer_a",
    "cia.cia_a.timer_b",
    "cia.cia_a.timer_a_running",
    "cia.cia_a.timer_b_running",
    "cia.cia_a.icr_status",
    "cia.cia_a.icr_mask",
    "cia.cia_a.irq_active",
    "cia.cia_a.ddr_a",
    "cia.cia_a.ddr_b",
    "cia.cia_a.port_a_output",
    "cia.cia_a.port_b_output",
    "cia.cia_a.tod_counter",
    "cia.cia_a.tod_alarm",
    "cia.cia_a.tod_halted",
    "cia.cia_b.cra",
    "cia.cia_b.crb",
    "cia.cia_b.timer_a",
    "cia.cia_b.timer_b",
    "cia.cia_b.timer_a_running",
    "cia.cia_b.timer_b_running",
    "cia.cia_b.icr_status",
    "cia.cia_b.icr_mask",
    "cia.cia_b.irq_active",
    "cia.cia_b.ddr_a",
    "cia.cia_b.ddr_b",
    "cia.cia_b.port_a_output",
    "cia.cia_b.port_b_output",
    "cia.cia_b.tod_counter",
    "cia.cia_b.tod_alarm",
    "cia.cia_b.tod_halted",
    "keyboard.state",
    "keyboard.timer",
    "keyboard.queued",
    "keyboard.bytes_sent",
    "keyboard.cia_a_sdr",
    "keyboard.cia_a_spmode",
    "input.joy0_x",
    "input.joy0_y",
    "input.joy0dat",
    "input.joy1_x",
    "input.joy1_y",
    "input.joy1dat",
    "input.port0_primary_button_pressed",
    "input.port1_primary_button_pressed",
    "input.joystick1_up",
    "input.joystick1_down",
    "input.joystick1_left",
    "input.joystick1_right",
    "input.joystick1_fire",
    "input.joystick1_button2",
    "input.joystick1_button3",
    "input.potgo",
    "input.potgor",
    "input.pot_raw_pin_levels",
    "input.pot0dat",
    "input.pot1dat",
    "debug.register_read_counts",
    "debug.register_read_log_count",
    "debug.last_register_read",
    "debug.custom_write_log_count",
    "debug.last_custom_write",
    "debug.palette_log_count",
    "debug.last_palette_write",
    "debug.bplcon0_log_count",
    "debug.last_bplcon0_write",
    "debug.peak_intena",
    "debug.intena_write_count",
    "debug.intena_transition_count",
    "debug.last_intena_transition",
    "debug.dmacon_transition_count",
    "debug.last_dmacon_transition",
    "debug.cop1lc_write_count",
    "debug.last_cop1lc_write",
    "debug.cop2lc_write_count",
    "debug.last_cop2lc_write",
    "debug.dsk_write_count",
    "debug.last_dsk_write",
    "debug.blitter_start_count",
    "debug.blitter_log_count",
    "debug.last_blitter_start",
    "debug.cia_a_write_count",
    "debug.last_cia_a_write",
    "debug.cia_b_write_count",
    "debug.last_cia_b_write",
    "debug.cia_a_read_counts",
    "debug.cia_b_read_counts",
    "debug.rtc_access_count",
    "debug.last_rtc_access",
    "debug.watch_range",
    "debug.watch_write_count",
    "debug.last_watch_write",
    "display.color00",
    "display.color01",
    "disk.inserted",
    "disk.writable",
    "disk.sectors_per_track",
    "disk.read_data_available",
    "disk.change_pending",
    "disk.cylinder",
    "disk.head",
    "disk.motor_on",
    "disk.motor_spinning",
    "disk.ready_low",
    "disk.step_events",
    "disk.selected",
    "disk.status",
    "disk.status.disk_change_low",
    "disk.status.write_protect_low",
    "disk.status.track0_low",
    "disk.status.ready_low",
    "disk.spin_timer",
    "disk.index_timer",
    "disk.disk_changed_latch",
    "disk.prev_step",
    "disk.write_capture_words",
    "disk.write_pending_words",
    "disk.id_shift_register",
    "disk.id_bit",
    "disk.id_ready_bit",
    "disk.write_protect_low",
    "disk.track0_low",
    "disk.dskpt",
    "disk.dsklen",
    "disk.dsksync",
    "disk.dskdatr",
    "disk.dskdat",
    "disk.dskbytr",
    "disk.dskbytr_data",
    "disk.dskbytr_next_data",
    "disk.dskbytr_next_delay_cck",
    "disk.dskbytr_valid",
    "disk.dskbytr_wordequal",
    "disk.dskbytr_wordequal_delay_cck",
    "disk.dskdat_queue",
    "disk.dsklen_armed",
    "disk.dma_pending",
    "disk.dma_words_remaining",
    "disk.dma_is_write",
    "disk.dma_wordsync_waiting",
    "disk.dma_write_active",
    "disk.dsklen_dma_enabled",
    "disk.dsklen_write_enabled",
    "disk.wordsync_enabled",
    "disk.fast_enabled",
    "disk.disk_byte_delay_cck",
    "disk.pll_phase",
    "disk.pll_variable_rate",
    "disk.track_cache_present",
    "disk.track_cache_cylinder",
    "disk.track_cache_head",
    "disk.track_cache_bytes",
    "disk.track_word_count",
    "disk.track_word_cursor",
    "disk.track_pacer_ccks",
    "disk.track_word_interval_ccks",
    "sprite0.dma_on",
    "sprite0.vstart",
    "sprite0.vstop",
    "sprite0.pixels_rendered",
    "sprite0.ptr",
    $($variant_path),*
        ]
    };
}

const OCS_VARIANT_QUERY_PATHS: &[&str] =
    amiga_variant_query_paths!("a1000.boot_rom_visible", "a1000.wom_locked",);

impl AmigaMachine for AmigaOcs {
    const CHIPSET_FB_WIDTH: u32 = FB_WIDTH;
    const CHIPSET_FB_HEIGHT: u32 = FB_HEIGHT;

    type Snapshot = AmigaOcsSnapshot;

    fn build(firmware: &[u8], config: AmigaConfig) -> Self {
        crate::runtime::build_amiga_ocs(config, firmware)
    }

    fn tick(&mut self) {
        AmigaOcs::tick(self);
    }

    fn advance_to_cpu_boundary(&mut self) -> bool {
        AmigaOcs::advance_to_cpu_boundary(self)
    }

    fn drain_cpu_boundaries(&mut self) -> impl Iterator<Item = CpuTraceEntry> {
        AmigaOcs::drain_cpu_boundaries(self).map(|boundary| {
            (
                boundary.system_tick,
                boundary.instr_start_pc,
                boundary.sr,
                boundary.opcode,
            )
        })
    }

    fn frame_ticks(&self) -> u64 {
        // Region-aware: PAL = 141,648 ticks (312 × 227 × 2). NTSC =
        // 119,210 ticks (131 × 227 + 131 × 228, then × 2). The
        // chip-layer alternation handles the per-line short/long
        // distinction; the runtime needs only the frame total.
        match self.region() {
            AgnusRegion::Pal => crate::A500_PAL_FRAME_TICKS,
            AgnusRegion::Ntsc => crate::A500_NTSC_FRAME_TICKS,
        }
    }

    fn video_field_count(&self) -> u64 {
        self.denise()
            .completed_display_field_count(self.agnus().vbl_count)
    }

    fn cck_hz(&self) -> u64 {
        match self.region() {
            AgnusRegion::Pal => crate::A500_PAL_CCK_HZ,
            AgnusRegion::Ntsc => crate::A500_NTSC_CCK_HZ,
        }
    }

    fn chipset_framebuffer(&self) -> &[u32] {
        self.denise().framebuffer()
    }

    fn mix_audio_stereo(&self) -> (f32, f32) {
        self.paula().mix_audio_stereo()
    }

    fn led_filter_engaged(&self) -> bool {
        self.cia_a().power_led()
    }

    fn key_event(&mut self, code: u8, pressed: bool) {
        AmigaOcs::key_event(self, code, pressed);
    }

    fn move_mouse_port0(&mut self, dx: i32, dy: i32) {
        AmigaOcs::move_mouse_port0(self, dx, dy);
    }

    fn set_mouse_button_port0(&mut self, button: &str, pressed: bool) {
        AmigaOcs::set_mouse_button_port0(self, button, pressed);
    }

    fn set_joystick_control(&mut self, port: u8, name: &str, pressed: bool) {
        let _ = AmigaOcs::set_joystick_control(self, port, name, pressed);
    }

    fn insert_floppy0(&mut self, adf: Adf, change_pending: bool) {
        if change_pending {
            self.insert_adf_with_change_pending(adf);
        } else {
            self.insert_adf(adf);
        }
    }

    fn snapshot_state(&self) -> Self::Snapshot {
        AmigaOcs::snapshot_state(self)
    }

    fn restore_snapshot_state(&mut self, snapshot: Self::Snapshot) {
        AmigaOcs::restore_snapshot_state(self, snapshot);
    }

    fn validate_configuration(&self, config: AmigaConfig) -> Result<(), String> {
        validate_machine_configuration(
            config,
            MachineConfigurationState {
                region: self.region(),
                agnus_id: self.agnus().agnus_id,
                cpu_model: self.active_cpu().model(),
                cpu_variant_state_coherent: self.active_cpu().variant_state_is_coherent(),
                cpu_clock_numerator: self.cpu_clock().numerator(),
                cpu_clock_denominator: self.cpu_clock().denominator(),
                cpu_domain_phase_coherent: self.cpu_domain_phase_is_coherent(),
                chip_ram_bytes: self.memory().chip_ram_size(),
                slow_ram_bytes: self.memory().slow_ram_size(),
                fast_ram: self.autoconfig(),
                a530_config: self.gvp_a530().map(|board| board.config()),
                a530_ram_bytes: self.gvp_a530().map(|board| board.ram_size()),
                a530_autoconfig_state: self.gvp_a530().map(|board| board.autoconfig_state()),
                a530_configuration_coherent: self
                    .gvp_a530()
                    .is_none_or(|board| board.configuration_is_coherent()),
                synchronized_bridge_present: self.has_synchronized_motherboard_bridge(),
                synchronized_bridge_coherent: self.motherboard_bridge_is_coherent(),
            },
        )?;
        if self.uses_fat_agnus_8372a() != config.model().uses_fat_agnus_8372a() {
            return Err("installed OCS-shaped Agnus revision does not match configuration".into());
        }
        Ok(())
    }

    fn variant_query_paths() -> &'static [&'static str] {
        OCS_VARIANT_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<Value>, QueryError> {
        // Folded chip snapshots (#456): grouped `agnus` / `paula` / `cia`
        // / `blitter` / `chipset` / `disk` objects + per-field leaves.
        if let Some(value) = resolve_chip_query(self, path) {
            return Ok(Some(value));
        }
        let value = match path {
            "a1000.boot_rom_visible" => json!(self.memory().a1000_boot_rom_visible()),
            "a1000.wom_locked" => json!(self.memory().a1000_wom_locked()),
            "memory.overlay" => json!(self.memory().overlay()),
            "cpu.pc" => json!(self.cpu().regs.pc),
            "cpu.sr" => json!(self.cpu().regs.sr),
            "cpu.ipl" => json!(self.cpu().ipl),
            "sprite0.dma_on" => json!(self.agnus().sprite_dma_on(0)),
            "sprite0.vstart" => json!(self.agnus().sprite_vstart(0)),
            "sprite0.vstop" => json!(self.agnus().sprite_vstop(0)),
            "sprite0.pixels_rendered" => {
                json!(self.denise().ocs.sprite_pixels_rendered(0))
            }
            "sprite0.ptr" => json!(self.agnus().spr_pt[0]),
            "debug.dsk_write_count" => json!(self.debug_dsk_log.len()),
            "debug.last_dsk_write" => {
                json!(self.debug_dsk_log.last().map(|(cck, pc, reg, val)| {
                    json!({"cck": cck, "pc": pc, "reg": reg, "val": val})
                }))
            }
            "display.color00" => json!(self.color(0)),
            "display.color01" => json!(self.color(1)),
            "keyboard.state" => json!(self.keyboard().debug_state_name()),
            "keyboard.queued" => json!(self.keyboard().queued_key_count()),
            "input.joy0dat" => json!(self.joy0dat()),
            "input.joy1dat" => json!(self.joy1dat()),
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}

/// Type alias for the OCS-shaped runtime. Covers the A1000, A500
/// family and A2000B profiles. Fat Agnus 8372A configurations remain
/// in this arm because they retain OCS Denise, while Agnus-side
/// capabilities are selected independently from the installed chip
/// RAM layout.
pub type AmigaOcsRuntime = AmigaRuntime<AmigaOcs>;

// ===================================================================
// AmigaEcs impl — A500+ and A600 today; A3000 follows once Ramsey /
// Fat Gary are ported. The trait body is mechanically
// identical to the AmigaOcs impl: the chip-level differences (BEAMCON0
// register handling, BPLCON3 register, programmable sync generator)
// are absorbed inside AgnusEcs / DeniseEcs via Deref/DerefMut, so the
// machine layer's call sites are unchanged. The two impls coexist so
// a future ECS-only behaviour can be carved out without touching OCS.
// ===================================================================

const ECS_VARIANT_QUERY_PATHS: &[&str] = amiga_variant_query_paths!();

impl AmigaMachine for AmigaEcs {
    const CHIPSET_FB_WIDTH: u32 = FB_WIDTH;
    const CHIPSET_FB_HEIGHT: u32 = FB_HEIGHT;

    type Snapshot = AmigaEcsSnapshot;

    fn build(firmware: &[u8], config: AmigaConfig) -> Self {
        crate::runtime::build_amiga_ecs(config, firmware)
    }

    fn tick(&mut self) {
        AmigaEcs::tick(self);
    }

    fn advance_to_cpu_boundary(&mut self) -> bool {
        AmigaEcs::advance_to_cpu_boundary(self)
    }

    fn drain_cpu_boundaries(&mut self) -> impl Iterator<Item = CpuTraceEntry> {
        AmigaEcs::drain_cpu_boundaries(self).map(|boundary| {
            (
                boundary.system_tick,
                boundary.instr_start_pc,
                boundary.sr,
                boundary.opcode,
            )
        })
    }

    fn frame_ticks(&self) -> u64 {
        match self.region() {
            AgnusRegion::Pal => crate::A500_PAL_FRAME_TICKS,
            AgnusRegion::Ntsc => crate::A500_NTSC_FRAME_TICKS,
        }
    }

    fn video_field_count(&self) -> u64 {
        self.denise()
            .completed_display_field_count(self.agnus().vbl_count)
    }

    fn cck_hz(&self) -> u64 {
        match self.region() {
            AgnusRegion::Pal => crate::A500_PAL_CCK_HZ,
            AgnusRegion::Ntsc => crate::A500_NTSC_CCK_HZ,
        }
    }

    fn chipset_framebuffer(&self) -> &[u32] {
        self.denise().framebuffer()
    }

    fn mix_audio_stereo(&self) -> (f32, f32) {
        self.paula().mix_audio_stereo()
    }

    fn led_filter_engaged(&self) -> bool {
        self.cia_a().power_led()
    }

    fn key_event(&mut self, code: u8, pressed: bool) {
        AmigaEcs::key_event(self, code, pressed);
    }

    fn move_mouse_port0(&mut self, dx: i32, dy: i32) {
        AmigaEcs::move_mouse_port0(self, dx, dy);
    }

    fn set_mouse_button_port0(&mut self, button: &str, pressed: bool) {
        AmigaEcs::set_mouse_button_port0(self, button, pressed);
    }

    fn set_joystick_control(&mut self, port: u8, name: &str, pressed: bool) {
        let _ = AmigaEcs::set_joystick_control(self, port, name, pressed);
    }

    fn insert_floppy0(&mut self, adf: Adf, change_pending: bool) {
        if change_pending {
            self.insert_adf_with_change_pending(adf);
        } else {
            self.insert_adf(adf);
        }
    }

    fn snapshot_state(&self) -> Self::Snapshot {
        AmigaEcs::snapshot_state(self)
    }

    fn restore_snapshot_state(&mut self, snapshot: Self::Snapshot) {
        AmigaEcs::restore_snapshot_state(self, snapshot);
    }

    fn validate_configuration(&self, config: AmigaConfig) -> Result<(), String> {
        validate_machine_configuration(
            config,
            MachineConfigurationState {
                region: self.region(),
                agnus_id: self.agnus().agnus_id,
                cpu_model: self.active_cpu().model(),
                cpu_variant_state_coherent: self.active_cpu().variant_state_is_coherent(),
                cpu_clock_numerator: self.cpu_clock().numerator(),
                cpu_clock_denominator: self.cpu_clock().denominator(),
                cpu_domain_phase_coherent: self.cpu_domain_phase_is_coherent(),
                chip_ram_bytes: self.memory().chip_ram_size(),
                slow_ram_bytes: self.memory().slow_ram_size(),
                fast_ram: self.autoconfig(),
                a530_config: None,
                a530_ram_bytes: None,
                a530_autoconfig_state: None,
                a530_configuration_coherent: true,
                synchronized_bridge_present: false,
                synchronized_bridge_coherent: true,
            },
        )
    }

    fn variant_query_paths() -> &'static [&'static str] {
        ECS_VARIANT_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<Value>, QueryError> {
        // Folded chip snapshots (#456): grouped `agnus` / `paula` / `cia`
        // / `blitter` / `chipset` / `denise` / `disk` objects +
        // per-field leaves, including the ECS programmable-timing state.
        if let Some(value) = resolve_chip_query(self, path) {
            return Ok(Some(value));
        }
        let value = match path {
            "memory.overlay" => json!(self.memory().overlay()),
            "cpu.pc" => json!(self.cpu().regs.pc),
            "cpu.sr" => json!(self.cpu().regs.sr),
            "cpu.ipl" => json!(self.cpu().ipl),
            "sprite0.dma_on" => json!(self.agnus().sprite_dma_on(0)),
            "sprite0.vstart" => json!(self.agnus().sprite_vstart(0)),
            "sprite0.vstop" => json!(self.agnus().sprite_vstop(0)),
            "sprite0.pixels_rendered" => {
                json!(self.denise().ocs.sprite_pixels_rendered(0))
            }
            "sprite0.ptr" => json!(self.agnus().spr_pt[0]),
            "debug.dsk_write_count" => json!(self.debug_dsk_log.len()),
            "debug.last_dsk_write" => {
                json!(self.debug_dsk_log.last().map(|(cck, pc, reg, val)| {
                    json!({"cck": cck, "pc": pc, "reg": reg, "val": val})
                }))
            }
            "display.color00" => json!(self.color(0)),
            "display.color01" => json!(self.color(1)),
            "keyboard.state" => json!(self.keyboard().debug_state_name()),
            "keyboard.queued" => json!(self.keyboard().queued_key_count()),
            "input.joy0dat" => json!(self.joy0dat()),
            "input.joy1dat" => json!(self.joy1dat()),
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}

/// Type alias for the ECS runtime — currently A500+ and A600. A3000
/// lands here once Ramsey and Fat Gary are ported. The chip stack is
/// AgnusEcs + DeniseEcs over the existing OCS Paula + CIA pair.
pub type AmigaEcsRuntime = AmigaRuntime<AmigaEcs>;

// ===================================================================
// AmigaA1200 impl — AGA chipset, 68EC020, A1200 / (future) CD32 / A4000.
//
// The chip stack uses AGA Alice (Agnus replacement) + AGA Lisa (Denise
// replacement, exposed as `Denise<DeniseAga>`) + Paula 8364 + the same
// two-CIA pair + the AGA-specific Gayle controller (IDE + control
// registers). For the trait impl, the surface is mechanically the same
// as OCS / ECS — only the snapshot type and query-path catalogue differ
// (A1000 paths drop, future Gayle / Akiko paths arrive in Phase 2).
// ===================================================================

/// AGA query paths. Drops the A1000-only paths (no A1200 bootstrap ROM
/// or WOM) and adds AGA-specific paths as Phase 2 chip-level tools
/// land. CPU / Agnus / Paula / disk / keyboard paths share the same
/// names as OCS / ECS so curriculum scripts targeting "cpu.pc"
/// work across the family.
const AGA_VARIANT_QUERY_PATHS: &[&str] = amiga_variant_query_paths!(
    "aga",
    "aga.deniseid",
    "aga.bplcon3",
    "aga.bplcon3_bank",
    "aga.bplcon3_loct",
    "aga.bplcon4",
    "aga.spr_width",
    "aga.ham_prev_rgb24",
    "aga.programmed_hblank_active",
    "aga.palette_24_nonzero_per_bank",
    "aga.palette_24_bank0",
    "aga.ocs_palette_12bit",
);

impl AmigaMachine for AmigaA1200 {
    const CHIPSET_FB_WIDTH: u32 = FB_WIDTH;
    const CHIPSET_FB_HEIGHT: u32 = FB_HEIGHT;

    type Snapshot = AmigaA1200Snapshot;

    fn build(firmware: &[u8], config: AmigaConfig) -> Self {
        crate::runtime::build_amiga_a1200(config, firmware)
    }

    fn tick(&mut self) {
        AmigaA1200::tick(self);
    }

    fn advance_to_cpu_boundary(&mut self) -> bool {
        AmigaA1200::advance_to_cpu_boundary(self)
    }

    fn drain_cpu_boundaries(&mut self) -> impl Iterator<Item = CpuTraceEntry> {
        AmigaA1200::drain_cpu_boundaries(self).map(|boundary| {
            (
                boundary.system_tick,
                boundary.instr_start_pc,
                boundary.sr,
                boundary.opcode,
            )
        })
    }

    fn frame_ticks(&self) -> u64 {
        match self.region() {
            AgnusRegion::Pal => crate::A500_PAL_FRAME_TICKS,
            AgnusRegion::Ntsc => crate::A500_NTSC_FRAME_TICKS,
        }
    }

    fn video_field_count(&self) -> u64 {
        self.denise()
            .completed_display_field_count(self.agnus().vbl_count)
    }

    fn cck_hz(&self) -> u64 {
        // AGA uses the same master clock as OCS / ECS (28.375160 MHz
        // PAL, 28.636360 MHz NTSC); the chip-RAM bus is double-pumped
        // for 32-bit fetches but the CCK rate at master/8 is unchanged.
        match self.region() {
            AgnusRegion::Pal => crate::A500_PAL_CCK_HZ,
            AgnusRegion::Ntsc => crate::A500_NTSC_CCK_HZ,
        }
    }

    fn chipset_framebuffer(&self) -> &[u32] {
        self.denise().framebuffer()
    }

    fn mix_audio_stereo(&self) -> (f32, f32) {
        self.paula().mix_audio_stereo()
    }

    fn led_filter_engaged(&self) -> bool {
        self.cia_a().power_led()
    }

    fn key_event(&mut self, code: u8, pressed: bool) {
        AmigaA1200::key_event(self, code, pressed);
    }

    fn move_mouse_port0(&mut self, dx: i32, dy: i32) {
        AmigaA1200::move_mouse_port0(self, dx, dy);
    }

    fn set_mouse_button_port0(&mut self, button: &str, pressed: bool) {
        AmigaA1200::set_mouse_button_port0(self, button, pressed);
    }

    fn set_joystick_control(&mut self, port: u8, name: &str, pressed: bool) {
        let _ = AmigaA1200::set_joystick_control(self, port, name, pressed);
    }

    fn insert_floppy0(&mut self, adf: Adf, change_pending: bool) {
        if change_pending {
            self.insert_adf_with_change_pending(adf);
        } else {
            self.insert_adf(adf);
        }
    }

    fn snapshot_state(&self) -> Self::Snapshot {
        AmigaA1200::snapshot_state(self)
    }

    fn restore_snapshot_state(&mut self, snapshot: Self::Snapshot) {
        AmigaA1200::restore_snapshot_state(self, snapshot);
    }

    fn validate_configuration(&self, config: AmigaConfig) -> Result<(), String> {
        validate_machine_configuration(
            config,
            MachineConfigurationState {
                region: self.region(),
                agnus_id: self.agnus().agnus_id,
                cpu_model: self.active_cpu().model(),
                cpu_variant_state_coherent: self.active_cpu().variant_state_is_coherent(),
                cpu_clock_numerator: self.cpu_clock().numerator(),
                cpu_clock_denominator: self.cpu_clock().denominator(),
                cpu_domain_phase_coherent: self.cpu_domain_phase_is_coherent(),
                chip_ram_bytes: self.memory().chip_ram_size(),
                slow_ram_bytes: self.memory().slow_ram_size(),
                fast_ram: self.autoconfig(),
                a530_config: None,
                a530_ram_bytes: None,
                a530_autoconfig_state: None,
                a530_configuration_coherent: true,
                synchronized_bridge_present: false,
                synchronized_bridge_coherent: true,
            },
        )
    }

    fn variant_query_paths() -> &'static [&'static str] {
        AGA_VARIANT_QUERY_PATHS
    }

    fn resolve_variant_query(&self, path: &str) -> Result<Option<Value>, QueryError> {
        // Folded chip snapshots (#456): the shared `agnus` / `paula` /
        // `cia` / `blitter` / `chipset` / `disk` groups plus the
        // AGA-only `aga` group (Lisa registers + 24-bit palette).
        if let Some(value) = resolve_chip_query(self, path) {
            return Ok(Some(value));
        }
        if is_chip(path, "aga") {
            return Ok(aga_snapshot(self).and_then(|snap| chip_field(path, "aga", snap)));
        }
        let value = match path {
            "memory.overlay" => json!(self.memory().overlay()),
            "cpu.pc" => json!(self.cpu().regs.pc),
            "cpu.sr" => json!(self.cpu().regs.sr),
            "cpu.ipl" => json!(self.cpu().ipl),
            "sprite0.dma_on" => json!(self.agnus().sprite_dma_on(0)),
            "sprite0.vstart" => json!(self.agnus().sprite_vstart(0)),
            "sprite0.vstop" => json!(self.agnus().sprite_vstop(0)),
            "sprite0.pixels_rendered" => {
                json!(self.denise().ocs.sprite_pixels_rendered(0))
            }
            "sprite0.ptr" => json!(self.agnus().spr_pt[0]),
            "debug.dsk_write_count" => json!(self.debug_dsk_log.len()),
            "debug.last_dsk_write" => {
                json!(self.debug_dsk_log.last().map(|(cck, pc, reg, val)| {
                    json!({"cck": cck, "pc": pc, "reg": reg, "val": val})
                }))
            }
            "display.color00" => json!(self.color(0)),
            "display.color01" => json!(self.color(1)),
            "keyboard.state" => json!(self.keyboard().debug_state_name()),
            "keyboard.queued" => json!(self.keyboard().queued_key_count()),
            "input.joy0dat" => json!(self.joy0dat()),
            "input.joy1dat" => json!(self.joy1dat()),
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}

/// Type alias for the AGA runtime — covers A1200 today, with A4000 /
/// CD32 to land here once their machine-specific chips (Fat Gary +
/// Ramsey for A4000, Akiko for CD32) are ported.
pub type AmigaA1200Runtime = AmigaRuntime<AmigaA1200>;

// ===================================================================
// AmigaRuntimeKind — runtime-time dispatch over OCS / ECS / AGA.
//
// Verifier binaries (emu198x-amiga) take a
// `--model` argument that may pick either an OCS or an ECS variant.
// Storing a concrete `AmigaOcsRuntime` field in the binary forces
// every model through OCS chips even when the Model is ECS-flavoured
// (e.g. `A500PlusEcsPal`). `AmigaRuntimeKind` is the dispatcher: it
// wraps either runtime type and forwards the `MachineCore` surface
// to the inner case based on `Model::is_ecs()`.
// ===================================================================

/// Runtime-time dispatch over the available Amiga machine kinds.
/// Constructed via `AmigaRuntimeKind::new(model, firmware)` (or
/// `from_firmware` / `blank`); the inner case is picked by
/// `Model::is_ecs()`. Implements `MachineCore` so callers can drive
/// it like any other runtime.
// One instance per session, held for its lifetime; boxing the larger
// variant would only add heap indirection to the hot per-tick
// `MachineCore` forwarding path.
//
// The arms are chip-stack TIERS, not machines: new Amiga models (A600,
// A4000, CD32, NTSC ...) route onto an existing arm via `Model`, so they
// add a `Model` variant + a branch in `new`/`from_firmware`/`blank`, NOT
// a match arm. The forwarders below are therefore written as explicit
// 3-arm matches, which read fine inline. The Spectrum enum has 13 arms
// (per-variant, no tier to collapse onto) and uses a `match_kind!`
// forwarding macro instead — see
// `knowledge/decisions/runtime-internal-shape.md`. If the Amiga ever
// grows past ~5 chip-stack tiers (e.g. a SAGA / Apollo arm lands and the
// explicit matches start to sting), adopt the same macro here — it's a
// mechanical, internal-only change with no API churn.
#[allow(clippy::large_enum_variant)]
pub enum AmigaRuntimeKind {
    /// OCS-shaped stack — A1000, A500 family and A2000B. The A2000B
    /// combines Fat Agnus 8372A with OCS Denise.
    Ocs(AmigaOcsRuntime),
    /// ECS chip stack — A500+ and A600 today; A3000 follows once its
    /// machine-specific chips are ported.
    Ecs(AmigaEcsRuntime),
    /// AGA chip stack — A1200 today (PAL/NTSC); A4000 / CD32 land
    /// here once their machine-specific chips (Fat Gary + Ramsey for
    /// A4000, Akiko for CD32) are ported.
    Aga(AmigaA1200Runtime),
}

impl AmigaRuntimeKind {
    /// Construct using the model's preset RAM layout. Picks OCS or
    /// ECS based on `Model::is_ecs()`.
    ///
    /// # Errors
    /// Returns the underlying `MachineError` from the dispatched
    /// runtime constructor.
    pub fn new(model: Model, firmware_rom: Vec<u8>) -> Result<Self, emu198x_shell::MachineError> {
        if model.is_aga() {
            AmigaA1200Runtime::new(model, firmware_rom).map(Self::Aga)
        } else if model.is_ecs() {
            AmigaEcsRuntime::new(model, firmware_rom).map(Self::Ecs)
        } else {
            AmigaOcsRuntime::new(model, firmware_rom).map(Self::Ocs)
        }
    }

    /// Construct with a zero-filled placeholder firmware. Useful for
    /// tests and verifier dry-runs.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        if model.is_aga() {
            Self::Aga(AmigaA1200Runtime::blank(model))
        } else if model.is_ecs() {
            Self::Ecs(AmigaEcsRuntime::blank(model))
        } else {
            Self::Ocs(AmigaOcsRuntime::blank(model))
        }
    }

    /// Active model — same on each inner case.
    #[must_use]
    pub fn model(&self) -> Model {
        match self {
            Self::Ocs(rt) => rt.model(),
            Self::Ecs(rt) => rt.model(),
            Self::Aga(rt) => rt.model(),
        }
    }

    /// Read-back: was this runtime constructed against the ECS chip
    /// stack? Equivalent to `self.model().is_ecs()` but reads the
    /// dispatched-case directly.
    #[must_use]
    pub fn is_ecs(&self) -> bool {
        matches!(self, Self::Ecs(_))
    }

    /// Read-back: was this runtime constructed against the AGA chip
    /// stack? Equivalent to `self.model().is_aga()` but reads the
    /// dispatched-case directly.
    #[must_use]
    pub fn is_aga(&self) -> bool {
        matches!(self, Self::Aga(_))
    }

    /// Advance exactly to the next active-CPU instruction boundary.
    ///
    /// Returns completed Amiga system ticks. A higher-clocked processor can
    /// reach a boundary part-way through a tick, in which case zero complete
    /// ticks may be reported while the remaining CPU edges are retained. If
    /// `tick_limit` is reached first, returns the complete ticks consumed
    /// without implying that a boundary was crossed; compare
    /// [`AmigaLiveAccess::cpu_instruction_starts`] before and after when the
    /// distinction matters.
    pub(crate) fn step_cpu_instruction(&mut self, tick_limit: u64) -> u64 {
        let start_tick = AmigaLiveAccess::tick_count(self);
        let start_instruction = AmigaLiveAccess::cpu_instruction_starts(self);
        let mut completed_ticks = 0;

        while AmigaLiveAccess::cpu_instruction_starts(self) == start_instruction
            && completed_ticks < tick_limit
        {
            let crossed_boundary = match self {
                Self::Ocs(runtime) => runtime.advance_to_cpu_boundary_traced(),
                Self::Ecs(runtime) => runtime.advance_to_cpu_boundary_traced(),
                Self::Aga(runtime) => runtime.advance_to_cpu_boundary_traced(),
            };
            completed_ticks = AmigaLiveAccess::tick_count(self).wrapping_sub(start_tick);
            if crossed_boundary {
                break;
            }
        }

        completed_ticks
    }
}

impl emu198x_shell::FamilyRuntime for AmigaRuntimeKind {
    type Model = Model;

    /// Construct the dispatched variant from the profile's firmware set,
    /// picking OCS / ECS / AGA by `Model`. This is the shell-level
    /// constructor `HeadlessSession::swap_machine` drives, so a future
    /// Amiga `set_machine` tool gets variant swapping for free (#456).
    ///
    /// # Errors
    /// Returns the underlying `MachineError` from the dispatched runtime
    /// constructor.
    fn from_firmware(
        model: Self::Model,
        firmware: &emu198x_shell::FirmwareSet<'_>,
    ) -> Result<Self, emu198x_shell::MachineError> {
        if model.is_aga() {
            AmigaA1200Runtime::from_firmware(model, firmware).map(Self::Aga)
        } else if model.is_ecs() {
            AmigaEcsRuntime::from_firmware(model, firmware).map(Self::Ecs)
        } else {
            AmigaOcsRuntime::from_firmware(model, firmware).map(Self::Ocs)
        }
    }

    /// Native frame length in machine ticks, read from the active
    /// variant's chipset (`AmigaMachine::frame_ticks`) so a swap re-paces
    /// the session to the new variant's video timing rather than assuming
    /// the PAL-OCS constant.
    fn native_frame_ticks(&self) -> u64 {
        match self {
            Self::Ocs(rt) => rt.machine().frame_ticks(),
            Self::Ecs(rt) => rt.machine().frame_ticks(),
            Self::Aga(rt) => rt.machine().frame_ticks(),
        }
    }
}

impl emu198x_shell::MachineCore for AmigaRuntimeKind {
    fn profile(&self) -> &emu198x_shell::MachineProfile {
        match self {
            Self::Ocs(rt) => rt.profile(),
            Self::Ecs(rt) => rt.profile(),
            Self::Aga(rt) => rt.profile(),
        }
    }

    fn time(&self) -> emu198x_shell::MachineTime {
        match self {
            Self::Ocs(rt) => rt.time(),
            Self::Ecs(rt) => rt.time(),
            Self::Aga(rt) => rt.time(),
        }
    }

    fn reset(&mut self, kind: emu198x_shell::ResetKind) {
        match self {
            Self::Ocs(rt) => rt.reset(kind),
            Self::Ecs(rt) => rt.reset(kind),
            Self::Aga(rt) => rt.reset(kind),
        }
    }

    fn load_media(
        &mut self,
        media: &emu198x_shell::MediaSet<'_>,
    ) -> Result<(), emu198x_shell::MachineError> {
        match self {
            Self::Ocs(rt) => rt.load_media(media),
            Self::Ecs(rt) => rt.load_media(media),
            Self::Aga(rt) => rt.load_media(media),
        }
    }

    fn eject_media(&mut self, slot: &str) -> Result<(), emu198x_shell::MachineError> {
        match self {
            Self::Ocs(rt) => rt.eject_media(slot),
            Self::Ecs(rt) => rt.eject_media(slot),
            Self::Aga(rt) => rt.eject_media(slot),
        }
    }

    fn run_until(
        &mut self,
        target: emu198x_shell::MachineTime,
        host: &mut emu198x_shell::HostIo<'_>,
    ) -> Result<emu198x_shell::RunResult, emu198x_shell::MachineError> {
        match self {
            Self::Ocs(rt) => rt.run_until(target, host),
            Self::Ecs(rt) => rt.run_until(target, host),
            Self::Aga(rt) => rt.run_until(target, host),
        }
    }

    fn snapshot(&self) -> Result<Vec<u8>, emu198x_shell::MachineError> {
        match self {
            Self::Ocs(rt) => rt.snapshot(),
            Self::Ecs(rt) => rt.snapshot(),
            Self::Aga(rt) => rt.snapshot(),
        }
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), emu198x_shell::MachineError> {
        match self {
            Self::Ocs(rt) => rt.restore(bytes),
            Self::Ecs(rt) => rt.restore(bytes),
            Self::Aga(rt) => rt.restore(bytes),
        }
    }

    fn command(
        &mut self,
        command: &emu198x_shell::ControlCommand,
    ) -> Result<(), emu198x_shell::MachineError> {
        match self {
            Self::Ocs(rt) => rt.command(command),
            Self::Ecs(rt) => rt.command(command),
            Self::Aga(rt) => rt.command(command),
        }
    }

    fn capabilities(&self) -> emu198x_shell::CapabilitySet {
        match self {
            Self::Ocs(rt) => rt.capabilities(),
            Self::Ecs(rt) => rt.capabilities(),
            Self::Aga(rt) => rt.capabilities(),
        }
    }

    // The Amiga joins the shared debug tier via `impl DebugPrimitives` (see
    // `debug.rs`), which the shell's blanket impl turns into `DebugTarget`.
    // Always present: the family enum is always backed by a live machine.
    fn debug_target(&self) -> Option<&dyn emu198x_shell::DebugTarget> {
        Some(self)
    }
    fn debug_target_mut(&mut self) -> Option<&mut dyn emu198x_shell::DebugTarget> {
        Some(self)
    }

    // The Amiga joins the shared watch tier via `impl WatchTarget` below; it
    // exposes the memory-write surface only (Paula, not an AY-3-8912).
    fn watch_target(&self) -> Option<&dyn emu198x_shell::WatchTarget> {
        Some(self)
    }
    fn watch_target_mut(&mut self) -> Option<&mut dyn emu198x_shell::WatchTarget> {
        Some(self)
    }

    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        Some(self)
    }
}

/// Keyboard description for the shared `press_key` / `press_keys` / `type_string`
/// tools. The Amiga has a real Shift-aware `keys_for_char` and the full HRM
/// rawkey vocabulary (including the two Amiga keys for the Ctrl-Amiga-Amiga
/// reset), so it carries a bespoke impl rather than the ASCII default.
impl emu198x_shell::KeyboardTarget for AmigaRuntimeKind {
    fn key_name_is_valid(&self, name: &str) -> bool {
        crate::input::key_name_is_valid(name)
    }

    fn key_names_hint(&self) -> &'static str {
        "A-Z, 0-9, F1-F10, Space, Return, Esc, Tab, Backspace, Delete, Help, \
         cursor Up/Down/Left/Right, LShift/RShift, Ctrl, LAlt/RAlt, Caps, \
         LAmiga/RAmiga (or raw-NN)"
    }

    fn keys_for_char(&self, ch: char) -> Option<Vec<String>> {
        crate::input::keys_for_char(ch).map(|keys| keys.into_iter().map(str::to_owned).collect())
    }

    fn key_timing(&self) -> emu198x_shell::KeyTiming {
        emu198x_shell::KeyTiming {
            default_hold_frames: crate::DEFAULT_KEY_HOLD_FRAMES,
            max_hold_frames: crate::MAX_KEY_HOLD_FRAMES,
            // press_key settles 1 frame after release; type_string runs 2 between
            // characters with no extra repeat settle (the 2-frame gap separates
            // identical keys).
            press_settle_frames: 1,
            inter_key_settle_frames: 2,
            repeat_settle_frames: 0,
            default_type_settle_frames: crate::DEFAULT_TYPE_SETTLE_FRAMES,
        }
    }
}

/// Memory-write watch over the shared [`emu198x_shell::WatchTarget`]. The
/// Amiga stamps each write with its colour-clock count and byte/word width
/// (preserved through `WatchMemoryRecord::cck` / `size_bytes`), and grows the
/// capture buffer without limit (so `start` reports capacity `0` = unbounded).
/// No AY surface — the Amiga's sound chip is Paula.
impl emu198x_shell::WatchTarget for AmigaRuntimeKind {
    fn supports_memory_watch(&self) -> bool {
        true
    }

    fn start_memory_watch(
        &mut self,
        addr: u32,
        len: u32,
    ) -> Result<u32, emu198x_shell::WatchError> {
        AmigaLiveAccess::set_watch(self, Some((addr, len)));
        Ok(0)
    }

    fn clear_memory_watch(&mut self) -> (bool, u32) {
        let had_watch = AmigaLiveAccess::watch_range(self).is_some();
        let captured = AmigaLiveAccess::watch_log(self).len() as u32;
        AmigaLiveAccess::set_watch(self, None);
        (had_watch, captured)
    }

    fn memory_watch_range(&self) -> Option<(u32, u32)> {
        AmigaLiveAccess::watch_range(self)
    }

    fn memory_watch_records(&self) -> Option<Vec<emu198x_shell::WatchMemoryRecord>> {
        // `Some` only while armed, matching the shared shape (a `None` range
        // reports as "no active watch" rather than an empty log).
        AmigaLiveAccess::watch_range(self)?;
        Some(
            AmigaLiveAccess::watch_log(self)
                .iter()
                .map(
                    |&(cck, pc, addr, val, is_word)| emu198x_shell::WatchMemoryRecord {
                        pc,
                        addr,
                        value: u32::from(val),
                        cck: Some(cck),
                        size_bytes: if is_word { 2 } else { 1 },
                    },
                )
                .collect(),
        )
    }
}

// Audio-control surface. AudioControls and PaulaChannel are the same
// types in both machine crates (re-exported from commodore-paula-8364),
// so the wrapper just dispatches.
impl AmigaRuntimeKind {
    #[must_use]
    pub fn audio_controls(&self) -> machine_commodore_amiga_ocs::AudioControls {
        match self {
            Self::Ocs(rt) => rt.audio_controls(),
            Self::Ecs(rt) => rt.audio_controls(),
            Self::Aga(rt) => rt.audio_controls(),
        }
    }

    pub fn set_audio_controls(&mut self, controls: machine_commodore_amiga_ocs::AudioControls) {
        match self {
            Self::Ocs(rt) => rt.set_audio_controls(controls),
            Self::Ecs(rt) => rt.set_audio_controls(controls),
            Self::Aga(rt) => rt.set_audio_controls(controls),
        }
    }

    pub fn set_audio_channel_enabled(
        &mut self,
        channel: machine_commodore_amiga_ocs::PaulaChannel,
        enabled: bool,
    ) {
        match self {
            Self::Ocs(rt) => rt.set_audio_channel_enabled(channel, enabled),
            Self::Ecs(rt) => rt.set_audio_channel_enabled(channel, enabled),
            Self::Aga(rt) => rt.set_audio_channel_enabled(channel, enabled),
        }
    }

    pub fn set_audio_channel_gain(
        &mut self,
        channel: machine_commodore_amiga_ocs::PaulaChannel,
        gain: f32,
    ) {
        match self {
            Self::Ocs(rt) => rt.set_audio_channel_gain(channel, gain),
            Self::Ecs(rt) => rt.set_audio_channel_gain(channel, gain),
            Self::Aga(rt) => rt.set_audio_channel_gain(channel, gain),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AmigaOcsRuntime, Model};
    use emu198x_shell::{MachineCore, ResetKind};

    /// Spec invariant: every advertised variant query path is unique.
    /// Doubles would silently clobber each other in a sorted listing.
    #[test]
    fn ocs_variant_query_paths_are_unique() {
        let mut sorted: Vec<&&str> = OCS_VARIANT_QUERY_PATHS.iter().collect();
        sorted.sort();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(sorted.len(), deduped.len(), "duplicate variant query paths");
    }

    #[test]
    fn ecs_variant_query_paths_are_unique() {
        let mut sorted: Vec<&&str> = ECS_VARIANT_QUERY_PATHS.iter().collect();
        sorted.sort();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(
            sorted.len(),
            deduped.len(),
            "duplicate ECS variant query paths"
        );
    }

    #[test]
    fn autoconfig_chain_rejects_downstream_progress_while_a530_is_visible() {
        let mut downstream = AutoconfigBoard::fast_ram(1024);
        downstream.write_word(0x4A, 0x0000);

        let error = validate_autoconfig_chain(
            Some(AutoconfigState::Unconfigured),
            Some(1024 * 1024),
            Some(&downstream),
        )
        .expect_err("the downstream board cannot be probed before the A530");
        assert!(error.contains("advanced before"));
    }

    #[test]
    fn autoconfig_chain_rejects_overlapping_configured_ranges() {
        let mut downstream = AutoconfigBoard::fast_ram(1024);
        downstream.write_word(0x4A, 0x0000);
        downstream.write_word(0x48, 0x2000);

        let error = validate_autoconfig_chain(
            Some(AutoconfigState::Configured { base: 0x0020_0000 }),
            Some(1024 * 1024),
            Some(&downstream),
        )
        .expect_err("two boards cannot own the same mapped range");
        assert!(error.contains("overlap"));
    }

    #[test]
    fn autoconfig_chain_accepts_ordered_non_overlapping_ranges() {
        let mut downstream = AutoconfigBoard::fast_ram(1024);
        downstream.write_word(0x4A, 0x0000);
        downstream.write_word(0x48, 0x4000);

        validate_autoconfig_chain(
            Some(AutoconfigState::Configured { base: 0x0020_0000 }),
            Some(1024 * 1024),
            Some(&downstream),
        )
        .expect("the downstream board may configure after the A530");
    }

    #[test]
    fn model_builders_install_the_canonical_agnus_identity() {
        for model in [
            Model::A1000OcsPal,
            Model::A1000OcsNtsc,
            Model::A500OcsPal,
            Model::A500OcsNtsc,
            Model::A500OcsPalA501,
            Model::A500OcsNtscA501,
            Model::A2000OcsPal,
            Model::A2000OcsNtsc,
            Model::A500OcsPalMaxed,
            Model::A500OcsNtscMaxed,
            Model::A500PlusEcsPal,
            Model::A500PlusEcsNtsc,
            Model::A600EcsPal,
            Model::A600EcsNtsc,
            Model::A1200AgaPal,
            Model::A1200AgaNtsc,
            Model::A500OcsPalGvpA530,
            Model::A500OcsNtscGvpA530,
        ] {
            let mut runtime = AmigaRuntimeKind::blank(model);
            assert_eq!(
                AmigaLiveAccess::agnus(&runtime).agnus_id,
                expected_agnus_id(model),
                "{model:?} builder installed the wrong VPOSR identity"
            );
            runtime.reset(ResetKind::Hard);
            assert_eq!(
                AmigaLiveAccess::agnus(&runtime).agnus_id,
                expected_agnus_id(model),
                "{model:?} hard reset installed the wrong VPOSR identity"
            );
        }
    }

    #[test]
    fn configuration_validation_rejects_original_agnus_region_id_mismatch() {
        let runtime = AmigaOcsRuntime::blank(Model::A500OcsPal);
        let machine = runtime.machine();
        let error = validate_machine_configuration(
            runtime.config(),
            MachineConfigurationState {
                region: machine.region(),
                agnus_id: 0x1000,
                cpu_model: machine.active_cpu().model(),
                cpu_variant_state_coherent: machine.active_cpu().variant_state_is_coherent(),
                cpu_clock_numerator: machine.cpu_clock().numerator(),
                cpu_clock_denominator: machine.cpu_clock().denominator(),
                cpu_domain_phase_coherent: machine.cpu_domain_phase_is_coherent(),
                chip_ram_bytes: machine.memory().chip_ram_size(),
                slow_ram_bytes: machine.memory().slow_ram_size(),
                fast_ram: None,
                a530_config: None,
                a530_ram_bytes: None,
                a530_autoconfig_state: None,
                a530_configuration_coherent: true,
                synchronized_bridge_present: false,
                synchronized_bridge_coherent: true,
            },
        )
        .expect_err("PAL timing with the original NTSC identity must be rejected");

        assert!(error.contains("Agnus identity mismatch"));
    }

    #[test]
    fn aga_variant_query_paths_are_unique() {
        let mut sorted: Vec<&&str> = AGA_VARIANT_QUERY_PATHS.iter().collect();
        sorted.sort();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(
            sorted.len(),
            deduped.len(),
            "duplicate AGA variant query paths"
        );
    }

    #[test]
    fn blank_dispatches_a1200_to_aga_variant() {
        let kind = AmigaRuntimeKind::blank(Model::A1200AgaPal);
        assert!(kind.is_aga(), "A1200 should land in the Aga arm");
        assert!(!kind.is_ecs());
        assert_eq!(kind.model(), Model::A1200AgaPal);
    }

    #[test]
    fn blank_dispatches_a500_to_ocs_variant() {
        let kind = AmigaRuntimeKind::blank(Model::A500OcsPal);
        assert!(!kind.is_aga());
        assert!(!kind.is_ecs());
        assert_eq!(kind.model(), Model::A500OcsPal);
    }

    #[test]
    fn blank_dispatches_a500plus_to_ecs_variant() {
        let kind = AmigaRuntimeKind::blank(Model::A500PlusEcsPal);
        assert!(kind.is_ecs());
        assert!(!kind.is_aga());
        assert_eq!(kind.model(), Model::A500PlusEcsPal);
    }

    #[test]
    fn blank_dispatches_a600_to_ecs_variant() {
        let kind = AmigaRuntimeKind::blank(Model::A600EcsPal);
        assert!(kind.is_ecs(), "A600 should land in the Ecs arm");
        assert!(!kind.is_aga());
        assert_eq!(kind.model(), Model::A600EcsPal);
    }

    #[test]
    fn blank_dispatches_a2000_to_ocs_variant() {
        let kind = AmigaRuntimeKind::blank(Model::A2000OcsPal);
        assert!(!kind.is_aga());
        assert!(!kind.is_ecs(), "A2000 should land in the Ocs arm");
        assert_eq!(kind.model(), Model::A2000OcsPal);
        let AmigaRuntimeKind::Ocs(mut runtime) = kind else {
            panic!("A2000 should use the OCS-shaped machine");
        };
        assert_eq!(
            runtime.machine().agnus().agnus_id,
            0x2000,
            "the OCS-shaped A2000 still carries Fat Agnus 8372A"
        );
        assert!(runtime.machine().uses_fat_agnus_8372a());
        runtime.reset(ResetKind::Hard);
        assert_eq!(
            runtime.machine().agnus().agnus_id,
            0x2000,
            "hard reset must preserve the model's explicit Agnus revision"
        );
        assert!(
            runtime.machine().uses_fat_agnus_8372a(),
            "hard reset must preserve the concrete Fat Agnus extension layer"
        );
    }
}
