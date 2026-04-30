//! `MachineCore` wrapper for the DMG.

use common_nintendo_game_boy::{DMG_GREYSCALE_RGBA, SCREEN_HEIGHT, SCREEN_WIDTH};
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FramePacket, HostIo, MachineCore, MachineError,
    MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind, RunResult,
    StopReason,
};
use machine_nintendo_game_boy::{ApuChannel, AudioControls, GameBoy};

use crate::input::apply_input_event;
use crate::snapshot;
use crate::{Model, profile_for};

/// Audio sample rate produced by the APU. Matches
/// `nintendo_game_boy_apu::SAMPLE_RATE_HZ`.
const APU_SAMPLE_RATE_HZ: u32 = 48_000;

/// Stereo audio: left + right interleaved.
const APU_CHANNELS: u8 = 2;

/// Generous per-drain bound. One frame is ~1 600 stereo floats at
/// 48 kHz × 60 fps; 4 096 leaves room for a frame plus catch-up
/// without ever resizing inside the drain loop.
const AUDIO_DRAIN_CHUNK: usize = 4_096;

/// Family runtime for the Game Boy. Holds the loaded machine plus
/// host-boundary scratch space (audio drain + cartridge bytes for
/// reset rebuilds).
pub struct GameBoyRuntime {
    model: Model,
    profile: MachineProfile,
    machine: Option<GameBoy>,
    cartridge_bytes: Option<Vec<u8>>,
    time: MachineTime,
    audio_buffer: Vec<f32>,
}

impl GameBoyRuntime {
    /// Creates a blank runtime with no cartridge inserted.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            model,
            profile: profile_for(model),
            machine: None,
            cartridge_bytes: None,
            time: MachineTime::default(),
            audio_buffer: Vec::with_capacity(AUDIO_DRAIN_CHUNK),
        }
    }

    /// Returns the loaded machine, when a cartridge is inserted.
    #[must_use]
    pub fn machine(&self) -> Option<&GameBoy> {
        self.machine.as_ref()
    }

    /// Mutable access to the loaded machine, when a cartridge is inserted.
    pub fn machine_mut(&mut self) -> Option<&mut GameBoy> {
        self.machine.as_mut()
    }

    /// Returns host-side APU mixer controls for the loaded machine.
    #[must_use]
    pub fn audio_controls(&self) -> Option<AudioControls> {
        self.machine.as_ref().map(GameBoy::audio_controls)
    }

    /// Replaces host-side APU mixer controls for the loaded machine.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        if let Some(machine) = self.machine.as_mut() {
            machine.set_audio_controls(controls);
        }
    }

    /// Enables or mutes one APU channel in host output.
    pub fn set_audio_channel_enabled(&mut self, channel: ApuChannel, enabled: bool) {
        if let Some(machine) = self.machine.as_mut() {
            machine.set_audio_channel_enabled(channel, enabled);
        }
    }

    /// Sets one APU channel's host-side gain.
    pub fn set_audio_channel_gain(&mut self, channel: ApuChannel, gain: f32) {
        if let Some(machine) = self.machine.as_mut() {
            machine.set_audio_channel_gain(channel, gain);
        }
    }

    /// Returns whether the loaded cartridge has battery-backed external RAM.
    #[must_use]
    pub fn has_battery_backed_ram(&self) -> bool {
        self.machine
            .as_ref()
            .is_some_and(|machine| machine.cartridge().has_battery_backed_ram())
    }

    /// Returns the loaded cartridge's external RAM, if present.
    #[must_use]
    pub fn cartridge_ram(&self) -> Option<&[u8]> {
        self.machine
            .as_ref()
            .map(|machine| machine.cartridge().ram())
            .filter(|ram| !ram.is_empty())
    }

    /// Replaces the loaded cartridge's external RAM from a sidecar save image.
    ///
    /// # Errors
    ///
    /// Returns [`MachineError::InvalidMedia`] when no cartridge RAM is loaded or
    /// when the supplied save length does not match the cartridge RAM size.
    pub fn restore_cartridge_ram(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        let Some(machine) = self.machine.as_mut() else {
            return Err(MachineError::InvalidMedia {
                slot: "cartridge".to_owned(),
                reason: "no cartridge is loaded".to_owned(),
            });
        };
        let ram = machine.cartridge_mut().ram_mut();
        if ram.is_empty() {
            return Err(MachineError::InvalidMedia {
                slot: "cartridge".to_owned(),
                reason: "loaded cartridge has no external RAM".to_owned(),
            });
        }
        if bytes.len() != ram.len() {
            return Err(MachineError::InvalidMedia {
                slot: "cartridge".to_owned(),
                reason: format!(
                    "save RAM length {} does not match cartridge RAM length {}",
                    bytes.len(),
                    ram.len()
                ),
            });
        }

        ram.copy_from_slice(bytes);
        Ok(())
    }

    /// Read-only access to the runtime profile descriptor. Used by
    /// the snapshot module for envelope validation.
    #[must_use]
    pub(crate) fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    /// Current machine time, exposed by name distinct from the
    /// `MachineCore::time` trait method so internal modules don't
    /// have to import the trait.
    #[must_use]
    pub(crate) const fn time_value(&self) -> MachineTime {
        self.time
    }

    /// Cartridge bytes used to rebuild the machine on reset. Used by
    /// the snapshot module for envelope encoding.
    #[must_use]
    pub(crate) fn cartridge_bytes(&self) -> Option<&[u8]> {
        self.cartridge_bytes.as_deref()
    }

    pub(crate) fn set_machine(&mut self, machine: Option<GameBoy>) {
        self.machine = machine;
    }

    pub(crate) fn set_cartridge_bytes(&mut self, bytes: Option<Vec<u8>>) {
        self.cartridge_bytes = bytes;
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn clear_audio_buffer(&mut self) {
        self.audio_buffer.clear();
    }

    fn rebuild_machine(&mut self) {
        let preserved_ram = self.cartridge_ram().map(|ram| ram.to_vec());
        let Some(bytes) = self.cartridge_bytes.clone() else {
            self.machine = None;
            return;
        };
        match GameBoy::from_rom_with_boot_profile(bytes, self.model.boot_profile()) {
            Ok((_, mut gb)) => {
                if let Some(preserved_ram) = preserved_ram {
                    let ram = gb.cartridge_mut().ram_mut();
                    if ram.len() == preserved_ram.len() {
                        ram.copy_from_slice(&preserved_ram);
                    }
                }
                self.machine = Some(gb);
            }
            Err(_) => {
                self.machine = None;
                self.cartridge_bytes = None;
            }
        }
    }

    fn drain_audio(&mut self) {
        self.audio_buffer.clear();
        let Some(machine) = self.machine.as_mut() else {
            return;
        };
        loop {
            let start = self.audio_buffer.len();
            self.audio_buffer.resize(start + AUDIO_DRAIN_CHUNK, 0.0);
            let written = machine.drain_audio(&mut self.audio_buffer[start..]);
            self.audio_buffer.truncate(start + written);
            if written < AUDIO_DRAIN_CHUNK {
                break;
            }
        }
    }
}

impl MachineCore for GameBoyRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_machine();
        self.time = MachineTime::default();
        self.audio_buffer.clear();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            if image.slot.as_ref() != "cartridge" {
                return Err(MachineError::UnknownMediaSlot {
                    slot: image.slot.as_ref().to_owned(),
                });
            }
            if image.kind != MediaKind::Cartridge {
                return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
            }

            let bytes = image.bytes.to_vec();
            let (_, gb) =
                GameBoy::from_rom_with_boot_profile(bytes.clone(), self.model.boot_profile())
                    .map_err(|reason| MachineError::InvalidMedia {
                        slot: image.slot.as_ref().to_owned(),
                        reason: reason.to_string(),
                    })?;
            self.machine = Some(gb);
            self.cartridge_bytes = Some(bytes);
            self.time = MachineTime::default();
            self.audio_buffer.clear();
        }
        Ok(())
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        for event in host.input_events {
            apply_input_event(self.machine.as_mut(), event);
        }

        if self.machine.is_none() {
            return Ok(RunResult::new(self.time, StopReason::WaitingForInput));
        }

        while self.time < target {
            let m_cycles = self
                .machine
                .as_mut()
                .expect("machine checked above")
                .run_frame();
            self.time = self.time.saturating_add(u64::from(m_cycles));

            let frame = self
                .machine
                .as_ref()
                .expect("machine still loaded")
                .framebuffer();
            host.frame_sink.push_frame(FramePacket {
                timestamp: self.time,
                format: PixelFormat::Indexed8,
                width: SCREEN_WIDTH,
                height: SCREEN_HEIGHT,
                palette: Some(&DMG_GREYSCALE_RGBA),
                pixels: frame,
            })?;

            self.drain_audio();
            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: APU_SAMPLE_RATE_HZ,
                channels: APU_CHANNELS,
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

#[cfg(test)]
mod tests {
    use super::{GameBoyRuntime, ResetKind};
    use crate::Model;
    use emu198x_shell::MachineCore;

    /// `rebuild_machine` returns through its `Err` arm when the
    /// cartridge bytes recorded by a previous load no longer parse
    /// as a Game Boy ROM. Drive that arm by surgically poking
    /// invalid bytes through the `pub(crate)` setter and then
    /// calling `reset` — the runtime should clear both the machine
    /// and the cartridge bytes rather than panic.
    #[test]
    fn reset_clears_state_when_cartridge_bytes_no_longer_parse() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        runtime.set_cartridge_bytes(Some(vec![0xDE, 0xAD, 0xBE, 0xEF]));
        runtime.reset(ResetKind::Hard);
        assert!(runtime.machine().is_none());
        assert!(runtime.cartridge_bytes().is_none());
    }
}
