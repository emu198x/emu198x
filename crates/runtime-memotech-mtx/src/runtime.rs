//! Runtime wrapper for the Memotech MTX.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};
use machine_memotech_mtx::{Mtx, MtxModel};

use crate::input::apply_input_event;
use crate::profiles::{Model, ROM_FIRMWARE_ID, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;

const ROM_SIZE: usize = 16 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// This machine's keyboard for the shared `press_key` / `type_string` tools:
/// the standard layout, backed by this machine's own key-name resolver so a
/// character it cannot type is refused rather than silently dropped (#1196).
static KEYBOARD: emu198x_shell::StandardKeyboard = emu198x_shell::StandardKeyboard::new(
    emu198x_shell::STANDARD_KEY_TIMING,
    crate::input::knows_key_name,
);

pub struct MtxRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Mtx>,
    rom_bytes: Option<Vec<u8>>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    controller_cache: crate::input::ControllerCache,
}

impl MtxRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            rom_bytes: None,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            controller_cache: crate::input::ControllerCache::default(),
        }
    }

    /// Build directly from an OS + paged-ROM image (8 KB OS followed by 8 KB
    /// BASIC, ASSEM, …).
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the size is not the 8 KB OS
    /// plus a whole number of 8 KB paged ROMs.
    pub fn new(model: Model, rom: Vec<u8>) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.set_rom(rom)?;
        Ok(runtime)
    }

    /// Build from a firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or ROM is missing.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let bytes =
            firmware
                .bytes(ROM_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: ROM_FIRMWARE_ID.to_owned(),
                })?;
        Self::new(model, bytes.to_vec())
    }

    /// Replace the ROM and rebuild.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the size is wrong.
    pub fn set_rom(&mut self, rom: Vec<u8>) -> Result<(), MachineError> {
        // 8 KB OS plus one or more 8 KB paged ROMs (BASIC, ASSEM, …).
        if rom.len() < ROM_SIZE || !rom.len().is_multiple_of(0x2000) {
            return Err(MachineError::InvalidFirmware {
                id: ROM_FIRMWARE_ID.to_owned(),
                reason: format!(
                    "ROM is {} bytes; expected the 8 KB OS plus 8 KB paged ROMs \
                     (a multiple of 8192, ≥ {ROM_SIZE})",
                    rom.len()
                ),
            });
        }
        self.rom_bytes = Some(rom);
        self.rebuild_machine()
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Mtx> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Mtx> {
        self.machine.as_mut()
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    /// Install a machine restored from a snapshot, re-deriving the host RGBA
    /// framebuffer from its live state. Replaces the cold-boot rebuild on the
    /// restore path so the resumed machine keeps its CPU/VDP/PSG/CTC/RAM state.
    pub(crate) fn set_machine(&mut self, machine: Option<Mtx>) {
        if let Some(machine) = &machine {
            let width = machine.framebuffer_width();
            let height = machine.framebuffer_height();
            self.rgba_width = width;
            self.rgba_height = height;
            self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        }
        self.machine = machine;
        self.update_rgba_framebuffer();
    }

    fn rebuild_machine(&mut self) -> Result<(), MachineError> {
        let Some(rom) = self.rom_bytes.clone() else {
            self.machine = None;
            return Ok(());
        };
        let mtx_model = match self.model {
            Model::Mtx500 => MtxModel::Mtx500,
            Model::Mtx512 => MtxModel::Mtx512,
        };
        let machine = Mtx::new(rom, mtx_model).map_err(|reason| MachineError::InvalidFirmware {
            id: ROM_FIRMWARE_ID.to_owned(),
            reason,
        })?;
        let width = machine.framebuffer_width();
        let height = machine.framebuffer_height();
        self.rgba_width = width;
        self.rgba_height = height;
        self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        self.machine = Some(machine);
        self.update_rgba_framebuffer();
        Ok(())
    }

    fn update_rgba_framebuffer(&mut self) {
        let Some(machine) = self.machine.as_ref() else {
            self.rgba_framebuffer.fill(0);
            return;
        };
        for (index, &pixel) in machine.framebuffer().iter().enumerate() {
            let base = index * 4;
            self.rgba_framebuffer[base] = ((pixel >> 16) & 0xff) as u8;
            self.rgba_framebuffer[base + 1] = ((pixel >> 8) & 0xff) as u8;
            self.rgba_framebuffer[base + 2] = (pixel & 0xff) as u8;
            self.rgba_framebuffer[base + 3] = ((pixel >> 24) & 0xff) as u8;
        }
    }
}

impl MachineCore for MtxRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }
    fn time(&self) -> MachineTime {
        self.time
    }
    fn reset(&mut self, _kind: ResetKind) {
        let _ = self.rebuild_machine();
        self.time = MachineTime::default();
    }
    fn load_media(&mut self, _media: &MediaSet<'_>) -> Result<(), MachineError> {
        Ok(())
    }
    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        if self.machine.is_none() {
            return Ok(RunResult::new(self.time, StopReason::WaitingForInput));
        }
        for event in host.input_events {
            if let Some(machine) = self.machine.as_mut() {
                apply_input_event(machine, &mut self.controller_cache, event);
            }
        }
        while self.time < target {
            let ticks = self
                .machine
                .as_mut()
                .expect("machine checked above")
                .run_frame();
            self.time = self.time.saturating_add(ticks);
            self.update_rgba_framebuffer();
            host.frame_sink.push_frame(FramePacket {
                timestamp: self.time,
                format: PixelFormat::Rgba8888,
                width: self.rgba_width,
                height: self.rgba_height,
                palette: None,
                pixels: &self.rgba_framebuffer,
            })?;
            let audio = self
                .machine
                .as_mut()
                .expect("machine checked above")
                .take_audio_buffer();
            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: AUDIO_SAMPLE_RATE,
                channels: 1,
                samples: &audio,
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
    /// The TMS9918 family drove a television through a colour-subcarrier
    /// crystal, so its dots are not square: 8:7 on the NTSC parts, about
    /// 1.382 on the PAL TMS9929A. Presenting the 288x240 framebuffer unstretched
    /// claimed otherwise.
    fn display(&self) -> Option<Display> {
        Display::television_for_region(
            self.profile().region,
            ti_tms9918::PAL_DOT_CLOCK_HZ,
            ti_tms9918::NTSC_DOT_CLOCK_HZ,
        )
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();
    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        self.machine
            .is_some()
            .then_some(&KEYBOARD as &dyn emu198x_shell::KeyboardTarget)
    }
}

emu198x_shell::impl_z80_debug_primitives!(MtxRuntime);
