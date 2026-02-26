//! The "Rock" - A Cycle-Strict Amiga Emulator.
//!
//! Foundation: Crystal-accuracy.
//! Bus Model: Reactive (Request/Acknowledge), not Predictive.
//! CPU Model: Ticks every 4 crystal cycles, polls bus until DTACK.

pub mod bus;
pub mod config;
pub mod memory;

use crate::memory::Memory;
use commodore_agnus_ecs::AgnusEcs as Agnus;
use commodore_agnus_ocs::{BlitterDmaOp, Copper, SlotOwner};
use commodore_denise_ecs::DeniseEcs as DeniseOcs;
use commodore_paula_8364::Paula8364;
use drive_amiga_floppy::AmigaFloppyDrive;
use format_adf::Adf;
use mos_cia_8520::Cia8520;
use motorola_68000::cpu::Cpu68000;
use peripheral_amiga_keyboard::AmigaKeyboard;
use std::sync::OnceLock;

// Re-export chip crates so tests and downstream users can access types.
pub use crate::config::{AmigaChipset, AmigaConfig, AmigaModel};
pub use commodore_agnus_ecs;
pub use commodore_agnus_ocs;
pub use commodore_denise_ecs;
pub use commodore_denise_ocs;
pub use commodore_paula_8364;
pub use drive_amiga_floppy;
pub use format_adf;
pub use mos_cia_8520;
use motorola_68000::bus::{BusStatus, FunctionCode, M68kBus};
pub use peripheral_amiga_keyboard;

/// Standard Amiga PAL Master Crystal Frequency (Hz)
pub const PAL_CRYSTAL_HZ: u64 = 28_375_160;
/// Standard Amiga NTSC Master Crystal Frequency (Hz)
pub const NTSC_CRYSTAL_HZ: u64 = 28_636_360;

/// Number of crystal ticks per Colour Clock (CCK)
pub const TICKS_PER_CCK: u64 = 8;
/// Number of crystal ticks per CPU Cycle
pub const TICKS_PER_CPU: u64 = 4;
/// Number of crystal ticks per CIA E-clock
pub const TICKS_PER_ECLOCK: u64 = 40;
/// Number of crystal ticks per PAL frame (A500/OCS timing).
pub const PAL_FRAME_TICKS: u64 = (commodore_agnus_ocs::PAL_CCKS_PER_LINE as u64)
    * (commodore_agnus_ocs::PAL_LINES_PER_FRAME as u64)
    * TICKS_PER_CCK;
/// Paula audio sample rate exposed to host runners.
pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
const PAL_CCK_HZ: u64 = PAL_CRYSTAL_HZ / TICKS_PER_CCK;

/// Vertical start of visible display (PAL line $2C = 44).
const DISPLAY_VSTART: u16 = 0x2C;

#[derive(Debug, Clone)]
struct DiskDmaRuntime {
    data: Vec<u8>,
    byte_index: usize,
    words_remaining: u32,
    is_write: bool,
    wordsync_enabled: bool,
    wordsync_waiting: bool,
}

/// Coarse ECS sync-window state in the emulator's current beam units.
///
/// This is intended for debug/test visibility while fuller ECS sync generation
/// behavior is still being introduced.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BeamSyncState {
    pub hsync: bool,
    pub vsync: bool,
}

/// Latched beam-edge class changes for the current CCK.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BeamEdgeFlags {
    pub hsync_changed: bool,
    pub vsync_changed: bool,
    pub hblank_changed: bool,
    pub vblank_changed: bool,
    pub visible_changed: bool,
}

impl BeamEdgeFlags {
    #[must_use]
    pub const fn any(self) -> bool {
        self.hsync_changed
            || self.vsync_changed
            || self.hblank_changed
            || self.vblank_changed
            || self.visible_changed
    }
}

/// Coarse ECS beam output pin state derived from the latched sync/blank model.
///
/// This is debug/test-facing and intentionally approximate while fuller ECS
/// signal-generator behavior is still being implemented.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BeamPinState {
    /// Horizontal sync pin level (`true` = high) after `HSYTRUE` polarity.
    pub hsync_high: bool,
    /// Vertical sync pin level (`true` = high) after `VSYTRUE` polarity.
    pub vsync_high: bool,
    /// Composite sync pin level (`true` = high) after `CSYTRUE` polarity.
    pub csync_high: bool,
    /// Coarse composite blank activity (`BLANKEN` gated).
    pub blank_active: bool,
}

/// Coarse composite-sync mode/routing state for ECS debug visibility.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BeamCompositeSyncDebug {
    /// Composite-sync activity before `CSYTRUE` polarity is applied.
    pub active: bool,
    /// `BEAMCON0.CSCBEN` (composite sync redirection) latch state.
    pub redirected: bool,
    /// Coarse composite-sync source mode.
    pub mode: BeamCompositeSyncMode,
}

/// Coarse composite-sync source mode in the current ECS bring-up model.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum BeamCompositeSyncMode {
    /// Hardwired composite sync derived from H/V sync activity.
    #[default]
    HardwiredHvOr,
    /// ECS variable composite sync enabled, but still using H/V OR as a
    /// conservative placeholder until full CS timing is modeled.
    VariablePlaceholderHvOr,
}

/// Debug/test-facing beam snapshot in the emulator's current beam units.
///
/// This is intended as a stable machine-level inspection API while ECS sync
/// and blanking behavior is still being brought up incrementally.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BeamDebugSnapshot {
    pub vpos: u16,
    pub hpos_cck: u16,
    pub sync: BeamSyncState,
    pub composite_sync: BeamCompositeSyncDebug,
    pub hblank: bool,
    pub vblank: bool,
    pub pins: BeamPinState,
    pub fb_coords: Option<(u32, u32)>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BeamPixelOutputDebug {
    pub vpos: u16,
    pub hpos_cck: u16,
    pub bpl1dat_trigger_cck: u16,
    pub serial_window_start_cck: u16,
    pub serial_window_len_cck: u16,
    pub serial_window_active: bool,
    pub diw_hstart_beam_x: Option<u16>,
    pub diw_hstop_beam_x: Option<u16>,
    pub pixel0: commodore_denise_ocs::DeniseOutputPixelDebug,
    pub pixel1: commodore_denise_ocs::DeniseOutputPixelDebug,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BlitterProgressDebugStats {
    pub busy_ccks: u64,
    pub granted_ops: u64,
    pub cpu_slot_grant_ccks: u64,
    pub copper_idle_grant_ccks: u64,
    pub copper_slot_idle_ccks: u64,
    pub copper_slot_busy_ccks: u64,
    pub bitplane_slot_ccks: u64,
    pub refresh_slot_ccks: u64,
    pub sprite_slot_ccks: u64,
    pub disk_slot_ccks: u64,
    pub audio_slot_ccks: u64,
    pub max_queue_len_seen: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlitShadowCompareMode {
    Area,
    Line,
}

#[derive(Clone)]
struct BlitShadowCompareSnapshot {
    mode: BlitShadowCompareMode,
    seq: usize,
    captured_master_clock: u64,
    captured_vpos: u16,
    captured_hpos: u16,
    agnus: Agnus,
    memory: Memory,
}

pub struct Amiga {
    pub master_clock: u64,
    pub model: AmigaModel,
    pub chipset: AmigaChipset,
    pub cpu: Cpu68000,
    pub agnus: Agnus,
    pub memory: Memory,
    pub denise: DeniseOcs,
    pub copper: Copper,
    pub cia_a: Cia8520,
    pub cia_b: Cia8520,
    pub paula: Paula8364,
    pub floppy: AmigaFloppyDrive,
    pub keyboard: AmigaKeyboard,
    audio_sample_phase: u64,
    audio_buffer: Vec<f32>,
    disk_dma_runtime: Option<DiskDmaRuntime>,
    sprite_dma_phase: [u8; 8],
    beam_debug_snapshot: BeamDebugSnapshot,
    beam_edge_flags: BeamEdgeFlags,
    beam_pixel_outputs_debug: BeamPixelOutputDebug,
    blitter_progress_debug: BlitterProgressDebugStats,
    blitter_shadow_compare_snapshot: Option<BlitShadowCompareSnapshot>,
    blitter_area_shadow_compare_started: usize,
    blitter_line_shadow_compare_started: usize,
    /// Pending BPLCON0 write to Denise (value, CCK countdown).
    /// Agnus sees the new value immediately; Denise sees it after 2 CCK.
    pub bplcon0_denise_pending: Option<(u16, u8)>,
    /// Pending DDFSTRT write (value, CCK countdown).
    pub ddfstrt_pending: Option<(u16, u8)>,
    /// Pending DDFSTOP write (value, CCK countdown).
    pub ddfstop_pending: Option<(u16, u8)>,
    /// Pending color register writes (palette index, value, CCK countdown).
    pub color_pending: Vec<(usize, u16, u8)>,
    /// Vertical bitplane DMA enable flip-flop, latched at line start (hpos=0).
    ///
    /// On real hardware Agnus latches the vertical DMA enable once per line
    /// based on VSTART/VSTOP comparisons, not per-CCK.  Without this latch,
    /// wrap-around window checks (e.g. VSTART=$FFF from COP1 init) falsely
    /// enable DMA on the first display line, causing BPLxPT to advance before
    /// the copper has written correct pointer values.
    bpl_dma_vactive_latch: bool,
}

impl Amiga {
    pub fn new(kickstart: Vec<u8>) -> Self {
        Self::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ocs,
            kickstart,
        })
    }

    /// Construct a machine instance from a config object.
    ///
    /// Model/chipset combinations are introduced incrementally; unsupported
    /// combinations may still behave like the nearest implemented baseline.
    pub fn new_with_config(config: AmigaConfig) -> Self {
        let AmigaConfig {
            model,
            chipset,
            kickstart,
        } = config;
        let chip_ram_size = match model {
            AmigaModel::A500 => 512 * 1024,
            // A500+ ships with 1MB chip RAM.
            AmigaModel::A500Plus => 1024 * 1024,
        };

        let agnus = match chipset {
            AmigaChipset::Ocs => {
                commodore_agnus_ecs::AgnusEcs::from_ocs(commodore_agnus_ocs::Agnus::new())
            }
            AmigaChipset::Ecs => commodore_agnus_ecs::AgnusEcs::new(),
        };
        let denise = match chipset {
            AmigaChipset::Ocs => {
                commodore_denise_ecs::DeniseEcs::from_ocs(commodore_denise_ocs::DeniseOcs::new())
            }
            AmigaChipset::Ecs => commodore_denise_ecs::DeniseEcs::new(),
        };

        let mut cpu = Cpu68000::new();
        let memory = Memory::new(chip_ram_size, kickstart);

        // Initial reset vectors come from ROM (overlay is ON at power-on,
        // mapping Kickstart to $000000).
        let ssp = (u32::from(memory.kickstart[0]) << 24)
            | (u32::from(memory.kickstart[1]) << 16)
            | (u32::from(memory.kickstart[2]) << 8)
            | u32::from(memory.kickstart[3]);
        let pc = (u32::from(memory.kickstart[4]) << 24)
            | (u32::from(memory.kickstart[5]) << 16)
            | (u32::from(memory.kickstart[6]) << 8)
            | u32::from(memory.kickstart[7]);

        cpu.reset_to(ssp, pc);

        // CIA-A PRA external inputs (active-low accent signals):
        //   Bit 7: /FIR1 = 1 (joystick fire not pressed)
        //   Bit 6: /FIR0 = 1 (joystick fire not pressed)
        //   Bit 5: /DSKRDY = 1 (drive not ready)
        //   Bit 4: /DSKTRACK0 = 0 (at track 0)
        //   Bit 3: /DSKPROT = 1 (not write protected)
        //   Bit 2: /DSKCHANGE = 0 (disk removed — no disk in drive)
        //   Bits 1,0: LED/OVL outputs, external pull-up = 1,1
        let mut cia_a = Cia8520::new("A");
        cia_a.external_a = 0xEB; // 0b_1110_1011

        Self {
            master_clock: 0,
            model,
            chipset,
            cpu,
            agnus,
            memory,
            denise,
            copper: Copper::new(),
            cia_a,
            cia_b: Cia8520::new("B"),
            paula: Paula8364::new(),
            floppy: AmigaFloppyDrive::new(),
            keyboard: AmigaKeyboard::new(),
            audio_sample_phase: 0,
            audio_buffer: Vec::with_capacity((AUDIO_SAMPLE_RATE as usize / 50) * 4),
            disk_dma_runtime: None,
            sprite_dma_phase: [0; 8],
            beam_debug_snapshot: BeamDebugSnapshot::default(),
            beam_edge_flags: BeamEdgeFlags::default(),
            beam_pixel_outputs_debug: BeamPixelOutputDebug::default(),
            blitter_progress_debug: BlitterProgressDebugStats::default(),
            blitter_shadow_compare_snapshot: None,
            blitter_area_shadow_compare_started: 0,
            blitter_line_shadow_compare_started: 0,
            bplcon0_denise_pending: None,
            ddfstrt_pending: None,
            ddfstop_pending: None,
            color_pending: Vec::new(),
            bpl_dma_vactive_latch: false,
        }
    }

    pub fn tick(&mut self) {
        self.master_clock += 1;

        if self.master_clock % TICKS_PER_CCK == 0 {
            let vpos = self.agnus.vpos;
            let hpos = self.agnus.hpos;
            if hpos == 0 {
                self.denise.begin_beam_line();
                // Clear this line's framebuffer to COLOR00 (border color).
                // The render path may not write every pixel in the line —
                // e.g. when DDFSTRT is high enough that fewer than 160 CCKs
                // remain before line end. Without this clear, unwritten
                // tail pixels would retain stale data from previous frames.
                let fb_y = vpos.wrapping_sub(DISPLAY_VSTART);
                if (fb_y as u32) < commodore_denise_ocs::FB_HEIGHT as u32 {
                    self.denise.clear_fb_line_to_background(fb_y as u32);
                }
                // Update vertical bitplane DMA flip-flop at line start.
                // On real hardware Agnus uses a flip-flop that is SET when the
                // beam reaches VSTART and CLEARED when it reaches VSTOP. It is
                // NOT recalculated from scratch every line — it's persistent
                // state. This matters when VSTART is unreachable (e.g. $FFF
                // from COP1 init): the flip-flop stays cleared from the
                // previous VSTOP, even though a wrap-around range check would
                // return true.
                self.update_bpl_dma_vactive_flipflop(vpos);
            }
            let prev_sync = self.beam_debug_snapshot.sync;
            let prev_snapshot = self.beam_debug_snapshot;
            let current_snapshot = self.beam_debug_snapshot_at(vpos, hpos);
            let current_sync = current_snapshot.sync;
            self.beam_debug_snapshot = current_snapshot;
            self.beam_edge_flags = BeamEdgeFlags {
                hsync_changed: prev_snapshot.sync.hsync != current_snapshot.sync.hsync,
                vsync_changed: prev_snapshot.sync.vsync != current_snapshot.sync.vsync,
                hblank_changed: prev_snapshot.hblank != current_snapshot.hblank,
                vblank_changed: prev_snapshot.vblank != current_snapshot.vblank,
                visible_changed: prev_snapshot.fb_coords.is_some()
                    != current_snapshot.fb_coords.is_some(),
            };

            let hsync_tod_pulse =
                if self.chipset == AmigaChipset::Ecs && self.agnus.varhsyen_enabled() {
                    !prev_sync.hsync && current_sync.hsync
                } else {
                    hpos == 0
                };
            let vsync_tod_pulse =
                if self.chipset == AmigaChipset::Ecs && self.agnus.varvsyen_enabled() {
                    !prev_sync.vsync && current_sync.vsync
                } else {
                    vpos == 0 && hpos == 0
                };

            // VERTB fires at the start of vblank (beam at line 0, start of frame).
            // The check runs before tick_cck(), so vpos/hpos reflect the current
            // beam position. vpos=0, hpos=0 means the beam just wrapped from the
            // end of the previous frame.
            // CIA-B TOD input is HSYNC — pulse once per scanline.
            if hsync_tod_pulse {
                self.cia_b.tod_pulse();
            }

            if vpos == 0 && hpos == 0 {
                // bit 5 = VERTB
                self.paula.request_interrupt(5);
                // Agnus restarts the copper from COP1LC at vertical blank,
                // but only when copper DMA is enabled (DMAEN + COPEN).
                if self.agnus.dma_enabled(0x0080) {
                    self.copper.restart_cop1();
                }
            }

            // CIA-A TOD input is VSYNC. On OCS we pulse at frame wrap; on ECS
            // with variable VSYNC enabled we pulse on the programmable sync
            // window rising edge instead.
            if vsync_tod_pulse {
                self.cia_a.tod_pulse();
            }

            // --- Drain pending register pipeline writes ---
            // Agnus→Denise register writes propagate with a 2-CCK delay.
            if let Some((val, ref mut countdown)) = self.bplcon0_denise_pending {
                if *countdown <= 1 {
                    self.denise.bplcon0 = val;
                    self.bplcon0_denise_pending = None;
                } else {
                    *countdown -= 1;
                }
            }
            if let Some((val, ref mut countdown)) = self.ddfstrt_pending {
                if *countdown <= 1 {
                    self.agnus.ddfstrt = val;
                    self.ddfstrt_pending = None;
                } else {
                    *countdown -= 1;
                }
            }
            if let Some((val, ref mut countdown)) = self.ddfstop_pending {
                if *countdown <= 1 {
                    self.agnus.ddfstop = val;
                    self.ddfstop_pending = None;
                } else {
                    *countdown -= 1;
                }
            }
            self.color_pending.retain_mut(|(idx, val, countdown)| {
                if *countdown <= 1 {
                    self.denise.set_palette(*idx, *val);
                    false
                } else {
                    *countdown -= 1;
                    true
                }
            });

            // --- Output pixels BEFORE DMA ---
            // This creates the current pipeline delay model: shift registers
            // hold data from the PREVIOUS fetch group. New data loaded this
            // CCK won't appear until the next output.
            let hires = (self.agnus.bplcon0 & 0x8000) != 0;
            if hires {
                // WinUAE folds DDFSTRT fetch-start phase into effective BPLCON1
                // alignment. Mirror the OCS/ECS hires fetch-start contribution
                // as a small comparator phase bias (mask width = 3 bits).
                let ddf_phase_term =
                    (((self.agnus.ddfstrt.wrapping_sub(0x0018)) & 0x0003) << 1) as u8;
                self.denise.set_bplcon1_phase_bias(ddf_phase_term & 0x07);
            } else {
                self.denise.set_bplcon1_phase_bias(0);
            }
            let beam_x0_u16 = hpos.wrapping_mul(2);
            let beam_x1_u16 = beam_x0_u16.wrapping_add(1);
            let beam_x0 = u32::from(beam_x0_u16);
            let beam_x1 = u32::from(beam_x1_u16);
            let beam_y = u32::from(vpos);
            let playfield_gate0 = self.playfield_window_active_beam_x(vpos, hpos, beam_x0_u16);
            let playfield_gate1 = self.playfield_window_active_beam_x(vpos, hpos, beam_x1_u16);
            let first_pixel_cck = self.agnus.ddfstrt.wrapping_add(if hires { 4 } else { 8 });
            let mut visible_ccks = (commodore_denise_ocs::FB_WIDTH / 2) as u16;
            let mut diw_hstart_beam_x = None;
            let mut diw_hstop_beam_x = None;
            if hires
                && let Some((ecs_hspan_cck, hstart, hstop)) =
                    self.ecs_display_h_span_cck_and_bounds()
            {
                visible_ccks = ecs_hspan_cck;
                diw_hstart_beam_x = Some(hstart);
                diw_hstop_beam_x = Some(hstop);
            }
            let mut odd_scroll = ((self.denise.bplcon1 >> 4) & 0x000F) as u16;
            let mut even_scroll = (self.denise.bplcon1 & 0x000F) as u16;
            if hires {
                // HRM: hires horizontal scrolling is in 2-pixel increments,
                // so the low bit of each delay nibble is effectively ignored.
                odd_scroll &= !1;
                even_scroll &= !1;
            }
            let max_scroll = odd_scroll.max(even_scroll);
            let hidden_margin_ccks = if hires {
                (max_scroll + 3) / 4
            } else {
                (max_scroll + 1) / 2
            };
            let serial_window_start = first_pixel_cck.wrapping_sub(hidden_margin_ccks);
            let serial_window_ccks = visible_ccks.wrapping_add(hidden_margin_ccks);
            let cck_rel = hpos.wrapping_sub(serial_window_start);
            let in_serial_window = cck_rel < serial_window_ccks;
            // Denise BPL1DAT copy comparator is keyed to Denise's horizontal
            // counter domain (WinUAE's `denise_hcounter_cmp`), not the host
            // render serial-window origin. Keep this un-biased for now; line
            // buffer/display placement can be adjusted separately.
            self.denise.set_serial_phase_bias(0);
            let hires_linebuf_offsets = if hires {
                // Host hires preview X maps directly to our serial-window
                // origin in this model: first visible host pixel corresponds
                // to the first visible beam sample (`first_pixel_cck`), while
                // the line-buffer cursor starts at `serial_window_start`.
                //
                // Keep placement exact to that relation and let the explicit
                // BPL1DAT/HDIW offsets below handle the rest. The earlier
                // WinUAE-inspired `delayoffset` approximation was useful as a
                // probe, but it also mixed concerns and distorted placement.
                let sample_offset = i32::from(hidden_margin_ccks.saturating_mul(4));
                let bpl1dat_trigger_offset = i32::from(first_pixel_cck.wrapping_sub(serial_window_start)) * 4;
                let serial_origin_beam_x = serial_window_start.wrapping_mul(2);
                let line_beam_len = (commodore_agnus_ocs::PAL_CCKS_PER_LINE as u16) * 2;
                let serial_idx_from_beam_x = |beam_x: u16| -> i32 {
                    let delta_beam = if beam_x >= serial_origin_beam_x {
                        beam_x - serial_origin_beam_x
                    } else {
                        line_beam_len - serial_origin_beam_x + beam_x
                    };
                    i32::from(delta_beam) * 2
                };
                let (hstart_offset, hstop_offset) = match (diw_hstart_beam_x, diw_hstop_beam_x) {
                    (Some(hstart), Some(hstop)) if hstart != hstop => {
                        // Wrap-around HDIW windows are possible on ECS, but
                        // the current line-buffer preview path only models a
                        // single contiguous playfield span. Defer wrap support
                        // until the full line renderer is in place.
                        if hstart < hstop {
                            (
                                Some(serial_idx_from_beam_x(hstart)),
                                Some(serial_idx_from_beam_x(hstop)),
                            )
                        } else {
                            (None, None)
                        }
                    }
                    _ => (None, None),
                };
                commodore_denise_ocs::DeniseHiresLinebufOffsets {
                    sample_offset,
                    bpl1dat_trigger_offset,
                    hstart_offset,
                    hstop_offset,
                }
            } else {
                commodore_denise_ocs::DeniseHiresLinebufOffsets::default()
            };
            if hpos == 0 {
                self.denise.set_hires_linebuf_offsets(hires_linebuf_offsets);
            }
            let pixel0_debug = if let Some((fb_x, fb_y)) =
                self.beam_to_fb_render_beam_x(vpos, hpos, beam_x0_u16)
            {
                    self.denise
                        .output_pixel_with_beam_and_playfield_gate(
                            fb_x, fb_y, beam_x0, beam_y, playfield_gate0,
                        )
                } else if in_serial_window {
                    // Denise keeps shifting through pre-window scroll pixels (and
                    // clipped pixels generally) even when the framebuffer output is
                    // not visible. Otherwise BPLCON1 alignment drifts in hires modes.
                    self.denise
                        .output_pixel_with_beam_and_playfield_gate(
                            u32::MAX, u32::MAX, beam_x0, beam_y, playfield_gate0,
                        )
                } else {
                    commodore_denise_ocs::DeniseOutputPixelDebug::default()
                };
            let pixel1_debug = if let Some((fb_x, fb_y)) =
                self.beam_to_fb_render_beam_x(vpos, hpos, beam_x1_u16)
            {
                self.denise
                    .output_pixel_with_beam_and_playfield_gate(
                        fb_x, fb_y, beam_x1, beam_y, playfield_gate1,
                    )
            } else if in_serial_window {
                self.denise
                    .output_pixel_with_beam_and_playfield_gate(
                        u32::MAX, u32::MAX, beam_x1, beam_y, playfield_gate1,
                    )
            } else {
                commodore_denise_ocs::DeniseOutputPixelDebug::default()
            };

            // --- DMA slots ---
            let bus_plan = self.agnus.cck_bus_plan();
            let audio_dma_slot = bus_plan.audio_dma_service_channel;
            if bus_plan.disk_dma_slot_granted {
                self.service_disk_dma_slot();
            }
            if let Some(sprite) = bus_plan.sprite_dma_service_channel {
                self.service_sprite_dma_slot(sprite as usize);
            }
            let mut copper_used_chip_bus = false;
            let mut fetched_plane_0 = false;
            let mut bitplane_dma_fetch_plane = bus_plan.bitplane_dma_fetch_plane;
            if bitplane_dma_fetch_plane.is_some() && !self.bitplane_dma_vertical_active(vpos) {
                bitplane_dma_fetch_plane = None;
            }
            if let Some(plane) = bitplane_dma_fetch_plane {
                let idx = plane as usize;
                let addr = self.agnus.bpl_pt[idx];
                let hi = self.memory.read_chip_byte(addr);
                let lo = self.memory.read_chip_byte(addr | 1);
                let val = (u16::from(hi) << 8) | u16::from(lo);
                self.denise.load_bitplane(idx, val);
                self.agnus.bpl_pt[idx] = addr.wrapping_add(2);
                if plane == 0 {
                    fetched_plane_0 = true;
                    if hires {
                        // WinUAE records BPL1DAT trigger timing in the line
                        // renderer's internal output-counter domain and later
                        // adds one RES_MAX unit (`+4` at ECS hires).
                        let output_pos =
                            i32::from(hpos.wrapping_sub(serial_window_start)) * 4 + 4;
                        self.denise.note_hires_linebuf_bpl1dat_trigger_offset(output_pos);
                        self.denise.trigger_shift_load();
                    } else {
                        self.denise.trigger_shift_load();
                    }
                }
            } else if bus_plan.copper_dma_slot_granted {
                let copper_used_chip_bus_cell = std::cell::Cell::new(false);
                let res = {
                    let memory = &self.memory;
                    self.copper.tick(vpos, hpos, |addr| {
                        copper_used_chip_bus_cell.set(true);
                        let hi = memory.read_chip_byte(addr);
                        let lo = memory.read_chip_byte(addr | 1);
                        (u16::from(hi) << 8) | u16::from(lo)
                    })
                };
                copper_used_chip_bus = copper_used_chip_bus_cell.get();
                if let Some((reg, val)) = res {
                    // COPCON protection (HRM Ch.2): copper cannot write
                    // registers $000-$03E at all, and $040-$07E only
                    // when the CDANG (danger) bit is set in COPCON.
                    if reg >= 0x080 || (reg >= 0x040 && self.copper.danger) {
                        if reg == 0x09C && (val & 0x0010) != 0 {
                            self.paula.request_interrupt(4);
                        }
                        self.write_custom_reg(reg, val);
                    }
                }
            }
            let audio_return_progress_this_cck =
                bus_plan.paula_return_progress(copper_used_chip_bus);

            if self.agnus.blitter_busy && self.agnus.dma_enabled(0x0040) {
                self.blitter_progress_debug.busy_ccks += 1;
                self.blitter_progress_debug.max_queue_len_seen = self
                    .blitter_progress_debug
                    .max_queue_len_seen
                    .max(self.agnus.blitter_ccks_remaining);
                if bus_plan.blitter_dma_progress_granted {
                    self.blitter_progress_debug.cpu_slot_grant_ccks += 1;
                }
                match bus_plan.slot_owner {
                    SlotOwner::Copper => {
                        if copper_used_chip_bus {
                            self.blitter_progress_debug.copper_slot_busy_ccks += 1;
                        } else {
                            self.blitter_progress_debug.copper_slot_idle_ccks += 1;
                        }
                    }
                    SlotOwner::Bitplane(_) => self.blitter_progress_debug.bitplane_slot_ccks += 1,
                    SlotOwner::Refresh => self.blitter_progress_debug.refresh_slot_ccks += 1,
                    SlotOwner::Sprite(_) => self.blitter_progress_debug.sprite_slot_ccks += 1,
                    SlotOwner::Disk => self.blitter_progress_debug.disk_slot_ccks += 1,
                    SlotOwner::Audio(_) => self.blitter_progress_debug.audio_slot_ccks += 1,
                    SlotOwner::Cpu => {}
                }
            }

            // Apply bitplane modulo after the last fetch group of the line.
            if fetched_plane_0 {
                // Bitplane shift-load is already triggered above in the DMA
                // dispatch block (trigger_shift_load for lowres,
                // queue_shift_load_from_bpl1dat for hires). Do NOT repeat it
                // here — a second trigger_shift_load corrupts the BPLCON1
                // barrel-shift carry by overwriting bpl_prev_data.
                // Plane 0 is fetched at the end of the current DDF group:
                // ddfseq position 7 in lowres, 3 in hires (simplified model).
                let group_end_offset = if (self.agnus.bplcon0 & 0x8000) != 0 {
                    3
                } else {
                    7
                };
                let hires = (self.agnus.bplcon0 & 0x8000) != 0;
                let group_start = hpos - group_end_offset;
                let modulo_threshold = if hires {
                    self.agnus.ddfstop.wrapping_add(4)
                } else {
                    self.agnus.ddfstop
                };
                if group_start >= modulo_threshold {
                    let num_bpl = self.agnus.num_bitplanes();
                    for i in 0..num_bpl as usize {
                        let modulo = if i % 2 == 0 {
                            self.agnus.bpl1mod // Odd planes (BPL1/3/5)
                        } else {
                            self.agnus.bpl2mod // Even planes (BPL2/4/6)
                        };
                        self.agnus.bpl_pt[i] = (self.agnus.bpl_pt[i] as i32 + modulo as i32) as u32;
                    }
                }
            }

            self.beam_pixel_outputs_debug = BeamPixelOutputDebug {
                vpos,
                hpos_cck: hpos,
                bpl1dat_trigger_cck: first_pixel_cck,
                serial_window_start_cck: serial_window_start,
                serial_window_len_cck: serial_window_ccks,
                serial_window_active: in_serial_window,
                diw_hstart_beam_x,
                diw_hstop_beam_x,
                pixel0: pixel0_debug,
                pixel1: pixel1_debug,
            };

            self.paula.tick_audio_cck_with_bus(
                self.agnus.dmacon,
                audio_dma_slot,
                audio_return_progress_this_cck,
                |addr| self.memory.read_chip_byte(addr),
            );
            self.paula.tick_disk_cck();

            // Coarse blitter scheduler: preserve BUSY across CCKs so Agnus bus
            // arbitration (including nasty-mode CPU steals) affects machine
            // timing before the existing synchronous blit implementation runs.
            // Progress now advances only on explicit Agnus free-slot grants.
            self.maybe_capture_area_blit_shadow_compare();
            self.maybe_capture_line_blit_shadow_compare();
            let blitter_progress_this_cck = bus_plan.blitter_dma_progress_granted
                || (matches!(bus_plan.slot_owner, SlotOwner::Copper)
                    && self.agnus.blitter_busy
                    && self.agnus.dma_enabled(0x0040)
                    && !copper_used_chip_bus);
            if blitter_progress_this_cck
                && matches!(bus_plan.slot_owner, SlotOwner::Copper)
                && !copper_used_chip_bus
                && self.agnus.blitter_busy
                && self.agnus.dma_enabled(0x0040)
            {
                self.blitter_progress_debug.copper_idle_grant_ccks += 1;
            }
            if let Some(blit_op) = self
                .agnus
                .tick_blitter_scheduler_op(blitter_progress_this_cck)
            {
                let line_mode = (self.agnus.bltcon1 & 0x0001) != 0;
                if line_mode && machine_experiment_sync_line_blitter() {
                    self.agnus.clear_blitter_scheduler();
                    execute_blit(&mut self.agnus, &mut self.paula, &mut self.memory);
                } else if (self.agnus.bltcon1 & 0x0001) == 0
                    && machine_experiment_sync_area_blitter()
                {
                    self.agnus.clear_blitter_scheduler();
                    execute_blit(&mut self.agnus, &mut self.paula, &mut self.memory);
                } else {
                    let drain_incremental_area = (self.agnus.bltcon1 & 0x0001) == 0
                        && machine_experiment_drain_incremental_area_blitter();
                    let burst_incremental_area_ops = if (self.agnus.bltcon1 & 0x0001) == 0 {
                        machine_experiment_burst_incremental_area_blitter_ops()
                    } else {
                        0
                    };
                    let mut incremental_completed =
                        execute_incremental_blitter_op(&mut self.agnus, &mut self.memory, blit_op);
                    self.blitter_progress_debug.granted_ops += 1;
                    if burst_incremental_area_ops > 0 && !incremental_completed {
                        for _ in 0..burst_incremental_area_ops {
                            let Some(next_op) = self.agnus.tick_blitter_scheduler_op(true) else {
                                break;
                            };
                            incremental_completed = execute_incremental_blitter_op(
                                &mut self.agnus,
                                &mut self.memory,
                                next_op,
                            );
                            self.blitter_progress_debug.granted_ops += 1;
                            if incremental_completed {
                                break;
                            }
                        }
                    }
                    if drain_incremental_area && !incremental_completed {
                        while let Some(next_op) = self.agnus.tick_blitter_scheduler_op(true) {
                            incremental_completed = execute_incremental_blitter_op(
                                &mut self.agnus,
                                &mut self.memory,
                                next_op,
                            );
                            self.blitter_progress_debug.granted_ops += 1;
                            if incremental_completed {
                                break;
                            }
                        }
                    }
                    if incremental_completed {
                        if line_mode {
                            self.finish_line_blit_shadow_compare();
                        } else {
                            self.finish_area_blit_shadow_compare();
                        }
                        self.agnus.clear_blitter_scheduler();
                        self.agnus.blitter_busy = false;
                        self.paula.request_interrupt(6);
                    }
                }
            }
            if self.agnus.blitter_exec_ready() {
                self.blitter_shadow_compare_snapshot = None;
                execute_blit(&mut self.agnus, &mut self.paula, &mut self.memory);
            }

            self.audio_sample_phase += u64::from(AUDIO_SAMPLE_RATE);
            while self.audio_sample_phase >= PAL_CCK_HZ {
                self.audio_sample_phase -= PAL_CCK_HZ;
                let (left, right) = self.paula.mix_audio_stereo();
                self.audio_buffer.push(left);
                self.audio_buffer.push(right);
            }

            self.agnus.tick_cck();

            // Check for pending disk DMA after CCK tick
            if self.paula.disk_dma_pending {
                self.paula.disk_dma_pending = false;
                self.start_disk_dma_transfer();
            }
        }

        // Cpu68000::tick() already self-gates to 4-crystal boundaries.
        // Call it every master tick so we don't double-apply the divide-by-4.
        let mut bus = AmigaBusWrapper {
            chipset: self.chipset,
            agnus: &mut self.agnus,
            memory: &mut self.memory,
            denise: &mut self.denise,
            copper: &mut self.copper,
            cia_a: &mut self.cia_a,
            cia_b: &mut self.cia_b,
            paula: &mut self.paula,
            floppy: &mut self.floppy,
            keyboard: &mut self.keyboard,
            bplcon0_denise_pending: &mut self.bplcon0_denise_pending,
            ddfstrt_pending: &mut self.ddfstrt_pending,
            ddfstop_pending: &mut self.ddfstop_pending,
            color_pending: &mut self.color_pending,
        };
        self.cpu.tick(&mut bus, self.master_clock);

        if self.master_clock % TICKS_PER_ECLOCK == 0 {
            self.cia_a.tick();
            if self.cia_a.irq_active() {
                self.paula.request_interrupt(3);
            }
            self.cia_b.tick();
            if self.cia_b.irq_active() {
                self.paula.request_interrupt(13);
            }

            // Floppy drive motor spin-up timer
            self.floppy.tick();

            // Update CIA-A PRA with floppy status (active-low signals)
            let status = self.floppy.status();
            let mut ext_a = self.cia_a.external_a;
            // PA2: /DSKCHANGE — 0 when disk changed
            if status.disk_change {
                ext_a &= !0x04;
            } else {
                ext_a |= 0x04;
            }
            // PA3: /DSKPROT — 0 when write-protected
            if status.write_protect {
                ext_a &= !0x08;
            } else {
                ext_a |= 0x08;
            }
            // PA4: /DSKTRACK0 — 0 when at track 0
            if status.track0 {
                ext_a &= !0x10;
            } else {
                ext_a |= 0x10;
            }
            // PA5: /DSKRDY — 0 when motor at speed
            if status.ready {
                ext_a &= !0x20;
            } else {
                ext_a |= 0x20;
            }
            self.cia_a.external_a = ext_a;

            // Keyboard: tick and inject serial byte if ready
            if let Some(byte) = self.keyboard.tick() {
                self.cia_a.receive_serial_byte(byte);
            }
        }
    }

    pub fn write_custom_reg(&mut self, offset: u16, val: u16) {
        if (0x120..=0x13E).contains(&offset) && (offset & 2) != 0 {
            let idx = ((offset - 0x120) / 4) as usize;
            if idx < 8 {
                // Treat the low-word pointer write as the commit point for
                // restarting sprite DMA control-word fetch sequencing.
                self.sprite_dma_phase[idx] = 0;
            }
        }
        if (0x140..=0x17E).contains(&offset) {
            let sprite = ((offset - 0x140) / 8) as usize;
            let reg = ((offset - 0x140) % 8) / 2;
            if sprite < 8 && reg == 1 {
                // HRM: writing SPRxCTL disables the sprite DMA channel until the
                // vertical beam counter matches VSTART again.
                self.sprite_dma_phase[sprite] = 4;
            }
        }
        queue_pipelined_write(
            &mut self.bplcon0_denise_pending,
            &mut self.ddfstrt_pending,
            &mut self.ddfstop_pending,
            &mut self.color_pending,
            offset,
            val,
        );
        write_custom_register(
            self.chipset,
            &mut self.agnus,
            &mut self.denise,
            &mut self.copper,
            &mut self.paula,
            &mut self.memory,
            offset,
            val,
        );
    }

    /// Advance the machine by one PAL video frame (A500/OCS timing).
    pub fn run_frame(&mut self) {
        for _ in 0..PAL_FRAME_TICKS {
            self.tick();
        }
    }

    /// Borrow the current raw ARGB framebuffer (320x256).
    pub fn framebuffer(&self) -> &[u32] {
        &self.denise.framebuffer
    }

    /// Borrow the hires preview framebuffer (640x256).
    ///
    /// This is primarily a debug/screenshot aid for ECS/KS2.x hires screens.
    #[must_use]
    pub fn framebuffer_hires(&self) -> &[u32] {
        &self.denise.framebuffer_hires
    }

    /// Drain interleaved stereo audio samples (`f32`, `L,R,...`).
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.audio_buffer)
    }

    /// Insert a disk image into the internal floppy drive (DF0:).
    pub fn insert_disk(&mut self, adf: Adf) {
        self.floppy.insert_disk(adf);
    }

    /// Eject the current DF0: disk, if any.
    pub fn eject_disk(&mut self) {
        self.floppy.eject_disk();
    }

    /// Return whether DF0: currently has a disk inserted.
    pub fn has_disk(&self) -> bool {
        self.floppy.has_disk()
    }

    /// Queue an Amiga keyboard event (raw Amiga keycode).
    pub fn key_event(&mut self, keycode: u8, pressed: bool) {
        self.keyboard.key_event(keycode, pressed);
    }

    /// Inject a keyboard handshake pulse from the host side.
    pub fn keyboard_handshake(&mut self) {
        self.keyboard.handshake();
    }

    /// Start a disk DMA transfer (DSKLEN double-write protocol).
    ///
    /// Data movement is performed incrementally on Agnus disk DMA slots.
    fn start_disk_dma_transfer(&mut self) {
        let word_count = (self.paula.dsklen & 0x3FFF) as u32;
        let is_write = self.paula.dsklen & 0x4000 != 0;

        if word_count == 0 {
            // Keep existing boot-level behavior: completion interrupt even with a
            // zero transfer count.
            self.paula.request_interrupt(1);
            self.disk_dma_runtime = None;
            return;
        }

        let data = if is_write {
            Vec::new()
        } else {
            self.floppy.encode_mfm_track().unwrap_or_default()
        };
        let wordsync_enabled = !is_write && (self.paula.adkcon & 0x0400 != 0);
        self.disk_dma_runtime = Some(DiskDmaRuntime {
            data,
            byte_index: 0,
            words_remaining: word_count,
            is_write,
            wordsync_enabled,
            wordsync_waiting: wordsync_enabled,
        });
    }

    /// Service one Agnus disk DMA slot.
    fn service_disk_dma_slot(&mut self) {
        if self.disk_dma_runtime.is_none() {
            // Simplified programmed-I/O disk write path: consume queued DSKDAT
            // words on disk slots when write mode is selected and no DMA
            // transfer is active.
            if (self.paula.dsklen & 0x4000) != 0
                && let Some(word) = self.paula.take_dskdat_queued_word()
            {
                self.floppy.note_write_mfm_word(word);
                self.paula.note_disk_write_pio_word(word);
            }
            return;
        }

        let Some(runtime) = self.disk_dma_runtime.as_mut() else {
            return;
        };

        if runtime.words_remaining == 0 {
            self.disk_dma_runtime = None;
            return;
        }

        let mut dma_word_completed = false;
        if !runtime.is_write {
            let mut stream_word: Option<(u8, u8, u16)> = None;
            if runtime.data.len() >= 2 {
                let len = runtime.data.len();
                let hi = runtime.data[runtime.byte_index % len];
                let lo = runtime.data[(runtime.byte_index + 1) % len];
                runtime.byte_index = (runtime.byte_index + 2) % len;
                let word = (u16::from(hi) << 8) | u16::from(lo);
                stream_word = Some((hi, lo, word));
            }

            if let Some((hi, lo, word)) = stream_word {
                // Simplified disk path: surface Paula disk read state from the
                // DMA stream even though the full serial disk decoder is not modeled.
                let matched_sync = self.paula.note_disk_read_word(word);
                if matched_sync {
                    self.paula.request_interrupt(12); // DSKSYN
                }

                let suppress_dma_write = if runtime.wordsync_enabled {
                    if runtime.wordsync_waiting {
                        if matched_sync {
                            // HRM: DMA starts with the following word after a DSKSYNC match.
                            runtime.wordsync_waiting = false;
                        }
                        true
                    } else {
                        // HRM: during read DMA, resync every time the match word is found.
                        matched_sync
                    }
                } else {
                    false
                };

                if !suppress_dma_write {
                    let mut addr = self.agnus.dsk_pt;
                    self.memory.write_byte(addr, hi);
                    addr = addr.wrapping_add(1);
                    self.memory.write_byte(addr, lo);
                    addr = addr.wrapping_add(1);
                    self.agnus.dsk_pt = addr;
                    dma_word_completed = true;
                }
            }
        } else {
            let mut addr = self.agnus.dsk_pt;
            let hi = self.memory.read_chip_byte(addr);
            addr = addr.wrapping_add(1);
            let lo = self.memory.read_chip_byte(addr);
            addr = addr.wrapping_add(1);
            let word = (u16::from(hi) << 8) | u16::from(lo);
            self.floppy.note_write_mfm_word(word);
            self.paula.note_disk_write_dma_word(word);
            self.agnus.dsk_pt = addr;
            dma_word_completed = true;
        }

        if dma_word_completed {
            runtime.words_remaining = runtime.words_remaining.saturating_sub(1);
            if runtime.words_remaining == 0 {
                self.disk_dma_runtime = None;
                // DSKBLK interrupt on transfer completion.
                self.paula.request_interrupt(1);
            }
        }
    }

    fn sprite_line_active(vpos: u16, vstart: u16, vstop: u16) -> bool {
        if vstart == vstop {
            return false;
        }
        if vstart < vstop {
            vpos >= vstart && vpos < vstop
        } else {
            // Wrapped sprite: active from VSTART..end_of_frame and 0..VSTOP-1.
            vpos >= vstart || vpos < vstop
        }
    }

    fn next_sprite_dma_vpos(vpos: u16) -> u16 {
        let next = vpos.wrapping_add(1);
        if next >= commodore_agnus_ocs::PAL_LINES_PER_FRAME {
            0
        } else {
            next
        }
    }

    /// Service one Agnus sprite DMA slot.
    ///
    /// Minimal OCS bring-up model: fetch one word from `SPRxPT` and advance a
    /// coarse sprite DMA phase machine:
    /// - first two slots after pointer reload fetch `SPRxPOS` / `SPRxCTL`
    /// - subsequent slots fetch `SPRxDATA` / `SPRxDATB` pairs
    ///
    /// This is still not a full hardware sprite DMA state machine, but it
    /// keeps Denise sprite registers coherent enough for basic rendering.
    fn service_sprite_dma_slot(&mut self, sprite: usize) {
        if sprite >= 8 {
            return;
        }
        let vpos = self.agnus.vpos;
        if self.sprite_dma_phase[sprite] == 4 {
            let pos = self.denise.spr_pos[sprite];
            let ctl = self.denise.spr_ctl[sprite];
            let vstart = (((ctl >> 2) & 0x0001) << 8) | ((pos >> 8) & 0x00FF);
            let vstop = (((ctl >> 1) & 0x0001) << 8) | ((ctl >> 8) & 0x00FF);
            // HRM: sprite DMA remains disabled until the beam equals VSTART.
            if vpos != vstart {
                return;
            }
            // If VSTOP==VSTART, no sprite lines are output; the next fetched
            // word pair becomes the next SPRxPOS/SPRxCTL instead of DATA/DATB.
            self.sprite_dma_phase[sprite] = if vstop == vstart { 0 } else { 2 };
        }

        let addr = self.agnus.spr_pt[sprite];
        let hi = self.memory.read_chip_byte(addr);
        let lo = self.memory.read_chip_byte(addr | 1);
        let word = (u16::from(hi) << 8) | u16::from(lo);
        match self.sprite_dma_phase[sprite] {
            0 => {
                self.denise.write_sprite_pos(sprite, word);
                self.sprite_dma_phase[sprite] = 1;
            }
            1 => {
                self.denise.write_sprite_ctl(sprite, word);
                let pos = self.denise.spr_pos[sprite];
                let ctl = self.denise.spr_ctl[sprite];
                let vstart = (((ctl >> 2) & 0x0001) << 8) | ((pos >> 8) & 0x00FF);
                let vstop = (((ctl >> 1) & 0x0001) << 8) | ((ctl >> 8) & 0x00FF);
                self.sprite_dma_phase[sprite] = if vstop != vstart && vpos == vstart {
                    2
                } else {
                    4
                };
            }
            2 => {
                self.denise.write_sprite_data(sprite, word);
                self.sprite_dma_phase[sprite] = 3;
            }
            _ => {
                self.denise.write_sprite_datb(sprite, word);
                let pos = self.denise.spr_pos[sprite];
                let ctl = self.denise.spr_ctl[sprite];
                let vstart = (((ctl >> 2) & 0x0001) << 8) | ((pos >> 8) & 0x00FF);
                let vstop = (((ctl >> 1) & 0x0001) << 8) | ((ctl >> 8) & 0x00FF);
                let next_vpos = Self::next_sprite_dma_vpos(vpos);
                self.sprite_dma_phase[sprite] =
                    if Self::sprite_line_active(next_vpos, vstart, vstop) {
                        2
                    } else {
                        0
                    };
            }
        }
        self.agnus.spr_pt[sprite] = addr.wrapping_add(2);
    }

    fn beam_to_fb_beam_x(&self, vpos: u16, hpos_cck: u16, beam_x: u16) -> Option<(u32, u32)> {
        let fb_y = if self.chipset == AmigaChipset::Ecs {
            // Coarse ECS hard-stop scaffolding: when programmable beam/sync/blank
            // control is active but neither HARDDIS nor VARVBEN disables the
            // legacy vertical hard stop, clamp visibility to the legacy OCS
            // vertical display span. This is intentionally narrower than a full
            // ECS signal-generator model and avoids changing DIWHIGH-only paths.
            let legacy_vertical_hard_stop_active = self.agnus.varbeamen_enabled()
                && !self.agnus.harddis_enabled()
                && !self.agnus.varvben_enabled();
            if legacy_vertical_hard_stop_active {
                let legacy_end = DISPLAY_VSTART + commodore_denise_ocs::FB_HEIGHT as u16;
                if vpos < DISPLAY_VSTART || vpos >= legacy_end {
                    return None;
                }
            }

            if self.agnus.hblank_window_active(hpos_cck) {
                return None;
            }
            let (vstart, vstop, hstart, hstop) = if self.agnus.diwhigh_written() {
                let diwhigh = self.agnus.diwhigh();
                let vstart =
                    (((diwhigh & 0x000F) as u16) << 8) | ((self.agnus.diwstrt >> 8) & 0x00FF);
                let vstop = ((((diwhigh >> 8) & 0x000F) as u16) << 8)
                    | ((self.agnus.diwstop >> 8) & 0x00FF);
                let hstart = (((diwhigh >> 5) & 0x0001) << 8) | (self.agnus.diwstrt & 0x00FF);
                let hstop = (((diwhigh >> 13) & 0x0001) << 8) | (self.agnus.diwstop & 0x00FF);
                (vstart, vstop, hstart, hstop)
            } else {
                // HRM legacy ECS behavior: if DIWHIGH is not written, the
                // old OCS implicit H8/V8 scheme still applies.
                let vstart = (self.agnus.diwstrt >> 8) & 0x00FF; // V8=0
                let stop_low = (self.agnus.diwstop >> 8) & 0x00FF;
                let stop_v8 = ((!((stop_low >> 7) & 0x1)) & 0x1) << 8; // V8 != V7
                let vstop = stop_v8 | stop_low;
                let hstart = self.agnus.diwstrt & 0x00FF; // H8=0
                let hstop = 0x0100 | (self.agnus.diwstop & 0x00FF); // H8=1
                (vstart, vstop, hstart, hstop)
            };

            if self.agnus.vblank_window_active(vpos) {
                return None;
            }

            if vstart == vstop {
                return None;
            }

            let rel = if vstart < vstop {
                if vpos < vstart || vpos >= vstop {
                    return None;
                }
                vpos - vstart
            } else {
                let lines_per_frame = commodore_agnus_ocs::PAL_LINES_PER_FRAME;
                if vstart > lines_per_frame {
                    return None;
                }
                if !(vpos >= vstart || vpos < vstop) {
                    return None;
                }
                if vpos >= vstart {
                    vpos - vstart
                } else {
                    (lines_per_frame - vstart) + vpos
                }
            };

            if rel >= commodore_denise_ocs::FB_HEIGHT as u16 {
                return None;
            }
            // DIWSTRT/DIWSTOP define visibility, but the machine framebuffer
            // remains anchored to the fixed PAL display raster origin (as in
            // the OCS path). Using DIWSTRT as the framebuffer origin shifts
            // ECS/KS2.x content to the top-left and clips the lower layout.
            let physical_rel = vpos.wrapping_sub(DISPLAY_VSTART);
            if physical_rel >= commodore_denise_ocs::FB_HEIGHT as u16 {
                return None;
            }

            let h_active = if hstart == hstop {
                false
            } else if hstart < hstop {
                beam_x >= hstart && beam_x < hstop
            } else {
                beam_x >= hstart || beam_x < hstop
            };
            if !h_active {
                return None;
            }
            physical_rel
        } else {
            let rel = vpos.wrapping_sub(DISPLAY_VSTART);
            if rel >= commodore_denise_ocs::FB_HEIGHT as u16 {
                return None;
            }
            rel
        };
        let hires = (self.agnus.bplcon0 & 0x8000) != 0;
        // First bitplane pixel appears one CCK after the final BPL1 fetch in a
        // fetch group. In this simplified model the group width is 8 CCKs in
        // lowres and 4 CCKs in hires.
        let first_pixel_cck = self.agnus.ddfstrt.wrapping_add(if hires { 4 } else { 8 });
        let cck_offset = hpos_cck.wrapping_sub(first_pixel_cck);
        let mut fb_x = (u32::from(cck_offset) * 2) + u32::from(beam_x & 1);
        if hires {
            let phase = machine_experiment_hires_fb_x_offset();
            if phase != 0 {
                let shifted = fb_x as i32 + phase;
                if shifted < 0 || shifted >= commodore_denise_ocs::FB_WIDTH as i32 {
                    return None;
                }
                fb_x = shifted as u32;
            }
        }
        if fb_x >= commodore_denise_ocs::FB_WIDTH {
            return None;
        }
        Some((fb_x, u32::from(fb_y)))
    }

    fn beam_to_fb(&self, vpos: u16, hpos_cck: u16) -> Option<(u32, u32)> {
        self.beam_to_fb_beam_x(vpos, hpos_cck, hpos_cck.wrapping_mul(2))
    }

    fn playfield_window_active_beam_x(&self, vpos: u16, hpos_cck: u16, beam_x: u16) -> bool {
        if self.chipset != AmigaChipset::Ecs {
            return true;
        }
        if self.agnus.hblank_window_active(hpos_cck) || self.agnus.vblank_window_active(vpos) {
            return false;
        }
        let (vstart, vstop, hstart, hstop) = self.ecs_decoded_diw_window();
        if vstart == vstop || hstart == hstop {
            return false;
        }
        let v_active = if vstart < vstop {
            vpos >= vstart && vpos < vstop
        } else {
            vpos >= vstart || vpos < vstop
        };
        let h_active = if hstart < hstop {
            beam_x >= hstart && beam_x < hstop
        } else {
            beam_x >= hstart || beam_x < hstop
        };
        v_active && h_active
    }

    /// Render-path beam mapping in the fixed host framebuffer raster.
    ///
    /// Unlike `beam_to_fb_beam_x()`, this intentionally does not clip to
    /// `DIWSTRT/DIWSTOP`; DIW controls playfield visibility, but Denise still
    /// outputs border color outside the display window. ECS blank/sync/hard-stop
    /// scaffolding is still honored here.
    fn beam_to_fb_render_beam_x(&self, vpos: u16, hpos_cck: u16, beam_x: u16) -> Option<(u32, u32)> {
        let fb_y = if self.chipset == AmigaChipset::Ecs {
            let legacy_vertical_hard_stop_active = self.agnus.varbeamen_enabled()
                && !self.agnus.harddis_enabled()
                && !self.agnus.varvben_enabled();
            if legacy_vertical_hard_stop_active {
                let legacy_end = DISPLAY_VSTART + commodore_denise_ocs::FB_HEIGHT as u16;
                if vpos < DISPLAY_VSTART || vpos >= legacy_end {
                    return None;
                }
            }
            if self.agnus.hblank_window_active(hpos_cck) || self.agnus.vblank_window_active(vpos) {
                return None;
            }
            let physical_rel = vpos.wrapping_sub(DISPLAY_VSTART);
            if physical_rel >= commodore_denise_ocs::FB_HEIGHT as u16 {
                return None;
            }
            physical_rel
        } else {
            let rel = vpos.wrapping_sub(DISPLAY_VSTART);
            if rel >= commodore_denise_ocs::FB_HEIGHT as u16 {
                return None;
            }
            rel
        };

        let hires = (self.agnus.bplcon0 & 0x8000) != 0;
        let first_pixel_cck = self.agnus.ddfstrt.wrapping_add(if hires { 4 } else { 8 });
        let cck_offset = hpos_cck.wrapping_sub(first_pixel_cck);
        let mut fb_x = (u32::from(cck_offset) * 2) + u32::from(beam_x & 1);
        if hires {
            let phase = machine_experiment_hires_fb_x_offset();
            if phase != 0 {
                let shifted = fb_x as i32 + phase;
                if shifted < 0 || shifted >= commodore_denise_ocs::FB_WIDTH as i32 {
                    return None;
                }
                fb_x = shifted as u32;
            }
        }
        if fb_x >= commodore_denise_ocs::FB_WIDTH {
            return None;
        }
        Some((fb_x, u32::from(fb_y)))
    }

    fn ecs_decoded_diw_window(&self) -> (u16, u16, u16, u16) {
        if self.agnus.diwhigh_written() {
            let diwhigh = self.agnus.diwhigh();
            let vstart = (((diwhigh & 0x000F) as u16) << 8) | ((self.agnus.diwstrt >> 8) & 0x00FF);
            let vstop =
                ((((diwhigh >> 8) & 0x000F) as u16) << 8) | ((self.agnus.diwstop >> 8) & 0x00FF);
            let hstart = (((diwhigh >> 5) & 0x0001) << 8) | (self.agnus.diwstrt & 0x00FF);
            let hstop = (((diwhigh >> 13) & 0x0001) << 8) | (self.agnus.diwstop & 0x00FF);
            (vstart, vstop, hstart, hstop)
        } else {
            // HRM legacy ECS behavior: if DIWHIGH is not written, the old OCS
            // implicit H8/V8 scheme still applies.
            let vstart = (self.agnus.diwstrt >> 8) & 0x00FF; // V8=0
            let stop_low = (self.agnus.diwstop >> 8) & 0x00FF;
            let stop_v8 = ((!((stop_low >> 7) & 0x1)) & 0x1) << 8; // V8 != V7
            let vstop = stop_v8 | stop_low;
            let hstart = self.agnus.diwstrt & 0x00FF; // H8=0
            let hstop = 0x0100 | (self.agnus.diwstop & 0x00FF); // H8=1
            (vstart, vstop, hstart, hstop)
        }
    }

    fn ecs_display_h_span_cck_and_bounds(&self) -> Option<(u16, u16, u16)> {
        if self.chipset != AmigaChipset::Ecs {
            return None;
        }
        let (_vstart, _vstop, hstart, hstop) = self.ecs_decoded_diw_window();
        if hstart == hstop {
            return None;
        }
        let line_beam = commodore_agnus_ocs::PAL_CCKS_PER_LINE * 2;
        let span_beam = if hstart < hstop {
            hstop - hstart
        } else if hstart <= line_beam {
            (line_beam - hstart).wrapping_add(hstop)
        } else {
            return None;
        };
        let span_cck = span_beam.div_ceil(2).max(1);
        Some((span_cck, hstart, hstop))
    }

    /// Update the vertical bitplane DMA flip-flop at line start.
    ///
    /// Real Agnus uses a flip-flop: SET when beam reaches VSTART, CLEARED
    /// when beam reaches VSTOP.  This is edge-triggered — the flip-flop
    /// retains its state between edges.  A wrap-around window (VSTART >
    /// VSTOP) works naturally: the flip-flop gets set at VSTART, stays set
    /// through frame wrap, and gets cleared at VSTOP.
    ///
    /// During vblank, DMA is unconditionally disabled regardless of the
    /// flip-flop state.
    fn update_bpl_dma_vactive_flipflop(&mut self, vpos: u16) {
        if self.chipset == AmigaChipset::Ecs {
            let (vstart, vstop, _hstart, _hstop) = self.ecs_decoded_diw_window();
            // Edge-triggered: set on VSTART, clear on VSTOP.
            // If VSTART == VSTOP the window is degenerate — keep cleared.
            if vstart == vstop {
                self.bpl_dma_vactive_latch = false;
            } else if vpos == vstart {
                self.bpl_dma_vactive_latch = true;
            } else if vpos == vstop {
                self.bpl_dma_vactive_latch = false;
            }
            // Otherwise the latch retains its previous value.
        } else {
            // OCS: simple fixed window, no flip-flop needed.
            let rel = vpos.wrapping_sub(DISPLAY_VSTART);
            self.bpl_dma_vactive_latch = rel < commodore_denise_ocs::FB_HEIGHT as u16;
        }
    }

    fn bitplane_dma_vertical_active(&self, vpos: u16) -> bool {
        if self.chipset == AmigaChipset::Ecs {
            if self.agnus.vblank_window_active(vpos) {
                return false;
            }
            self.bpl_dma_vactive_latch
        } else {
            self.bpl_dma_vactive_latch
        }
    }

    fn maybe_capture_area_blit_shadow_compare(&mut self) {
        if self.blitter_shadow_compare_snapshot.is_some() {
            return;
        }
        let Some(limit) = machine_debug_compare_area_blits_limit() else {
            return;
        };
        if self.blitter_area_shadow_compare_started >= limit {
            return;
        }
        if !self.agnus.blitter_busy || (self.agnus.bltcon1 & 0x0001) != 0 {
            return;
        }
        if (self.agnus.bltcon0 & 0x0100) == 0 {
            return;
        }
        if !self.agnus.has_incremental_blitter_runtime() {
            return;
        }

        let seq = self.blitter_area_shadow_compare_started;
        self.blitter_area_shadow_compare_started += 1;
        self.blitter_shadow_compare_snapshot = Some(BlitShadowCompareSnapshot {
            mode: BlitShadowCompareMode::Area,
            seq,
            captured_master_clock: self.master_clock,
            captured_vpos: self.agnus.vpos,
            captured_hpos: self.agnus.hpos,
            agnus: self.agnus.clone(),
            memory: self.memory.clone(),
        });
    }

    fn maybe_capture_line_blit_shadow_compare(&mut self) {
        if self.blitter_shadow_compare_snapshot.is_some() {
            return;
        }
        let Some(limit) = machine_debug_compare_line_blits_limit() else {
            return;
        };
        if self.blitter_line_shadow_compare_started >= limit {
            return;
        }
        if !self.agnus.blitter_busy || (self.agnus.bltcon1 & 0x0001) == 0 {
            return;
        }
        if !self.agnus.has_incremental_blitter_runtime() {
            return;
        }

        let seq = self.blitter_line_shadow_compare_started;
        self.blitter_line_shadow_compare_started += 1;
        self.blitter_shadow_compare_snapshot = Some(BlitShadowCompareSnapshot {
            mode: BlitShadowCompareMode::Line,
            seq,
            captured_master_clock: self.master_clock,
            captured_vpos: self.agnus.vpos,
            captured_hpos: self.agnus.hpos,
            agnus: self.agnus.clone(),
            memory: self.memory.clone(),
        });
    }

    fn finish_area_blit_shadow_compare(&mut self) {
        let Some(snapshot) = self.blitter_shadow_compare_snapshot.take() else {
            return;
        };
        if snapshot.mode != BlitShadowCompareMode::Area {
            self.blitter_shadow_compare_snapshot = Some(snapshot);
            return;
        }
        let seq = snapshot.seq;

        let mut agnus_ref = snapshot.agnus.clone();
        let mut paula_ref = Paula8364::new();
        let mut memory_ref = snapshot.memory.clone();
        execute_blit(&mut agnus_ref, &mut paula_ref, &mut memory_ref);

        let d_words = area_blit_expected_d_words(&snapshot.agnus);
        let mut first_mismatch = None;
        for (idx, addr) in d_words.iter().copied().enumerate() {
            let expected = chip_word_at(&memory_ref.chip_ram, memory_ref.chip_ram_mask, addr);
            let actual = chip_word_at(&self.memory.chip_ram, self.memory.chip_ram_mask, addr);
            if expected != actual {
                first_mismatch = Some((idx, addr, expected, actual));
                break;
            }
        }

        let ptr_mismatch = (agnus_ref.blt_apt != self.agnus.blt_apt)
            || (agnus_ref.blt_bpt != self.agnus.blt_bpt)
            || (agnus_ref.blt_cpt != self.agnus.blt_cpt)
            || (agnus_ref.blt_dpt != self.agnus.blt_dpt);

        if let Some((idx, addr, expected, actual)) = first_mismatch {
            let (width_words, height) = blit_size_dims(&snapshot.agnus);
            let width_words_usize = width_words.max(1) as usize;
            let row = idx / width_words_usize;
            let col = idx % width_words_usize;
            eprintln!(
                "[blitcmp #{seq}] MISMATCH mode=area size={}x{} desc={} lf={:02X} row={} col={} addr={:06X} expected={:04X} actual={:04X} start_pc? mc={} beam={:03}/{:03} ptrs_ref=({:06X},{:06X},{:06X},{:06X}) ptrs_live=({:06X},{:06X},{:06X},{:06X})",
                width_words,
                height,
                (snapshot.agnus.bltcon1 & 0x0002) != 0,
                (snapshot.agnus.bltcon0 & 0x00FF) as u8,
                row,
                col,
                addr & 0xFFFFFF,
                expected,
                actual,
                snapshot.captured_master_clock,
                snapshot.captured_vpos,
                snapshot.captured_hpos,
                agnus_ref.blt_apt,
                agnus_ref.blt_bpt,
                agnus_ref.blt_cpt,
                agnus_ref.blt_dpt,
                self.agnus.blt_apt,
                self.agnus.blt_bpt,
                self.agnus.blt_cpt,
                self.agnus.blt_dpt,
            );
        } else if ptr_mismatch {
            let (width_words, height) = blit_size_dims(&snapshot.agnus);
            eprintln!(
                "[blitcmp #{seq}] PTR-MISMATCH mode=area size={}x{} desc={} lf={:02X} ptrs_ref=({:06X},{:06X},{:06X},{:06X}) ptrs_live=({:06X},{:06X},{:06X},{:06X})",
                width_words,
                height,
                (snapshot.agnus.bltcon1 & 0x0002) != 0,
                (snapshot.agnus.bltcon0 & 0x00FF) as u8,
                agnus_ref.blt_apt,
                agnus_ref.blt_bpt,
                agnus_ref.blt_cpt,
                agnus_ref.blt_dpt,
                self.agnus.blt_apt,
                self.agnus.blt_bpt,
                self.agnus.blt_cpt,
                self.agnus.blt_dpt,
            );
        } else if machine_debug_compare_area_blits_verbose() {
            let (width_words, height) = blit_size_dims(&snapshot.agnus);
            eprintln!(
                "[blitcmp #{}] ok size={}x{} desc={} lf={:02X}",
                snapshot.seq,
                width_words,
                height,
                (snapshot.agnus.bltcon1 & 0x0002) != 0,
                (snapshot.agnus.bltcon0 & 0x00FF) as u8
            );
        }
    }

    fn finish_line_blit_shadow_compare(&mut self) {
        let Some(snapshot) = self.blitter_shadow_compare_snapshot.take() else {
            return;
        };
        if snapshot.mode != BlitShadowCompareMode::Line {
            self.blitter_shadow_compare_snapshot = Some(snapshot);
            return;
        }
        let seq = snapshot.seq;

        let mut agnus_ref = snapshot.agnus.clone();
        let mut paula_ref = Paula8364::new();
        let mut memory_ref = snapshot.memory.clone();
        execute_blit(&mut agnus_ref, &mut paula_ref, &mut memory_ref);

        let d_words = line_blit_expected_d_words(&snapshot.agnus);
        let mut first_mismatch = None;
        for (step, addr) in d_words.iter().copied().enumerate() {
            let expected = chip_word_at(&memory_ref.chip_ram, memory_ref.chip_ram_mask, addr);
            let actual = chip_word_at(&self.memory.chip_ram, self.memory.chip_ram_mask, addr);
            if expected != actual {
                first_mismatch = Some((step, addr, expected, actual));
                break;
            }
        }

        let ptr_mismatch = (agnus_ref.blt_apt != self.agnus.blt_apt)
            || (agnus_ref.blt_bpt != self.agnus.blt_bpt)
            || (agnus_ref.blt_cpt != self.agnus.blt_cpt)
            || (agnus_ref.blt_dpt != self.agnus.blt_dpt)
            || (agnus_ref.blt_bdat != self.agnus.blt_bdat);

        let steps = line_blit_steps(&snapshot.agnus);
        if let Some((step, addr, expected, actual)) = first_mismatch {
            eprintln!(
                "[blitcmp #{seq}] MISMATCH mode=line steps={} desc={} lf={:02X} step={} addr={:06X} expected={:04X} actual={:04X} mc={} beam={:03}/{:03} ptrs_ref=({:06X},{:06X},{:06X},{:06X}) ptrs_live=({:06X},{:06X},{:06X},{:06X}) bdat_ref={:04X} bdat_live={:04X}",
                steps,
                (snapshot.agnus.bltcon1 & 0x0002) != 0,
                (snapshot.agnus.bltcon0 & 0x00FF) as u8,
                step,
                addr & 0xFFFFFF,
                expected,
                actual,
                snapshot.captured_master_clock,
                snapshot.captured_vpos,
                snapshot.captured_hpos,
                agnus_ref.blt_apt,
                agnus_ref.blt_bpt,
                agnus_ref.blt_cpt,
                agnus_ref.blt_dpt,
                self.agnus.blt_apt,
                self.agnus.blt_bpt,
                self.agnus.blt_cpt,
                self.agnus.blt_dpt,
                agnus_ref.blt_bdat,
                self.agnus.blt_bdat,
            );
        } else if ptr_mismatch {
            eprintln!(
                "[blitcmp #{seq}] PTR-MISMATCH mode=line steps={} desc={} lf={:02X} ptrs_ref=({:06X},{:06X},{:06X},{:06X}) ptrs_live=({:06X},{:06X},{:06X},{:06X}) bdat_ref={:04X} bdat_live={:04X}",
                steps,
                (snapshot.agnus.bltcon1 & 0x0002) != 0,
                (snapshot.agnus.bltcon0 & 0x00FF) as u8,
                agnus_ref.blt_apt,
                agnus_ref.blt_bpt,
                agnus_ref.blt_cpt,
                agnus_ref.blt_dpt,
                self.agnus.blt_apt,
                self.agnus.blt_bpt,
                self.agnus.blt_cpt,
                self.agnus.blt_dpt,
                agnus_ref.blt_bdat,
                self.agnus.blt_bdat,
            );
        } else if machine_debug_compare_line_blits_verbose() {
            eprintln!(
                "[blitcmp #{}] ok mode=line steps={} desc={} lf={:02X}",
                snapshot.seq,
                steps,
                (snapshot.agnus.bltcon1 & 0x0002) != 0,
                (snapshot.agnus.bltcon0 & 0x00FF) as u8
            );
        }
    }

    /// Report coarse ECS sync-window state at a specific beam position.
    ///
    /// On OCS, or before ECS sync-window behavior is enabled, both fields are
    /// `false`.
    #[must_use]
    pub fn beam_sync_state_at(&self, vpos: u16, hpos_cck: u16) -> BeamSyncState {
        if self.chipset != AmigaChipset::Ecs {
            return BeamSyncState {
                hsync: false,
                vsync: false,
            };
        }
        BeamSyncState {
            hsync: self.agnus.hsync_window_active(hpos_cck),
            vsync: self.agnus.vsync_window_active(vpos),
        }
    }

    fn beam_pin_state_from_components(
        &self,
        sync: BeamSyncState,
        hblank: bool,
        vblank: bool,
        composite_sync_active: bool,
    ) -> BeamPinState {
        if self.chipset != AmigaChipset::Ecs {
            return BeamPinState::default();
        }

        let hsync_high = if self.agnus.hsytrue_enabled() {
            sync.hsync
        } else {
            !sync.hsync
        };
        let vsync_high = if self.agnus.vsytrue_enabled() {
            sync.vsync
        } else {
            !sync.vsync
        };
        let csync_high = if self.agnus.csytrue_enabled() {
            composite_sync_active
        } else {
            !composite_sync_active
        };

        BeamPinState {
            hsync_high,
            vsync_high,
            csync_high,
            blank_active: self.agnus.blanken_enabled() && (hblank || vblank),
        }
    }

    fn beam_composite_sync_debug_from_components(
        &self,
        sync: BeamSyncState,
    ) -> BeamCompositeSyncDebug {
        if self.chipset != AmigaChipset::Ecs {
            return BeamCompositeSyncDebug::default();
        }

        // Conservative ECS Phase 3 model: `VARCSYEN` changes the modeled
        // source mode, but timing still reuses the H/V sync OR until dedicated
        // composite-sync timing/HCENTER behavior is implemented.
        let mode = if self.agnus.varcsyen_enabled() {
            BeamCompositeSyncMode::VariablePlaceholderHvOr
        } else {
            BeamCompositeSyncMode::HardwiredHvOr
        };

        BeamCompositeSyncDebug {
            active: sync.hsync || sync.vsync,
            redirected: self.agnus.cscben_enabled(),
            mode,
        }
    }

    /// Latched coarse ECS sync-window state for the current CCK.
    ///
    /// This value updates once per colour clock in [`Self::tick`], using the
    /// beam position sampled at the start of the CCK.
    #[must_use]
    pub const fn current_beam_sync_state(&self) -> BeamSyncState {
        self.beam_debug_snapshot.sync
    }

    /// Build a debug beam snapshot at an explicit beam position.
    #[must_use]
    pub fn beam_debug_snapshot_at(&self, vpos: u16, hpos_cck: u16) -> BeamDebugSnapshot {
        let (hblank, vblank) = if self.chipset == AmigaChipset::Ecs {
            (
                self.agnus.hblank_window_active(hpos_cck),
                self.agnus.vblank_window_active(vpos),
            )
        } else {
            (false, false)
        };
        let sync = self.beam_sync_state_at(vpos, hpos_cck);
        let composite_sync = self.beam_composite_sync_debug_from_components(sync);
        BeamDebugSnapshot {
            vpos,
            hpos_cck,
            sync,
            composite_sync,
            hblank,
            vblank,
            pins: self.beam_pin_state_from_components(sync, hblank, vblank, composite_sync.active),
            fb_coords: self.beam_to_fb(vpos, hpos_cck),
        }
    }

    /// Latched debug beam snapshot for the current CCK.
    ///
    /// This value updates once per colour clock in [`Self::tick`], using the
    /// beam position sampled at the start of the CCK.
    #[must_use]
    pub const fn current_beam_debug_snapshot(&self) -> BeamDebugSnapshot {
        self.beam_debug_snapshot
    }

    /// Latched coarse ECS beam pin state for the current CCK.
    #[must_use]
    pub const fn current_beam_pin_state(&self) -> BeamPinState {
        self.beam_debug_snapshot.pins
    }

    /// Latched beam-edge class changes for the current CCK.
    ///
    /// These flags compare the previous and current latched
    /// [`BeamDebugSnapshot`] values at each colour-clock boundary.
    #[must_use]
    pub const fn current_beam_edge_flags(&self) -> BeamEdgeFlags {
        self.beam_edge_flags
    }

    /// Latched Denise subpixel outputs for the current colour clock.
    #[must_use]
    pub const fn current_beam_pixel_outputs_debug(&self) -> BeamPixelOutputDebug {
        self.beam_pixel_outputs_debug
    }

    #[must_use]
    pub const fn blitter_progress_debug_stats(&self) -> BlitterProgressDebugStats {
        self.blitter_progress_debug
    }
}

fn machine_experiment_hires_fb_x_offset() -> i32 {
    static OFFSET: OnceLock<i32> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        std::env::var("AMIGA_EXPERIMENT_HIRES_FB_X_OFFSET")
            .ok()
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0)
    })
}

fn machine_experiment_sync_line_blitter() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("AMIGA_EXPERIMENT_SYNC_LINE_BLITTER").is_some())
}

fn machine_experiment_sync_area_blitter() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("AMIGA_EXPERIMENT_SYNC_AREA_BLITTER").is_some())
}

fn machine_experiment_drain_incremental_area_blitter() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("AMIGA_EXPERIMENT_DRAIN_INCREMENTAL_AREA_BLITTER").is_some())
}

fn machine_experiment_burst_incremental_area_blitter_ops() -> u32 {
    static OPS: OnceLock<u32> = OnceLock::new();
    *OPS.get_or_init(|| {
        std::env::var("AMIGA_EXPERIMENT_BURST_INCREMENTAL_AREA_BLITTER_OPS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
    })
}

fn machine_debug_compare_area_blits_limit() -> Option<usize> {
    static LIMIT: OnceLock<Option<usize>> = OnceLock::new();
    *LIMIT.get_or_init(|| {
        std::env::var("AMIGA_COMPARE_AREA_BLITS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&n| n > 0)
    })
}

fn machine_debug_compare_area_blits_verbose() -> bool {
    static VERBOSE: OnceLock<bool> = OnceLock::new();
    *VERBOSE.get_or_init(|| std::env::var_os("AMIGA_COMPARE_AREA_BLITS_VERBOSE").is_some())
}

fn machine_debug_compare_line_blits_limit() -> Option<usize> {
    static LIMIT: OnceLock<Option<usize>> = OnceLock::new();
    *LIMIT.get_or_init(|| {
        std::env::var("AMIGA_COMPARE_LINE_BLITS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&n| n > 0)
    })
}

fn machine_debug_compare_line_blits_verbose() -> bool {
    static VERBOSE: OnceLock<bool> = OnceLock::new();
    *VERBOSE.get_or_init(|| std::env::var_os("AMIGA_COMPARE_LINE_BLITS_VERBOSE").is_some())
}

pub struct AmigaBusWrapper<'a> {
    pub chipset: AmigaChipset,
    pub agnus: &'a mut Agnus,
    pub memory: &'a mut Memory,
    pub denise: &'a mut DeniseOcs,
    pub copper: &'a mut Copper,
    pub cia_a: &'a mut Cia8520,
    pub cia_b: &'a mut Cia8520,
    pub paula: &'a mut Paula8364,
    pub floppy: &'a mut AmigaFloppyDrive,
    pub keyboard: &'a mut AmigaKeyboard,
    // Pipeline state for delayed register writes (Agnus→Denise propagation).
    pub bplcon0_denise_pending: &'a mut Option<(u16, u8)>,
    pub ddfstrt_pending: &'a mut Option<(u16, u8)>,
    pub ddfstop_pending: &'a mut Option<(u16, u8)>,
    pub color_pending: &'a mut Vec<(usize, u16, u8)>,
}

impl<'a> M68kBus for AmigaBusWrapper<'a> {
    fn poll_ipl(&mut self) -> u8 {
        self.paula.compute_ipl()
    }
    fn poll_interrupt_ack(&mut self, level: u8) -> BusStatus {
        BusStatus::Ready(24 + level as u16)
    }
    fn reset(&mut self) {
        // RESET instruction asserts the hardware reset line for 124 CPU cycles.
        // This resets all peripherals to their power-on state.
        self.cia_a.reset();
        self.cia_b.reset();
        // After CIA-A reset, DDR-A = 0 (all inputs). On the A500, the /OVL
        // pin has a pull-up resistor, so with CIA-A not driving it, overlay
        // defaults to ON — ROM mapped at $0.
        self.memory.overlay = true;
        // Reset custom chip state
        self.paula.reset();
        self.agnus.dmacon = 0;
    }

    fn poll_cycle(
        &mut self,
        addr: u32,
        fc: FunctionCode,
        is_read: bool,
        is_word: bool,
        data: Option<u16>,
    ) -> BusStatus {
        // Amiga uses autovectors for all hardware interrupts.
        // The CPU issues an IACK bus cycle with FC=InterruptAck. On real
        // hardware the address bus carries the level in A1-A3, but the CPU
        // core uses a fixed address ($FFFFFF). Compute the pending level
        // from Paula's IPL state instead.
        if fc == FunctionCode::InterruptAck {
            let level = self.paula.compute_ipl() as u16;
            return BusStatus::Ready(24 + level);
        }

        let addr = addr & 0xFFFFFF;

        // CIA-A ($BFE001, odd bytes, accent on D0-D7)
        // CIA-A is accent wired to the low byte of the data bus.
        // It responds to odd byte accesses AND word accesses (both
        // assert /LDS). A word write to an even CIA-A address still
        // delivers data to CIA-A via D0-D7.
        if (addr & 0xFFF000) == 0xBFE000 {
            let reg = ((addr >> 8) & 0x0F) as u8;
            if is_read {
                if addr & 1 != 0 {
                    return BusStatus::Ready(u16::from(self.cia_a.read(reg)));
                }
                return BusStatus::Ready(0xFF00);
            } else {
                // CIA-A receives data from D0-D7 (low byte) on:
                //   - Byte writes to odd addresses (data in low byte)
                //   - Word writes to any address (both UDS/LDS active)
                let should_write = (addr & 1 != 0) || is_word;
                if should_write {
                    let val = data.unwrap_or(0) as u8; // low byte = D0-D7
                    self.cia_a.write(reg, val);
                    if reg == 0 || reg == 2 {
                        let out = self.cia_a.port_a_output();
                        self.memory.overlay = out & 0x01 != 0;
                    }
                    // CRA write with bit 6 set = SP output mode = keyboard handshake
                    if reg == 0x0E && val & 0x40 != 0 {
                        self.keyboard.handshake();
                    }
                }
                return BusStatus::Ready(0);
            }
        }

        // CIA-B ($BFD000, even bytes, accent on D8-D15)
        // CIA-B is wired to the high byte of the data bus (D8-D15).
        // On real hardware, byte writes to even addresses put the byte on D8-D15.
        // However, our CPU always places byte data in the low byte of the data word
        // (bits 0-7), regardless of address alignment. For word writes, D8-D15
        // contains the high byte as expected.
        if (addr & 0xFFF000) == 0xBFD000 {
            let reg = ((addr >> 8) & 0x0F) as u8;
            if is_read {
                if addr & 1 == 0 {
                    return BusStatus::Ready(u16::from(self.cia_b.read(reg)) << 8 | 0x00FF);
                }
                return BusStatus::Ready(0x00FF);
            } else {
                let should_write = (addr & 1 == 0) || is_word;
                if should_write {
                    let val = if is_word {
                        (data.unwrap_or(0) >> 8) as u8 // word: CIA-B data from D8-D15
                    } else {
                        data.unwrap_or(0) as u8 // byte: CPU puts value in low byte
                    };
                    self.cia_b.write(reg, val);
                    // PRB write: update floppy drive control signals
                    if reg == 0x01 {
                        let prb = self.cia_b.port_b_output();
                        // Active-low signals: asserted when bit is 0
                        let step = prb & 0x01 == 0; // PB0: /DSKSTEP
                        let dir_inward = prb & 0x02 == 0; // PB1: /DSKDIREC (0=inward)
                        let side_upper = prb & 0x04 == 0; // PB2: /DSKSIDE (0=upper/head 1)
                        let sel = prb & 0x08 == 0; // PB3: /DSKSEL0
                        let motor = prb & 0x80 == 0; // PB7: /DSKMOTOR
                        self.floppy
                            .update_control(step, dir_inward, side_upper, sel, motor);
                    }
                }
                return BusStatus::Ready(0);
            }
        }

        // Custom Registers ($DFF000)
        if (addr & 0xFFF000) == 0xDFF000 {
            let offset = (addr & 0x1FE) as u16;
            if !is_read {
                let val = if is_word {
                    data.unwrap_or(0)
                } else {
                    let byte = data.unwrap_or(0) as u8;
                    let lane_word = if addr & 1 == 0 {
                        u16::from(byte) << 8
                    } else {
                        u16::from(byte)
                    };
                    if let Some(current) = custom_register_byte_merge_latch(
                        self.chipset,
                        self.agnus,
                        self.denise,
                        self.paula,
                        offset,
                    ) {
                        if addr & 1 == 0 {
                            (current & 0x00FF) | lane_word
                        } else {
                            (current & 0xFF00) | lane_word
                        }
                    } else {
                        // Fallback for unsupported byte-write merge targets:
                        // preserve the correct bus lane, but treat the write as
                        // a full-word register write (legacy behavior).
                        lane_word
                    }
                };
                queue_pipelined_write(
                    self.bplcon0_denise_pending,
                    self.ddfstrt_pending,
                    self.ddfstop_pending,
                    self.color_pending,
                    offset,
                    val,
                );
                write_custom_register(
                    self.chipset,
                    self.agnus,
                    self.denise,
                    self.copper,
                    self.paula,
                    self.memory,
                    offset,
                    val,
                );
            } else {
                // Custom register read: get the 16-bit value, then for
                // byte reads extract the correct byte. On the 68000 bus,
                // even-address bytes come from D8-D15 (high byte) and
                // odd-address bytes from D0-D7 (low byte). The CPU's
                // ReadByte stores the value as-is, so we must place the
                // relevant byte in the position the CPU expects (low byte).
                let word = match offset {
                    // DMACONR: DMA control (active bits) + blitter busy/zero
                    0x002 => {
                        let busy = if self.agnus.blitter_busy { 0x4000 } else { 0 };
                        self.agnus.dmacon | busy
                    }
                    0x004 => {
                        // VPOSR: expose PAL Agnus ID bits and beam MSBs.
                        // LOF is currently not modeled, so bit 15 stays 0.
                        let agnus_id = match self.chipset {
                            AmigaChipset::Ocs => 0x00u16, // PAL OCS Agnus
                            AmigaChipset::Ecs => 0x20u16, // PAL ECS (HR) Agnus
                        };
                        let v8 = (self.agnus.vpos >> 8) & 1;
                        let v9 = (self.agnus.vpos >> 9) & 1;
                        let v10 = (self.agnus.vpos >> 10) & 1;
                        (agnus_id << 8) | (v10 << 2) | (v9 << 1) | v8
                    }
                    0x006 => {
                        // VHPOSR: V7..V0 in high byte, H8..H1 (CCK units) in low byte.
                        ((self.agnus.vpos & 0xFF) << 8) | (self.agnus.hpos & 0xFF)
                    }
                    0x008 => self.paula.dskdatr,
                    0x00A | 0x00C => 0,
                    0x00E => self.denise.read_clxdat(),
                    0x010 => self.paula.adkcon,
                    0x016 => 0xFF00,
                    0x018 => 0x39FF,
                    0x01A => self.paula.read_dskbytr(self.agnus.dmacon),
                    0x01C => self.paula.intena,
                    0x01E => self.paula.intreq,
                    0x05C if self.chipset == AmigaChipset::Ecs => self.agnus.bltsizv_ecs,
                    0x05E if self.chipset == AmigaChipset::Ecs => self.agnus.bltsizh_ecs,
                    0x0A0..=0x0DA => self.paula.read_audio_register(offset).unwrap_or(0),
                    0x106 if self.chipset == AmigaChipset::Ecs => self.denise.bplcon3,
                    0x1C0 if self.chipset == AmigaChipset::Ecs => self.agnus.htotal(),
                    0x1C2 if self.chipset == AmigaChipset::Ecs => self.agnus.hsstop(),
                    0x1C4 if self.chipset == AmigaChipset::Ecs => self.agnus.hbstrt(),
                    0x1C6 if self.chipset == AmigaChipset::Ecs => self.agnus.hbstop(),
                    0x1C8 if self.chipset == AmigaChipset::Ecs => self.agnus.vtotal(),
                    0x1CA if self.chipset == AmigaChipset::Ecs => self.agnus.vsstop(),
                    0x1CC if self.chipset == AmigaChipset::Ecs => self.agnus.vbstrt(),
                    0x1CE if self.chipset == AmigaChipset::Ecs => self.agnus.vbstop(),
                    0x1DC if self.chipset == AmigaChipset::Ecs => self.agnus.beamcon0(),
                    0x1DE if self.chipset == AmigaChipset::Ecs => self.agnus.hsstrt(),
                    0x1E0 if self.chipset == AmigaChipset::Ecs => self.agnus.vsstrt(),
                    0x1E4 if self.chipset == AmigaChipset::Ecs => self.agnus.diwhigh(),
                    0x07C => match self.chipset {
                        // Original Denise has no DENISEID register; many programs observe
                        // bus residue. Keep legacy all-ones behavior for OCS until bus
                        // residue is modeled.
                        AmigaChipset::Ocs => 0xFFFF,
                        // HRM Appendix C: Enhanced Denise (8373) returns $FC in low byte.
                        AmigaChipset::Ecs => 0x00FC,
                    },
                    _ => 0,
                };
                // For byte reads, extract the correct byte from the word.
                // 68000 bus: even addr → high byte (D8-D15), odd → low byte (D0-D7).
                // The CPU's ReadByte stores the value as u16 and uses the low byte,
                // so we place the relevant byte in bits 7-0.
                if !is_word {
                    let byte = if addr & 1 == 0 {
                        (word >> 8) as u8
                    } else {
                        word as u8
                    };
                    return BusStatus::Ready(u16::from(byte));
                }
                return BusStatus::Ready(word);
            }
            return BusStatus::Ready(0);
        }

        if addr < 0x200000 {
            let bus_plan = self.agnus.cck_bus_plan();
            if bus_plan.cpu_chip_bus_granted {
                if is_read {
                    let val = if is_word {
                        let hi = self.memory.read_byte(addr);
                        let lo = self.memory.read_byte(addr | 1);
                        (u16::from(hi) << 8) | u16::from(lo)
                    } else {
                        u16::from(self.memory.read_byte(addr))
                    };
                    BusStatus::Ready(val)
                } else {
                    let val = data.unwrap_or(0);
                    if is_word {
                        self.memory.write_byte(addr, (val >> 8) as u8);
                        self.memory.write_byte(addr | 1, val as u8);
                    } else {
                        self.memory.write_byte(addr, val as u8);
                    }
                    BusStatus::Ready(0)
                }
            } else {
                BusStatus::Wait
            }
        } else {
            if is_read {
                let val = if is_word {
                    let hi = self.memory.read_byte(addr);
                    let lo = self.memory.read_byte(addr | 1);
                    (u16::from(hi) << 8) | u16::from(lo)
                } else {
                    u16::from(self.memory.read_byte(addr))
                };
                BusStatus::Ready(val)
            } else {
                BusStatus::Ready(0)
            }
        }
    }
}

fn custom_register_byte_merge_latch(
    chipset: AmigaChipset,
    agnus: &Agnus,
    denise: &DeniseOcs,
    paula: &Paula8364,
    offset: u16,
) -> Option<u16> {
    match offset {
        // Display / DMA-visible latches commonly byte-written by ROMs/copper.
        0x08E => Some(agnus.diwstrt),
        0x090 => Some(agnus.diwstop),
        0x092 => Some(agnus.ddfstrt),
        0x094 => Some(agnus.ddfstop),
        0x098 => Some(denise.clxcon),
        0x09A => Some(paula.intena),
        0x09C => Some(paula.intreq),
        0x09E => Some(paula.adkcon),
        0x05C if chipset == AmigaChipset::Ecs => Some(agnus.bltsizv_ecs),
        0x05E if chipset == AmigaChipset::Ecs => Some(agnus.bltsizh_ecs),
        0x100 => Some(agnus.bplcon0),
        0x102 => Some(denise.bplcon1),
        0x104 => Some(denise.bplcon2),
        0x106 if chipset == AmigaChipset::Ecs => Some(denise.bplcon3),
        0x108 => Some(agnus.bpl1mod as u16),
        0x10A => Some(agnus.bpl2mod as u16),
        0x180..=0x1BE => {
            let idx = ((offset - 0x180) / 2) as usize;
            Some(denise.palette[idx])
        }
        0x1C0 if chipset == AmigaChipset::Ecs => Some(agnus.htotal()),
        0x1C2 if chipset == AmigaChipset::Ecs => Some(agnus.hsstop()),
        0x1C4 if chipset == AmigaChipset::Ecs => Some(agnus.hbstrt()),
        0x1C6 if chipset == AmigaChipset::Ecs => Some(agnus.hbstop()),
        0x1C8 if chipset == AmigaChipset::Ecs => Some(agnus.vtotal()),
        0x1CA if chipset == AmigaChipset::Ecs => Some(agnus.vsstop()),
        0x1CC if chipset == AmigaChipset::Ecs => Some(agnus.vbstrt()),
        0x1CE if chipset == AmigaChipset::Ecs => Some(agnus.vbstop()),
        0x1DC if chipset == AmigaChipset::Ecs => Some(agnus.beamcon0()),
        0x1DE if chipset == AmigaChipset::Ecs => Some(agnus.hsstrt()),
        0x1E0 if chipset == AmigaChipset::Ecs => Some(agnus.vsstrt()),
        0x1E4 if chipset == AmigaChipset::Ecs => Some(agnus.diwhigh()),
        _ => None,
    }
}

/// Queue writes to registers that propagate with a 2-CCK pipeline delay.
///
/// Returns `true` if the register was handled (caller should still call
/// `write_custom_register` for any non-pipelined side-effects on the same
/// offset — the free function's match arms for these offsets are no-ops).
fn queue_pipelined_write(
    bplcon0_denise_pending: &mut Option<(u16, u8)>,
    ddfstrt_pending: &mut Option<(u16, u8)>,
    ddfstop_pending: &mut Option<(u16, u8)>,
    color_pending: &mut Vec<(usize, u16, u8)>,
    offset: u16,
    val: u16,
) {
    match offset {
        // BPLCON0: Agnus sees the new value immediately; Denise sees it
        // after 2 CCK (the drain logic in tick() applies it).
        0x100 => {
            *bplcon0_denise_pending = Some((val, 2));
        }
        // DDFSTRT / DDFSTOP: shadow register with 2-CCK delay.
        0x092 => {
            *ddfstrt_pending = Some((val, 2));
        }
        0x094 => {
            *ddfstop_pending = Some((val, 2));
        }
        // Color palette: Denise sees the new color after 2 CCK.
        0x180..=0x1BE => {
            let idx = ((offset - 0x180) / 2) as usize;
            color_pending.push((idx, val, 2));
        }
        _ => {}
    }
}

/// Shared custom register write dispatch used by both CPU and copper paths.
fn write_custom_register(
    chipset: AmigaChipset,
    agnus: &mut Agnus,
    denise: &mut DeniseOcs,
    copper: &mut Copper,
    paula: &mut Paula8364,
    _memory: &mut Memory,
    offset: u16,
    val: u16,
) {
    match offset {
        // Blitter registers
        0x040 => agnus.bltcon0 = val,
        0x042 => agnus.bltcon1 = val,
        0x044 => agnus.blt_afwm = val,
        0x046 => agnus.blt_alwm = val,
        0x048 => agnus.blt_cpt = (agnus.blt_cpt & 0x0000FFFF) | (u32::from(val) << 16),
        0x04A => agnus.blt_cpt = (agnus.blt_cpt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x04C => agnus.blt_bpt = (agnus.blt_bpt & 0x0000FFFF) | (u32::from(val) << 16),
        0x04E => agnus.blt_bpt = (agnus.blt_bpt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x050 => agnus.blt_apt = (agnus.blt_apt & 0x0000FFFF) | (u32::from(val) << 16),
        0x052 => agnus.blt_apt = (agnus.blt_apt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x054 => agnus.blt_dpt = (agnus.blt_dpt & 0x0000FFFF) | (u32::from(val) << 16),
        0x056 => agnus.blt_dpt = (agnus.blt_dpt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x058 => {
            agnus.bltsize = val;
            agnus.start_blit();
        }
        0x05C if chipset == AmigaChipset::Ecs => {
            agnus.bltsizv_ecs = val;
        }
        0x05E if chipset == AmigaChipset::Ecs => {
            agnus.bltsizh_ecs = val;
            // ECS big-blit compatibility path: BLTSIZV then BLTSIZH starts the
            // blitter. Until the scheduler/executor use full ECS widths, fold
            // the low 10/6 bits into the legacy BLTSIZE register.
            let h = agnus.bltsizv_ecs & 0x7FFF;
            let w = agnus.bltsizh_ecs & 0x07FF;
            agnus.bltsize = ((h & 0x03FF) << 6) | (w & 0x003F);
            agnus.start_blit();
        }
        0x060 => agnus.blt_cmod = val as i16,
        0x062 => agnus.blt_bmod = val as i16,
        0x064 => agnus.blt_amod = val as i16,
        0x066 => agnus.blt_dmod = val as i16,
        0x070 => agnus.blt_cdat = val,
        0x072 => agnus.blt_bdat = val,
        0x074 => agnus.blt_adat = val,

        // Copper
        0x080 => copper.cop1lc = (copper.cop1lc & 0x0000FFFF) | (u32::from(val) << 16),
        0x082 => copper.cop1lc = (copper.cop1lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x084 => copper.cop2lc = (copper.cop2lc & 0x0000FFFF) | (u32::from(val) << 16),
        0x086 => copper.cop2lc = (copper.cop2lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x088 => copper.restart_cop1(),
        0x08A => copper.restart_cop2(),

        // Display
        0x08E => agnus.diwstrt = val,
        0x090 => agnus.diwstop = val,
        // DDFSTRT/DDFSTOP writes are pipelined (2-CCK delay) — handled
        // by queue_pipelined_write() at the call site.
        0x092 | 0x094 => {}


        // DMA control
        0x096 => {
            if val & 0x8000 != 0 {
                agnus.dmacon |= val & 0x7FFF;
            } else {
                agnus.dmacon &= !(val & 0x7FFF);
            }
        }
        0x098 => denise.clxcon = val,

        // Interrupts
        0x09A => paula.write_intena(val),
        0x09C => paula.write_intreq(val),

        // Audio/disk control
        0x09E => paula.write_adkcon(val),

        // Disk
        0x020 => agnus.dsk_pt = (agnus.dsk_pt & 0x0000FFFF) | (u32::from(val) << 16),
        0x022 => agnus.dsk_pt = (agnus.dsk_pt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x024 => paula.write_dsklen(val),
        0x026 => paula.write_dskdat(val),
        0x07E => paula.dsksync = val,

        // Serial (discard)
        0x030 | 0x032 => {}

        // Copper danger
        0x02E => copper.danger = val & 0x02 != 0,

        // ECS display/beam extensions (latch-only for now, gated off on OCS)
        0x1C0 if chipset == AmigaChipset::Ecs => agnus.write_htotal(val),
        0x1C2 if chipset == AmigaChipset::Ecs => agnus.write_hsstop(val),
        0x1C4 if chipset == AmigaChipset::Ecs => agnus.write_hbstrt(val),
        0x1C6 if chipset == AmigaChipset::Ecs => agnus.write_hbstop(val),
        0x1C8 if chipset == AmigaChipset::Ecs => agnus.write_vtotal(val),
        0x1CA if chipset == AmigaChipset::Ecs => agnus.write_vsstop(val),
        0x1CC if chipset == AmigaChipset::Ecs => agnus.write_vbstrt(val),
        0x1CE if chipset == AmigaChipset::Ecs => agnus.write_vbstop(val),
        0x1DC if chipset == AmigaChipset::Ecs => agnus.write_beamcon0(val),
        0x1DE if chipset == AmigaChipset::Ecs => agnus.write_hsstrt(val),
        0x1E0 if chipset == AmigaChipset::Ecs => agnus.write_vsstrt(val),
        0x1E4 if chipset == AmigaChipset::Ecs => agnus.write_diwhigh(val),

        // Bitplane control — Agnus sees BPLCON0 immediately; Denise
        // update is pipelined (2 CCK) via queue_pipelined_write().
        0x100 => {
            agnus.bplcon0 = val;
        }
        0x102 => denise.bplcon1 = val,
        0x104 => denise.bplcon2 = val,
        0x106 if chipset == AmigaChipset::Ecs => denise.bplcon3 = val,

        // Bitplane modulos
        0x108 => agnus.bpl1mod = val as i16,
        0x10A => agnus.bpl2mod = val as i16,

        // Bitplane pointers ($0E0-$0EE)
        0x0E0..=0x0EE => {
            let idx = ((offset - 0x0E0) / 4) as usize;
            if offset & 2 == 0 {
                agnus.bpl_pt[idx] = (agnus.bpl_pt[idx] & 0x0000FFFF) | (u32::from(val) << 16);
            } else {
                agnus.bpl_pt[idx] = (agnus.bpl_pt[idx] & 0xFFFF0000) | u32::from(val & 0xFFFE);
            }
        }

        // Sprite pointers ($120-$13E)
        0x120..=0x13E => {
            let idx = ((offset - 0x120) / 4) as usize;
            if idx < 8 {
                agnus.write_sprite_pointer_reg(idx, (offset & 2) == 0, val);
            }
        }

        // Sprite data ($140-$17E): 8 sprites x 4 regs (POS, CTL, DATA, DATB)
        0x140..=0x17E => {
            let sprite = ((offset - 0x140) / 8) as usize;
            let reg = ((offset - 0x140) % 8) / 2;
            if sprite < 8 {
                match reg {
                    0 => denise.write_sprite_pos(sprite, val),
                    1 => denise.write_sprite_ctl(sprite, val),
                    2 => denise.write_sprite_data(sprite, val),
                    3 => denise.write_sprite_datb(sprite, val),
                    _ => {}
                }
            }
        }

        // Color palette ($180-$1BE) — pipelined via queue_pipelined_write().
        0x180..=0x1BE => {}

        // Paula audio channels (AUD0-AUD3)
        0x0A0..=0x0DA => {
            let _ = paula.write_audio_register(offset, val);
        }

        _ => {}
    }
}

/// Execute one queued blitter DMA timing op against any incremental blitter
/// runtime currently active in Agnus (today: line mode only).
fn execute_incremental_blitter_op(
    agnus: &mut Agnus,
    memory: &mut Memory,
    op: BlitterDmaOp,
) -> bool {
    let chip_len = memory.chip_ram.len();
    let chip_ptr = memory.chip_ram.as_mut_ptr();
    agnus.execute_incremental_blitter_op(
        op,
        |addr| {
            let a = addr & 0x1FFFFE;
            if (a as usize + 1) < chip_len {
                // SAFETY: `chip_ptr` points to `memory.chip_ram` for this
                // function call, and bounds are checked before access.
                unsafe {
                    (u16::from(*chip_ptr.add(a as usize)) << 8)
                        | u16::from(*chip_ptr.add(a as usize + 1))
                }
            } else {
                0
            }
        },
        |addr, val| {
            let a = addr & 0x1FFFFE;
            if (a as usize + 1) < chip_len {
                // SAFETY: `chip_ptr` points to `memory.chip_ram` for this
                // function call, and bounds are checked before access.
                unsafe {
                    *chip_ptr.add(a as usize) = (val >> 8) as u8;
                    *chip_ptr.add(a as usize + 1) = val as u8;
                }
            }
        },
    )
}

fn blit_size_dims(agnus: &Agnus) -> (u32, u32) {
    let height = (agnus.bltsize >> 6) & 0x03FF;
    let width_words = agnus.bltsize & 0x003F;
    let height = if height == 0 { 1024 } else { height } as u32;
    let width_words = if width_words == 0 { 64 } else { width_words } as u32;
    (width_words, height)
}

fn area_blit_expected_d_words(agnus: &Agnus) -> Vec<u32> {
    let (width_words, height) = blit_size_dims(agnus);
    let mut d_words = Vec::with_capacity((width_words * height) as usize);
    let desc = (agnus.bltcon1 & 0x0002) != 0;
    let ptr_step: i32 = if desc { -2 } else { 2 };
    let mod_dir: i32 = if desc { -1 } else { 1 };
    let mut dpt = agnus.blt_dpt;
    for _ in 0..height {
        for _ in 0..width_words {
            d_words.push(dpt & 0x1FFFFE);
            dpt = (dpt as i32 + ptr_step) as u32;
        }
        dpt = (dpt as i32 + i32::from(agnus.blt_dmod) * mod_dir) as u32;
    }
    d_words
}

fn line_blit_steps(agnus: &Agnus) -> u32 {
    let length = ((agnus.bltsize >> 6) & 0x03FF) as u32;
    if length == 0 { 1024 } else { length }
}

fn line_blit_expected_d_words(agnus: &Agnus) -> Vec<u32> {
    let length = line_blit_steps(agnus);
    let ash = (agnus.bltcon0 >> 12) & 0xF;
    let sud = agnus.bltcon1 & 0x0010 != 0;
    let sul = agnus.bltcon1 & 0x0008 != 0;
    let aul = agnus.bltcon1 & 0x0004 != 0;
    let oct_code = ((sud as u8) << 2) | ((sul as u8) << 1) | (aul as u8);
    let octant = match oct_code {
        0b000 => 6,
        0b001 => 1,
        0b010 => 5,
        0b011 => 2,
        0b100 => 7,
        0b101 => 4,
        0b110 => 0,
        0b111 => 3,
        _ => unreachable!(),
    };
    let (major_is_y, x_neg, y_neg) = match octant {
        0 => (false, false, false),
        1 => (true, false, false),
        2 => (true, true, false),
        3 => (false, true, false),
        4 => (false, true, true),
        5 => (true, true, true),
        6 => (true, false, true),
        7 => (false, false, true),
        _ => unreachable!(),
    };

    let mut error = agnus.blt_apt as i16;
    let error_add = agnus.blt_bmod;
    let error_sub = agnus.blt_amod;
    let mut dpt = agnus.blt_dpt;
    let mut pixel_bit = ash;
    let row_mod = agnus.blt_cmod as i16;
    let mut out = Vec::with_capacity(length as usize);

    for _ in 0..length {
        out.push(dpt & 0x1FFFFE);

        let step_x = |dpt: &mut u32, pixel_bit: &mut u16| {
            if x_neg {
                *pixel_bit = pixel_bit.wrapping_sub(1) & 0xF;
                if *pixel_bit == 15 {
                    *dpt = dpt.wrapping_sub(2);
                }
            } else {
                *pixel_bit = (*pixel_bit + 1) & 0xF;
                if *pixel_bit == 0 {
                    *dpt = dpt.wrapping_add(2);
                }
            }
        };
        let step_y = |dpt: &mut u32| {
            if y_neg {
                *dpt = (*dpt as i32 + row_mod as i32) as u32;
            } else {
                *dpt = (*dpt as i32 - row_mod as i32) as u32;
            }
        };

        if error >= 0 {
            if major_is_y {
                step_y(&mut dpt);
                step_x(&mut dpt, &mut pixel_bit);
            } else {
                step_x(&mut dpt, &mut pixel_bit);
                step_y(&mut dpt);
            }
            error = error.wrapping_add(error_sub);
        } else {
            if major_is_y {
                step_y(&mut dpt);
            } else {
                step_x(&mut dpt, &mut pixel_bit);
            }
            error = error.wrapping_add(error_add);
        }
    }

    out
}

fn chip_word_at(chip_ram: &[u8], chip_ram_mask: u32, addr: u32) -> u16 {
    let base = (addr & chip_ram_mask & !1) as usize;
    let hi = chip_ram[base % chip_ram.len()];
    let lo = chip_ram[(base + 1) % chip_ram.len()];
    (u16::from(hi) << 8) | u16::from(lo)
}

/// Execute a blitter operation synchronously when the coarse scheduler matures.
///
/// On real hardware the blitter runs in DMA slots over many CCKs. We still run
/// the whole operation instantly here, but only after a coarse per-CCK delay so
/// `BLTBUSY` and nasty-mode arbitration persist across CCKs.
fn execute_blit(agnus: &mut Agnus, paula: &mut Paula8364, memory: &mut Memory) {
    let height = (agnus.bltsize >> 6) & 0x3FF;
    let width_words = agnus.bltsize & 0x3F;
    let height = if height == 0 { 1024 } else { height } as u32;
    let width_words = if width_words == 0 { 64 } else { width_words } as u32;

    // LINE mode (BLTCON1 bit 0): Bresenham line drawing.
    // Uses a completely different algorithm from area mode.
    if agnus.bltcon1 & 0x0001 != 0 {
        execute_blit_line(agnus, paula, memory);
        return;
    }

    let use_a = agnus.bltcon0 & 0x0800 != 0;
    let use_b = agnus.bltcon0 & 0x0400 != 0;
    let use_c = agnus.bltcon0 & 0x0200 != 0;
    let use_d = agnus.bltcon0 & 0x0100 != 0;
    let lf = agnus.bltcon0 as u8; // minterm function (low 8 bits)
    let a_shift = (agnus.bltcon0 >> 12) & 0xF;
    let b_shift = (agnus.bltcon1 >> 12) & 0xF;
    let desc = agnus.bltcon1 & 0x0002 != 0;
    let fci = (agnus.bltcon1 & 0x0004) != 0; // Fill Carry Input
    let ife = (agnus.bltcon1 & 0x0008) != 0; // Inclusive Fill Enable
    let efe = (agnus.bltcon1 & 0x0010) != 0; // Exclusive Fill Enable
    let fill_enabled = ife || efe;

    let mut apt = agnus.blt_apt;
    let mut bpt = agnus.blt_bpt;
    let mut cpt = agnus.blt_cpt;
    let mut dpt = agnus.blt_dpt;

    let read_word = |mem: &Memory, addr: u32| -> u16 {
        let hi = mem.read_chip_byte(addr);
        let lo = mem.read_chip_byte(addr.wrapping_add(1));
        (u16::from(hi) << 8) | u16::from(lo)
    };

    let write_word = |mem: &mut Memory, addr: u32, val: u16| {
        mem.write_byte(addr, (val >> 8) as u8);
        mem.write_byte(addr.wrapping_add(1), val as u8);
    };

    let ptr_step: i32 = if desc { -2 } else { 2 };

    // The barrel shifter carries bits across rows — a_prev/b_prev are only
    // zeroed once before the entire blit, NOT per-row (HRM p. 179-180).
    let mut a_prev: u16 = 0;
    let mut b_prev: u16 = 0;

    for _row in 0..height {
        let mut fill_carry: u16 = if fci { 1 } else { 0 };

        for col in 0..width_words {
            // Read source channels.
            // DMA reads update the holding registers (BLTADAT/BLTBDAT/BLTCDAT)
            // so subsequent blits with the channel disabled see the last DMA value.
            let a_raw = if use_a {
                let w = read_word(&*memory, apt);
                apt = (apt as i32 + ptr_step) as u32;
                agnus.blt_adat = w;
                w
            } else {
                agnus.blt_adat
            };
            let b_raw = if use_b {
                let w = read_word(&*memory, bpt);
                bpt = (bpt as i32 + ptr_step) as u32;
                agnus.blt_bdat = w;
                w
            } else {
                agnus.blt_bdat
            };
            let c_val = if use_c {
                let w = read_word(&*memory, cpt);
                cpt = (cpt as i32 + ptr_step) as u32;
                agnus.blt_cdat = w;
                w
            } else {
                agnus.blt_cdat
            };

            // Apply first/last word masks to A channel
            let mut a_masked = a_raw;
            if col == 0 {
                a_masked &= agnus.blt_afwm;
            }
            if col == width_words - 1 {
                a_masked &= agnus.blt_alwm;
            }

            // Barrel shift A: combine with previous word.
            // In DESC mode the shift direction reverses (left instead of
            // right), so the combined word order must be swapped.
            let a_combined = if desc {
                (u32::from(a_masked) << 16) | u32::from(a_prev)
            } else {
                (u32::from(a_prev) << 16) | u32::from(a_masked)
            };
            let a_shifted = if desc {
                (a_combined >> (16 - a_shift)) as u16
            } else {
                (a_combined >> a_shift) as u16
            };

            // Barrel shift B
            let b_combined = if desc {
                (u32::from(b_raw) << 16) | u32::from(b_prev)
            } else {
                (u32::from(b_prev) << 16) | u32::from(b_raw)
            };
            let b_shifted = if desc {
                (b_combined >> (16 - b_shift)) as u16
            } else {
                (b_combined >> b_shift) as u16
            };

            a_prev = a_masked;
            b_prev = b_raw;

            // Compute minterm for each bit
            let mut result: u16 = 0;
            for bit in 0..16 {
                let a_bit = (a_shifted >> bit) & 1;
                let b_bit = (b_shifted >> bit) & 1;
                let c_bit = (c_val >> bit) & 1;
                let index = (a_bit << 2) | (b_bit << 1) | c_bit;
                if (lf >> index) & 1 != 0 {
                    result |= 1 << bit;
                }
            }

            // Area fill: process bits right-to-left (bit 0 to bit 15),
            // toggling fill state at each '1' bit in the result.
            if fill_enabled {
                let mut filled: u16 = 0;
                for bit in 0..16u16 {
                    let d_bit = (result >> bit) & 1;
                    fill_carry ^= d_bit;
                    let out = if efe { fill_carry ^ d_bit } else { fill_carry };
                    filled |= out << bit;
                }
                result = filled;
            }

            // Write D channel
            if use_d {
                write_word(memory, dpt, result);
                dpt = (dpt as i32 + ptr_step) as u32;
            }
        }

        // Apply modulos at end of each row.
        // HRM p. 182/199: In descending mode the blitter subtracts modulos.
        let mod_dir: i32 = if desc { -1 } else { 1 };
        if use_a {
            apt = (apt as i32 + i32::from(agnus.blt_amod) * mod_dir) as u32;
        }
        if use_b {
            bpt = (bpt as i32 + i32::from(agnus.blt_bmod) * mod_dir) as u32;
        }
        if use_c {
            cpt = (cpt as i32 + i32::from(agnus.blt_cmod) * mod_dir) as u32;
        }
        if use_d {
            dpt = (dpt as i32 + i32::from(agnus.blt_dmod) * mod_dir) as u32;
        }
    }

    // Update pointer registers
    agnus.blt_apt = apt;
    agnus.blt_bpt = bpt;
    agnus.blt_cpt = cpt;
    agnus.blt_dpt = dpt;

    agnus.clear_blitter_scheduler();
    agnus.blitter_busy = false;
    paula.request_interrupt(6); // bit 6 = BLIT
}

/// Blitter LINE mode: Bresenham line drawing.
///
/// In line mode the blitter draws one pixel per "row" of BLTSIZE, stepping
/// through a Bresenham decision variable stored in BLTAPT.
///
/// Register usage in line mode:
///   BLTCON0 bits 15-12: Starting pixel position within word (ASH)
///   BLTCON0 bits 11-8:  Channel enables (must have A,C,D; B optional for texture)
///   BLTCON0 bits 7-0:   Minterm (usually $CA for normal, $0A for XOR)
///   BLTCON1 bits 15-12: Not used (BSH ignored in line mode)
///   BLTCON1 bit 4:      SUD (sign of dY step: 0=down, 1=up)
///   BLTCON1 bit 3:      SUL (sign of dX step: 0=right, 1=left)
///   BLTCON1 bit 2:      AUL (which axis is major: 0=X, 1=Y)
///   BLTCON1 bit 1:      SING (single-bit mode — only set pixel, don't clear)
///   BLTCON1 bit 0:      LINE (must be 1)
///   BLTAPT:  Initial Bresenham error accumulator (2*dy - dx, sign-extended)
///   BLTBDAT: Line texture pattern (usually $FFFF for solid)
///   BLTCPT/BLTDPT: Destination address (same value, points to first pixel's word)
///   BLTAMOD: 4*(dy - dx) — Bresenham decrement (when error >= 0)
///   BLTBMOD: 4*dy        — Bresenham increment (when error < 0)
///   BLTCMOD/BLTDMOD: Destination row modulo (bytes per row of the bitmap)
///   BLTAFWM: $8000 (not really used — the single-pixel mask comes from ASH)
///   BLTSIZE: height field = line length in pixels, width field = 2 (always)
fn execute_blit_line(agnus: &mut Agnus, paula: &mut Paula8364, memory: &mut Memory) {
    let length = ((agnus.bltsize >> 6) & 0x3FF) as u32;
    let length = if length == 0 { 1024 } else { length };

    let ash = (agnus.bltcon0 >> 12) & 0xF;
    let lf = agnus.bltcon0 as u8;
    let use_b = agnus.bltcon0 & 0x0400 != 0;
    // Octant control bits (HRM Appendix A):
    // SUD/SUL/AUL are not simple X/Y sign + major-axis flags. They form a
    // hardware-specific octant code and must be decoded via the HRM table.
    let sud = agnus.bltcon1 & 0x0010 != 0;
    let sul = agnus.bltcon1 & 0x0008 != 0;
    let aul = agnus.bltcon1 & 0x0004 != 0;
    let sing = agnus.bltcon1 & 0x0002 != 0;
    let oct_code = ((sud as u8) << 2) | ((sul as u8) << 1) | (aul as u8);
    // Code (SUD:SUL:AUL) -> octant index, per HRM table.
    let octant = match oct_code {
        0b000 => 6,
        0b001 => 1,
        0b010 => 5,
        0b011 => 2,
        0b100 => 7,
        0b101 => 4,
        0b110 => 0,
        0b111 => 3,
        _ => unreachable!(),
    };
    let (major_is_y, x_neg, y_neg) = match octant {
        0 => (false, false, false), // +X, +Y, X-major
        1 => (true, false, false),  // +X, +Y, Y-major
        2 => (true, true, false),   // -X, +Y, Y-major
        3 => (false, true, false),  // -X, +Y, X-major
        4 => (false, true, true),   // -X, -Y, X-major
        5 => (true, true, true),    // -X, -Y, Y-major
        6 => (true, false, true),   // +X, -Y, Y-major
        7 => (false, false, true),  // +X, -Y, X-major
        _ => unreachable!(),
    };

    let mut error = agnus.blt_apt as i16;
    let error_add = agnus.blt_bmod; // 4*dy (added when error < 0)
    let error_sub = agnus.blt_amod; // 4*(dy-dx) (added when error >= 0, typically negative)

    let mut cpt = agnus.blt_cpt;
    let mut dpt = agnus.blt_dpt;
    let mut pixel_bit = ash; // Current pixel position within word (0-15)

    let row_mod = agnus.blt_cmod as i16; // Destination row stride in bytes

    // Texture pattern from channel B (rotated each step)
    let mut texture = agnus.blt_bdat;
    let texture_enabled = use_b;

    fn read_word(mem: &Memory, addr: u32) -> u16 {
        let a = addr & 0x1FFFFE;
        if (a as usize + 1) < mem.chip_ram.len() {
            (u16::from(mem.chip_ram[a as usize]) << 8) | u16::from(mem.chip_ram[a as usize + 1])
        } else {
            0
        }
    }
    fn write_word(mem: &mut Memory, addr: u32, val: u16) {
        let a = addr & 0x1FFFFE;
        if (a as usize + 1) < mem.chip_ram.len() {
            mem.chip_ram[a as usize] = (val >> 8) as u8;
            mem.chip_ram[a as usize + 1] = val as u8;
        }
    }

    for _step in 0..length {
        // Build the single-pixel mask from the current bit position
        let pixel_mask: u16 = 0x8000 >> pixel_bit;

        // Channel A = the single pixel mask
        let a_val = pixel_mask;

        // Channel B = texture bit (MSB of rotating texture register)
        let b_val = if texture_enabled {
            if texture & 0x8000 != 0 {
                0xFFFF
            } else {
                0x0000
            }
        } else {
            0xFFFF
        };

        // Channel C = destination read-back (DMA updates the holding register)
        let c_val = read_word(&*memory, cpt);
        agnus.blt_cdat = c_val;

        // Compute minterm per bit
        let mut result: u16 = 0;
        for bit in 0..16u16 {
            let a_bit = (a_val >> bit) & 1;
            let b_bit = (b_val >> bit) & 1;
            let c_bit = (c_val >> bit) & 1;
            let index = (a_bit << 2) | (b_bit << 1) | c_bit;
            if (lf >> index) & 1 != 0 {
                result |= 1 << bit;
            }
        }

        // In SING mode, only modify the single pixel — keep other bits from C
        if sing {
            result = (result & pixel_mask) | (c_val & !pixel_mask);
        }

        // Write result to destination
        write_word(memory, dpt, result);

        // Rotate texture pattern (shift left by 1, wrap MSB to LSB)
        if texture_enabled {
            texture = texture.rotate_left(1);
        }

        // Bresenham step: decide whether to step on major axis only, or both axes.
        // Address updates are decoded from the HRM octant table above.
        let step_x = |cpt: &mut u32, dpt: &mut u32, pixel_bit: &mut u16| {
            if x_neg {
                *pixel_bit = pixel_bit.wrapping_sub(1) & 0xF;
                if *pixel_bit == 15 {
                    *cpt = cpt.wrapping_sub(2);
                    *dpt = dpt.wrapping_sub(2);
                }
            } else {
                *pixel_bit = (*pixel_bit + 1) & 0xF;
                if *pixel_bit == 0 {
                    *cpt = cpt.wrapping_add(2);
                    *dpt = dpt.wrapping_add(2);
                }
            }
        };
        let step_y = |cpt: &mut u32, dpt: &mut u32| {
            // In Amiga blitter line mode, BLTCPT/BLTDPT use a bottom-up raster
            // address convention (HRM/SPG examples compute the start row as
            // (rows - y - 1)). Therefore screen Y+ ("down") moves to a LOWER
            // memory address, and screen Y- ("up") moves to a HIGHER address.
            if y_neg {
                *cpt = (*cpt as i32 + row_mod as i32) as u32;
                *dpt = (*dpt as i32 + row_mod as i32) as u32;
            } else {
                *cpt = (*cpt as i32 - row_mod as i32) as u32;
                *dpt = (*dpt as i32 - row_mod as i32) as u32;
            }
        };

        if error >= 0 {
            // Step on BOTH axes (diagonal move)
            if major_is_y {
                step_y(&mut cpt, &mut dpt);
                step_x(&mut cpt, &mut dpt, &mut pixel_bit);
            } else {
                step_x(&mut cpt, &mut dpt, &mut pixel_bit);
                step_y(&mut cpt, &mut dpt);
            }
            error = error.wrapping_add(error_sub as i16);
        } else {
            // Step on major axis ONLY
            if major_is_y {
                step_y(&mut cpt, &mut dpt);
            } else {
                step_x(&mut cpt, &mut dpt, &mut pixel_bit);
            }
            error = error.wrapping_add(error_add as i16);
        }
    }

    // Update registers
    agnus.blt_apt = error as u16 as u32;
    agnus.blt_cpt = cpt;
    agnus.blt_dpt = dpt;
    agnus.blt_bdat = texture;

    agnus.clear_blitter_scheduler();
    agnus.blitter_busy = false;
    paula.request_interrupt(6);
}

#[cfg(test)]
mod tests {
    use super::{
        Amiga, AmigaBusWrapper, AmigaChipset, AmigaConfig, AmigaModel, BeamCompositeSyncDebug,
        BeamCompositeSyncMode, BeamDebugSnapshot, BeamEdgeFlags, BeamPinState, BeamSyncState,
        TICKS_PER_CCK,
    };
    use motorola_68000::bus::{BusStatus, FunctionCode, M68kBus};

    fn dummy_kickstart() -> Vec<u8> {
        // Minimal reset vectors (SSP=0, PC=0) are enough for constructor tests.
        vec![0; 8]
    }

    fn tick_one_cck(amiga: &mut Amiga) {
        for _ in 0..TICKS_PER_CCK {
            amiga.tick();
        }
    }

    fn read_custom_word_via_cpu_bus(amiga: &mut Amiga, offset: u16) -> u16 {
        let mut bus = AmigaBusWrapper {
            chipset: amiga.chipset,
            agnus: &mut amiga.agnus,
            memory: &mut amiga.memory,
            denise: &mut amiga.denise,
            copper: &mut amiga.copper,
            cia_a: &mut amiga.cia_a,
            cia_b: &mut amiga.cia_b,
            paula: &mut amiga.paula,
            floppy: &mut amiga.floppy,
            keyboard: &mut amiga.keyboard,
            bplcon0_denise_pending: &mut amiga.bplcon0_denise_pending,
            ddfstrt_pending: &mut amiga.ddfstrt_pending,
            ddfstop_pending: &mut amiga.ddfstop_pending,
            color_pending: &mut amiga.color_pending,
        };
        match M68kBus::poll_cycle(
            &mut bus,
            0x00DFF000 | u32::from(offset),
            FunctionCode::SupervisorData,
            true,
            true,
            None,
        ) {
            BusStatus::Ready(v) => v,
            other => panic!("expected ready custom register read, got {other:?}"),
        }
    }

    #[test]
    fn amiga_new_defaults_to_ocs_chipset() {
        let amiga = Amiga::new(dummy_kickstart());
        assert_eq!(amiga.model, AmigaModel::A500);
        assert_eq!(amiga.chipset, AmigaChipset::Ocs);
    }

    #[test]
    fn amiga_config_accepts_ecs_chipset_selection() {
        let amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });
        assert_eq!(amiga.chipset, AmigaChipset::Ecs);
    }

    #[test]
    fn amiga_config_a500plus_uses_one_meg_chip_ram() {
        let amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500Plus,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });
        assert_eq!(amiga.model, AmigaModel::A500Plus);
        assert_eq!(amiga.memory.chip_ram.len(), 1024 * 1024);
        assert_eq!(amiga.memory.chip_ram_mask, 0x0F_FFFF);
    }

    #[test]
    fn ocs_ignores_ecs_beamcon0_and_diwhigh_writes() {
        let mut amiga = Amiga::new(dummy_kickstart());
        amiga.write_custom_reg(0x1DC, 0x1234);
        amiga.write_custom_reg(0x1E4, 0x5678);
        assert_eq!(amiga.agnus.beamcon0(), 0);
        assert_eq!(amiga.agnus.diwhigh(), 0);
    }

    #[test]
    fn ecs_latches_beamcon0_and_diwhigh_writes() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });
        amiga.write_custom_reg(0x1DC, 0x0020);
        amiga.write_custom_reg(0x1E4, 0xA5A5);
        assert_eq!(amiga.agnus.beamcon0(), 0x0020);
        assert_eq!(amiga.agnus.diwhigh(), 0xA5A5);
    }

    #[test]
    fn ecs_beam_to_fb_uses_diwhigh_vertical_start_stop_bits() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        // ECS display window vertical range 0x100..0x120 and horizontal range
        // 0x110..0x150 (DIWHIGH supplies V8 and H8 bits).
        amiga.write_custom_reg(0x08E, 0x0010); // VSTART=$00, HSTART=$10
        amiga.write_custom_reg(0x090, 0x2050); // VSTOP =$20, HSTOP =$50
        amiga.write_custom_reg(0x1E4, 0x2121); // stop H8/V8 + start H8/V8
        amiga.agnus.ddfstrt = 100;

        // hpos 136 => beam_x 272 (=0x110), inside ECS horizontal window
        assert_eq!(amiga.beam_to_fb(256, 136), Some((56, 212)));
        // Last visible CCK before HSTOP (beam_x=334)
        assert_eq!(amiga.beam_to_fb(287, 167), Some((118, 243)));
        // Horizontal clipping via DIWHIGH.H8 (would otherwise be in framebuffer range)
        assert_eq!(amiga.beam_to_fb(256, 120), None); // beam_x=240 < HSTART
        assert_eq!(amiga.beam_to_fb(256, 180), None); // beam_x=360 >= HSTOP
        // Vertical clipping still applies
        assert_eq!(amiga.beam_to_fb(288, 8), None);
        assert_eq!(amiga.beam_to_fb(255, 8), None);
    }

    #[test]
    fn ecs_beam_to_fb_uses_legacy_diw_decode_until_diwhigh_is_written() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        // Classic PAL OCS-style display window values.
        amiga.write_custom_reg(0x08E, 0x2C81); // DIWSTRT
        amiga.write_custom_reg(0x090, 0x2CC1); // DIWSTOP
        amiga.agnus.ddfstrt = 0x38; // typical-ish lowres fetch start

        // With no DIWHIGH write, ECS should still decode these using the old
        // implicit H8/V8 rules, producing a visible 320x256 window.
        assert!(amiga.beam_to_fb(44, 65).is_some());
        assert!(amiga.beam_to_fb(299, 223).is_some());
        assert_eq!(amiga.beam_to_fb(300, 65), None);
    }

    #[test]
    fn ecs_latches_htotal_and_vtotal_writes() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        amiga.write_custom_reg(0x1C0, 0x0033);
        amiga.write_custom_reg(0x1C8, 0x0123);

        assert_eq!(amiga.agnus.htotal(), 0x0033);
        assert_eq!(amiga.agnus.vtotal(), 0x0123);
    }

    #[test]
    fn ocs_custom_reads_for_ecs_beam_registers_return_zero() {
        let mut amiga = Amiga::new(dummy_kickstart());
        amiga.write_custom_reg(0x1C0, 0x0033);
        amiga.write_custom_reg(0x1C2, 0x0044);
        amiga.write_custom_reg(0x1C4, 0x0011);
        amiga.write_custom_reg(0x1C6, 0x0022);
        amiga.write_custom_reg(0x1C8, 0x0123);
        amiga.write_custom_reg(0x1CA, 0x0234);
        amiga.write_custom_reg(0x1CC, 0x0044);
        amiga.write_custom_reg(0x1CE, 0x0055);
        amiga.write_custom_reg(0x1DC, 0x4567);
        amiga.write_custom_reg(0x1DE, 0x0066);
        amiga.write_custom_reg(0x1E0, 0x0177);
        amiga.write_custom_reg(0x1E4, 0x89AB);

        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1C0), 0);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1C2), 0);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1C4), 0);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1C6), 0);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1C8), 0);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1CA), 0);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1CC), 0);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1CE), 0);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1DC), 0);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1DE), 0);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1E0), 0);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1E4), 0);
    }

    #[test]
    fn ecs_custom_reads_return_latched_beam_registers() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });
        amiga.write_custom_reg(0x1C0, 0x0033);
        amiga.write_custom_reg(0x1C2, 0x0044);
        amiga.write_custom_reg(0x1C4, 0x0011);
        amiga.write_custom_reg(0x1C6, 0x0022);
        amiga.write_custom_reg(0x1C8, 0x0123);
        amiga.write_custom_reg(0x1CA, 0x0234);
        amiga.write_custom_reg(0x1CC, 0x0044);
        amiga.write_custom_reg(0x1CE, 0x0055);
        amiga.write_custom_reg(0x1DC, 0x4567);
        amiga.write_custom_reg(0x1DE, 0x0066);
        amiga.write_custom_reg(0x1E0, 0x0177);
        amiga.write_custom_reg(0x1E4, 0x89AB);

        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1C0), 0x0033);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1C2), 0x0044);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1C4), 0x0011);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1C6), 0x0022);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1C8), 0x0123);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1CA), 0x0234);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1CC), 0x0044);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1CE), 0x0055);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1DC), 0x4567);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1DE), 0x0066);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1E0), 0x0177);
        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x1E4), 0x89AB);
    }

    #[test]
    fn ecs_varbeamen_applies_programmed_beam_wrap_limits() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        amiga.write_custom_reg(0x1C0, 3); // HTOTAL highest hpos count
        amiga.write_custom_reg(0x1C8, 1); // VTOTAL highest line number
        amiga.write_custom_reg(0x1DC, commodore_agnus_ecs::BEAMCON0_VARBEAMEN);

        for expected_h in [1u16, 2, 3] {
            amiga.agnus.tick_cck();
            assert_eq!(amiga.agnus.hpos, expected_h);
            assert_eq!(amiga.agnus.vpos, 0);
        }
        amiga.agnus.tick_cck();
        assert_eq!(amiga.agnus.hpos, 0);
        assert_eq!(amiga.agnus.vpos, 1);

        for _ in 0..4 {
            amiga.agnus.tick_cck();
        }
        assert_eq!(amiga.agnus.hpos, 0);
        assert_eq!(amiga.agnus.vpos, 0);
    }

    #[test]
    fn ecs_varvben_blanks_beam_to_fb_inside_programmed_vertical_window() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        amiga.write_custom_reg(0x08E, 0x2C00); // VSTART=$2C, HSTART=$00
        amiga.write_custom_reg(0x090, 0x64FF); // VSTOP =$64, HSTOP =$FF
        amiga.agnus.ddfstrt = 0;

        assert!(amiga.beam_to_fb(60, 8).is_some());

        amiga.write_custom_reg(0x1CC, 55);
        amiga.write_custom_reg(0x1CE, 65);
        amiga.write_custom_reg(0x1DC, commodore_agnus_ecs::BEAMCON0_VARVBEN);

        assert_eq!(amiga.beam_to_fb(60, 8), None);
        assert!(amiga.beam_to_fb(70, 8).is_some());
    }

    #[test]
    fn ecs_harddis_hbstrt_hbstop_blank_beam_to_fb_inside_programmed_horizontal_window() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        amiga.write_custom_reg(0x08E, 0x2C00); // VSTART=$2C, HSTART=$00
        amiga.write_custom_reg(0x090, 0x64FF); // VSTOP =$64, HSTOP =$FF
        amiga.agnus.ddfstrt = 0;

        assert!(amiga.beam_to_fb(60, 10).is_some());

        amiga.write_custom_reg(0x1C4, 8); // HBSTRT
        amiga.write_custom_reg(0x1C6, 12); // HBSTOP
        amiga.write_custom_reg(0x1DC, commodore_agnus_ecs::BEAMCON0_HARDDIS);

        assert_eq!(amiga.beam_to_fb(60, 10), None);
        assert!(amiga.beam_to_fb(60, 20).is_some());
    }

    #[test]
    fn ecs_harddis_and_varvben_disable_coarse_legacy_vertical_hard_stop() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        // ECS display window vertical range 0x100..0x120 and horizontal range
        // 0x110..0x150 (DIWHIGH supplies V8 and H8 bits).
        amiga.write_custom_reg(0x08E, 0x0010); // VSTART=$00, HSTART=$10
        amiga.write_custom_reg(0x090, 0x2050); // VSTOP =$20, HSTOP =$50
        amiga.write_custom_reg(0x1E4, 0x2121); // stop H8/V8 + start H8/V8
        amiga.agnus.ddfstrt = 100;

        // DIWHIGH-only ECS path remains visible.
        assert_eq!(amiga.beam_to_fb(256, 136), Some((56, 212)));

        // With framebuffer Y anchored to the physical PAL display origin,
        // this sample line (v=0x100) remains visible even when VARBEAMEN is
        // enabled. The coarse legacy hard-stop clamp only affects positions
        // outside the physical 256-line framebuffer span.
        amiga.write_custom_reg(0x1DC, commodore_agnus_ecs::BEAMCON0_VARBEAMEN);
        assert_eq!(amiga.beam_to_fb(256, 136), Some((56, 212)));

        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_VARBEAMEN | commodore_agnus_ecs::BEAMCON0_HARDDIS,
        );
        assert_eq!(amiga.beam_to_fb(256, 136), Some((56, 212)));

        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_VARBEAMEN | commodore_agnus_ecs::BEAMCON0_VARVBEN,
        );
        assert_eq!(amiga.beam_to_fb(256, 136), Some((56, 212)));
    }

    #[test]
    fn ecs_beam_sync_state_reports_programmed_sync_windows() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        // Before enabling ECS variable sync windows, state is inactive.
        assert_eq!(
            amiga.beam_sync_state_at(105, 35),
            BeamSyncState {
                hsync: false,
                vsync: false
            }
        );

        amiga.write_custom_reg(0x1C2, 40); // HSSTOP
        amiga.write_custom_reg(0x1CA, 110); // VSSTOP
        amiga.write_custom_reg(0x1DE, 30); // HSSTRT
        amiga.write_custom_reg(0x1E0, 100); // VSSTRT
        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_VARHSYEN | commodore_agnus_ecs::BEAMCON0_VARVSYEN,
        );

        assert_eq!(
            amiga.beam_sync_state_at(105, 35),
            BeamSyncState {
                hsync: true,
                vsync: true
            }
        );
        assert_eq!(
            amiga.beam_sync_state_at(105, 20),
            BeamSyncState {
                hsync: false,
                vsync: true
            }
        );
        assert_eq!(
            amiga.beam_sync_state_at(95, 35),
            BeamSyncState {
                hsync: true,
                vsync: false
            }
        );
    }

    #[test]
    fn ecs_latched_sync_state_tracks_hsync_wrap_across_line_zero() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        amiga.write_custom_reg(0x1C2, 5); // HSSTOP
        amiga.write_custom_reg(0x1DE, 220); // HSSTRT
        amiga.write_custom_reg(0x1DC, commodore_agnus_ecs::BEAMCON0_VARHSYEN);

        amiga.agnus.vpos = 50;
        amiga.agnus.hpos = 219;
        tick_one_cck(&mut amiga);
        assert_eq!(
            amiga.current_beam_sync_state(),
            BeamSyncState {
                hsync: false,
                vsync: false
            }
        );

        tick_one_cck(&mut amiga); // hpos=220
        assert_eq!(
            amiga.current_beam_sync_state(),
            BeamSyncState {
                hsync: true,
                vsync: false
            }
        );

        amiga.agnus.hpos = commodore_agnus_ocs::PAL_CCKS_PER_LINE - 1;
        tick_one_cck(&mut amiga); // hpos=last -> still inside wrapped window
        assert_eq!(
            amiga.current_beam_sync_state(),
            BeamSyncState {
                hsync: true,
                vsync: false
            }
        );
        assert_eq!(amiga.agnus.hpos, 0);

        tick_one_cck(&mut amiga); // hpos=0 -> inside wrapped window
        assert_eq!(
            amiga.current_beam_sync_state(),
            BeamSyncState {
                hsync: true,
                vsync: false
            }
        );

        amiga.agnus.hpos = 5;
        tick_one_cck(&mut amiga); // stop boundary is exclusive
        assert_eq!(
            amiga.current_beam_sync_state(),
            BeamSyncState {
                hsync: false,
                vsync: false
            }
        );
    }

    #[test]
    fn ecs_latched_sync_state_tracks_vsync_wrap_across_frame_zero() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        let last_line = commodore_agnus_ocs::PAL_LINES_PER_FRAME - 1;
        let last_hpos = commodore_agnus_ocs::PAL_CCKS_PER_LINE - 1;

        amiga.write_custom_reg(0x1CA, 2); // VSSTOP
        amiga.write_custom_reg(0x1E0, last_line); // VSSTRT
        amiga.write_custom_reg(0x1DC, commodore_agnus_ecs::BEAMCON0_VARVSYEN);

        amiga.agnus.vpos = last_line;
        amiga.agnus.hpos = 0;
        tick_one_cck(&mut amiga);
        assert_eq!(
            amiga.current_beam_sync_state(),
            BeamSyncState {
                hsync: false,
                vsync: true
            }
        );

        amiga.agnus.hpos = last_hpos;
        tick_one_cck(&mut amiga); // line wrap to frame 0
        assert_eq!(
            amiga.current_beam_sync_state(),
            BeamSyncState {
                hsync: false,
                vsync: true
            }
        );
        assert_eq!(amiga.agnus.vpos, 0);
        assert_eq!(amiga.agnus.hpos, 0);

        tick_one_cck(&mut amiga); // vpos=0 still inside wrapped VSYNC window
        assert_eq!(
            amiga.current_beam_sync_state(),
            BeamSyncState {
                hsync: false,
                vsync: true
            }
        );

        amiga.agnus.vpos = 2;
        amiga.agnus.hpos = 0;
        tick_one_cck(&mut amiga); // stop boundary is exclusive
        assert_eq!(
            amiga.current_beam_sync_state(),
            BeamSyncState {
                hsync: false,
                vsync: false
            }
        );
    }

    #[test]
    fn ecs_beam_debug_snapshot_reports_sync_blanking_and_visibility() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        amiga.write_custom_reg(0x08E, 0x2C00); // DIWSTRT
        amiga.write_custom_reg(0x090, 0x90FF); // DIWSTOP (keep line 105 visible)
        amiga.agnus.ddfstrt = 0;

        amiga.write_custom_reg(0x1C4, 8); // HBSTRT
        amiga.write_custom_reg(0x1C6, 12); // HBSTOP
        amiga.write_custom_reg(0x1CC, 55); // VBSTRT
        amiga.write_custom_reg(0x1CE, 65); // VBSTOP
        amiga.write_custom_reg(0x1C2, 40); // HSSTOP
        amiga.write_custom_reg(0x1CA, 110); // VSSTOP
        amiga.write_custom_reg(0x1DE, 30); // HSSTRT
        amiga.write_custom_reg(0x1E0, 100); // VSSTRT
        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_HARDDIS
                | commodore_agnus_ecs::BEAMCON0_VARVBEN
                | commodore_agnus_ecs::BEAMCON0_VARHSYEN
                | commodore_agnus_ecs::BEAMCON0_VARVSYEN,
        );

        let active = amiga.beam_debug_snapshot_at(105, 35);
        assert_eq!(
            active,
            BeamDebugSnapshot {
                vpos: 105,
                hpos_cck: 35,
                sync: BeamSyncState {
                    hsync: true,
                    vsync: true
                },
                composite_sync: BeamCompositeSyncDebug {
                    active: true,
                    redirected: false,
                    mode: BeamCompositeSyncMode::HardwiredHvOr,
                },
                hblank: false,
                vblank: false,
                pins: BeamPinState {
                    hsync_high: false,
                    vsync_high: false,
                    csync_high: false,
                    blank_active: false,
                },
                fb_coords: Some((54, 61)),
            }
        );

        let blanked = amiga.beam_debug_snapshot_at(60, 10);
        assert_eq!(
            blanked,
            BeamDebugSnapshot {
                vpos: 60,
                hpos_cck: 10,
                sync: BeamSyncState {
                    hsync: false,
                    vsync: false
                },
                composite_sync: BeamCompositeSyncDebug {
                    active: false,
                    redirected: false,
                    mode: BeamCompositeSyncMode::HardwiredHvOr,
                },
                hblank: true,
                vblank: true,
                pins: BeamPinState {
                    hsync_high: true,
                    vsync_high: true,
                    csync_high: true,
                    blank_active: false,
                },
                fb_coords: None,
            }
        );
    }

    #[test]
    fn ecs_current_beam_debug_snapshot_uses_latched_sync_state() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        amiga.write_custom_reg(0x08E, 0x2C00); // DIWSTRT
        amiga.write_custom_reg(0x090, 0x90FF); // DIWSTOP (keep line 105 visible)
        amiga.agnus.ddfstrt = 0;
        amiga.write_custom_reg(0x1C2, 40); // HSSTOP
        amiga.write_custom_reg(0x1CA, 110); // VSSTOP
        amiga.write_custom_reg(0x1DE, 30); // HSSTRT
        amiga.write_custom_reg(0x1E0, 100); // VSSTRT
        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_VARHSYEN | commodore_agnus_ecs::BEAMCON0_VARVSYEN,
        );

        amiga.agnus.vpos = 105;
        amiga.agnus.hpos = 35;
        tick_one_cck(&mut amiga);

        let snapshot = amiga.current_beam_debug_snapshot();
        assert_eq!(snapshot.vpos, 105);
        assert_eq!(snapshot.hpos_cck, 35);
        assert_eq!(
            snapshot,
            BeamDebugSnapshot {
                vpos: 105,
                hpos_cck: 35,
                sync: BeamSyncState {
                    hsync: true,
                    vsync: true
                },
                composite_sync: BeamCompositeSyncDebug {
                    active: true,
                    redirected: false,
                    mode: BeamCompositeSyncMode::HardwiredHvOr,
                },
                hblank: false,
                vblank: false,
                pins: BeamPinState {
                    hsync_high: false,
                    vsync_high: false,
                    csync_high: false,
                    blank_active: false,
                },
                fb_coords: Some((54, 61)),
            }
        );
        assert_eq!(amiga.agnus.hpos, 36); // Beam advanced after the sampled CCK.
    }

    #[test]
    fn ecs_beam_debug_snapshot_reports_blanken_and_sync_polarity_pin_states() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        amiga.write_custom_reg(0x1C4, 8); // HBSTRT
        amiga.write_custom_reg(0x1C6, 12); // HBSTOP
        amiga.write_custom_reg(0x1CC, 55); // VBSTRT
        amiga.write_custom_reg(0x1CE, 65); // VBSTOP
        amiga.write_custom_reg(0x1C2, 40); // HSSTOP
        amiga.write_custom_reg(0x1CA, 110); // VSSTOP
        amiga.write_custom_reg(0x1DE, 30); // HSSTRT
        amiga.write_custom_reg(0x1E0, 100); // VSSTRT
        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_HARDDIS
                | commodore_agnus_ecs::BEAMCON0_VARVBEN
                | commodore_agnus_ecs::BEAMCON0_VARHSYEN
                | commodore_agnus_ecs::BEAMCON0_VARVSYEN,
        );

        let active_low_sync = amiga.beam_debug_snapshot_at(105, 35);
        assert_eq!(
            active_low_sync.composite_sync,
            BeamCompositeSyncDebug {
                active: true,
                redirected: false,
                mode: BeamCompositeSyncMode::HardwiredHvOr,
            }
        );
        assert_eq!(
            active_low_sync.pins,
            BeamPinState {
                hsync_high: false,
                vsync_high: false,
                csync_high: false,
                blank_active: false,
            }
        );

        let blank_no_redirect = amiga.beam_debug_snapshot_at(60, 10);
        assert_eq!(
            blank_no_redirect.composite_sync,
            BeamCompositeSyncDebug {
                active: false,
                redirected: false,
                mode: BeamCompositeSyncMode::HardwiredHvOr,
            }
        );
        assert_eq!(
            blank_no_redirect.pins,
            BeamPinState {
                hsync_high: true,
                vsync_high: true,
                csync_high: true,
                blank_active: false,
            }
        );

        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_HARDDIS
                | commodore_agnus_ecs::BEAMCON0_VARVBEN
                | commodore_agnus_ecs::BEAMCON0_VARHSYEN
                | commodore_agnus_ecs::BEAMCON0_VARVSYEN
                | commodore_agnus_ecs::BEAMCON0_BLANKEN
                | commodore_agnus_ecs::BEAMCON0_CSCBEN
                | commodore_agnus_ecs::BEAMCON0_CSYTRUE
                | commodore_agnus_ecs::BEAMCON0_VSYTRUE
                | commodore_agnus_ecs::BEAMCON0_HSYTRUE
                | commodore_agnus_ecs::BEAMCON0_VARCSYEN,
        );

        let true_polarity_sync = amiga.beam_debug_snapshot_at(105, 35);
        assert_eq!(
            true_polarity_sync.composite_sync,
            BeamCompositeSyncDebug {
                active: true,
                redirected: true,
                mode: BeamCompositeSyncMode::VariablePlaceholderHvOr,
            }
        );
        assert_eq!(
            true_polarity_sync.pins,
            BeamPinState {
                hsync_high: true,
                vsync_high: true,
                csync_high: true,
                blank_active: false,
            }
        );

        let blank_redirected = amiga.beam_debug_snapshot_at(60, 10);
        assert_eq!(
            blank_redirected.composite_sync,
            BeamCompositeSyncDebug {
                active: false,
                redirected: true,
                mode: BeamCompositeSyncMode::VariablePlaceholderHvOr,
            }
        );
        assert_eq!(
            blank_redirected.pins,
            BeamPinState {
                hsync_high: false,
                vsync_high: false,
                csync_high: false,
                blank_active: true,
            }
        );
    }

    #[test]
    fn latched_beam_edge_flags_report_class_changes_for_current_cck() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        amiga.write_custom_reg(0x08E, 0x2C00); // DIWSTRT
        amiga.write_custom_reg(0x090, 0x90FF); // DIWSTOP
        amiga.agnus.ddfstrt = 0;
        amiga.write_custom_reg(0x1C2, 40); // HSSTOP
        amiga.write_custom_reg(0x1DE, 30); // HSSTRT
        amiga.write_custom_reg(0x1DC, commodore_agnus_ecs::BEAMCON0_VARHSYEN);

        amiga.agnus.vpos = 100;
        amiga.agnus.hpos = 29;
        tick_one_cck(&mut amiga);
        assert_eq!(
            amiga.current_beam_edge_flags(),
            BeamEdgeFlags {
                hsync_changed: false,
                vsync_changed: false,
                hblank_changed: false,
                vblank_changed: false,
                visible_changed: true,
            }
        );

        tick_one_cck(&mut amiga); // hpos=30 enters HSYNC, remains visible
        assert_eq!(
            amiga.current_beam_edge_flags(),
            BeamEdgeFlags {
                hsync_changed: true,
                vsync_changed: false,
                hblank_changed: false,
                vblank_changed: false,
                visible_changed: false,
            }
        );
        assert!(amiga.current_beam_edge_flags().any());

        tick_one_cck(&mut amiga); // still inside HSYNC, no new edge
        assert_eq!(amiga.current_beam_edge_flags(), BeamEdgeFlags::default());
        assert!(!amiga.current_beam_edge_flags().any());
    }

    #[test]
    fn ecs_latched_beam_snapshot_visibility_tracks_coarse_hard_stop_and_harddis() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        // ECS display window vertical range 0x100..0x120 and horizontal range
        // 0x110..0x150 (DIWHIGH supplies V8 and H8 bits).
        amiga.write_custom_reg(0x08E, 0x0010); // VSTART=$00, HSTART=$10
        amiga.write_custom_reg(0x090, 0x2050); // VSTOP =$20, HSTOP =$50
        amiga.write_custom_reg(0x1E4, 0x2121); // stop H8/V8 + start H8/V8
        amiga.agnus.ddfstrt = 100;

        amiga.agnus.vpos = 256;
        amiga.agnus.hpos = 136;
        tick_one_cck(&mut amiga);
        assert_eq!(
            amiga.current_beam_debug_snapshot().fb_coords,
            Some((56, 212))
        );

        amiga.write_custom_reg(0x1DC, commodore_agnus_ecs::BEAMCON0_VARBEAMEN);
        amiga.agnus.vpos = 256;
        amiga.agnus.hpos = 136;
        tick_one_cck(&mut amiga);
        let hard_stopped = amiga.current_beam_debug_snapshot();
        assert_eq!(hard_stopped.vpos, 256);
        assert_eq!(hard_stopped.hpos_cck, 136);
        assert_eq!(hard_stopped.fb_coords, Some((56, 212)));

        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_VARBEAMEN | commodore_agnus_ecs::BEAMCON0_HARDDIS,
        );
        amiga.agnus.vpos = 256;
        amiga.agnus.hpos = 136;
        tick_one_cck(&mut amiga);
        assert_eq!(
            amiga.current_beam_debug_snapshot().fb_coords,
            Some((56, 212))
        );
    }

    #[test]
    fn ecs_current_beam_pin_state_uses_latched_snapshot_pins() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        amiga.write_custom_reg(0x1C4, 8); // HBSTRT
        amiga.write_custom_reg(0x1C6, 12); // HBSTOP
        amiga.write_custom_reg(0x1CC, 55); // VBSTRT
        amiga.write_custom_reg(0x1CE, 65); // VBSTOP
        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_HARDDIS
                | commodore_agnus_ecs::BEAMCON0_VARVBEN
                | commodore_agnus_ecs::BEAMCON0_BLANKEN,
        );

        amiga.agnus.vpos = 60;
        amiga.agnus.hpos = 10;
        tick_one_cck(&mut amiga);

        assert_eq!(
            amiga.current_beam_pin_state(),
            BeamPinState {
                hsync_high: true,
                vsync_high: true,
                csync_high: true,
                blank_active: true,
            }
        );
    }

    #[test]
    fn ecs_hsync_rising_edge_drives_cia_b_tod_pulse() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        amiga.write_custom_reg(0x1C2, 40); // HSSTOP
        amiga.write_custom_reg(0x1DE, 30); // HSSTRT
        amiga.write_custom_reg(0x1DC, commodore_agnus_ecs::BEAMCON0_VARHSYEN);

        amiga.agnus.vpos = 50;
        amiga.agnus.hpos = 29;
        tick_one_cck(&mut amiga);
        assert_eq!(amiga.cia_b.tod_counter(), 0);

        tick_one_cck(&mut amiga); // sample hpos=30 => HSYNC rising edge
        assert_eq!(amiga.cia_b.tod_counter(), 1);

        tick_one_cck(&mut amiga); // still inside sync window => no extra pulse
        assert_eq!(amiga.cia_b.tod_counter(), 1);
    }

    #[test]
    fn ecs_vsync_rising_edge_drives_cia_a_tod_pulse() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        amiga.write_custom_reg(0x1CA, 110); // VSSTOP
        amiga.write_custom_reg(0x1E0, 100); // VSSTRT
        amiga.write_custom_reg(0x1DC, commodore_agnus_ecs::BEAMCON0_VARVSYEN);

        amiga.agnus.vpos = 99;
        amiga.agnus.hpos = 0;
        tick_one_cck(&mut amiga);
        assert_eq!(amiga.cia_a.tod_counter(), 0);

        amiga.agnus.vpos = 100;
        amiga.agnus.hpos = 0;
        tick_one_cck(&mut amiga); // sample vpos=100 => VSYNC rising edge
        assert_eq!(amiga.cia_a.tod_counter(), 1);

        amiga.agnus.vpos = 101;
        amiga.agnus.hpos = 0;
        tick_one_cck(&mut amiga); // still inside sync window => no extra pulse
        assert_eq!(amiga.cia_a.tod_counter(), 1);
    }

    #[test]
    fn ecs_sync_tod_pulses_follow_wrapped_sync_windows_without_double_pulsing() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            kickstart: dummy_kickstart(),
        });

        let last_line = commodore_agnus_ocs::PAL_LINES_PER_FRAME - 1;

        amiga.write_custom_reg(0x1C2, 5); // HSSTOP (wrap)
        amiga.write_custom_reg(0x1DE, 220); // HSSTRT (wrap)
        amiga.write_custom_reg(0x1CA, 2); // VSSTOP (wrap)
        amiga.write_custom_reg(0x1E0, last_line); // VSSTRT (wrap)
        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_VARHSYEN | commodore_agnus_ecs::BEAMCON0_VARVSYEN,
        );

        // HSYNC wrap: pulse on entry at 220, no pulse while continuing through 0.
        amiga.agnus.vpos = 50;
        amiga.agnus.hpos = 219;
        tick_one_cck(&mut amiga);
        assert_eq!(amiga.cia_b.tod_counter(), 0);

        tick_one_cck(&mut amiga); // 220 => rising edge
        assert_eq!(amiga.cia_b.tod_counter(), 1);

        amiga.agnus.hpos = 0;
        tick_one_cck(&mut amiga); // wrapped segment continuation
        assert_eq!(amiga.cia_b.tod_counter(), 1);

        amiga.agnus.hpos = 5;
        tick_one_cck(&mut amiga); // leave window
        assert_eq!(amiga.cia_b.tod_counter(), 1);

        amiga.agnus.hpos = 220;
        tick_one_cck(&mut amiga); // re-enter => another rising edge
        assert_eq!(amiga.cia_b.tod_counter(), 2);

        // VSYNC wrap: pulse on entry at last_line, no pulse while continuing at line 0.
        amiga.agnus.vpos = last_line - 1;
        amiga.agnus.hpos = 0;
        tick_one_cck(&mut amiga);
        let before_vsync = amiga.cia_a.tod_counter();

        amiga.agnus.vpos = last_line;
        amiga.agnus.hpos = 0;
        tick_one_cck(&mut amiga); // rising edge
        assert_eq!(amiga.cia_a.tod_counter(), before_vsync + 1);

        amiga.agnus.vpos = 0;
        amiga.agnus.hpos = 0;
        tick_one_cck(&mut amiga); // wrapped segment continuation
        assert_eq!(amiga.cia_a.tod_counter(), before_vsync + 1);
    }

    #[test]
    fn ocs_cia_tod_pulses_still_follow_frame_and_line_wrap_points() {
        let mut amiga = Amiga::new(dummy_kickstart());

        amiga.agnus.vpos = 10;
        amiga.agnus.hpos = 1;
        tick_one_cck(&mut amiga);
        assert_eq!(amiga.cia_a.tod_counter(), 0);
        assert_eq!(amiga.cia_b.tod_counter(), 0);

        amiga.agnus.vpos = 10;
        amiga.agnus.hpos = 0;
        tick_one_cck(&mut amiga); // HSYNC pulse on line start
        assert_eq!(amiga.cia_a.tod_counter(), 0);
        assert_eq!(amiga.cia_b.tod_counter(), 1);

        amiga.agnus.vpos = 0;
        amiga.agnus.hpos = 0;
        tick_one_cck(&mut amiga); // frame wrap pulses both TOD inputs
        assert_eq!(amiga.cia_a.tod_counter(), 1);
        assert_eq!(amiga.cia_b.tod_counter(), 2);
    }
}
