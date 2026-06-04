//! Runtime wrapper for the fresh-workspace MSX1 baseline.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind,
    RunResult, StopReason,
};
use machine_msx::{MapperType, Msx, MsxRegion};

use crate::input::apply_input_event;
use crate::profiles::{BIOS_FIRMWARE_ID, Model, profile_for};
use crate::snapshot;

const BIOS_SIZE: usize = 32 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;
/// PSG buffer size — matches `machine-msx`'s internal allocation.
const AUDIO_SAMPLES_PER_FRAME: usize = 1024;

/// MSX1 runtime. The machine cannot construct without a 32 KB BIOS,
/// so the runtime stays in `Option` until firmware is provided (via
/// `from_firmware`, `MachineCore::load_media` with a `Snapshot` slot,
/// or programmatic `set_bios`).
pub struct MsxRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Msx>,
    bios_bytes: Option<Vec<u8>>,
    cart1_bytes: Option<Vec<u8>>,
    cart1_mapper: MapperType,
    cart2_bytes: Option<Vec<u8>>,
    cart2_mapper: MapperType,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    audio_scratch: Vec<f32>,
}

impl MsxRuntime {
    /// Construct a runtime in the blank state (no BIOS, no cartridge).
    /// Use `load_media` with a firmware entry or programmatic
    /// `set_bios` to bootstrap before `run_until`.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            bios_bytes: None,
            cart1_bytes: None,
            cart1_mapper: MapperType::Plain,
            cart2_bytes: None,
            cart2_mapper: MapperType::Plain,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            audio_scratch: vec![0.0; AUDIO_SAMPLES_PER_FRAME],
        }
    }

    /// Build a runtime around an explicit 32 KB BIOS image.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the BIOS is not
    /// exactly 32 KB.
    pub fn new(model: Model, bios: Vec<u8>) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.set_bios(bios)?;
        Ok(runtime)
    }

    /// Build a runtime from a profile firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if the firmware set fails profile validation
    /// or omits the BIOS image.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let bytes =
            firmware
                .bytes(BIOS_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: BIOS_FIRMWARE_ID.to_owned(),
                })?;
        Self::new(model, bytes.to_vec())
    }

    /// Replace the BIOS image and rebuild the wrapped machine.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the BIOS is not
    /// exactly 32 KB.
    pub fn set_bios(&mut self, bios: Vec<u8>) -> Result<(), MachineError> {
        if bios.len() != BIOS_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: BIOS_FIRMWARE_ID.to_owned(),
                reason: format!("BIOS is {} bytes; expected {BIOS_SIZE}", bios.len()),
            });
        }
        self.bios_bytes = Some(bios);
        self.rebuild_machine();
        Ok(())
    }

    /// Insert a cartridge into slot 1 with the given mapper.
    pub fn insert_cartridge1(&mut self, rom: Vec<u8>, mapper: MapperType) {
        self.cart1_bytes = Some(rom.clone());
        self.cart1_mapper = mapper;
        if let Some(machine) = self.machine.as_mut() {
            machine.insert_cart1(rom, mapper);
        }
    }

    /// Insert a cartridge into slot 2 with the given mapper.
    pub fn insert_cartridge2(&mut self, rom: Vec<u8>, mapper: MapperType) {
        self.cart2_bytes = Some(rom.clone());
        self.cart2_mapper = mapper;
        if let Some(machine) = self.machine.as_mut() {
            machine.insert_cart2(rom, mapper);
        }
    }

    /// The wrapped machine when BIOS has been loaded.
    #[must_use]
    pub fn machine(&self) -> Option<&Msx> {
        self.machine.as_ref()
    }

    /// The wrapped machine when BIOS has been loaded (mutable).
    pub fn machine_mut(&mut self) -> Option<&mut Msx> {
        self.machine.as_mut()
    }

    /// Currently configured model.
    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    /// Raw BIOS image (when loaded).
    #[must_use]
    pub(crate) fn bios_bytes(&self) -> Option<&[u8]> {
        self.bios_bytes.as_deref()
    }

    /// Cartridge slot 1 ROM image (when present).
    #[must_use]
    pub(crate) fn cart1_bytes(&self) -> Option<&[u8]> {
        self.cart1_bytes.as_deref()
    }

    /// Cartridge slot 1 mapper.
    #[must_use]
    pub(crate) fn cart1_mapper(&self) -> MapperType {
        self.cart1_mapper
    }

    /// Cartridge slot 2 ROM image (when present).
    #[must_use]
    pub(crate) fn cart2_bytes(&self) -> Option<&[u8]> {
        self.cart2_bytes.as_deref()
    }

    /// Cartridge slot 2 mapper.
    #[must_use]
    pub(crate) fn cart2_mapper(&self) -> MapperType {
        self.cart2_mapper
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn set_bios_bytes(&mut self, bytes: Option<Vec<u8>>) {
        self.bios_bytes = bytes;
    }

    pub(crate) fn set_cart1(&mut self, bytes: Option<Vec<u8>>, mapper: MapperType) {
        self.cart1_bytes = bytes;
        self.cart1_mapper = mapper;
    }

    pub(crate) fn set_cart2(&mut self, bytes: Option<Vec<u8>>, mapper: MapperType) {
        self.cart2_bytes = bytes;
        self.cart2_mapper = mapper;
    }

    /// Rebuild the wrapped Msx instance from the post-restore state.
    /// Called by the snapshot decoder so the next frame draw sees the
    /// post-snapshot contents instead of zeros.
    pub(crate) fn rebuild_after_restore(&mut self) {
        self.rebuild_machine();
    }

    /// Rebuild the wrapped Msx instance from the current BIOS +
    /// cartridge bytes. Called after BIOS / region / reset changes.
    fn rebuild_machine(&mut self) {
        let Some(bios) = self.bios_bytes.clone() else {
            self.machine = None;
            return;
        };
        let region = match self.model.region() {
            emu198x_shell::Region::Pal => MsxRegion::Pal,
            _ => MsxRegion::Ntsc,
        };
        let mut machine = Msx::new(bios, region);
        if let Some(rom) = self.cart1_bytes.clone() {
            machine.insert_cart1(rom, self.cart1_mapper);
        }
        if let Some(rom) = self.cart2_bytes.clone() {
            machine.insert_cart2(rom, self.cart2_mapper);
        }
        let width = machine.framebuffer_width();
        let height = machine.framebuffer_height();
        self.rgba_width = width;
        self.rgba_height = height;
        self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        self.machine = Some(machine);
        self.update_rgba_framebuffer();
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

    /// Drain the PSG's mixed audio output buffer (48 kHz mono f32).
    fn take_audio_buffer(&mut self) -> Vec<f32> {
        let Some(machine) = self.machine.as_mut() else {
            return Vec::new();
        };
        let mut out = vec![0.0_f32; self.audio_scratch.len()];
        machine.psg_mut().end_frame(&mut out);
        // The PSG zero-pads when its internal count is shorter than
        // the requested slice; trim trailing zeros to avoid emitting
        // silence past the actual frame.
        if let Some(last_non_zero) = out.iter().rposition(|s| *s != 0.0) {
            out.truncate(last_non_zero + 1);
        } else {
            out.clear();
        }
        out
    }
}

impl MachineCore for MsxRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_machine();
        self.time = MachineTime::default();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            match (image.slot.as_ref(), image.kind) {
                ("cartridge-1", MediaKind::Cartridge) => {
                    self.insert_cartridge1(image.bytes.to_vec(), MapperType::Plain);
                }
                ("cartridge-2", MediaKind::Cartridge) => {
                    self.insert_cartridge2(image.bytes.to_vec(), MapperType::Plain);
                }
                (slot, kind) => {
                    if matches!(kind, MediaKind::Cartridge) {
                        return Err(MachineError::UnknownMediaSlot {
                            slot: slot.to_owned(),
                        });
                    }
                    return Err(MachineError::UnsupportedMediaKind { kind });
                }
            }
        }
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
                apply_input_event(machine, event);
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

            let audio = self.take_audio_buffer();
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

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();
}

// Z80 debug target via the shared macro (lazy `machine: Option<Msx>`). The
// previous hand-rolled impl differed only by an extra `tstates` field in
// cpu_state, which is available via the `msx.cpu.tstates` query.
emu198x_shell::impl_z80_debug_target!(MsxRuntime);
