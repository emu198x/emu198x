//! Runtime wrapper around an Amiga machine variant.
//!
//! `AmigaRuntime<M: AmigaMachine>` implements `MachineCore` so the
//! shell can drive any Amiga variant through a common interface. The
//! runtime owns variant-agnostic state (configuration, time, frame counters,
//! audio buffers, RGBA framebuffer, boot heuristic), and delegates
//! per-frame ticking, framebuffer access, audio sampling,
//! and chip-state queries to the machine through the `AmigaMachine`
//! trait.
//!
//! Per-concern siblings — `queries.rs`, `snapshot.rs`, `input.rs` —
//! are also generic over `M` and own the pieces of the surface that
//! don't belong in lifecycle. `variants.rs` carries the trait + the
//! per-variant impls + the public type aliases (`AmigaOcsRuntime`).

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, ResetKind, RunResult,
    StopReason,
};
use format_commodore_amiga_adf::Adf;
use machine_commodore_amiga_a1200::AmigaA1200;
use machine_commodore_amiga_ecs::{
    AmigaEcs, AudioControls as EcsAudioControls, PaulaChannel as EcsPaulaChannel,
};
use machine_commodore_amiga_ocs::{
    AmigaOcs, AudioControls, FB_HEIGHT, FB_WIDTH, PaulaChannel, RamConfig,
};

use crate::input::apply_input_event;
use crate::live_access::AmigaLiveAccess;
use crate::snapshot;
use crate::variants::AmigaMachine;
use crate::{
    Accelerator, AmigaConfig, ChipsetKind, ECS_AGA_CHIP_RAM_BYTES, FAT_AGNUS_CHIP_RAM_BYTES,
    FATTER_AGNUS_CHIP_RAM_BYTES, KIB, Model, OCS_AGNUS_CHIP_RAM_BYTES, profile_for,
};

pub(crate) const KICKSTART_ROM_ID: &str = "commodore-amiga-kickstart-rom";
pub(crate) const A1000_BOOTSTRAP_ROM_ID: &str = "commodore-amiga-a1000-bootstrap-rom";
const VALID_KICKSTART_SIZES: &[usize] = &[256 * 1024, 512 * 1024];
const VALID_A1000_BOOTSTRAP_SIZES: &[usize] = &[64 * 1024];
pub(crate) const AUDIO_SAMPLE_RATE_HZ: u32 = 48_000;
pub(crate) const AUDIO_CHANNELS: u8 = 2;

/// Machine framebuffer width (= `FB_WIDTH`). Re-exported for host
/// integrations that size their output buffers without pulling in
/// the machine crate directly. Tracks the OCS chipset framebuffer
/// today; future RTG variants will publish per-slot dimensions.
pub const DISPLAY_WIDTH: u32 = FB_WIDTH;
/// Machine framebuffer height (= `FB_HEIGHT`).
pub const DISPLAY_HEIGHT: u32 = FB_HEIGHT;

/// Firmware-backed Amiga runtime. Generic over an `M: AmigaMachine`
/// so ECS / AGA / SAGA / Vampire / PiStorm / RTG variants can plug
/// in by adding new `impl AmigaMachine for X` blocks plus a public
/// type alias next to `AmigaOcsRuntime` in `variants.rs`.
pub struct AmigaRuntime<M: AmigaMachine> {
    profile: MachineProfile,
    /// Immutable machine-construction intent. Held separately from the
    /// machine snapshot so reset and restore rebuild the same region, RAM,
    /// processor, and accelerator configuration.
    config: AmigaConfig,
    // pub(crate) so the `cpu_trace` sibling module's `tick_traced` can
    // take disjoint field borrows of `machine` (read) and `cpu_trace`
    // (write) in one method — accessor methods would borrow all of
    // `self` and block the split.
    pub(crate) machine: M,
    time: MachineTime,
    firmware_rom: Vec<u8>,
    floppy0_bytes: Option<Vec<u8>>,
    rgba_framebuffer: Vec<u8>,
    frame_count: u64,
    /// Pixel counts from the most recently emitted frame — drives the
    /// `boot.*` query set.
    non_black_pixels: u32,
    non_white_pixels: u32,
    first_active_row: Option<u32>,
    /// Fractional 48 kHz resampler phase. The source advances once
    /// per machine tick (master/4); audio output itself only changes
    /// on CCK boundaries, but sampling at the finer runtime tick keeps
    /// this phase stable across frame boundaries.
    audio_sample_accumulator: u64,
    audio_buffer: Vec<f32>,
    /// Tick rate in Hz (= 2 × cck_hz). Cached at construction so the
    /// audio resampler doesn't query the machine every tick.
    tick_hz: u64,
    /// Instruction-boundary CPU trace, armed via the MCP `cpu_trace_*`
    /// tools and captured by `tick_traced`. See [`crate::cpu_trace`].
    /// `pub(crate)` for the same disjoint-borrow reason as `machine`.
    pub(crate) cpu_trace: crate::cpu_trace::CpuTrace,
    /// Paula's analog output filter chain (RC low-pass + switchable LED
    /// filter + DC-blocking high-pass), applied to each host sample.
    /// Configured from `model`; transient IIR state, not snapshotted.
    audio_filter: crate::audio_filter::AmigaAudioFilter,
}

// =====================================================================
// Generic methods — work for every `M: AmigaMachine`.
// =====================================================================

impl<M: AmigaMachine> AmigaRuntime<M> {
    /// Read-only access to the wrapped machine.
    #[must_use]
    pub fn machine(&self) -> &M {
        &self.machine
    }

    /// Mutable access to the wrapped machine. Only for tests /
    /// integrations that need to drive the tick loop directly (e.g.
    /// autoconfig boot tests that run the machine outside `run_until`).
    pub fn machine_mut(&mut self) -> &mut M {
        &mut self.machine
    }

    /// Active model represented by the canonical construction configuration.
    ///
    /// The model selects profile metadata and machine internals including the
    /// chipset stack, video region, processor, RAM defaults, and accelerator.
    #[must_use]
    pub fn model(&self) -> Model {
        self.config.model()
    }

    /// Canonical immutable machine-construction configuration.
    #[must_use]
    pub const fn config(&self) -> AmigaConfig {
        self.config
    }

    /// Copy the machine's ARGB framebuffer into the RGBA frame
    /// packet buffer the shell expects. ARGB → RGBA is a simple
    /// byte reorder. Side-effect: refreshes the pixel-based boot
    /// heuristic (`non_black_pixels` / `non_white_pixels` /
    /// `first_active_row`) so the next `boot.detected` query reads
    /// consistent values.
    fn update_rgba_framebuffer(&mut self) {
        let fb = self.machine.chipset_framebuffer();
        let expected = (DISPLAY_WIDTH * DISPLAY_HEIGHT) as usize;
        debug_assert_eq!(fb.len(), expected);
        if self.rgba_framebuffer.len() != expected * 4 {
            self.rgba_framebuffer.resize(expected * 4, 0);
        }

        let mut non_black = 0u32;
        let mut non_white = 0u32;
        let mut first_active_row: Option<u32> = None;

        for (i, &pixel) in fb.iter().enumerate() {
            let base = i * 4;
            self.rgba_framebuffer[base] = ((pixel >> 16) & 0xFF) as u8; // R
            self.rgba_framebuffer[base + 1] = ((pixel >> 8) & 0xFF) as u8; // G
            self.rgba_framebuffer[base + 2] = (pixel & 0xFF) as u8; // B
            self.rgba_framebuffer[base + 3] = ((pixel >> 24) & 0xFF) as u8; // A

            let rgb = pixel & 0x00FF_FFFF;
            if rgb != 0 {
                non_black = non_black.saturating_add(1);
                if first_active_row.is_none() {
                    first_active_row = Some(i as u32 / DISPLAY_WIDTH);
                }
            }
            if rgb != 0x00FF_FFFF {
                non_white = non_white.saturating_add(1);
            }
        }

        self.non_black_pixels = non_black;
        self.non_white_pixels = non_white;
        self.first_active_row = first_active_row;
    }

    // -----------------------------------------------------------------
    // pub(crate) accessors for the queries / snapshot sibling modules
    // -----------------------------------------------------------------

    /// Frame counter. Used by the `amiga.machine.frame_count` query
    /// and by the snapshot envelope.
    #[must_use]
    pub(crate) fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Non-black pixel count from the most recent frame. Drives the
    /// boot-status heuristic and the snapshot envelope.
    #[must_use]
    pub(crate) fn non_black_pixels(&self) -> u32 {
        self.non_black_pixels
    }

    /// Non-white pixel count from the most recent frame.
    #[must_use]
    pub(crate) fn non_white_pixels(&self) -> u32 {
        self.non_white_pixels
    }

    /// Topmost active scanline from the most recent frame.
    #[must_use]
    pub(crate) fn first_active_row(&self) -> Option<u32> {
        self.first_active_row
    }

    /// Inserted DF0 bytes (if any) — used by the snapshot envelope so
    /// a restore can re-mount the same image.
    #[must_use]
    pub(crate) fn floppy0_bytes(&self) -> Option<&[u8]> {
        self.floppy0_bytes.as_deref()
    }

    /// Firmware used to construct this runtime's machine. Snapshot restore
    /// builds its candidate machine from the same immutable image.
    #[must_use]
    pub(crate) fn firmware_rom(&self) -> &[u8] {
        &self.firmware_rom
    }

    /// 48 kHz resampler phase. Held in the snapshot so a restore picks
    /// up sampling at exactly the same fractional offset.
    #[must_use]
    pub(crate) fn audio_sample_accumulator(&self) -> u64 {
        self.audio_sample_accumulator
    }

    /// Current machine time, exposed by name distinct from the
    /// `MachineCore::time` trait method so internal modules don't have
    /// to import the trait.
    #[must_use]
    pub(crate) const fn time_value(&self) -> MachineTime {
        self.time
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn set_config(&mut self, config: AmigaConfig) {
        self.config = config;
    }

    /// Replace the live machine after an independently restored candidate has
    /// passed all snapshot validation.
    pub(crate) fn replace_machine(&mut self, machine: M) {
        self.machine = machine;
    }

    pub(crate) fn set_frame_count(&mut self, frame_count: u64) {
        self.frame_count = frame_count;
    }

    pub(crate) fn set_non_black_pixels(&mut self, count: u32) {
        self.non_black_pixels = count;
    }

    pub(crate) fn set_non_white_pixels(&mut self, count: u32) {
        self.non_white_pixels = count;
    }

    pub(crate) fn set_first_active_row(&mut self, row: Option<u32>) {
        self.first_active_row = row;
    }

    pub(crate) fn set_audio_sample_accumulator(&mut self, accum: u64) {
        self.audio_sample_accumulator = accum;
    }

    pub(crate) fn clear_floppy0_bytes(&mut self) {
        self.floppy0_bytes = None;
    }

    pub(crate) fn set_floppy0_bytes(&mut self, bytes: Option<Vec<u8>>) {
        self.floppy0_bytes = bytes;
    }

    pub(crate) fn clear_audio_buffer(&mut self) {
        self.audio_buffer.clear();
    }

    /// Rebuild the transient analog-filter chain for the configured model.
    ///
    /// IIR history is intentionally absent from snapshots, so reset and a
    /// successful restore both return it to its canonical zero-history state.
    pub(crate) fn reset_audio_filter(&mut self) {
        self.audio_filter = crate::audio_filter::AmigaAudioFilter::for_model(self.model());
    }

    #[cfg(test)]
    pub(crate) fn filter_audio_for_test(
        &mut self,
        mut left: f32,
        mut right: f32,
        led_bright: bool,
    ) -> (f32, f32) {
        self.audio_filter.apply(&mut left, &mut right, led_bright);
        (left, right)
    }

    /// Drop instruction trace entries after a successful snapshot restore.
    ///
    /// Trace data is observational runtime state rather than emulated machine
    /// state, so it must not span a restore boundary. Arm state and filters
    /// remain in place for the continuing debug session.
    pub(crate) fn clear_cpu_trace_after_restore(&mut self) {
        self.cpu_trace.clear_on_reset();
    }

    /// Repack the machine's framebuffer into the runtime's RGBA8888
    /// buffer. Called by the snapshot module after a restore so the
    /// next frame draw sees the post-snapshot contents instead of
    /// stale RGB data.
    pub(crate) fn refresh_rgba_framebuffer(&mut self) {
        self.update_rgba_framebuffer();
    }
}

// The tick funnel reads CPU state for the trace, so it needs the
// `AmigaLiveAccess` bound (every concrete Amiga machine satisfies it).
// Kept off the bulk `impl<M: AmigaMachine>` block so the bound doesn't
// cascade onto the query / snapshot siblings.
impl<M: AmigaMachine + AmigaLiveAccess> AmigaRuntime<M> {
    /// Advance the host-audio resampler after one completed Amiga system tick.
    fn sample_audio_after_tick(&mut self) {
        self.audio_sample_accumulator = self
            .audio_sample_accumulator
            .saturating_add(u64::from(AUDIO_SAMPLE_RATE_HZ));

        while self.audio_sample_accumulator >= self.tick_hz {
            self.audio_sample_accumulator -= self.tick_hz;
            let led_bright = self.machine.led_filter_engaged();
            let (mut left, mut right) = self.machine.mix_audio_stereo();
            // Paula's analog output filter chain. The LED filter's
            // resonant peak can boost slightly past unity, so clamp
            // after filtering as the line driver would.
            self.audio_filter.apply(&mut left, &mut right, led_bright);
            self.audio_buffer.push(left.clamp(-1.0, 1.0));
            self.audio_buffer.push(right.clamp(-1.0, 1.0));
        }
    }

    fn tick_and_sample_audio(&mut self) {
        // Route through the trace funnel so an armed CPU trace captures
        // every instruction boundary the run loop crosses — same path
        // the per-tick `step` / `run_until_*` tools use.
        self.tick_traced();
        self.sample_audio_after_tick();
    }

    /// Account for complete system ticks crossed by exact CPU stepping.
    ///
    /// A faster CPU can stop part-way through a system tick. In that case
    /// `completed_ticks` is zero, but the framebuffer is still refreshed
    /// because the tick's chipset phase may already have run.
    pub(crate) fn account_debug_progress(&mut self, completed_ticks: u64) {
        for _ in 0..completed_ticks {
            self.sample_audio_after_tick();
        }
        self.time = self.time.saturating_add(completed_ticks);
        self.update_rgba_framebuffer();
    }
}

impl<M: AmigaMachine + AmigaLiveAccess> MachineCore for AmigaRuntime<M> {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        // The variant rebuilds itself in place via `AmigaMachine::
        // rebuild(firmware, config)`. After the new chip stack
        // exists, re-mount any cached DF0 image, zero the time /
        // frame counters, and refresh the RGBA mirror so the next
        // frame draw sees the post-reset contents.
        self.machine.rebuild(&self.firmware_rom, self.config);
        if let Some(bytes) = self.floppy0_bytes.clone() {
            self.insert_floppy_bytes("floppy-0", &bytes)
                .expect("re-mounting cached DF0 image should not fail");
        }
        self.time = MachineTime::default();
        self.frame_count = 0;
        self.audio_sample_accumulator = 0;
        self.audio_buffer.clear();
        self.reset_audio_filter();
        // Drop pre-reset trace entries so they don't bleed into
        // post-reset analysis (arm-state + filter are kept).
        self.cpu_trace.clear_on_reset();
        self.update_rgba_framebuffer();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            if image.slot.as_ref() != "floppy-0" {
                return Err(MachineError::UnknownMediaSlot {
                    slot: image.slot.as_ref().to_owned(),
                });
            }
            if image.kind != MediaKind::Disk {
                return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
            }
            self.insert_floppy_bytes(image.slot.as_ref(), image.bytes)?;
        }
        Ok(())
    }

    fn eject_media(&mut self, slot: &str) -> Result<(), MachineError> {
        if slot != "floppy-0" {
            return Err(MachineError::UnknownMediaSlot {
                slot: slot.to_owned(),
            });
        }
        self.machine.eject_floppy0();
        self.clear_floppy0_bytes();
        Ok(())
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        // Apply queued input at the top of the run window. Keyboard,
        // mouse, and joystick are wired through the generic
        // `apply_input_event`; unknown event kinds are silently
        // dropped.
        for event in host.input_events {
            apply_input_event(&mut self.machine, event);
        }

        while self.time < target {
            // Run one frame's worth of machine ticks. The variant
            // declares its own frame length via `M::frame_ticks()` —
            // PAL OCS = 141,648 ticks; NTSC variants will return a
            // different value once they land.
            let frame_ticks = self.machine.frame_ticks();
            self.audio_buffer.clear();
            for _ in 0..frame_ticks {
                self.tick_and_sample_audio();
            }
            self.frame_count = self.frame_count.saturating_add(1);
            self.time = self.time.saturating_add(frame_ticks);
            self.update_rgba_framebuffer();

            host.frame_sink.push_frame(FramePacket {
                timestamp: self.time,
                format: emu198x_shell::PixelFormat::Rgba8888,
                width: DISPLAY_WIDTH,
                height: DISPLAY_HEIGHT,
                palette: None,
                pixels: &self.rgba_framebuffer,
            })?;

            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: AUDIO_SAMPLE_RATE_HZ,
                channels: AUDIO_CHANNELS,
                samples: &self.audio_buffer,
            })?;
        }
        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        snapshot::encode(self)
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        snapshot::decode(self, bytes)
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: command.operation_name(),
        })
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
}

/// Generic floppy-bytes insertion. Decodes the ADF, asks the variant
/// (via `AmigaMachine::insert_floppy0`) to mount it with the right
/// disk-change-pending bookkeeping for the current model, and caches
/// the bytes for snapshot replay.
impl<M: AmigaMachine> AmigaRuntime<M> {
    fn insert_floppy_bytes(&mut self, slot: &str, bytes: &[u8]) -> Result<(), MachineError> {
        let adf = Adf::from_bytes(bytes.to_vec()).map_err(|reason| MachineError::InvalidMedia {
            slot: slot.to_owned(),
            reason: reason.to_string(),
        })?;
        // Both A1000 PAL and A1000 NTSC need disk-change-pending
        // bookkeeping during the bootstrap-to-Kickstart-disk handoff.
        // Other (Kickstart-resident) variants insert without it.
        let change_pending = self.model().is_a1000();
        self.machine.insert_floppy0(adf, change_pending);
        self.floppy0_bytes = Some(bytes.to_vec());
        Ok(())
    }
}

// =====================================================================
// AmigaOcs-specific construction + audio control surface.
//
// These methods are constrained to `AmigaRuntime<AmigaOcs>` because they
// reference OCS-specific types (`RamConfig`, `AudioControls`,
// `PaulaChannel`) and OCS-specific construction helpers
// (`AmigaOcs::with_a1000_bootstrap_rom`, `AmigaOcs::with_ram_config`).
// When ECS / AGA / SAGA / Vampire variants land, each gets its own
// `impl AmigaRuntime<XxxMachine>` block here (or in `variants.rs`)
// with whatever construction shape its machine demands.
// =====================================================================

impl AmigaRuntime<AmigaOcs> {
    /// Construct a runtime from owned model-specific firmware bytes,
    /// using the model's preset RAM layout.
    ///
    /// # Errors
    ///
    /// Returns an error if the firmware size is not valid for the
    /// selected model.
    pub fn new(model: Model, firmware_rom: Vec<u8>) -> Result<Self, MachineError> {
        Self::with_config(model.config(), firmware_rom)
    }

    /// Construct a runtime with an explicit RAM layout, bypassing the
    /// model's preset. Useful for matching custom hardware profiles
    /// (e.g. A500 + custom Zorro-II fast-RAM size) or driving tests
    /// over ranges the enum doesn't cover. The model still determines
    /// the profile metadata (display name, firmware, media slots).
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM size is invalid, the RAM layout is
    /// unsupported, or the selected Agnus cannot address the requested chip
    /// RAM.
    pub fn with_ram_config(
        model: Model,
        firmware_rom: Vec<u8>,
        ram_config: RamConfig,
    ) -> Result<Self, MachineError> {
        Self::with_config(model.config().with_ram(ram_config), firmware_rom)
    }

    /// Construct from a complete canonical configuration.
    pub fn with_config(config: AmigaConfig, firmware_rom: Vec<u8>) -> Result<Self, MachineError> {
        let model = config.model();
        validate_config(config, ChipsetKind::Ocs)?;
        validate_firmware_rom(model, &firmware_rom)?;
        let machine = build_amiga_ocs(config, &firmware_rom);
        let tick_hz = AmigaMachine::cck_hz(&machine).saturating_mul(2);
        let mut runtime = Self {
            profile: profile_for(model),
            config,
            machine,
            time: MachineTime::default(),
            firmware_rom,
            floppy0_bytes: None,
            rgba_framebuffer: vec![0; (DISPLAY_WIDTH * DISPLAY_HEIGHT * 4) as usize],
            frame_count: 0,
            non_black_pixels: 0,
            non_white_pixels: 0,
            first_active_row: None,
            audio_sample_accumulator: 0,
            audio_buffer: Vec::with_capacity(audio_buffer_capacity_for_frame(tick_hz)),
            tick_hz,
            cpu_trace: crate::cpu_trace::CpuTrace::default(),
            audio_filter: crate::audio_filter::AmigaAudioFilter::for_model(model),
        };
        runtime.update_rgba_framebuffer();
        Ok(runtime)
    }

    /// Construct from the profile's firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if firmware is missing or invalid.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let firmware_id = firmware_id_for_model(model);
        let image = firmware
            .bytes(firmware_id)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: firmware_id.to_owned(),
            })?;
        Self::new(model, image.to_vec())
    }

    /// Construct with a zero-filled placeholder model-specific ROM.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self::new(model, blank_firmware_rom(model))
            .expect("blank model firmware image should be valid")
    }

    /// RAM layout currently installed — read back for diagnostics or
    /// for tests asserting a preset was honoured. This is the RAM
    /// portion of the runtime's canonical construction configuration.
    #[must_use]
    pub fn ram_config(&self) -> RamConfig {
        self.config.ram()
    }

    /// Current host-side Paula audio controls. OCS-specific because
    /// `AudioControls` and `PaulaChannel` are defined in the OCS
    /// machine crate; future variants with different audio surfaces
    /// add their own impl block here.
    #[must_use]
    pub fn audio_controls(&self) -> AudioControls {
        self.machine.audio_controls()
    }

    /// Replace all host-side Paula audio controls.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.machine.set_audio_controls(controls);
    }

    /// Enable or disable one Paula channel in host output.
    pub fn set_audio_channel_enabled(&mut self, channel: PaulaChannel, enabled: bool) {
        self.machine.set_audio_channel_enabled(channel, enabled);
    }

    /// Set one Paula channel's host-side gain.
    pub fn set_audio_channel_gain(&mut self, channel: PaulaChannel, gain: f32) {
        self.machine.set_audio_channel_gain(channel, gain);
    }
}

// =====================================================================
// AmigaEcs-specific construction + audio control surface.
// Parallels the AmigaOcs block above. A500+ and A600 variants route
// through here; A3000 joins them once Ramsey and Fat Gary are ported.
// =====================================================================

impl AmigaRuntime<AmigaEcs> {
    /// Construct an ECS runtime from owned firmware bytes, using the
    /// model's preset RAM layout.
    pub fn new(model: Model, firmware_rom: Vec<u8>) -> Result<Self, MachineError> {
        Self::with_config(model.config(), firmware_rom)
    }

    /// Construct an ECS runtime with an explicit RAM layout.
    pub fn with_ram_config(
        model: Model,
        firmware_rom: Vec<u8>,
        ram_config: RamConfig,
    ) -> Result<Self, MachineError> {
        Self::with_config(model.config().with_ram(ram_config), firmware_rom)
    }

    /// Construct an ECS runtime from a complete canonical configuration.
    pub fn with_config(config: AmigaConfig, firmware_rom: Vec<u8>) -> Result<Self, MachineError> {
        let model = config.model();
        validate_config(config, ChipsetKind::Ecs)?;
        validate_firmware_rom(model, &firmware_rom)?;
        let machine = build_amiga_ecs(config, &firmware_rom);
        let tick_hz = AmigaMachine::cck_hz(&machine).saturating_mul(2);
        let mut runtime = Self {
            profile: profile_for(model),
            config,
            machine,
            time: MachineTime::default(),
            firmware_rom,
            floppy0_bytes: None,
            rgba_framebuffer: vec![0; (DISPLAY_WIDTH * DISPLAY_HEIGHT * 4) as usize],
            frame_count: 0,
            non_black_pixels: 0,
            non_white_pixels: 0,
            first_active_row: None,
            audio_sample_accumulator: 0,
            audio_buffer: Vec::with_capacity(audio_buffer_capacity_for_frame(tick_hz)),
            tick_hz,
            cpu_trace: crate::cpu_trace::CpuTrace::default(),
            audio_filter: crate::audio_filter::AmigaAudioFilter::for_model(model),
        };
        runtime.update_rgba_framebuffer();
        Ok(runtime)
    }

    /// Construct an ECS runtime from a profile firmware set.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let firmware_id = firmware_id_for_model(model);
        let image = firmware
            .bytes(firmware_id)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: firmware_id.to_owned(),
            })?;
        Self::new(model, image.to_vec())
    }

    /// Construct an ECS runtime with a zero-filled placeholder ROM.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self::new(model, blank_firmware_rom(model))
            .expect("blank ECS model firmware image should be valid")
    }

    /// RAM layout currently installed.
    #[must_use]
    pub fn ram_config(&self) -> RamConfig {
        self.config.ram()
    }

    /// Current host-side Paula audio controls. The ECS machine crate
    /// re-exports the same `AudioControls` / `PaulaChannel` types as
    /// OCS via the shared Paula crate, but keeps a local alias so the
    /// two impls don't share a concrete type signature here.
    #[must_use]
    pub fn audio_controls(&self) -> EcsAudioControls {
        self.machine.audio_controls()
    }

    /// Replace all host-side Paula audio controls.
    pub fn set_audio_controls(&mut self, controls: EcsAudioControls) {
        self.machine.set_audio_controls(controls);
    }

    /// Enable or disable one Paula channel in host output.
    pub fn set_audio_channel_enabled(&mut self, channel: EcsPaulaChannel, enabled: bool) {
        self.machine.set_audio_channel_enabled(channel, enabled);
    }

    /// Set one Paula channel's host-side gain.
    pub fn set_audio_channel_gain(&mut self, channel: EcsPaulaChannel, gain: f32) {
        self.machine.set_audio_channel_gain(channel, gain);
    }
}

// =====================================================================
// AmigaA1200-specific construction + audio control surface.
// Parallels the AmigaOcs / AmigaEcs blocks above. The A1200 reuses the
// shared Paula 8364, so `AudioControls` / `PaulaChannel` are the same
// OCS-crate types the OCS impl uses.
// =====================================================================

impl AmigaRuntime<AmigaA1200> {
    /// Construct a runtime from owned model-specific firmware bytes,
    /// using the model's preset RAM layout (2 MiB chip for stock
    /// A1200).
    ///
    /// # Errors
    /// Returns an error if the firmware size is not valid for the
    /// selected model (A1200 expects a 512 KiB Kickstart 3.0/3.1).
    pub fn new(model: Model, firmware_rom: Vec<u8>) -> Result<Self, MachineError> {
        Self::with_config(model.config(), firmware_rom)
    }

    /// Construct a runtime with an explicit RAM layout, bypassing the
    /// model's preset. Useful for testing fast-RAM expansion configs
    /// (trapdoor accelerator + fast RAM) without baking those into
    /// the model catalogue.
    ///
    /// # Errors
    /// Returns an error if the ROM size is invalid.
    pub fn with_ram_config(
        model: Model,
        firmware_rom: Vec<u8>,
        ram_config: RamConfig,
    ) -> Result<Self, MachineError> {
        Self::with_config(model.config().with_ram(ram_config), firmware_rom)
    }

    /// Construct an AGA runtime from a complete canonical configuration.
    pub fn with_config(config: AmigaConfig, firmware_rom: Vec<u8>) -> Result<Self, MachineError> {
        let model = config.model();
        validate_config(config, ChipsetKind::Aga)?;
        validate_firmware_rom(model, &firmware_rom)?;
        let machine = build_amiga_a1200(config, &firmware_rom);
        let tick_hz = AmigaMachine::cck_hz(&machine).saturating_mul(2);
        let mut runtime = Self {
            profile: profile_for(model),
            config,
            machine,
            time: MachineTime::default(),
            firmware_rom,
            floppy0_bytes: None,
            rgba_framebuffer: vec![0; (DISPLAY_WIDTH * DISPLAY_HEIGHT * 4) as usize],
            frame_count: 0,
            non_black_pixels: 0,
            non_white_pixels: 0,
            first_active_row: None,
            audio_sample_accumulator: 0,
            audio_buffer: Vec::with_capacity(audio_buffer_capacity_for_frame(tick_hz)),
            tick_hz,
            cpu_trace: crate::cpu_trace::CpuTrace::default(),
            audio_filter: crate::audio_filter::AmigaAudioFilter::for_model(model),
        };
        runtime.update_rgba_framebuffer();
        Ok(runtime)
    }

    /// Construct from the profile's firmware set.
    ///
    /// # Errors
    /// Returns an error if firmware is missing or invalid.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let firmware_id = firmware_id_for_model(model);
        let image = firmware
            .bytes(firmware_id)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: firmware_id.to_owned(),
            })?;
        Self::new(model, image.to_vec())
    }

    /// Construct with a zero-filled placeholder firmware.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self::new(model, blank_firmware_rom(model))
            .expect("blank model firmware image should be valid")
    }

    /// RAM layout currently installed.
    #[must_use]
    pub fn ram_config(&self) -> RamConfig {
        self.config.ram()
    }

    /// Current host-side Paula audio controls.
    #[must_use]
    pub fn audio_controls(&self) -> AudioControls {
        self.machine.audio_controls()
    }

    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.machine.set_audio_controls(controls);
    }

    pub fn set_audio_channel_enabled(&mut self, channel: PaulaChannel, enabled: bool) {
        self.machine.set_audio_channel_enabled(channel, enabled);
    }

    pub fn set_audio_channel_gain(&mut self, channel: PaulaChannel, gain: f32) {
        self.machine.set_audio_channel_gain(channel, gain);
    }
}

pub(crate) fn build_amiga_a1200(config: AmigaConfig, firmware_rom: &[u8]) -> AmigaA1200 {
    // A1200 only ever boots from Kickstart 3.0 / 3.1 (no A1000-style
    // bootstrap path). Region drives PAL/NTSC Agnus selection inside
    // the chip layer; A1200 reuses the same shared AgnusRegion enum.
    let model = config.model();
    let firmware = firmware_rom.to_vec();
    if model.is_ntsc() {
        AmigaA1200::with_ram_config_ntsc(firmware, config.ram())
    } else {
        AmigaA1200::with_ram_config(firmware, config.ram())
    }
}

pub(crate) fn build_amiga_ecs(config: AmigaConfig, firmware_rom: &[u8]) -> AmigaEcs {
    // ECS uses the same Kickstart-only construction as the A500
    // family. A1000-style bootstrap and A3000 SuperKickstart paths
    // come later. Region drives PAL/NTSC Agnus selection inside the
    // chip layer.
    let model = config.model();
    let firmware = firmware_rom.to_vec();
    if model.is_ntsc() {
        AmigaEcs::with_ram_config_ntsc(firmware, config.ram())
    } else {
        AmigaEcs::with_ram_config(firmware, config.ram())
    }
}

pub(crate) fn build_amiga_ocs(config: AmigaConfig, firmware_rom: &[u8]) -> AmigaOcs {
    // Cross product of two model axes:
    //   - A1000 (bootstrap ROM into WOM) vs A500-family (Kickstart)
    //   - PAL vs NTSC Agnus
    // Every A500-family layout routes through the same autoconfig-
    // aware constructor; the Zorro-II fast-RAM board is attached
    // automatically when `ram_config.fast_kb > 0`.
    let model = config.model();
    let ram_config = config.ram();
    let firmware = firmware_rom.to_vec();
    if let Some(Accelerator::GvpA530(board_config)) = config.accelerator() {
        return if model.is_ntsc() {
            AmigaOcs::with_gvp_a530_config_ntsc(firmware, ram_config, board_config)
        } else {
            AmigaOcs::with_gvp_a530_config(firmware, ram_config, board_config)
        };
    }
    match (
        model.is_a1000(),
        model.is_ntsc(),
        model.uses_fat_agnus_8372a(),
    ) {
        (true, false, false) => AmigaOcs::with_a1000_bootstrap_rom(firmware, ram_config),
        (true, true, false) => AmigaOcs::with_a1000_bootstrap_rom_ntsc(firmware, ram_config),
        (false, false, true) => AmigaOcs::with_fat_agnus_ram_config(firmware, ram_config),
        (false, true, true) => AmigaOcs::with_fat_agnus_ram_config_ntsc(firmware, ram_config),
        (false, false, false) => AmigaOcs::with_ram_config(firmware, ram_config),
        (false, true, false) => AmigaOcs::with_ram_config_ntsc(firmware, ram_config),
        (true, _, true) => unreachable!("A1000 profiles never use Fat Agnus 8372A"),
    }
}

/// Validate construction intent before a machine constructor can observe it.
///
/// Custom RAM layouts remain supported. Processor and accelerator axes stay
/// bound to the selected catalogue model so profile identity cannot silently
/// describe different hardware.
pub(crate) fn validate_config(
    config: AmigaConfig,
    expected_chipset: ChipsetKind,
) -> Result<(), MachineError> {
    if config.model().chipset() != expected_chipset {
        return Err(MachineError::InvalidRequest {
            reason: format!(
                "model {:?} uses {:?}, not {:?}",
                config.model(),
                config.model().chipset(),
                expected_chipset
            ),
        });
    }
    if !config.ram().is_valid() {
        return Err(MachineError::InvalidRequest {
            reason: format!("unsupported Amiga RAM layout: {:?}", config.ram()),
        });
    }
    let model = config.model();
    let chip_ram_ceiling = if model.is_a1000() {
        FATTER_AGNUS_CHIP_RAM_BYTES
    } else {
        match expected_chipset {
            ChipsetKind::Ocs if model.uses_fat_agnus_8372a() => FAT_AGNUS_CHIP_RAM_BYTES,
            ChipsetKind::Ocs => OCS_AGNUS_CHIP_RAM_BYTES,
            ChipsetKind::Ecs | ChipsetKind::Aga => ECS_AGA_CHIP_RAM_BYTES,
        }
    };
    let chip_ram_bytes = config.ram().chip_kb as usize * KIB;
    if chip_ram_bytes > chip_ram_ceiling {
        return Err(MachineError::InvalidRequest {
            reason: format!(
                "model {model:?} addresses at most {} KiB chip RAM, not {} KiB",
                chip_ram_ceiling / KIB,
                config.ram().chip_kb
            ),
        });
    }
    let canonical = config.model().config();
    if config.cpu() != canonical.cpu() || config.accelerator() != canonical.accelerator() {
        return Err(MachineError::InvalidRequest {
            reason: format!(
                "processor or accelerator does not match model {:?}",
                config.model()
            ),
        });
    }
    Ok(())
}

fn firmware_id_for_model(model: Model) -> &'static str {
    if model.is_a1000() {
        A1000_BOOTSTRAP_ROM_ID
    } else {
        KICKSTART_ROM_ID
    }
}

pub(crate) fn blank_standard_kickstart_rom() -> Vec<u8> {
    let mut kickstart = vec![0u8; 256 * 1024];
    kickstart[0] = 0x00;
    kickstart[1] = 0x08;
    kickstart[2] = 0x00;
    kickstart[3] = 0x00;
    kickstart[4] = 0x00;
    kickstart[5] = 0xF8;
    kickstart[6] = 0x00;
    kickstart[7] = 0x08;
    kickstart[8] = 0x60;
    kickstart[9] = 0xFE;
    kickstart
}

pub(crate) fn blank_a1000_bootstrap_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 64 * 1024];
    rom[0] = 0x11;
    rom[1] = 0x11;
    rom[2] = 0x4E;
    rom[3] = 0xF9;
    rom[4] = 0x00;
    rom[5] = 0xF8;
    rom[6] = 0x00;
    rom[7] = 0x08;
    rom[8] = 0x60;
    rom[9] = 0xFE;
    rom
}

fn blank_firmware_rom(model: Model) -> Vec<u8> {
    if model.is_a1000() {
        blank_a1000_bootstrap_rom()
    } else {
        blank_standard_kickstart_rom()
    }
}

fn validate_firmware_rom(model: Model, firmware_rom: &[u8]) -> Result<(), MachineError> {
    let (valid_sizes, firmware_id) = if model.is_a1000() {
        (VALID_A1000_BOOTSTRAP_SIZES, A1000_BOOTSTRAP_ROM_ID)
    } else {
        (VALID_KICKSTART_SIZES, KICKSTART_ROM_ID)
    };
    if valid_sizes.contains(&firmware_rom.len()) {
        return Ok(());
    }
    Err(MachineError::InvalidFirmware {
        id: firmware_id.to_owned(),
        reason: format!(
            "expected one of {:?} bytes, got {}",
            valid_sizes,
            firmware_rom.len()
        ),
    })
}

fn audio_sample_frames_for_ticks(ticks: u64, tick_hz: u64) -> usize {
    if tick_hz == 0 {
        return 0;
    }
    usize::try_from((ticks.saturating_mul(u64::from(AUDIO_SAMPLE_RATE_HZ))) / tick_hz)
        .unwrap_or(usize::MAX)
}

fn audio_buffer_capacity_for_frame(tick_hz: u64) -> usize {
    // A reasonable upper bound for one PAL frame at 48 kHz: 960
    // stereo samples. Computing dynamically against tick_hz keeps the
    // capacity right for NTSC and future Vampire-clock variants.
    let frame_ticks = if tick_hz > 0 {
        // PAL frame ≈ tick_hz / 50; NTSC ≈ tick_hz / 60. Use the
        // larger of the two as an upper bound so the buffer doesn't
        // grow at runtime under either region.
        tick_hz / 50
    } else {
        tick_hz
    };
    audio_sample_frames_for_ticks(frame_ticks, tick_hz)
        .saturating_add(1)
        .saturating_mul(usize::from(AUDIO_CHANNELS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::A500_PAL_CCK_HZ;
    use crate::variants::AmigaRuntimeKind;

    #[test]
    fn bounded_step_accounts_progress_when_stopped_cpu_has_no_next_boundary() {
        let mut rom = blank_standard_kickstart_rom();
        // Reset PC is $F80008. Execute STOP #$2700 there, with interrupts
        // masked, so the following debugger step cannot reach a new boundary.
        rom[8..12].copy_from_slice(&[0x4E, 0x72, 0x27, 0x00]);
        let mut runtime = AmigaRuntimeKind::new(Model::A500OcsPal, rom).expect("test ROM is valid");

        runtime.step_cpu_instruction(1_024);
        let starts_before = runtime.cpu_instruction_starts();
        assert_eq!(starts_before, 1, "the STOP instruction must have started");

        let tick_limit = 32;
        let machine_ticks_before = runtime.tick_count();
        let runtime_time_before = runtime.time().get();
        let AmigaRuntimeKind::Ocs(inner) = &mut runtime else {
            panic!("A500 must use the OCS runtime");
        };
        let audio_phase_before = inner.audio_sample_accumulator();
        // Make every cached framebuffer diagnostic deliberately stale. Exact
        // stepping must rebuild them even when the CPU is stopped.
        inner.non_black_pixels = u32::MAX;
        inner.non_white_pixels = u32::MAX;
        inner.first_active_row = Some(u32::MAX);

        let consumed = runtime.step_cpu_instruction(tick_limit);

        assert_eq!(consumed, tick_limit);
        assert_eq!(
            runtime.cpu_instruction_starts(),
            starts_before,
            "an unchanged boundary counter reports that the bounded step did not complete an instruction"
        );
        assert_eq!(
            runtime.tick_count().wrapping_sub(machine_ticks_before),
            tick_limit
        );
        assert_eq!(
            runtime.time().get().wrapping_sub(runtime_time_before),
            tick_limit,
            "bounded stepping must still account for completed machine ticks"
        );

        let AmigaRuntimeKind::Ocs(inner) = &runtime else {
            panic!("A500 must use the OCS runtime");
        };
        let tick_hz = A500_PAL_CCK_HZ * 2;
        assert_eq!(
            inner.audio_sample_accumulator(),
            (audio_phase_before + tick_limit * u64::from(AUDIO_SAMPLE_RATE_HZ)) % tick_hz,
            "bounded stepping must advance host-audio phase once per completed tick"
        );

        let framebuffer = inner.machine().denise().framebuffer();
        let expected_non_black = framebuffer
            .iter()
            .filter(|pixel| **pixel & 0x00FF_FFFF != 0)
            .count() as u32;
        let expected_non_white = framebuffer
            .iter()
            .filter(|pixel| **pixel & 0x00FF_FFFF != 0x00FF_FFFF)
            .count() as u32;
        let expected_first_active_row = framebuffer
            .iter()
            .position(|pixel| *pixel & 0x00FF_FFFF != 0)
            .map(|index| index as u32 / DISPLAY_WIDTH);
        assert_eq!(inner.non_black_pixels, expected_non_black);
        assert_eq!(inner.non_white_pixels, expected_non_white);
        assert_eq!(inner.first_active_row, expected_first_active_row);
    }
}
