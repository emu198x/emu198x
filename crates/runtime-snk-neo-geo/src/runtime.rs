//! The Neo Geo runtime: the machine behind the shell's traits.
//!
//! Shaped like the Mega Drive's, with the ColecoVision's firmware handling:
//! the console cannot start without its BIOS, so the machine stays `None`
//! until a BIOS and a cartridge are both in. The media are kept here, not in
//! snapshots, since a cartridge's C-ROMs run to tens of megabytes; a restore
//! hands them back to the machine. Time is counted in master clocks.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind,
    RunResult, StopReason,
};
use format_snk_neo_geo_romset::{RomSet, RomSetError, from_zip, is_zip};
use machine_snk_neo_geo::{AUDIO_SAMPLE_RATE, BIOS_SIZE, Cartridge, NeoGeo};

use crate::input::apply_input_event;
use crate::profiles::{BIOS_FIRMWARE_ID, Model, ZOOM_FIRMWARE_ID, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;

/// The picture's size.
pub const FRAME_WIDTH: u32 = 320;
pub const FRAME_HEIGHT: u32 = 224;
/// The pixel clock: the master clock over 4.
const PIXEL_HZ: f64 = 6_000_000.0;

pub struct NeoGeoRuntime {
    model: Model,
    profile: MachineProfile,
    pub(crate) machine: Option<NeoGeo>,
    bios: Option<Vec<u8>>,
    zoom: Vec<u8>,
    cartridge: Option<Cartridge>,
    pub(crate) time: MachineTime,
    frames: u64,
    rgba_framebuffer: Vec<u8>,
}

impl NeoGeoRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            model,
            profile: profile_for(model),
            machine: None,
            bios: None,
            zoom: Vec::new(),
            cartridge: None,
            time: MachineTime::default(),
            frames: 0,
            rgba_framebuffer: vec![0; (FRAME_WIDTH * FRAME_HEIGHT * 4) as usize],
        }
    }

    /// A runtime with `bios` (the file as dumped, 16-bit words
    /// byte-swapped) and optionally the L0 ROM.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the BIOS is not 128 KB.
    pub fn new(model: Model, bios: Vec<u8>, zoom: Option<Vec<u8>>) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.zoom = zoom.unwrap_or_default();
        runtime.set_bios(bios)?;
        Ok(runtime)
    }

    /// Build from a profile firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or the BIOS is missing or the
    /// wrong size.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        firmware.validate_for_profile(&profile_for(model))?;
        let bios =
            firmware
                .bytes(BIOS_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: BIOS_FIRMWARE_ID.to_owned(),
                })?;
        let zoom = firmware.bytes(ZOOM_FIRMWARE_ID).map(<[u8]>::to_vec);
        Self::new(model, bios.to_vec(), zoom)
    }

    /// Replace the BIOS, given as dumped, and power on again.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the BIOS is not 128 KB.
    pub fn set_bios(&mut self, bios: Vec<u8>) -> Result<(), MachineError> {
        if bios.len() != BIOS_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: BIOS_FIRMWARE_ID.to_owned(),
                reason: format!("BIOS is {} bytes; expected {BIOS_SIZE}", bios.len()),
            });
        }
        // The dump holds each word low byte first.
        self.bios = Some(
            bios.chunks(2)
                .flat_map(|w| w.iter().rev().copied())
                .collect(),
        );
        self.rebuild_machine();
        Ok(())
    }

    /// Insert a cartridge from its ROM set and power on.
    pub fn insert_rom_set(&mut self, set: RomSet) {
        self.cartridge = Some(Cartridge {
            p: set.p,
            s: set.s,
            m: set.m,
            v_a: set.v_a,
            v_b: set.v_b,
            c: set.c,
        });
        self.rebuild_machine();
    }

    /// Insert a cartridge image: a zip of a ROM set, or a bare P-ROM in the
    /// 68000's byte order, as homebrew is often built.
    ///
    /// # Errors
    ///
    /// Returns an error if a zip holds no usable ROM set.
    pub fn insert_cartridge(&mut self, image: &[u8]) -> Result<(), RomSetError> {
        let set = if is_zip(image) {
            from_zip(image)?
        } else {
            RomSet {
                p: image.to_vec(),
                ..RomSet::default()
            }
        };
        self.insert_rom_set(set);
        Ok(())
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    #[must_use]
    pub fn machine(&self) -> Option<&NeoGeo> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut NeoGeo> {
        self.machine.as_mut()
    }

    /// Whether a cartridge is in, with or without a BIOS to run it.
    #[must_use]
    pub fn cartridge_loaded(&self) -> bool {
        self.cartridge.is_some()
    }

    /// Frames run since power-on.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The model id recorded in snapshots.
    #[must_use]
    pub fn model_id(&self) -> &'static str {
        self.model.model_id()
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    /// Install a machine restored from a snapshot, giving it back the
    /// media this runtime holds.
    ///
    /// # Errors
    ///
    /// Returns an error if the snapshot has a machine but this runtime has
    /// no BIOS or cartridge to give it.
    pub(crate) fn set_machine(&mut self, machine: Option<NeoGeo>) -> Result<(), MachineError> {
        let machine = match machine {
            Some(mut machine) => {
                let (Some(bios), Some(cartridge)) = (&self.bios, &self.cartridge) else {
                    return Err(MachineError::InvalidSnapshot {
                        reason: "the snapshot needs the BIOS and cartridge it was taken with"
                            .to_owned(),
                    });
                };
                machine.attach_media(cartridge.clone(), bios.clone(), self.zoom.clone());
                Some(machine)
            }
            None => None,
        };
        self.machine = machine;
        self.update_rgba_framebuffer();
        Ok(())
    }

    fn rebuild_machine(&mut self) {
        self.frames = 0;
        self.machine = match (&self.bios, &self.cartridge) {
            (Some(bios), Some(cartridge)) => Some(NeoGeo::new(
                cartridge.clone(),
                bios.clone(),
                self.zoom.clone(),
            )),
            _ => None,
        };
        self.update_rgba_framebuffer();
    }

    /// Convert the LSPC2's `0xAARRGGBB` picture to packed RGBA bytes.
    pub(crate) fn update_rgba_framebuffer(&mut self) {
        let Some(machine) = self.machine.as_ref() else {
            self.rgba_framebuffer.fill(0);
            return;
        };
        for (rgba, &pixel) in self
            .rgba_framebuffer
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(machine.framebuffer())
        {
            *rgba = [
                (pixel >> 16) as u8,
                (pixel >> 8) as u8,
                pixel as u8,
                (pixel >> 24) as u8,
            ];
        }
    }
}

impl emu198x_shell::FamilyRuntime for NeoGeoRuntime {
    type Model = Model;
    fn variant_ids() -> &'static [&'static str] {
        &Model::VARIANT_IDS
    }
    fn model_from_id(id: &str) -> Option<Model> {
        Model::from_variant_id(id)
    }
    fn variant_id(model: Model) -> &'static str {
        model.variant_id()
    }
    fn profile_for(model: Model) -> MachineProfile {
        profile_for(model)
    }
    fn rom_convention() -> emu198x_shell::RomConvention {
        emu198x_shell::RomConvention {
            env_var: Some("EMU198X_NEOGEO_ROM_DIR"),
            dirs: &["snk-neo-geo"],
        }
    }
    fn firmware_sources(model: Model) -> Vec<emu198x_shell::FirmwareSource> {
        model.firmware_sources()
    }
    fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        Self::from_firmware(model, firmware)
    }
    fn replacement(&self, model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let mut replacement = Self::from_firmware(model, firmware)?;
        replacement.cartridge.clone_from(&self.cartridge);
        replacement.rebuild_machine();
        Ok(replacement)
    }
    fn native_frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }
}

impl MachineCore for NeoGeoRuntime {
    fn set_machine<Q: emu198x_shell::SessionQueryProvider<Self>>(
        session: &mut emu198x_shell::HeadlessSession<Self, Q>,
        machine: &str,
    ) -> Result<emu198x_shell::VariantSwitched, emu198x_shell::LoaderError> {
        emu198x_shell::swap_variant(session, machine)
    }

    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    /// A soft reset is the reset button: both CPUs restart from the BIOS,
    /// RAM and video RAM keep what they had. Any other powers off and on.
    fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Soft
            && let Some(machine) = self.machine.as_mut()
        {
            machine.reset();
            return;
        }
        self.rebuild_machine();
        self.time = MachineTime::default();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            match (image.slot.as_ref(), image.kind) {
                ("cartridge-1", MediaKind::Cartridge) => {
                    self.insert_cartridge(image.bytes).map_err(|err| {
                        MachineError::InvalidMedia {
                            slot: "cartridge-1".to_owned(),
                            reason: err.to_string(),
                        }
                    })?;
                }
                (slot, MediaKind::Cartridge) => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
                }
                (_, kind) => return Err(MachineError::UnsupportedMediaKind { kind }),
            }
        }
        Ok(())
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        let Some(machine) = self.machine.as_mut() else {
            return Ok(RunResult::new(self.time, StopReason::WaitingForInput));
        };
        for event in host.input_events {
            apply_input_event(machine, event);
        }

        while self.time < target {
            let machine = self.machine.as_mut().expect("machine checked above");
            let ticks = machine.run_frame();
            let audio = machine.take_audio();
            self.time = self.time.saturating_add(ticks);
            self.frames += 1;
            self.update_rgba_framebuffer();

            host.frame_sink.push_frame(FramePacket {
                signal: None,
                timestamp: self.time,
                format: PixelFormat::Rgba8888,
                width: FRAME_WIDTH,
                height: FRAME_HEIGHT,
                palette: None,
                pixels: &self.rgba_framebuffer,
            })?;
            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: AUDIO_SAMPLE_RATE,
                channels: 2,
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

    /// The pixel clock is the master clock over 4, 6 MHz, on an NTSC
    /// television.
    fn display(&self) -> Option<Display> {
        Display::television_for_region(self.profile().region, PIXEL_HZ, PIXEL_HZ)
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();
}
