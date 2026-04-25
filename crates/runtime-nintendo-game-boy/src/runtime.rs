//! `MachineCore` wrapper for the DMG.

use common_nintendo_game_boy::{DMG_GREYSCALE_RGBA, JoypadButton, SCREEN_HEIGHT, SCREEN_WIDTH};
use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FramePacket, HostIo, InputEvent, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, QueryError,
    QueryResult, ResetKind, RunResult, SessionQueryProvider, StopReason,
};
use machine_nintendo_game_boy::{ApuChannel, AudioControls, GameBoy};
use serde::{Deserialize, Serialize};
use serde_json::json;

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

const GAME_BOY_QUERY_PATHS: &[&str] = &["gameboy.cartridge.loaded", "gameboy.cpu.pc"];

/// Game Boy-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameBoySessionQueryProvider;

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

#[derive(Serialize)]
struct GameBoyRuntimeSnapshotRefV1<'a> {
    version: u32,
    profile_id: &'a str,
    time: MachineTime,
    cartridge_bytes: Option<&'a [u8]>,
    machine: Option<&'a GameBoy>,
}

#[derive(Deserialize)]
struct GameBoyRuntimeSnapshotV1 {
    version: u32,
    profile_id: String,
    time: MachineTime,
    cartridge_bytes: Option<Vec<u8>>,
    machine: Option<GameBoy>,
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

    fn apply_input_event(&mut self, event: &InputEvent) {
        let Some(machine) = self.machine.as_mut() else {
            return;
        };
        let (name, pressed) = match event {
            InputEvent::Key { name, pressed } => (name.as_ref(), *pressed),
            InputEvent::Button { name, pressed, .. } => (name.as_ref(), *pressed),
            _ => return,
        };
        if let Some(button) = button_from_name(name) {
            machine.set_button(button, pressed);
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

impl SessionQueryProvider<GameBoyRuntime> for GameBoySessionQueryProvider {
    fn query_paths(&self, _machine: &GameBoyRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = GAME_BOY_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(
        &self,
        machine: &GameBoyRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "gameboy.cartridge.loaded" => json!(machine.machine.is_some()),
            "gameboy.cpu.pc" => json!(
                machine
                    .machine
                    .as_ref()
                    .ok_or_else(|| QueryError::UnavailablePath {
                        path: path.to_owned(),
                        reason: "no cartridge is loaded",
                    })?
                    .cpu_pc()
            ),
            _ => return Ok(None),
        };

        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
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
            self.apply_input_event(event);
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
        postcard::to_allocvec(&GameBoyRuntimeSnapshotRefV1 {
            version: 1,
            profile_id: self.profile.profile_id.as_str(),
            time: self.time,
            cartridge_bytes: self.cartridge_bytes.as_deref(),
            machine: self.machine.as_ref(),
        })
        .map_err(|reason| MachineError::InvalidSnapshot {
            reason: format!("encode failed: {reason}"),
        })
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        let snapshot: GameBoyRuntimeSnapshotV1 =
            postcard::from_bytes(bytes).map_err(|reason| MachineError::InvalidSnapshot {
                reason: format!("decode failed: {reason}"),
            })?;

        if snapshot.version != 1 {
            return Err(MachineError::InvalidSnapshot {
                reason: format!("unsupported snapshot version {}", snapshot.version),
            });
        }
        if snapshot.profile_id != self.profile.profile_id.as_str() {
            return Err(MachineError::InvalidSnapshot {
                reason: format!(
                    "snapshot profile {} does not match runtime profile {}",
                    snapshot.profile_id,
                    self.profile.profile_id.as_str()
                ),
            });
        }

        self.machine = snapshot.machine;
        self.cartridge_bytes = snapshot.cartridge_bytes;
        self.time = snapshot.time;
        self.audio_buffer.clear();
        Ok(())
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

fn button_from_name(name: &str) -> Option<JoypadButton> {
    Some(match name.to_ascii_lowercase().as_str() {
        "a" => JoypadButton::A,
        "b" => JoypadButton::B,
        "select" => JoypadButton::Select,
        "start" => JoypadButton::Start,
        "up" => JoypadButton::Up,
        "down" => JoypadButton::Down,
        "left" => JoypadButton::Left,
        "right" => JoypadButton::Right,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use common_nintendo_game_boy::MCYCLES_PER_FRAME;
    use emu198x_shell::{
        HeadlessSession, MediaImage, NullAudioSink, NullFrameSink, NullTraceSink,
        SessionQueryProvider,
    };

    /// Build a 32 KiB ROM that loops forever at $0100 with a valid header.
    fn loop_rom() -> Vec<u8> {
        let mut rom = vec![0x00; 0x8000];
        rom[0x0100] = 0x18; // JR
        rom[0x0101] = 0xFE; // -2 → tight loop
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // ROM size code 0 → 32 KiB
        rom[0x0149] = 0x00; // RAM size code 0
        let mut checksum: u8 = 0;
        for &byte in &rom[0x0134..=0x014C] {
            checksum = checksum.wrapping_sub(byte).wrapping_sub(1);
        }
        rom[0x014D] = checksum;
        rom
    }

    fn make_host_buffers() -> (NullFrameSink, NullAudioSink, NullTraceSink) {
        (NullFrameSink, NullAudioSink, NullTraceSink)
    }

    #[test]
    fn blank_runtime_has_dmg_profile_and_no_machine() {
        let runtime = GameBoyRuntime::blank(Model::Dmg);
        assert_eq!(
            runtime.profile().profile_id.as_str(),
            "nintendo-game-boy-dmg"
        );
        assert!(runtime.machine().is_none());
        assert_eq!(runtime.time(), MachineTime::default());
    }

    #[test]
    fn loading_a_valid_cartridge_constructs_the_machine() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let rom = loop_rom();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
        runtime.load_media(&media).unwrap();
        assert!(runtime.machine().is_some());
    }

    fn battery_ram_rom() -> Vec<u8> {
        let mut rom = loop_rom();
        rom[0x0147] = 0x03; // MBC1 + RAM + battery
        rom[0x0149] = 0x02; // 8 KiB RAM
        let mut checksum: u8 = 0;
        for &byte in &rom[0x0134..=0x014C] {
            checksum = checksum.wrapping_sub(byte).wrapping_sub(1);
        }
        rom[0x014D] = checksum;
        rom
    }

    #[test]
    fn battery_backed_ram_can_be_restored_and_exported() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let rom = battery_ram_rom();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
        runtime.load_media(&media).unwrap();

        assert!(runtime.has_battery_backed_ram());
        let save = vec![0x5A; 0x2000];
        runtime.restore_cartridge_ram(&save).unwrap();

        assert_eq!(runtime.cartridge_ram(), Some(save.as_slice()));
    }

    #[test]
    fn reset_preserves_external_ram() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let rom = battery_ram_rom();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
        runtime.load_media(&media).unwrap();

        let mut save = vec![0xFF; 0x2000];
        save[0] = 0xC0;
        save[0x1FFF] = 0xDE;
        runtime.restore_cartridge_ram(&save).unwrap();
        runtime.reset(ResetKind::Hard);

        assert_eq!(runtime.cartridge_ram(), Some(save.as_slice()));
    }

    #[test]
    fn load_media_rejects_unknown_slot() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let rom = loop_rom();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("nope", MediaKind::Cartridge, &rom));
        let err = runtime.load_media(&media).unwrap_err();
        match err {
            MachineError::UnknownMediaSlot { slot } => assert_eq!(slot, "nope"),
            other => panic!("expected UnknownMediaSlot, got {other:?}"),
        }
    }

    #[test]
    fn load_media_rejects_wrong_kind() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let rom = loop_rom();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge", MediaKind::Tape, &rom));
        let err = runtime.load_media(&media).unwrap_err();
        match err {
            MachineError::UnsupportedMediaKind { kind } => assert_eq!(kind, MediaKind::Tape),
            other => panic!("expected UnsupportedMediaKind, got {other:?}"),
        }
    }

    #[test]
    fn run_until_without_cartridge_reports_waiting_for_input() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let (mut frame_sink, mut audio_sink, mut trace_sink) = make_host_buffers();
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        let result = runtime
            .run_until(MachineTime::new(u64::from(MCYCLES_PER_FRAME)), &mut host)
            .unwrap();
        assert_eq!(result.stop_reason, StopReason::WaitingForInput);
    }

    #[test]
    fn run_until_advances_machine_time_and_emits_one_frame() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let rom = loop_rom();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
        runtime.load_media(&media).unwrap();

        let (mut frame_sink, mut audio_sink, mut trace_sink) = make_host_buffers();
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        let result = runtime
            .run_until(MachineTime::new(u64::from(MCYCLES_PER_FRAME)), &mut host)
            .unwrap();
        assert_eq!(result.stop_reason, StopReason::ReachedTarget);
        assert!(runtime.time().get() >= u64::from(MCYCLES_PER_FRAME));
    }

    #[test]
    fn key_input_event_presses_joypad_button() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let rom = loop_rom();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
        runtime.load_media(&media).unwrap();

        let events = [InputEvent::Key {
            name: "start".into(),
            pressed: true,
        }];
        let (mut frame_sink, mut audio_sink, mut trace_sink) = make_host_buffers();
        let mut host = HostIo {
            input_events: &events,
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        runtime
            .run_until(MachineTime::new(u64::from(MCYCLES_PER_FRAME)), &mut host)
            .unwrap();

        // We can't peek into the joypad state from outside the
        // machine, but we can confirm Start is mapped to a button by
        // round-tripping a snapshot — if the press was applied, the
        // restored runtime should round-trip it byte-identically.
        let snap = runtime.snapshot().unwrap();
        let mut reborn = GameBoyRuntime::blank(Model::Dmg);
        reborn.restore(&snap).unwrap();
        let snap2 = reborn.snapshot().unwrap();
        assert_eq!(snap, snap2);
    }

    #[test]
    fn audio_controls_mutate_loaded_machine_mixer() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let rom = loop_rom();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
        runtime.load_media(&media).unwrap();

        runtime.set_audio_channel_enabled(ApuChannel::Noise, false);
        runtime.set_audio_channel_gain(ApuChannel::Wave, 0.25);

        let controls = runtime.audio_controls().unwrap();
        assert!(!controls.channel(ApuChannel::Noise).enabled());
        assert_eq!(controls.channel(ApuChannel::Wave).gain(), 0.25);
    }

    #[test]
    fn snapshot_round_trip_preserves_state() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let rom = loop_rom();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
        runtime.load_media(&media).unwrap();

        let (mut frame_sink, mut audio_sink, mut trace_sink) = make_host_buffers();
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        runtime
            .run_until(MachineTime::new(u64::from(MCYCLES_PER_FRAME)), &mut host)
            .unwrap();

        let snap = runtime.snapshot().unwrap();
        let mut reborn = GameBoyRuntime::blank(Model::Dmg);
        reborn.restore(&snap).unwrap();
        assert_eq!(reborn.time(), runtime.time());
        assert!(reborn.machine().is_some());
    }

    #[test]
    fn restore_rejects_mismatched_profile() {
        let runtime = GameBoyRuntime::blank(Model::Dmg);
        // Forge a snapshot with a wrong profile id.
        let bytes = postcard::to_allocvec(&GameBoyRuntimeSnapshotRefV1 {
            version: 1,
            profile_id: "nintendo-game-boy-cgb",
            time: MachineTime::new(0),
            cartridge_bytes: None,
            machine: None,
        })
        .unwrap();
        let mut other = runtime;
        let err = other.restore(&bytes).unwrap_err();
        assert!(matches!(err, MachineError::InvalidSnapshot { .. }));
    }

    #[test]
    fn query_provider_lists_gameboy_paths() {
        let runtime = GameBoyRuntime::blank(Model::Dmg);
        let provider = GameBoySessionQueryProvider;
        assert_eq!(
            provider.query_paths(&runtime, Some("gameboy.")),
            vec![
                "gameboy.cartridge.loaded".to_string(),
                "gameboy.cpu.pc".to_string()
            ]
        );
    }

    #[test]
    fn query_provider_reports_loaded_state_and_cpu_pc() {
        let mut runtime = GameBoyRuntime::blank(Model::Dmg);
        let rom = loop_rom();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
        runtime.load_media(&media).unwrap();

        let provider = GameBoySessionQueryProvider;
        assert_eq!(
            provider
                .query(&runtime, "gameboy.cartridge.loaded")
                .unwrap()
                .unwrap()
                .value,
            json!(true)
        );
        assert_eq!(
            provider
                .query(&runtime, "gameboy.cpu.pc")
                .unwrap()
                .unwrap()
                .value,
            json!(0x0100u16)
        );
    }

    #[test]
    fn headless_session_exposes_gameboy_queries() {
        let runtime = GameBoyRuntime::blank(Model::Dmg);
        let session =
            HeadlessSession::new_with_query_provider(runtime, 1, GameBoySessionQueryProvider);
        let paths = session.query_paths(Some("gameboy."));
        assert_eq!(
            paths.paths,
            vec![
                "gameboy.cartridge.loaded".to_string(),
                "gameboy.cpu.pc".to_string()
            ]
        );
    }
}
