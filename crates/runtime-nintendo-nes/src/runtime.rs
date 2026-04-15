//! Runtime wrapper for the fresh-workspace NES baseline.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FramePacket, HostIo, InputEvent, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, QueryError, QueryResult,
    ResetKind, RunResult, SessionQueryProvider, StopReason,
};
use format_nintendo_nes_ines::parse_ines;
use machine_nintendo_nes::{FB_HEIGHT, FB_WIDTH, Nes};
use serde_json::json;

use crate::{Model, profile_for};

const NES_QUERY_PATHS: &[&str] = &[
    "nes.cartridge.loaded",
    "nes.cartridge.mapper",
    "nes.cpu.pc",
    "nes.machine.frame_count",
    "nes.machine.master_clock",
    "nes.ppu.dot",
    "nes.ppu.scanline",
];

/// NES-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NesSessionQueryProvider;

/// Firmwareless NES runtime over the concrete machine crate.
pub struct NesRuntime {
    profile: MachineProfile,
    machine: Option<Nes>,
    time: MachineTime,
    cartridge_bytes: Option<Vec<u8>>,
    cartridge_mapper: Option<u16>,
    rgba_framebuffer: Vec<u8>,
}

impl NesRuntime {
    /// Creates a blank runtime with no cartridge inserted.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            machine: None,
            time: MachineTime::default(),
            cartridge_bytes: None,
            cartridge_mapper: None,
            rgba_framebuffer: vec![0; (FB_WIDTH * FB_HEIGHT * 4) as usize],
        }
    }

    /// Returns the wrapped NES machine when a cartridge is loaded.
    #[must_use]
    pub fn machine(&self) -> Option<&Nes> {
        self.machine.as_ref()
    }

    fn load_cartridge_bytes(&mut self, slot: &str, bytes: &[u8]) -> Result<(), MachineError> {
        let parsed = parse_ines(bytes).map_err(|reason| MachineError::InvalidMedia {
            slot: slot.to_owned(),
            reason,
        })?;

        self.machine = Some(Nes::new(parsed.mapper));
        self.cartridge_bytes = Some(bytes.to_vec());
        self.cartridge_mapper = Some(parsed.header.mapper_number);
        self.time = MachineTime::default();
        self.rgba_framebuffer.fill(0);
        Ok(())
    }

    fn rebuild_loaded_machine(&mut self) {
        let Some(bytes) = self.cartridge_bytes.as_deref() else {
            self.machine = None;
            self.cartridge_mapper = None;
            self.rgba_framebuffer.fill(0);
            return;
        };

        match parse_ines(bytes) {
            Ok(parsed) => {
                self.machine = Some(Nes::new(parsed.mapper));
                self.cartridge_mapper = Some(parsed.header.mapper_number);
                self.rgba_framebuffer.fill(0);
            }
            Err(_) => {
                self.machine = None;
                self.cartridge_mapper = None;
                self.rgba_framebuffer.fill(0);
            }
        }
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

    fn apply_input_event(&mut self, event: &InputEvent) {
        let Some(machine) = self.machine.as_mut() else {
            return;
        };

        match event {
            InputEvent::Button {
                port,
                name,
                pressed,
            } if *port == 1 => {
                if let Some(bit) = button_bit(name.as_ref()) {
                    let mask = 1u8 << bit;
                    let mut state = machine.controller1_state;
                    if *pressed {
                        state |= mask;
                    } else {
                        state &= !mask;
                    }
                    machine.set_controller1(state);
                }
            }
            InputEvent::Key { name, pressed } => {
                if let Some(bit) = button_bit(name.as_ref()) {
                    let mask = 1u8 << bit;
                    let mut state = machine.controller1_state;
                    if *pressed {
                        state |= mask;
                    } else {
                        state &= !mask;
                    }
                    machine.set_controller1(state);
                }
            }
            _ => {}
        }
    }
}

impl SessionQueryProvider<NesRuntime> for NesSessionQueryProvider {
    fn query_paths(&self, _machine: &NesRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = NES_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &NesRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "nes.cartridge.loaded" => json!(machine.machine.is_some()),
            "nes.cartridge.mapper" => json!(machine.cartridge_mapper),
            "nes.machine.frame_count" => {
                json!(machine.machine.as_ref().map_or(0, Nes::frame_count))
            }
            "nes.machine.master_clock" => {
                json!(machine.machine.as_ref().map_or(0, Nes::master_clock))
            }
            "nes.cpu.pc" => json!(
                machine
                    .machine
                    .as_ref()
                    .ok_or_else(|| QueryError::UnavailablePath {
                        path: path.to_owned(),
                        reason: "no cartridge is loaded",
                    })?
                    .cpu
                    .regs
                    .pc
            ),
            "nes.ppu.scanline" => json!(
                machine
                    .machine
                    .as_ref()
                    .ok_or_else(|| QueryError::UnavailablePath {
                        path: path.to_owned(),
                        reason: "no cartridge is loaded",
                    })?
                    .ppu
                    .scanline()
            ),
            "nes.ppu.dot" => json!(
                machine
                    .machine
                    .as_ref()
                    .ok_or_else(|| QueryError::UnavailablePath {
                        path: path.to_owned(),
                        reason: "no cartridge is loaded",
                    })?
                    .ppu
                    .dot()
            ),
            _ => return Ok(None),
        };

        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

impl MachineCore for NesRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_loaded_machine();
        self.time = MachineTime::default();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            if image.slot.as_ref() != "cartridge-1" {
                return Err(MachineError::UnknownMediaSlot {
                    slot: image.slot.as_ref().to_owned(),
                });
            }

            if image.kind != MediaKind::Cartridge {
                return Err(MachineError::UnsupportedMediaKind { kind: image.kind });
            }

            self.load_cartridge_bytes(image.slot.as_ref(), image.bytes)?;
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
            self.apply_input_event(event);
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
                format: emu198x_shell::PixelFormat::Rgba8888,
                width: FB_WIDTH,
                height: FB_HEIGHT,
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
                sample_rate: 48_000,
                channels: 1,
                samples: &audio,
            })?;
        }

        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: "snapshot-export",
        })
    }

    fn restore(&mut self, _bytes: &[u8]) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: "snapshot-import",
        })
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

fn button_bit(name: &str) -> Option<u8> {
    Some(match name.to_ascii_lowercase().as_str() {
        "a" => 0,
        "b" => 1,
        "select" => 2,
        "start" => 3,
        "up" => 4,
        "down" => 5,
        "left" => 6,
        "right" => 7,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::{MediaImage, NullAudioSink, NullFrameSink, NullTraceSink};
    use std::path::Path;

    fn minimal_ines() -> Vec<u8> {
        let mut prg = vec![0xea; 16 * 1024];
        prg[0x3ffc] = 0x00;
        prg[0x3ffd] = 0x80;
        let chr = vec![0u8; 8 * 1024];
        let mut data = vec![0u8; 16 + prg.len() + chr.len()];
        data[0..4].copy_from_slice(b"NES\x1a");
        data[4] = 1;
        data[5] = 1;
        data[16..16 + prg.len()].copy_from_slice(&prg);
        data[16 + prg.len()..].copy_from_slice(&chr);
        data
    }

    #[test]
    fn runtime_loads_cartridge_and_runs_one_frame() {
        const FRAME_TICKS: u64 = 341 * 262;
        let rom = minimal_ines();
        let mut runtime = NesRuntime::blank(Model::NesNtsc);
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
        runtime.load_media(&media).expect("valid iNES should load");

        let mut frame_sink = NullFrameSink;
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };

        let result = runtime
            .run_until(MachineTime::new(FRAME_TICKS), &mut host)
            .expect("one frame should run");

        assert_eq!(result.stop_reason, StopReason::ReachedTarget);
        assert!(runtime.time() >= MachineTime::new(FRAME_TICKS));
        assert_eq!(
            runtime.machine().expect("cartridge loaded").frame_count(),
            1
        );
    }

    #[test]
    fn button_input_updates_controller_state() {
        const FRAME_TICKS: u64 = 341 * 262;
        let rom = minimal_ines();
        let mut runtime = NesRuntime::blank(Model::NesNtsc);
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
        runtime.load_media(&media).expect("valid iNES should load");

        let events = [InputEvent::Button {
            port: 1,
            name: "start".into(),
            pressed: true,
        }];
        let mut frame_sink = NullFrameSink;
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &events,
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };

        runtime
            .run_until(MachineTime::new(FRAME_TICKS), &mut host)
            .expect("one frame should run");

        assert_eq!(
            runtime
                .machine()
                .expect("cartridge loaded")
                .controller1_state
                & (1 << 3),
            1 << 3
        );
    }

    #[test]
    fn query_provider_reports_loaded_cartridge_state() {
        let rom = minimal_ines();
        let mut runtime = NesRuntime::blank(Model::NesNtsc);
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
        runtime.load_media(&media).expect("valid iNES should load");

        let provider = NesSessionQueryProvider;
        let loaded = provider
            .query(&runtime, "nes.cartridge.loaded")
            .expect("query should succeed")
            .expect("provider should own the path");

        assert_eq!(loaded.value, json!(true));
    }

    #[test]
    #[ignore = "uses local NES reference ROM"]
    fn real_ines_super_mario_bros_runs_and_draws() {
        const FRAME_TICKS: u64 = 341 * 262;
        let path = Path::new(
            "/Users/stevehill/Projects/Emu198x-Unclean/Reference/nintendo/nes/Super Mario Bros. (1985-09-13)(Nintendo)(JP-US).nes",
        );
        if !path.is_file() {
            eprintln!("SKIPPING: local Super Mario Bros. ROM not found");
            return;
        }

        let rom = std::fs::read(path).expect("reference ROM should read");
        let mut runtime = NesRuntime::blank(Model::NesNtsc);
        let mut media = MediaSet::new();
        media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
        runtime
            .load_media(&media)
            .expect("reference iNES should load");

        let mut frame_sink = NullFrameSink;
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };

        runtime
            .run_until(MachineTime::new(FRAME_TICKS * 240), &mut host)
            .expect("reference ROM should run");

        let machine = runtime.machine().expect("cartridge should remain loaded");
        assert!(machine.frame_count() > 0);
        assert!(machine.framebuffer().iter().any(|&pixel| pixel != 0));
    }
}
