//! Runtime wrapper for the BBC Micro.

use common_acorn_cassette::TapePulse;
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, MediaTransportAction,
    PixelFormat, ResetKind, RunResult, StopReason,
};
use machine_acorn_bbc_micro::BbcMicro;

use crate::input::apply_input_event;
use crate::profiles::{MOS_FIRMWARE_ID, Model, profile_for};
use crate::snapshot;
use emu198x_shell::display::Display;

/// Framebuffer pixels per second.
const PIXEL_CLOCK_HZ: f64 = 16_000_000.0;

const MOS_SIZE: usize = 16 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;

pub struct BbcMicroRuntime {
    profile: MachineProfile,
    model: Model,
    machine: Option<BbcMicro>,
    mos_bytes: Option<Vec<u8>>,
    sideways_roms: Vec<(usize, Vec<u8>)>,
    teletext_font: Vec<u8>,
    /// The mounted cassette's decoded waveform, kept so it survives a reset's
    /// machine rebuild (the tape stays in the deck across a reset).
    tape_pulses: Option<Vec<TapePulse>>,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
}

impl BbcMicroRuntime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            mos_bytes: None,
            sideways_roms: Vec::new(),
            teletext_font: Vec::new(),
            tape_pulses: None,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
        }
    }

    /// Build from explicit 16 KB MOS ROM.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if the MOS is not 16 KB.
    pub fn new(model: Model, mos: Vec<u8>) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.set_mos(mos)?;
        Ok(runtime)
    }

    /// Build from a firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or MOS is missing.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let bytes =
            firmware
                .bytes(MOS_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: MOS_FIRMWARE_ID.to_owned(),
                })?;
        let mut runtime = Self::new(model, bytes.to_vec())?;
        if let Some(font) = firmware.bytes(crate::FONT_FIRMWARE_ID) {
            runtime.set_teletext_font(font.to_vec());
        }
        Ok(runtime)
    }

    /// Construct with BASIC as the default language in bank 15, when available.
    /// Callers install explicit sideways banks afterwards so they take precedence.
    ///
    /// # Errors
    ///
    /// As [`Self::from_firmware`].
    pub fn from_firmware_with_basic(
        model: Model,
        firmware: &FirmwareSet<'_>,
    ) -> Result<Self, MachineError> {
        let mut runtime = Self::from_firmware(model, firmware)?;
        if let Some(basic) = firmware.bytes(crate::BASIC_FIRMWARE_ID) {
            runtime.insert_sideways_rom(15, basic.to_vec());
        }
        Ok(runtime)
    }

    /// Replace the MOS and rebuild.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if size is wrong.
    pub fn set_mos(&mut self, mos: Vec<u8>) -> Result<(), MachineError> {
        if mos.len() != MOS_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: MOS_FIRMWARE_ID.to_owned(),
                reason: format!("MOS is {} bytes; expected {MOS_SIZE}", mos.len()),
            });
        }
        self.mos_bytes = Some(mos);
        self.rebuild_machine();
        Ok(())
    }

    /// Supply the SAA5050 teletext character ROM used to render MODE 7.
    pub fn set_teletext_font(&mut self, font: Vec<u8>) {
        self.teletext_font = font;
        self.rebuild_machine();
    }

    /// Install a sideways ROM into a slot (0..=15).
    pub fn insert_sideways_rom(&mut self, bank: usize, rom: Vec<u8>) {
        self.sideways_roms.retain(|(b, _)| *b != bank);
        self.sideways_roms.push((bank, rom));
        self.rebuild_machine();
    }

    #[must_use]
    pub fn machine(&self) -> Option<&BbcMicro> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut BbcMicro> {
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
    /// restore path so the resumed machine keeps its 6502/CRTC/VIA/PSG/RAM
    /// state. Mirrors `rebuild_machine`'s rgba-buffer sizing exactly so
    /// `update_rgba_framebuffer` cannot index out of bounds.
    pub(crate) fn set_machine(&mut self, machine: Option<BbcMicro>) {
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

    fn rebuild_machine(&mut self) {
        let Some(mos) = self.mos_bytes.clone() else {
            self.machine = None;
            return;
        };
        let mut machine = BbcMicro::new(mos);
        if !self.teletext_font.is_empty() {
            machine.set_teletext_font(self.teletext_font.clone());
        }
        for (bank, rom) in &self.sideways_roms {
            machine.insert_rom(*bank, rom.clone());
        }
        // Re-mount the cassette so a reset doesn't eject the tape.
        if let Some(pulses) = &self.tape_pulses {
            machine.insert_tape(pulses.clone());
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
}

impl emu198x_shell::FamilyRuntime for BbcMicroRuntime {
    type Model = Model;

    fn variant_ids() -> &'static [&'static str] {
        &Model::VARIANT_IDS
    }

    fn model_from_id(id: &str) -> Option<Model> {
        Model::from_variant_id(id)
    }

    fn variant_id(model: Model) -> &'static str {
        model.profile_id()
    }

    fn profile_for(model: Model) -> MachineProfile {
        profile_for(model)
    }

    fn rom_convention() -> emu198x_shell::RomConvention {
        emu198x_shell::RomConvention {
            env_var: Some("EMU198X_BBC_ROM_DIR"),
            dirs: &["acorn-bbc-micro"],
        }
    }

    fn firmware_sources(model: Model) -> Vec<emu198x_shell::FirmwareSource> {
        model.firmware_sources()
    }

    fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        Self::from_firmware(model, firmware)
    }

    fn native_frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }
}

impl MachineCore for BbcMicroRuntime {
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
            let slot = image.slot.as_ref();
            match image.kind {
                MediaKind::Tape if slot == "tape-1" => {
                    let tape = format_acorn_uef::parse(image.bytes).map_err(|reason| {
                        MachineError::InvalidMedia {
                            slot: slot.to_owned(),
                            reason: reason.to_string(),
                        }
                    })?;
                    if let Some(machine) = self.machine.as_mut() {
                        machine.insert_tape(tape.pulses.clone());
                    }
                    self.tape_pulses = Some(tape.pulses);
                }
                _ => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
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
    /// Start or stop the deck.
    ///
    /// The tape used to advance purely on the guest's motor line, so a script
    /// could not stop it, could not re-position it, and could not inspect a
    /// stalled load without the deck running on underneath (#1198). The host
    /// gate ANDs with the motor line, so a running deck still only moves when
    /// the machine asks.
    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        match command {
            ControlCommand::MediaTransport(cmd) => {
                if cmd.slot.as_ref() != "tape-1" {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: cmd.slot.as_ref().to_owned(),
                    });
                }
                let running = match cmd.action {
                    MediaTransportAction::Start => true,
                    MediaTransportAction::Stop => false,
                    _ => {
                        return Err(MachineError::UnsupportedOperation {
                            operation: "media-transport",
                        });
                    }
                };
                let Some(machine) = self.machine.as_mut() else {
                    return Err(MachineError::InvalidRequest {
                        reason: "no machine is running, so there is no deck to drive".to_owned(),
                    });
                };
                machine.set_deck_running(running);
                Ok(())
            }
            _ => Err(MachineError::UnsupportedOperation {
                operation: command.operation_name(),
            }),
        }
    }
    /// 16 MHz, which is the framebuffer's clock in every screen mode: the core
    /// renders each mode into one 640-wide buffer, so the mode changes how
    /// many source pixels there are and not how fast the buffer fills.
    fn display(&self) -> Option<Display> {
        Display::television_for_region(self.profile().region, PIXEL_CLOCK_HZ, PIXEL_CLOCK_HZ)
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
    emu198x_shell::debug_target_hooks!();

    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        self.machine
            .is_some()
            .then_some(&crate::input::BbcKeyboard as &dyn emu198x_shell::KeyboardTarget)
    }
}

emu198x_shell::impl_6502_debug_primitives!(BbcMicroRuntime);

#[cfg(test)]
mod catalogue_tests {
    use super::*;
    use emu198x_shell::{FamilyRuntime, FirmwareImage};

    #[test]
    fn firmware_constructor_installs_font_and_language_policy_survives_reset() {
        let mos = vec![0; 16384];
        let font = vec![0x3c; 960];
        let basic = vec![0x42; 16384];
        let mut firmware = FirmwareSet::new();
        for (id, bytes) in [
            (crate::MOS_FIRMWARE_ID, &mos),
            (crate::FONT_FIRMWARE_ID, &font),
            (crate::BASIC_FIRMWARE_ID, &basic),
        ] {
            firmware.push(FirmwareImage::new(id, bytes));
        }
        let mut bare =
            BbcMicroRuntime::from_firmware(Model::BbcModelB, &firmware).expect("MOS/font");
        assert_eq!(bare.teletext_font, font);
        bare.machine_mut().expect("machine").poke(0xfe30, 15);
        assert_eq!(bare.machine().expect("bare MOS").peek(0x8000), 0xff);
        let mut language = BbcMicroRuntime::from_firmware_with_basic(Model::BbcModelB, &firmware)
            .expect("language");
        language.reset(ResetKind::Hard);
        language.machine_mut().expect("machine").poke(0xfe30, 15);
        assert_eq!(language.machine().expect("language").peek(0x8000), 0x42);
        assert_eq!(language.teletext_font, font);
        assert_eq!(language.native_frame_ticks(), 39_936);
        assert!(
            !language
                .capabilities()
                .contains(&emu198x_shell::known_capability("variant-switch"))
        );
    }
}
