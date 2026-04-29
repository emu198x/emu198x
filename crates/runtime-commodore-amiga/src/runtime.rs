//! Runtime wrapper around the A500 OCS machine.
//!
//! Implements `MachineCore` so the shell can drive the machine
//! through a common interface. The runtime owns the ROM bytes so
//! reset rebuilds from them, emits one frame per `run_until`
//! iteration, and delegates keyboard input and ADF insertion to
//! `AmigaOcs`. Per-concern siblings — `queries.rs`, `snapshot.rs`,
//! `input.rs` — own the pieces of the surface that don't belong in
//! lifecycle.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, ResetKind, RunResult,
    StopReason,
};
use format_commodore_amiga_adf::Adf;
use machine_commodore_amiga_ocs::{
    AmigaOcs, AudioControls, FB_HEIGHT, FB_WIDTH, PaulaChannel, RamConfig,
};

use crate::input::apply_input_event;
use crate::snapshot;
use crate::{A500_PAL_CCK_HZ, A500_PAL_FRAME_TICKS, Model, profile_for};

pub(crate) const KICKSTART_ROM_ID: &str = "commodore-amiga-kickstart-rom";
pub(crate) const A1000_BOOTSTRAP_ROM_ID: &str = "commodore-amiga-a1000-bootstrap-rom";
const VALID_KICKSTART_SIZES: &[usize] = &[256 * 1024, 512 * 1024];
const VALID_A1000_BOOTSTRAP_SIZES: &[usize] = &[64 * 1024];
pub(crate) const AUDIO_SAMPLE_RATE_HZ: u32 = 48_000;
pub(crate) const AUDIO_CHANNELS: u8 = 2;
pub(crate) const A500_PAL_TICK_HZ: u64 = A500_PAL_CCK_HZ * 2;

/// Machine framebuffer width (= `FB_WIDTH`). Re-exported for host
/// integrations that size their output buffers without pulling in
/// the machine crate directly.
pub const DISPLAY_WIDTH: u32 = FB_WIDTH;
/// Machine framebuffer height (= `FB_HEIGHT`).
pub const DISPLAY_HEIGHT: u32 = FB_HEIGHT;

/// Firmware-backed Amiga runtime over the OCS machine family.
pub struct AmigaRuntime {
    profile: MachineProfile,
    model: Model,
    /// Active RAM layout. Defaults to `model.ram_config()` for the
    /// standard model presets; `from_ram_config` overrides it with a
    /// caller-supplied layout. Held here so `reset` / `rebuild_machine`
    /// reconstructs with the same sizes.
    ram_config: RamConfig,
    machine: AmigaOcs,
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
    /// per machine tick (master/4); Paula output itself only changes
    /// on CCK boundaries, but sampling at the finer runtime tick keeps
    /// this phase stable across frame boundaries.
    audio_sample_accumulator: u64,
    audio_buffer: Vec<f32>,
}

impl AmigaRuntime {
    /// Construct a runtime from owned model-specific firmware bytes,
    /// using the model's preset RAM layout.
    ///
    /// # Errors
    ///
    /// Returns an error if the firmware size is not valid for the
    /// selected model.
    pub fn new(model: Model, firmware_rom: Vec<u8>) -> Result<Self, MachineError> {
        Self::with_ram_config(model, firmware_rom, model.ram_config())
    }

    /// Construct a runtime with an explicit RAM layout, bypassing the
    /// model's preset. Useful for matching custom hardware profiles
    /// (e.g. A500 + custom Zorro-II fast-RAM size) or driving tests
    /// over ranges the enum doesn't cover. The model still determines
    /// the profile metadata (display name, firmware, media slots).
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM size is invalid. Panics if the RAM
    /// layout is not one of the supported size combinations — see
    /// `RamConfig::is_valid`.
    pub fn with_ram_config(
        model: Model,
        firmware_rom: Vec<u8>,
        ram_config: RamConfig,
    ) -> Result<Self, MachineError> {
        validate_firmware_rom(model, &firmware_rom)?;
        let machine = build_machine(model, ram_config, &firmware_rom);
        let mut runtime = Self {
            profile: profile_for(model),
            model,
            ram_config,
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
            audio_buffer: Vec::with_capacity(audio_buffer_capacity_for_frame()),
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

    /// Read-only access to the wrapped machine.
    #[must_use]
    pub fn machine(&self) -> &AmigaOcs {
        &self.machine
    }

    /// Mutable access to the wrapped machine. Only for tests /
    /// integrations that need to drive the tick loop directly (e.g.
    /// autoconfig boot tests that run the machine outside `run_until`).
    pub fn machine_mut(&mut self) -> &mut AmigaOcs {
        &mut self.machine
    }

    /// Current host-side Paula audio controls.
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

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        validate_firmware_rom(self.model, &self.firmware_rom)?;
        self.machine = build_machine(self.model, self.ram_config, &self.firmware_rom);
        if let Some(bytes) = self.floppy0_bytes.clone() {
            self.insert_floppy_bytes("floppy-0", &bytes)?;
        }
        self.time = MachineTime::default();
        self.frame_count = 0;
        self.audio_sample_accumulator = 0;
        self.audio_buffer.clear();
        self.update_rgba_framebuffer();
        Ok(())
    }

    /// RAM layout currently installed — read back for diagnostics or
    /// for tests asserting a preset was honoured.
    #[must_use]
    pub fn ram_config(&self) -> RamConfig {
        self.ram_config
    }

    /// Active model (affects profile metadata, not the RAM layout).
    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    fn insert_floppy_bytes(&mut self, slot: &str, bytes: &[u8]) -> Result<(), MachineError> {
        let adf = Adf::from_bytes(bytes.to_vec()).map_err(|reason| MachineError::InvalidMedia {
            slot: slot.to_owned(),
            reason: reason.to_string(),
        })?;
        if self.model == Model::A1000OcsPal {
            self.machine.insert_adf_with_change_pending(adf);
        } else {
            self.machine.insert_adf(adf);
        }
        self.floppy0_bytes = Some(bytes.to_vec());
        Ok(())
    }

    /// Snapshot-side hook into `insert_floppy_bytes`. The snapshot
    /// module re-mounts the persisted disk image after restoring the
    /// machine; the call has to go through the same media path so the
    /// A1000 disk-change-pending bookkeeping fires.
    pub(crate) fn insert_floppy_bytes_pub(
        &mut self,
        slot: &str,
        bytes: &[u8],
    ) -> Result<(), MachineError> {
        self.insert_floppy_bytes(slot, bytes)
    }

    /// Copy the machine's ARGB framebuffer into the RGBA frame
    /// packet buffer the shell expects. ARGB → RGBA is a simple
    /// byte reorder. Side-effect: refreshes the pixel-based boot
    /// heuristic (`non_black_pixels` / `non_white_pixels` /
    /// `first_active_row`) so the next `boot.detected` query reads
    /// consistent values.
    fn update_rgba_framebuffer(&mut self) {
        let fb = self.machine.denise().framebuffer();
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

    fn tick_and_sample_audio(&mut self) {
        self.machine.tick();
        self.audio_sample_accumulator = self
            .audio_sample_accumulator
            .saturating_add(u64::from(AUDIO_SAMPLE_RATE_HZ));

        while self.audio_sample_accumulator >= A500_PAL_TICK_HZ {
            self.audio_sample_accumulator -= A500_PAL_TICK_HZ;
            let (left, right) = self.machine.paula().mix_audio_stereo();
            self.audio_buffer.push(left);
            self.audio_buffer.push(right);
        }
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

    pub(crate) fn set_ram_config(&mut self, ram_config: RamConfig) {
        self.ram_config = ram_config;
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

    pub(crate) fn clear_audio_buffer(&mut self) {
        self.audio_buffer.clear();
    }

    /// Repack the machine's framebuffer into the runtime's RGBA8888
    /// buffer. Called by the snapshot module after a restore so the
    /// next frame draw sees the post-snapshot contents instead of
    /// stale RGB data.
    pub(crate) fn refresh_rgba_framebuffer(&mut self) {
        self.update_rgba_framebuffer();
    }
}

impl MachineCore for AmigaRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_machine()
            .expect("stored Kickstart image should remain valid across resets");
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

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        // Apply queued input at the top of the run window. Keyboard
        // is the only input kind wired right now; mouse / joystick
        // come later.
        for event in host.input_events {
            apply_input_event(&mut self.machine, event);
        }

        while self.time < target {
            // Run one PAL frame.
            self.audio_buffer.clear();
            for _ in 0..A500_PAL_FRAME_TICKS {
                self.tick_and_sample_audio();
            }
            self.frame_count = self.frame_count.saturating_add(1);
            self.time = self.time.saturating_add(A500_PAL_FRAME_TICKS);
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

fn build_machine(model: Model, ram_config: RamConfig, firmware_rom: &[u8]) -> AmigaOcs {
    match model {
        Model::A1000OcsPal => AmigaOcs::with_a1000_bootstrap_rom(firmware_rom.to_vec(), ram_config),
        Model::A500OcsPal
        | Model::A500OcsPalA501
        | Model::A500PlusOcsPal
        | Model::A500OcsPalMaxed => {
            // Every A500-family layout in the current `Model`
            // catalogue routes through the same autoconfig-aware
            // constructor. A Zorro-II fast-RAM board is attached
            // automatically when `ram_config.fast_kb > 0`; the ROM's
            // `expansion.library` picks it up during boot without
            // runtime cooperation.
            AmigaOcs::with_ram_config(firmware_rom.to_vec(), ram_config)
        }
    }
}

fn firmware_id_for_model(model: Model) -> &'static str {
    match model {
        Model::A1000OcsPal => A1000_BOOTSTRAP_ROM_ID,
        Model::A500OcsPal
        | Model::A500OcsPalA501
        | Model::A500PlusOcsPal
        | Model::A500OcsPalMaxed => KICKSTART_ROM_ID,
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
    match model {
        Model::A1000OcsPal => blank_a1000_bootstrap_rom(),
        Model::A500OcsPal
        | Model::A500OcsPalA501
        | Model::A500PlusOcsPal
        | Model::A500OcsPalMaxed => blank_standard_kickstart_rom(),
    }
}

fn validate_firmware_rom(model: Model, firmware_rom: &[u8]) -> Result<(), MachineError> {
    let (valid_sizes, firmware_id) = match model {
        Model::A1000OcsPal => (VALID_A1000_BOOTSTRAP_SIZES, A1000_BOOTSTRAP_ROM_ID),
        Model::A500OcsPal
        | Model::A500OcsPalA501
        | Model::A500PlusOcsPal
        | Model::A500OcsPalMaxed => (VALID_KICKSTART_SIZES, KICKSTART_ROM_ID),
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

pub(crate) fn audio_sample_frames_for_ticks(ticks: u64) -> usize {
    usize::try_from((ticks.saturating_mul(u64::from(AUDIO_SAMPLE_RATE_HZ))) / A500_PAL_TICK_HZ)
        .unwrap_or(usize::MAX)
}

fn audio_buffer_capacity_for_frame() -> usize {
    audio_sample_frames_for_ticks(A500_PAL_FRAME_TICKS)
        .saturating_add(1)
        .saturating_mul(usize::from(AUDIO_CHANNELS))
}
