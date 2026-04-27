//! Runtime wrapper for the Dragon 32.

use emu198x_shell::{
    CapabilitySet, FirmwareSet, FramePacket, HostIo, InputEvent, MachineCore, MachineError,
    MachineProfile, MachineTime, MediaSet, PixelFormat, QueryError, QueryResult, ResetKind,
    RunResult, SessionQueryProvider, StopReason,
};
use machine_dragon_32::{Dragon32, DragonKey, MatrixKey, ROM_SIZE};
use motorola_vdg_6847::{
    TEXT_VISIBLE_FRAMEBUFFER_HEIGHT, TEXT_VISIBLE_FRAMEBUFFER_WIDTH, TextPalette,
};
use serde_json::json;

use crate::{Model, profile_for};

const DRAGON_QUERY_PATHS: &[&str] = &[
    "boot.detected",
    "boot.reason",
    "dragon.cpu.cycles",
    "dragon.cpu.instructions",
    "dragon.cpu.pc",
    "dragon.machine.halted",
    "dragon.text.base",
];

/// Dragon-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DragonSessionQueryProvider;

/// Dragon 32 runtime.
pub struct DragonRuntime {
    profile: MachineProfile,
    firmware_rom: [u8; ROM_SIZE],
    machine: Dragon32,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
}

impl DragonRuntime {
    /// Build a Dragon runtime from profile-declared firmware.
    ///
    /// # Errors
    ///
    /// Returns an error if required firmware is missing or the Dragon BASIC ROM
    /// is not exactly 16 KiB.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        let rom_id = "dragon32-basic-rom";
        firmware.validate_for_profile(&profile)?;
        let rom = firmware
            .bytes(rom_id)
            .ok_or_else(|| MachineError::MissingFirmware {
                id: rom_id.to_owned(),
            })?;
        Self::new(model, rom).map_err(|reason| MachineError::InvalidFirmware {
            id: rom_id.to_owned(),
            reason,
        })
    }

    /// Build a Dragon runtime from raw ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the supplied ROM is not exactly 16 KiB.
    pub fn new(model: Model, rom: &[u8]) -> Result<Self, String> {
        let firmware_rom: [u8; ROM_SIZE] = rom
            .try_into()
            .map_err(|_| format!("Dragon 32 BASIC ROM must be exactly {ROM_SIZE} bytes"))?;
        Ok(Self {
            profile: profile_for(model),
            firmware_rom,
            machine: Dragon32::new(&firmware_rom),
            time: MachineTime::default(),
            rgba_framebuffer: Vec::with_capacity(
                TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT * 4,
            ),
        })
    }

    /// Build a runtime backed by a zero-filled ROM image.
    #[must_use]
    pub fn blank(model: Model) -> Self {
        let firmware_rom = [0; ROM_SIZE];
        Self {
            profile: profile_for(model),
            firmware_rom,
            machine: Dragon32::new(&firmware_rom),
            time: MachineTime::default(),
            rgba_framebuffer: Vec::with_capacity(
                TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT * 4,
            ),
        }
    }

    /// Returns the current machine.
    #[must_use]
    pub fn machine(&self) -> &Dragon32 {
        &self.machine
    }

    fn rebuild_machine(&mut self) {
        self.machine = Dragon32::new(&self.firmware_rom);
        self.time = MachineTime::default();
        self.rgba_framebuffer.clear();
    }

    fn apply_input_event(&mut self, event: &InputEvent) -> Result<(), MachineError> {
        let (name, pressed) = match event {
            InputEvent::Key { name, pressed } => (name.as_ref(), *pressed),
            InputEvent::Button { name, pressed, .. } => (name.as_ref(), *pressed),
            _ => return Ok(()),
        };
        let Some(key) = DragonKey::from_label(name) else {
            return Ok(());
        };
        let key = MatrixKey::from_dragon_key(key);
        let result = if pressed {
            self.machine.keyboard_mut().press(key)
        } else {
            self.machine.keyboard_mut().release(key)
        };
        result.map_err(|reason| MachineError::InvalidRequest {
            reason: reason.to_string(),
        })
    }

    fn update_framebuffer(&mut self) {
        let argb = self
            .machine
            .render_visible_text_argb(TextPalette::default());
        self.rgba_framebuffer.clear();
        self.rgba_framebuffer.reserve(argb.len() * 4);
        for pixel in argb {
            self.rgba_framebuffer.push(((pixel >> 16) & 0xFF) as u8);
            self.rgba_framebuffer.push(((pixel >> 8) & 0xFF) as u8);
            self.rgba_framebuffer.push((pixel & 0xFF) as u8);
            self.rgba_framebuffer.push(((pixel >> 24) & 0xFF) as u8);
        }
    }
}

impl MachineCore for DragonRuntime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_machine();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        if media.is_empty() {
            return Ok(());
        }
        Err(MachineError::UnsupportedMediaKind {
            kind: media.images[0].kind,
        })
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        for event in host.input_events {
            self.apply_input_event(event)?;
        }

        if target <= self.time {
            return Ok(RunResult::new(self.time, StopReason::ReachedTarget));
        }

        let cycles_to_run = target.0.saturating_sub(self.time.0);
        let report = self.machine.run_cycles(cycles_to_run, 0);
        self.time = self.time.saturating_add(report.cycles);
        self.update_framebuffer();
        host.frame_sink.push_frame(FramePacket {
            timestamp: self.time,
            format: PixelFormat::Rgba8888,
            width: TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
            height: TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32,
            palette: None,
            pixels: &self.rgba_framebuffer,
        })?;

        let stop_reason = if report.stop_reason == machine_dragon_32::StopReason::CpuHalted {
            StopReason::Halted
        } else {
            StopReason::ReachedTarget
        };
        Ok(RunResult::new(self.time, stop_reason))
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: "snapshot",
        })
    }

    fn restore(&mut self, _bytes: &[u8]) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: "restore",
        })
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }
}

impl SessionQueryProvider<DragonRuntime> for DragonSessionQueryProvider {
    fn query_paths(&self, _machine: &DragonRuntime, prefix: Option<&str>) -> Vec<String> {
        DRAGON_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect()
    }

    fn query(
        &self,
        machine: &DragonRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "boot.detected" => json!(machine.boot_status().detected),
            "boot.reason" => json!(machine.boot_status().reason),
            "dragon.cpu.cycles" => json!(machine.machine.cycles()),
            "dragon.cpu.instructions" => json!(machine.machine.instructions()),
            "dragon.cpu.pc" => json!(machine.machine.pc()),
            "dragon.machine.halted" => json!(machine.machine.is_halted()),
            "dragon.text.base" => json!(machine.machine.text_screen_base()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

struct BootStatus {
    detected: bool,
    reason: &'static str,
}

impl DragonRuntime {
    fn boot_status(&self) -> BootStatus {
        let text = self.machine.capture_text_screen().to_plain_text();
        if text.lines().any(|line| line.trim() == "OK") {
            BootStatus {
                detected: true,
                reason: "basic-ok-prompt",
            }
        } else {
            BootStatus {
                detected: false,
                reason: "waiting-for-basic-ok-prompt",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use emu198x_shell::{
        FirmwareImage, FirmwareSet, FramePacket, FrameSink, HostIo, MachineCore, MachineTime,
        NullAudioSink, NullTraceSink, PixelFormat,
    };

    use super::*;

    #[derive(Default)]
    struct CaptureFrameSink {
        frames: usize,
        last_size: Option<(u32, u32)>,
        last_format: Option<PixelFormat>,
    }

    impl FrameSink for CaptureFrameSink {
        fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
            self.frames += 1;
            self.last_size = Some((frame.width, frame.height));
            self.last_format = Some(frame.format);
            Ok(())
        }
    }

    fn rom_with_reset_vector(pc: u16) -> [u8; ROM_SIZE] {
        let mut rom = [0; ROM_SIZE];
        let [hi, lo] = pc.to_be_bytes();
        rom[0x3FFE] = hi;
        rom[0x3FFF] = lo;
        rom
    }

    #[test]
    fn runtime_builds_from_declared_firmware() {
        let rom = rom_with_reset_vector(0x8000);
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new("dragon32-basic-rom", &rom));

        let runtime = DragonRuntime::from_firmware(Model::Dragon32Pal, &firmware)
            .expect("declared firmware should build runtime");

        assert_eq!(runtime.profile().profile_id.as_str(), "dragon-32-pal");
    }

    #[test]
    fn runtime_emits_text_framebuffer() {
        let mut runtime = DragonRuntime::blank(Model::Dragon32Pal);
        let mut frame_sink = CaptureFrameSink::default();
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };

        let result = runtime
            .run_until(MachineTime(64), &mut host)
            .expect("runtime should run");

        assert_eq!(result.reached, MachineTime(64));
        assert_eq!(frame_sink.frames, 1);
        assert_eq!(
            frame_sink.last_size,
            Some((
                TEXT_VISIBLE_FRAMEBUFFER_WIDTH as u32,
                TEXT_VISIBLE_FRAMEBUFFER_HEIGHT as u32
            ))
        );
        assert_eq!(frame_sink.last_format, Some(PixelFormat::Rgba8888));
    }

    #[test]
    fn query_provider_reports_machine_state() {
        let runtime = DragonRuntime::blank(Model::Dragon32Pal);
        let provider = DragonSessionQueryProvider;

        let query = provider
            .query(&runtime, "dragon.text.base")
            .expect("query should not fail")
            .expect("query should be owned");

        assert_eq!(query.value, json!(0));
    }

    #[test]
    fn boot_query_reports_pending_without_basic_prompt() {
        let runtime = DragonRuntime::blank(Model::Dragon32Pal);
        let provider = DragonSessionQueryProvider;

        let detected = provider
            .query(&runtime, "boot.detected")
            .expect("query should not fail")
            .expect("query should be owned");
        let reason = provider
            .query(&runtime, "boot.reason")
            .expect("query should not fail")
            .expect("query should be owned");

        assert_eq!(detected.value, json!(false));
        assert_eq!(reason.value, json!("waiting-for-basic-ok-prompt"));
    }
}
