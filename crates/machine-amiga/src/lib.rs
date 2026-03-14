//! The "Rock" - A Cycle-Strict Amiga Emulator.
//!
//! Foundation: Crystal-accuracy.
//! Bus Model: Reactive (Request/Acknowledge), not Predictive.
//! CPU Model: Ticks every 4 crystal cycles, polls bus until DTACK.

pub mod bus;
pub mod config;
#[cfg(feature = "native")]
pub mod mcp;
pub mod memory;

use crate::memory::Memory;
use commodore_agnus_aga::AgnusAga as Agnus;
use commodore_agnus_ocs::{BlitterDmaOp, Copper, SlotOwner};
use commodore_denise_aga::DeniseAga as DeniseOcs;
use commodore_buster::Buster;
use commodore_dmac_390537::Dmac390537;
use commodore_super_buster::SuperBuster;
use commodore_fat_gary::FatGary;
use commodore_gary::Gary;
use commodore_gayle::Gayle;
use commodore_paula_8364::Paula8364;
use commodore_ramsey::Ramsey;
use drive_amiga_floppy::AmigaFloppyDrive;
use format_adf::Adf;
use mos_cia_8520::Cia8520;
use motorola_68000::cpu::Cpu68000;
use motorola_68000::model::CpuModel;
use peripheral_amiga_keyboard::AmigaKeyboard;

// Re-export chip crates so tests and downstream users can access types.
pub use crate::config::{
    AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion, NTSC_RASTER_FB_HEIGHT,
    PAL_RASTER_FB_HEIGHT, RASTER_FB_WIDTH,
};
pub use commodore_agnus_aga;
pub use commodore_agnus_ecs;
pub use commodore_agnus_ocs;
pub use commodore_denise_aga;
pub use commodore_denise_ecs;
pub use commodore_denise_ocs;
pub use commodore_fat_gary;
pub use commodore_gary;
pub use commodore_gayle;
pub use commodore_paula_8364;
pub use commodore_ramsey;
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

/// CPU clock mode. Models that derive their clock from the system
/// crystal use `CrystalDerived`; models with an independent CPU
/// oscillator (A3000, A4000) use `Independent`.
#[derive(Debug, Clone)]
enum CpuClockMode {
    /// CPU clock = master_clock × (4 / divisor). Exact integer ratio.
    CrystalDerived { divisor: u64 },
    /// Independent oscillator. Bresenham accumulator ticks the CPU
    /// at `freq_hz` regardless of the video crystal.
    Independent {
        freq_hz: u64,
        phase: u64,
        clock: u64,
    },
}

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

/// Visual indicator state for the status bar (power LED, drive LED).
#[derive(Debug, Clone, Copy)]
pub struct IndicatorState {
    /// Power LED on the case. Active-low: CIA-A PRA bit 1 = 0 means on.
    pub power_led_on: bool,
    /// Drive motor is enabled (CIAB PRB SEL+MTR).
    pub drive_motor_on: bool,
    /// Disk DMA transfer is in progress.
    pub drive_dma_active: bool,
}

/// Plays recorded floppy drive samples for step clicks and motor hum.
/// Added to the audio output *after* the Paula low-pass filter so
/// mechanical transients aren't dulled.
///
/// Samples extracted from "Floppy disk drive" recordings by MrAuralization
/// on Freesound.org, licensed CC BY 4.0. See `sounds/ATTRIBUTION.md`.
pub struct DriveSoundGenerator {
    // Click sample: mono i16 at 48 kHz, ~150 ms
    click_pos: usize,
    click_playing: bool,

    // Motor loop: mono i16 at 48 kHz, ~300 ms, crossfaded for seamless loop
    motor_pos: usize,
    motor_envelope: f32,
    motor_target: f32,

    // Event tracking
    prev_step_counter: u32,

    pub enabled: bool,
}

/// Embedded raw PCM samples (mono i16 little-endian, 48 kHz).
mod drive_samples {
    static CLICK_RAW: &[u8] = include_bytes!("sounds/drive_click.raw");
    static MOTOR_RAW: &[u8] = include_bytes!("sounds/drive_motor.raw");

    /// Decode i16le bytes to f32 sample, bounds-checked.
    fn sample_at(data: &[u8], index: usize) -> f32 {
        let byte_pos = index * 2;
        if byte_pos + 1 >= data.len() {
            return 0.0;
        }
        let val = i16::from_le_bytes([data[byte_pos], data[byte_pos + 1]]);
        val as f32 / 32768.0
    }

    pub fn click_len() -> usize {
        CLICK_RAW.len() / 2
    }
    pub fn click_sample(index: usize) -> f32 {
        sample_at(CLICK_RAW, index)
    }
    pub fn motor_len() -> usize {
        MOTOR_RAW.len() / 2
    }
    pub fn motor_sample(index: usize) -> f32 {
        sample_at(MOTOR_RAW, index)
    }
}

impl DriveSoundGenerator {
    fn new(_sample_rate: u32) -> Self {
        Self {
            click_pos: 0,
            click_playing: false,
            motor_pos: 0,
            motor_envelope: 0.0,
            motor_target: 0.0,
            prev_step_counter: 0,
            enabled: true,
        }
    }

    /// Read drive state and fire sound events. Call once per audio sample.
    fn update_state(&mut self, motor_spinning: bool, step_counter: u32) {
        self.motor_target = if motor_spinning { 1.0 } else { 0.0 };

        if step_counter != self.prev_step_counter {
            self.prev_step_counter = step_counter;
            // Restart click sample from the beginning
            self.click_pos = 0;
            self.click_playing = true;
        }
    }

    /// Generate one mono sample. Returns 0.0 when disabled.
    fn generate_sample(&mut self) -> f32 {
        if !self.enabled {
            return 0.0;
        }

        let mut out = 0.0f32;

        // Motor hum: loop the recorded sample with envelope ramp (~50 ms
        // at 48 kHz = 2400 samples for smooth on/off).
        let ramp_rate = 1.0 / 2400.0;
        if self.motor_envelope < self.motor_target {
            self.motor_envelope = (self.motor_envelope + ramp_rate).min(self.motor_target);
        } else if self.motor_envelope > self.motor_target {
            self.motor_envelope = (self.motor_envelope - ramp_rate).max(self.motor_target);
        }
        if self.motor_envelope > 0.001 {
            let motor_len = drive_samples::motor_len();
            if motor_len > 0 {
                out +=
                    drive_samples::motor_sample(self.motor_pos % motor_len) * self.motor_envelope;
                self.motor_pos += 1;
                if self.motor_pos >= motor_len {
                    self.motor_pos = 0;
                }
            }
        }

        // Step click: play once from start to end, then stop.
        if self.click_playing {
            let click_len = drive_samples::click_len();
            if self.click_pos < click_len {
                out += drive_samples::click_sample(self.click_pos);
                self.click_pos += 1;
            } else {
                self.click_playing = false;
            }
        }

        out
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
    /// ECS variable composite sync enabled — uses real HSYNC XOR VSYNC.
    VariableXorSync,
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
#[cfg_attr(feature = "native", derive(serde::Serialize))]
pub enum BlitterInterruptSource {
    SchedulerIncremental,
    AreaCore,
    LineCore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "native", derive(serde::Serialize))]
pub struct BlitterIrqDebugEvent {
    pub tick: u64,
    pub source: BlitterInterruptSource,
    pub pc: u32,
    pub instr_start_pc: u32,
    pub ir: u16,
    pub sr: u16,
    pub vpos: u16,
    pub hpos: u16,
    pub dmacon: u16,
    pub intena: u16,
    pub intreq_before: u16,
    pub intreq_after: u16,
    pub blitter_busy: bool,
    pub blitter_ccks_remaining: u32,
    pub bltcon0: u16,
    pub bltcon1: u16,
    pub bltsize: u16,
    pub bltsizv_ecs: u16,
    pub bltsizh_ecs: u16,
    pub blt_apt: u32,
    pub blt_bpt: u32,
    pub blt_cpt: u32,
    pub blt_dpt: u32,
    pub blt_amod: i16,
    pub blt_bmod: i16,
    pub blt_cmod: i16,
    pub blt_dmod: i16,
    pub blt_adat: u16,
    pub blt_bdat: u16,
    pub blt_cdat: u16,
    pub blt_afwm: u16,
    pub blt_alwm: u16,
}

const MAX_BLITTER_IRQ_DEBUG_EVENTS: usize = 32;

#[derive(Debug, Default, Clone)]
struct BlitterIrqDebugLog {
    events: Vec<BlitterIrqDebugEvent>,
    first_assert: Option<BlitterIrqDebugEvent>,
}

impl BlitterIrqDebugLog {
    fn record(&mut self, event: BlitterIrqDebugEvent) {
        if self.first_assert.is_none() && (event.intreq_before & 0x0040) == 0 {
            self.first_assert = Some(event);
        }
        if self.events.len() < MAX_BLITTER_IRQ_DEBUG_EVENTS {
            self.events.push(event);
        }
    }
}

pub struct Amiga {
    pub master_clock: u64,
    pub model: AmigaModel,
    pub chipset: AmigaChipset,
    pub region: AmigaRegion,
    /// How the CPU clock relates to the master crystal. Crystal-derived
    /// models (A500, A1200) divide the video crystal; independent-clock
    /// models (A3000, A4000) use a Bresenham accumulator.
    cpu_clock_mode: CpuClockMode,
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
    // -- Input state --------------------------------------------------------
    /// Mouse port 0 (mouse) quadrature counter X (8-bit, wraps).
    pub mouse_x: u8,
    /// Mouse port 0 (mouse) quadrature counter Y (8-bit, wraps).
    pub mouse_y: u8,
    /// Joystick port 1 direction/fire bits (directly reported in JOY1DAT).
    /// Bits: [7:0] = XDAT (quadrature X), [15:8] = YDAT (quadrature Y).
    pub joy1dat: u16,
    /// Mouse/joystick button state. Each bit is active-low (0 = pressed):
    ///   Bit 0: left mouse button (LMB)
    ///   Bit 1: right mouse button (RMB)
    ///   Bit 2: middle mouse button (MMB, active-low via POTGOR)
    ///   Bit 3: joystick fire button (active-low via CIA-A PRA bit 7)
    pub input_buttons: u8,

    /// Previous state of CIA-A CRA bit 6 for edge-detecting the keyboard
    /// handshake. The real hardware acknowledges on the falling edge (1→0).
    pub cia_a_cra_sp_prev: bool,
    /// Previous motherboard EXTER line state so EXTER-routed board IRQs only
    /// latch Paula on rising edges instead of retriggering every master tick
    /// while held high.
    pub motherboard_external_irq_prev: bool,
    /// Gayle gate array (IDE + address decode). Present only on A600/A1200.
    pub gayle: Option<Gayle>,
    /// SDMAC 390537 SCSI controller. Present only on A3000/A3000T.
    pub dmac: Option<Dmac390537>,
    /// Ramsey DRAM controller resource registers. Present on A3000/A4000.
    pub ramsey: Option<Ramsey>,
    /// Fat Gary address decode and motherboard resource registers. Present on
    /// A3000/A4000.
    pub fat_gary: Option<FatGary>,
    /// Gary address decoder. Every model has one — configured at construction
    /// based on which peripherals are present.
    pub gary: Gary,
    /// Buster Zorro II bus controller. Present on A500/A1000/A2000/A500+.
    pub buster: Option<Buster>,
    /// Super Buster Zorro III bus controller. Present on A3000/A4000.
    pub super_buster: Option<SuperBuster>,
    /// Debug counter: number of VERTB interrupts asserted.
    pub vertb_count: u64,
    /// Debug counter: number of CIA-A TOD pulses.
    pub cia_a_tod_pulse_count: u64,
    audio_sample_phase: u64,
    audio_buffer: Vec<f32>,
    /// RC low-pass filter state (left, right) for hardware output stage.
    audio_lpf_left: f32,
    audio_lpf_right: f32,
    /// Filter coefficient: alpha = omega / (1 + omega), omega = 2π * cutoff / sample_rate.
    audio_lpf_alpha: f32,
    disk_dma_runtime: Option<DiskDmaRuntime>,
    sprite_dma_phase: [u8; 8],
    beam_debug_snapshot: BeamDebugSnapshot,
    beam_edge_flags: BeamEdgeFlags,
    beam_pixel_outputs_debug: BeamPixelOutputDebug,
    blitter_progress_debug: BlitterProgressDebugStats,
    blitter_irq_debug: BlitterIrqDebugLog,
    /// Pending BPLCON0 write to Denise (value, CCK countdown).
    /// Agnus sees the new value immediately; Denise sees it after 2 CCK.
    pub bplcon0_denise_pending: Option<(u16, u8)>,
    /// Pending DDFSTRT write (value, CCK countdown).
    pub ddfstrt_pending: Option<(u16, u8)>,
    /// Pending DDFSTOP write (value, CCK countdown).
    pub ddfstop_pending: Option<(u16, u8)>,
    /// Pending color register writes.
    /// Fields: (palette base index 0-31, value, CCK countdown, AGA BPLCON3 snapshot).
    /// The BPLCON3 snapshot captures bank selection and LOCT state at write
    /// time so that the drain correctly orders consecutive high/low nibble writes.
    pub color_pending: Vec<(usize, u16, u8, u16)>,
    /// Vertical bitplane DMA enable flip-flop, latched at line start (hpos=0).
    ///
    /// On real hardware Agnus latches the vertical DMA enable once per line
    /// based on VSTART/VSTOP comparisons, not per-CCK.  Without this latch,
    /// wrap-around window checks (e.g. VSTART=$FFF from COP1 init) falsely
    /// enable DMA on the first display line, causing BPLxPT to advance before
    /// the copper has written correct pointer values.
    bpl_dma_vactive_latch: bool,
    pub drive_sounds: DriveSoundGenerator,
    // -- Serial port --------------------------------------------------------
    /// SERPER ($032): baud rate period. Bits 14-0 = period, bit 15 = 9-bit mode.
    serper: u16,
    /// SERDATR ($018) status bits. Bit 13 = TBE, bit 12 = TSRE, bit 11 = RBF.
    /// Data bits 8-0 are the last received byte (always 0 for now).
    pub serdatr: u16,
    /// Countdown in CCKs until transmit shift register finishes sending.
    /// When this reaches 0, TBE and TSRE are set and TBE interrupt fires.
    serial_shift_countdown: u32,
    /// Countdown in CCKs until receive shift register finishes receiving.
    /// When this reaches 0, the byte is stored in SERDATR and RBF fires.
    serial_rx_countdown: u32,
    /// The byte currently being shifted in by the receive shift register.
    serial_rx_shift_byte: u16,
    /// Queue of bytes waiting to be received. The next byte starts shifting
    /// in as soon as the previous receive completes and SERPER is configured.
    serial_rx_queue: std::collections::VecDeque<u8>,
    // -- Battery-backed clock (RTC) -----------------------------------------
    /// RTC control registers D/E/F. Index 0 = reg D, 1 = reg E, 2 = reg F.
    pub rtc_control: [u8; 3],
    /// Whether host time has been latched into the RTC snapshot on first read.
    pub rtc_time_latched: bool,
    /// Snapshot of host time, latched on first RTC access. BCD-encoded.
    /// Indexed: 0=sec_lo, 1=sec_hi, 2=min_lo, 3=min_hi, 4=hr_lo, 5=hr_hi,
    /// 6=day_lo, 7=day_hi, 8=month_lo, 9=month_hi, 10=year_lo, 11=year_hi.
    pub rtc_time: [u8; 12],
}

impl Amiga {
    pub fn new(kickstart: Vec<u8>) -> Self {
        Self::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ocs,
            region: AmigaRegion::Pal,
            kickstart,
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            region,
            kickstart,
            slow_ram_size,
            ide_disk,
            scsi_disk,
            pcmcia_card,
        } = config;
        let chip_ram_size = match model {
            AmigaModel::A1000 | AmigaModel::A500 | AmigaModel::A2000 => 512 * 1024,
            AmigaModel::A500Plus | AmigaModel::A600 => 1024 * 1024,
            AmigaModel::A1200 | AmigaModel::A3000 | AmigaModel::A4000 => 2 * 1024 * 1024,
        };

        let region_lines = region.lines_per_frame();
        let raster_fb_height = match region {
            AmigaRegion::Pal => PAL_RASTER_FB_HEIGHT,
            AmigaRegion::Ntsc => NTSC_RASTER_FB_HEIGHT,
        };

        let agnus = {
            let mut ocs = commodore_agnus_ocs::Agnus::new_with_region_lines(region_lines);
            if chipset.is_aga() {
                ocs.max_bitplanes = 8;
            }
            let mut ecs = commodore_agnus_ecs::AgnusEcs::from_ocs(ocs);
            if chipset.is_ecs_or_aga() {
                ecs.set_pal_mode(region == AmigaRegion::Pal);
            }
            commodore_agnus_aga::AgnusAga::from_ecs(ecs)
        };
        let denise = {
            let mut ocs = commodore_denise_ocs::DeniseOcs::new_with_raster_height(raster_fb_height);
            if chipset.is_aga() {
                ocs.max_bitplanes = 8;
            }
            let ecs = commodore_denise_ecs::DeniseEcs::from_ocs(ocs);
            commodore_denise_aga::DeniseAga::from_ecs(ecs)
        };

        let (mut cpu, cpu_clock_mode) = match model {
            AmigaModel::A1200 => (
                Cpu68000::new_with_model(CpuModel::M68EC020),
                CpuClockMode::CrystalDerived { divisor: 2 },
            ),
            AmigaModel::A3000 => (
                Cpu68000::new_with_model(CpuModel::M68030),
                CpuClockMode::Independent {
                    freq_hz: 25_000_000,
                    phase: 0,
                    clock: 0,
                },
            ),
            AmigaModel::A4000 => (
                Cpu68000::new_with_model(CpuModel::M68040),
                CpuClockMode::Independent {
                    freq_hz: 25_000_000,
                    phase: 0,
                    clock: 0,
                },
            ),
            _ => (
                Cpu68000::new(),
                CpuClockMode::CrystalDerived {
                    divisor: TICKS_PER_CPU,
                },
            ),
        };
        // A3000/A4000 motherboard fast RAM (RAMSEY): 2 MB at $07E00000.
        // The 68030 instruction cache makes the RomTag scan fast enough.
        let (fast_ram_size, fast_ram_base) = match model {
            AmigaModel::A3000 | AmigaModel::A4000 => (2 * 1024 * 1024, 0x07E0_0000u32),
            _ => (0usize, 0u32),
        };
        let memory = Memory::new_with_fast_ram(
            chip_ram_size,
            kickstart,
            slow_ram_size,
            fast_ram_size,
            fast_ram_base,
        );

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

        let has_pcmcia = pcmcia_card.is_some();

        Self {
            master_clock: 0,
            model,
            chipset,
            region,
            cpu_clock_mode,
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
            mouse_x: 0,
            mouse_y: 0,
            joy1dat: 0,
            input_buttons: 0x0F, // all buttons released (active-low)
            cia_a_cra_sp_prev: false,
            motherboard_external_irq_prev: false,
            gayle: match model {
                AmigaModel::A600 | AmigaModel::A1200 => {
                    use crate::config::PcmciaCardConfig;
                    let gayle = if let Some(image) = ide_disk {
                        let geometry = commodore_gayle::DiskGeometry::from_image_size(image.len());
                        Gayle::with_disk(image, geometry)
                    } else {
                        match pcmcia_card {
                            Some(PcmciaCardConfig::Sram(image)) => {
                                Gayle::with_pcmcia_sram(image, false)
                            }
                            Some(PcmciaCardConfig::CompactFlash { image }) => {
                                let geom =
                                    commodore_gayle::DiskGeometry::from_image_size(image.len());
                                Gayle::with_pcmcia_cf(image, geom)
                            }
                            Some(PcmciaCardConfig::Ne2000 { mac }) => {
                                Gayle::with_pcmcia_ne2000(mac)
                            }
                            None => Gayle::new(),
                        }
                    };
                    Some(gayle)
                }
                _ => None,
            },
            dmac: match model {
                AmigaModel::A3000 => {
                    if let Some(image) = scsi_disk {
                        Some(Dmac390537::with_disk(0, image))
                    } else {
                        Some(Dmac390537::new())
                    }
                }
                _ => None,
            },
            ramsey: match model {
                AmigaModel::A3000 | AmigaModel::A4000 => Some(Ramsey::new()),
                _ => None,
            },
            fat_gary: match model {
                AmigaModel::A3000 | AmigaModel::A4000 => Some(FatGary::new()),
                _ => None,
            },
            gary: {
                let mut gary = Gary::new();
                gary.set_slow_ram_present(slow_ram_size > 0);
                gary.set_gayle_present(matches!(model, AmigaModel::A600 | AmigaModel::A1200));
                gary.set_dmac_present(matches!(model, AmigaModel::A3000));
                gary.set_resource_regs_present(
                    matches!(model, AmigaModel::A3000 | AmigaModel::A4000),
                );
                // A1000 has no RTC. A500 original has no RTC (unless expansion
                // board, which we don't model). Everything else has one.
                gary.set_rtc_present(!matches!(
                    model,
                    AmigaModel::A500 | AmigaModel::A1000
                ));
                gary.set_pcmcia_present(has_pcmcia);
                gary
            },
            buster: match model {
                // A500/A2000 have Zorro II slots. A1000 has a single
                // expansion slot but uses the same protocol.
                AmigaModel::A500 | AmigaModel::A1000 | AmigaModel::A2000
                | AmigaModel::A500Plus => Some(Buster::new()),
                _ => None,
            },
            super_buster: match model {
                AmigaModel::A3000 | AmigaModel::A4000 => Some(SuperBuster::new()),
                _ => None,
            },
            vertb_count: 0,
            cia_a_tod_pulse_count: 0,
            audio_sample_phase: 0,
            audio_buffer: Vec::with_capacity((AUDIO_SAMPLE_RATE as usize / 50) * 4),
            audio_lpf_left: 0.0,
            audio_lpf_right: 0.0,
            // RC low-pass at ~4500 Hz, matching the Amiga's hardware output filter.
            // alpha = omega / (1 + omega), omega = 2π × 4500 / 48000 ≈ 0.589
            audio_lpf_alpha: {
                let omega = 2.0 * std::f32::consts::PI * 4500.0 / AUDIO_SAMPLE_RATE as f32;
                omega / (1.0 + omega)
            },
            disk_dma_runtime: None,
            sprite_dma_phase: [0; 8],
            beam_debug_snapshot: BeamDebugSnapshot::default(),
            beam_edge_flags: BeamEdgeFlags::default(),
            beam_pixel_outputs_debug: BeamPixelOutputDebug::default(),
            blitter_progress_debug: BlitterProgressDebugStats::default(),
            blitter_irq_debug: BlitterIrqDebugLog::default(),
            bplcon0_denise_pending: None,
            ddfstrt_pending: None,
            ddfstop_pending: None,
            color_pending: Vec::new(),
            bpl_dma_vactive_latch: false,
            drive_sounds: DriveSoundGenerator::new(AUDIO_SAMPLE_RATE),
            serper: 0,
            serdatr: 0x3000, // TBE + TSRE set (transmitter idle)
            serial_shift_countdown: 0,
            serial_rx_countdown: 0,
            serial_rx_shift_byte: 0,
            serial_rx_queue: std::collections::VecDeque::new(),
            rtc_control: [0; 3],
            rtc_time_latched: false,
            rtc_time: [0; 12],
        }
    }

    // -- Reset API ----------------------------------------------------------

    /// Soft reset: reset all peripherals to power-on state (equivalent to
    /// the 68000 RESET instruction asserting the hardware reset line) then
    /// restart the CPU from the Kickstart reset vectors.
    pub fn soft_reset(&mut self) {
        // Reset peripherals — same as the bus-level RESET handler.
        self.cia_a.reset();
        self.cia_b.reset();
        self.memory.overlay = true;
        self.paula.reset();
        self.agnus.dmacon = 0;
        self.motherboard_external_irq_prev = false;
        if let Some(ramsey) = self.ramsey.as_mut() {
            ramsey.reset();
        }
        if let Some(fat_gary) = self.fat_gary.as_mut() {
            fat_gary.reset();
        }

        // Restart CPU from ROM vectors (overlay is now ON, so ROM is at $0).
        let ssp = u32::from_be_bytes([
            self.memory.kickstart[0],
            self.memory.kickstart[1],
            self.memory.kickstart[2],
            self.memory.kickstart[3],
        ]);
        let pc = u32::from_be_bytes([
            self.memory.kickstart[4],
            self.memory.kickstart[5],
            self.memory.kickstart[6],
            self.memory.kickstart[7],
        ]);
        self.cpu.reset_to(ssp, pc);
    }

    /// Hard reset: full power-on reset. Clears chip RAM, resets all chips
    /// and counters, restarts the CPU. Equivalent to cycling power.
    pub fn hard_reset(&mut self) {
        // Zero chip RAM.
        self.memory.chip_ram.fill(0);

        // Reset all peripherals via soft_reset.
        self.soft_reset();

        // Reset additional state that a soft reset preserves.
        self.copper = Copper::new();
        self.floppy = AmigaFloppyDrive::new();
        self.keyboard = AmigaKeyboard::new();
        self.mouse_x = 0;
        self.mouse_y = 0;
        self.joy1dat = 0;
        self.input_buttons = 0x0F;
        self.cia_a_cra_sp_prev = false;
        self.master_clock = 0;
        self.vertb_count = 0;
        self.cia_a_tod_pulse_count = 0;
        self.audio_buffer.clear();
        self.audio_lpf_left = 0.0;
        self.audio_lpf_right = 0.0;
        self.disk_dma_runtime = None;
        self.sprite_dma_phase = [0; 8];

        // Restore CIA-A external inputs to power-on state.
        self.cia_a.external_a = 0xEB;
    }

    // -- Input API ----------------------------------------------------------

    /// Push a mouse movement delta. The counters wrap naturally at 8 bits,
    /// producing the quadrature encoding Amiga software expects.
    pub fn push_mouse_delta(&mut self, dx: i16, dy: i16) {
        self.mouse_x = self.mouse_x.wrapping_add(dx as u8);
        self.mouse_y = self.mouse_y.wrapping_add(dy as u8);
    }

    /// Set a mouse button state. `button`: 0 = LMB, 1 = RMB, 2 = MMB.
    pub fn set_mouse_button(&mut self, button: u8, pressed: bool) {
        if button > 2 {
            return;
        }
        if pressed {
            self.input_buttons &= !(1 << button);
        } else {
            self.input_buttons |= 1 << button;
        }
        self.update_cia_a_buttons();
    }

    /// Set joystick direction bits. `direction` uses the standard encoding:
    ///   Bit 0: right, Bit 1: left, Bit 2: down, Bit 3: up.
    /// Set `fire` true when the fire button is pressed.
    pub fn set_joystick(&mut self, direction: u8, fire: bool) {
        // JOY1DAT quadrature encoding:
        //   Bit 1 (XOR bit 0) = right, Bit 9 (XOR bit 8) = down
        //   Bit 0 = raw X counter LSB, Bit 8 = raw Y counter LSB
        // The direction bits are translated to the counter positions that
        // produce the expected XOR results.
        let right = direction & 0x01 != 0;
        let left = direction & 0x02 != 0;
        let down = direction & 0x04 != 0;
        let up = direction & 0x08 != 0;

        // X axis: bit 1 = direction flag, bit 0 = counter LSB
        let x_lo: u16 = if left { 0b01 } else { 0b00 };
        let x_hi: u16 = if right || left { 0b10 } else { 0b00 };
        // Y axis: bit 9 = direction flag, bit 8 = counter LSB
        let y_lo: u16 = if up { 0b01 } else { 0b00 };
        let y_hi: u16 = if down || up { 0b10 } else { 0b00 };

        self.joy1dat = (y_hi << 8) | (y_lo << 8) | x_hi | x_lo;

        // Fire button
        if fire {
            self.input_buttons &= !(1 << 3);
        } else {
            self.input_buttons |= 1 << 3;
        }
        self.update_cia_a_buttons();
    }

    /// Update CIA-A PRA bits 6-7 from current button state.
    fn update_cia_a_buttons(&mut self) {
        // CIA-A PRA bit 6 = /FIR0 (left mouse button, active-low)
        // CIA-A PRA bit 7 = /FIR1 (joystick fire button, active-low)
        let fir0 = self.input_buttons & 0x01; // LMB: bit 0
        let fir1 = (self.input_buttons >> 3) & 0x01; // joy fire: bit 3
        self.cia_a.external_a = (self.cia_a.external_a & 0x3F)
            | (fir0 << 6)
            | (fir1 << 7);
    }

    /// Push a byte into the serial receive queue. The byte will be shifted
    /// in at the baud rate configured in SERPER, and an RBF interrupt (level 5)
    /// fires when reception completes. If a previous byte is still being
    /// shifted in, the new byte waits in the queue.
    pub fn push_serial_byte(&mut self, byte: u8) {
        self.serial_rx_queue.push_back(byte);
    }

    /// Inject a received Ethernet frame into the NE2000 PCMCIA card.
    ///
    /// No-op if no NE2000 card is inserted.
    pub fn push_network_rx_packet(&mut self, data: &[u8]) {
        if let Some(gayle) = self.gayle.as_mut() {
            if let Some(nic) = gayle.ne2000_mut() {
                nic.push_rx_packet(data);
            }
        }
    }

    /// Pop a transmitted Ethernet frame from the NE2000 PCMCIA card.
    ///
    /// Returns `None` if no NE2000 card or no pending transmit.
    #[must_use]
    pub fn pop_network_tx_packet(&mut self) -> Option<Vec<u8>> {
        self.gayle
            .as_mut()
            .and_then(|gayle| gayle.ne2000_mut())
            .and_then(|nic| nic.pop_tx_packet())
    }

    pub fn tick(&mut self) {
        self.master_clock += 1;

        if self.master_clock.is_multiple_of(TICKS_PER_CCK) {
            let vpos = self.agnus.vpos;
            let hpos = self.agnus.hpos;
            if hpos == 0 {
                self.denise.begin_beam_line();
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

            let hsync_tod_pulse = if self.chipset.is_ecs_or_aga() && self.agnus.varhsyen_enabled() {
                !prev_sync.hsync && current_sync.hsync
            } else {
                hpos == 0
            };
            let vsync_tod_pulse = if self.chipset.is_ecs_or_aga() && self.agnus.varvsyen_enabled() {
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
                self.vertb_count += 1;
                self.paula.request_interrupt(5);
                // Agnus restarts the copper from COP1LC at vertical blank,
                // but only when copper DMA is enabled (DMAEN + COPEN).
                if self.agnus.dma_enabled(0x0080) {
                    self.copper.restart_cop1();
                }
                // Sync interlace state from Agnus to Denise at frame start.
                let interlace = (self.agnus.bplcon0 & 0x0004) != 0;
                self.denise.interlace_active = interlace;
                self.denise.lof = self.agnus.lof;
            }

            // CIA-A TOD input is VSYNC. On OCS we pulse at frame wrap; on ECS
            // with variable VSYNC enabled we pulse on the programmable sync
            // window rising edge instead.
            if vsync_tod_pulse {
                self.cia_a_tod_pulse_count += 1;
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
            self.color_pending
                .retain_mut(|(idx, val, countdown, bplcon3_snap)| {
                    if *countdown <= 1 {
                        if self.chipset.is_aga() {
                            // Apply with the BPLCON3 state captured at write time
                            // so that LOCT and bank selection are correctly ordered
                            // relative to the color register write.
                            let saved = self.denise.bplcon3;
                            self.denise.bplcon3 = *bplcon3_snap;
                            self.denise.set_palette_aga(*idx, *val);
                            self.denise.bplcon3 = saved;
                        } else {
                            self.denise.set_palette(*idx, *val);
                        }
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
            let beam_x0_u16 = hpos.wrapping_mul(2);
            let beam_x1_u16 = beam_x0_u16.wrapping_add(1);
            let beam_x0 = u32::from(beam_x0_u16);
            let beam_x1 = u32::from(beam_x1_u16);
            let beam_y = u32::from(vpos);

            let playfield_gate0 = self.playfield_window_active_beam_x(vpos, hpos, beam_x0_u16);
            let playfield_gate1 = self.playfield_window_active_beam_x(vpos, hpos, beam_x1_u16);
            let mut diw_hstart_beam_x = None;
            let mut diw_hstop_beam_x = None;
            if (self.agnus.bplcon0 & 0x8000) != 0
                && let Some((_ecs_hspan_cck, hstart, hstop)) =
                    self.ecs_display_h_span_cck_and_bounds()
            {
                diw_hstart_beam_x = Some(hstart);
                diw_hstop_beam_x = Some(hstop);
            }
            // Output two half-CCK pixels. Beam position IS the coordinate.
            let pixel0_debug = self.denise.output_pixel_with_beam_and_playfield_gate(
                beam_x0,
                beam_y,
                beam_x0,
                beam_y,
                playfield_gate0,
            );
            let pixel1_debug = self.denise.output_pixel_with_beam_and_playfield_gate(
                beam_x1,
                beam_y,
                beam_x1,
                beam_y,
                playfield_gate1,
            );

            // --- Raster framebuffer writes ---
            // Each output call produces 4 independently-composed colour
            // indices in `quad_color_idx`. SuperHires: 4 unique per call.
            // Hires: [c0, c1, c1, c1]. Lores: all identical.
            // Write 8 sub-pixels per CCK (4 from pixel0 → sub 0-3,
            // 4 from pixel1 → sub 4-7).
            if self.chipset.is_aga() {
                for i in 0..4u8 {
                    let ci = pixel0_debug.quad_color_idx[i as usize];
                    let sp = pixel0_debug.quad_is_sprite[i as usize];
                    let rgb = self.denise.resolve_color_rgb24(ci, sp);
                    let argb = commodore_denise_ocs::DeniseOcs::rgb24_to_argb32(rgb);
                    self.denise.write_raster_pixel(hpos, vpos, i, argb);
                }
                for i in 0..4u8 {
                    let ci = pixel1_debug.quad_color_idx[i as usize];
                    let sp = pixel1_debug.quad_is_sprite[i as usize];
                    let rgb = self.denise.resolve_color_rgb24(ci, sp);
                    let argb = commodore_denise_ocs::DeniseOcs::rgb24_to_argb32(rgb);
                    self.denise.write_raster_pixel(hpos, vpos, 4 + i, argb);
                }
            } else {
                for i in 0..4u8 {
                    let ci = pixel0_debug.quad_color_idx[i as usize];
                    let rgb = self.denise.resolve_color_rgb12(ci);
                    let argb = commodore_denise_ocs::DeniseOcs::rgb12_to_argb32(rgb);
                    self.denise.write_raster_pixel(hpos, vpos, i, argb);
                }
                for i in 0..4u8 {
                    let ci = pixel1_debug.quad_color_idx[i as usize];
                    let rgb = self.denise.resolve_color_rgb12(ci);
                    let argb = commodore_denise_ocs::DeniseOcs::rgb12_to_argb32(rgb);
                    self.denise.write_raster_pixel(hpos, vpos, 4 + i, argb);
                }
            }

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
                let fetch_width = self.agnus.bpl_fetch_width() as u32;
                let addr = self.agnus.bpl_pt[idx];
                if fetch_width > 1 {
                    // AGA wider fetch: read `fetch_width` words, queue
                    // earlier words into the FIFO, load the last word
                    // into the holding latch.
                    for w in 0..fetch_width {
                        let word_addr = addr.wrapping_add(w * 2);
                        let hi = self.memory.read_chip_byte(word_addr);
                        let lo = self.memory.read_chip_byte(word_addr | 1);
                        let val = (u16::from(hi) << 8) | u16::from(lo);
                        if w < fetch_width - 1 {
                            self.denise.push_bpl_fifo(idx, val);
                        } else {
                            self.denise.load_bitplane(idx, val);
                        }
                    }
                } else {
                    let hi = self.memory.read_chip_byte(addr);
                    let lo = self.memory.read_chip_byte(addr | 1);
                    let val = (u16::from(hi) << 8) | u16::from(lo);
                    self.denise.load_bitplane(idx, val);
                }
                self.agnus.bpl_pt[idx] = addr.wrapping_add(fetch_width * 2);
                if plane == 0 {
                    fetched_plane_0 = true;
                    self.denise.queue_shift_load_from_bpl1dat();
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
                // dispatch block. Do NOT repeat it here — a second
                // trigger_shift_load corrupts the BPLCON1 barrel-shift carry
                // by overwriting bpl_prev_data.
                // Plane 0 is fetched at the end of the current DDF group:
                // ddfseq position 7 in lowres, 3 in hires.
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
                let incremental_completed =
                    execute_incremental_blitter_op(&mut self.agnus, &mut self.memory, blit_op);
                self.blitter_progress_debug.granted_ops += 1;
                if incremental_completed {
                    self.agnus.clear_blitter_scheduler();
                    self.agnus.blitter_busy = false;
                    self.request_blitter_interrupt(BlitterInterruptSource::SchedulerIncremental);
                }
            }
            if self.agnus.blitter_exec_ready()
                && let Some(source) = execute_blit(&mut self.agnus, &mut self.memory)
            {
                self.request_blitter_interrupt(source);
            }

            self.audio_sample_phase += u64::from(AUDIO_SAMPLE_RATE);
            while self.audio_sample_phase >= PAL_CCK_HZ {
                self.audio_sample_phase -= PAL_CCK_HZ;
                let (left, right) = self.paula.mix_audio_stereo();
                // Apply one-pole RC low-pass filter (~4.5 kHz cutoff)
                // to match the Amiga's hardware output stage. Paula only.
                let a = self.audio_lpf_alpha;
                self.audio_lpf_left += a * (left - self.audio_lpf_left);
                self.audio_lpf_right += a * (right - self.audio_lpf_right);
                // Drive sounds are mechanical — not routed through the
                // hardware audio filter. Add after the LPF to preserve
                // the sharp transients that make clicks sound real.
                self.drive_sounds.update_state(
                    self.floppy.motor_spinning(),
                    self.floppy.step_event_counter(),
                );
                let drive = self.drive_sounds.generate_sample();
                self.audio_buffer.push(self.audio_lpf_left + drive);
                self.audio_buffer.push(self.audio_lpf_right + drive);
            }

            self.agnus.tick_cck();

            // Check for pending disk DMA after CCK tick
            if self.paula.disk_dma_pending {
                self.paula.disk_dma_pending = false;
                self.start_disk_dma_transfer();
            }

            // Service SDMAC DMA transfers (A3000 SCSI)
            if let Some(dmac) = &mut self.dmac
                && dmac.dma_active()
                && dmac.dma_pending()
            {
                service_dmac_dma(dmac, &mut self.memory);
            }
        }

        // Tick the CPU. Crystal-derived clocks scale the master clock to
        // match the model's frequency. Independent clocks use a Bresenham
        // accumulator to advance the CPU at the correct rate.
        match &mut self.cpu_clock_mode {
            CpuClockMode::CrystalDerived { divisor } => {
                let cpu_clock = self.master_clock * (TICKS_PER_CPU / *divisor);
                let mut bus = AmigaBusWrapper {
                    model: self.model,
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
                    cia_a_cra_sp_prev: &mut self.cia_a_cra_sp_prev,
                    motherboard_external_irq_prev: &mut self.motherboard_external_irq_prev,
                    gayle: &mut self.gayle,
                    dmac: &mut self.dmac,
                    ramsey: &mut self.ramsey,
                    fat_gary: &mut self.fat_gary,
                    gary: &self.gary,
                    buster: &mut self.buster,
                    super_buster: &mut self.super_buster,
                    bplcon0_denise_pending: &mut self.bplcon0_denise_pending,
                    ddfstrt_pending: &mut self.ddfstrt_pending,
                    ddfstop_pending: &mut self.ddfstop_pending,
                    color_pending: &mut self.color_pending,
                    cpu_pc: self.cpu.regs.pc,
                    mouse_x: &mut self.mouse_x,
                    mouse_y: &mut self.mouse_y,
                    joy1dat: self.joy1dat,
                    input_buttons: self.input_buttons,
                    serdatr: self.serdatr,
                    rtc_control: &mut self.rtc_control,
                    rtc_time: &mut self.rtc_time,
                    rtc_time_latched: &mut self.rtc_time_latched,
                };
                self.cpu.tick(&mut bus, cpu_clock);
            }
            CpuClockMode::Independent {
                freq_hz,
                phase,
                clock,
            } => {
                let master_hz = if self.region == AmigaRegion::Pal {
                    PAL_CRYSTAL_HZ
                } else {
                    NTSC_CRYSTAL_HZ
                };
                *phase += *freq_hz;
                while *phase >= master_hz {
                    *phase -= master_hz;
                    *clock += 1;
                    let mut bus = AmigaBusWrapper {
                        model: self.model,
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
                        cia_a_cra_sp_prev: &mut self.cia_a_cra_sp_prev,
                        motherboard_external_irq_prev: &mut self.motherboard_external_irq_prev,
                        gayle: &mut self.gayle,
                        dmac: &mut self.dmac,
                        ramsey: &mut self.ramsey,
                        fat_gary: &mut self.fat_gary,
                    gary: &self.gary,
                        buster: &mut self.buster,
                        super_buster: &mut self.super_buster,
                        bplcon0_denise_pending: &mut self.bplcon0_denise_pending,
                        ddfstrt_pending: &mut self.ddfstrt_pending,
                        ddfstop_pending: &mut self.ddfstop_pending,
                        color_pending: &mut self.color_pending,
                        cpu_pc: self.cpu.regs.pc,
                        mouse_x: &mut self.mouse_x,
                        mouse_y: &mut self.mouse_y,
                        joy1dat: self.joy1dat,
                        input_buttons: self.input_buttons,
                        serdatr: self.serdatr,
                        rtc_control: &mut self.rtc_control,
                        rtc_time: &mut self.rtc_time,
                        rtc_time_latched: &mut self.rtc_time_latched,
                    };
                    // Scale clock to CPU bus-cycle domain: the 68000
                    // tick() gates on clock % 4 == 0, so multiply by 4
                    // so every Bresenham step maps to one bus cycle.
                    self.cpu.tick(&mut bus, *clock * TICKS_PER_CPU);
                }
            }
        }

        let motherboard_external_irq = self.motherboard_external_irq_pending();
        if motherboard_external_irq && !self.motherboard_external_irq_prev {
            self.paula.request_interrupt(13);
        }
        self.motherboard_external_irq_prev = motherboard_external_irq;

        if self.master_clock.is_multiple_of(TICKS_PER_ECLOCK) {
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

        // Serial port countdowns run at CCK rate (≈3.58 MHz).
        if self.master_clock.is_multiple_of(TICKS_PER_CCK) {
            // Transmit: when countdown reaches 0, byte is "sent" — set
            // TBE + TSRE and trigger TBE interrupt (INTREQ bit 0).
            if self.serial_shift_countdown > 0 {
                self.serial_shift_countdown -= 1;
                if self.serial_shift_countdown == 0 {
                    self.serdatr |= 0x3000; // TBE + TSRE
                    self.paula.request_interrupt(0); // TBE (level 1)
                }
            }

            // Receive: shift in bytes from the rx queue at the configured
            // baud rate. When a byte finishes, store it in SERDATR bits
            // 8-0, set RBF (bit 11), and fire RBF interrupt (bit 11).
            if self.serial_rx_countdown == 0 {
                if let Some(byte) = self.serial_rx_queue.pop_front() {
                    let period = u32::from(self.serper & 0x7FFF) + 1;
                    let bits = if self.serper & 0x8000 != 0 { 11u32 } else { 10 };
                    self.serial_rx_countdown = period * bits;
                    self.serial_rx_shift_byte = u16::from(byte);
                }
            }
            if self.serial_rx_countdown > 0 {
                self.serial_rx_countdown -= 1;
                if self.serial_rx_countdown == 0 {
                    let nine_bit = self.serper & 0x8000 != 0;
                    let data_mask: u16 = if nine_bit { 0x01FF } else { 0x00FF };
                    let stop_bit: u16 = if nine_bit { 0x0200 } else { 0x0100 };
                    self.serdatr = (self.serdatr & 0xF000)
                        | stop_bit
                        | (self.serial_rx_shift_byte & data_mask);
                    self.serdatr |= 0x0800; // RBF
                    self.paula.request_interrupt(11); // RBF (level 5)
                }
            }
        }
    }

    fn motherboard_external_irq_pending(&self) -> bool {
        // Gayle can present an IDE IRQ or PCMCIA IRQ directly to Paula EXTER
        // on Gayle-based machines. A3000 SDMAC does not follow that path: the
        // KS3.1 level-6 dispatch goes through ciab.resource, and wiring SDMAC
        // straight into Paula EXTER reaches the wrong handler.
        self.gayle
            .as_ref()
            .is_some_and(|gayle| gayle.ide_irq_pending() || gayle.pcmcia_irq_pending())
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
            self.denise.bplcon3,
        );
        // SERDAT ($030): Start serial transmit.
        if offset == 0x030 {
            self.serdatr &= !0x3000; // clear TBE + TSRE
            let period = u32::from(self.serper & 0x7FFF) + 1;
            let bits = if self.serper & 0x8000 != 0 { 11 } else { 10 }; // data + start + stop
            self.serial_shift_countdown = period * bits;
        }
        // SERPER ($032): Set baud rate period.
        if offset == 0x032 {
            self.serper = val;
        }
        // JOYTEST ($036): Write sets both JOY0DAT and JOY1DAT counters.
        if offset == 0x036 {
            self.mouse_x = (val & 0xFF) as u8;
            self.mouse_y = (val >> 8) as u8;
            self.joy1dat = val;
        }
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

    /// Drain interleaved stereo audio samples (`f32`, `L,R,...`).
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.audio_buffer)
    }

    /// Current state of the power and drive activity LEDs.
    pub fn indicator_state(&self) -> IndicatorState {
        let pra = self.cia_a.port_a_output();
        IndicatorState {
            power_led_on: pra & 0x02 == 0, // active-low
            drive_motor_on: self.floppy.motor_on(),
            drive_dma_active: self.disk_dma_runtime.is_some(),
        }
    }

    /// Insert an ADF disk image into the internal floppy drive (DF0:).
    pub fn insert_disk(&mut self, adf: Adf) {
        self.floppy.insert_disk(adf);
    }

    /// Insert any disk image implementing `DiskImage` (ADF, IPF, etc.).
    pub fn insert_disk_image(&mut self, image: Box<dyn drive_amiga_floppy::DiskImage>) {
        self.floppy.insert_disk_image(image);
    }

    /// Eject the current DF0: disk, if any.
    pub fn eject_disk(&mut self) {
        self.floppy.eject_disk();
    }

    /// Return whether DF0: currently has a disk inserted.
    pub fn has_disk(&self) -> bool {
        self.floppy.has_disk()
    }

    /// Return the current ADF image as raw bytes, or `None` if no disk is inserted.
    pub fn save_adf(&self) -> Option<Vec<u8>> {
        self.floppy.save_adf()
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
            // When no disk is inserted, produce a silent MFM stream (all zeros)
            // so the DMA transfer completes and trackdisk gets its DSKBLK
            // interrupt. Without this, the DMA hangs forever waiting for data.
            self.floppy
                .encode_mfm_track()
                .unwrap_or_else(|| vec![0u8; 32])
        };
        let has_disk = self.floppy.has_disk();
        let wordsync_enabled = !is_write && has_disk && (self.paula.adkcon & 0x0400 != 0);
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
                let was_write = runtime.is_write;
                self.disk_dma_runtime = None;
                // Persist written sectors to ADF image when write DMA completes.
                if was_write {
                    self.floppy.flush_write_capture();
                }
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

    fn next_sprite_dma_vpos(vpos: u16, lines_per_frame: u16) -> u16 {
        let next = vpos.wrapping_add(1);
        if next >= lines_per_frame { 0 } else { next }
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
        let fetch_width = self.agnus.spr_fetch_width() as u32;

        // Helper: read one word from chip RAM at `a`.
        let read_word = |mem: &Memory, a: u32| -> u16 {
            let hi = mem.read_chip_byte(a);
            let lo = mem.read_chip_byte(a | 1);
            (u16::from(hi) << 8) | u16::from(lo)
        };

        match self.sprite_dma_phase[sprite] {
            // Phases 0/1 (POS/CTL): always single-word fetch.
            0 => {
                let word = read_word(&self.memory, addr);
                self.denise.write_sprite_pos(sprite, word);
                self.sprite_dma_phase[sprite] = 1;
                self.agnus.spr_pt[sprite] = addr.wrapping_add(2);
            }
            1 => {
                let word = read_word(&self.memory, addr);
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
                self.agnus.spr_pt[sprite] = addr.wrapping_add(2);
            }
            // Phase 2 (DATA): fetch 1-4 words depending on FMODE sprite width.
            2 => {
                let mut words = [0u16; 4];
                for i in 0..fetch_width {
                    words[i as usize] = read_word(&self.memory, addr.wrapping_add(i * 2));
                }
                self.denise
                    .write_sprite_data_wide(sprite, &words[..fetch_width as usize]);
                self.sprite_dma_phase[sprite] = 3;
                self.agnus.spr_pt[sprite] = addr.wrapping_add(fetch_width * 2);
            }
            // Phase 3 (DATB): fetch 1-4 words depending on FMODE sprite width.
            _ => {
                let mut words = [0u16; 4];
                for i in 0..fetch_width {
                    words[i as usize] = read_word(&self.memory, addr.wrapping_add(i * 2));
                }
                self.denise
                    .write_sprite_datb_wide(sprite, &words[..fetch_width as usize]);
                let pos = self.denise.spr_pos[sprite];
                let ctl = self.denise.spr_ctl[sprite];
                let vstart = (((ctl >> 2) & 0x0001) << 8) | ((pos >> 8) & 0x00FF);
                let vstop = (((ctl >> 1) & 0x0001) << 8) | ((ctl >> 8) & 0x00FF);
                let next_vpos = Self::next_sprite_dma_vpos(vpos, self.agnus.lines_per_frame);
                self.sprite_dma_phase[sprite] =
                    if Self::sprite_line_active(next_vpos, vstart, vstop) {
                        2
                    } else {
                        0
                    };
                self.agnus.spr_pt[sprite] = addr.wrapping_add(fetch_width * 2);
            }
        }
    }

    /// Map a beam position to raster framebuffer coordinates.
    ///
    /// Returns `Some((fb_x, fb_y))` in raster-buffer space when the position
    /// is in the visible (non-blanked, in-window) area. Returns `None` when
    /// blanked by ECS programmable blank windows or outside the ECS display
    /// window. OCS has no programmable blanking — all positions are visible
    /// (the playfield gate clips bitplane data separately via DIWSTRT/DIWSTOP).
    fn beam_to_fb_beam_x(&self, vpos: u16, hpos_cck: u16, beam_x: u16) -> Option<(u32, u32)> {
        if self.chipset.is_ecs_or_aga() {
            if self.agnus.hblank_window_active(hpos_cck) {
                return None;
            }
            if self.agnus.vblank_window_active(vpos) {
                return None;
            }
            let (vstart, vstop, hstart, hstop) = self.ecs_decoded_diw_window();
            if vstart == vstop {
                return None;
            }
            let v_active = if vstart < vstop {
                vpos >= vstart && vpos < vstop
            } else {
                vpos >= vstart || vpos < vstop
            };
            if !v_active {
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
        }
        // Raster coordinates: beam position maps directly.
        // beam_x is in hires pixels; framebuffer is superhires (×4).
        let fb_x = u32::from(beam_x) * 4;
        let fb_y = u32::from(vpos) * 2;
        if fb_x >= RASTER_FB_WIDTH || fb_y >= self.denise.raster_fb_height {
            return None;
        }
        Some((fb_x, fb_y))
    }

    fn beam_to_fb(&self, vpos: u16, hpos_cck: u16) -> Option<(u32, u32)> {
        self.beam_to_fb_beam_x(vpos, hpos_cck, hpos_cck.wrapping_mul(2))
    }

    fn playfield_window_active_beam_x(&self, vpos: u16, _hpos_cck: u16, beam_x: u16) -> bool {
        // Denise compares its internal horizontal counter against DIWSTRT/DIWSTOP.
        // The Denise counter trails Agnus beam_x by 1 pixel (half-CCK). This is the
        // same offset used for the BPLCON1 scroll comparator (verified pixel-perfect
        // against FS-UAE). Rather than subtracting 1 from beam_x, we add 1 to the
        // comparison thresholds.
        const DENISE_H_OFFSET: u16 = 0;

        if !self.chipset.is_ecs_or_aga() {
            // OCS: clip to DIWSTRT/DIWSTOP with implicit H8/V8 bits.
            let vstart = (self.agnus.diwstrt >> 8) & 0x00FF;
            let stop_v_low = (self.agnus.diwstop >> 8) & 0x00FF;
            let stop_v8 = ((!((stop_v_low >> 7) & 1)) & 1) << 8;
            let vstop = stop_v8 | stop_v_low;
            let hstart = (self.agnus.diwstrt & 0x00FF) + DENISE_H_OFFSET;
            let hstop = (0x0100 | (self.agnus.diwstop & 0x00FF)) + DENISE_H_OFFSET;
            let v_active = if vstart == vstop {
                true // no vertical clipping if start == stop
            } else if vstart < vstop {
                vpos >= vstart && vpos < vstop
            } else {
                vpos >= vstart || vpos < vstop
            };
            let h_active = if hstart == hstop {
                true
            } else if hstart < hstop {
                beam_x >= hstart && beam_x < hstop
            } else {
                beam_x >= hstart || beam_x < hstop
            };
            return v_active && h_active;
        }
        if self.agnus.hblank_window_active(_hpos_cck) || self.agnus.vblank_window_active(vpos) {
            return false;
        }
        let (vstart, vstop, hstart, hstop) = self.ecs_decoded_diw_window();
        if vstart == vstop || hstart == hstop {
            return false;
        }
        let hstart = hstart + DENISE_H_OFFSET;
        let hstop = hstop + DENISE_H_OFFSET;
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

    fn ecs_decoded_diw_window(&self) -> (u16, u16, u16, u16) {
        if self.agnus.diwhigh_written() {
            let diwhigh = self.agnus.diwhigh();
            let vstart = ((diwhigh & 0x000F) << 8) | ((self.agnus.diwstrt >> 8) & 0x00FF);
            let vstop = (((diwhigh >> 8) & 0x000F) << 8) | ((self.agnus.diwstop >> 8) & 0x00FF);
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
        if !self.chipset.is_ecs_or_aga() {
            return None;
        }
        let (_vstart, _vstop, hstart, hstop) = self.ecs_decoded_diw_window();
        if hstart == hstop {
            return None;
        }
        // hstart/hstop are register values (DIWSTRT/DIWSTOP H field).
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
        if self.chipset.is_ecs_or_aga() {
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
            // OCS: fixed display window, lines $2C..$12C (44..300).
            let rel = vpos.wrapping_sub(0x2C);
            self.bpl_dma_vactive_latch = rel < 256;
        }
    }

    fn bitplane_dma_vertical_active(&self, vpos: u16) -> bool {
        if self.chipset.is_ecs_or_aga() {
            if self.agnus.vblank_window_active(vpos) {
                return false;
            }
            self.bpl_dma_vactive_latch
        } else {
            self.bpl_dma_vactive_latch
        }
    }

    /// Report coarse ECS sync-window state at a specific beam position.
    ///
    /// On OCS, or before ECS sync-window behavior is enabled, both fields are
    /// `false`.
    #[must_use]
    pub fn beam_sync_state_at(&self, vpos: u16, hpos_cck: u16) -> BeamSyncState {
        if !self.chipset.is_ecs_or_aga() {
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
        if !self.chipset.is_ecs_or_aga() {
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
        _hpos_cck: u16,
        _vpos: u16,
    ) -> BeamCompositeSyncDebug {
        if !self.chipset.is_ecs_or_aga() {
            return BeamCompositeSyncDebug::default();
        }

        let (active, mode) = if self.agnus.varcsyen_enabled() {
            // ECS/AGA variable composite sync: HSYNC XOR VSYNC.
            let csync_raw = sync.hsync ^ sync.vsync;
            (csync_raw, BeamCompositeSyncMode::VariableXorSync)
        } else {
            // Hardwired mode: simple OR of H and V sync.
            (
                sync.hsync || sync.vsync,
                BeamCompositeSyncMode::HardwiredHvOr,
            )
        };

        BeamCompositeSyncDebug {
            active,
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
        let (hblank, vblank) = if self.chipset.is_ecs_or_aga() {
            (
                self.agnus.hblank_window_active(hpos_cck),
                self.agnus.vblank_window_active(vpos),
            )
        } else {
            (false, false)
        };
        let sync = self.beam_sync_state_at(vpos, hpos_cck);
        let composite_sync = self.beam_composite_sync_debug_from_components(sync, hpos_cck, vpos);
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

    #[must_use]
    pub fn blitter_irq_debug_events(&self) -> &[BlitterIrqDebugEvent] {
        &self.blitter_irq_debug.events
    }

    #[must_use]
    pub const fn first_blitter_irq_assert(&self) -> Option<BlitterIrqDebugEvent> {
        self.blitter_irq_debug.first_assert
    }

    fn request_blitter_interrupt(&mut self, source: BlitterInterruptSource) {
        let intreq_before = self.paula.intreq;
        self.paula.request_interrupt(6);
        self.blitter_irq_debug.record(BlitterIrqDebugEvent {
            tick: self.master_clock,
            source,
            pc: self.cpu.regs.pc,
            instr_start_pc: self.cpu.instr_start_pc,
            ir: self.cpu.ir,
            sr: self.cpu.regs.sr,
            vpos: self.agnus.vpos,
            hpos: self.agnus.hpos,
            dmacon: self.agnus.dmacon,
            intena: self.paula.intena,
            intreq_before,
            intreq_after: self.paula.intreq,
            blitter_busy: self.agnus.blitter_busy,
            blitter_ccks_remaining: self.agnus.blitter_ccks_remaining,
            bltcon0: self.agnus.bltcon0,
            bltcon1: self.agnus.bltcon1,
            bltsize: self.agnus.bltsize,
            bltsizv_ecs: self.agnus.bltsizv_ecs,
            bltsizh_ecs: self.agnus.bltsizh_ecs,
            blt_apt: self.agnus.blt_apt,
            blt_bpt: self.agnus.blt_bpt,
            blt_cpt: self.agnus.blt_cpt,
            blt_dpt: self.agnus.blt_dpt,
            blt_amod: self.agnus.blt_amod,
            blt_bmod: self.agnus.blt_bmod,
            blt_cmod: self.agnus.blt_cmod,
            blt_dmod: self.agnus.blt_dmod,
            blt_adat: self.agnus.blt_adat,
            blt_bdat: self.agnus.blt_bdat,
            blt_cdat: self.agnus.blt_cdat,
            blt_afwm: self.agnus.blt_afwm,
            blt_alwm: self.agnus.blt_alwm,
        });
    }
}

impl emu_core::Observable for Amiga {
    fn query(&self, path: &str) -> Option<emu_core::Value> {
        use emu_core::Value;

        if let Some(rest) = path.strip_prefix("cpu.") {
            self.cpu.query(rest)
        } else if let Some(rest) = path.strip_prefix("agnus.") {
            let inner = self.agnus.as_inner();
            match rest {
                "vpos" => Some(Value::U16(inner.vpos)),
                "hpos" => Some(Value::U16(inner.hpos)),
                "bplcon0" => Some(Value::U16(inner.bplcon0)),
                "diwstrt" => Some(Value::U16(inner.diwstrt)),
                "diwstop" => Some(Value::U16(inner.diwstop)),
                "ddfstrt" => Some(Value::U16(inner.ddfstrt)),
                "ddfstop" => Some(Value::U16(inner.ddfstop)),
                "dmacon" => Some(Value::U16(inner.dmacon)),
                "blitter_busy" => Some(Value::Bool(inner.blitter_busy)),
                "blitter_ccks_remaining" => Some(Value::U32(inner.blitter_ccks_remaining)),
                "beamcon0" => Some(Value::U16(self.agnus.beamcon0())),
                "htotal" => Some(Value::U16(self.agnus.htotal())),
                "hsstop" => Some(Value::U16(self.agnus.hsstop())),
                "vtotal" => Some(Value::U16(self.agnus.vtotal())),
                "vsstop" => Some(Value::U16(self.agnus.vsstop())),
                "hbstrt" => Some(Value::U16(self.agnus.hbstrt())),
                "hbstop" => Some(Value::U16(self.agnus.hbstop())),
                "vbstrt" => Some(Value::U16(self.agnus.vbstrt())),
                "vbstop" => Some(Value::U16(self.agnus.vbstop())),
                "hsstrt" => Some(Value::U16(self.agnus.hsstrt())),
                "vsstrt" => Some(Value::U16(self.agnus.vsstrt())),
                "diwhigh" => Some(Value::U16(self.agnus.diwhigh())),
                "diwhigh_written" => Some(Value::Bool(self.agnus.diwhigh_written())),
                "mode.varbeamen" => Some(Value::Bool(self.agnus.varbeamen_enabled())),
                "mode.varvben" => Some(Value::Bool(self.agnus.varvben_enabled())),
                "mode.varvsyen" => Some(Value::Bool(self.agnus.varvsyen_enabled())),
                "mode.varhsyen" => Some(Value::Bool(self.agnus.varhsyen_enabled())),
                "mode.cscben" => Some(Value::Bool(self.agnus.cscben_enabled())),
                "mode.varcsyen" => Some(Value::Bool(self.agnus.varcsyen_enabled())),
                "mode.harddis" => Some(Value::Bool(self.agnus.harddis_enabled())),
                "mode.blanken" => Some(Value::Bool(self.agnus.blanken_enabled())),
                "mode.csytrue" => Some(Value::Bool(self.agnus.csytrue_enabled())),
                "mode.vsytrue" => Some(Value::Bool(self.agnus.vsytrue_enabled())),
                "mode.hsytrue" => Some(Value::Bool(self.agnus.hsytrue_enabled())),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("denise.") {
            if let Some(idx_str) = rest.strip_prefix("palette.") {
                let idx: usize = idx_str.parse().ok()?;
                if idx < 32 {
                    Some(Value::U16(self.denise.as_inner().palette[idx]))
                } else {
                    None
                }
            } else {
                match rest {
                    "bplcon0" => Some(Value::U16(self.denise.bplcon0)),
                    "bplcon3" => Some(Value::U16(self.denise.bplcon3)),
                    "mode.shres" => Some(Value::Bool(self.denise.shres_enabled())),
                    "mode.bplhwrm" => Some(Value::Bool(self.denise.bplhwrm_enabled())),
                    "mode.sprhwrm" => Some(Value::Bool(self.denise.sprhwrm_enabled())),
                    "mode.killehb" => Some(Value::Bool(self.denise.killehb_enabled())),
                    "mode.border_blank" => Some(Value::Bool(self.denise.border_blank_enabled())),
                    "mode.border_opaque" => Some(Value::Bool(self.denise.border_opaque_enabled())),
                    _ => None,
                }
            }
        } else if let Some(rest) = path.strip_prefix("ramsey.") {
            let ramsey = self.ramsey.as_ref()?;
            match rest {
                "config" => Some(Value::U8(ramsey.config())),
                "revision" => Some(Value::U8(ramsey.revision())),
                "wrap_enabled" => Some(Value::Bool(ramsey.wrap_enabled())),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("fat_gary.") {
            let fat_gary = self.fat_gary.as_ref()?;
            match rest {
                "toenb" => Some(Value::U8(fat_gary.toenb())),
                "timeout" => Some(Value::U8(fat_gary.timeout())),
                "coldboot" => Some(Value::U8(fat_gary.coldboot_flag())),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("gayle.") {
            let gayle = self.gayle.as_ref()?;
            match rest {
                "cs" => Some(Value::U8(gayle.cs())),
                "irq" => Some(Value::U8(gayle.irq())),
                "int_enable" => Some(Value::U8(gayle.int_enable())),
                "cfg" => Some(Value::U8(gayle.cfg())),
                "ide_status" => Some(Value::U8(gayle.ide_status())),
                "drive_present" => Some(Value::Bool(gayle.drive_present())),
                "ide_irq_pending" => Some(Value::Bool(gayle.ide_irq_pending())),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("dmac.") {
            let dmac = self.dmac.as_ref()?;
            match rest {
                "cntr" => Some(Value::U8(dmac.cntr())),
                "dawr" => Some(Value::U8(dmac.dawr())),
                "wtc" => Some(Value::U32(dmac.wtc())),
                "acr" => Some(Value::U32(dmac.acr())),
                "istr" => Some(Value::U8(dmac.current_istr())),
                "wd.selected_reg" => Some(Value::U8(dmac.wd_selected_reg())),
                "wd.asr" => Some(Value::U8(dmac.wd_asr())),
                "wd.scsi_status" => Some(Value::U8(dmac.wd_scsi_status())),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("paula.") {
            if let Some(ch_rest) = rest.strip_prefix("audio.") {
                // paula.audio.0.period, paula.audio.0.volume, paula.audio.0.sample
                let dot = ch_rest.find('.')?;
                let ch: usize = ch_rest[..dot].parse().ok()?;
                let field = &ch_rest[dot + 1..];
                let (per, vol, sample) = self.paula.audio_channel_state(ch)?;
                match field {
                    "period" => Some(Value::U16(per)),
                    "volume" => Some(Value::U8(vol)),
                    "sample" => Some(Value::I8(sample)),
                    _ => None,
                }
            } else {
                match rest {
                    "intena" => Some(Value::U16(self.paula.intena)),
                    "intreq" => Some(Value::U16(self.paula.intreq)),
                    "adkcon" => Some(Value::U16(self.paula.adkcon)),
                    _ => None,
                }
            }
        } else if let Some(rest) = path.strip_prefix("cia_a.") {
            match rest {
                "timer_a" => Some(Value::U16(self.cia_a.timer_a())),
                "timer_b" => Some(Value::U16(self.cia_a.timer_b())),
                "icr_status" => Some(Value::U8(self.cia_a.icr_status())),
                "icr_mask" => Some(Value::U8(self.cia_a.icr_mask())),
                "cra" => Some(Value::U8(self.cia_a.cra())),
                "crb" => Some(Value::U8(self.cia_a.crb())),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("cia_b.") {
            match rest {
                "timer_a" => Some(Value::U16(self.cia_b.timer_a())),
                "timer_b" => Some(Value::U16(self.cia_b.timer_b())),
                "icr_status" => Some(Value::U8(self.cia_b.icr_status())),
                "icr_mask" => Some(Value::U8(self.cia_b.icr_mask())),
                "cra" => Some(Value::U8(self.cia_b.cra())),
                "crb" => Some(Value::U8(self.cia_b.crb())),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("memory.") {
            let addr =
                if let Some(hex) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(hex) = rest.strip_prefix('$') {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    rest.parse().ok()
                };
            addr.map(|a| Value::U8(self.memory.read_byte(a)))
        } else {
            match path {
                "master_clock" => Some(Value::U64(self.master_clock)),
                _ => self.cpu.query(path),
            }
        }
    }

    fn query_paths(&self) -> &'static [&'static str] {
        &[
            "cpu.pc",
            "cpu.sr",
            "cpu.ccr",
            "cpu.d0",
            "cpu.d1",
            "cpu.d2",
            "cpu.d3",
            "cpu.d4",
            "cpu.d5",
            "cpu.d6",
            "cpu.d7",
            "cpu.a0",
            "cpu.a1",
            "cpu.a2",
            "cpu.a3",
            "cpu.a4",
            "cpu.a5",
            "cpu.a6",
            "cpu.a7",
            "cpu.usp",
            "cpu.ssp",
            "cpu.ir",
            "cpu.irc",
            "cpu.flags.c",
            "cpu.flags.v",
            "cpu.flags.z",
            "cpu.flags.n",
            "cpu.flags.x",
            "cpu.flags.s",
            "cpu.flags.t",
            "cpu.flags.ipl",
            "cpu.halted",
            "cpu.idle",
            "agnus.vpos",
            "agnus.hpos",
            "agnus.bplcon0",
            "agnus.diwstrt",
            "agnus.diwstop",
            "agnus.ddfstrt",
            "agnus.ddfstop",
            "agnus.dmacon",
            "agnus.blitter_busy",
            "agnus.blitter_ccks_remaining",
            "agnus.beamcon0",
            "agnus.htotal",
            "agnus.hsstop",
            "agnus.vtotal",
            "agnus.vsstop",
            "agnus.hbstrt",
            "agnus.hbstop",
            "agnus.vbstrt",
            "agnus.vbstop",
            "agnus.hsstrt",
            "agnus.vsstrt",
            "agnus.diwhigh",
            "agnus.diwhigh_written",
            "agnus.mode.varbeamen",
            "agnus.mode.varvben",
            "agnus.mode.varvsyen",
            "agnus.mode.varhsyen",
            "agnus.mode.cscben",
            "agnus.mode.varcsyen",
            "agnus.mode.harddis",
            "agnus.mode.blanken",
            "agnus.mode.csytrue",
            "agnus.mode.vsytrue",
            "agnus.mode.hsytrue",
            "denise.palette.0",
            "denise.palette.1",
            "denise.palette.2",
            "denise.palette.3",
            "denise.palette.4",
            "denise.palette.5",
            "denise.palette.6",
            "denise.palette.7",
            "denise.palette.8",
            "denise.palette.9",
            "denise.palette.10",
            "denise.palette.11",
            "denise.palette.12",
            "denise.palette.13",
            "denise.palette.14",
            "denise.palette.15",
            "denise.palette.16",
            "denise.palette.17",
            "denise.palette.18",
            "denise.palette.19",
            "denise.palette.20",
            "denise.palette.21",
            "denise.palette.22",
            "denise.palette.23",
            "denise.palette.24",
            "denise.palette.25",
            "denise.palette.26",
            "denise.palette.27",
            "denise.palette.28",
            "denise.palette.29",
            "denise.palette.30",
            "denise.palette.31",
            "denise.bplcon0",
            "denise.bplcon3",
            "denise.mode.shres",
            "denise.mode.bplhwrm",
            "denise.mode.sprhwrm",
            "denise.mode.killehb",
            "denise.mode.border_blank",
            "denise.mode.border_opaque",
            "ramsey.config",
            "ramsey.revision",
            "ramsey.wrap_enabled",
            "fat_gary.toenb",
            "fat_gary.timeout",
            "fat_gary.coldboot",
            "gayle.cs",
            "gayle.irq",
            "gayle.int_enable",
            "gayle.cfg",
            "gayle.ide_status",
            "gayle.drive_present",
            "gayle.ide_irq_pending",
            "dmac.cntr",
            "dmac.dawr",
            "dmac.wtc",
            "dmac.acr",
            "dmac.istr",
            "dmac.wd.selected_reg",
            "dmac.wd.asr",
            "dmac.wd.scsi_status",
            "paula.intena",
            "paula.intreq",
            "paula.adkcon",
            "paula.audio.<0-3>.period",
            "paula.audio.<0-3>.volume",
            "paula.audio.<0-3>.sample",
            "cia_a.timer_a",
            "cia_a.timer_b",
            "cia_a.icr_status",
            "cia_a.icr_mask",
            "cia_a.cra",
            "cia_a.crb",
            "cia_b.timer_a",
            "cia_b.timer_b",
            "cia_b.icr_status",
            "cia_b.icr_mask",
            "cia_b.cra",
            "cia_b.crb",
            "memory.<address>",
            "master_clock",
        ]
    }
}

pub struct AmigaBusWrapper<'a> {
    pub model: AmigaModel,
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
    pub cia_a_cra_sp_prev: &'a mut bool,
    pub motherboard_external_irq_prev: &'a mut bool,
    pub gayle: &'a mut Option<Gayle>,
    pub dmac: &'a mut Option<Dmac390537>,
    pub ramsey: &'a mut Option<Ramsey>,
    pub fat_gary: &'a mut Option<FatGary>,
    pub gary: &'a Gary,
    pub buster: &'a mut Option<Buster>,
    pub super_buster: &'a mut Option<SuperBuster>,
    // Pipeline state for delayed register writes (Agnus→Denise propagation).
    pub bplcon0_denise_pending: &'a mut Option<(u16, u8)>,
    pub ddfstrt_pending: &'a mut Option<(u16, u8)>,
    pub ddfstop_pending: &'a mut Option<(u16, u8)>,
    pub color_pending: &'a mut Vec<(usize, u16, u8, u16)>,
    pub cpu_pc: u32,
    pub mouse_x: &'a mut u8,
    pub mouse_y: &'a mut u8,
    pub joy1dat: u16,
    pub input_buttons: u8,
    pub serdatr: u16,
    pub rtc_control: &'a mut [u8; 3],
    pub rtc_time: &'a mut [u8; 12],
    pub rtc_time_latched: &'a mut bool,
}

/// Transfer pending DMAC DMA data between the SCSI buffer and system memory.
///
/// On real hardware the SDMAC uses burst DMA. We transfer the entire
/// buffer at once (instantaneous) because there are no cycle-stealing
/// effects to model on the A3000's 32-bit bus. The ACR (address counter)
/// and WTC (word transfer count) registers track progress.
fn service_dmac_dma(dmac: &mut Dmac390537, memory: &mut Memory) {
    let remaining = dmac.dma_bytes_remaining();
    if remaining == 0 {
        return;
    }
    let mut addr = dmac.acr();
    if dmac.dma_direction_read() {
        // Target → initiator: SCSI read data → system memory.
        for _ in 0..remaining {
            let byte = dmac.dma_read_byte();
            memory.write_byte_32(addr, byte);
            addr = addr.wrapping_add(1);
        }
    } else {
        // Initiator → target: system memory → SCSI write buffer.
        for _ in 0..remaining {
            let byte = memory.read_byte_32(addr);
            dmac.dma_write_byte(byte);
            addr = addr.wrapping_add(1);
        }
    }
    // Update ACR to reflect the post-transfer address.
    // WTC is not decremented here — KS relies on the WD33C93 transfer
    // count, not the SDMAC WTC, for completion detection.
}

fn motherboard_paula_intreq_bits(model: AmigaModel, dmac: Option<&Dmac390537>) -> u16 {
    match model {
        // The A3000 ROM installs its SCSI interrupt server on Exec's level-2
        // list and the Paula level-2 autovector handler only dispatches that
        // list when PORTS is pending. Surface SDMAC timeout/complete events as
        // an effective PORTS source instead of a raw CPU IPL2 line.
        AmigaModel::A3000 if dmac.is_some_and(Dmac390537::irq_pending) => 0x0008,
        _ => 0,
    }
}

fn effective_paula_intreq(model: AmigaModel, paula: &Paula8364, dmac: Option<&Dmac390537>) -> u16 {
    paula.intreq | motherboard_paula_intreq_bits(model, dmac)
}

fn effective_paula_ipl(model: AmigaModel, paula: &Paula8364, dmac: Option<&Dmac390537>) -> u8 {
    // Master enable: bit 14
    if paula.intena & 0x4000 == 0 {
        return 0;
    }

    let active = paula.intena & effective_paula_intreq(model, paula, dmac) & 0x3FFF;
    if active == 0 {
        return 0;
    }

    if active & 0x2000 != 0 {
        return 6;
    }
    if active & 0x1800 != 0 {
        return 5;
    }
    if active & 0x0780 != 0 {
        return 4;
    }
    if active & 0x0070 != 0 {
        return 3;
    }
    if active & 0x0008 != 0 {
        return 2;
    }
    if active & 0x0007 != 0 {
        return 1;
    }

    0
}

fn motherboard_cpu_irq_level(_model: AmigaModel, _dmac: Option<&Dmac390537>) -> u8 {
    0
}

impl<'a> M68kBus for AmigaBusWrapper<'a> {
    fn poll_ipl(&mut self) -> u8 {
        effective_paula_ipl(self.model, self.paula, self.dmac.as_ref())
            .max(motherboard_cpu_irq_level(self.model, self.dmac.as_ref()))
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
        *self.motherboard_external_irq_prev = false;
        if let Some(ramsey) = self.ramsey.as_mut() {
            ramsey.reset();
        }
        if let Some(fat_gary) = self.fat_gary.as_mut() {
            fat_gary.reset();
        }
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
        // from the combined Paula + motherboard IPL state instead.
        if fc == FunctionCode::InterruptAck {
            let level = self.poll_ipl() as u16;
            return BusStatus::Ready(24 + level);
        }

        // Motherboard fast RAM (RAMSEY, A3000/A4000) sits above 24-bit
        // space. Check the full 32-bit address before applying the mask.
        if !self.memory.fast_ram.is_empty() {
            let base = self.memory.fast_ram_base;
            let end = base.wrapping_add(self.memory.fast_ram.len() as u32);
            if addr >= base && addr < end {
                let offset = (addr - base) & self.memory.fast_ram_mask;
                if is_read {
                    let val = if is_word {
                        let hi = self.memory.fast_ram[offset as usize];
                        let lo = self.memory.fast_ram[(offset | 1) as usize];
                        (u16::from(hi) << 8) | u16::from(lo)
                    } else {
                        u16::from(self.memory.fast_ram[offset as usize])
                    };
                    return BusStatus::Ready(val);
                } else {
                    let val = data.unwrap_or(0);
                    if is_word {
                        self.memory.fast_ram[offset as usize] = (val >> 8) as u8;
                        self.memory.fast_ram[(offset | 1) as usize] = val as u8;
                    } else {
                        self.memory.fast_ram[offset as usize] = val as u8;
                    }
                    return BusStatus::Ready(0);
                }
            }
        }

        // On A3000/A4000, Fat Gary only forwards $000000-$FFFFFF to the
        // 24-bit bus. Addresses $01000000+ that didn't match fast RAM
        // above are unmapped — return 0 for reads, sink writes.
        //
        // Real hardware would BERR after a bus timeout, but our bus error
        // exception frame handling isn't complete enough yet (VBR-relative
        // vector fetches, 68040 format $7 PC semantics). Return 0 for now;
        // exec's memory probe interprets repeated 0 reads as "no hardware".
        if let Some(fat_gary) = self.fat_gary.as_ref()
            && !fat_gary.forwards_to_24bit_bus(addr)
        {
            return BusStatus::Ready(0);
        }

        // Everything else uses 24-bit decode. On 24-bit CPUs (68000)
        // only A0-A23 are wired, so the mask is a no-op.
        let addr = addr & 0xFFFFFF;

        use commodore_gary::ChipSelect;

        match self.gary.decode(addr) {
            // CIA-A ($BFExxx, accent on D0-D7 low byte)
            ChipSelect::CiaA => {
                let reg = ((addr >> 8) & 0x0F) as u8;
                if is_read {
                    if addr & 1 != 0 {
                        return BusStatus::Ready(u16::from(self.cia_a.read(reg)));
                    }
                    return BusStatus::Ready(0xFF00);
                }
                let should_write = (addr & 1 != 0) || is_word;
                if should_write {
                    let val = data.unwrap_or(0) as u8;
                    self.cia_a.write(reg, val);
                    if reg == 0 || reg == 2 {
                        let out = self.cia_a.port_a_output();
                        self.memory.overlay = out & 0x01 != 0;
                    }
                    if reg == 0x0E {
                        let sp_now = val & 0x40 != 0;
                        if *self.cia_a_cra_sp_prev && !sp_now {
                            self.keyboard.handshake();
                        }
                        *self.cia_a_cra_sp_prev = sp_now;
                    }
                }
                BusStatus::Ready(0)
            }

            // CIA-B ($BFDxxx, accent on D8-D15 high byte)
            ChipSelect::CiaB => {
                let reg = ((addr >> 8) & 0x0F) as u8;
                if is_read {
                    if addr & 1 == 0 {
                        return BusStatus::Ready(u16::from(self.cia_b.read(reg)) << 8 | 0x00FF);
                    }
                    return BusStatus::Ready(0x00FF);
                }
                let should_write = (addr & 1 == 0) || is_word;
                if should_write {
                    let val = if is_word {
                        (data.unwrap_or(0) >> 8) as u8
                    } else {
                        data.unwrap_or(0) as u8
                    };
                    self.cia_b.write(reg, val);
                    if reg == 0x01 {
                        let prb = self.cia_b.port_b_output();
                        let step = prb & 0x01 == 0;
                        let dir_inward = prb & 0x02 == 0;
                        let side_upper = prb & 0x04 == 0;
                        let sel = prb & 0x08 == 0;
                        let motor = prb & 0x80 == 0;
                        self.floppy
                            .update_control(step, dir_inward, side_upper, sel, motor);
                    }
                }
                BusStatus::Ready(0)
            }

            // Custom chip registers ($DFFxxx)
            ChipSelect::Custom => {
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
                        if custom_register_byte_zero_extend(offset) {
                            lane_word
                        } else if let Some(current) = custom_register_byte_merge_latch(
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
                        self.denise.bplcon3,
                    );
                    // JOYTEST ($036): preset both mouse and joystick counters.
                    if offset == 0x036 {
                        *self.mouse_x = (val & 0xFF) as u8;
                        *self.mouse_y = (val >> 8) as u8;
                    }
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
                    return BusStatus::Ready(0);
                }
                let word = match offset {
                    0x002 => {
                        let busy = if self.agnus.blitter_busy { 0x4000 } else { 0 };
                        self.agnus.dmacon | busy
                    }
                    0x004 => {
                        let lof_bit = if self.agnus.lof { 0x8000u16 } else { 0 };
                        let agnus_id = match self.chipset {
                            AmigaChipset::Ocs => 0x00u16,
                            AmigaChipset::Ecs => 0x20u16,
                            AmigaChipset::Aga => 0x22u16,
                        };
                        let v8 = (self.agnus.vpos >> 8) & 1;
                        let v9 = (self.agnus.vpos >> 9) & 1;
                        let v10 = (self.agnus.vpos >> 10) & 1;
                        lof_bit | (agnus_id << 8) | (v10 << 2) | (v9 << 1) | v8
                    }
                    0x006 => {
                        ((self.agnus.vpos & 0xFF) << 8) | (self.agnus.hpos & 0xFF)
                    }
                    0x008 => self.paula.dskdatr,
                    0x00A => u16::from(*self.mouse_y) << 8 | u16::from(*self.mouse_x),
                    0x00C => self.joy1dat,
                    0x00E => self.denise.read_clxdat(),
                    0x010 => self.paula.adkcon,
                    0x016 => {
                        // POTGOR: active-low button state. All bits high when
                        // idle ($FF00). Pressed buttons clear their data line.
                        let rmb = (self.input_buttons >> 1) & 0x01; // 1=released, 0=pressed
                        let mmb = (self.input_buttons >> 2) & 0x01;
                        // Bits 10 (DATLY=RMB) and 8 (DATLX=MMB), rest high.
                        (0xFF00 & !(1u16 << 10) & !(1u16 << 8))
                            | (u16::from(rmb) << 10)
                            | (u16::from(mmb) << 8)
                    }
                    0x018 => self.serdatr,
                    0x01A => self.paula.read_dskbytr(self.agnus.dmacon),
                    0x01C => self.paula.intena,
                    0x01E => effective_paula_intreq(self.model, self.paula, self.dmac.as_ref()),
                    0x05C if self.chipset.is_ecs_or_aga() => self.agnus.bltsizv_ecs,
                    0x05E if self.chipset.is_ecs_or_aga() => self.agnus.bltsizh_ecs,
                    0x0A0..=0x0DA => self.paula.read_audio_register(offset).unwrap_or(0),
                    0x106 if self.chipset.is_ecs_or_aga() => self.denise.bplcon3,
                    0x10C if self.chipset.is_aga() => self.denise.bplcon4,
                    0x1FC if self.chipset.is_aga() => self.agnus.fmode,
                    0x1C0 if self.chipset.is_ecs_or_aga() => self.agnus.htotal(),
                    0x1C2 if self.chipset.is_ecs_or_aga() => self.agnus.hsstop(),
                    0x1C4 if self.chipset.is_ecs_or_aga() => self.agnus.hbstrt(),
                    0x1C6 if self.chipset.is_ecs_or_aga() => self.agnus.hbstop(),
                    0x1C8 if self.chipset.is_ecs_or_aga() => self.agnus.vtotal(),
                    0x1CA if self.chipset.is_ecs_or_aga() => self.agnus.vsstop(),
                    0x1CC if self.chipset.is_ecs_or_aga() => self.agnus.vbstrt(),
                    0x1CE if self.chipset.is_ecs_or_aga() => self.agnus.vbstop(),
                    0x1DC if self.chipset.is_ecs_or_aga() => self.agnus.beamcon0(),
                    0x1DE if self.chipset.is_ecs_or_aga() => self.agnus.hsstrt(),
                    0x1E0 if self.chipset.is_ecs_or_aga() => self.agnus.vsstrt(),
                    0x1E4 if self.chipset.is_ecs_or_aga() => self.agnus.diwhigh(),
                    0x07C => match self.chipset {
                        AmigaChipset::Ocs => 0xFFFF,
                        AmigaChipset::Ecs => self.denise.as_inner().deniseid(),
                        AmigaChipset::Aga => {
                            // A4000 AGA Lisa returns $FCF8; other AGA models
                            // return $00F8. Matches WinUAE's IDE_A4000 check.
                            if self.model == AmigaModel::A4000 {
                                0xFCF8
                            } else {
                                self.denise.deniseid()
                            }
                        }
                    }
                    _ => 0,
                };
                if !is_word {
                    let byte = if addr & 1 == 0 {
                        (word >> 8) as u8
                    } else {
                        word as u8
                    };
                    return BusStatus::Ready(u16::from(byte));
                }
                BusStatus::Ready(word)
            }

            // SDMAC 390537 SCSI controller ($DD0000-$DDFFFF)
            ChipSelect::Dmac => {
                if let Some(dmac) = self.dmac {
                    if is_read {
                        let val = if is_word {
                            dmac.read_word(addr)
                        } else {
                            u16::from(dmac.read_byte(addr))
                        };
                        return BusStatus::Ready(val);
                    }
                    let val = data.unwrap_or(0);
                    if is_word {
                        dmac.write_word(addr, val);
                    } else {
                        dmac.write_byte(addr, val as u8);
                    }
                    BusStatus::Ready(0)
                } else {
                    BusStatus::Ready(0)
                }
            }

            // Motherboard resource registers ($DE0000-$DEFFFF)
            ChipSelect::ResourceRegisters => {
                let addr64 = (addr >> 6) & 3;
                let addr2 = addr & 3;
                if is_read {
                    let val = self
                        .ramsey
                        .as_ref()
                        .map_or(0, |ramsey| ramsey.read_resource_byte(addr64, addr2))
                        | self
                            .fat_gary
                            .as_ref()
                            .map_or(0, |fat_gary| fat_gary.read_resource_byte(addr2));
                    BusStatus::Ready(u16::from(val))
                } else {
                    let val = data.unwrap_or(0) as u8;
                    if let Some(ramsey) = self.ramsey.as_mut() {
                        ramsey.write_resource_byte(addr64, addr2, val);
                    }
                    if let Some(fat_gary) = self.fat_gary.as_mut() {
                        fat_gary.write_resource_byte(addr2, val);
                    }
                    BusStatus::Ready(0)
                }
            }

            // Gayle gate array ($D80000-$DFFFFF)
            ChipSelect::Gayle => {
                if let Some(gayle) = self.gayle {
                    if is_read {
                        let val = if is_word {
                            gayle.read_word(addr)
                        } else {
                            u16::from(gayle.read(addr))
                        };
                        return BusStatus::Ready(val);
                    }
                    let val = data.unwrap_or(0);
                    if is_word {
                        gayle.write_word(addr, val);
                    } else {
                        gayle.write(addr, val as u8);
                    }
                    BusStatus::Ready(0)
                } else {
                    BusStatus::Ready(0)
                }
            }

            // Chip RAM ($000000-$1FFFFF) — DMA-arbitrated
            ChipSelect::ChipRam => {
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
            }

            // Slow RAM, ROM — non-DMA memory regions
            ChipSelect::SlowRam | ChipSelect::Rom => {
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
            }

            // Autoconfig — route to Buster if present, else plain memory
            ChipSelect::Autoconfig => {
                if let Some(sb) = self.super_buster.as_mut() {
                    // A3000/A4000: Super Buster handles both Z3 and Z2 autoconfig.
                    if is_read {
                        BusStatus::Ready(u16::from(sb.autoconfig_read(addr)))
                    } else {
                        sb.autoconfig_write(addr, data.unwrap_or(0) as u8);
                        BusStatus::Ready(0)
                    }
                } else if let Some(buster) = self.buster.as_mut() {
                    // A500/A2000 etc: Zorro II Buster.
                    if is_read {
                        BusStatus::Ready(u16::from(buster.autoconfig_read(addr)))
                    } else {
                        buster.autoconfig_write(addr, data.unwrap_or(0) as u8);
                        BusStatus::Ready(0)
                    }
                } else {
                    // No bus controller (A600/A1200 without expansion).
                    BusStatus::Ready(0)
                }
            }

            // Battery-backed clock (MSM6242B) at $DC0000-$DC003F
            // Nybble-wide BCD registers: RTC A0-A3 = CPU A2-A5
            ChipSelect::Rtc => {
                let reg = ((addr & 0x3F) >> 2) as usize;
                if is_read {
                    // Latch time on first read
                    if !*self.rtc_time_latched {
                        *self.rtc_time_latched = true;
                        // Fixed time: 1993-01-01 12:00:00 (Friday)
                        // S1,S10,MI1,MI10,H1,H10,D1,D10,MO1,MO10,Y1,Y10
                        *self.rtc_time = [0, 0, 0, 0, 2, 1, 1, 0, 1, 0, 3, 9];
                    }
                    let val: u8 = match reg {
                        0..=11 => self.rtc_time[reg] & 0x0F,
                        12 => 5, // day of week: Friday
                        13 => self.rtc_control[0] & 0x0F,
                        14 => self.rtc_control[1] & 0x0F,
                        15 => self.rtc_control[2] & 0x0F,
                        _ => 0,
                    };
                    BusStatus::Ready(u16::from(val))
                } else {
                    let val = (data.unwrap_or(0) & 0x0F) as u8;
                    match reg {
                        0..=11 => self.rtc_time[reg] = val,
                        13 => self.rtc_control[0] = val,
                        14 => {
                            self.rtc_control[1] = val;
                            // Bit 0 = HOLD: clearing unlatches time
                            if val & 1 == 0 {
                                *self.rtc_time_latched = false;
                            }
                        }
                        15 => self.rtc_control[2] = val,
                        _ => {}
                    }
                    BusStatus::Ready(0)
                }
            }

            // PCMCIA common memory ($600000-$9FFFFF)
            ChipSelect::PcmciaCommon => {
                if let Some(gayle) = self.gayle.as_mut() {
                    if is_read {
                        BusStatus::Ready(u16::from(gayle.read_pcmcia_common(addr)))
                    } else {
                        gayle.write_pcmcia_common(addr, data.unwrap_or(0) as u8);
                        BusStatus::Ready(0)
                    }
                } else {
                    BusStatus::Ready(0)
                }
            }

            // PCMCIA attribute/IO/reset ($A00000-$A5FFFF)
            ChipSelect::PcmciaAttr => {
                if let Some(gayle) = self.gayle.as_mut() {
                    if is_read {
                        if is_word {
                            BusStatus::Ready(gayle.read_pcmcia_attr_word(addr))
                        } else {
                            BusStatus::Ready(u16::from(gayle.read_pcmcia_attr(addr)))
                        }
                    } else if is_word {
                        gayle.write_pcmcia_attr_word(addr, data.unwrap_or(0));
                        BusStatus::Ready(0)
                    } else {
                        gayle.write_pcmcia_attr(addr, data.unwrap_or(0) as u8);
                        BusStatus::Ready(0)
                    }
                } else {
                    BusStatus::Ready(0)
                }
            }

            // Unmapped — check expansion boards, then Fat Gary timeout, else 0
            ChipSelect::Unmapped => {
                // A4000 NCR 53C710 SCSI controller at $DD0000-$DDFFFF.
                // We don't model the NCR chip, so return $FF (bus pull-ups)
                // for reads. This makes the scsi.device probe detect "no
                // hardware" immediately instead of entering a retry loop
                // that calls DoIO(timer.device) during COLD init — which
                // would permanently hijack the exec scheduler via Wait().
                if self.model == AmigaModel::A4000
                    && (addr & 0xFF_0000) == 0xDD_0000
                {
                    return if is_read {
                        BusStatus::Ready(0x00FF)
                    } else {
                        BusStatus::Ready(0)
                    };
                }

                // Check Super Buster Z3+Z2 boards (A3000/A4000).
                if let Some(sb) = self.super_buster.as_mut() {
                    if is_read {
                        if let Some(val) = sb.z3_board_read(addr) {
                            return BusStatus::Ready(u16::from(val));
                        }
                        if let Some(val) = sb.z2_board_read(addr) {
                            return BusStatus::Ready(u16::from(val));
                        }
                    } else {
                        let val = data.unwrap_or(0) as u8;
                        if sb.z3_board_write(addr, val) || sb.z2_board_write(addr, val) {
                            return BusStatus::Ready(0);
                        }
                    }
                }
                // Check Buster Z2 boards (A500/A2000 etc).
                if let Some(buster) = self.buster.as_mut() {
                    if is_read {
                        if let Some(val) = buster.board_read(addr) {
                            return BusStatus::Ready(u16::from(val));
                        }
                    } else {
                        let val = data.unwrap_or(0) as u8;
                        if buster.board_write(addr, val) {
                            return BusStatus::Ready(0);
                        }
                    }
                }
                // No board claimed it — check Fat Gary timeout on A3000/A4000.
                if let Some(fat_gary) = self.fat_gary.as_ref() {
                    match fat_gary.check_timeout(addr) {
                        commodore_fat_gary::TimeoutResult::Ok => BusStatus::Ready(0),
                        commodore_fat_gary::TimeoutResult::BusTimeout => BusStatus::Error,
                        commodore_fat_gary::TimeoutResult::Unmapped => BusStatus::Ready(0),
                    }
                } else {
                    BusStatus::Ready(0)
                }
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
        // Blitter control registers — KS 2.04+ may byte-write these.
        0x040 => Some(agnus.bltcon0),
        0x042 => Some(agnus.bltcon1),
        // Display / DMA-visible latches commonly byte-written by ROMs/copper.
        0x08E => Some(agnus.diwstrt),
        0x090 => Some(agnus.diwstop),
        0x092 => Some(agnus.ddfstrt),
        0x094 => Some(agnus.ddfstop),
        0x098 => Some(denise.clxcon),
        0x09A => Some(paula.intena),
        0x09C => Some(paula.intreq),
        0x09E => Some(paula.adkcon),
        0x05C if chipset.is_ecs_or_aga() => Some(agnus.bltsizv_ecs),
        0x05E if chipset.is_ecs_or_aga() => Some(agnus.bltsizh_ecs),
        0x100 => Some(agnus.bplcon0),
        0x102 => Some(denise.bplcon1),
        0x104 => Some(denise.bplcon2),
        0x106 if chipset.is_ecs_or_aga() => Some(denise.bplcon3),
        0x10C if chipset.is_aga() => Some(denise.bplcon4),
        0x108 => Some(agnus.bpl1mod as u16),
        0x10A => Some(agnus.bpl2mod as u16),
        0x180..=0x1BE => {
            let idx = ((offset - 0x180) / 2) as usize;
            Some(denise.palette[idx])
        }
        0x1C0 if chipset.is_ecs_or_aga() => Some(agnus.htotal()),
        0x1C2 if chipset.is_ecs_or_aga() => Some(agnus.hsstop()),
        0x1C4 if chipset.is_ecs_or_aga() => Some(agnus.hbstrt()),
        0x1C6 if chipset.is_ecs_or_aga() => Some(agnus.hbstop()),
        0x1C8 if chipset.is_ecs_or_aga() => Some(agnus.vtotal()),
        0x1CA if chipset.is_ecs_or_aga() => Some(agnus.vsstop()),
        0x1CC if chipset.is_ecs_or_aga() => Some(agnus.vbstrt()),
        0x1CE if chipset.is_ecs_or_aga() => Some(agnus.vbstop()),
        0x1DC if chipset.is_ecs_or_aga() => Some(agnus.beamcon0()),
        0x1DE if chipset.is_ecs_or_aga() => Some(agnus.hsstrt()),
        0x1E0 if chipset.is_ecs_or_aga() => Some(agnus.vsstrt()),
        0x1E4 if chipset.is_ecs_or_aga() => Some(agnus.diwhigh()),
        _ => None,
    }
}

fn custom_register_byte_zero_extend(offset: u16) -> bool {
    matches!(offset, 0x096 | 0x09A | 0x09C | 0x09E)
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
    color_pending: &mut Vec<(usize, u16, u8, u16)>,
    offset: u16,
    val: u16,
    bplcon3: u16,
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
        // Capture BPLCON3 at write time for correct LOCT/bank ordering.
        0x180..=0x1BE => {
            let idx = ((offset - 0x180) / 2) as usize;
            color_pending.push((idx, val, 2, bplcon3));
        }
        _ => {}
    }
}

/// Shared custom register write dispatch used by both CPU and copper paths.
#[allow(clippy::too_many_arguments)]
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
        0x05A if chipset.is_ecs_or_aga() => {
            // BLTCON0L (ECS): write only the low byte (minterm/LF) of BLTCON0,
            // preserving the upper byte (ASH + channel enables).
            agnus.bltcon0 = (agnus.bltcon0 & 0xFF00) | (val & 0x00FF);
        }
        0x05C if chipset.is_ecs_or_aga() => {
            agnus.bltsizv_ecs = val;
        }
        0x05E if chipset.is_ecs_or_aga() => {
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
        0x080 => {
            copper.cop1lc = (copper.cop1lc & 0x0000FFFF) | (u32::from(val) << 16);
        }
        0x082 => {
            copper.cop1lc = (copper.cop1lc & 0xFFFF0000) | u32::from(val & 0xFFFE);
        }
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

        // Serial port — handled in write_custom_reg() which has &mut self
        0x030 | 0x032 => {}

        // Copper danger
        0x02E => copper.danger = val & 0x02 != 0,

        // ECS display/beam extensions (latch-only for now, gated off on OCS)
        0x1C0 if chipset.is_ecs_or_aga() => agnus.write_htotal(val),
        0x1C2 if chipset.is_ecs_or_aga() => agnus.write_hsstop(val),
        0x1C4 if chipset.is_ecs_or_aga() => agnus.write_hbstrt(val),
        0x1C6 if chipset.is_ecs_or_aga() => agnus.write_hbstop(val),
        0x1C8 if chipset.is_ecs_or_aga() => agnus.write_vtotal(val),
        0x1CA if chipset.is_ecs_or_aga() => agnus.write_vsstop(val),
        0x1CC if chipset.is_ecs_or_aga() => agnus.write_vbstrt(val),
        0x1CE if chipset.is_ecs_or_aga() => agnus.write_vbstop(val),
        0x1DC if chipset.is_ecs_or_aga() => agnus.write_beamcon0(val),
        0x1DE if chipset.is_ecs_or_aga() => agnus.write_hsstrt(val),
        0x1E0 if chipset.is_ecs_or_aga() => agnus.write_vsstrt(val),
        0x1E4 if chipset.is_ecs_or_aga() => agnus.write_diwhigh(val),

        // Bitplane control — Agnus sees BPLCON0 immediately; Denise
        // update is pipelined (2 CCK) via queue_pipelined_write().
        0x100 => {
            agnus.bplcon0 = val;
        }
        0x102 => denise.bplcon1 = val,
        0x104 => denise.bplcon2 = val,
        0x106 if chipset.is_ecs_or_aga() => denise.bplcon3 = val,
        0x10C if chipset.is_aga() => denise.bplcon4 = val,

        // Bitplane modulos
        0x108 => agnus.bpl1mod = val as i16,
        0x10A => agnus.bpl2mod = val as i16,

        // Bitplane pointers ($0E0-$0FE): OCS 6 planes, AGA 8 planes.
        0x0E0..=0x0FE => {
            let idx = ((offset - 0x0E0) / 4) as usize;
            if idx >= 8 {
                // should not happen given the range, but be defensive
            } else if idx < 6 || chipset.is_aga() {
                if offset & 2 == 0 {
                    agnus.bpl_pt[idx] = (agnus.bpl_pt[idx] & 0x0000FFFF) | (u32::from(val) << 16);
                } else {
                    agnus.bpl_pt[idx] = (agnus.bpl_pt[idx] & 0xFFFF0000) | u32::from(val & 0xFFFE);
                }
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

        // AGA FMODE ($1FC) — bitplane/sprite DMA fetch width.
        0x1FC if chipset.is_aga() => {
            agnus.fmode = val;
            denise.set_sprite_width_from_fmode(val);
        }

        // Paula audio channels (AUD0-AUD3)
        0x0A0..=0x0DA => {
            let _ = paula.write_audio_register(offset, val);
        }

        _ => {}
    }
}

/// Execute one queued blitter DMA timing op against any incremental blitter
/// runtime currently active in Agnus.
pub fn execute_incremental_blitter_op(
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

/// Execute a blitter operation synchronously when the coarse scheduler matures.
///
/// On real hardware the blitter runs in DMA slots over many CCKs. We still run
/// the whole operation instantly here, but only after a coarse per-CCK delay so
/// `BLTBUSY` and nasty-mode arbitration persist across CCKs.
fn execute_blit(agnus: &mut Agnus, memory: &mut Memory) -> Option<BlitterInterruptSource> {
    let height = (agnus.bltsize >> 6) & 0x3FF;
    let width_words = agnus.bltsize & 0x3F;
    let height = if height == 0 { 1024 } else { height } as u32;
    let width_words = if width_words == 0 { 64 } else { width_words } as u32;

    // LINE mode (BLTCON1 bit 0): Bresenham line drawing.
    // Uses a completely different algorithm from area mode.
    if agnus.bltcon1 & 0x0001 != 0 {
        return Some(execute_blit_line(agnus, memory));
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
    Some(BlitterInterruptSource::AreaCore)
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
fn execute_blit_line(agnus: &mut Agnus, memory: &mut Memory) -> BlitterInterruptSource {
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

    let row_mod = agnus.blt_cmod; // Destination row stride in bytes

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
            error = error.wrapping_add(error_sub);
        } else {
            // Step on major axis ONLY
            if major_is_y {
                step_y(&mut cpt, &mut dpt);
            } else {
                step_x(&mut cpt, &mut dpt, &mut pixel_bit);
            }
            error = error.wrapping_add(error_add);
        }
    }

    // Update registers
    agnus.blt_apt = error as u16 as u32;
    agnus.blt_cpt = cpt;
    agnus.blt_dpt = dpt;
    agnus.blt_bdat = texture;

    agnus.clear_blitter_scheduler();
    agnus.blitter_busy = false;
    BlitterInterruptSource::LineCore
}

#[cfg(test)]
mod tests {
    use super::{
        Amiga, AmigaBusWrapper, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion,
        BeamCompositeSyncDebug, BeamCompositeSyncMode, BeamDebugSnapshot, BeamEdgeFlags,
        BeamPinState, BeamSyncState, BlitterInterruptSource, TICKS_PER_CCK,
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
            model: amiga.model,
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
            cia_a_cra_sp_prev: &mut amiga.cia_a_cra_sp_prev,
            motherboard_external_irq_prev: &mut amiga.motherboard_external_irq_prev,
            gayle: &mut amiga.gayle,
            dmac: &mut amiga.dmac,
            ramsey: &mut amiga.ramsey,
            fat_gary: &mut amiga.fat_gary,
            gary: &amiga.gary,
            buster: &mut amiga.buster,
            super_buster: &mut amiga.super_buster,
            bplcon0_denise_pending: &mut amiga.bplcon0_denise_pending,
            ddfstrt_pending: &mut amiga.ddfstrt_pending,
            ddfstop_pending: &mut amiga.ddfstop_pending,
            color_pending: &mut amiga.color_pending,
            cpu_pc: 0,
            mouse_x: &mut amiga.mouse_x,
            mouse_y: &mut amiga.mouse_y,
            joy1dat: amiga.joy1dat,
            input_buttons: amiga.input_buttons,
            serdatr: amiga.serdatr,
            rtc_control: &mut amiga.rtc_control,
            rtc_time: &mut amiga.rtc_time,
            rtc_time_latched: &mut amiga.rtc_time_latched,
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

    fn write_custom_byte_via_cpu_bus(amiga: &mut Amiga, offset: u16, byte_addr_lsb: u16, val: u8) {
        let mut bus = AmigaBusWrapper {
            model: amiga.model,
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
            cia_a_cra_sp_prev: &mut amiga.cia_a_cra_sp_prev,
            motherboard_external_irq_prev: &mut amiga.motherboard_external_irq_prev,
            gayle: &mut amiga.gayle,
            dmac: &mut amiga.dmac,
            ramsey: &mut amiga.ramsey,
            fat_gary: &mut amiga.fat_gary,
            gary: &amiga.gary,
            buster: &mut amiga.buster,
            super_buster: &mut amiga.super_buster,
            bplcon0_denise_pending: &mut amiga.bplcon0_denise_pending,
            ddfstrt_pending: &mut amiga.ddfstrt_pending,
            ddfstop_pending: &mut amiga.ddfstop_pending,
            color_pending: &mut amiga.color_pending,
            cpu_pc: 0,
            mouse_x: &mut amiga.mouse_x,
            mouse_y: &mut amiga.mouse_y,
            joy1dat: amiga.joy1dat,
            input_buttons: amiga.input_buttons,
            serdatr: amiga.serdatr,
            rtc_control: &mut amiga.rtc_control,
            rtc_time: &mut amiga.rtc_time,
            rtc_time_latched: &mut amiga.rtc_time_latched,
        };
        let addr = 0x00DFF000 | u32::from(offset | byte_addr_lsb);
        let result = M68kBus::poll_cycle(
            &mut bus,
            addr,
            FunctionCode::SupervisorData,
            false,
            false,
            Some(u16::from(val)),
        );
        assert_eq!(result, BusStatus::Ready(0));
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });
        assert_eq!(amiga.chipset, AmigaChipset::Ecs);
    }

    #[test]
    fn ecs_bplcon3_killehb_disables_halfbrite_decode() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });
        amiga.denise.set_palette(5, 0x0ACE);
        amiga.denise.bplcon0 = 0x6000; // 6 planes, EHB

        assert_eq!(amiga.denise.resolve_color_rgb12(0x25), 0x0567);

        amiga.write_custom_reg(0x106, 0x0201); // ENBPLCN3 | KILLEHB
        assert_eq!(amiga.denise.bplcon3, 0x0201);
        assert_eq!(amiga.denise.resolve_color_rgb12(0x25), 0x0ACE);
    }

    #[test]
    fn amiga_config_a500plus_uses_one_meg_chip_ram() {
        let amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500Plus,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });
        assert_eq!(amiga.model, AmigaModel::A500Plus);
        assert_eq!(amiga.memory.chip_ram.len(), 1024 * 1024);
        assert_eq!(amiga.memory.chip_ram_mask, 0x0F_FFFF);
    }

    #[test]
    fn a3000_and_a4000_models_attach_motherboard_support_chips() {
        let a3000 = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A3000,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });
        assert!(a3000.ramsey.is_some());
        assert!(a3000.fat_gary.is_some());
        assert!(a3000.dmac.is_some());

        let a4000 = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A4000,
            chipset: AmigaChipset::Aga,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });
        assert!(a4000.ramsey.is_some());
        assert!(a4000.fat_gary.is_some());
        assert!(a4000.dmac.is_none());
    }

    #[test]
    fn dmac_irq_pending_does_not_directly_raise_paula_exter() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A3000,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });

        let dmac = amiga.dmac.as_mut().expect("A3000 should expose SDMAC");
        dmac.write_word(0xDD_0040, 0x0018); // SASR = COMMAND
        dmac.write_word(0xDD_0042, 0x0000); // SCMD = RESET -> WD INT
        assert!(!dmac.irq_pending(), "INTEN gate should keep INT_P low");

        dmac.write_word(0xDD_000A, 0x0004); // CNTR.INTEN
        assert!(dmac.irq_pending(), "SDMAC should report INT_P once enabled");

        amiga.tick();
        assert_eq!(
            amiga.paula.intreq & (1 << 13),
            0,
            "A3000 SDMAC IRQ should not be wired straight to Paula EXTER"
        );
    }

    #[test]
    fn dmac_irq_pending_routes_to_paula_ports_for_level_2_dispatch() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A3000,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });

        let dmac = amiga.dmac.as_mut().expect("A3000 should expose SDMAC");
        dmac.write_word(0xDD_0040, 0x0018); // SASR = COMMAND
        dmac.write_word(0xDD_0042, 0x0000); // SCMD = RESET -> WD INT
        dmac.write_word(0xDD_000A, 0x0004); // CNTR.INTEN
        assert!(dmac.irq_pending(), "SDMAC should report INT_P once enabled");

        assert_eq!(
            super::motherboard_paula_intreq_bits(amiga.model, amiga.dmac.as_ref()),
            0x0008
        );
        assert_eq!(
            super::effective_paula_intreq(amiga.model, &amiga.paula, amiga.dmac.as_ref()),
            0x0008
        );
        assert_eq!(
            super::motherboard_cpu_irq_level(amiga.model, amiga.dmac.as_ref()),
            0
        );
        amiga.paula.intena = 0x4008;

        let mut bus = AmigaBusWrapper {
            model: amiga.model,
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
            cia_a_cra_sp_prev: &mut amiga.cia_a_cra_sp_prev,
            motherboard_external_irq_prev: &mut amiga.motherboard_external_irq_prev,
            gayle: &mut amiga.gayle,
            dmac: &mut amiga.dmac,
            ramsey: &mut amiga.ramsey,
            fat_gary: &mut amiga.fat_gary,
            gary: &amiga.gary,
            buster: &mut amiga.buster,
            super_buster: &mut amiga.super_buster,
            bplcon0_denise_pending: &mut amiga.bplcon0_denise_pending,
            ddfstrt_pending: &mut amiga.ddfstrt_pending,
            ddfstop_pending: &mut amiga.ddfstop_pending,
            color_pending: &mut amiga.color_pending,
            cpu_pc: 0,
            mouse_x: &mut amiga.mouse_x,
            mouse_y: &mut amiga.mouse_y,
            joy1dat: amiga.joy1dat,
            input_buttons: amiga.input_buttons,
            serdatr: amiga.serdatr,
            rtc_control: &mut amiga.rtc_control,
            rtc_time: &mut amiga.rtc_time,
            rtc_time_latched: &mut amiga.rtc_time_latched,
        };

        assert_eq!(M68kBus::poll_ipl(&mut bus), 2);
        assert_eq!(
            M68kBus::poll_cycle(
                &mut bus,
                0x00DF_F01E,
                FunctionCode::SupervisorData,
                true,
                true,
                None,
            ),
            BusStatus::Ready(0x0008)
        );
        assert_eq!(
            M68kBus::poll_cycle(
                &mut bus,
                0x00FF_FFFF,
                FunctionCode::InterruptAck,
                true,
                true,
                None,
            ),
            BusStatus::Ready(26)
        );
    }

    #[test]
    fn motherboard_resource_registers_read_back_from_support_chips() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A3000,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });

        let mut bus = AmigaBusWrapper {
            model: amiga.model,
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
            cia_a_cra_sp_prev: &mut amiga.cia_a_cra_sp_prev,
            motherboard_external_irq_prev: &mut amiga.motherboard_external_irq_prev,
            gayle: &mut amiga.gayle,
            dmac: &mut amiga.dmac,
            ramsey: &mut amiga.ramsey,
            fat_gary: &mut amiga.fat_gary,
            gary: &amiga.gary,
            buster: &mut amiga.buster,
            super_buster: &mut amiga.super_buster,
            bplcon0_denise_pending: &mut amiga.bplcon0_denise_pending,
            ddfstrt_pending: &mut amiga.ddfstrt_pending,
            ddfstop_pending: &mut amiga.ddfstop_pending,
            color_pending: &mut amiga.color_pending,
            cpu_pc: 0,
            mouse_x: &mut amiga.mouse_x,
            mouse_y: &mut amiga.mouse_y,
            joy1dat: amiga.joy1dat,
            input_buttons: amiga.input_buttons,
            serdatr: amiga.serdatr,
            rtc_control: &mut amiga.rtc_control,
            rtc_time: &mut amiga.rtc_time,
            rtc_time_latched: &mut amiga.rtc_time_latched,
        };

        let ramsey_rev = M68kBus::poll_cycle(
            &mut bus,
            0x00DE_0043,
            FunctionCode::SupervisorData,
            true,
            false,
            None,
        );
        let coldboot = M68kBus::poll_cycle(
            &mut bus,
            0x00DE_0002,
            FunctionCode::SupervisorData,
            true,
            false,
            None,
        );

        assert_eq!(
            ramsey_rev,
            BusStatus::Ready(u16::from(commodore_ramsey::REVISION_04))
        );
        assert_eq!(
            coldboot,
            BusStatus::Ready(u16::from(commodore_fat_gary::FatGary::COLDBOOT_FLAG))
        );
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
    fn mouse_delta_updates_joy0dat() {
        let mut amiga = Amiga::new(dummy_kickstart());
        amiga.push_mouse_delta(10, 20);
        let joy0dat = read_custom_word_via_cpu_bus(&mut amiga, 0x00A);
        assert_eq!(joy0dat & 0xFF, 10, "X counter");
        assert_eq!(joy0dat >> 8, 20, "Y counter");
    }

    #[test]
    fn mouse_delta_wraps_counters() {
        let mut amiga = Amiga::new(dummy_kickstart());
        amiga.push_mouse_delta(-1, -1);
        let joy0dat = read_custom_word_via_cpu_bus(&mut amiga, 0x00A);
        assert_eq!(joy0dat & 0xFF, 0xFF, "X wraps to 255");
        assert_eq!(joy0dat >> 8, 0xFF, "Y wraps to 255");
    }

    #[test]
    fn mouse_buttons_affect_cia_a_and_potgor() {
        let mut amiga = Amiga::new(dummy_kickstart());

        // Press LMB: CIA-A PRA bit 6 = 0 (active-low)
        amiga.set_mouse_button(0, true);
        assert_eq!(amiga.cia_a.external_a & 0x40, 0, "LMB pressed => bit 6 low");

        // Release LMB
        amiga.set_mouse_button(0, false);
        assert_eq!(amiga.cia_a.external_a & 0x40, 0x40, "LMB released => bit 6 high");

        // Press RMB: POTGOR bit 10 = 0 (active-low)
        amiga.set_mouse_button(1, true);
        let potgor = read_custom_word_via_cpu_bus(&mut amiga, 0x016);
        assert_eq!(potgor & (1 << 10), 0, "RMB pressed => POTGOR bit 10 low");
    }

    #[test]
    fn joytest_presets_counters() {
        let mut amiga = Amiga::new(dummy_kickstart());
        amiga.write_custom_reg(0x036, 0xAB_CD);
        let joy0dat = read_custom_word_via_cpu_bus(&mut amiga, 0x00A);
        assert_eq!(joy0dat, 0xAB_CD);
    }

    #[test]
    fn ecs_latches_beamcon0_and_diwhigh_writes() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });

        // ECS display window vertical range 0x100..0x120 and horizontal range
        // 0x110..0x150 (DIWHIGH supplies V8 and H8 bits).
        amiga.write_custom_reg(0x08E, 0x0010); // VSTART=$00, HSTART=$10
        amiga.write_custom_reg(0x090, 0x2050); // VSTOP =$20, HSTOP =$50
        amiga.write_custom_reg(0x1E4, 0x2121); // stop H8/V8 + start H8/V8
        amiga.agnus.ddfstrt = 100;

        // hpos 136 => beam_x 272 (=0x110), inside ECS horizontal window
        // Raster coords: fb_x = 272*4 = 1088, fb_y = 256*2 = 512
        assert_eq!(amiga.beam_to_fb(256, 136), Some((1088, 512)));
        // Last visible CCK before HSTOP (beam_x=334)
        // Raster coords: fb_x = 334*4 = 1336, fb_y = 287*2 = 574
        assert_eq!(amiga.beam_to_fb(287, 167), Some((1336, 574)));
        // Horizontal clipping via DIWHIGH.H8
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });

        amiga.write_custom_reg(0x1C0, 0x0033);
        amiga.write_custom_reg(0x1C8, 0x0123);

        assert_eq!(amiga.agnus.htotal(), 0x0033);
        assert_eq!(amiga.agnus.vtotal(), 0x0123);
    }

    #[test]
    fn ocs_custom_reads_for_ecs_beam_registers_return_zero() {
        let mut amiga = Amiga::new(dummy_kickstart());
        amiga.write_custom_reg(0x106, 0x0201);
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

        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x106), 0);
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });
        amiga.write_custom_reg(0x106, 0x0201);
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

        assert_eq!(read_custom_word_via_cpu_bus(&mut amiga, 0x106), 0x0201);
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
    fn deniseid_read_reflects_chipset_generation() {
        let mut ocs = Amiga::new(dummy_kickstart());
        assert_eq!(read_custom_word_via_cpu_bus(&mut ocs, 0x07C), 0xFFFF);

        let mut ecs = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });
        assert_eq!(read_custom_word_via_cpu_bus(&mut ecs, 0x07C), 0x00FC);

        let mut aga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A1200,
            chipset: AmigaChipset::Aga,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });
        assert_eq!(read_custom_word_via_cpu_bus(&mut aga, 0x07C), 0x00F8);
    }

    #[test]
    fn ntsc_region_wraps_beam_at_262_lines() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ocs,
            region: AmigaRegion::Ntsc,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });

        // Verify NTSC frame timing: 262 lines x 227 CCKs.
        assert_eq!(amiga.agnus.lines_per_frame, 262);

        // Advance to line 261 (last NTSC line).
        amiga.agnus.vpos = 261;
        amiga.agnus.hpos = 226; // last CCK of the line
        amiga.agnus.tick_cck();
        // Should wrap to line 0 (frame boundary).
        assert_eq!(amiga.agnus.vpos, 0);
        assert_eq!(amiga.agnus.hpos, 0);

        // Verify raster buffer is NTSC-sized (524 rows).
        assert_eq!(amiga.denise.framebuffer_raster.len(), (1816 * 524) as usize);
    }

    #[test]
    fn pal_region_wraps_beam_at_312_lines() {
        let mut amiga = Amiga::new(dummy_kickstart());

        assert_eq!(amiga.agnus.lines_per_frame, 312);

        amiga.agnus.vpos = 311;
        amiga.agnus.hpos = 226;
        amiga.agnus.tick_cck();
        assert_eq!(amiga.agnus.vpos, 0);
        assert_eq!(amiga.agnus.hpos, 0);

        // PAL line 261 should NOT wrap (it's mid-frame for PAL).
        amiga.agnus.vpos = 261;
        amiga.agnus.hpos = 226;
        amiga.agnus.tick_cck();
        assert_eq!(amiga.agnus.vpos, 262);
        assert_eq!(amiga.agnus.hpos, 0);

        // Verify raster buffer is PAL-sized (624 rows).
        assert_eq!(amiga.denise.framebuffer_raster.len(), (1816 * 624) as usize);
    }

    #[test]
    fn ecs_varbeamen_applies_programmed_beam_wrap_limits() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
    fn ecs_beam_to_fb_visible_with_diwhigh_and_varbeamen_flags() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });

        // ECS display window vertical range 0x100..0x120 and horizontal range
        // 0x110..0x150 (DIWHIGH supplies V8 and H8 bits).
        amiga.write_custom_reg(0x08E, 0x0010); // VSTART=$00, HSTART=$10
        amiga.write_custom_reg(0x090, 0x2050); // VSTOP =$20, HSTOP =$50
        amiga.write_custom_reg(0x1E4, 0x2121); // stop H8/V8 + start H8/V8

        // Raster coords: beam_x=272, fb_x=1088, fb_y=512
        assert_eq!(amiga.beam_to_fb(256, 136), Some((1088, 512)));

        // With raster framebuffer, all visible (non-blanked) positions have
        // coordinates regardless of VARBEAMEN/HARDDIS/VARVBEN flags.
        amiga.write_custom_reg(0x1DC, commodore_agnus_ecs::BEAMCON0_VARBEAMEN);
        assert_eq!(amiga.beam_to_fb(256, 136), Some((1088, 512)));

        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_VARBEAMEN | commodore_agnus_ecs::BEAMCON0_HARDDIS,
        );
        assert_eq!(amiga.beam_to_fb(256, 136), Some((1088, 512)));

        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_VARBEAMEN | commodore_agnus_ecs::BEAMCON0_VARVBEN,
        );
        assert_eq!(amiga.beam_to_fb(256, 136), Some((1088, 512)));
    }

    #[test]
    fn ecs_beam_sync_state_reports_programmed_sync_windows() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
                fb_coords: Some((280, 210)),
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
                fb_coords: Some((280, 210)),
            }
        );
        assert_eq!(amiga.agnus.hpos, 36); // Beam advanced after the sampled CCK.
    }

    #[test]
    fn ecs_beam_debug_snapshot_reports_blanken_and_sync_polarity_pin_states() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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

        // At (105, 35): both hsync and vsync are active.
        // XOR composite sync: true ^ true = false (sync cancels when both active).
        let true_polarity_sync = amiga.beam_debug_snapshot_at(105, 35);
        assert_eq!(
            true_polarity_sync.composite_sync,
            BeamCompositeSyncDebug {
                active: false,
                redirected: true,
                mode: BeamCompositeSyncMode::VariableXorSync,
            }
        );
        assert_eq!(
            true_polarity_sync.pins,
            BeamPinState {
                hsync_high: true,
                vsync_high: true,
                csync_high: false,
                blank_active: false,
            }
        );

        let blank_redirected = amiga.beam_debug_snapshot_at(60, 10);
        assert_eq!(
            blank_redirected.composite_sync,
            BeamCompositeSyncDebug {
                active: false,
                redirected: true,
                mode: BeamCompositeSyncMode::VariableXorSync,
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            Some((1088, 512))
        );

        amiga.write_custom_reg(0x1DC, commodore_agnus_ecs::BEAMCON0_VARBEAMEN);
        amiga.agnus.vpos = 256;
        amiga.agnus.hpos = 136;
        tick_one_cck(&mut amiga);
        let hard_stopped = amiga.current_beam_debug_snapshot();
        assert_eq!(hard_stopped.vpos, 256);
        assert_eq!(hard_stopped.hpos_cck, 136);
        assert_eq!(hard_stopped.fb_coords, Some((1088, 512)));

        amiga.write_custom_reg(
            0x1DC,
            commodore_agnus_ecs::BEAMCON0_VARBEAMEN | commodore_agnus_ecs::BEAMCON0_HARDDIS,
        );
        amiga.agnus.vpos = 256;
        amiga.agnus.hpos = 136;
        tick_one_cck(&mut amiga);
        assert_eq!(
            amiga.current_beam_debug_snapshot().fb_coords,
            Some((1088, 512))
        );
    }

    #[test]
    fn ecs_current_beam_pin_state_uses_latched_snapshot_pins() {
        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
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

    #[test]
    fn observable_cpu_and_system_state() {
        use emu_core::Observable;
        use emu_core::Value;

        let rom = vec![0u8; 256 * 1024];
        let amiga = Amiga::new(rom);

        // CPU register queries
        assert!(amiga.query("cpu.pc").is_some());
        assert!(amiga.query("cpu.sr").is_some());
        assert!(amiga.query("cpu.d0").is_some());
        assert!(amiga.query("cpu.flags.z").is_some());

        // Agnus beam position
        assert!(matches!(amiga.query("agnus.vpos"), Some(Value::U16(_))));
        assert!(matches!(amiga.query("agnus.hpos"), Some(Value::U16(_))));
        assert!(matches!(amiga.query("agnus.bplcon0"), Some(Value::U16(0))));
        assert!(matches!(amiga.query("agnus.beamcon0"), Some(Value::U16(0))));
        assert!(matches!(amiga.query("agnus.diwhigh"), Some(Value::U16(0))));
        assert!(matches!(
            amiga.query("agnus.diwhigh_written"),
            Some(Value::Bool(false))
        ));
        assert!(matches!(
            amiga.query("agnus.mode.varbeamen"),
            Some(Value::Bool(false))
        ));
        assert!(matches!(
            amiga.query("agnus.mode.harddis"),
            Some(Value::Bool(false))
        ));

        // Denise palette
        assert!(matches!(
            amiga.query("denise.palette.0"),
            Some(Value::U16(_))
        ));
        assert!(amiga.query("denise.palette.31").is_some());
        assert!(amiga.query("denise.palette.32").is_none());
        assert!(matches!(amiga.query("denise.bplcon0"), Some(Value::U16(0))));
        assert!(matches!(amiga.query("denise.bplcon3"), Some(Value::U16(0))));
        assert!(matches!(
            amiga.query("denise.mode.shres"),
            Some(Value::Bool(false))
        ));
        assert!(matches!(
            amiga.query("denise.mode.killehb"),
            Some(Value::Bool(false))
        ));

        // Paula interrupt registers
        assert!(matches!(amiga.query("paula.intena"), Some(Value::U16(_))));
        assert!(matches!(amiga.query("paula.intreq"), Some(Value::U16(_))));
        assert!(matches!(amiga.query("paula.adkcon"), Some(Value::U16(_))));

        // Paula audio channels
        assert!(matches!(
            amiga.query("paula.audio.0.period"),
            Some(Value::U16(_))
        ));
        assert!(matches!(
            amiga.query("paula.audio.0.volume"),
            Some(Value::U8(_))
        ));
        assert!(matches!(
            amiga.query("paula.audio.3.sample"),
            Some(Value::I8(_))
        ));
        assert!(amiga.query("paula.audio.4.period").is_none());

        // CIA
        assert!(matches!(amiga.query("cia_a.timer_a"), Some(Value::U16(_))));
        assert!(matches!(amiga.query("cia_b.cra"), Some(Value::U8(_))));

        // Memory (ROM area: kickstart mirror at $F80000)
        assert!(matches!(amiga.query("memory.0xF80000"), Some(Value::U8(_))));
        assert!(matches!(amiga.query("memory.$000000"), Some(Value::U8(_))));

        // Master clock
        assert!(matches!(amiga.query("master_clock"), Some(Value::U64(_))));

        // Unknown paths
        assert!(amiga.query("nonexistent").is_none());
        assert!(amiga.query("agnus.nonexistent").is_none());
    }

    #[test]
    fn observable_query_paths_are_concrete_for_cpu_and_denise() {
        use emu_core::Observable;

        let amiga = Amiga::new(dummy_kickstart());
        let paths = amiga.query_paths();

        assert!(paths.contains(&"cpu.pc"));
        assert!(paths.contains(&"cpu.flags.z"));
        assert!(!paths.contains(&"cpu.<68000_paths>"));

        assert!(paths.contains(&"agnus.dmacon"));
        assert!(paths.contains(&"agnus.blitter_busy"));
        assert!(paths.contains(&"agnus.blitter_ccks_remaining"));

        assert!(paths.contains(&"denise.palette.0"));
        assert!(paths.contains(&"denise.palette.31"));
        assert!(!paths.contains(&"denise.palette.<0-31>"));
    }

    #[test]
    fn observable_denise_ecs_mode_state_reflects_bplcon_register_bits() {
        use emu_core::Observable;
        use emu_core::Value;

        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });

        amiga.write_custom_reg(0x100, 0x0070);
        amiga.write_custom_reg(0x106, 0x0231);

        assert_eq!(amiga.query("denise.bplcon0"), Some(Value::U16(0)));
        assert_eq!(amiga.query("denise.bplcon3"), Some(Value::U16(0x0231)));
        assert_eq!(amiga.query("denise.mode.shres"), Some(Value::Bool(false)));
        assert_eq!(amiga.query("denise.mode.bplhwrm"), Some(Value::Bool(false)));
        assert_eq!(amiga.query("denise.mode.sprhwrm"), Some(Value::Bool(false)));
        assert_eq!(amiga.query("denise.mode.killehb"), Some(Value::Bool(true)));
        assert_eq!(
            amiga.query("denise.mode.border_blank"),
            Some(Value::Bool(true))
        );
        assert_eq!(
            amiga.query("denise.mode.border_opaque"),
            Some(Value::Bool(false))
        );

        tick_one_cck(&mut amiga);
        tick_one_cck(&mut amiga);

        assert_eq!(amiga.query("denise.bplcon0"), Some(Value::U16(0x0070)));
        assert_eq!(amiga.query("denise.mode.shres"), Some(Value::Bool(true)));
        assert_eq!(amiga.query("denise.mode.bplhwrm"), Some(Value::Bool(true)));
        assert_eq!(amiga.query("denise.mode.sprhwrm"), Some(Value::Bool(true)));
    }

    #[test]
    fn observable_agnus_ecs_state_reflects_latched_register_bits() {
        use emu_core::Observable;
        use emu_core::Value;

        let mut amiga = Amiga::new_with_config(AmigaConfig {
            model: AmigaModel::A500,
            chipset: AmigaChipset::Ecs,
            region: AmigaRegion::Pal,
            kickstart: dummy_kickstart(),
            slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
        });

        amiga.write_custom_reg(0x100, 0x1070);
        amiga.write_custom_reg(0x08E, 0x1234);
        amiga.write_custom_reg(0x090, 0x5678);
        amiga.write_custom_reg(0x092, 0x0038);
        amiga.write_custom_reg(0x094, 0x00D0);
        // 0x559F sets all BEAMCON0 mode bits except VARVSYEN (bit 9) under the
        // corrected WinUAE/HRM bit layout. This replaces the old 0xAB3E value
        // which matched a shifted-by-one bit numbering.
        amiga.write_custom_reg(0x1DC, 0x559F);
        amiga.write_custom_reg(0x1C0, 0x0033);
        amiga.write_custom_reg(0x1C2, 0x0044);
        amiga.write_custom_reg(0x1C4, 0x0011);
        amiga.write_custom_reg(0x1C6, 0x0022);
        amiga.write_custom_reg(0x1C8, 0x0123);
        amiga.write_custom_reg(0x1CA, 0x0234);
        amiga.write_custom_reg(0x1CC, 0x0044);
        amiga.write_custom_reg(0x1CE, 0x0055);
        amiga.write_custom_reg(0x1DE, 0x0066);
        amiga.write_custom_reg(0x1E0, 0x0177);
        amiga.write_custom_reg(0x1E4, 0x89AB);

        assert_eq!(amiga.query("agnus.bplcon0"), Some(Value::U16(0x1070)));
        assert_eq!(amiga.query("agnus.diwstrt"), Some(Value::U16(0x1234)));
        assert_eq!(amiga.query("agnus.diwstop"), Some(Value::U16(0x5678)));
        assert_eq!(amiga.query("agnus.ddfstrt"), Some(Value::U16(0)));
        assert_eq!(amiga.query("agnus.ddfstop"), Some(Value::U16(0)));
        assert_eq!(amiga.query("agnus.dmacon"), Some(Value::U16(0)));
        assert_eq!(amiga.query("agnus.blitter_busy"), Some(Value::Bool(false)));
        assert_eq!(
            amiga.query("agnus.blitter_ccks_remaining"),
            Some(Value::U32(0))
        );
        assert_eq!(amiga.query("agnus.beamcon0"), Some(Value::U16(0x559F)));
        assert_eq!(amiga.query("agnus.htotal"), Some(Value::U16(0x0033)));
        assert_eq!(amiga.query("agnus.hsstop"), Some(Value::U16(0x0044)));
        assert_eq!(amiga.query("agnus.hbstrt"), Some(Value::U16(0x0011)));
        assert_eq!(amiga.query("agnus.hbstop"), Some(Value::U16(0x0022)));
        assert_eq!(amiga.query("agnus.vtotal"), Some(Value::U16(0x0123)));
        assert_eq!(amiga.query("agnus.vsstop"), Some(Value::U16(0x0234)));
        assert_eq!(amiga.query("agnus.vbstrt"), Some(Value::U16(0x0044)));
        assert_eq!(amiga.query("agnus.vbstop"), Some(Value::U16(0x0055)));
        assert_eq!(amiga.query("agnus.hsstrt"), Some(Value::U16(0x0066)));
        assert_eq!(amiga.query("agnus.vsstrt"), Some(Value::U16(0x0177)));
        assert_eq!(amiga.query("agnus.diwhigh"), Some(Value::U16(0x89AB)));
        assert_eq!(
            amiga.query("agnus.diwhigh_written"),
            Some(Value::Bool(true))
        );
        assert_eq!(amiga.query("agnus.mode.varbeamen"), Some(Value::Bool(true)));
        assert_eq!(amiga.query("agnus.mode.varvben"), Some(Value::Bool(true)));
        assert_eq!(amiga.query("agnus.mode.varvsyen"), Some(Value::Bool(false)));
        assert_eq!(amiga.query("agnus.mode.varhsyen"), Some(Value::Bool(true)));
        assert_eq!(amiga.query("agnus.mode.cscben"), Some(Value::Bool(true)));
        assert_eq!(amiga.query("agnus.mode.varcsyen"), Some(Value::Bool(true)));
        assert_eq!(amiga.query("agnus.mode.harddis"), Some(Value::Bool(true)));
        assert_eq!(amiga.query("agnus.mode.blanken"), Some(Value::Bool(true)));
        assert_eq!(amiga.query("agnus.mode.csytrue"), Some(Value::Bool(true)));
        assert_eq!(amiga.query("agnus.mode.vsytrue"), Some(Value::Bool(true)));
        assert_eq!(amiga.query("agnus.mode.hsytrue"), Some(Value::Bool(true)));

        tick_one_cck(&mut amiga);
        tick_one_cck(&mut amiga);

        assert_eq!(amiga.query("agnus.ddfstrt"), Some(Value::U16(0x0038)));
        assert_eq!(amiga.query("agnus.ddfstop"), Some(Value::U16(0x00D0)));
    }

    #[test]
    fn custom_byte_write_can_clear_low_byte_intreq_bits() {
        let mut amiga = Amiga::new(dummy_kickstart());
        amiga.paula.intreq = 0x6040;

        write_custom_byte_via_cpu_bus(&mut amiga, 0x09C, 1, 0x40);

        assert_eq!(amiga.paula.intreq, 0x6000);
    }

    #[test]
    fn custom_byte_write_can_clear_high_byte_intena_bits_without_touching_low_bits() {
        let mut amiga = Amiga::new(dummy_kickstart());
        amiga.paula.intena = 0x402C;

        write_custom_byte_via_cpu_bus(&mut amiga, 0x09A, 0, 0x40);

        assert_eq!(amiga.paula.intena, 0x002C);
    }

    #[test]
    fn blitter_irq_debug_records_first_assert_source() {
        const REG_BLTCON0: u16 = 0x040;
        const REG_BLTDPTH: u16 = 0x054;
        const REG_BLTDPTL: u16 = 0x056;
        const REG_BLTSIZE: u16 = 0x058;
        const REG_DMACON: u16 = 0x096;
        const DMACON_DMAEN: u16 = 0x0200;
        const DMACON_BLTEN: u16 = 0x0040;

        let mut amiga = Amiga::new(dummy_kickstart());
        amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_BLTEN);
        amiga.write_custom_reg(REG_BLTCON0, 0x0100); // D-only area blit.
        amiga.write_custom_reg(REG_BLTDPTH, 0x0000);
        amiga.write_custom_reg(REG_BLTDPTL, 0x0100);
        amiga.write_custom_reg(REG_BLTSIZE, (1 << 6) | 1);

        for _ in 0..10_000 {
            if amiga.first_blitter_irq_assert().is_some() {
                break;
            }
            amiga.tick();
        }

        let first = amiga
            .first_blitter_irq_assert()
            .expect("expected a blitter IRQ assertion");
        assert_eq!(first.source, BlitterInterruptSource::SchedulerIncremental);
        assert_eq!(first.intreq_before & 0x0040, 0);
        assert_ne!(first.intreq_after & 0x0040, 0);
        assert_eq!(
            amiga
                .blitter_irq_debug_events()
                .first()
                .map(|event| event.source),
            Some(BlitterInterruptSource::SchedulerIncremental)
        );
    }

    #[test]
    fn serial_receive_sets_rbf_and_stores_data_after_baud_countdown() {
        let mut amiga = Amiga::new(dummy_kickstart());

        // Configure baud rate: period=0 → 1 CCK per bit, 10 bits = 10 CCKs total.
        amiga.write_custom_reg(0x032, 0x0000); // SERPER: period=0, 8-bit mode
        // Enable RBF interrupt so we can verify it fires.
        amiga.paula.intena = 0xC800; // master + RBF

        // Initially TBE+TSRE set, RBF clear.
        assert_eq!(amiga.serdatr & 0x3800, 0x3000);

        // Push a byte into the receive queue.
        amiga.push_serial_byte(0x42);

        // Tick 9 CCKs — byte should still be shifting in.
        for _ in 0..9 {
            tick_one_cck(&mut amiga);
        }
        assert_eq!(
            amiga.serdatr & 0x0800,
            0,
            "RBF should not be set before countdown completes"
        );

        // Tick the 10th CCK — receive completes.
        tick_one_cck(&mut amiga);
        assert_ne!(
            amiga.serdatr & 0x0800,
            0,
            "RBF should be set after countdown completes"
        );
        assert_eq!(
            amiga.serdatr & 0x01FF,
            0x0142, // stop bit (bit 8) + data 0x42
            "received byte should be stored in SERDATR bits 8-0"
        );
        // RBF interrupt should be pending in Paula.
        assert_ne!(
            amiga.paula.intreq & 0x0800,
            0,
            "RBF interrupt should be requested"
        );
    }

    #[test]
    fn serial_receive_queues_multiple_bytes() {
        let mut amiga = Amiga::new(dummy_kickstart());
        amiga.write_custom_reg(0x032, 0x0000); // period=0, 10 CCKs per byte

        amiga.push_serial_byte(0xAA);
        amiga.push_serial_byte(0x55);

        // First byte: 10 CCKs.
        for _ in 0..10 {
            tick_one_cck(&mut amiga);
        }
        assert_eq!(amiga.serdatr & 0x00FF, 0xAA);
        assert_ne!(amiga.serdatr & 0x0800, 0); // RBF set

        // Second byte starts immediately on the next tick, finishes after 10 more.
        for _ in 0..10 {
            tick_one_cck(&mut amiga);
        }
        assert_eq!(amiga.serdatr & 0x00FF, 0x55);
    }
}
